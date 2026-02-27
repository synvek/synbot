//! Feishu channel — WebSocket long-connection based.
//!
//! Uses the `open-lark` SDK's WebSocket client to maintain a persistent
//! connection with Feishu, receiving messages via event subscription and
//! sending replies through the IM v1 message API.
//!
//! Integrates `RetryPolicy` / `RetryState` for resilient WebSocket
//! reconnection with exponential backoff on transient errors and immediate
//! abort + system notification on unrecoverable errors (e.g. invalid
//! credentials).
//!
//! Note: `EventDispatcherHandler` from open-lark is `!Send`, so the
//! WebSocket event loop runs on a dedicated single-threaded tokio runtime.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use open_lark::client::ws_client::LarkWsClient;
use open_lark::prelude::*;
use crate::rig_provider::SynbotCompletionModel;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tracing::{error, info, warn};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::file_handler;
use crate::channels::approval_classifier;
use crate::channels::{approval_formatter, Channel, RetryPolicy, RetryState};
use crate::config::{AllowlistEntry, FeishuConfig};
use crate::tools::approval::{ApprovalManager, ApprovalResponse};

/// Optional sender to notify the user when file upload fails (e.g. missing permission).
type OutboundTx = Option<tokio::sync::broadcast::Sender<OutboundMessage>>;

pub struct FeishuChannel {
    config: FeishuConfig,
    show_tool_calls: bool,
    tool_result_preview_chars: usize,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    /// When set, used to send a user-visible message when file upload fails (e.g. permission 99991672).
    outbound_tx: OutboundTx,
    running: bool,
    approval_manager: Option<Arc<ApprovalManager>>,
    approval_classifier: Option<Arc<dyn SynbotCompletionModel>>,
    pending_approvals: Arc<RwLock<HashMap<String, (String, String)>>>,
    /// Workspace directory for saving incoming files and resolving outbound file paths.
    workspace_dir: Option<PathBuf>,
}

/// Internal error type to distinguish transient from unrecoverable WS errors.
#[derive(Debug)]
enum FeishuWsError {
    /// Transient error — should be retried with backoff.
    Transient(String),
    /// Unrecoverable error — should stop retrying and notify the Agent.
    Unrecoverable(String),
}

impl std::fmt::Display for FeishuWsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transient(msg) => write!(f, "transient: {msg}"),
            Self::Unrecoverable(msg) => write!(f, "unrecoverable: {msg}"),
        }
    }
}

/// Download file/image from a message using "get message resource" API.
/// Use when im.v1.file.get / image.get fail (e.g. user-sent files with file_v3 key).
/// GET /open-apis/im/v1/messages/{message_id}/resources/{file_key}?type=file|image
async fn feishu_fetch_message_resource(
    app_id: &str,
    app_secret: &str,
    message_id: &str,
    file_key: &str,
    resource_type: &str,
) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e: reqwest::Error| e.to_string())?;
    let token_url = "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal";
    let token_body = serde_json::json!({
        "app_id": app_id,
        "app_secret": app_secret,
    });
    let token_resp = client
        .post(token_url)
        .json(&token_body)
        .send()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    let token_json: serde_json::Value = token_resp
        .json()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    let token = token_json
        .get("tenant_access_token")
        .and_then(|v: &serde_json::Value| v.as_str())
        .ok_or_else(|| "missing tenant_access_token in auth response".to_string())?;
    let resource_url = format!(
        "https://open.feishu.cn/open-apis/im/v1/messages/{}/resources/{}?type={}",
        message_id, file_key, resource_type
    );
    let resp = client
        .get(&resource_url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, body));
    }
    let bytes = resp.bytes().await.map_err(|e: reqwest::Error| e.to_string())?;
    Ok(bytes.to_vec())
}

/// 上传图片到飞书 IM（走 /open-apis/im/v1/images，非 files）。
/// 表单：image_type=message，image=<二进制>。返回 image_key。
async fn feishu_upload_image(
    app_id: &str,
    app_secret: &str,
    file_name: &str,
    file_data: Vec<u8>,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e: reqwest::Error| e.to_string())?;
    let token_url = "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal";
    let token_body = serde_json::json!({
        "app_id": app_id,
        "app_secret": app_secret,
    });
    let token_resp = client
        .post(token_url)
        .json(&token_body)
        .send()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    let token_json: serde_json::Value = token_resp
        .json()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    let token = token_json
        .get("tenant_access_token")
        .and_then(|v: &serde_json::Value| v.as_str())
        .ok_or_else(|| "missing tenant_access_token in auth response".to_string())?;

    let part = reqwest::multipart::Part::bytes(file_data).file_name(file_name.to_string());
    let form = reqwest::multipart::Form::new()
        .text("image_type", "message")
        .part("image", part);

    let resp = client
        .post("https://open.feishu.cn/open-apis/im/v1/images")
        .header("Authorization", format!("Bearer {}", token))
        .multipart(form)
        .send()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, body));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    let image_key = json
        .get("data")
        .and_then(|d| d.get("image_key"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing data.image_key in upload response".to_string())?;
    Ok(image_key.to_string())
}

/// 根据文件扩展名映射为飞书 IM 上传接口要求的 file_type 类型。
/// 飞书文档：file_type 取值为 "image" | "audio" | "video" | "file"，不是扩展名；见 https://open.feishu.cn/document/server-docs/im-v1/file/create
fn feishu_file_type_from_extension(ext: &str) -> &'static str {
    let ext = ext.to_lowercase();
    let ext = ext.trim();
    match ext {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" => "image",
        "mp3" | "wav" | "amr" | "aac" | "ogg" => "audio",
        "mp4" | "mov" | "avi" | "mkv" | "flv" => "video",
        _ => "stream", // pdf, doc, docx, txt, zip, rar 等
    }
}

