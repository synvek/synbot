//! Feishu channel — WebSocket long-connection (feishu_ws) + official REST API.
//!
//! Receives messages via Feishu WebSocket (no open-lark). Sending and file
//! operations use the official Feishu Open API (feishu_api module). Config is
//! unchanged (app_id, app_secret, allowlist, etc.).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use crate::channels::feishu_ws::Frame;
use serde::Deserialize;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{error, info, warn};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::file_handler;
use crate::channels::approval_classifier;
use crate::channels::feishu_api::FeishuApiClient;
use crate::channels::feishu_ws::{build_event_response_frame, get_ws_endpoint, run_ws_loop};
use crate::channels::{approval_formatter, Channel, RetryPolicy, RetryState};
use crate::config::{AllowlistEntry, FeishuConfig};
use crate::rig_provider::SynbotCompletionModel;
use crate::tools::approval::{ApprovalManager, ApprovalResponse};

/// Optional sender to notify the user when file upload fails (e.g. missing permission).
type OutboundTx = Option<tokio::sync::broadcast::Sender<OutboundMessage>>;

/// Per-channel state for processing events (pending approvals, classifier, workspace).
#[derive(Clone)]
struct FeishuChannelEventState {
    pending_approvals: Arc<RwLock<HashMap<String, (String, String)>>>,
    approval_classifier: Option<Arc<dyn SynbotCompletionModel>>,
    workspace_dir: Option<PathBuf>,
}

#[derive(Debug)]
enum FeishuWsError {
    Transient(String),
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

pub struct FeishuChannel {
    config: FeishuConfig,
    show_tool_calls: bool,
    tool_result_preview_chars: usize,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    outbound_tx: OutboundTx,
    running: bool,
    approval_manager: Option<Arc<ApprovalManager>>,
    approval_classifier: Option<Arc<dyn SynbotCompletionModel>>,
    pending_approvals: Arc<RwLock<HashMap<String, (String, String)>>>,
    workspace_dir: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Feishu event payload (shared by WebSocket P2ImMessageReceiveV1 → process_im_message_receive)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct FeishuImMessageEvent {
    pub sender: Option<FeishuSender>,
    pub message: Option<FeishuMessage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeishuSender {
    pub sender_id: Option<FeishuSenderId>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeishuSenderId {
    pub open_id: Option<String>,
    pub user_id: Option<String>,
    pub union_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeishuMessage {
    pub message_id: Option<String>,
    pub chat_id: Option<String>,
    pub chat_type: Option<String>,
    pub message_type: Option<String>,
    pub content: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers: token, upload, file type, approval keywords, error classification
// ---------------------------------------------------------------------------

/// Download file/image from a message using "get message resource" API.
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
    let token = super::feishu_api::get_tenant_access_token(app_id, app_secret).await?;
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

async fn feishu_upload_image(
    app_id: &str,
    app_secret: &str,
    file_name: &str,
    file_data: Vec<u8>,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e: reqwest::Error| e.to_string())?;
    let token = super::feishu_api::get_tenant_access_token(app_id, app_secret).await?;
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

fn feishu_file_type_from_extension(ext: &str) -> &'static str {
    let ext = ext.to_lowercase();
    let ext = ext.trim();
    match ext {
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" => "image",
        "mp3" | "wav" | "amr" | "aac" | "ogg" => "audio",
        "mp4" | "mov" | "avi" | "mkv" | "flv" => "video",
        _ => "stream",
    }
}

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
    let token = super::feishu_api::get_tenant_access_token(app_id, app_secret).await?;
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

fn parse_approval_response_keywords(text: &str) -> Option<bool> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let t_lower = t.to_lowercase();
    let reject_exact = ["no", "n", "reject", "拒绝", "否", "deny", "不同意"];
    if reject_exact.iter().any(|s| t_lower == *s || t_lower.starts_with(&format!("{} ", s)) || t_lower.ends_with(&format!(" {}", s))) {
        return Some(false);
    }
    if t.contains("不同意") || t.contains("拒绝") {
        return Some(false);
    }
    let approve_exact = ["yes", "y", "approve", "批准", "是", "ok", "同意", "好", "1"];
    if approve_exact.iter().any(|s| t_lower == *s || t_lower.starts_with(&format!("{} ", s)) || t_lower.ends_with(&format!(" {}", s))) {
        return Some(true);
    }
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

// ---------------------------------------------------------------------------
// FeishuChannel
// ---------------------------------------------------------------------------

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

    pub fn with_outbound_tx(mut self, tx: tokio::sync::broadcast::Sender<OutboundMessage>) -> Self {
        self.outbound_tx = Some(tx);
        self
    }

    pub fn with_approval_manager(mut self, manager: Arc<ApprovalManager>) -> Self {
        self.approval_manager = Some(manager);
        self
    }

    pub fn with_approval_classifier(mut self, model: Arc<dyn SynbotCompletionModel>) -> Self {
        self.approval_classifier = Some(model);
        self
    }

    fn format_approval_request(request: &crate::tools::approval::ApprovalRequest) -> String {
        approval_formatter::format_approval_request(request)
    }

    fn build_api_client(&self) -> FeishuApiClient {
        FeishuApiClient::new(&self.config.app_id, &self.config.app_secret)
    }

    /// Send text message via Feishu IM v1 API (chunked if needed).
    async fn send_text(client: &FeishuApiClient, chat_id: &str, text: &str) -> Result<()> {
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
            client
                .send_message("chat_id", chat_id, "text", &content)
                .await
                .map_err(|e| {
                    error!("Feishu send_text error: {e:#}");
                    e
                })?;
        }
        Ok(())
    }

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
}

/// WebSocket event callback payload (schema 2.0): header.event_type + event body.
#[derive(Debug, Deserialize)]
struct WsEventPayload {
    #[serde(default)]
    header: WsEventHeader,
    #[serde(default)]
    event: Option<FeishuImMessageEvent>,
}

#[derive(Debug, Default, Deserialize)]
struct WsEventHeader {
    #[serde(rename = "event_type")]
    event_type: Option<String>,
}

// ---------------------------------------------------------------------------
// WebSocket: attempt one connection using feishu_ws (no open-lark)
// ---------------------------------------------------------------------------

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
    let http_client = if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
        crate::appcontainer_dns::build_reqwest_client()
    } else {
        reqwest::Client::new()
    };

