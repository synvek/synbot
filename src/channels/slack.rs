//! Slack channel — Socket Mode integration via slack-morphism-rust.
//!
//! Uses Slack Socket Mode (WebSocket) to receive events without a public HTTP endpoint.
//! Sends messages via Slack Web API (chat.postMessage) with the Bot token.
//! Supports allowlist, group @-mention stripping, and tool progress forwarding.
//! Incoming file attachments are saved to workspace; outbound files are sent one per message.
//! File upload uses Slack's replacement API (files.getUploadURLExternal + files.completeUploadExternal)
//! because files.upload is deprecated and returns method_deprecated.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use slack_morphism::prelude::*;
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::file_handler;
use crate::channels::{Channel, approval_formatter};
use crate::config::{AllowlistEntry, SlackConfig};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum message length for Slack (API limit 40000; use smaller chunks for safety).
const SLACK_MAX_MESSAGE_LEN: usize = 4000;

/// Normalize Slack channel ID for API use. Slack events may give `<#C12345>` or `<#C12345|name>`;
/// chat.postMessage expects the raw ID (e.g. `C12345`).
fn slack_channel_id_raw(chat_id: &str) -> String {
    let s = chat_id.trim();
    if s.starts_with("<#") && s.len() > 2 {
        let inner = &s[2..];
        let id = inner
            .find('|')
            .map(|i| &inner[..i])
            .unwrap_or(inner)
            .trim_end_matches('>')
            .trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    s.to_string()
}

/// Upload a file to Slack using the replacement API (files.getUploadURLExternal + POST to URL + files.completeUploadExternal).
/// files.upload is deprecated and returns method_deprecated for many apps.
async fn slack_upload_file_v2(
    token: &str,
    channel_id: &str,
    filename: &str,
    file_data: Vec<u8>,
) -> Result<(), String> {
    let len = file_data.len();
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e: reqwest::Error| e.to_string())?;

    // 1) Get upload URL — Slack expects application/x-www-form-urlencoded, not JSON
    let get_url = "https://slack.com/api/files.getUploadURLExternal";
    let len_str = len.to_string();
    let params = [("filename", filename), ("length", len_str.as_str())];
    let resp = client
        .post(get_url)
        .header("Authorization", format!("Bearer {}", token))
        .form(&params)
        .send()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    let status = resp.status();
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    if !status.is_success() || json.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        let err = json.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
        return Err(format!("Slack getUploadURLExternal: {} ({})", status, err));
    }
    let upload_url = json
        .get("upload_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing upload_url in response".to_string())?;
    let file_id = json
        .get("file_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing file_id in response".to_string())?;

    // 2) POST file to upload URL (multipart with filename)
    let part = reqwest::multipart::Part::bytes(file_data).file_name(filename.to_string());
    let form = reqwest::multipart::Form::new().part("filename", part);
    let resp = client
        .post(upload_url)
        .multipart(form)
        .send()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Slack upload URL returned {}: {}", status, body));
    }

    // 3) Complete upload and share to channel
    let complete_url = "https://slack.com/api/files.completeUploadExternal";
    let body = serde_json::json!({
        "files": [{"id": file_id}],
        "channel_id": channel_id
    });
    let resp = client
        .post(complete_url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&body)
        .send()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    let status = resp.status();
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e: reqwest::Error| e.to_string())?;
    if !status.is_success() || json.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        let err = json.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
        return Err(format!("Slack completeUploadExternal: {} ({})", status, err));
    }
    Ok(())
}

/// Download a Slack file from its private download URL (requires Bearer token).
async fn slack_download_attachment(
    token: &str,
    url: &url::Url,
) -> Result<Vec<u8>, String> {
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e: reqwest::Error| e.to_string())?;
    let resp = client
        .get(url.as_str())
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await.map_err(|e: reqwest::Error| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Slack file download: HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await.map_err(|e: reqwest::Error| e.to_string())?;
    Ok(bytes.to_vec())
}

// ---------------------------------------------------------------------------
// Message splitting (reuse pattern from Discord)
// ---------------------------------------------------------------------------

