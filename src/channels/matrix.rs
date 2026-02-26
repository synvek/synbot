//! Matrix channel — matrix-sdk integration.
//!
//! Connects to a Matrix homeserver, logs in (username/password or access token),
//! syncs and handles room messages. Converts Matrix room messages to InboundMessage
//! and sends OutboundMessage back to rooms. Supports allowlist and optional @mention in rooms.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use matrix_sdk::{
    config::SyncSettings,
    ruma::{
        events::room::message::{
            MessageType, RoomMessageEventContent, SyncRoomMessageEvent,
        },
        RoomId,
    },
    authentication::{matrix::MatrixSession, AuthSession},
    Client, Room, RoomState, SessionMeta, SessionTokens,
};
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::{approval_formatter, Channel};
use crate::config::{AllowlistEntry, MatrixConfig};

// ---------------------------------------------------------------------------
// Message splitting (Matrix has no strict limit; use a reasonable chunk size)
// ---------------------------------------------------------------------------

const MATRIX_MAX_MESSAGE_LEN: usize = 4000;

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
// MatrixChannel
// ---------------------------------------------------------------------------

pub struct MatrixChannel {
    config: MatrixConfig,
    show_tool_calls: bool,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    client: Option<Arc<Client>>,
    workspace_dir: Option<PathBuf>,
}

impl MatrixChannel {
    pub fn new(
        config: MatrixConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: broadcast::Receiver<OutboundMessage>,
        show_tool_calls: bool,
        workspace_dir: Option<PathBuf>,
    ) -> Result<Self> {
        Ok(Self {
            config,
            show_tool_calls,
            inbound_tx,
            outbound_rx: Some(outbound_rx),
            client: None,
            workspace_dir,
        })
    }

    async fn ensure_client(&mut self) -> Result<Arc<Client>> {
        if let Some(c) = &self.client {
            return Ok(Arc::clone(c));
        }
        let homeserver_url = self
            .config
            .homeserver_url
            .trim()
            .trim_end_matches('/')
            .to_string();
        let url = url::Url::parse(&homeserver_url)
            .context("Matrix homeserver_url must be a valid URL (e.g. https://matrix.example.org)")?;
        let server = url.host_str().unwrap_or("localhost").to_string();

        let client = Client::builder().homeserver_url(url).build().await?;

        if let Some(ref token) = self.config.access_token {
            let token = token.trim();
            if !token.is_empty() {
                let username = self.config.username.trim();
                if username.is_empty() {
                    anyhow::bail!("Matrix: when using accessToken, username (user ID e.g. @bot:example.org) must be set");
                }
                let user_id = matrix_sdk::ruma::UserId::parse(username)
                    .context("Matrix username must be a valid user ID (e.g. @bot:example.org)")?;
                let device_id = matrix_sdk::ruma::device_id!("SYNBOT");
                let session = AuthSession::Matrix(MatrixSession {
                    meta: SessionMeta {
                        user_id: user_id.to_owned(),
                        device_id: device_id.to_owned(),
                    },
                    tokens: SessionTokens {
                        access_token: token.to_string(),
                        refresh_token: None,
                    },
                });
                client.restore_session(session).await?;
                info!(
                    channel = %self.config.name,
                    user_id = %user_id,
                    "Matrix: restored session with access token"
                );
            }
        }

        if !client.is_active() {
            let username = self.config.username.trim();
            let password = self.config.password.trim();
            if username.is_empty() || password.is_empty() {
                anyhow::bail!(
                    "Matrix: set either accessToken or both username and password for login"
                );
            }
                let user_id = if username.starts_with('@') && username.contains(':') {
                matrix_sdk::ruma::UserId::parse(username)
                    .context("Matrix username must be a valid user ID (e.g. @bot:example.org)")?
            } else {
                matrix_sdk::ruma::UserId::parse(&format!("@{}:{}", username, server))
                    .context("Matrix username could not be parsed as user ID")?
            };
            client
                .matrix_auth()
                .login_username(user_id.as_str(), password)
                .initial_device_display_name("synbot")
                .send()
                .await
                .context("Matrix login failed")?;
            info!(
                channel = %self.config.name,
                user_id = %user_id,
                "Matrix: logged in with username/password"
            );
        }

        self.client = Some(Arc::new(client));
        Ok(Arc::clone(self.client.as_ref().unwrap()))
    }

