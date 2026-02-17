//! Discord channel — WebSocket-based integration via Discord Gateway.
//!
//! Uses `tokio-tungstenite` to maintain a persistent WebSocket connection
//! to the Discord Gateway. Implements the Gateway protocol including
//! Identify, Heartbeat, and Resume events.
//!
//! Integrates `RetryPolicy` / `RetryState` for resilient reconnection
//! with exponential backoff on transient errors and immediate abort +
//! system notification on unrecoverable errors (e.g. invalid token).
//!
//! Messages exceeding Discord's 2000-character limit are automatically
//! split into sequential messages.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{error, info, warn};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::{approval_parser, Channel, RetryPolicy, RetryState};
use crate::config::{AllowlistEntry, DiscordConfig};
use crate::tools::approval::{ApprovalManager, ApprovalResponse};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Discord Gateway URL (v10).
const GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";
/// Discord REST API base URL.
const API_BASE: &str = "https://discord.com/api/v10";
/// Maximum message length allowed by Discord.
const DISCORD_MAX_MESSAGE_LEN: usize = 2000;

/// Gateway opcodes.
mod opcode {
    pub const DISPATCH: u64 = 0;
    pub const HEARTBEAT: u64 = 1;
    pub const IDENTIFY: u64 = 2;
    pub const RESUME: u64 = 6;
    pub const RECONNECT: u64 = 7;
    pub const INVALID_SESSION: u64 = 9;
    pub const HELLO: u64 = 10;
    pub const HEARTBEAT_ACK: u64 = 11;
}

/// Gateway intents: GUILDS (1<<0) | GUILD_MESSAGES (1<<9) | DIRECT_MESSAGES (1<<12) | MESSAGE_CONTENT (1<<15).
const GATEWAY_INTENTS: u64 = (1 << 0) | (1 << 9) | (1 << 12) | (1 << 15);

// ---------------------------------------------------------------------------
// Error classification
// ---------------------------------------------------------------------------

/// Internal error type to distinguish transient from unrecoverable errors.
#[derive(Debug)]
enum DiscordGatewayError {
    Transient(String),
    Unrecoverable(String),
    ReconnectRequested,
    InvalidSession(bool),
}

impl std::fmt::Display for DiscordGatewayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transient(msg) => write!(f, "transient: {msg}"),
            Self::Unrecoverable(msg) => write!(f, "unrecoverable: {msg}"),
            Self::ReconnectRequested => write!(f, "reconnect requested by server"),
            Self::InvalidSession(r) => write!(f, "invalid session (resumable={r})"),
        }
    }
}

fn classify_discord_error(error_msg: &str) -> DiscordGatewayError {
    let lower = error_msg.to_lowercase();
    if lower.contains("401")
        || lower.contains("403")
        || lower.contains("authentication")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("invalid token")
        || lower.contains("4004")
    {
        DiscordGatewayError::Unrecoverable(error_msg.to_string())
    } else {
        DiscordGatewayError::Transient(error_msg.to_string())
    }
}

// ---------------------------------------------------------------------------
// Resume state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct ResumeState {
    session_id: Option<String>,
    resume_gateway_url: Option<String>,
    sequence: Option<u64>,
}

// ---------------------------------------------------------------------------
// Message splitting
// ---------------------------------------------------------------------------

/// Split a message into chunks of at most `max_len` characters.
///
/// Splits on the last newline before the limit when possible, otherwise
/// splits at exactly `max_len` characters. The concatenation of all
/// returned chunks equals the original string.
pub fn split_message(content: &str, max_len: usize) -> Vec<String> {
    if max_len == 0 {
        return vec![content.to_string()];
    }
    if content.len() <= max_len {
        return vec![content.to_string()];
    }
    let mut chunks = Vec::new();
    let mut remaining = content;
    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }
        let split_at = remaining[..max_len]
            .rfind('\n')
            .map(|pos| pos + 1)
            .unwrap_or(max_len);
        chunks.push(remaining[..split_at].to_string());
        remaining = &remaining[split_at..];
    }
    chunks
}

// ---------------------------------------------------------------------------
// Discord message conversion
// ---------------------------------------------------------------------------