fn split_message(content: &str, max_len: usize) -> Vec<String> {
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
// State passed into Socket Mode callback via user_state
// ---------------------------------------------------------------------------

/// State injected into SlackClientEventsListenerEnvironment for the push-events callback.
#[derive(Clone)]
struct SlackPushState {
    inner: Arc<SlackPushStateInner>,
}

struct SlackPushStateInner {
    inbound_tx: mpsc::Sender<InboundMessage>,
    channel_name: String,
    allowlist: Vec<AllowlistEntry>,
    enable_allowlist: bool,
    group_my_name: Option<String>,
    /// Workspace directory for saving incoming attachments; required to download message files.
    workspace_dir: Option<PathBuf>,
    /// Bot token (xoxb-...) for downloading private file URLs.
    token: String,
}

// ---------------------------------------------------------------------------
// SlackChannel
// ---------------------------------------------------------------------------

pub struct SlackChannel {
    config: SlackConfig,
    show_tool_calls: bool,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    bot_token: SlackApiToken,
    running: bool,
    /// Workspace directory for resolving outbound file paths. Slack does not support multiple files in one message; we send one file per message.
    workspace_dir: Option<PathBuf>,
}

impl SlackChannel {
    pub fn new(
        config: SlackConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: broadcast::Receiver<OutboundMessage>,
        show_tool_calls: bool,
        workspace_dir: Option<PathBuf>,
    ) -> Result<Self> {
        // Warn if tokens look swapped (common cause of not_allowed_token_type)
        let token_trim = config.token.trim();
        let app_token_trim = config.app_token.trim();
        if token_trim.starts_with("xapp-") && app_token_trim.starts_with("xoxb-") {
            warn!(
                "Slack: token and appToken appear swapped. Use token=xoxb-... (Bot) and appToken=xapp-... (App-level). Otherwise you will see 'not_allowed_token_type'."
            );
        } else if app_token_trim.starts_with("xoxb-") {
            warn!(
                "Slack: appToken looks like a Bot token (xoxb-). Socket Mode requires an App-level token (xapp-). Create one in Slack app: Settings → Basic Information → App-Level Tokens, with scope connections:write."
            );
        } else if token_trim.starts_with("xapp-") {
            warn!(
                "Slack: token looks like an App-level token (xapp-). For sending messages use the Bot User OAuth Token (xoxb-) from OAuth & Permissions."
            );
        }

        let token_value: SlackApiTokenValue = config.token.clone().into();
        let bot_token = SlackApiToken::new(token_value);
        Ok(Self {
            config,
            show_tool_calls,
            inbound_tx,
            outbound_rx: Some(outbound_rx),
            bot_token,
            running: false,
            workspace_dir,
        })
    }

    async fn send_message(&self, channel_id: &str, content: &str) -> Result<()> {
        let raw_id = slack_channel_id_raw(channel_id);
        let client = SlackClient::new(SlackClientHyperConnector::new()?);
        let session = client.open_session(&self.bot_token);
        let chunks = split_message(content, SLACK_MAX_MESSAGE_LEN);
        for chunk in &chunks {
            let req = SlackApiChatPostMessageRequest::new(
                raw_id.clone().into(),
                SlackMessageContent::new().with_text(chunk.clone().into()),
            );
            session.chat_post_message(&req).await?;
        }
        Ok(())
    }

    async fn run_socket_mode(&self) -> Result<()> {
        let client = Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));

        let push_state = SlackPushState {
            inner: Arc::new(SlackPushStateInner {
                inbound_tx: self.inbound_tx.clone(),
                channel_name: self.config.name.clone(),
                allowlist: self.config.allowlist.clone(),
                enable_allowlist: self.config.enable_allowlist,
                group_my_name: self.config.group_my_name.clone(),
                workspace_dir: self.workspace_dir.clone(),
                token: self.config.token.clone(),
            }),
        };

        let socket_mode_callbacks =
            SlackSocketModeListenerCallbacks::new().with_push_events(slack_push_events_handler);

        fn error_handler(
            err: Box<dyn std::error::Error + Send + Sync>,
            _client: Arc<SlackHyperClient>,
            _states: SlackClientEventsUserState,
        ) -> HttpStatusCode {
            tracing::error!("Slack Socket Mode error: {err:#}");
            HttpStatusCode::OK
        }

        let listener_environment = Arc::new(
            SlackClientEventsListenerEnvironment::new(client.clone())
                .with_error_handler(error_handler)
                .with_user_state(push_state),
        );

        let socket_mode_listener = SlackClientSocketModeListener::new(
            &SlackClientSocketModeConfig::new(),
            listener_environment,
            socket_mode_callbacks,
        );

        let app_token_value: SlackApiTokenValue = self.config.app_token.clone().into();
        let app_token = SlackApiToken::new(app_token_value);

        info!(
            channel = %self.config.name,
            "Slack Socket Mode connecting (app token configured)..."
        );
        info!(
            channel = %self.config.name,
            "If you see 'not_allowed_token_type': use appToken=xapp-... (App-level token) and token=xoxb-... (Bot token). Do not swap them."
        );
        if let Err(e) = socket_mode_listener.listen_for(&app_token).await {
            error!(
                channel = %self.config.name,
                error = %e,
                "Slack Socket Mode listen_for failed. If error is 'not_allowed_token_type': use appToken=xapp-... (from App-Level Tokens) and token=xoxb-... (Bot User OAuth Token). Check Socket Mode is enabled in the Slack app."
            );
            return Err(e.into());
        }
        info!(
            channel = %self.config.name,
            "Slack Socket Mode registered, starting WebSocket connections (wait a few seconds, then send a message in Slack; if no response, check Event Subscriptions: message.channels, message.im, app_mention)"
        );
        info!(
            channel = %self.config.name,
            "Tip: to see WebSocket connect/disconnect logs, run with RUST_LOG=slack_morphism=debug (not RSUT_LOG)"
        );

        socket_mode_listener.serve().await;
        Ok(())
    }
}