/// Upload file to Feishu IM via POST multipart/form-data.
/// 飞书要求：multipart 表单必须包含 file_type（类型：image/audio/video/file）、file_name（表单字段）和 file（二进制）。见官方文档 create 接口。
/// Returns file_key on success.
async fn feishu_upload_file(
    app_id: &str,
    app_secret: &str,
    file_type: &str,
    file_name: &str,
    file_data: Vec<u8>,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e: reqwest::Error| e.to_string())?;
    let token_url = "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal";
    let token_body = serde_json::json!({
        "app_id": app_id,
        "app_secret": app_secret,
    });
    let token_resp = client
        .post(token_url)
        .json(&token_body)
        .send()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    let token_json: serde_json::Value = token_resp
        .json()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    let token = token_json
        .get("tenant_access_token")
        .and_then(|v: &serde_json::Value| v.as_str())
        .ok_or_else(|| "missing tenant_access_token in auth response".to_string())?;

    let part = reqwest::multipart::Part::bytes(file_data).file_name(file_name.to_string());
    let form = reqwest::multipart::Form::new()
        .text("file_type", file_type.to_string())
        .text("file_name", file_name.to_string())
        .part("file", part);

    let resp = client
        .post("https://open.feishu.cn/open-apis/im/v1/files")
        .header("Authorization", format!("Bearer {}", token))
        .multipart(form)
        .send()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, body));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    let file_key = json
        .get("data")
        .and_then(|d| d.get("file_key"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing data.file_key in upload response".to_string())?;
    Ok(file_key.to_string())
}

/// Classify a Feishu WebSocket / API error string as transient or unrecoverable.
///
/// Errors containing authentication-related keywords (401, 403, "invalid",
/// "unauthorized", "forbidden", "credential") are treated as unrecoverable.
/// Keyword fallback when LLM classifier is not used or returns unknown. Some(true)=approve, Some(false)=reject, None=ambiguous.
fn parse_approval_response_keywords(text: &str) -> Option<bool> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let t_lower = t.to_lowercase();
    // Reject first so "disagree" / reject keywords are not mistaken for approve
    let reject_exact = ["no", "n", "reject", "拒绝", "否", "deny", "不同意"];
    if reject_exact.iter().any(|s| t_lower == *s || t_lower.starts_with(&format!("{} ", s)) || t_lower.ends_with(&format!(" {}", s))) {
        return Some(false);
    }
    if t.contains("不同意") || t.contains("拒绝") {
        return Some(false);
    }
    // Approve: exact / prefix / suffix
    let approve_exact = ["yes", "y", "approve", "批准", "是", "ok", "同意", "好", "1"];
    if approve_exact.iter().any(|s| t_lower == *s || t_lower.starts_with(&format!("{} ", s)) || t_lower.ends_with(&format!(" {}", s))) {
        return Some(true);
    }
    // Approve: contain approve keywords (e.g. agree, approve, ok) without negation
    if (t.contains("同意") || t.contains("批准") || t.contains("好")) && !t.contains("不") {
        return Some(true);
    }
    None
}

fn classify_feishu_error(error_msg: &str) -> FeishuWsError {
    let lower = error_msg.to_lowercase();
    if lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("invalid app")
        || lower.contains("invalid credential")
        || lower.contains("app_id")
        || lower.contains("app_secret")
    {
        FeishuWsError::Unrecoverable(error_msg.to_string())
    } else {
        FeishuWsError::Transient(error_msg.to_string())
    }
}

