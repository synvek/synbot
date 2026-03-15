//! DingTalk channel — Stream mode (self-implemented protocol).
//!
//! Receives robot messages via CALLBACK; replies via sessionWebhook POST.
//! File/picture/audio/video (single chat): download via robot messageFiles API; outbound files via media upload + sessionWebhook.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{info, warn};

use crate::bus::{InboundMessage, OutboundMessage, OutboundMessageType};
use crate::channels::approval_formatter;
use crate::channels::dingtalk_stream;
use crate::channels::file_handler;
use crate::channels::Channel;
use crate::config::DingTalkConfig;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BotMessageData {
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    session_webhook: Option<String>,
    #[serde(default)]
    session_webhook_expired_time: Option<i64>,
    #[serde(default)]
    sender_id: Option<String>,
    #[serde(default)]
    msg_id: Option<String>,
    #[serde(default)]
    text: Option<TextContent>,
    #[serde(default)]
    msgtype: Option<String>,
    /// Same as Open Platform 机器人 robotCode; required for messageFiles/download.
    #[serde(default)]
    robot_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TextContent {
    #[serde(default)]
    content: Option<String>,
}

struct SessionEntry {
    webhook: String,
    /// Unix ms when webhook expires (from DingTalk).
    expires_ms: Option<i64>,
}

pub struct DingTalkChannel {
    /// Fallback when callback JSON has no robotCode (should normally be present on Stream callbacks).
    robot_code_config: String,
    config: DingTalkConfig,
    show_tool_calls: bool,
    tool_result_preview_chars: usize,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    /// conversationId -> session webhook for reply
    sessions: Arc<RwLock<HashMap<String, SessionEntry>>>,
    http: reqwest::Client,
    workspace_dir: Option<PathBuf>,
}

/// Resolve clientId/clientSecret with optional appKey/appSecret fallback; trims whitespace.
fn effective_credentials(config: &DingTalkConfig) -> (String, String) {
    let id = config.client_id.trim();
    let id = if id.is_empty() {
        config.app_key.as_deref().unwrap_or("").trim()
    } else {
        id
    };
    let secret = config.client_secret.trim();
    let secret = if secret.is_empty() {
        config.app_secret.as_deref().unwrap_or("").trim()
    } else {
        secret
    };
    (id.to_string(), secret.to_string())
}

impl DingTalkChannel {
    pub fn new(
        config: DingTalkConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: broadcast::Receiver<OutboundMessage>,
        show_tool_calls: bool,
        tool_result_preview_chars: usize,
        workspace_dir: Option<PathBuf>,
    ) -> Self {
        let robot_code_config = config.robot_code.trim().to_string();
        Self {
            robot_code_config,
            config,
            show_tool_calls,
            tool_result_preview_chars,
            inbound_tx,
            outbound_rx: Some(outbound_rx),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            http: reqwest::Client::new(),
            workspace_dir,
        }
    }

    fn extract_text(data: &BotMessageData) -> String {
        data.text
            .as_ref()
            .and_then(|t| t.content.clone())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_default()
    }

    /// File/picture/video/audio: `downloadCode` under `content`, `file`, or root (Stream 回调多种形态).
    fn extract_download_from_value(v: &serde_json::Value) -> Option<(String, String)> {
        let msgtype = v
            .get("msgType")
            .or_else(|| v.get("msgtype"))
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_lowercase();
        let mut obj = match msgtype.as_str() {
            "file" => v.get("file").or_else(|| v.get("content")).cloned(),
            "picture" | "image" | "png" => v.get("content").or_else(|| v.get("picture")).or_else(|| v.get("image")).cloned(),
            "video" => v.get("content").or_else(|| v.get("video")).cloned(),
            "audio" | "voice" => v.get("content").or_else(|| v.get("audio")).cloned(),
            "" => None,
            _ => v.get("content").cloned(),
        };
        if obj.is_none() && matches!(msgtype.as_str(), "file" | "picture" | "image" | "video" | "audio" | "voice") {
            obj = v.get("content").cloned();
        }
        let mut content = obj.unwrap_or_else(|| serde_json::json!({}));
        if let Some(s) = content.as_str() {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                content = parsed;
            }
        }
        let code = content
            .get("downloadCode")
            .or_else(|| content.get("download_code"))
            .or_else(|| v.get("downloadCode"))
            .or_else(|| v.get("download_code"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())?;
        // 纯文本消息不要当文件处理
        if msgtype == "text" {
            return None;
        }
        let name = content
            .get("fileName")
            .or_else(|| content.get("file_name"))
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| match msgtype.as_str() {
                "picture" | "png" | "image" => "image.png".into(),
                "video" => "video.mp4".into(),
                "audio" | "voice" => "voice.amr".into(),
                _ => "file".into(),
            });
        Some((code, name))
    }
}

