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
    Error {
        message: String,
    },
    Pong,
    Connected {
        session_id: String,
    },
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
                    // Filter messages for this user's chat_id
                    if msg.chat_id == user_id {
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
        let server_msg = WsServerMessage::ChatResponse {
            content: msg.0.content,
            timestamp: Utc::now(),
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
    // TODO: Extract user_id from authentication
    // For now, generate a random user ID
    let user_id = Uuid::new_v4().to_string();

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