    async fn send_to_room(&self, room_id: &RoomId, content: &str) -> Result<()> {
        let client = match &self.client {
            Some(c) => c,
            None => return Ok(()),
        };
        let room = match client.get_room(room_id) {
            Some(r) => r,
            None => {
                warn!(room_id = %room_id, "Matrix: room not found, cannot send");
                return Ok(());
            }
        };
        let chunks = split_message(content, MATRIX_MAX_MESSAGE_LEN);
        for chunk in chunks {
            let msg = RoomMessageEventContent::text_plain(chunk);
            room.send(msg).await.context("Matrix send message")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Channel trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Channel for MatrixChannel {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn start(&mut self) -> Result<()> {
        info!(channel = %self.config.name, "Matrix channel starting");

        let client = self.ensure_client().await?;

        let channel_name = self.config.name.clone();
        let allowlist = self.config.allowlist.clone();
        let enable_allowlist = self.config.enable_allowlist;
        let group_my_name = self.config.group_my_name.clone();
        let inbound_tx = self.inbound_tx.clone();

        client.add_event_handler(
            move |event: SyncRoomMessageEvent, room: Room| {
                let channel_name = channel_name.clone();
                let allowlist = allowlist.clone();
                let enable_allowlist = enable_allowlist;
                let group_my_name = group_my_name.clone();
                let inbound_tx = inbound_tx.clone();

                async move {
                    if room.state() != RoomState::Joined {
                        return;
                    }
                    let event = match &event {
                        matrix_sdk::ruma::events::SyncMessageLikeEvent::Original(ev) => ev,
                        _ => return,
                    };
                    let MessageType::Text(text_content) = &event.content.msgtype else {
                        return;
                    };
                    let sender = event.sender.as_str();
                    let room_id = room.room_id().as_str();
                    let body = text_content.body.trim();
                    if body.is_empty() {
                        return;
                    }

                    if enable_allowlist && !allowlist.is_empty() {
                        let allowed = allowlist.iter().any(|e| {
                            e.chat_id == room_id || e.chat_id == sender
                        });
                        if !allowed {
                            warn!(room_id = %room_id, "Matrix: room/user not in allowlist, ignoring");
                            return;
                        }
                    }

                    let mut content = body.to_string();
                    if let Some(ref bot_id) = group_my_name {
                        let mention_prefix = format!("@{}", bot_id.trim_start_matches('@'));
                        if content.starts_with(&mention_prefix) {
                            content = content
                                .strip_prefix(&mention_prefix)
                                .unwrap_or(content.as_str())
                                .trim_start()
                                .to_string();
                        } else {
                            let full_mention = format!("{}:", mention_prefix);
                            if content.starts_with(&full_mention) {
                                content = content
                                    .strip_prefix(&full_mention)
                                    .unwrap_or(content.as_str())
                                    .trim_start()
                                    .to_string();
                            } else {
                                return;
                            }
                        }
                    }

                    let inbound = InboundMessage {
                        channel: channel_name.clone(),
                        sender_id: sender.to_string(),
                        chat_id: room_id.to_string(),
                        content,
                        timestamp: chrono::Utc::now(),
                        media: vec![],
                        metadata: serde_json::json!({
                            "event_id": event.event_id.to_string(),
                        }),
                    };
                    if let Err(e) = inbound_tx.send(inbound).await {
                        error!("Matrix: failed to forward inbound message: {e}");
                    }
                }
            },
        );

        let mut outbound_rx = self.outbound_rx.take().unwrap();
        let client_out = Arc::clone(&client);
        let channel_name_out = self.config.name.clone();
        let show_tool_calls = self.show_tool_calls;
        let workspace_dir = self.workspace_dir.clone();

        tokio::spawn(async move {
            while let Ok(msg) = outbound_rx.recv().await {
                if msg.channel != channel_name_out {
                    continue;
                }
                let room_id = match RoomId::parse(&msg.chat_id) {
                    Ok(id) => id,
                    Err(e) => {
                        error!(chat_id = %msg.chat_id, "Matrix: invalid room id: {e}");
                        continue;
                    }
                };
                let (content, _media_paths): (String, Vec<String>) = match &msg.message_type {
                    crate::bus::OutboundMessageType::Chat { content, media: _ } => {
                        (content.clone(), vec![])
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
                            format!("{} — {}", tool_name, status)
                        } else {
                            format!("{} — {}\n{}", tool_name, status, preview)
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
                if !content.is_empty() {
                    if let Some(room) = client_out.get_room(&room_id) {
                        let chunks = split_message(&content, MATRIX_MAX_MESSAGE_LEN);
                        for chunk in chunks {
                            if let Err(e) = room.send(RoomMessageEventContent::text_plain(&chunk)).await {
                                error!("Matrix outbound send error: {e:#}");
                            }
                        }
                    }
                }
                if let Some(ws) = &workspace_dir {
                    if let crate::bus::OutboundMessageType::Chat { media, .. } = &msg.message_type {
                        for path_str in media {
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
                                    if let Some(room) = client_out.get_room(&room_id) {
                                        if let Err(e) = room.send(
                                            RoomMessageEventContent::text_plain(&format!("[Attachment: {}]", file_name))
                                        ).await {
                                            error!("Matrix file notice send error: {e:#}");
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        let sync_token = client.sync_once(SyncSettings::default()).await?.next_batch;
        let settings = SyncSettings::default().token(sync_token);
        client.sync(settings).await?;

        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        info!(channel = %self.config.name, "Matrix channel stopping");
        Ok(())
    }

    async fn send(&self, msg: &OutboundMessage) -> Result<()> {
        let room_id = match RoomId::parse(&msg.chat_id) {
            Ok(id) => id,
            Err(e) => {
                anyhow::bail!("Matrix: invalid room id {:?}: {}", msg.chat_id, e);
            }
        };

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
                    format!("{} — {}", tool_name, status)
                } else {
                    format!("{} — {}\n{}", tool_name, status, preview)
                };
                self.send_to_room(&room_id, &content).await?;
                return Ok(());
            }
        };

        if !content.is_empty() {
            self.send_to_room(&room_id, &content).await?;
        }
        if !media.is_empty() {
            for path_str in &media {
                if let Some(ws) = &self.workspace_dir {
                    let path = std::path::Path::new(path_str);
                    let abs = if path.is_absolute() {
                        path.to_path_buf()
                    } else {
                        ws.join(path_str)
                    };
                    if abs.exists() {
                        let file_name = abs
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "file".to_string());
                        self.send_to_room(
                            &room_id,
                            &format!("[Attachment: {}]", file_name),
                        )
                        .await?;
                    }
                }
            }
        }
        Ok(())
    }
}