#[async_trait]
impl Channel for DingTalkChannel {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn start(&mut self) -> Result<()> {
        let channel_name = self.config.name.clone();
        let (client_id, client_secret) = effective_credentials(&self.config);
        let inbound_tx = self.inbound_tx.clone();
        let sessions = Arc::clone(&self.sessions);
        let allowlist_enforce = self.config.enable_allowlist;
        let allowlist = self.config.allowlist.clone();

        let outbound_rx = self.outbound_rx.take().expect("start once");
        let sessions_out = Arc::clone(&self.sessions);
        let http_out = self.http.clone();
        let show_tool_calls = self.show_tool_calls;
        let tool_preview = self.tool_result_preview_chars;
        let workspace_dir = self.workspace_dir.clone();

        let channel_name_out = channel_name.clone();
        let client_id_ws = client_id.clone();
        let client_secret_ws = client_secret.clone();
        let workspace_out = workspace_dir.clone();
        tokio::spawn(async move {
            run_outbound_dingtalk(
                channel_name_out,
                outbound_rx,
                sessions_out,
                http_out,
                show_tool_calls,
                tool_preview,
                client_id_ws,
                client_secret_ws,
                workspace_out,
            )
            .await;
        });

        let http_loop = self.http.clone();
        let default_agent = self.config.default_agent.clone();
        let app_key_file = client_id.clone();
        let app_secret_file = client_secret.clone();
        let robot_code_cfg = self.robot_code_config.clone();
        dingtalk_stream::run_forever(http_loop, client_id, client_secret, move |data_str| {
            let inbound_tx = inbound_tx.clone();
            let sessions = Arc::clone(&sessions);
            let channel_name = channel_name.clone();
            let default_agent = default_agent.clone();
            let allowlist_enforce = allowlist_enforce;
            let allowlist = allowlist.clone();
            let http = reqwest::Client::new();
            let workspace_dir = workspace_dir.clone();
            let app_key = app_key_file.clone();
            let app_secret = app_secret_file.clone();
            let robot_code_cfg = robot_code_cfg.clone();
            tokio::spawn(async move {
                let data: BotMessageData = match serde_json::from_str(&data_str) {
                    Ok(d) => d,
                    Err(e) => {
                        warn!(error = %e, "DingTalk bot message JSON parse failed");
                        return;
                    }
                };
                let conversation_id = data
                    .conversation_id
                    .clone()
                    .unwrap_or_default();
                if conversation_id.is_empty() {
                    warn!("DingTalk bot message missing conversationId");
                    return;
                }
                let sender_id = data.sender_id.clone().unwrap_or_default();
                if allowlist_enforce && !allowlist.is_empty() {
                    let ok = allowlist.iter().any(|e| {
                        e.chat_id == conversation_id
                            || e.chat_id == sender_id
                            || (!sender_id.is_empty() && e.chat_id.contains(&sender_id))
                    });
                    if !ok {
                        return;
                    }
                }

                if let Some(ref wh) = data.session_webhook {
                    let entry = SessionEntry {
                        webhook: wh.clone(),
                        expires_ms: data.session_webhook_expired_time,
                    };
                    sessions
                        .write()
                        .await
                        .insert(conversation_id.clone(), entry);
                }

                let value: serde_json::Value =
                    serde_json::from_str(&data_str).unwrap_or_default();
                let mut text = DingTalkChannel::extract_text(&data);
                if text.is_empty() {
                    if let Some(t) = value.get("text") {
                        if let Some(c) = t.get("content").and_then(|x| x.as_str()) {
                            text = c.trim().to_string();
                        }
                    }
                }

                let download = DingTalkChannel::extract_download_from_value(&value);
                let workspace = workspace_dir.as_ref();

                if let Some((download_code, file_name)) = download {
                    if workspace.is_none() {
                        warn!("DingTalk file message: no workspace; set main_agent.workspace so files can be saved");
                        let mut metadata = serde_json::json!({ "default_agent": default_agent });
                        if let Some(ref mid) = data.msg_id {
                            metadata["msgId"] = serde_json::Value::String(mid.clone());
                        }
                        let _ = inbound_tx
                            .send(InboundMessage {
                                channel: channel_name.clone(),
                                sender_id: if sender_id.is_empty() {
                                    conversation_id.clone()
                                } else {
                                    sender_id.clone()
                                },
                                chat_id: conversation_id.clone(),
                                content: format!(
                                    "[File] {} — configure main_agent.workspace to process files (like Feishu).",
                                    file_name
                                ),
                                timestamp: chrono::Utc::now(),
                                media: vec![],
                                metadata,
                            })
                            .await;
                        return;
                    }
                    let ws = workspace.unwrap();
                    let robot_code = data
                        .robot_code
                        .clone()
                        .or_else(|| {
                            value
                                .get("robotCode")
                                .or_else(|| value.get("robot_code"))
                                .and_then(|x| x.as_str())
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                        })
                        .or_else(|| {
                            if robot_code_cfg.is_empty() {
                                None
                            } else {
                                Some(robot_code_cfg.clone())
                            }
                        })
                        .unwrap_or_default();
                    if robot_code.is_empty() {
                        warn!("DingTalk file download needs robotCode (callback or channels.dingtalk[].robotCode)");
                        send_file_error(
                            &inbound_tx,
                            &channel_name,
                            &conversation_id,
                            &sender_id,
                            &file_name,
                            &default_agent,
                            &data.msg_id,
                            "robotCode missing: set robotCode in config (开放平台 → 机器人 → robotCode) or ensure Stream callback includes robotCode",
                        )
                        .await;
                        return;
                    }
                    let token = match dingtalk_access_token(&http, &app_key, &app_secret).await
                    {
                        Ok(t) => t,
                        Err(e) => {
                            warn!(error = %e, "DingTalk access token failed (file)");
                            send_file_error(
                                &inbound_tx,
                                &channel_name,
                                &conversation_id,
                                &sender_id,
                                &file_name,
                                &default_agent,
                                &data.msg_id,
                                &format!("token: {}", e),
                            )
                            .await;
                            return;
                        }
                    };
                    let url = match dingtalk_robot_download_url(
                        &http,
                        &token,
                        &download_code,
                        &robot_code,
                    )
                    .await
                    {
                        Ok(u) => u,
                        Err(e) => {
                            warn!(error = %e, "DingTalk messageFiles/download failed");
                            send_file_error(
                                &inbound_tx,
                                &channel_name,
                                &conversation_id,
                                &sender_id,
                                &file_name,
                                &default_agent,
                                &data.msg_id,
                                &e,
                            )
                            .await;
                            return;
                        }
                    };
                    let bytes = match http.get(&url).send().await {
                        Ok(r) if r.status().is_success() => match r.bytes().await {
                            Ok(b) => b.to_vec(),
                            Err(e) => {
                                send_file_error(
                                    &inbound_tx,
                                    &channel_name,
                                    &conversation_id,
                                    &sender_id,
                                    &file_name,
                                    &default_agent,
                                    &data.msg_id,
                                    &e.to_string(),
                                )
                                .await;
                                return;
                            }
                        },
                        Ok(r) => {
                            send_file_error(
                                &inbound_tx,
                                &channel_name,
                                &conversation_id,
                                &sender_id,
                                &file_name,
                                &default_agent,
                                &data.msg_id,
                                &format!("HTTP {}", r.status()),
                            )
                            .await;
                            return;
                        }
                        Err(e) => {
                            send_file_error(
                                &inbound_tx,
                                &channel_name,
                                &conversation_id,
                                &sender_id,
                                &file_name,
                                &default_agent,
                                &data.msg_id,
                                &e.to_string(),
                            )
                            .await;
                            return;
                        }
                    };
                    let path_result = file_handler::save_incoming_file(ws, &file_name, &bytes);
                    let mut metadata = serde_json::json!({ "default_agent": default_agent });
                    if let Some(ref mid) = data.msg_id {
                        metadata["msgId"] = serde_json::Value::String(mid.clone());
                    }
                    match path_result {
                        Ok(path) => {
                            info!(path = %path.display(), "DingTalk incoming file saved");
                            let _ = inbound_tx
                                .send(InboundMessage {
                                    channel: channel_name,
                                    sender_id: if sender_id.is_empty() {
                                        conversation_id.clone()
                                    } else {
                                        sender_id
                                    },
                                    chat_id: conversation_id,
                                    content: format!("[File] {}", file_name),
                                    timestamp: chrono::Utc::now(),
                                    media: vec![path.to_string_lossy().into_owned()],
                                    metadata,
                                })
                                .await;
                        }
                        Err(e) => {
                            warn!(error = %e, "DingTalk save file failed");
                            let _ = inbound_tx
                                .send(InboundMessage {
                                    channel: channel_name,
                                    sender_id: if sender_id.is_empty() {
                                        conversation_id.clone()
                                    } else {
                                        sender_id
                                    },
                                    chat_id: conversation_id,
                                    content: format!("[File] {} save failed: {}", file_name, e),
                                    timestamp: chrono::Utc::now(),
                                    media: vec![],
                                    metadata,
                                })
                                .await;
                        }
                    }
                    return;
                }

                if text.is_empty() {
                    return;
                }

                let mut metadata = serde_json::json!({
                    "default_agent": default_agent,
                });
                if let Some(ref mid) = data.msg_id {
                    metadata["msgId"] = serde_json::Value::String(mid.clone());
                }
                let msg = InboundMessage {
                    channel: channel_name,
                    sender_id: if sender_id.is_empty() {
                        conversation_id.clone()
                    } else {
                        sender_id
                    },
                    chat_id: conversation_id,
                    content: text,
                    timestamp: chrono::Utc::now(),
                    media: vec![],
                    metadata,
                };
                if inbound_tx.send(msg).await.is_err() {
                    warn!("DingTalk inbound_tx closed");
                }
            });
        })
        .await;
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    async fn send(&self, _msg: &OutboundMessage) -> Result<()> {
        Ok(())
    }
}

async fn send_file_error(
    inbound_tx: &mpsc::Sender<InboundMessage>,
    channel_name: &str,
    conversation_id: &str,
    sender_id: &str,
    file_name: &str,
    default_agent: &str,
    msg_id: &Option<String>,
    err: &str,
) {
    let mut metadata = serde_json::json!({ "default_agent": default_agent });
    if let Some(ref mid) = msg_id {
        metadata["msgId"] = serde_json::Value::String(mid.clone());
    }
    let _ = inbound_tx
        .send(InboundMessage {
            channel: channel_name.to_string(),
            sender_id: if sender_id.is_empty() {
                conversation_id.to_string()
            } else {
                sender_id.to_string()
            },
            chat_id: conversation_id.to_string(),
            content: format!("[File] {} — {}", file_name, err),
            timestamp: chrono::Utc::now(),
            media: vec![],
            metadata,
        })
        .await;
}

async fn dingtalk_access_token(
    http: &reqwest::Client,
    app_key: &str,
    app_secret: &str,
) -> Result<String, String> {
    if app_key.is_empty() || app_secret.is_empty() {
        return Err("appKey/appSecret empty".into());
    }
    let body = serde_json::json!({
        "appKey": app_key,
        "appSecret": app_secret
    });
    let resp = http
        .post("https://api.dingtalk.com/v1.0/oauth2/accessToken")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let txt = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("oauth2/accessToken {status}: {txt}"));
    }
    let v: serde_json::Value =
        serde_json::from_str(&txt).map_err(|e| format!("token JSON: {e}: {txt}"))?;
    v.get("accessToken")
        .or_else(|| v.get("access_token"))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("no accessToken in {txt}"))
}

