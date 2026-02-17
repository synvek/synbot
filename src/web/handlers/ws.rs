use actix::{Actor, ActorContext, AsyncContext, Handler, Message as ActixMessage, StreamHandler, WrapFuture, ActorFutureExt};
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::bus::{InboundMessage, OutboundMessage};
use crate::web::channel::{WebChannel, WebSocketConnection};
use crate::web::state::AppState;

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// WebSocket client message types
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMessage {
    Chat { content: String },
    ApprovalResponse {
        request_id: String,
        approved: bool,
    },
    Ping,
}

/// WebSocket server message types
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsServerMessage {
    ChatResponse {
        content: String,
        timestamp: chrono::DateTime<Utc>,
    },
    ApprovalRequest {
        request: crate::tools::approval::ApprovalRequest,
    },
    ApprovalResult {
        request_id: String,
        approved: bool,
        message: String,
    },
    Error {
        message: String,
    },
    Pong,
    Connected {
        session_id: String,
    },
    History {
        messages: Vec<HistoryMessage>,
    },
    /// Tool execution progress (sent in real time during agent run)
    ToolProgress {
        tool_name: String,
        status: String,
        result_preview: String,
    },
}

/// Message in history. When loading channel view, `agent_id` is set so the UI can show which agent (main or role) the message belongs to.
#[derive(Debug, Serialize)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<Utc>,
    /// Agent that produced or received this message (main, dev, …). Omitted for backward compat when single-session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

/// WebSocket session actor
pub struct WsSession {
    /// Unique connection ID
    id: String,
    /// User ID (from auth or generated)
    user_id: String,
    /// Last heartbeat time
    hb: Instant,
    /// Application state
    state: web::Data<AppState>,
    /// Web channel
    web_channel: Option<WebChannel>,
}

impl WsSession {
    pub fn new(user_id: String, state: web::Data<AppState>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            user_id,
            hb: Instant::now(),
            state,
            web_channel: None,
        }
    }

    /// Helper method to start heartbeat process
    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // Check client heartbeat
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                tracing::warn!("WebSocket client heartbeat failed, disconnecting");
                ctx.stop();
                return;
            }

            ctx.ping(b"");
        });
    }

    /// Send a server message to the client
    fn send_message(&self, ctx: &mut ws::WebsocketContext<Self>, msg: WsServerMessage) {
        if let Ok(json) = serde_json::to_string(&msg) {
            ctx.text(json);
        }
    }
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("WebSocket connection started: {}", self.id);

        // Start heartbeat
        self.hb(ctx);

        // Create web channel
        let web_channel = WebChannel::new(
            self.state.inbound_tx.clone(),
            self.state.outbound_tx.clone(),
        );

        // Register connection
        let conn = WebSocketConnection {
            id: self.id.clone(),
            user_id: self.user_id.clone(),
            connected_at: Utc::now(),
        };

        let web_channel_clone = web_channel.clone();
        let conn_clone = conn.clone();
        ctx.spawn(
            async move {
                web_channel_clone.register_connection(conn_clone).await;
            }
            .into_actor(self)
            .map(|_, _, _| {}),
        );

        // Subscribe to outbound messages
        let mut outbound_rx = web_channel.subscribe_outbound();
        let user_id = self.user_id.clone();
        let addr = ctx.address();

        ctx.spawn(
            async move {
                while let Ok(msg) = outbound_rx.recv().await {
                    // Only forward messages for web channel and this user
                    if msg.channel == "web" && msg.chat_id == user_id {
                        addr.do_send(OutboundMessageWrapper(msg));
                    }
                }
            }
            .into_actor(self)
            .map(|_, _, _| {}),
        );

        self.web_channel = Some(web_channel);

        // Send connected message
        self.send_message(
            ctx,
            WsServerMessage::Connected {
                session_id: self.id.clone(),
            },
        );

        // Load and send history for this channel (web) and user: all sessions (main + roles) so the UI shows the full thread.
        let state = self.state.clone();
        let user_id = self.user_id.clone();
        let addr = ctx.address();
        ctx.spawn(
            async move {
                use crate::agent::session_id::SessionScope;

                let sm = state.session_manager.read().await;
                let sessions = sm.get_sessions_for_channel("web", SessionScope::Dm, &user_id);

                if sessions.is_empty() {
                    tracing::info!("No sessions found for web/dm/{}", user_id);
                    addr.do_send(HistoryMessageWrapper(Vec::new()));
                    return;
                }

                let total_msgs: usize = sessions.iter().map(|(_, msgs)| msgs.len()).sum();
                tracing::info!(
                    "Loading channel history for web/dm/{}: {} session(s), {} message(s)",
                    user_id,
                    sessions.len(),
                    total_msgs
                );

                let mut all: Vec<(chrono::DateTime<Utc>, HistoryMessage)> = Vec::new();
                for (meta, messages) in &sessions {
                    let agent_id = meta.id.agent_id.clone();
                    for msg in messages {
                        all.push((
                            msg.timestamp,
                            HistoryMessage {
                                role: msg.role.clone(),
                                content: msg.content.clone(),
                                timestamp: msg.timestamp,
                                agent_id: Some(agent_id.clone()),
                            },
                        ));
                    }
                }
                all.sort_by_key(|(ts, _)| *ts);
                let history_messages: Vec<HistoryMessage> = all.into_iter().map(|(_, m)| m).collect();
                addr.do_send(HistoryMessageWrapper(history_messages));
            }
            .into_actor(self)
            .map(|_, _, _| {}),
        );
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        tracing::info!("WebSocket connection stopped: {}", self.id);

        // Unregister connection
        if let Some(web_channel) = &self.web_channel {
            let web_channel_clone = web_channel.clone();
            let conn_id = self.id.clone();
            ctx.spawn(
                async move {
                    web_channel_clone.unregister_connection(&conn_id).await;
                }
                .into_actor(self)
                .map(|_, _, _| {}),
            );
        }
    }
}