impl FeishuChannel {
    pub fn new(
        config: FeishuConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: broadcast::Receiver<OutboundMessage>,
        show_tool_calls: bool,
        tool_result_preview_chars: usize,
        workspace_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            config,
            show_tool_calls,
            tool_result_preview_chars,
            inbound_tx,
            outbound_rx: Some(outbound_rx),
            outbound_tx: None,
            running: false,
            approval_manager: None,
            approval_classifier: None,
            pending_approvals: Arc::new(RwLock::new(HashMap::new())),
            workspace_dir,
        }
    }

    /// Set the outbound sender so the channel can notify the user when file upload fails (e.g. missing im:resource permission).
    pub fn with_outbound_tx(mut self, tx: tokio::sync::broadcast::Sender<OutboundMessage>) -> Self {
        self.outbound_tx = Some(tx);
        self
    }

    /// Set the approval manager.
    pub fn with_approval_manager(mut self, manager: Arc<ApprovalManager>) -> Self {
        self.approval_manager = Some(manager);
        self
    }

    /// Set the approval reply classifier (LLM). When set, uses the model to classify approve/reject; otherwise uses keyword matching.
    pub fn with_approval_classifier(mut self, model: Arc<dyn SynbotCompletionModel>) -> Self {
        self.approval_classifier = Some(model);
        self
    }

    /// Register a user's pending approval request.
    async fn register_pending_approval(&self, user_id: String, request_id: String, chat_id: String) {
        let mut pending = self.pending_approvals.write().await;
        pending.insert(user_id, (request_id, chat_id));
    }

    /// Get and remove the user's pending approval request.
    async fn take_pending_approval(&self, user_id: &str) -> Option<(String, String)> {
        let mut pending = self.pending_approvals.write().await;
        pending.remove(user_id)
    }

    /// Check whether the user has a pending approval request.
    #[allow(dead_code)]
    async fn has_pending_approval(&self, user_id: &str) -> bool {
        let pending = self.pending_approvals.read().await;
        pending.contains_key(user_id)
    }

    fn format_approval_request(request: &crate::tools::approval::ApprovalRequest) -> String {
        approval_formatter::format_approval_request(request)
    }

    /// Build a LarkClient for API calls (bot info, send messages).
    fn build_lark_client(&self) -> LarkClient {
        let builder = LarkClient::builder(&self.config.app_id, &self.config.app_secret)
            .with_app_type(AppType::SelfBuild)
            .with_enable_token_cache(true);

        let builder = if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
            builder.with_http_client(crate::appcontainer_dns::build_reqwest_client())
        } else {
            builder
        };

        builder.build()
    }

    /// Send a text message to a chat via the IM v1 API.
    async fn send_text(client: &LarkClient, chat_id: &str, text: &str) -> Result<()> {
        // Feishu text message limit is ~150KB; split at a safe boundary.
        const CHUNK_SIZE: usize = 30_000;
        let chunks: Vec<&str> = if text.len() <= CHUNK_SIZE {
            vec![text]
        } else {
            text.as_bytes()
                .chunks(CHUNK_SIZE)
                .map(|c| std::str::from_utf8(c).unwrap_or(""))
                .collect()
        };

        for chunk in chunks {
            let content = serde_json::json!({ "text": chunk }).to_string();
            let body = CreateMessageRequestBody::builder()
                .receive_id(chat_id)
                .msg_type("text")
                .content(content)
                .build();
            let req = CreateMessageRequest::builder()
                .receive_id_type("chat_id")
                .request_body(body)
                .build();

            if let Err(e) = client.im.v1.message.create(req, None).await {
                error!("Feishu send_text error: {e:#}");
                return Err(e.into());
            }
        }
        Ok(())
    }

    /// Static version of send_text for use in spawn_local contexts
    async fn send_text_static(client: &LarkClient, chat_id: &str, text: &str) -> Result<()> {
        Self::send_text(client, chat_id, text).await
    }

    /// Send a system notification to the Agent via the MessageBus.
    async fn notify_system_error(&self, error_msg: &str) {
        let notification = InboundMessage {
            channel: "system".into(),
            sender_id: "feishu".into(),
            chat_id: "system".into(),
            content: format!("[Feishu] Unrecoverable error: {error_msg}"),
            timestamp: chrono::Utc::now(),
            media: vec![],
            metadata: serde_json::json!({
                "error_kind": "unrecoverable",
                "source_channel": "feishu",
            }),
        };
        if let Err(e) = self.inbound_tx.send(notification).await {
            error!("Failed to send system notification for Feishu error: {e}");
        }
    }

    /// Attempt a single WebSocket connection cycle.
    ///
    /// This spawns a dedicated OS thread (because `EventDispatcherHandler`
    /// is `!Send`) and blocks until the connection closes or errors out.
    /// Returns `Ok(())` if the connection closed normally, or a classified
    /// error for the retry loop to handle.
    async fn attempt_ws_connection(
        inbound_tx: mpsc::Sender<InboundMessage>,
        allowlist: Vec<AllowlistEntry>,
        channel_name: String,
        enable_allowlist: bool,
        group_my_name: Option<String>,
        app_id: String,
        app_secret: String,
        approval_manager: Option<Arc<ApprovalManager>>,
        approval_classifier: Option<Arc<dyn SynbotCompletionModel>>,
        pending_approvals: Arc<RwLock<HashMap<String, (String, String)>>>,
        workspace_dir: Option<PathBuf>,
    ) -> std::result::Result<(), FeishuWsError> {
        let (result_tx, result_rx) = oneshot::channel::<Result<(), String>>();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to build Feishu WS runtime");

            let local = tokio::task::LocalSet::new();
            
            // Clone app_id and app_secret before moving into the closure
            let app_id_for_config = app_id.clone();
            let app_secret_for_config = app_secret.clone();
            
            local.block_on(&rt, async move {
                let handler = EventDispatcherHandler::builder()
                    .register_p2_im_message_receive_v1(move |event| {
                        info!("Feishu event callback fired!");
                        let sender_open_id =
                            event.event.sender.sender_id.open_id.clone();
                        info!(
                            sender = %sender_open_id,
                            "Feishu received message from sender"
                        );

                        let msg = &event.event.message;
                        info!(
                            message_type = %msg.message_type,
                            chat_id = %msg.chat_id,
                            content = %msg.content,
                            "Feishu message detail"
                        );

                        // File / image / media: download to workspace and forward as inbound with media.
                        // Feishu may send files as "file", "image", or "media"; content is JSON with file_key and/or image_key.
                        let is_file_like = msg.message_type == "file"
                            || msg.message_type == "image"
                            || msg.message_type == "media";
                        if is_file_like {
                            if let Some(ref ws) = workspace_dir {
                                if let Ok(content_json) =
                                    serde_json::from_str::<serde_json::Value>(&msg.content)
                                {
                                    let file_key =
                                        content_json.get("file_key").and_then(|v| v.as_str());
                                    let image_key =
                                        content_json.get("image_key").and_then(|v| v.as_str());
                                    let file_name = content_json
                                        .get("file_name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or(if msg.message_type == "image" {
                                            "image.png"
                                        } else {
                                            "file"
                                        });
                                    let (key, use_image_api) = if let Some(fk) = file_key {
                                        (fk.to_string(), false)
                                    } else if let Some(ik) = image_key {
                                        (ik.to_string(), true)
                                    } else {
                                        (String::new(), false)
                                    };
                                    if !key.is_empty() {
                                        info!(
                                            message_type = %msg.message_type,
                                            key = %key,
                                            file_name = %file_name,
                                            "Feishu file/image message: downloading to workspace"
                                        );
                                        let app_id_c = app_id.clone();
                                        let app_secret_c = app_secret.clone();
                                        let ws_c = ws.clone();
                                        let inbound_tx_c = inbound_tx.clone();
                                        let channel_name_c = channel_name.clone();
                                        let sender_open_id_c = sender_open_id.clone();
                                        let chat_id_c = msg.chat_id.clone();
                                        let message_id_c = msg.message_id.clone();
                                        let chat_type_c = msg.chat_type.clone();
                                        let message_type_c = msg.message_type.clone();
                                        let key = key.clone();
                                        let file_name = file_name.to_string();
                                        std::thread::spawn(move || {
                                            let rt =
                                                tokio::runtime::Runtime::new().unwrap();
                                            let _ = rt.block_on(async move {
                                                let client = LarkClient::builder(
                                                    &app_id_c, &app_secret_c,
                                                )
                                                .with_app_type(AppType::SelfBuild)
                                                .with_enable_token_cache(true)
                                                .build();
                                                let download_result = if use_image_api {
                                                    client.im.v1.image.get(&key, None).await
                                                        .map(|r| r.data)
                                                } else {
                                                    client.im.v1.file.get(&key, None).await
                                                        .map(|r| r.data)
                                                };
                                                let data_result = match download_result {
                                                    Ok(data) => Ok(data),
                                                    Err(e) => {
                                                        warn!("Feishu file/image get failed, trying message resource API: {e:#}");
                                                        let resource_type =
                                                            if use_image_api { "image" } else { "file" };
                                                        feishu_fetch_message_resource(
                                                            &app_id_c,
                                                            &app_secret_c,
                                                            &message_id_c,
                                                            &key,
                                                            resource_type,
                                                        )
                                                        .await
                                                        .map_err(anyhow::Error::msg)
                                                    }
                                                };
                                                match data_result {
                                                    Ok(data) => {
                                                        if let Ok(path) =
                                                            file_handler::save_incoming_file(
                                                                &ws_c,
                                                                &file_name,
                                                                &data,
                                                            )
                                                        {
                                                            let media_path =
                                                                path.to_string_lossy().into_owned();
                                                            info!(
                                                                path = %media_path,
                                                                "Feishu file saved to workspace"
                                                            );
                                                            let _ = inbound_tx_c
                                                                .send(InboundMessage {
                                                                    channel: channel_name_c,
                                                                    sender_id: sender_open_id_c,
                                                                    chat_id: chat_id_c,
                                                                    content: format!(
                                                                        "[文件] {}",
                                                                        file_name
                                                                    ),
                                                                    timestamp: chrono::Utc::now(),
                                                                    media: vec![media_path],
                                                                    metadata: serde_json::json!({
                                                                        "message_id": message_id_c,
                                                                        "message_type": message_type_c,
                                                                        "chat_type": chat_type_c,
                                                                    }),
                                                                })
                                                                .await;
                                                        } else {
                                                            warn!("Feishu: save_incoming_file failed for {}", file_name);
                                                            let _ = inbound_tx_c.send(InboundMessage {
                                                                channel: channel_name_c,
                                                                sender_id: sender_open_id_c,
                                                                chat_id: chat_id_c,
                                                                content: format!("[文件] {} 保存到工作区失败", file_name),
                                                                timestamp: chrono::Utc::now(),
                                                                media: vec![],
                                                                metadata: serde_json::json!({
                                                                    "message_id": message_id_c,
                                                                    "message_type": message_type_c,
                                                                    "chat_type": chat_type_c,
                                                                }),
                                                            }).await;
                                                        }
                                                    }
                                                    Err(e) => {
                                                        warn!("Feishu file/image download failed: {e:#}");
                                                        let _ = inbound_tx_c.send(InboundMessage {
                                                            channel: channel_name_c,
                                                            sender_id: sender_open_id_c,
                                                            chat_id: chat_id_c,
                                                            content: format!(
                                                                "[文件] {} 下载失败（{}），请尝试重新发送或使用其他格式",
                                                                file_name,
                                                                e
                                                            ),
                                                            timestamp: chrono::Utc::now(),
                                                            media: vec![],
                                                            metadata: serde_json::json!({
                                                                "message_id": message_id_c,
                                                                "message_type": message_type_c,
                                                                "chat_type": chat_type_c,
                                                                "download_error": e.to_string(),
                                                            }),
                                                        }).await;
                                                    }
                                                }
                                            });
                                        });
                                        return;
                                    }
                                }
                            }
                            warn!(
                                "Feishu {} message skipped (no workspace or missing file_key/image_key)",
                                msg.message_type
                            );
                            return;
                        }

                        // Extract text; for non-text messages forward raw content
                        let text = if msg.message_type == "text" {
                            serde_json::from_str::<serde_json::Value>(&msg.content)
                                .ok()
                                .and_then(|v| {
                                    v.get("text")
                                        .and_then(|t| t.as_str().map(String::from))
                                })
                                .unwrap_or_default()
                        } else {
                            msg.content.clone()
                        };

                        if text.is_empty() {
                            warn!("Feishu message text is empty, skipping");
                            return;
                        }

                        let chat_id = msg.chat_id.clone();
                        let is_group = msg.chat_type != "p2p";
                        let (trigger_agent, content, is_group_meta) = if !enable_allowlist {
                            if is_group {
                                if let Some(ref my_name) = group_my_name {
                                    let trimmed = text.trim_start();
                                    let mention = format!("@{}", my_name);
                                    let starts = trimmed.starts_with(&mention)
                                        || trimmed
                                            .strip_prefix('@')
                                            .map(|s| s.trim_start().starts_with(my_name))
                                            .unwrap_or(false);
                                    if !starts {
                                        info!(
                                            chat_id = %chat_id,
                                            "Feishu: group message not @bot, saving to session only"
                                        );
                                        let inbound_tx_clone = inbound_tx.clone();
                                        let sender_id = sender_open_id.clone();
                                        let channel_name_clone = channel_name.clone();
                                        let message_id = msg.message_id.clone();
                                        let message_type = msg.message_type.clone();
                                        let chat_type = msg.chat_type.clone();
                                        let _ = inbound_tx_clone.try_send(InboundMessage {
                                            channel: channel_name_clone,
                                            sender_id,
                                            chat_id: chat_id.clone(),
                                            content: text.clone(),
                                            timestamp: chrono::Utc::now(),
                                            media: vec![],
                                            metadata: serde_json::json!({
                                                "message_id": message_id,
                                                "message_type": message_type,
                                                "chat_type": chat_type,
                                                "trigger_agent": false,
                                                "group": true,
                                            }),
                                        });
                                        return;
                                    }
                                    // Strip only bot mention then 0+ spaces; do not strip @@role so agent loop can route @@dev etc.
                                    let stripped = trimmed
                                        .strip_prefix(&mention)
                                        .map(str::trim_start)
                                        .unwrap_or_else(|| trimmed.strip_prefix('@').map(str::trim_start).unwrap_or(trimmed));
                                    (true, stripped.to_string(), true)
                                } else {
                                    (true, text.clone(), true)
                                }
                            } else {
                                (true, text.clone(), false)
                            }
                        } else {
                            let entry = allowlist.iter().find(|e| e.chat_id == chat_id);
                            match entry {
                                None => {
                                    warn!(chat_id = %chat_id, "Feishu: chat not in allowlist, saving to session only");
                                    let inbound_tx_clone = inbound_tx.clone();
                                    let sender_id = sender_open_id.clone();
                                    let text_clone = text.clone();
                                    let message_id = msg.message_id.clone();
                                    let message_type = msg.message_type.clone();
                                    let chat_type = msg.chat_type.clone();
                                    let channel_name_clone = channel_name.clone();
                                    let app_id_clone = app_id.clone();
                                    let app_secret_clone = app_secret.clone();
                                    tokio::task::spawn_local(async move {
                                        let client = {
                                            let builder = LarkClient::builder(&app_id_clone, &app_secret_clone)
                                                .with_app_type(AppType::SelfBuild)
                                                .with_enable_token_cache(true);
                                            let builder = if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
                                                builder.with_http_client(crate::appcontainer_dns::build_reqwest_client())
                                            } else {
                                                builder
                                            };
                                            builder.build()
                                        };
                                        let _ = Self::send_text_static(&client, &chat_id, "未配置聊天许可，请配置。").await;
                                        let inbound = InboundMessage {
                                            channel: channel_name_clone,
                                            sender_id,
                                            chat_id,
                                            content: text_clone,
                                            timestamp: chrono::Utc::now(),
                                            media: vec![],
                                            metadata: serde_json::json!({
                                                "message_id": message_id,
                                                "message_type": message_type,
                                                "chat_type": chat_type,
                                                "trigger_agent": false,
                                            }),
                                        };
                                        if let Err(e) = inbound_tx_clone.try_send(inbound) {
                                            error!("Feishu failed to forward allowlist-denied message: {e}");
                                        }
                                    });
                                    return;
                                }
                                Some(e) => {
                                    if let Some(ref my_name) = e.my_name {
                                        let trimmed = text.trim_start();
                                        let mention = format!("@{}", my_name);
                                        let starts = trimmed.starts_with(&mention)
                                            || trimmed
                                                .strip_prefix('@')
                                                .map(|s| s.trim_start().starts_with(my_name))
                                                .unwrap_or(false);
                                        if !starts {
                                            info!(
                                                chat_id = %chat_id,
                                                "Feishu: group message not @bot, saving to session only"
                                            );
                                            let inbound_tx_clone = inbound_tx.clone();
                                            let sender_id = sender_open_id.clone();
                                            let channel_name_clone = channel_name.clone();
                                            let message_id = msg.message_id.clone();
                                            let message_type = msg.message_type.clone();
                                            let chat_type = msg.chat_type.clone();
                                            let _ = inbound_tx_clone.try_send(InboundMessage {
                                                channel: channel_name_clone,
                                                sender_id,
                                                chat_id: chat_id.clone(),
                                                content: text.clone(),
                                                timestamp: chrono::Utc::now(),
                                                media: vec![],
                                                metadata: serde_json::json!({
                                                    "message_id": message_id,
                                                    "message_type": message_type,
                                                    "chat_type": chat_type,
                                                    "trigger_agent": false,
                                                    "group": true,
                                                }),
                                            });
                                            return;
                                        }
                                        // Strip only bot mention then 0+ spaces; do not strip @@role so agent loop can route @@dev etc.
                                        let stripped = trimmed
                                            .strip_prefix(&mention)
                                            .map(str::trim_start)
                                            .unwrap_or_else(|| trimmed.strip_prefix('@').map(str::trim_start).unwrap_or(trimmed));
                                        (true, stripped.to_string(), true)
                                    } else {
                                        (true, text.clone(), false)
                                    }
                                }
                            }
                        };

                        // Async work in spawn (closure is sync): if user has pending approval, classify (LLM or keywords) and submit so the waiting exec unblocks; else forward to bus.
                        let inbound_tx_fwd = inbound_tx.clone();
                        let pending_approvals_fwd = pending_approvals.clone();
                        let approval_manager_fwd = approval_manager.clone();
                        let approval_classifier_fwd = approval_classifier.clone();
                        let sender_open_id_fwd = sender_open_id.clone();
                        let channel_name_fwd = channel_name.clone();
                        let content_fwd = content.clone();
                        let chat_id_fwd = msg.chat_id.clone();
                        let is_group_meta_fwd = is_group_meta;
                        let message_id_fwd = msg.message_id.clone();
                        let message_type_fwd = msg.message_type.clone();
                        let chat_type_fwd = msg.chat_type.clone();
                        tokio::task::spawn_local(async move {
                            let mut pending = pending_approvals_fwd.write().await;
                            // Look up by chat_id: we register by chat_id (session_id's last segment for dm)
                            let removed = pending.remove(&chat_id_fwd);
                            drop(pending);
                            if let Some((request_id, _)) = removed {
                                let mut approved_opt: Option<bool> = None;
                                if let Some(ref model) = approval_classifier_fwd {
                                    approved_opt = approval_classifier::classify_approval_response(
                                        model.as_ref(),
                                        &content_fwd,
                                    )
                                    .await;
                                }
                                if approved_opt.is_none() {
                                    approved_opt = parse_approval_response_keywords(&content_fwd);
                                }
                                if let Some(ref mgr) = approval_manager_fwd {
                                    if let Some(approved) = approved_opt {
                                        let response = ApprovalResponse {
                                            request_id: request_id.clone(),
                                            approved,
                                            responder: sender_open_id_fwd.clone(),
                                            timestamp: chrono::Utc::now(),
                                        };
                                        if let Err(e) = mgr.submit_response(response).await {
                                            error!("Feishu failed to submit approval response: {e:#}");
                                        } else {
                                            info!(
                                                request_id = %request_id,
                                                approved = approved,
                                                "Feishu approval response submitted, exec will continue"
                                            );
                                            return;
                                        }
                                    }
                                }
                                let mut meta = serde_json::json!({
                                    "pending_approval_request_id": request_id
                                });
                                if is_group_meta_fwd {
                                    meta["group"] = serde_json::json!(true);
                                }
                                let inbound = InboundMessage {
                                    channel: channel_name_fwd.clone(),
                                    sender_id: sender_open_id_fwd.clone(),
                                    chat_id: chat_id_fwd.clone(),
                                    content: content_fwd.clone(),
                                    timestamp: chrono::Utc::now(),
                                    media: vec![],
                                    metadata: meta,
                                };
                                let _ = inbound_tx_fwd.try_send(inbound);
                                return;
                            }
                            let mut meta = serde_json::json!({
                                "message_id": message_id_fwd,
                                "message_type": message_type_fwd,
                                "chat_type": chat_type_fwd,
                            });
                            if is_group_meta_fwd {
                                meta["group"] = serde_json::json!(true);
                            }
                            let inbound = InboundMessage {
                                channel: channel_name_fwd,
                                sender_id: sender_open_id_fwd,
                                chat_id: chat_id_fwd,
                                content: content_fwd,
                                timestamp: chrono::Utc::now(),
                                media: vec![],
                                metadata: meta,
                            };
                            match inbound_tx_fwd.try_send(inbound) {
                                Ok(()) => info!("Feishu inbound message forwarded to bus"),
                                Err(e) => error!("Failed to forward Feishu inbound message: {e}"),
                            }
                        });
                    })
                    .expect("Failed to register im.message.receive_v1 handler")
                    .build();

                let lark_config = Arc::new(
                    open_lark::core::config::Config::builder()
                        .app_id(&app_id_for_config)
                        .app_secret(&app_secret_for_config)
                        .req_timeout(std::time::Duration::from_secs(30))
                        .http_client(if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
                            crate::appcontainer_dns::build_reqwest_client()
                        } else {
                            reqwest::Client::new()
                        })
                        .build(),
                );

                info!("Feishu WebSocket connecting...");
                let result = LarkWsClient::open(lark_config, handler).await;
                match &result {
                    Ok(()) => {
                        info!("Feishu WebSocket connection closed normally");
                        let _ = result_tx.send(Ok(()));
                    }
                    Err(e) => {
                        error!("Feishu WebSocket error: {e:#}");
                        let _ = result_tx.send(Err(format!("{e}")));
                    }
                }
            });
        });

        // Wait for the WS connection to finish (it blocks until disconnect)
        match result_rx.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(classify_feishu_error(&e)),
            Err(_) => Err(FeishuWsError::Transient(
                "Feishu WebSocket thread terminated unexpectedly".into(),
            )),
        }
    }
}