/// Socket Mode push-events callback: reads state from user_state, converts event to InboundMessage, forwards to bus.
async fn slack_push_events_handler(
    event: SlackPushEventCallback,
    _client: Arc<SlackHyperClient>,
    states: SlackClientEventsUserState,
) -> UserCallbackResult<()> {
    let event_type = slack_push_event_type_name(&event);
    info!(
        event_type = %event_type,
        "Slack push event received (if you sent a message but see no reply, ensure Event Subscriptions include message.im, message.channels, app_mention)"
    );

    let guard = states.read().await;
    let state_inner = match guard.get_user_state::<SlackPushState>() {
        Some(s) => s.inner.clone(),
        None => {
            warn!("Slack push event: no user state, skipping");
            return Ok(());
        }
    };
    drop(guard);
    if let Some(inbound) = slack_push_to_inbound(
        &event,
        &state_inner.channel_name,
        &state_inner.allowlist,
        state_inner.enable_allowlist,
        state_inner.group_my_name.as_deref(),
        state_inner.workspace_dir.as_deref(),
        Some(state_inner.token.as_str()),
    )
    .await
    {
        info!(
            channel = %inbound.channel,
            chat_id = %inbound.chat_id,
            sender_id = %inbound.sender_id,
            "Slack message received, forwarding to agent"
        );
        if let Err(e) = state_inner.inbound_tx.send(inbound).await {
            error!("Slack: failed to forward inbound message to bus: {e}");
        }
    }
    Ok(())
}

/// Return a short name for the push event type (for logging).
fn slack_push_event_type_name(event: &SlackPushEventCallback) -> &'static str {
    use slack_morphism::events::SlackEventCallbackBody;
    match &event.event {
        SlackEventCallbackBody::Message(_) => "message",
        SlackEventCallbackBody::AppMention(_) => "app_mention",
        SlackEventCallbackBody::AppHomeOpened(_) => "app_home_opened",
        SlackEventCallbackBody::ChannelCreated(_) => "channel_created",
        SlackEventCallbackBody::ChannelDeleted(_) => "channel_deleted",
        _ => "other",
    }
}

