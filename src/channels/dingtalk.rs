//! DingTalk channel — Stream mode (self-implemented protocol).
//!
//! Receives robot messages via CALLBACK; replies via sessionWebhook POST.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::warn;

use crate::bus::{InboundMessage, OutboundMessage, OutboundMessageType};
use crate::channels::approval_formatter;
use crate::channels::dingtalk_stream;
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
    config: DingTalkConfig,
    show_tool_calls: bool,
    tool_result_preview_chars: usize,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    /// conversationId -> session webhook for reply
    sessions: Arc<RwLock<HashMap<String, SessionEntry>>>,
    http: reqwest::Client,
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
    ) -> Self {
        Self {
            config,
            show_tool_calls,
            tool_result_preview_chars,
            inbound_tx,
            outbound_rx: Some(outbound_rx),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            http: reqwest::Client::new(),
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

        let channel_name_out = channel_name.clone();
        tokio::spawn(async move {
            run_outbound_dingtalk(
                channel_name_out,
                outbound_rx,
                sessions_out,
                http_out,
                show_tool_calls,
                tool_preview,
            )
            .await;
        });

        let http_loop = self.http.clone();
        // Block here forever; run_forever internally reconnects.
        let default_agent = self.config.default_agent.clone();
        dingtalk_stream::run_forever(http_loop, client_id, client_secret, move |data_str| {
                let inbound_tx = inbound_tx.clone();
                let sessions = Arc::clone(&sessions);
                let channel_name = channel_name.clone();
                let default_agent = default_agent.clone();
                let allowlist_enforce = allowlist_enforce;
                let allowlist = allowlist.clone();
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
                    let text = DingTalkChannel::extract_text(&data);
                    if text.is_empty() {
                        return;
                    }
                    if let Some(ref wh) = data.session_webhook {
                        let entry = SessionEntry {
                            webhook: wh.clone(),
                            expires_ms: data.session_webhook_expired_time,
                        };
                        sessions.write().await.insert(conversation_id.clone(), entry);
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

async fn run_outbound_dingtalk(
    channel_name: String,
    mut outbound_rx: broadcast::Receiver<OutboundMessage>,
    sessions: Arc<RwLock<HashMap<String, SessionEntry>>>,
    http: reqwest::Client,
    show_tool_calls: bool,
    tool_preview: usize,
) {
        while let Ok(msg) = outbound_rx.recv().await {
            if msg.channel != channel_name {
                continue;
            }
            let (content, _media) = match &msg.message_type {
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
            if content.is_empty() {
                continue;
            }
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
            // DingTalk text length limit — split if needed
            const CHUNK: usize = 1800;
            for chunk in split_chunks(&content, CHUNK) {
                if let Err(e) = post_session_text_http(&http, &webhook, &chunk).await {
                    warn!(error = %e, "DingTalk sessionWebhook send failed");
                }
            }
        }
}

async fn post_session_text_http(http: &reqwest::Client, webhook: &str, content: &str) -> Result<(), String> {
    let body = serde_json::json!({
        "msgtype": "text",
        "text": { "content": content }
    });
    let resp = http
        .post(webhook)
        .header("Content-Type", "application/json")
        .json(&body)
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