#[async_trait]
impl Channel for FeishuChannel {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn start(&mut self) -> Result<()> {
        info!("Feishu channel starting (WebSocket long-connection)");
        self.running = true;

        let client = self.build_lark_client();

        // --- Verify bot credentials ---
        match client.bot.v3.info.get(None).await {
            Ok(response) => {
                if let Some(data) = response.data {
                    info!("Feishu bot connected successfully");
                    if let Some(name) = &data.bot.app_name {
                        info!("  Bot name: {name}");
                    }
                    if let Some(open_id) = &data.bot.open_id {
                        info!("  Open ID: {open_id}");
                    }
                } else {
                    warn!("Feishu bot info response contained no data");
                }
            }
            Err(e) => {
                // If credential verification fails, treat as unrecoverable
                let err_str = format!("{e:?}");
                let classified = classify_feishu_error(&err_str);
                if matches!(classified, FeishuWsError::Unrecoverable(_)) {
                    error!("Feishu credential verification failed: {e:?}");
                    self.notify_system_error(&format!(
                        "Credential verification failed: {e:?}"
                    ))
                    .await;
                    return Err(anyhow::anyhow!(
                        "Feishu channel stopped: credential verification failed: {e:?}"
                    ));
                }
                // Transient error during verification — log and continue
                warn!("Failed to fetch Feishu bot info (transient): {e:?}");
            }
        }