/// Convert a Discord MESSAGE_CREATE event payload into an InboundMessage.
///
/// Returns `None` if the message is from a bot, has no text content,
/// or required fields are missing. Allowlist is checked by the caller.
fn discord_event_to_inbound(data: &serde_json::Value) -> Option<InboundMessage> {
    let author = match data.get("author") {
        Some(a) => a,
        None => {
            warn!("Discord MESSAGE_CREATE: missing 'author' field, ignoring");
            return None;
        }
    };
    // Ignore bot messages
    if author.get("bot").and_then(|b| b.as_bool()).unwrap_or(false) {
        info!("Discord: Ignoring bot message");
        return None;
    }
    let sender_id = match author.get("id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            warn!("Discord MESSAGE_CREATE: missing author.id, ignoring");
            return None;
        }
    };
    let chat_id = match data.get("channel_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => {
            warn!("Discord MESSAGE_CREATE: missing 'channel_id', ignoring");
            return None;
        }
    };
    let content = match data.get("content").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            warn!("Discord MESSAGE_CREATE: missing 'content' field (enable Message Content Intent in Developer Portal if needed), ignoring");
            return None;
        }
    };

    if content.is_empty() {
        info!("Discord: Ignoring empty content");
        return None;
    }

    let message_id = data
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let guild_id = data
        .get("guild_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    info!(
        sender_id = %sender_id,
        chat_id = %chat_id,
        content_len = content.len(),
        "Discord: Converting event to inbound message"
    );

    Some(InboundMessage {
        channel: "discord".into(),
        sender_id,
        chat_id,
        content,
        timestamp: chrono::Utc::now(),
        media: vec![],
        metadata: serde_json::json!({
            "message_id": message_id,
            "guild_id": guild_id,
        }),
    })
}

// ---------------------------------------------------------------------------
// DiscordChannel
// ---------------------------------------------------------------------------

pub struct DiscordChannel {
    config: DiscordConfig,
    /// When true, forward tool execution progress to this channel (global && channel show_tool_calls).
    show_tool_calls: bool,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    client: reqwest::Client,
    running: bool,
    approval_manager: Option<Arc<ApprovalManager>>,
    /// 用户待处理审批请求映射：user_id -> (request_id, chat_id)
    pending_approvals: Arc<RwLock<HashMap<String, (String, String)>>>,
}

impl DiscordChannel {
    pub fn new(
        config: DiscordConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: broadcast::Receiver<OutboundMessage>,
        show_tool_calls: bool,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
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

    /// 设置审批管理器
    pub fn with_approval_manager(mut self, manager: Arc<ApprovalManager>) -> Self {
        self.approval_manager = Some(manager);
        self
    }

    /// 记录用户的待处理审批请求
    async fn register_pending_approval(&self, user_id: String, request_id: String, chat_id: String) {
        let mut pending = self.pending_approvals.write().await;
        pending.insert(user_id, (request_id, chat_id));
    }

    /// 获取并移除用户的待处理审批请求
    async fn take_pending_approval(&self, user_id: &str) -> Option<(String, String)> {
        let mut pending = self.pending_approvals.write().await;
        pending.remove(user_id)
    }

    /// 检查用户是否有待处理的审批请求
    #[allow(dead_code)]
    async fn has_pending_approval(&self, user_id: &str) -> bool {
        let pending = self.pending_approvals.read().await;
        pending.contains_key(user_id)
    }

    /// 格式化审批请求消息
    fn format_approval_request(request: &crate::tools::approval::ApprovalRequest) -> String {
        format!(
            "🔐 **命令执行审批请求**\n\n\
            **命令：**`{}`\n\
            **工作目录：**`{}`\n\
            **上下文：**{}\n\
            **请求时间：**{}\n\n\
            请回复以下关键词进行审批：\n\
            • 同意 / 批准 / yes / y - 批准执行\n\
            • 拒绝 / 不同意 / no / n - 拒绝执行\n\n\
            ⏱️ 请求将在 {} 秒后超时",
            request.command,
            request.working_dir,
            request.context,
            request.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            request.timeout_secs
        )
    }

    /// 发送审批结果反馈消息
    async fn send_approval_feedback(
        client: &reqwest::Client,
        token: &str,
        channel_id: &str,
        approved: bool,
    ) -> Result<()> {
        let feedback = if approved {
            "✅ **审批已通过**\n\n命令将继续执行。"
        } else {
            "🚫 **审批已拒绝**\n\n命令执行已取消。"
        };
        
        let url = format!("{}/channels/{}/messages", API_BASE, channel_id);
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bot {}", token))
            .json(&serde_json::json!({ "content": feedback }))
            .send()
            .await?;
            
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            error!(
                channel_id = %channel_id,
                status = %status,
                "Discord send_approval_feedback failed: {body}"
            );
            anyhow::bail!("Discord send failed: HTTP {status}: {body}");
        }
        
        Ok(())
    }