    let (ws_url, client_config) =
        get_ws_endpoint(&http_client, &app_id, &app_secret)
            .await
            .map_err(|e| FeishuWsError::Transient(e))?;

    let event_state = FeishuChannelEventState {
        pending_approvals: pending_approvals.clone(),
        approval_classifier: approval_classifier.clone(),
        workspace_dir: workspace_dir.clone(),
    };
    let config = FeishuConfig {
        name: channel_name.clone(),
        enabled: true,
        app_id: app_id.clone(),
        app_secret: app_secret.clone(),
        allowlist: allowlist.clone(),
        enable_allowlist,
        group_my_name: group_my_name.clone(),
        show_tool_calls: true,
    };

    info!("Feishu WebSocket connecting...");
    let result = run_ws_loop(ws_url, client_config, move |frame: Frame| {
        let inbound_tx = inbound_tx.clone();
        let channel_name = channel_name.clone();
        let config = config.clone();
        let approval_manager = approval_manager.clone();
        let event_state = event_state.clone();
        async move {
            let start = Instant::now();
            let payload = frame.payload.as_ref().and_then(|p| {
                serde_json::from_slice::<WsEventPayload>(p).ok()
            });
            let event = match payload.as_ref() {
                Some(p) if p.header.event_type.as_deref() == Some("im.message.receive_v1") => {
                    p.event.clone()
                }
                _ => return None,
            };
            let event = match event {
                Some(ev) => ev,
                None => return None,
            };
            let client = FeishuApiClient::new(&config.app_id, &config.app_secret);
            process_im_message_receive(
                &channel_name,
                &config,
                &event,
                &client,
                &inbound_tx,
                approval_manager.as_ref(),
                Some(&event_state),
            )
            .await;
            let elapsed = start.elapsed().as_millis();
            Some(build_event_response_frame(&frame, elapsed))
        }
    })
    .await;