/// Handle text messages from the WebSocket client
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                self.hb = Instant::now();

                // Parse client message
                match serde_json::from_str::<WsClientMessage>(&text) {
                    Ok(WsClientMessage::Chat { content }) => {
                        // Create inbound message
                        let inbound_msg = InboundMessage {
                            channel: "web".to_string(),
                            sender_id: self.user_id.clone(),
                            chat_id: self.user_id.clone(),
                            content,
                            timestamp: Utc::now(),
                            media: vec![],
                            metadata: serde_json::Value::Null,
                        };

                        // Send to message bus
                        let inbound_tx = self.state.inbound_tx.clone();
                        ctx.spawn(
                            async move {
                                if let Err(e) = inbound_tx.send(inbound_msg).await {
                                    tracing::error!("Failed to send inbound message: {}", e);
                                }
                            }
                            .into_actor(self)
                            .map(|_, _, _| {}),
                        );
                    }
                    Ok(WsClientMessage::ApprovalResponse { request_id, approved }) => {
                        // Handle approval response
                        let approval_manager = self.state.approval_manager.clone();
                        let user_id = self.user_id.clone();
                        
                        ctx.spawn(
                            async move {
                                let response = crate::tools::approval::ApprovalResponse {
                                    request_id: request_id.clone(),
                                    approved,
                                    responder: user_id,
                                    timestamp: Utc::now(),
                                };
                                
                                if let Err(e) = approval_manager.submit_response(response).await {
                                    tracing::error!("Failed to submit approval response: {}", e);
                                }
                                
                                (request_id, approved)
                            }
                            .into_actor(self)
                            .map(move |result, actor, ctx| {
                                let (request_id, approved) = result;
                                // Send confirmation back to client
                                let message = if approved {
                                    "approved".to_string()
                                } else {
                                    "rejected".to_string()
                                };
                                actor.send_message(
                                    ctx,
                                    WsServerMessage::ApprovalResult {
                                        request_id,
                                        approved,
                                        message,
                                    },
                                );
                            }),
                        );
                    }
                    Ok(WsClientMessage::Ping) => {
                        self.send_message(ctx, WsServerMessage::Pong);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse WebSocket message: {}", e);
                        self.send_message(
                            ctx,
                            WsServerMessage::Error {
                                message: format!("Invalid message format: {}", e),
                            },
                        );
                    }
                }
            }
            Ok(ws::Message::Binary(_)) => {
                tracing::warn!("Binary messages not supported");
            }
            Ok(ws::Message::Close(reason)) => {
                tracing::info!("WebSocket close: {:?}", reason);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

/// Wrapper for outbound messages to be sent as actor messages
#[derive(ActixMessage)]
#[rtype(result = "()")]
struct OutboundMessageWrapper(OutboundMessage);

/// Handle outbound messages from the message bus
impl Handler<OutboundMessageWrapper> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: OutboundMessageWrapper, ctx: &mut Self::Context) {
        let send = match msg.0.message_type {
            crate::bus::OutboundMessageType::Chat { content, .. } => {
                Some(WsServerMessage::ChatResponse {
                    content,
                    timestamp: Utc::now(),
                })
            }
            crate::bus::OutboundMessageType::ApprovalRequest { request } => {
                Some(WsServerMessage::ApprovalRequest { request })
            }
            crate::bus::OutboundMessageType::ToolProgress {
                tool_name,
                status,
                result_preview,
            } => {
                if self.state.config.show_tool_calls && self.state.config.web.show_tool_calls {
                    Some(WsServerMessage::ToolProgress {
                        tool_name,
                        status,
                        result_preview,
                    })
                } else {
                    None
                }
            }
        };
        if let Some(server_msg) = send {
            self.send_message(ctx, server_msg);
        }
    }
}