    /// Send a plain text message to a channel (used e.g. for allowlist denial reply).
    async fn send_text_to_channel(
        client: &reqwest::Client,
        token: &str,
        channel_id: &str,
        content: &str,
    ) -> Result<()> {
        let url = format!("{}/channels/{}/messages", API_BASE, channel_id);
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bot {}", token))
            .json(&serde_json::json!({ "content": content }))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            error!(
                channel_id = %channel_id,
                status = %status,
                "Discord send_text_to_channel failed: {body}"
            );
            anyhow::bail!("Discord send failed: HTTP {status}: {body}");
        }
        Ok(())
    }

    /// Send a system notification to the Agent via the MessageBus.
    async fn notify_system_error(&self, error_msg: &str) {
        let notification = InboundMessage {
            channel: "system".into(),
            sender_id: "discord".into(),
            chat_id: "system".into(),
            content: format!("[Discord] Unrecoverable error: {error_msg}"),
            timestamp: chrono::Utc::now(),
            media: vec![],
            metadata: serde_json::json!({
                "error_kind": "unrecoverable",
                "source_channel": "discord",
            }),
        };
        if let Err(e) = self.inbound_tx.send(notification).await {
            error!("Failed to send system notification for Discord error: {e}");
        }
    }

    /// Send a text message to a Discord channel via the REST API.
    /// Automatically splits messages exceeding 2000 characters.
    async fn send_message(&self, channel_id: &str, content: &str) -> Result<()> {
        let chunks = split_message(content, DISCORD_MAX_MESSAGE_LEN);
        for chunk in &chunks {
            let url = format!("{}/channels/{}/messages", API_BASE, channel_id);
            let resp = self
                .client
                .post(&url)
                .header("Authorization", format!("Bot {}", self.config.token))
                .json(&serde_json::json!({ "content": chunk }))
                .send()
                .await?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                error!(
                    channel_id = %channel_id,
                    status = %status,
                    "Discord send_message failed: {body}"
                );
                anyhow::bail!("Discord send failed: HTTP {status}: {body}");
            }
        }
        Ok(())
    }

    /// Run a single Gateway session. Connects, identifies (or resumes),
    /// processes events, and returns when the connection drops or the
    /// server requests a reconnect.
    async fn run_gateway_session(
        token: &str,
        inbound_tx: &mpsc::Sender<InboundMessage>,
        allowlist: &[AllowlistEntry],
        channel_name: &str,
        enable_allowlist: bool,
        group_my_name: Option<&str>,
        resume: &mut ResumeState,
        approval_manager: &Option<Arc<ApprovalManager>>,
        pending_approvals: &Arc<RwLock<HashMap<String, (String, String)>>>,
        client: &reqwest::Client,
    ) -> std::result::Result<(), DiscordGatewayError> {
        // Choose URL: use resume_gateway_url if we have one, else default.
        let ws_url = resume
            .resume_gateway_url
            .as_deref()
            .unwrap_or(GATEWAY_URL);

        info!(url = %ws_url, "Discord Gateway connecting...");

        let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url)
            .await
            .map_err(|e| {
                classify_discord_error(&format!("WebSocket connect failed: {e}"))
            })?;

        let (mut ws_tx, mut ws_rx) = ws_stream.split();

        // --- Wait for Hello (opcode 10) ---
        let hello_msg = ws_rx
            .next()
            .await
            .ok_or_else(|| {
                DiscordGatewayError::Transient("Connection closed before Hello".into())
            })?
            .map_err(|e| {
                DiscordGatewayError::Transient(format!("WS read error: {e}"))
            })?;

        let hello: serde_json::Value =
            serde_json::from_str(&hello_msg.to_text().unwrap_or("{}"))
                .unwrap_or_default();

        let heartbeat_interval_ms = hello
            .get("d")
            .and_then(|d| d.get("heartbeat_interval"))
            .and_then(|v| v.as_u64())
            .unwrap_or(41250);

        info!(
            heartbeat_interval_ms,
            "Discord Gateway Hello received"
        );

        // --- Send Identify or Resume ---
        if let (Some(session_id), Some(seq)) =
            (&resume.session_id, resume.sequence)
        {
            // Attempt to resume the previous session.
            let resume_payload = serde_json::json!({
                "op": opcode::RESUME,
                "d": {
                    "token": token,
                    "session_id": session_id,
                    "seq": seq,
                }
            });
            info!(session_id = %session_id, seq, "Discord sending Resume");
            ws_tx
                .send(tokio_tungstenite::tungstenite::Message::Text(
                    resume_payload.to_string().into(),
                ))
                .await
                .map_err(|e| {
                    DiscordGatewayError::Transient(format!("Failed to send Resume: {e}"))
                })?;
        } else {
            // Fresh connection — send Identify.
            let identify_payload = serde_json::json!({
                "op": opcode::IDENTIFY,
                "d": {
                    "token": token,
                    "intents": GATEWAY_INTENTS,
                    "properties": {
                        "os": std::env::consts::OS,
                        "browser": "synbot",
                        "device": "synbot",
                    }
                }
            });
            info!("Discord sending Identify");
            ws_tx
                .send(tokio_tungstenite::tungstenite::Message::Text(
                    identify_payload.to_string().into(),
                ))
                .await
                .map_err(|e| {
                    DiscordGatewayError::Transient(format!("Failed to send Identify: {e}"))
                })?;
        }

        // --- Heartbeat + event loop ---
        let mut heartbeat_interval =
            tokio::time::interval(Duration::from_millis(heartbeat_interval_ms));
        // The first tick completes immediately; skip it so we don't send
        // a heartbeat before the gateway is ready.
        heartbeat_interval.tick().await;

        loop {
            tokio::select! {
                _ = heartbeat_interval.tick() => {
                    let hb = serde_json::json!({
                        "op": opcode::HEARTBEAT,
                        "d": resume.sequence,
                    });
                    if let Err(e) = ws_tx
                        .send(tokio_tungstenite::tungstenite::Message::Text(
                            hb.to_string().into(),
                        ))
                        .await
                    {
                        return Err(DiscordGatewayError::Transient(
                            format!("Heartbeat send failed: {e}"),
                        ));
                    }
                }
                msg = ws_rx.next() => {
                    let msg = match msg {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => {
                            return Err(DiscordGatewayError::Transient(
                                format!("WS read error: {e}"),
                            ));
                        }
                        None => {
                            // Stream ended — connection closed.
                            return Err(DiscordGatewayError::Transient(
                                "WebSocket connection closed".into(),
                            ));
                        }
                    };

                    // Only process text frames.
                    let text = match msg.into_text() {
                        Ok(t) => t,
                        Err(_) => continue,
                    };

                    let payload: serde_json::Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let op = payload.get("op").and_then(|v| v.as_u64()).unwrap_or(u64::MAX);
                    let seq = payload.get("s").and_then(|v| v.as_u64());
                    if let Some(s) = seq {
                        resume.sequence = Some(s);
                    }

                    match op {
                        opcode::DISPATCH => {
                            let event_name = payload
                                .get("t")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");

                            tracing::debug!(event = %event_name, "Discord Gateway DISPATCH");

                            match event_name {
                                "READY" => {
                                    if let Some(d) = payload.get("d") {
                                        resume.session_id = d
                                            .get("session_id")
                                            .and_then(|v| v.as_str())
                                            .map(String::from);
                                        resume.resume_gateway_url = d
                                            .get("resume_gateway_url")
                                            .and_then(|v| v.as_str())
                                            .map(String::from);
                                        info!(
                                            session_id = ?resume.session_id,
                                            "Discord Gateway READY"
                                        );
                                    }
                                }
                                "RESUMED" => {
                                    info!("Discord Gateway session resumed");
                                }
                                "MESSAGE_CREATE" => {
                                    if let Some(d) = payload.get("d") {
                                        info!("Discord MESSAGE_CREATE event received");
                                        if let Some(mut inbound) = discord_event_to_inbound(d) {
                                            inbound.channel = channel_name.to_string();
                                            let is_group = !inbound
                                                .metadata
                                                .get("guild_id")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("")
                                                .is_empty();
                                            let (trigger_agent, skip_send) = if !enable_allowlist {
                                                if is_group {
                                                    if let Some(my_name) = group_my_name {
                                                        let trimmed = inbound.content.trim_start();
                                                        let mention_prefix = format!("<@!{}>", my_name);
                                                        let mention_prefix_alt = format!("<@{}>", my_name);
                                                        let starts = trimmed.starts_with(&mention_prefix)
                                                            || trimmed.starts_with(&mention_prefix_alt);
                                                        if !starts {
                                                            info!(
                                                                chat_id = %inbound.chat_id,
                                                                "Discord: group message not @bot, saving to session only"
                                                            );
                                                            inbound.metadata["trigger_agent"] =
                                                                serde_json::json!(false);
                                                            inbound.metadata["group"] =
                                                                serde_json::json!(true);
                                                            (false, true)
                                                        } else {
                                                            let stripped = trimmed
                                                                .strip_prefix(&mention_prefix)
                                                                .or_else(|| trimmed.strip_prefix(&mention_prefix_alt))
                                                                .unwrap_or(trimmed)
                                                                .trim_start();
                                                            inbound.content = stripped.to_string();
                                                            inbound.metadata["group"] =
                                                                serde_json::json!(true);
                                                            (true, false)
                                                        }
                                                    } else {
                                                        inbound.metadata["group"] =
                                                            serde_json::json!(true);
                                                        (true, false)
                                                    }
                                                } else {
                                                    (true, false)
                                                }
                                            } else {
                                                let entry = allowlist
                                                    .iter()
                                                    .find(|e| e.chat_id == inbound.chat_id);
                                                match entry {
                                                    None => {
                                                        warn!(
                                                            chat_id = %inbound.chat_id,
                                                            "Discord: chat not in allowlist, saving to session only"
                                                        );
                                                        let _ = Self::send_text_to_channel(
                                                            client,
                                                            token,
                                                            &inbound.chat_id,
                                                            "未配置聊天许可，请配置。",
                                                        )
                                                        .await;
                                                        inbound.metadata["trigger_agent"] =
                                                            serde_json::json!(false);
                                                        (false, true)
                                                    }
                                                    Some(e) => {
                                                        if let Some(ref my_name) = e.my_name {
                                                            let trimmed = inbound.content.trim_start();
                                                            let mention_prefix = format!("<@!{}>", my_name);
                                                            let mention_prefix_alt = format!("<@{}>", my_name);
                                                            let starts = trimmed.starts_with(&mention_prefix)
                                                                || trimmed.starts_with(&mention_prefix_alt);
                                                            if !starts {
                                                                info!(
                                                                    chat_id = %inbound.chat_id,
                                                                    "Discord: group message not @bot, saving to session only"
                                                                );
                                                                inbound.metadata["trigger_agent"] =
                                                                    serde_json::json!(false);
                                                                inbound.metadata["group"] =
                                                                    serde_json::json!(true);
                                                                (false, true)
                                                            } else {
                                                                let stripped = trimmed
                                                                    .strip_prefix(&mention_prefix)
                                                                    .or_else(|| trimmed.strip_prefix(&mention_prefix_alt))
                                                                    .unwrap_or(trimmed)
                                                                    .trim_start();
                                                                inbound.content = stripped.to_string();
                                                                inbound.metadata["group"] =
                                                                    serde_json::json!(true);
                                                                (true, false)
                                                            }
                                                        } else {
                                                            (true, false)
                                                        }
                                                    }
                                                }
                                            };
                                            if skip_send {
                                                let _ = inbound_tx.send(inbound).await;
                                                continue;
                                            }
                                            // 检查是否为审批响应
                                            if let Some(approved) = approval_parser::is_approval_response(&inbound.content) {
                                                // 检查用户是否有待处理的审批请求
                                                let mut pending = pending_approvals.write().await;
                                                if let Some((request_id, chat_id_str)) = pending.remove(&inbound.sender_id) {
                                                    if let Some(ref manager) = approval_manager {
                                                        let response = ApprovalResponse {
                                                            request_id: request_id.clone(),
                                                            approved,
                                                            responder: inbound.sender_id.clone(),
                                                            timestamp: chrono::Utc::now(),
                                                        };
                                                        
                                                        if let Err(e) = manager.submit_response(response).await {
                                                            error!("Failed to submit approval response: {}", e);
                                                            // 发送错误反馈
                                                            let _ = Self::send_approval_feedback(
                                                                client,
                                                                token,
                                                                &chat_id_str,
                                                                false,
                                                            ).await;
                                                        } else {
                                                            info!(
                                                                user_id = %inbound.sender_id,
                                                                request_id = %request_id,
                                                                approved = approved,
                                                                "Discord approval response submitted"
                                                            );
                                                            
                                                            // 发送成功反馈
                                                            let _ = Self::send_approval_feedback(
                                                                client,
                                                                token,
                                                                &chat_id_str,
                                                                approved,
                                                            ).await;
                                                        }
                                                    }
                                                    drop(pending); // 释放锁
                                                    continue; // 不将审批响应作为普通消息发送
                                                }
                                                drop(pending); // 释放锁
                                            }
                                            
                                            // 普通消息处理
                                            info!(
                                                sender = %inbound.sender_id,
                                                chat_id = %inbound.chat_id,
                                                "Discord message received"
                                            );
                                            if let Err(e) =
                                                inbound_tx.send(inbound).await
                                            {
                                                error!(
                                                    "Failed to forward Discord message: {e}"
                                                );
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    // Ignore other dispatch events.
                                }
                            }
                        }
                        opcode::HEARTBEAT => {
                            // Server requests an immediate heartbeat.
                            let hb = serde_json::json!({
                                "op": opcode::HEARTBEAT,
                                "d": resume.sequence,
                            });
                            let _ = ws_tx
                                .send(tokio_tungstenite::tungstenite::Message::Text(
                                    hb.to_string().into(),
                                ))
                                .await;
                        }
                        opcode::HEARTBEAT_ACK => {
                            // Heartbeat acknowledged — nothing to do.
                        }
                        opcode::RECONNECT => {
                            info!("Discord Gateway requested reconnect");
                            return Err(DiscordGatewayError::ReconnectRequested);
                        }
                        opcode::INVALID_SESSION => {
                            let resumable = payload
                                .get("d")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            warn!(resumable, "Discord Gateway invalid session");
                            return Err(DiscordGatewayError::InvalidSession(resumable));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Channel trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Channel for DiscordChannel {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn start(&mut self) -> Result<()> {
        info!("Discord channel starting (WebSocket Gateway)");
        self.running = true;

        // --- Spawn outbound message dispatcher ---
        let mut outbound_rx = self.outbound_rx.take().unwrap();
        let outbound_client = self.client.clone();
        let outbound_token = self.config.token.clone();
        let outbound_channel_name = self.config.name.clone();
        let pending_approvals_clone = self.pending_approvals.clone();
        let show_tool_calls = self.show_tool_calls;
        tokio::spawn(async move {
            while let Ok(msg) = outbound_rx.recv().await {
                if msg.channel != outbound_channel_name {
                    continue;
                }
                let content = match msg.message_type {
                    crate::bus::OutboundMessageType::Chat { content, .. } => content,
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
                            result_preview
                        };
                        if preview.is_empty() {
                            format!("🔧 {} — {}", tool_name, status)
                        } else {
                            format!("🔧 {} — {}\n{}", tool_name, status, preview)
                        }
                    }
                    crate::bus::OutboundMessageType::ApprovalRequest { request } => {
                        // 注册待处理的审批请求
                        // 从 session_id 中提取 user_id (格式: agent:role:channel:type:user_id)
                        let user_id = request.session_id.split(':').last().unwrap_or("").to_string();
                        if !user_id.is_empty() {
                            let mut pending = pending_approvals_clone.write().await;
                            pending.insert(user_id, (request.id.clone(), msg.chat_id.clone()));
                        }
                        
                        format!(
                            "🔐 **命令执行审批请求**\n\n\
                            **命令：**`{}`\n\
                            **工作目录：**`{}`\n\
                            **上下文：**{}\n\
                            **请求时间：**{}\n\n\
                            请回复以下关键词进行审批：\n\
                            • 同意 / 批准 / yes / y - 批准执行\n\
                            • 拒绝 / 不同意 / no / n - 拒绝执行\n\n\
                            ⏱️ 请求将在 {} 秒后超时",
                            request.command,
                            request.working_dir,
                            request.context,
                            request.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
                            request.timeout_secs
                        )
                    }
                };
                let chunks = split_message(&content, DISCORD_MAX_MESSAGE_LEN);
                for chunk in &chunks {
                    let url = format!(
                        "{}/channels/{}/messages",
                        API_BASE, msg.chat_id
                    );
                    let resp = outbound_client
                        .post(&url)
                        .header(
                            "Authorization",
                            format!("Bot {}", outbound_token),
                        )
                        .json(&serde_json::json!({ "content": chunk }))
                        .send()
                        .await;
                    if let Err(e) = resp {
                        error!("Discord outbound send error: {e:#}");
                    }
                }
            }
        });

        // --- Gateway connection loop with retry logic ---
        let retry_policy = RetryPolicy::default();
        let mut retry_state = RetryState::new();
        let mut resume = ResumeState::default();

        while self.running {
            let result = Self::run_gateway_session(
                &self.config.token,
                &self.inbound_tx,
                &self.config.allowlist,
                &self.config.name,
                self.config.enable_allowlist,
                self.config.group_my_name.as_deref(),
                &mut resume,
                &self.approval_manager,
                &self.pending_approvals,
                &self.client,
            )
            .await;

            match result {
                Ok(()) => {
                    // Should not normally return Ok — the loop runs until error.
                    if retry_state.attempts > 0 {
                        retry_state.reset();
                    }
                    info!("Discord Gateway session ended normally, reconnecting...");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(DiscordGatewayError::ReconnectRequested) => {
                    // Server asked us to reconnect — resume immediately.
                    info!("Discord reconnecting (server requested)");
                    retry_state.reset();
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(DiscordGatewayError::InvalidSession(resumable)) => {
                    if !resumable {
                        // Clear resume state — must do a fresh Identify.
                        resume = ResumeState::default();
                    }
                    info!(
                        resumable,
                        "Discord invalid session, reconnecting..."
                    );
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                Err(DiscordGatewayError::Unrecoverable(msg)) => {
                    error!(
                        error = %msg,
                        "Discord encountered unrecoverable error, stopping"
                    );
                    self.notify_system_error(&msg).await;
                    self.running = false;
                    return Err(anyhow::anyhow!(
                        "Discord channel stopped: unrecoverable error: {msg}"
                    ));
                }
                Err(DiscordGatewayError::Transient(msg)) => {
                    let should_retry =
                        retry_state.record_failure(&retry_policy, msg.clone());

                    if should_retry {
                        let delay = retry_state.next_delay(&retry_policy);
                        warn!(
                            error = %msg,
                            attempt = retry_state.attempts,
                            max_retries = retry_policy.max_retries,
                            delay_ms = delay.as_millis() as u64,
                            "Discord Gateway error, retrying after backoff"
                        );
                        tokio::time::sleep(delay).await;
                    } else {
                        error!(
                            error = %msg,
                            attempts = retry_state.attempts,
                            "Discord retries exhausted, entering cooldown"
                        );
                        let cooldown = retry_policy.max_delay;
                        warn!(
                            cooldown_secs = cooldown.as_secs(),
                            "Discord entering cooldown"
                        );
                        tokio::time::sleep(cooldown).await;
                        retry_state.reset();
                        info!("Discord cooldown complete, resuming");
                    }
                }
            }
        }

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        info!("Discord channel stopping");
        self.running = false;
        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> Result<()> {
        let content = match &msg.message_type {
            crate::bus::OutboundMessageType::Chat { content, .. } => content.clone(),
            crate::bus::OutboundMessageType::ApprovalRequest { request } => {
                format!("🔐 命令执行审批请求\n\n命令：{}\n工作目录：{}\n上下文：{}\n\n请回复以下关键词进行审批：\n• 同意 / 批准 / yes / y - 批准执行\n• 拒绝 / 不同意 / no / n - 拒绝执行\n\n⏱️ 请求将在 {} 秒后超时", 
                    request.command, request.working_dir, request.context, request.timeout_secs)
            }
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
        self.send_message(&msg.chat_id, &content).await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- split_message tests ----

    #[test]
    fn split_message_short_returns_single_chunk() {
        let result = split_message("hello", 2000);
        assert_eq!(result, vec!["hello"]);
    }

    #[test]
    fn split_message_exact_limit_returns_single_chunk() {
        let msg = "a".repeat(2000);
        let result = split_message(&msg, 2000);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 2000);
    }

    #[test]
    fn split_message_over_limit_splits_correctly() {
        let msg = "a".repeat(4500);
        let result = split_message(&msg, 2000);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].len(), 2000);
        assert_eq!(result[1].len(), 2000);
        assert_eq!(result[2].len(), 500);
        // Concatenation equals original
        let joined: String = result.into_iter().collect();
        assert_eq!(joined, msg);
    }

    #[test]
    fn split_message_prefers_newline_boundary() {
        let mut msg = String::new();
        msg.push_str(&"a".repeat(1990));
        msg.push('\n');
        msg.push_str(&"b".repeat(100));
        let result = split_message(&msg, 2000);
        assert_eq!(result.len(), 2);
        // First chunk should end at the newline (1991 chars including \n)
        assert!(result[0].ends_with('\n'));
        assert_eq!(result[0].len(), 1991);
        assert_eq!(result[1], "b".repeat(100));
    }

    #[test]
    fn split_message_empty_string() {
        let result = split_message("", 2000);
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn split_message_concatenation_equals_original() {
        let msg = "Hello\nWorld\nThis is a test\nwith newlines\n";
        let result = split_message(msg, 10);
        let joined: String = result.into_iter().collect();
        assert_eq!(joined, msg);
    }

    #[test]
    fn split_message_max_len_zero_returns_whole() {
        let result = split_message("hello", 0);
        assert_eq!(result, vec!["hello"]);
    }

    #[test]
    fn split_message_each_chunk_within_limit() {
        let msg = "x".repeat(5001);
        let result = split_message(&msg, 2000);
        for chunk in &result {
            assert!(chunk.len() <= 2000);
        }
    }

    // ---- discord_event_to_inbound tests ----

    fn make_message_create(
        sender_id: &str,
        channel_id: &str,
        content: &str,
        is_bot: bool,
    ) -> serde_json::Value {
        serde_json::json!({
            "id": "msg-123",
            "channel_id": channel_id,
            "guild_id": "guild-456",
            "content": content,
            "author": {
                "id": sender_id,
                "username": "testuser",
                "bot": is_bot,
            }
        })
    }

    #[test]
    fn converts_valid_message() {
        let data = make_message_create("user-1", "chan-1", "hello bot", false);
        let result = discord_event_to_inbound(&data);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert_eq!(msg.channel, "discord");
        assert_eq!(msg.sender_id, "user-1");
        assert_eq!(msg.chat_id, "chan-1");
        assert_eq!(msg.content, "hello bot");
    }

    #[test]
    fn ignores_bot_messages() {
        let data = make_message_create("bot-1", "chan-1", "bot msg", true);
        let result = discord_event_to_inbound(&data);
        assert!(result.is_none());
    }

    #[test]
    fn ignores_empty_content() {
        let data = make_message_create("user-1", "chan-1", "", false);
        let result = discord_event_to_inbound(&data);
        assert!(result.is_none());
    }

    #[test]
    fn metadata_contains_message_and_guild_id() {
        let data = make_message_create("user-1", "chan-1", "hi", false);
        let msg = discord_event_to_inbound(&data).unwrap();
        assert_eq!(msg.metadata["message_id"], "msg-123");
        assert_eq!(msg.metadata["guild_id"], "guild-456");
    }

    // ---- classify_discord_error tests ----

    #[test]
    fn classifies_auth_errors_as_unrecoverable() {
        assert!(matches!(
            classify_discord_error("HTTP 401 Unauthorized"),
            DiscordGatewayError::Unrecoverable(_)
        ));
        assert!(matches!(
            classify_discord_error("close code 4004"),
            DiscordGatewayError::Unrecoverable(_)
        ));
    }

    #[test]
    fn classifies_network_errors_as_transient() {
        assert!(matches!(
            classify_discord_error("connection reset"),
            DiscordGatewayError::Transient(_)
        ));
        assert!(matches!(
            classify_discord_error("DNS resolution failed"),
            DiscordGatewayError::Transient(_)
        ));
    }
}