async fn dingtalk_robot_download_url(
    http: &reqwest::Client,
    token: &str,
    download_code: &str,
    robot_code: &str,
) -> Result<String, String> {
    let body = serde_json::json!({
        "downloadCode": download_code,
        "robotCode": robot_code
    });
    let resp = http
        .post("https://api.dingtalk.com/v1.0/robot/messageFiles/download")
        .header("Content-Type", "application/json")
        .header("x-acs-dingtalk-access-token", token)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let txt = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("messageFiles/download {status}: {txt}"));
    }
    let v: serde_json::Value =
        serde_json::from_str(&txt).map_err(|e| format!("download JSON: {e}"))?;
    let url = v
        .get("downloadUrl")
        .or_else(|| v.get("download_url"))
        .or_else(|| v.get("resourceUrl"))
        .or_else(|| v.get("url"))
        .or_else(|| v.get("result").and_then(|r| r.get("downloadUrl")))
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());
    url.ok_or_else(|| format!("no downloadUrl in {txt}"))
}

async fn run_outbound_dingtalk(
    channel_name: String,
    mut outbound_rx: broadcast::Receiver<OutboundMessage>,
    sessions: Arc<RwLock<HashMap<String, SessionEntry>>>,
    http: reqwest::Client,
    show_tool_calls: bool,
    tool_preview: usize,
    app_key: String,
    app_secret: String,
    workspace_dir: Option<PathBuf>,
) {
    while let Ok(msg) = outbound_rx.recv().await {
        if msg.channel != channel_name {
            continue;
        }
        let (content, media) = match &msg.message_type {
            OutboundMessageType::Chat { content, media } => (content.clone(), media.clone()),
            OutboundMessageType::ApprovalRequest { request } => {
                let s = approval_formatter::format_approval_request(request);
                if s.is_empty() {
                    continue;
                }
                (s, vec![])
            }
            OutboundMessageType::ToolProgress {
                tool_name,
                status,
                result_preview,
            } => {
                if !show_tool_calls {
                    continue;
                }
                let prev = if result_preview.len() > tool_preview {
                    format!("{}…", &result_preview[..tool_preview])
                } else {
                    result_preview.clone()
                };
                (
                    format!("Tool `{}` — {}: {}", tool_name, status, prev),
                    vec![],
                )
            }
        };
        let map = sessions.read().await;
        let entry = match map.get(&msg.chat_id) {
            Some(e) => e,
            None => {
                warn!(chat_id = %msg.chat_id, "DingTalk outbound: no sessionWebhook for chat");
                continue;
            }
        };
        if let Some(exp) = entry.expires_ms {
            let now_ms = chrono::Utc::now().timestamp_millis();
            if now_ms > exp {
                warn!(chat_id = %msg.chat_id, "DingTalk sessionWebhook expired");
                continue;
            }
        }
        let webhook = entry.webhook.clone();
        drop(map);

        const CHUNK: usize = 1800;
        for chunk in split_chunks(&content, CHUNK) {
            if chunk.is_empty() {
                continue;
            }
            if let Err(e) = post_session_text_http(&http, &webhook, &chunk).await {
                warn!(error = %e, "DingTalk sessionWebhook text send failed");
            }
        }

        if !media.is_empty() {
            if workspace_dir.is_none() {
                warn!(chat_id = %msg.chat_id, "DingTalk outbound: cannot send files without main_agent.workspace (media paths are relative to workspace)");
                let _ = post_session_text_http(
                    &http,
                    &webhook,
                    "(文件发送失败：未配置 main_agent.workspace)",
                )
                .await;
            }
            let token = match dingtalk_access_token(&http, &app_key, &app_secret).await {
                Ok(t) => t,
                Err(e) => {
                    warn!(error = %e, "DingTalk outbound: access token for media upload");
                    continue;
                }
            };
            for path_str in &media {
                let path = std::path::Path::new(path_str);
                let abs: PathBuf = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    workspace_dir
                        .as_ref()
                        .map(|ws| ws.join(path_str))
                        .unwrap_or_else(|| path.to_path_buf())
                };
                if !abs.exists() {
                    warn!(path = %path_str, resolved = %abs.display(), "DingTalk outbound media missing");
                    continue;
                }
                info!(path = %abs.display(), "DingTalk outbound sending file");
                let Ok(file_data) = std::fs::read(&abs) else {
                    continue;
                };
                let file_name = abs
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "file".into());
                let ext = abs
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let (upload_type, msgtype_out) = if matches!(
                    ext.as_str(),
                    "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp"
                ) {
                    ("image", "image")
                } else {
                    ("file", "file")
                };
                let media_id =
                    match dingtalk_media_upload(&http, &token, upload_type, &file_name, &file_data)
                        .await
                    {
                        Ok(id) => id,
                        Err(e) => {
                            warn!(error = %e, "DingTalk media upload failed");
                            let _ = post_session_text_http(
                                &http,
                                &webhook,
                                &format!("(file send failed: {})", e),
                            )
                            .await;
                            continue;
                        }
                    };
                let body = if msgtype_out == "image" {
                    serde_json::json!({
                        "msgtype": "image",
                        "image": { "media_id": media_id }
                    })
                } else {
                    serde_json::json!({
                        "msgtype": "file",
                        "file": { "media_id": media_id }
                    })
                };
                match post_session_json(&http, &webhook, &body).await {
                    Ok(()) => info!(file = %file_name, "DingTalk file message sent (sessionWebhook may not display file/image per DingTalk docs)"),
                    Err(e) => warn!(error = %e, file = %file_name, "DingTalk sessionWebhook file send failed"),
                }
            }
            // Per DingTalk: "Webhook 支持简单的文本、Markdown类型消息发送" — sessionWebhook does NOT
            // support file/image delivery; POST may return 200 but the client does not show them.
            // Send a short note so the user knows why the attachment might not appear.
            let note = "若未收到附件：钉钉会话 Webhook 仅支持文本/Markdown，无法直接推送文件，文件已保存在服务端工作区。";
            let _ = post_session_text_http(&http, &webhook, note).await;
        }
    }
}