        // --- Spawn outbound message dispatcher ---
        let mut outbound_rx = self.outbound_rx.take().unwrap();
        let outbound_client = self.build_lark_client();
        let feishu_channel_name = self.config.name.clone();
        let feishu_app_id = self.config.app_id.clone();
        let feishu_app_secret = self.config.app_secret.clone();
        let pending_approvals_clone = self.pending_approvals.clone();
        let show_tool_calls = self.show_tool_calls;
        let tool_result_preview_chars = self.tool_result_preview_chars;
        let workspace_dir = self.workspace_dir.clone();
        let outbound_tx_for_fail = self.outbound_tx.clone();
        tokio::spawn(async move {
            while let Ok(msg) = outbound_rx.recv().await {
                if msg.channel != feishu_channel_name {
                    continue;
                }
                let (content, media_paths) = match &msg.message_type {
                    crate::bus::OutboundMessageType::Chat { content, media } => {
                        (content.clone(), media.clone())
                    }
                    crate::bus::OutboundMessageType::ToolProgress {
                        tool_name,
                        status,
                        result_preview,
                    } => {
                        if !show_tool_calls {
                            continue;
                        }
                        let preview = if result_preview.is_empty() {
                            String::new()
                        } else if result_preview.len() > tool_result_preview_chars {
                            format!("{}...", result_preview.chars().take(tool_result_preview_chars).collect::<String>())
                        } else {
                            result_preview.clone()
                        };
                        let content = if preview.is_empty() {
                            format!("🔧 {} — {}", tool_name, status)
                        } else {
                            format!("🔧 {} — {}\n{}", tool_name, status, preview)
                        };
                        (content, vec![])
                    }
                    crate::bus::OutboundMessageType::ApprovalRequest { request } => {
                        let user_id = request.session_id.split(':').last().unwrap_or("").to_string();
                        if !user_id.is_empty() {
                            let mut pending = pending_approvals_clone.write().await;
                            pending.insert(user_id, (request.id.clone(), msg.chat_id.clone()));
                        }
                        let content = request
                            .display_message
                            .as_deref()
                            .filter(|s| !s.is_empty())
                            .map(String::from)
                            .unwrap_or_else(|| Self::format_approval_request(&request));
                        (content, vec![])
                    }
                };
                if !content.is_empty() {
                    if let Err(e) =
                        FeishuChannel::send_text(&outbound_client, &msg.chat_id, &content).await
                    {
                        error!("Feishu outbound send error: {e:#}");
                    }
                }
                // Feishu supports one file per message; send each file as a separate file message
                if !media_paths.is_empty() && workspace_dir.is_some() {
                    let ws = workspace_dir.as_ref().unwrap();
                    for path_str in &media_paths {
                        let path = std::path::Path::new(path_str);
                        let abs = if path.is_absolute() {
                            path.to_path_buf()
                        } else {
                            ws.join(path_str)
                        };
                        if abs.exists() {
                            if let Ok(file_data) = std::fs::read(&abs) {
                                let file_name = abs
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| "file".to_string());
                                let ext = abs
                                    .extension()
                                    .map(|e| e.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| "txt".to_string());
                                let file_type = feishu_file_type_from_extension(&ext);
                                let upload_result = if file_type == "image" {
                                    feishu_upload_image(
                                        &feishu_app_id,
                                        &feishu_app_secret,
                                        &file_name,
                                        file_data,
                                    )
                                    .await
                                    .map(|image_key| ("image", serde_json::json!({ "image_key": image_key }).to_string()))
                                } else {
                                    feishu_upload_file(
                                        &feishu_app_id,
                                        &feishu_app_secret,
                                        file_type,
                                        &file_name,
                                        file_data,
                                    )
                                    .await
                                    .map(|file_key| ("file", serde_json::json!({ "file_key": file_key }).to_string()))
                                };
                                match upload_result {
                                    Ok((msg_type, content_str)) => {
                                        let body = CreateMessageRequestBody::builder()
                                            .receive_id(&msg.chat_id)
                                            .msg_type(msg_type)
                                            .content(content_str)
                                            .build();
                                        let req = CreateMessageRequest::builder()
                                            .receive_id_type("chat_id")
                                            .request_body(body)
                                            .build();
                                        if let Err(e) = outbound_client
                                            .im
                                            .v1
                                            .message
                                            .create(req, None)
                                            .await
                                        {
                                            error!("Feishu send file message error: {e:#}");
                                        }
                                    }
                                    Err(e) => {
                                        error!("Feishu file/image upload error: {e}");
                                        let is_permission_denied = e.contains("99991672")
                                            || e.contains("im:resource");
                                        if is_permission_denied {
                                            if let Some(ref tx) = outbound_tx_for_fail {
                                                let hint = "⚠️ 文件发送失败：应用未开通「发送与上传消息中的资源文件」权限。请在飞书开放平台为该应用开通 im:resource 或 im:resource:upload 权限后重试。";
                                                let _ = tx.send(OutboundMessage::chat(
                                                    feishu_channel_name.clone(),
                                                    msg.chat_id.clone(),
                                                    hint.to_string(),
                                                    vec![],
                                                    None,
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        // --- WebSocket connection loop with retry logic ---
        let retry_policy = RetryPolicy::default();
        let mut retry_state = RetryState::new();

        while self.running {
            let result = FeishuChannel::attempt_ws_connection(
                self.inbound_tx.clone(),
                self.config.allowlist.clone(),
                self.config.name.clone(),
                self.config.enable_allowlist,
                self.config.group_my_name.clone(),
                self.config.app_id.clone(),
                self.config.app_secret.clone(),
                self.approval_manager.clone(),
                self.approval_classifier.clone(),
                self.pending_approvals.clone(),
                self.workspace_dir.clone(),
            )
            .await;

            match result {
                Ok(()) => {
                    // Connection closed normally — reset state and reconnect
                    if retry_state.attempts > 0 {
                        info!(
                            attempts = retry_state.attempts,
                            "Feishu WebSocket recovered, resetting retry state"
                        );
                    }
                    retry_state.reset();
                    info!("Feishu WebSocket closed normally, reconnecting...");
                    // Brief pause before reconnecting to avoid tight loop
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
                Err(FeishuWsError::Unrecoverable(msg)) => {
                    error!(
                        error = %msg,
                        "Feishu encountered unrecoverable error, stopping channel"
                    );
                    self.notify_system_error(&msg).await;
                    self.running = false;
                    return Err(anyhow::anyhow!(
                        "Feishu channel stopped: unrecoverable error: {msg}"
                    ));
                }
                Err(FeishuWsError::Transient(msg)) => {
                    let should_retry =
                        retry_state.record_failure(&retry_policy, msg.clone());

                    if should_retry {
                        let delay = retry_state.next_delay(&retry_policy);
                        warn!(
                            error = %msg,
                            attempt = retry_state.attempts,
                            max_retries = retry_policy.max_retries,
                            delay_ms = delay.as_millis() as u64,
                            "Feishu WebSocket error, retrying after backoff"
                        );
                        tokio::time::sleep(delay).await;
                    } else {
                        // Retries exhausted — enter cooldown
                        error!(
                            error = %msg,
                            attempts = retry_state.attempts,
                            "Feishu retries exhausted, entering cooldown"
                        );

                        let cooldown = retry_policy.max_delay;
                        warn!(
                            cooldown_secs = cooldown.as_secs(),
                            "Feishu entering cooldown before reconnection attempt"
                        );
                        tokio::time::sleep(cooldown).await;

                        // Reset state and resume connection attempts
                        retry_state.reset();
                        info!("Feishu cooldown complete, resuming connection attempts");
                    }
                }
            }
        }

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        info!("Feishu channel stopping");
        self.running = false;
        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> Result<()> {
        let client = self.build_lark_client();
        let (content, media) = match &msg.message_type {
            crate::bus::OutboundMessageType::Chat { content, media } => {
                (content.clone(), media.clone())
            }
            crate::bus::OutboundMessageType::ApprovalRequest { request } => (
                request
                    .display_message
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .unwrap_or_else(|| approval_formatter::format_approval_request(request)),
                vec![],
            ),
            crate::bus::OutboundMessageType::ToolProgress {
                tool_name,
                status,
                result_preview,
            } => {
                if !self.show_tool_calls {
                    return Ok(());
                }
                let preview = if result_preview.is_empty() {
                    String::new()
                } else if result_preview.len() > self.tool_result_preview_chars {
                    format!("{}...", result_preview.chars().take(self.tool_result_preview_chars).collect::<String>())
                } else {
                    result_preview.clone()
                };
                let content = if preview.is_empty() {
                    format!("🔧 {} — {}", tool_name, status)
                } else {
                    format!("🔧 {} — {}\n{}", tool_name, status, preview)
                };
                FeishuChannel::send_text(&client, &msg.chat_id, &content).await?;
                return Ok(());
            }
        };
        if !content.is_empty() {
            FeishuChannel::send_text(&client, &msg.chat_id, &content).await?;
        }
        if !media.is_empty() && self.workspace_dir.is_some() {
            let ws = self.workspace_dir.as_ref().unwrap();
            for path_str in &media {
                let path = std::path::Path::new(path_str);
                let abs = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    ws.join(path_str)
                };
                if abs.exists() {
                    if let Ok(file_data) = std::fs::read(&abs) {
                        let file_name = abs
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "file".to_string());
                        let ext = abs
                            .extension()
                            .map(|e| e.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "txt".to_string());
                        let file_type = feishu_file_type_from_extension(&ext);
                        let send_result = if file_type == "image" {
                            feishu_upload_image(
                                &self.config.app_id,
                                &self.config.app_secret,
                                &file_name,
                                file_data,
                            )
                            .await
                            .map(|image_key| ("image", serde_json::json!({ "image_key": image_key }).to_string()))
                        } else {
                            feishu_upload_file(
                                &self.config.app_id,
                                &self.config.app_secret,
                                file_type,
                                &file_name,
                                file_data,
                            )
                            .await
                            .map(|file_key| ("file", serde_json::json!({ "file_key": file_key }).to_string()))
                        };
                        if let Ok((msg_type, content_str)) = send_result {
                            let body = CreateMessageRequestBody::builder()
                                .receive_id(&msg.chat_id)
                                .msg_type(msg_type)
                                .content(content_str)
                                .build();
                            let req = CreateMessageRequest::builder()
                                .receive_id_type("chat_id")
                                .request_body(body)
                                .build();
                            let _ = client.im.v1.message.create(req, None).await;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
