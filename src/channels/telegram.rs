//! Telegram channel — long-polling based.
//!
//! Uses the Telegram Bot API directly via reqwest (no heavy SDK dependency).
//! Integrates `RetryPolicy` / `RetryState` for resilient polling with
//! exponential backoff on transient errors and immediate abort + system
//! notification on unrecoverable errors (e.g. 401/403).

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{error, info, warn};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::{approval_formatter, Channel, RetryPolicy, RetryState};
use crate::config::{AllowlistEntry, TelegramConfig};
use crate::tools::approval::ApprovalManager;

const API_BASE: &str = "https://api.telegram.org/bot";

pub struct TelegramChannel {
    config: TelegramConfig,
    /// When true, forward tool execution progress to this channel (global && channel show_tool_calls).
    show_tool_calls: bool,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    client: reqwest::Client,
    running: bool,
    approval_manager: Option<Arc<ApprovalManager>>,
    /// Map of user's pending approval requests: user_id -> (request_id, chat_id)
    pending_approvals: Arc<RwLock<HashMap<String, (String, String)>>>,
}

#[derive(Debug, Deserialize)]
struct TgResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
    error_code: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct TgUpdate {
    update_id: i64,
    message: Option<TgMessage>,
}

#[derive(Debug, Deserialize)]
struct TgMessage {
    message_id: i64,
    from: Option<TgUser>,
    chat: TgChat,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TgUser {
    id: i64,
}

#[derive(Debug, Deserialize)]
struct TgChat {
    id: i64,
    /// "private" | "group" | "supergroup" | "channel"
    #[serde(rename = "type", default)]
    type_: Option<String>,
}

/// Returns `true` if the HTTP status code indicates an unrecoverable error
/// that should not be retried (e.g. invalid credentials).
fn is_unrecoverable_status(status: reqwest::StatusCode) -> bool {
    matches!(status.as_u16(), 401 | 403)
}

impl TelegramChannel {
    pub fn new(
        config: TelegramConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: broadcast::Receiver<OutboundMessage>,
        show_tool_calls: bool,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("failed to build HTTP client");
        Self {
            config,
            show_tool_calls,
            inbound_tx,
            outbound_rx: Some(outbound_rx),
            client,
            running: false,
            approval_manager: None,
            pending_approvals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set the approval manager.
    pub fn with_approval_manager(mut self, manager: Arc<ApprovalManager>) -> Self {
        self.approval_manager = Some(manager);
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
    async fn has_pending_approval(&self, user_id: &str) -> bool {
        let pending = self.pending_approvals.read().await;
        pending.contains_key(user_id)
    }

    fn format_approval_request(request: &crate::tools::approval::ApprovalRequest) -> String {
        approval_formatter::format_approval_request(request)
    }

    fn api_url(&self, method: &str) -> String {
        format!("{}{}/{}", API_BASE, self.config.token, method)
    }

    /// Perform a single getUpdates call.
    ///
    /// Returns `Ok(updates)` on success, or an error that the caller can
    /// classify as transient vs unrecoverable.
    async fn poll_updates(&self, offset: i64) -> Result<Vec<TgUpdate>, TelegramPollError> {
        let response = self
            .client
            .get(self.api_url("getUpdates"))
            .query(&[("offset", offset), ("timeout", 30)])
            .send()
            .await
            .map_err(|e| TelegramPollError::Transient(format!("HTTP request failed: {e:#}")))?;

        let status = response.status();
        if is_unrecoverable_status(status) {
            let body = response.text().await.unwrap_or_default();
            return Err(TelegramPollError::Unrecoverable(format!(
                "HTTP {status}: {body}"
            )));
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(TelegramPollError::Transient(format!(
                "HTTP {status}: {body}"
            )));
        }

        let resp: TgResponse<Vec<TgUpdate>> = response
            .json()
            .await
            .map_err(|e| TelegramPollError::Transient(format!("JSON parse error: {e:#}")))?;

        if !resp.ok {
            let code = resp.error_code.unwrap_or(0);
            let desc = resp.description.unwrap_or_default();
            if code == 401 || code == 403 {
                return Err(TelegramPollError::Unrecoverable(format!(
                    "Telegram API error {code}: {desc}"
                )));
            }
            return Err(TelegramPollError::Transient(format!(
                "Telegram API error {code}: {desc}"
            )));
        }

        Ok(resp.result.unwrap_or_default())
    }

    async fn send_text(&self, chat_id: i64, text: &str) -> Result<()> {
        // Telegram limits messages to 4096 chars; split if needed.
        for chunk in text.as_bytes().chunks(4000) {
            let chunk_str = String::from_utf8_lossy(chunk);
            self.client
                .post(self.api_url("sendMessage"))
                .json(&serde_json::json!({
                    "chat_id": chat_id,
                    "text": chunk_str,
                    "parse_mode": "HTML"
                }))
                .send()
                .await?;
        }
        Ok(())
    }

    /// Send a system notification to the Agent via the MessageBus.
    async fn notify_system_error(&self, error_msg: &str) {
        let notification = InboundMessage {
            channel: "system".into(),
            sender_id: "telegram".into(),
            chat_id: "system".into(),
            content: format!("[Telegram] Unrecoverable error: {error_msg}"),
            timestamp: chrono::Utc::now(),
            media: vec![],
            metadata: serde_json::json!({
                "error_kind": "unrecoverable",
                "source_channel": "telegram",
            }),
        };
        if let Err(e) = self.inbound_tx.send(notification).await {
            error!("Failed to send system notification for Telegram error: {e}");
        }
    }
}

/// Internal error type to distinguish transient from unrecoverable poll errors.
#[derive(Debug)]
enum TelegramPollError {
    /// Transient error — should be retried with backoff.
    Transient(String),
    /// Unrecoverable error — should stop retrying and notify the Agent.
    Unrecoverable(String),
}

impl std::fmt::Display for TelegramPollError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transient(msg) => write!(f, "transient: {msg}"),
            Self::Unrecoverable(msg) => write!(f, "unrecoverable: {msg}"),
        }
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn start(&mut self) -> Result<()> {
        info!("Telegram channel starting (long-polling)");
        self.running = true;
        let mut offset: i64 = 0;

        let retry_policy = RetryPolicy::default();
        let mut retry_state = RetryState::new();

        // Spawn outbound dispatcher
        let mut outbound_rx = self.outbound_rx.take().unwrap();
        let client = self.client.clone();
        let token = self.config.token.clone();
        let channel_name = self.config.name.clone();
        let pending_approvals = self.pending_approvals.clone();
        let show_tool_calls = self.show_tool_calls;
        tokio::spawn(async move {
            while let Ok(msg) = outbound_rx.recv().await {
                if msg.channel != channel_name {
                    continue;
                }
                if let Ok(chat_id) = msg.chat_id.parse::<i64>() {
                    let url = format!("{}{}/sendMessage", API_BASE, token);
                    let content = match &msg.message_type {
                        crate::bus::OutboundMessageType::Chat { content, .. } => content.clone(),
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
                            } else if result_preview.len() > 100 {
                                format!("{}...", result_preview.chars().take(100).collect::<String>())
                            } else {
                                result_preview.clone()
                            };
                            if preview.is_empty() {
                                format!("🔧 {} — {}", tool_name, status)
                            } else {
                                format!("🔧 {} — {}\n{}", tool_name, status, preview)
                            }
                        }
                        crate::bus::OutboundMessageType::ApprovalRequest { request } => {
                            // Register the pending approval request
                            // Extract user_id from session_id (format: agent:role:channel:type:user_id)
                            let user_id = request.session_id.split(':').last().unwrap_or("").to_string();
                            if !user_id.is_empty() {
                                let mut pending = pending_approvals.write().await;
                                pending.insert(user_id, (request.id.clone(), msg.chat_id.clone()));
                            }
                            // Prefer Agent-generated display message for the user's language
                            request
                                .display_message
                                .as_deref()
                                .filter(|s| !s.is_empty())
                                .map(String::from)
                                .unwrap_or_else(|| Self::format_approval_request(&request))
                        }
                    };
                    for chunk in content.as_bytes().chunks(4000) {
                        let chunk_str = String::from_utf8_lossy(chunk);
                        let _ = client
                            .post(&url)
                            .json(&serde_json::json!({
                                "chat_id": chat_id,
                                "text": chunk_str,
                                "parse_mode": "HTML"
                            }))
                            .send()
                            .await;
                    }
                }
            }
        });

