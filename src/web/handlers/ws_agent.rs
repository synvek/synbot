//! `/ws/agent` — streaming agent protocol (v2).
//!
//! v1 (`/ws/chat`) stays as-is for older clients: it forwards final chat messages and coarse tool
//! progress. This endpoint exposes the [`crate::agent::events::EventBus`] instead, so the UI can
//! render token deltas, tool lifecycles and turn boundaries, and replay what it missed after a
//! reconnect (`seq` based).
//!
//! Wire format (JSON text frames):
//!
//! Client → server
//! - `{"type":"hello","after_seq":0}` — subscribe, replay buffered events with `seq > after_seq`
//! - `{"type":"chat","content":"..."}`
//! - `{"type":"cancel"}` — delivers `/stop` for this conversation
//! - `{"type":"select_project","project_id":"..."}` — work inside that folder (`null` unbinds)
//! - `{"type":"approval_response","request_id":"...","approved":true}`
//! - `{"type":"ping"}`
//!
//! Server → client
//! - `{"type":"hello","session_id":"...","chat_id":"...","last_seq":12}`
//! - `{"type":"project","project":{...}}` — active project (sent on connect and after switching)
//! - `{"type":"history","messages":[...]}` — same shape as v1, sent once on connect
//! - `{"type":"replay","events":[...]}` — missed events, oldest first
//! - `{"type":"event","seq":13,"channel":"web","chat_id":"web_admin","event":{"type":"message_delta",...}}`
//! - `{"type":"approval_request","request":{...}}`
//! - `{"type":"approval_result","request_id":"...","approved":true,"message":"approved"}`
//! - `{"type":"pong"}` / `{"type":"error","message":"..."}`

use actix::{
    Actor, ActorContext, ActorFutureExt, AsyncContext, Handler, Message as ActixMessage,
    StreamHandler, WrapFuture,
};
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::agent::events::{AgentEvent, SequencedEvent};
use crate::bus::{InboundMessage, OutboundMessage, OutboundMessageType};
use crate::web::channel::WebChannel;
use crate::web::handlers::ws::HistoryMessage;
use crate::web::state::AppState;

/// The web UI is a single global admin session, matching `/ws/chat`.
const WEB_USER_ID: &str = "web_admin";
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentClientMessage {
    Hello {
        #[serde(default)]
        after_seq: u64,
    },
    Chat {
        content: String,
    },
    Cancel,
    /// Bind this conversation to a project (`None` = back to the global workspace).
    SelectProject {
        project_id: Option<String>,
    },
    ApprovalResponse {
        request_id: String,
        approved: bool,
    },
    Ping,
}

/// Event envelope mirroring [`SequencedEvent`], kept separate so the wire shape stays stable even
/// if the internal struct changes.
#[derive(Debug, Clone, Serialize)]
pub struct EventEnvelope {
    pub seq: u64,
    pub channel: String,
    pub chat_id: String,
    pub event: AgentEvent,
}