/// Convert Slack push event (Message or AppMention) to InboundMessage.
/// When the message has file attachments and workspace_dir + token are set, downloads each file to workspace (using file_handler naming) and sets media.
/// Returns None if from bot, no text/files, or not allowed.
async fn slack_push_to_inbound(
    event: &SlackPushEventCallback,
    channel_name: &str,
    allowlist: &[AllowlistEntry],
    enable_allowlist: bool,
    group_my_name: Option<&str>,
    workspace_dir: Option<&std::path::Path>,
    token: Option<&str>,
) -> Option<InboundMessage> {
    use slack_morphism::events::SlackEventCallbackBody;

    let (chat_id, sender_id, mut content, is_group, files_opt) = match &event.event {
        SlackEventCallbackBody::Message(msg) => {
            if msg.sender.bot_id.is_some() || msg.hidden == Some(true) {
                info!("Slack: ignoring message (from bot or hidden)");
                return None;
            }
            let sender_id = msg
                .sender
                .user
                .as_ref()
                .map(|u| u.to_slack_format())
                .unwrap_or_else(|| "unknown".to_string());
            let text = msg
                .content
                .as_ref()
                .and_then(|c| c.text.as_ref())
                .map(String::as_str)
                .unwrap_or("")
                .trim();
            let has_files = msg
                .content
                .as_ref()
                .and_then(|c| c.files.as_ref())
                .map(|f| !f.is_empty())
                .unwrap_or(false);
            if text.is_empty() && !has_files {
                info!("Slack: ignoring message (empty text and no attachments)");
                return None;
            }
            let content = if text.is_empty() {
                let n = msg
                    .content
                    .as_ref()
                    .and_then(|c| c.files.as_ref())
                    .map(|f| f.len())
                    .unwrap_or(0);
                format!("[User sent {} attachment(s)]", n)
            } else {
                text.to_string()
            };
            let channel_id = msg
                .origin
                .channel
                .as_ref()
                .map(|c| slack_channel_id_raw(&c.to_slack_format()))
                .unwrap_or_default();
            let is_group = channel_id.starts_with('C');
            let files = msg
                .content
                .as_ref()
                .and_then(|c| c.files.as_ref());
            (channel_id, sender_id, content, is_group, files)
        }
        SlackEventCallbackBody::AppMention(mention) => {
            let sender_id = mention.user.to_slack_format();
            let text = mention
                .content
                .text
                .as_deref()
                .unwrap_or("")
                .trim();
            if text.is_empty() {
                return None;
            }
            let channel_id = slack_channel_id_raw(&mention.channel.to_slack_format());
            (
                channel_id,
                sender_id,
                text.to_string(),
                true,
                None::<&Vec<SlackFile>>,
            )
        }
        _ => {
            info!(
                "Slack: push event is not message/app_mention (got another type). In Slack app enable Event Subscriptions → Subscribe to bot events → add: message.channels, message.im, app_mention"
            );
            return None;
        }
    };

    // Download attachments to workspace when possible
    let mut media = Vec::new();
    if let (Some(ws), Some(tok), Some(files)) = (workspace_dir, token, files_opt) {
        for file in files.iter() {
            let url = match &file.url_private_download {
                Some(u) => u,
                None => {
                    warn!(file_id = %file.id.0, "Slack: file has no url_private_download, skipping");
                    continue;
                }
            };
            let bytes = match slack_download_attachment(tok, url).await {
                Ok(b) => b,
                Err(e) => {
                    warn!(file_id = %file.id.0, error = %e, "Slack: failed to download attachment");
                    continue;
                }
            };
            let name: String = file
                .name
                .as_deref()
                .or(file.title.as_deref())
                .map(String::from)
                .unwrap_or_else(|| format!("file_{}", file.id.0));
            match file_handler::save_incoming_file(ws, &name, &bytes) {
                Ok(path) => {
                    let rel = path
                        .strip_prefix(ws)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .into_owned();
                    media.push(rel);
                }
                Err(e) => {
                    warn!(file_id = %file.id.0, error = %e, "Slack: failed to save attachment to workspace");
                }
            }
        }
    }

    // Allowlist check
    if enable_allowlist && !allowlist.is_empty() {
        let allowed = allowlist.iter().any(|e| e.chat_id == chat_id);
        if !allowed {
            warn!(chat_id = %chat_id, "Slack: chat not in allowlist, ignoring");
            return None;
        }
    }

    // In channels, optionally require @bot mention and strip it
    if is_group {
        if let Some(bot_id) = group_my_name {
            let mention_prefix = format!("<@{bot_id}>");
            let trimmed = content.trim_start();
            if !trimmed.starts_with(&mention_prefix) {
                info!(chat_id = %chat_id, "Slack: group message not @bot, ignoring");
                return None;
            }
            content = trimmed
                .strip_prefix(&mention_prefix)
                .unwrap_or(trimmed)
                .trim_start()
                .to_string();
        }
    }

    Some(InboundMessage {
        channel: channel_name.to_string(),
        sender_id,
        chat_id,
        content,
        timestamp: chrono::Utc::now(),
        media,
        metadata: serde_json::json!({
            "event_id": format!("{}", event.event_id.0),
            "team_id": format!("{}", event.team_id.0),
        }),
    })
}