/// Wrapper for history messages to be sent as actor messages
#[derive(ActixMessage)]
#[rtype(result = "()")]
struct HistoryMessageWrapper(Vec<HistoryMessage>);

/// Handle history messages
impl Handler<HistoryMessageWrapper> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: HistoryMessageWrapper, ctx: &mut Self::Context) {
        let server_msg = WsServerMessage::History {
            messages: msg.0,
        };

        self.send_message(ctx, server_msg);
    }
}

/// WebSocket route handler
pub async fn ws_chat(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    // Use a fixed user_id for web channel since it's a global management interface
    // All connections share the same session
    let user_id = "web_admin".to_string();

    tracing::info!("WebSocket connection for web admin interface");

    let ws_session = WsSession::new(user_id, state);
    let resp = ws::start(ws_session, &req, stream)?;

    Ok(resp)
}

/// WebSocket session for log streaming
pub struct WsLogSession {
    /// Unique connection ID
    id: String,
    /// Last heartbeat time
    hb: Instant,
    /// Application state
    state: web::Data<AppState>,
}

impl WsLogSession {
    pub fn new(state: web::Data<AppState>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            hb: Instant::now(),
            state,
        }
    }

    /// Helper method to start heartbeat process
    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            // Check client heartbeat
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                tracing::warn!("WebSocket log client heartbeat failed, disconnecting");
                ctx.stop();
                return;
            }

            ctx.ping(b"");
        });
    }
}

impl Actor for WsLogSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!("WebSocket log connection started: {}", self.id);

        // Start heartbeat
        self.hb(ctx);

        // Subscribe to log entries
        let log_buffer = self.state.log_buffer.clone();
        let addr = ctx.address();

        ctx.spawn(
            async move {
                let mut log_rx = {
                    let buffer = log_buffer.read().await;
                    buffer.subscribe()
                };

                while let Ok(entry) = log_rx.recv().await {
                    addr.do_send(LogEntryWrapper(entry));
                }
            }
            .into_actor(self)
            .map(|_, _, _| {}),
        );
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        tracing::info!("WebSocket log connection stopped: {}", self.id);
    }
}

/// Handle messages from the WebSocket client
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsLogSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Close(reason)) => {
                tracing::info!("WebSocket log close: {:?}", reason);
                ctx.stop();
            }
            _ => {}
        }
    }
}

/// Wrapper for log entries to be sent as actor messages
#[derive(ActixMessage)]
#[rtype(result = "()")]
struct LogEntryWrapper(crate::web::log_buffer::LogEntry);

/// Handle log entries from the log buffer
impl Handler<LogEntryWrapper> for WsLogSession {
    type Result = ();

    fn handle(&mut self, msg: LogEntryWrapper, ctx: &mut Self::Context) {
        if let Ok(json) = serde_json::to_string(&msg.0) {
            ctx.text(json);
        }
    }
}

/// WebSocket route handler for log streaming
pub async fn ws_logs(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let ws_session = WsLogSession::new(state);
    let resp = ws::start(ws_session, &req, stream)?;

    Ok(resp)
}
