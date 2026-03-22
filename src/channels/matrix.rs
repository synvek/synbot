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
use tokio::sync::{broadcast, broadcast::error::RecvError, mpsc};
use tracing::{debug, error, info, warn};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::{approval_formatter, Channel};
use crate::config::{
    pairing_allows, pairing_message, pairings_from_config_file_cached, sessions_root, MatrixConfig,
};

// ---------------------------------------------------------------------------
// Message splitting (Matrix has no strict limit; use a reasonable chunk size)
// ---------------------------------------------------------------------------

const MATRIX_MAX_MESSAGE_LEN: usize = 4000;

/// Fixed device id for synbot's Matrix session. Password login must send the same id every time;
/// otherwise the homeserver mints a new device and the SQLite crypto store (bound to user+device) rejects the session.
const MATRIX_DEVICE_ID: &str = "SYNBOT";

/// Extract a plain-text body for agent processing (`m.text`, `m.notice`, `m.emote`).
fn matrix_plain_body_from_msgtype(msgtype: &MessageType) -> Option<&str> {
    match msgtype {
        MessageType::Text(c) => Some(c.body.as_str()),
        MessageType::Notice(c) => Some(c.body.as_str()),
        MessageType::Emote(c) => Some(c.body.as_str()),
        _ => None,
    }
}

fn matrix_channel_slug(name: &str) -> String {
    let name = name.trim();
    let name = if name.is_empty() { "matrix" } else { name };
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn expand_user_path(raw: &str) -> PathBuf {
    let raw = raw.trim();
    if raw.starts_with('~') {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(raw.trim_start_matches("~/").trim_start_matches('~').trim_start_matches('/'))
    } else {
        PathBuf::from(raw)
    }
}

/// Normalized MXID for matching `event.sender` (echo suppression). Returns `None` if username unset.
fn matrix_bot_user_id(cfg: &MatrixConfig) -> Option<String> {
    let username = cfg.username.trim();
    if username.is_empty() {
        return None;
    }
    if username.starts_with('@') && username.contains(':') {
        return Some(username.to_string());
    }
    let homeserver_url = cfg.homeserver_url.trim();
    let url = url::Url::parse(homeserver_url).ok()?;
    let server = url.host_str().unwrap_or("localhost");
    let local = username.trim_start_matches('@');
    matrix_sdk::ruma::UserId::parse(&format!("@{local}:{server}"))
        .ok()
        .map(|u| u.to_string())
}

/// `allowlist[].chatId` must be a **room id** (`!sigil:server`) or **sender MXID** (`@user:server`).
/// Room aliases (`#name:server`) do not match. Compared trimmed and ASCII case-insensitive.
fn matrix_allowlist_entry_matches(entry_chat_id: &str, room_id: &str, sender: &str) -> bool {
    let e = entry_chat_id.trim();
    if e.is_empty() {
        return false;
    }
    let room = room_id.trim();
    let send = sender.trim();
    e == room
        || e == send
        || e.eq_ignore_ascii_case(room)
        || e.eq_ignore_ascii_case(send)
}

fn matrix_sqlite_path(cfg: &MatrixConfig) -> PathBuf {
    let custom = cfg.store_path.trim();
    if custom.is_empty() {
        sessions_root()
            .join("matrix")
            .join(matrix_channel_slug(&cfg.name))
            .join("store.sqlite")
    } else {
        expand_user_path(custom)
    }
}

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
    tool_result_preview_chars: usize,
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: Option<broadcast::Receiver<OutboundMessage>>,
    client: Option<Arc<Client>>,
    workspace_dir: Option<PathBuf>,
    config_path: Option<PathBuf>,
}