// ---------------------------------------------------------------------------
// Channel trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Channel for SlackChannel {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn start(&mut self) -> Result<()> {
        info!("Slack channel starting (Socket Mode)");
        self.running = true;

        let mut outbound_rx = self.outbound_rx.take().unwrap();
        let bot_token = self.bot_token.clone();
        let token_str = self.config.token.clone();
        let channel_name = self.config.name.clone();
        let show_tool_calls = self.show_tool_calls;
        let workspace_dir = self.workspace_dir.clone();

        tokio::spawn(async move {
            while let Ok(msg) = outbound_rx.recv().await {
                if msg.channel != channel_name {
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
                        } else if result_preview.len() > 100 {
                            format!("{}...", result_preview.chars().take(100).collect::<String>())
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
                        let content = request
                            .display_message
                            .clone()
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| approval_formatter::format_approval_request(request));
                        (content, vec![])
                    }
                };
                let client = match SlackClientHyperConnector::new() {
                    Ok(connector) => SlackClient::new(connector),
                    Err(e) => {
                        error!("Slack outbound: connector build failed: {e:#}");
                        continue;
                    }
                };
                let session = client.open_session(&bot_token);
                let raw_channel_id = slack_channel_id_raw(&msg.chat_id);

                if !content.is_empty() {
                    let chunks = split_message(&content, SLACK_MAX_MESSAGE_LEN);
                    for chunk in &chunks {
                        let req = SlackApiChatPostMessageRequest::new(
                            raw_channel_id.clone().into(),
                            SlackMessageContent::new().with_text(chunk.clone().into()),
                        );
                        if let Err(e) = session.chat_post_message(&req).await {
                            error!("Slack outbound send error: {e:#}");
                        }
                    }
                }
                // Slack does not support multiple files in one message; send one file per message
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
                                if let Err(e) = slack_upload_file_v2(
                                    &token_str,
                                    &raw_channel_id,
                                    &file_name,
                                    file_data,
                                )
                                .await
                                {
                                    error!("Slack file upload error: {e:#}");
                                }
                            }
                        }
                    }
                }
            }
        });

        self.run_socket_mode().await
    }

    async fn stop(&mut self) -> Result<()> {
        info!("Slack channel stopping");
        self.running = false;
        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> Result<()> {
        let (content, media) = match &msg.message_type {
            crate::bus::OutboundMessageType::Chat { content, media } => {
                (content.clone(), media.clone())
            }
            crate::bus::OutboundMessageType::ApprovalRequest { request } => (
                request
                    .display_message
                    .clone()
                    .filter(|s| !s.is_empty())
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
                } else if result_preview.len() > 100 {
                    format!("{}...", result_preview.chars().take(100).collect::<String>())
                } else {
                    result_preview.clone()
                };
                let content = if preview.is_empty() {
                    format!("🔧 {} — {}", tool_name, status)
                } else {
                    format!("🔧 {} — {}\n{}", tool_name, status, preview)
                };
                return self.send_message(&msg.chat_id, &content).await;
            }
        };
        if !content.is_empty() {
            self.send_message(&msg.chat_id, &content).await?;
        }
        if !media.is_empty() && self.workspace_dir.is_some() {
            let client = SlackClient::new(SlackClientHyperConnector::new()?);
            let session = client.open_session(&self.bot_token);
            let raw_channel_id = slack_channel_id_raw(&msg.chat_id);
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
                        let _ = slack_upload_file_v2(
                            &self.config.token,
                            &raw_channel_id,
                            &file_name,
                            file_data,
                        )
                        .await;
                    }
                }
            }
        }
        Ok(())
    }
}