        // Poll loop with retry logic
        while self.running {
            match self.poll_updates(offset).await {
                Ok(updates) => {
                    // Successful poll — reset retry state if we were recovering
                    if retry_state.attempts > 0 {
                        info!(
                            attempts = retry_state.attempts,
                            "Telegram polling recovered, resetting retry state"
                        );
                        retry_state.reset();
                    }

                    for u in updates {
                        offset = u.update_id + 1;
                        if let Some(m) = u.message {
                            let sender =
                                m.from.map(|u| u.id.to_string()).unwrap_or_default();
                            if let Some(text) = m.text {
                                let chat_id_str = m.chat.id.to_string();
                                let is_group = m
                                    .chat
                                    .type_
                                    .as_deref()
                                    .map_or(false, |t| t == "group" || t == "supergroup");
                                let (trigger_agent, content, is_group_meta) = if !self.config.enable_allowlist {
                                    // Allowlist disabled: allow all; for group still check @group_my_name if set
                                    if is_group {
                                        if let Some(ref my_name) = self.config.group_my_name {
                                            let trimmed = text.trim_start();
                                            let mention = format!("@{}", my_name);
                                            let starts = trimmed.starts_with(&mention)
                                                || trimmed
                                                    .strip_prefix('@')
                                                    .map(|s| s.trim_start().starts_with(my_name))
                                                    .unwrap_or(false);
                                            if !starts {
                                                info!(
                                                    chat_id = %chat_id_str,
                                                    "Telegram: group message not @bot, saving to session only"
                                                );
                                                let _ = self.inbound_tx.send(InboundMessage {
                                                    channel: self.config.name.clone(),
                                                    sender_id: sender.clone(),
                                                    chat_id: chat_id_str.clone(),
                                                    content: text.clone(),
                                                    timestamp: chrono::Utc::now(),
                                                    media: vec![],
                                                    metadata: serde_json::json!({
                                                        "trigger_agent": false,
                                                        "group": true,
                                                    }),
                                                }).await;
                                                continue;
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
                                    let entry = self
                                        .config
                                        .allowlist
                                        .iter()
                                        .find(|e| e.chat_id == chat_id_str);
                                    match entry {
                                        None => {
                                            warn!(
                                                chat_id = %chat_id_str,
                                                "Telegram: chat not in allowlist, saving to session only"
                                            );
                                            let _ = self
                                                .send_text(m.chat.id, "未配置聊天许可，请配置。")
                                                .await;
                                            let _ = self.inbound_tx.send(InboundMessage {
                                                channel: self.config.name.clone(),
                                                sender_id: sender.clone(),
                                                chat_id: chat_id_str.clone(),
                                                content: text.clone(),
                                                timestamp: chrono::Utc::now(),
                                                media: vec![],
                                                metadata: serde_json::json!({ "trigger_agent": false }),
                                            }).await;
                                            continue;
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
                                                        chat_id = %chat_id_str,
                                                        "Telegram: group message not @bot, saving to session only"
                                                    );
                                                    let _ = self.inbound_tx.send(InboundMessage {
                                                        channel: self.config.name.clone(),
                                                        sender_id: sender.clone(),
                                                        chat_id: chat_id_str.clone(),
                                                        content: text.clone(),
                                                        timestamp: chrono::Utc::now(),
                                                        media: vec![],
                                                        metadata: serde_json::json!({
                                                            "trigger_agent": false,
                                                            "group": true,
                                                        }),
                                                    }).await;
                                                    continue;
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
                                // If user has pending approval, forward message to agent with metadata for LLM to interpret
                                if let Some((request_id, _chat_id_str)) = self.take_pending_approval(&sender).await {
                                    let mut meta = serde_json::json!({
                                        "trigger_agent": true,
                                        "pending_approval_request_id": request_id
                                    });
                                    if is_group_meta {
                                        meta["group"] = serde_json::json!(true);
                                    }
                                    let _ = self.inbound_tx.send(InboundMessage {
                                        channel: self.config.name.clone(),
                                        sender_id: sender,
                                        chat_id: m.chat.id.to_string(),
                                        content: content.clone(),
                                        timestamp: chrono::Utc::now(),
                                        media: vec![],
                                        metadata: meta,
                                    }).await;
                                    continue;
                                }
                                // Normal message
                                let mut meta = serde_json::json!({ "trigger_agent": true });
                                if is_group_meta {
                                    meta["group"] = serde_json::json!(true);
                                }
                                let _ = self.inbound_tx.send(InboundMessage {
                                    channel: self.config.name.clone(),
                                    sender_id: sender,
                                    chat_id: m.chat.id.to_string(),
                                    content,
                                    timestamp: chrono::Utc::now(),
                                    media: vec![],
                                    metadata: meta,
                                }).await;
                            }
                        }
                    }
                }
                Err(TelegramPollError::Unrecoverable(msg)) => {
                    error!(
                        error = %msg,
                        "Telegram encountered unrecoverable error, stopping channel"
                    );
                    self.notify_system_error(&msg).await;
                    self.running = false;
                    return Err(anyhow::anyhow!(
                        "Telegram channel stopped: unrecoverable error: {msg}"
                    ));
                }
                Err(TelegramPollError::Transient(msg)) => {
                    let should_retry =
                        retry_state.record_failure(&retry_policy, msg.clone());

                    if should_retry {
                        let delay = retry_state.next_delay(&retry_policy);
                        warn!(
                            error = %msg,
                            attempt = retry_state.attempts,
                            max_retries = retry_policy.max_retries,
                            delay_ms = delay.as_millis() as u64,
                            "Telegram poll error, retrying after backoff"
                        );
                        tokio::time::sleep(delay).await;
                    } else {
                        // Retries exhausted — enter cooldown
                        error!(
                            error = %msg,
                            attempts = retry_state.attempts,
                            "Telegram retries exhausted, entering cooldown"
                        );

                        // Cooldown period: wait max_delay then reset and try again
                        let cooldown = retry_policy.max_delay;
                        warn!(
                            cooldown_secs = cooldown.as_secs(),
                            "Telegram entering cooldown before reconnection attempt"
                        );
                        tokio::time::sleep(cooldown).await;

                        // Reset state and resume polling
                        retry_state.reset();
                        info!("Telegram cooldown complete, resuming polling");
                    }
                }
            }
        }
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        self.running = false;
        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> Result<()> {
        let chat_id: i64 = msg.chat_id.parse()?;
        let content = match &msg.message_type {
            crate::bus::OutboundMessageType::Chat { content, .. } => content.clone(),
            crate::bus::OutboundMessageType::ApprovalRequest { request } => request
                .display_message
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(String::from)
                .unwrap_or_else(|| approval_formatter::format_approval_request(request)),
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
                } else if result_preview.len() > 100 {
                    format!("{}...", result_preview.chars().take(100).collect::<String>())
                } else {
                    result_preview.clone()
                };
                if preview.is_empty() {
                    format!("🔧 {} — {}", tool_name, status)
                } else {
                    format!("🔧 {} — {}\n{}", tool_name, status, preview)
                }
            }
        };
        self.send_text(chat_id, &content).await
    }
}