async fn dingtalk_media_upload(
    http: &reqwest::Client,
    access_token: &str,
    typ: &str,
    file_name: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let url = match url::Url::parse("https://oapi.dingtalk.com/media/upload") {
        Ok(mut u) => {
            u.query_pairs_mut()
                .append_pair("access_token", access_token)
                .append_pair("type", typ);
            u.to_string()
        }
        Err(_) => format!(
            "https://oapi.dingtalk.com/media/upload?access_token={}&type={}",
            access_token, typ
        ),
    };
    let part = reqwest::multipart::Part::bytes(bytes.to_vec())
        .file_name(file_name.to_string())
        .mime_str("application/octet-stream")
        .map_err(|e| e.to_string())?;
    let form = reqwest::multipart::Form::new().part("media", part);
    let resp = http
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let txt = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("media/upload {status}: {txt}"));
    }
    let v: serde_json::Value =
        serde_json::from_str(&txt).map_err(|e| format!("media JSON: {e}"))?;
    let err = v.get("errcode").and_then(|x| x.as_i64()).unwrap_or(0);
    if err != 0 {
        return Err(v.get("errmsg").and_then(|x| x.as_str()).unwrap_or(&txt).to_string());
    }
    v.get("media_id")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("no media_id: {txt}"))
}

async fn post_session_text_http(
    http: &reqwest::Client,
    webhook: &str,
    content: &str,
) -> Result<(), String> {
    let body = serde_json::json!({
        "msgtype": "text",
        "text": { "content": content }
    });
    post_session_json(http, webhook, &body).await
}

async fn post_session_json(
    http: &reqwest::Client,
    webhook: &str,
    body: &serde_json::Value,
) -> Result<(), String> {
    let resp = http
        .post(webhook)
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        let status = resp.status();
        let t = resp.text().await.unwrap_or_default();
        return Err(format!("sessionWebhook POST {status}: {t}"));
    }
    Ok(())
}

fn split_chunks(s: &str, max: usize) -> Vec<String> {
    if s.is_empty() {
        return vec![];
    }
    if s.len() <= max {
        return vec![s.to_string()];
    }
    let mut out = Vec::new();
    let mut rest = s;
    while !rest.is_empty() {
        if rest.len() <= max {
            out.push(rest.to_string());
            break;
        }
        let take = rest
            .char_indices()
            .take_while(|(i, _)| *i < max)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(max);
        out.push(rest[..take].to_string());
        rest = &rest[take..];
    }
    out
}