    match result {
        Ok(()) => {
            info!("Feishu WebSocket connection closed normally");
            Ok(())
        }
        Err(e) => Err(classify_feishu_error(&e)),
    }
}

// ---------------------------------------------------------------------------
// Process one im.message.receive_v1 event (shared by WebSocket callback)
// ---------------------------------------------------------------------------

/// Process a single im.message.receive_v1 event: allowlist/mention logic, file download, approval, forward to bus.
async fn process_im_message_receive(
    channel_name: &str,
    config: &FeishuConfig,
    event: &FeishuImMessageEvent,
    client: &FeishuApiClient,
    inbound_tx: &mpsc::Sender<InboundMessage>,
    approval_manager: Option<&Arc<ApprovalManager>>,
    event_state: Option<&FeishuChannelEventState>,
) {
    let sender = match &event.sender {
        Some(s) => s,
        None => return,
    };
    let sender_open_id = sender
        .sender_id
        .as_ref()
        .and_then(|s| s.open_id.as_deref())
        .unwrap_or("")
        .to_string();
    let msg = match &event.message {
        Some(m) => m,
        None => return,
    };
    let message_id = msg.message_id.as_deref().unwrap_or("").to_string();
    let chat_id = msg.chat_id.as_deref().unwrap_or("").to_string();
    let chat_type = msg.chat_type.as_deref().unwrap_or("").to_string();
    let message_type = msg.message_type.as_deref().unwrap_or("").to_string();
    let content_str = msg.content.as_deref().unwrap_or("");

    let is_file_like = message_type == "file" || message_type == "image" || message_type == "media";
    let workspace_dir = event_state.and_then(|s| s.workspace_dir.as_ref());

    if is_file_like {
        if let Some(ws) = workspace_dir {
            if let Ok(content_json) = serde_json::from_str::<serde_json::Value>(content_str) {
                let file_key = content_json.get("file_key").and_then(|v| v.as_str());
                let image_key = content_json.get("image_key").and_then(|v| v.as_str());
                let file_name = content_json
                    .get("file_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(if message_type == "image" { "image.png" } else { "file" });
                let (key, use_image_api) = if let Some(fk) = file_key {
                    (fk.to_string(), false)
                } else if let Some(ik) = image_key {
                    (ik.to_string(), true)
                } else {
                    (String::new(), false)
                };
                if !key.is_empty() {
                    let data_result = if use_image_api {
                        client.get_image(&key).await
                    } else {
                        client.get_file(&key).await
                    };
                    let data_result = match data_result {
                        Ok(d) => Ok(d),
                        Err(e) => {
                            warn!("Feishu file/image get failed, trying message resource API: {e:#}");
                            let resource_type = if use_image_api { "image" } else { "file" };
                            feishu_fetch_message_resource(
                                &config.app_id,
                                &config.app_secret,
                                &message_id,
                                &key,
                                resource_type,
                            )
                            .await
                            .map_err(anyhow::Error::msg)
                        }
                    };
                    match data_result {
                        Ok(data) => {
                            if let Ok(path) = file_handler::save_incoming_file(ws, file_name, &data) {
                                let media_path = path.to_string_lossy().into_owned();
                                let _ = inbound_tx
                                    .send(InboundMessage {
                                        channel: channel_name.to_string(),
                                        sender_id: sender_open_id.clone(),
                                        chat_id: chat_id.clone(),
                                        content: format!("[文件] {}", file_name),
                                        timestamp: chrono::Utc::now(),
                                        media: vec![media_path],
                                        metadata: serde_json::json!({
                                            "message_id": message_id,
                                            "message_type": message_type,
                                            "chat_type": chat_type,
                                        }),
                                    })
                                    .await;
                            } else {
                                let _ = inbound_tx
                                    .send(InboundMessage {
                                        channel: channel_name.to_string(),
                                        sender_id: sender_open_id.clone(),
                                        chat_id: chat_id.clone(),
                                        content: format!("[文件] {} 保存到工作区失败", file_name),
                                        timestamp: chrono::Utc::now(),
                                        media: vec![],
                                        metadata: serde_json::json!({
                                            "message_id": message_id,
                                            "message_type": message_type,
                                            "chat_type": chat_type,
                                        }),
                                    })
                                    .await;
                            }
                        }
                        Err(e) => {
                            let _ = inbound_tx
                                .send(InboundMessage {
                                    channel: channel_name.to_string(),
                                    sender_id: sender_open_id.clone(),
                                    chat_id: chat_id.clone(),
                                    content: format!("[文件] {} 下载失败（{}）", file_name, e),
                                    timestamp: chrono::Utc::now(),
                                    media: vec![],
                                    metadata: serde_json::json!({
                                        "message_id": message_id,
                                        "message_type": message_type,
                                        "chat_type": chat_type,
                                        "download_error": e.to_string(),
                                    }),
                                })
                                .await;
                        }
                    };
                }
                return;
            }
        }
        warn!(
            "Feishu {} message skipped (no workspace or missing file_key/image_key)",
            message_type
        );
        return;
    }

    let text = if message_type == "text" {
        serde_json::from_str::<serde_json::Value>(content_str)
            .ok()
            .and_then(|v| v.get("text").and_then(|t| t.as_str().map(String::from)))
            .unwrap_or_default()
    } else {
        content_str.to_string()
    };

    if text.is_empty() {
        warn!("Feishu message text is empty, skipping");
        return;
    }

    let is_group = chat_type != "p2p";
    let allowlist = &config.allowlist;
    let enable_allowlist = config.enable_allowlist;
    let group_my_name = &config.group_my_name;

    let (_trigger_agent, content, is_group_meta) = if !enable_allowlist {
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
                    let _ = inbound_tx
                        .try_send(InboundMessage {
                            channel: channel_name.to_string(),
                            sender_id: sender_open_id.clone(),
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
                warn!(chat_id = %chat_id, "Feishu: chat not in allowlist");
                let _ = client
                    .send_message("chat_id", &chat_id, "text", &serde_json::json!({ "text": "未配置聊天许可，请配置。" }).to_string())
                    .await;
                let _ = inbound_tx.try_send(InboundMessage {
                    channel: channel_name.to_string(),
                    sender_id: sender_open_id.clone(),
                    chat_id: chat_id.clone(),
                    content: text.clone(),
                    timestamp: chrono::Utc::now(),
                    media: vec![],
                    metadata: serde_json::json!({
                        "message_id": message_id,
                        "message_type": message_type,
                        "chat_type": chat_type,
                        "trigger_agent": false,
                    }),
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
                        let _ = inbound_tx.try_send(InboundMessage {
                            channel: channel_name.to_string(),
                            sender_id: sender_open_id.clone(),
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

    let pending_approvals = event_state.map(|s| s.pending_approvals.clone());
    let approval_classifier = event_state.and_then(|s| s.approval_classifier.clone());

    if let Some(ref pending) = pending_approvals {
        let removed = {
            let mut guard = pending.write().await;
            guard.remove(&chat_id)
        };
        if let Some((request_id, _)) = removed {
            let mut approved_opt: Option<bool> = None;
            if let Some(ref model) = approval_classifier {
                approved_opt = approval_classifier::classify_approval_response(model.as_ref(), &content).await;
            }
            if approved_opt.is_none() {
                approved_opt = parse_approval_response_keywords(&content);
            }
            if let Some(mgr) = approval_manager {
                if let Some(approved) = approved_opt {
                    let response = ApprovalResponse {
                        request_id: request_id.clone(),
                        approved,
                        responder: sender_open_id.clone(),
                        timestamp: chrono::Utc::now(),
                    };
                    if let Err(e) = mgr.submit_response(response).await {
                        error!("Feishu failed to submit approval response: {e:#}");
                    } else {
                        return;
                    }
                }
            }
            let mut meta = serde_json::json!({ "pending_approval_request_id": request_id });
            if is_group_meta {
                meta["group"] = serde_json::json!(true);
            }
            let _ = inbound_tx.try_send(InboundMessage {
                channel: channel_name.to_string(),
                sender_id: sender_open_id.clone(),
                chat_id: chat_id.clone(),
                content: content.clone(),
                timestamp: chrono::Utc::now(),
                media: vec![],
                metadata: meta,
            });
            return;
        }
    }

    let mut meta = serde_json::json!({
        "message_id": message_id,
        "message_type": message_type,
        "chat_type": chat_type,
    });
    if is_group_meta {
        meta["group"] = serde_json::json!(true);
    }
    let inbound = InboundMessage {
        channel: channel_name.to_string(),
        sender_id: sender_open_id,
        chat_id,
        content,
        timestamp: chrono::Utc::now(),
        media: vec![],
        metadata: meta,
    };
    match inbound_tx.try_send(inbound) {
        Ok(()) => info!("Feishu inbound message forwarded to bus"),
        Err(e) => error!("Failed to forward Feishu inbound message: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Channel impl
// ---------------------------------------------------------------------------

#[async_trait]
impl Channel for FeishuChannel {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn start(&mut self) -> Result<()> {
        info!("Feishu channel starting (WebSocket long-connection)");
        self.running = true;

        let client = self.build_api_client();

        match client.get_bot_info().await {
            Ok(info) => {
                info!("Feishu bot connected successfully");
                if let Some(name) = &info.app_name {
                    info!("  Bot name: {name}");
                }
                if let Some(open_id) = &info.open_id {
                    info!("  Open ID: {open_id}");
                }
            }
            Err(e) => {
                let err_str = format!("{e:?}");
                let classified = classify_feishu_error(&err_str);
                if matches!(classified, FeishuWsError::Unrecoverable(_)) {
                    error!("Feishu credential verification failed: {e:?}");
                    self.notify_system_error(&format!("Credential verification failed: {e:?}"))
                        .await;
                    return Err(anyhow::anyhow!(
                        "Feishu channel stopped: credential verification failed: {e:?}"
                    ));
                }
                warn!("Failed to fetch Feishu bot info (transient): {e:?}");
            }
        }

        let mut outbound_rx = self.outbound_rx.take().unwrap();
        let outbound_client = self.build_api_client();
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
                            format!(
                                "{}...",
                                result_preview.chars().take(tool_result_preview_chars).collect::<String>()
                            )
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
                            .unwrap_or_else(|| FeishuChannel::format_approval_request(&request));
                        (content, vec![])
                    }
                };
                if !content.is_empty() {
                    if let Err(e) = FeishuChannel::send_text(&outbound_client, &msg.chat_id, &content).await
                    {
                        error!("Feishu outbound send error: {e:#}");
                    }
                }
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
                                    .map(|image_key| {
                                        ("image", serde_json::json!({ "image_key": image_key }).to_string())
                                    })
                                } else {
                                    feishu_upload_file(
                                        &feishu_app_id,
                                        &feishu_app_secret,
                                        file_type,
                                        &file_name,
                                        file_data,
                                    )
                                    .await
                                    .map(|file_key| {
                                        ("file", serde_json::json!({ "file_key": file_key }).to_string())
                                    })
                                };
                                match upload_result {
                                    Ok((msg_type, content_str)) => {
                                        if let Err(e) = outbound_client
                                            .send_message("chat_id", &msg.chat_id, msg_type, &content_str)
                                            .await
                                        {
                                            error!("Feishu send file message error: {e:#}");
                                        }
                                    }
                                    Err(e) => {
                                        error!("Feishu file/image upload error: {e}");
                                        let is_permission_denied =
                                            e.contains("99991672") || e.contains("im:resource");
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

        let retry_policy = RetryPolicy::default();
        let mut retry_state = RetryState::new();

        while self.running {
            let result = attempt_ws_connection(
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
                    if retry_state.attempts > 0 {
                        info!(
                            attempts = retry_state.attempts,
                            "Feishu WebSocket recovered, resetting retry state"
                        );
                    }
                    retry_state.reset();
                    info!("Feishu WebSocket closed normally, reconnecting...");
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
                    let should_retry = retry_state.record_failure(&retry_policy, msg.clone());
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
                        error!(
                            error = %msg,
                            attempts = retry_state.attempts,
                            "Feishu retries exhausted, entering cooldown"
                        );
                        let cooldown = retry_policy.max_delay;
                        tokio::time::sleep(cooldown).await;
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
        let client = self.build_api_client();
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
                    format!(
                        "{}...",
                        result_preview
                            .chars()
                            .take(self.tool_result_preview_chars)
                            .collect::<String>()
                    )
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
                            .map(|image_key| {
                                ("image", serde_json::json!({ "image_key": image_key }).to_string())
                            })
                        } else {
                            feishu_upload_file(
                                &self.config.app_id,
                                &self.config.app_secret,
                                file_type,
                                &file_name,
                                file_data,
                            )
                            .await
                            .map(|file_key| {
                                ("file", serde_json::json!({ "file_key": file_key }).to_string())
                            })
                        };
                        if let Ok((msg_type, content_str)) = send_result {
                            let _ = client
                                .send_message("chat_id", &msg.chat_id, msg_type, &content_str)
                                .await;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