impl MatrixChannel {
    pub fn new(
        config: MatrixConfig,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_rx: broadcast::Receiver<OutboundMessage>,
        show_tool_calls: bool,
        tool_result_preview_chars: usize,
        workspace_dir: Option<PathBuf>,
        config_path: Option<PathBuf>,
    ) -> Result<Self> {
        Ok(Self {
            config,
            show_tool_calls,
            tool_result_preview_chars,
            inbound_tx,
            outbound_rx: Some(outbound_rx),
            client: None,
            workspace_dir,
            config_path,
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

        let db_path = matrix_sqlite_path(&self.config);
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Matrix: failed to create store directory {}",
                    parent.display()
                )
            })?;
        }
        let lock_holder = format!("synbot-matrix-{}", matrix_channel_slug(&self.config.name));
        info!(
            channel = %self.config.name,
            store = %db_path.display(),
            "Matrix: opening SQLite store (sync state + E2EE crypto)"
        );
        // Same reqwest/DNS/TLS as the rest of synbot (system resolver by default; Google DNS when SYNBOT_IN_APP_SANDBOX).
        let http = crate::appcontainer_dns::build_reqwest_client();
        let client = Client::builder()
            .homeserver_url(url)
            .http_client(http)
            .cross_process_store_locks_holder_name(lock_holder)
            .sqlite_store(&db_path, None)
            .build()
            .await
            .context("Matrix: failed to build client (check store path and permissions)")?;

        if let Some(ref token) = self.config.access_token {
            let token = token.trim();
            if !token.is_empty() {
                let username = self.config.username.trim();
                if username.is_empty() {
                    anyhow::bail!("Matrix: when using accessToken, username (user ID e.g. @bot:example.org) must be set");
                }
                let user_id = matrix_sdk::ruma::UserId::parse(username)
                    .context("Matrix username must be a valid user ID (e.g. @bot:example.org)")?;
                let device_id = matrix_sdk::ruma::device_id!(MATRIX_DEVICE_ID);
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
            let versions_probe = format!("{}/_matrix/client/versions", homeserver_url);
            client
                .matrix_auth()
                .login_username(user_id.as_str(), password)
                .device_id(MATRIX_DEVICE_ID)
                .initial_device_display_name("synbot")
                .send()
                .await
                .with_context(|| {
                    format!(
                        "Matrix login failed for {} at {}. \
                         HTTP 503 or `<non-json bytes>` usually means the homeserver is not ready or the URL hits a proxy/HTML error page. \
                         Check: curl -sS '{}'. \
                         If the error mentions DNS / hickory / `Operation not permitted`, the process cannot read system resolver config (macOS sandbox/Seatbelt): use `http://127.0.0.1:8008` instead of `localhost`, or run outside the sandbox. \
                         If the error mentions crypto store / account doesn't match / device: delete this channel's SQLite store file (default under sessions/matrix/{{name}}/store.sqlite) once to clear an old random device id, then log in again — synbot pins device id to `{}`. \
                         If login still fails, set username to the full MXID @localpart:server_name where server_name matches Synapse `server_name` (not only the URL hostname when they differ).",
                        user_id,
                        homeserver_url,
                        versions_probe,
                        MATRIX_DEVICE_ID
                    )
                })?;
            info!(
                channel = %self.config.name,
                user_id = %user_id,
                device_id = MATRIX_DEVICE_ID,
                "Matrix: logged in with username/password (stable device id for crypto store)"
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
        let default_agent = self.config.default_agent.clone();
        let allowlist = self.config.allowlist.clone();
        let enable_allowlist = self.config.enable_allowlist;
        let group_my_name = self.config.group_my_name.clone();
        let inbound_tx = self.inbound_tx.clone();
        let config_path = self.config_path.clone();
        let bot_user_id = matrix_bot_user_id(&self.config);

        if enable_allowlist && allowlist.is_empty() {
            warn!(
                channel = %self.config.name,
                "Matrix: enableAllowlist is true and allowlist is empty — deny-all until you add room/user entries or CLI pairings (security default)"
            );
        }

        client.add_event_handler(
            move |event: SyncRoomMessageEvent, room: Room| {
                let channel_name = channel_name.clone();
                let default_agent = default_agent.clone();
                let allowlist = allowlist.clone();
                let enable_allowlist = enable_allowlist;
                let group_my_name = group_my_name.clone();
                let inbound_tx = inbound_tx.clone();
                let config_path = config_path.clone();
                let bot_user_id = bot_user_id.clone();

                async move {
                    if room.state() != RoomState::Joined {
                        info!(
                            room_id = %room.room_id(),
                            state = ?room.state(),
                            "Matrix: skip timeline message (room not joined)"
                        );
                        return;
                    }
                    let event = match &event {
                        matrix_sdk::ruma::events::SyncMessageLikeEvent::Original(ev) => ev,
                        _ => {
                            info!(
                                room_id = %room.room_id(),
                                "Matrix: skip non-original message (redacted or unsigned)"
                            );
                            return;
                        }
                    };
                    let sender = event.sender.as_str();
                    let room_id = room.room_id().as_str();
                    if bot_user_id.as_deref() == Some(sender) {
                        debug!(room_id = %room_id, "Matrix: skip own message (echo)");
                        return;
                    }
                    info!(
                        room_id = %room_id,
                        sender = %sender,
                        "Matrix: m.room.message received"
                    );
                    let Some(raw_body) = matrix_plain_body_from_msgtype(&event.content.msgtype) else {
                        info!(
                            room_id = %room_id,
                            msgtype = ?event.content.msgtype,
                            "Matrix: skip non-plain message (only m.text / m.notice / m.emote are handled)"
                        );
                        return;
                    };
                    let body = raw_body.trim();
                    if body.is_empty() {
                        info!(room_id = %room_id, "Matrix: skip empty message body");
                        return;
                    }

                    if enable_allowlist {
                        let allowed = allowlist.iter().any(|e| {
                            matrix_allowlist_entry_matches(&e.chat_id, room_id, sender)
                        });
                        let pairings = config_path
                            .as_ref()
                            .map(|p| pairings_from_config_file_cached(p.as_path()))
                            .unwrap_or_default();
                        let paired = pairing_allows(room_id, "matrix", &pairings)
                            || pairing_allows(sender, "matrix", &pairings);
                        if !allowed && !paired {
                            warn!(
                                room_id = %room_id,
                                sender = %sender,
                                allowlist_size = allowlist.len(),
                                "Matrix: no allowlist match (use room id !…:server from Element Room settings → Advanced, not #alias; or sender MXID; chatId is trimmed/case-insensitive)"
                            );
                            let hint = if allowlist.is_empty() {
                                format!(
                                    "{} Allowlist tip: `chatId` must be this room's internal id (starts with !) or someone's @user:server — not a #room alias.",
                                    pairing_message("matrix", room_id)
                                )
                            } else {
                                let n = allowlist.len();
                                let entries_word = if n == 1 { "entry" } else { "entries" };
                                format!(
                                    "{} Your config lists {} allowlist {} but none matches this room or sender. Often the `chatId` is from another room: copy the value after \"Current chat id\" above into `channels.matrix[].allowlist[].chatId` (or use your MXID @user:server), then restart synbot.",
                                    pairing_message("matrix", room_id),
                                    n,
                                    entries_word
                                )
                            };
                            let _ = room
                                .send(RoomMessageEventContent::text_plain(hint))
                                .await;
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
                                info!(
                                    room_id = %room_id,
                                    "Matrix: skip group message (set groupMyName and start messages with @bot_id or @bot_id:)"
                                );
                                return;
                            }
                        }
                    }

                    let content_len = content.len();
                    let inbound = InboundMessage {
                        channel: channel_name.clone(),
                        sender_id: sender.to_string(),
                        chat_id: room_id.to_string(),
                        content,
                        timestamp: chrono::Utc::now(),
                        media: vec![],
                        metadata: serde_json::json!({
                            "event_id": event.event_id.to_string(),
                            "default_agent": default_agent,
                        }),
                    };
                    if let Err(e) = inbound_tx.send(inbound).await {
                        error!("Matrix: failed to forward inbound message: {e}");
                    } else {
                        info!(
                            room_id = %room_id,
                            sender = %sender,
                            chars = content_len,
                            "Matrix: message forwarded to agent bus"
                        );
                    }
                }
            },
        );

        let mut outbound_rx = self.outbound_rx.take().unwrap();
        let client_out = Arc::clone(&client);
        let channel_name_out = self.config.name.clone();
        let show_tool_calls = self.show_tool_calls;
        let tool_result_preview_chars = self.tool_result_preview_chars;
        let workspace_dir = self.workspace_dir.clone();

        tokio::spawn(async move {
            loop {
                let msg = match outbound_rx.recv().await {
                    Ok(m) => m,
                    Err(RecvError::Lagged(n)) => {
                        warn!(
                            skipped = n,
                            "Matrix: outbound broadcast lagged; continuing (replies would have been lost without this)"
                        );
                        continue;
                    }
                    Err(RecvError::Closed) => break,
                };
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
                        } else if result_preview.len() > tool_result_preview_chars {
                            format!("{}...", result_preview.chars().take(tool_result_preview_chars).collect::<String>())
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
                    match client_out.get_room(&room_id) {
                        Some(room) => {
                            let chunks = split_message(&content, MATRIX_MAX_MESSAGE_LEN);
                            for chunk in chunks {
                                if let Err(e) =
                                    room.send(RoomMessageEventContent::text_plain(&chunk)).await
                                {
                                    error!("Matrix outbound send error: {e:#}");
                                }
                            }
                        }
                        None => {
                            warn!(
                                room_id = %room_id,
                                "Matrix: outbound message dropped (room not in client; bot may not have joined this room)"
                            );
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

        let joined = client.joined_rooms();
        info!(
            channel = %self.config.name,
            joined_rooms = joined.len(),
            "Matrix: initial sync complete"
        );
        for room in &joined {
            let enc = room.encryption_state();
            if enc.is_encrypted() {
                info!(
                    channel = %self.config.name,
                    room_id = %room.room_id(),
                    "Matrix: encrypted room — E2EE is enabled; keys are persisted in the configured SQLite store"
                );
            } else if enc.is_unknown() {
                info!(
                    channel = %self.config.name,
                    room_id = %room.room_id(),
                    "Matrix: room encryption state unknown after first sync (if you never see 'message forwarded', check E2EE / room type)"
                );
            }
        }

        let settings = SyncSettings::default().token(sync_token);
        info!(
            channel = %self.config.name,
            "Matrix: entering long sync loop (receiving live events)"
        );
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
                } else if result_preview.len() > self.tool_result_preview_chars {
                    format!("{}...", result_preview.chars().take(self.tool_result_preview_chars).collect::<String>())
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