impl From<SequencedEvent> for EventEnvelope {
    fn from(value: SequencedEvent) -> Self {
        Self {
            seq: value.seq,
            channel: value.channel,
            chat_id: value.chat_id,
            event: value.event,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentServerMessage {
    Hello {
        session_id: String,
        chat_id: String,
        last_seq: u64,
    },
    History {
        messages: Vec<HistoryMessage>,
    },
    Replay {
        events: Vec<EventEnvelope>,
    },
    Event {
        seq: u64,
        channel: String,
        chat_id: String,
        event: AgentEvent,
    },
    ApprovalRequest {
        request: crate::tools::approval::ApprovalRequest,
    },
    ApprovalResult {
        request_id: String,
        approved: bool,
        message: String,
    },
    Pong,
    /// Active project after `select_project` or on connect; `null` means the global workspace.
    Project {
        project: Option<crate::project::Project>,
    },
    Error {
        message: String,
    },
}

pub struct AgentWsSession {
    id: String,
    user_id: String,
    hb: Instant,
    state: web::Data<AppState>,
    web_channel: Option<WebChannel>,
}

impl AgentWsSession {
    pub fn new(user_id: String, state: web::Data<AppState>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            user_id,
            hb: Instant::now(),
            state,
            web_channel: None,
        }
    }

    fn hb(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                tracing::warn!("Agent WebSocket client heartbeat failed, disconnecting");
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }

    fn send_message(&self, ctx: &mut ws::WebsocketContext<Self>, msg: AgentServerMessage) {
        if let Ok(json) = serde_json::to_string(&msg) {
            ctx.text(json);
        }
    }

    fn send_error(&self, ctx: &mut ws::WebsocketContext<Self>, message: impl Into<String>) {
        self.send_message(
            ctx,
            AgentServerMessage::Error {
                message: message.into(),
            },
        );
    }

    /// Deliver `after_seq..` from the ring buffer (only what the client missed).
    fn send_replay(&self, ctx: &mut ws::WebsocketContext<Self>, after_seq: u64) {
        let missed: Vec<EventEnvelope> = self
            .state
            .events
            .replay_from("web", &self.user_id, after_seq)
            .into_iter()
            .map(EventEnvelope::from)
            .collect();
        if !missed.is_empty() {
            self.send_message(ctx, AgentServerMessage::Replay { events: missed });
        }
    }

    fn send_hello(&self, ctx: &mut ws::WebsocketContext<Self>) {
        self.send_message(
            ctx,
            AgentServerMessage::Hello {
                session_id: self.id.clone(),
                chat_id: self.user_id.clone(),
                last_seq: self.state.events.last_seq(),
            },
        );
    }

    /// Send the project this conversation currently works in (on connect and after a switch).
    fn send_project(&self, ctx: &mut ws::WebsocketContext<Self>) {
        let projects = self.state.projects.clone();
        let user_id = self.user_id.clone();
        ctx.spawn(
            async move {
                let store = projects.read().await;
                store.project_for_chat(&user_id)
            }
            .into_actor(self)
            .map(|project, actor, ctx| {
                actor.send_message(ctx, AgentServerMessage::Project { project });
            }),
        );
    }

    /// Switch the workspace of this conversation. The agent reads the binding on the next turn, so
    /// an in-flight turn keeps running in the folder it started with.
    fn select_project(&self, ctx: &mut ws::WebsocketContext<Self>, project_id: Option<String>) {
        let projects = self.state.projects.clone();
        let user_id = self.user_id.clone();
        ctx.spawn(
            async move {
                let mut store = projects.write().await;
                store.bind(&user_id, project_id.as_deref())
            }
            .into_actor(self)
            .map(|result, actor, ctx| match result {
                Ok(project) => actor.send_message(ctx, AgentServerMessage::Project { project }),
                Err(e) => actor.send_error(ctx, format!("Could not select project: {e:#}")),
            }),
        );
    }

    fn submit_chat(&self, ctx: &mut ws::WebsocketContext<Self>, content: String) {
        let inbound_msg = InboundMessage {
            channel: "web".to_string(),
            sender_id: self.user_id.clone(),
            chat_id: self.user_id.clone(),
            content,
            timestamp: Utc::now(),
            media: vec![],
            metadata: serde_json::Value::Null,
        };
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

    /// Cancellation reuses the existing `/stop` control command, which the agent loop matches by
    /// `channel:chat_id` — no separate cancel registry needed.
    fn submit_cancel(&self, ctx: &mut ws::WebsocketContext<Self>) {
        let inbound_msg = InboundMessage {
            channel: "web".to_string(),
            sender_id: self.user_id.clone(),
            chat_id: self.user_id.clone(),
            content: "/stop".to_string(),
            timestamp: Utc::now(),
            media: vec![],
            metadata: serde_json::Value::Null,
        };
        let inbound_tx = self.state.inbound_tx.clone();
        ctx.spawn(
            async move {
                if let Err(e) = inbound_tx.send(inbound_msg).await {
                    tracing::error!("Failed to send /stop: {}", e);
                }
            }
            .into_actor(self)
            .map(|_, _, _| {}),
        );
    }
}

impl Actor for AgentWsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        tracing::info!(connection_id = %self.id, "Agent WebSocket connection started");
        self.hb(ctx);

        let web_channel = WebChannel::new(
            self.state.inbound_tx.clone(),
            self.state.outbound_tx.clone(),
        );
        self.web_channel = Some(web_channel.clone());

        // Live agent events (deltas + tool lifecycle) for this conversation.
        let mut event_rx = self.state.events.subscribe();
        let user_id = self.user_id.clone();
        let addr = ctx.address();
        ctx.spawn(
            async move {
                loop {
                    match event_rx.recv().await {
                        Ok(ev) => {
                            if ev.channel == "web" && ev.chat_id == user_id {
                                addr.do_send(AgentEventWrapper(ev));
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "Agent WebSocket lagged; client should replay");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
            .into_actor(self)
            .map(|_, _, _| {}),
        );

        // Approval requests still travel on the outbound bus (the approval manager owns them).
        let mut outbound_rx = web_channel.subscribe_outbound();
        let user_id = self.user_id.clone();
        let addr = ctx.address();
        ctx.spawn(
            async move {
                while let Ok(msg) = outbound_rx.recv().await {
                    if msg.channel == "web" && msg.chat_id == user_id {
                        addr.do_send(PassthroughMessage(msg));
                    }
                }
            }
            .into_actor(self)
            .map(|_, _, _| {}),
        );

        // `hello` advertises the current tail; a reconnecting client asks for `after_seq` to catch
        // up, while a fresh connection relies on `history` below (no replay, no duplicates).
        self.send_hello(ctx);
        self.send_project(ctx);

        // History is unchanged from v1 so the UI can keep one loader.
        let state = self.state.clone();
        let user_id = self.user_id.clone();
        let addr = ctx.address();
        ctx.spawn(
            async move {
                let messages = load_history(&state, &user_id).await;
                addr.do_send(HistoryWrapper(messages));
            }
            .into_actor(self)
            .map(|_, _, _| {}),
        );
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        tracing::info!(connection_id = %self.id, "Agent WebSocket connection stopped");
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

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for AgentWsSession {
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
                match serde_json::from_str::<AgentClientMessage>(&text) {
                    Ok(AgentClientMessage::Hello { after_seq }) => self.send_replay(ctx, after_seq),
                    Ok(AgentClientMessage::Chat { content }) => self.submit_chat(ctx, content),
                    Ok(AgentClientMessage::Cancel) => self.submit_cancel(ctx),
                    Ok(AgentClientMessage::SelectProject { project_id }) => {
                        self.select_project(ctx, project_id)
                    }
                    Ok(AgentClientMessage::ApprovalResponse {
                        request_id,
                        approved,
                    }) => {
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
                                actor.send_message(
                                    ctx,
                                    AgentServerMessage::ApprovalResult {
                                        request_id,
                                        approved,
                                        message: if approved { "approved" } else { "rejected" }
                                            .to_string(),
                                    },
                                );
                            }),
                        );
                    }
                    Ok(AgentClientMessage::Ping) => {
                        self.send_message(ctx, AgentServerMessage::Pong)
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse agent WebSocket message: {}", e);
                        self.send_error(ctx, format!("Invalid message format: {}", e));
                    }
                }
            }
            Ok(ws::Message::Binary(_)) => tracing::warn!("Binary messages not supported"),
            Ok(ws::Message::Close(reason)) => {
                tracing::info!("Agent WebSocket close: {:?}", reason);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

/// Build the same history the v1 socket sends: every session of `web/<user>` merged by timestamp.
async fn load_history(state: &AppState, user_id: &str) -> Vec<HistoryMessage> {
    use crate::agent::session_id::SessionScope;

    let sm = state.session_manager.read().await;
    let sessions = sm.get_sessions_for_channel("web", SessionScope::Dm, user_id);
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
    all.into_iter().map(|(_, m)| m).collect()
}

#[derive(ActixMessage)]
#[rtype(result = "()")]
struct AgentEventWrapper(SequencedEvent);

impl Handler<AgentEventWrapper> for AgentWsSession {
    type Result = ();

    fn handle(&mut self, msg: AgentEventWrapper, ctx: &mut Self::Context) {
        let SequencedEvent {
            seq,
            channel,
            chat_id,
            event,
        } = msg.0;
        self.send_message(
            ctx,
            AgentServerMessage::Event {
                seq,
                channel,
                chat_id,
                event,
            },
        );
    }
}

#[derive(ActixMessage)]
#[rtype(result = "()")]
struct HistoryWrapper(Vec<HistoryMessage>);

impl Handler<HistoryWrapper> for AgentWsSession {
    type Result = ();

    fn handle(&mut self, msg: HistoryWrapper, ctx: &mut Self::Context) {
        self.send_message(ctx, AgentServerMessage::History { messages: msg.0 });
    }
}

/// Only approvals are forwarded from the outbound bus: chat text and tool progress are already
/// covered by `AgentEvent`s, forwarding them would duplicate the transcript.
#[derive(ActixMessage)]
#[rtype(result = "()")]
struct PassthroughMessage(OutboundMessage);

impl Handler<PassthroughMessage> for AgentWsSession {
    type Result = ();

    fn handle(&mut self, msg: PassthroughMessage, ctx: &mut Self::Context) {
        if let OutboundMessageType::ApprovalRequest { request } = msg.0.message_type {
            self.send_message(ctx, AgentServerMessage::ApprovalRequest { request });
        }
    }
}

pub async fn ws_agent(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let session = AgentWsSession::new(WEB_USER_ID.to_string(), state);
    ws::start(session, &req, stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_project_message_accepts_id_and_null() {
        let with_id: AgentClientMessage =
            serde_json::from_str(r#"{"type":"select_project","project_id":"abc"}"#).unwrap();
        match with_id {
            AgentClientMessage::SelectProject { project_id } => {
                assert_eq!(project_id.as_deref(), Some("abc"))
            }
            other => panic!("unexpected variant: {other:?}"),
        }

        let unbind: AgentClientMessage =
            serde_json::from_str(r#"{"type":"select_project","project_id":null}"#).unwrap();
        match unbind {
            AgentClientMessage::SelectProject { project_id } => assert!(project_id.is_none()),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn project_message_serializes_as_tagged_payload() {
        let json = serde_json::to_value(AgentServerMessage::Project { project: None }).unwrap();
        assert_eq!(json["type"], "project");
        assert!(json["project"].is_null());
    }
}
