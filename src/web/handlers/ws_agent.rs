//! `/ws/agent` — streaming agent protocol (v2).
//!
//! v1 (`/ws/chat`) stays as-is for older clients: it forwards final chat messages and coarse tool
//! progress. This endpoint exposes the [`crate::agent::events::EventBus`] instead, so the UI can
//! render token deltas, tool lifecycles and turn boundaries, and replay what it missed after a
//! reconnect (`seq` based).
//!
//! Wire format (JSON text frames):
//!
//! The conversation is chosen by the socket URL: `/ws/agent?chat_id=w-…`. Reading it from the
//! query string (instead of a `hello` field) means the event filter, replay buffer and history
//! loader are bound before the first frame arrives. Omitting it keeps the pre-multi-session
//! behaviour and binds the legacy `web_admin` conversation.
//!
//! Client → server
//! - `{"type":"hello","after_seq":0}` — subscribe, replay buffered events with `seq > after_seq`
//! - `{"type":"chat","content":"...","model":{"provider":"...","model":"..."}}` — `model` is optional (one turn)
//! - `{"type":"cancel"}` — delivers `/stop` for this conversation
//! - `{"type":"plan_mode","enabled":true}` — next turn only reads, then proposes a plan
//! - `{"type":"plan_decision","approve":true,"plan_id":"…"}` — run or drop the pending plan
//! - `{"type":"select_project","project_id":"..."}` — work inside that folder (`null` unbinds)
//! - `{"type":"approval_response","request_id":"...","approved":true}`
//! - `{"type":"ping"}`
//!
//! Server → client
//! - `{"type":"hello","session_id":"...","chat_id":"...","last_seq":12}`
//! - `{"type":"project","project":{...}}` — active project (sent on connect and after switching)
//! - `{"type":"plan","plan":{...}}` — the plan waiting for approval (`null` when there is none)
//! - `{"type":"history","chat_id":"...","messages":[...]}` — same shape as v1, sent once on connect
//! - `{"type":"replay","events":[...]}` — missed events, oldest first
//! - `{"type":"event","seq":13,"channel":"web","chat_id":"web_admin","event":{"type":"message_delta",...}}`
//! - `{"type":"event",...,"event":{"type":"notice","message":"[Clear] Session cleared."}}` —
//!   out-of-band text for this conversation: control-command replies (`/clear`, `/stop`, `/tools`, …)
//!   and workflow progress (`[Workflow] …`), neither of which belongs to a turn
//! - `{"type":"event",...,"event":{"type":"turn_skipped"}}` — the message started no turn, so no
//!   `turn_finished` follows (workflow pending input, save-only message); release the composer
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
use crate::web::handlers::api::{persist_session_meta, web_session_id};
use crate::web::handlers::ws::HistoryMessage;
use crate::web::state::AppState;

/// The web UI authenticates as one admin user; the conversation is a separate dimension.
const WEB_USER_ID: &str = "web_admin";
/// Conversation id used by every build before multi-session support, and by clients that do not
/// pass `chat_id`. Its history is kept as-is so an upgrade never loses a conversation.
const LEGACY_WEB_CHAT_ID: &str = "web_admin";
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// New console conversation ids look like `w-0123456789ab`: URL-safe, no collision with IM chat
/// ids, and cheap to validate before it is used as a session identifier or a file name.
pub fn is_valid_chat_id(id: &str) -> bool {
    if id == LEGACY_WEB_CHAT_ID {
        return true;
    }
    match id.strip_prefix("w-") {
        Some(rest) => {
            rest.len() == 12
                && rest
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        }
        None => false,
    }
}

/// Mint a new conversation id for `POST /api/sessions`.
pub fn new_chat_id() -> String {
    format!("w-{}", &Uuid::new_v4().simple().to_string()[..12])
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentClientMessage {
    Hello {
        #[serde(default)]
        after_seq: u64,
    },
    Chat {
        content: String,
        /// Per-turn model choice. Nothing is persisted here: the console stores the session-level
        /// default separately and sends it with every message.
        #[serde(default)]
        model: Option<crate::agent::session_manager::ModelOverride>,
    },
    Cancel,
    /// Plan-first mode: the next turn may only read, then stops with a plan.
    PlanMode {
        enabled: bool,
    },
    /// Answer the pending plan: `approve` runs it, `cancel` drops it. `plan_id` guards against a
    /// stale card (another tab already decided).
    PlanDecision {
        approve: bool,
        #[serde(default)]
        plan_id: Option<String>,
    },
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
        chat_id: String,
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
        /// Set when another surface (phone, another tab) answered or the request timed out.
        #[serde(skip_serializing_if = "Option::is_none")]
        responder: Option<String>,
    },
    Pong,
    /// Active project after `select_project` or on connect; `null` means the global workspace.
    Project {
        project: Option<crate::project::Project>,
    },
    /// Plan waiting for approval in this conversation; sent on connect so a reload restores the
    /// card even when its `plan_proposed` event has left the replay buffer.
    Plan {
        plan: Option<crate::agent::session_manager::PendingPlan>,
    },
    Error {
        message: String,
        /// Machine-readable reason (`invalid_chat_id`, …).
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
}

pub struct AgentWsSession {
    id: String,
    /// Authenticated principal. Kept separate from `chat_id` so approvals and audit trails still
    /// name the operator when several conversations run side by side.
    user_id: String,
    /// Conversation this socket is bound to.
    chat_id: String,
    /// Set when the URL asked for an id that failed validation, so the client learns why it got
    /// the legacy conversation instead.
    invalid_chat_id: Option<String>,
    hb: Instant,
    state: web::Data<AppState>,
    web_channel: Option<WebChannel>,
}

impl AgentWsSession {
    pub fn new(
        user_id: String,
        requested_chat_id: Option<&str>,
        state: web::Data<AppState>,
    ) -> Self {
        let (chat_id, invalid_chat_id) = match requested_chat_id {
            None => (LEGACY_WEB_CHAT_ID.to_string(), None),
            Some(raw) if is_valid_chat_id(raw) => (raw.to_string(), None),
            Some(raw) => (LEGACY_WEB_CHAT_ID.to_string(), Some(raw.to_string())),
        };
        Self {
            id: Uuid::new_v4().to_string(),
            user_id,
            chat_id,
            invalid_chat_id,
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
                code: None,
            },
        );
    }

    fn send_error_with_code(
        &self,
        ctx: &mut ws::WebsocketContext<Self>,
        message: impl Into<String>,
        code: &str,
    ) {
        self.send_message(
            ctx,
            AgentServerMessage::Error {
                message: message.into(),
                code: Some(code.to_string()),
            },
        );
    }

    /// Deliver `after_seq..` from the ring buffer (only what the client missed).
    fn send_replay(&self, ctx: &mut ws::WebsocketContext<Self>, after_seq: u64) {
        let missed: Vec<EventEnvelope> = self
            .state
            .events
            .replay_from("web", &self.chat_id, after_seq)
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
                chat_id: self.chat_id.clone(),
                last_seq: self.state.events.last_seq(),
            },
        );
    }

    /// Send the project this conversation currently works in (on connect and after a switch).
    fn send_project(&self, ctx: &mut ws::WebsocketContext<Self>) {
        let projects = self.state.projects.clone();
        let chat_id = self.chat_id.clone();
        ctx.spawn(
            async move {
                let store = projects.read().await;
                store.project_for_chat(&chat_id)
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
        let chat_id = self.chat_id.clone();
        ctx.spawn(
            async move {
                let mut store = projects.write().await;
                store.bind(&chat_id, project_id.as_deref())
            }
            .into_actor(self)
            .map(|result, actor, ctx| match result {
                Ok(project) => actor.send_message(ctx, AgentServerMessage::Project { project }),
                Err(e) => actor.send_error(ctx, format!("Could not select project: {e:#}")),
            }),
        );
    }

    /// Send the plan awaiting a decision (on connect). A reload therefore restores the card even
    /// when its `plan_proposed` event has already left the replay buffer.
    fn send_plan(&self, ctx: &mut ws::WebsocketContext<Self>) {
        let state = self.state.clone();
        let session_id = web_session_id(&self.chat_id);
        ctx.spawn(
            async move {
                let sm = state.session_manager.read().await;
                sm.get_meta(&session_id)
                    .and_then(|meta| meta.pending_plan.clone())
            }
            .into_actor(self)
            .map(|plan, actor, ctx| {
                actor.send_message(ctx, AgentServerMessage::Plan { plan });
            }),
        );
    }

    /// Toggle plan-first mode for this conversation. Nothing runs until the next message, so the
    /// console only needs confirmation that the flag was written.
    fn set_plan_mode(&self, ctx: &mut ws::WebsocketContext<Self>, enabled: bool) {
        let state = self.state.clone();
        let session_id = web_session_id(&self.chat_id);
        let chat_id = self.chat_id.clone();
        ctx.spawn(
            async move {
                {
                    let mut sm = state.session_manager.write().await;
                    sm.get_or_create(&session_id);
                    sm.set_plan_mode(&session_id, enabled);
                }
                if let Err(e) = persist_session_meta(&state, &session_id).await {
                    tracing::warn!(error = %e, "Failed to persist plan mode");
                }
                if enabled {
                    state.events.publish(
                        "web",
                        &chat_id,
                        AgentEvent::Notice {
                            message: "[Plan] Plan mode on: the next turn only reads, then proposes a plan."
                                .to_string(),
                        },
                    );
                } else {
                    state.events.publish(
                        "web",
                        &chat_id,
                        AgentEvent::Notice {
                            message: "[Plan] Plan mode off.".to_string(),
                        },
                    );
                }
            }
            .into_actor(self)
            .map(|_, _, _| {}),
        );
    }

    /// Answer the pending plan. The decision travels as an inbound message so it goes through the
    /// same loop lock, persistence and turn tracking as a chat message — no second concurrency path.
    fn submit_plan_decision(
        &self,
        ctx: &mut ws::WebsocketContext<Self>,
        approve: bool,
        plan_id: Option<String>,
    ) {
        let metadata = serde_json::json!({
            "plan_decision": if approve { "approve" } else { "cancel" },
            "plan_id": plan_id,
        });
        let inbound_msg = InboundMessage {
            channel: "web".to_string(),
            sender_id: self.user_id.clone(),
            chat_id: self.chat_id.clone(),
            // On approval the loop synthesises the instruction from the stored plan.
            content: String::new(),
            timestamp: Utc::now(),
            media: vec![],
            metadata,
        };
        let inbound_tx = self.state.inbound_tx.clone();
        ctx.spawn(
            async move {
                if let Err(e) = inbound_tx.send(inbound_msg).await {
                    tracing::error!("Failed to send plan decision: {}", e);
                }
            }
            .into_actor(self)
            .map(|_, _, _| {}),
        );
    }

    fn submit_chat(
        &self,
        ctx: &mut ws::WebsocketContext<Self>,
        content: String,
        model: Option<crate::agent::session_manager::ModelOverride>,
    ) {
        let inbound_msg = InboundMessage {
            channel: "web".to_string(),
            sender_id: self.user_id.clone(),
            chat_id: self.chat_id.clone(),
            content,
            timestamp: Utc::now(),
            media: vec![],
            // A per-turn model choice travels with the message so the agent loop does not need to
            // read console state; everything else stays null for control commands.
            metadata: match model {
                Some(model) => serde_json::json!({ "model": model }),
                None => serde_json::Value::Null,
            },
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
            chat_id: self.chat_id.clone(),
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
        let chat_id = self.chat_id.clone();
        let addr = ctx.address();
        ctx.spawn(
            async move {
                loop {
                    match event_rx.recv().await {
                        Ok(ev) => {
                            if ev.channel == "web" && ev.chat_id == chat_id {
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
        let chat_id = self.chat_id.clone();
        let addr = ctx.address();
        ctx.spawn(
            async move {
                while let Ok(msg) = outbound_rx.recv().await {
                    if msg.channel == "web" && msg.chat_id == chat_id {
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
        self.send_plan(ctx);
        if let Some(raw) = self.invalid_chat_id.clone() {
            self.send_error_with_code(
                ctx,
                format!("Invalid chat_id {raw:?}; falling back to the legacy conversation"),
                "invalid_chat_id",
            );
        }

        // History is unchanged from v1 so the UI can keep one loader.
        let state = self.state.clone();
        let chat_id = self.chat_id.clone();
        let addr = ctx.address();
        ctx.spawn(
            async move {
                let messages = load_history(&state, &chat_id).await;
                addr.do_send(HistoryWrapper { chat_id, messages });
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
                    Ok(AgentClientMessage::Chat { content, model }) => {
                        self.submit_chat(ctx, content, model)
                    }
                    Ok(AgentClientMessage::Cancel) => self.submit_cancel(ctx),
                    Ok(AgentClientMessage::PlanMode { enabled }) => {
                        self.set_plan_mode(ctx, enabled)
                    }
                    Ok(AgentClientMessage::PlanDecision { approve, plan_id }) => {
                        self.submit_plan_decision(ctx, approve, plan_id)
                    }
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
                                        responder: None,
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

/// Build the same history the v1 socket sends: every session of `web/<chat_id>` merged by timestamp.
async fn load_history(state: &AppState, chat_id: &str) -> Vec<HistoryMessage> {
    use crate::agent::session_id::SessionScope;

    let sm = state.session_manager.read().await;
    let sessions = sm.get_sessions_for_channel("web", SessionScope::Dm, chat_id);
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
struct HistoryWrapper {
    chat_id: String,
    messages: Vec<HistoryMessage>,
}

impl Handler<HistoryWrapper> for AgentWsSession {
    type Result = ();

    fn handle(&mut self, msg: HistoryWrapper, ctx: &mut Self::Context) {
        self.send_message(
            ctx,
            AgentServerMessage::History {
                chat_id: msg.chat_id,
                messages: msg.messages,
            },
        );
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
        match msg.0.message_type {
            OutboundMessageType::ApprovalRequest { request } => {
                self.send_message(ctx, AgentServerMessage::ApprovalRequest { request });
            }
            // Answered on another surface: retire the card here instead of leaving it clickable.
            OutboundMessageType::ApprovalResolved { receipt } => {
                let message = match &receipt.outcome {
                    crate::bus::ApprovalReceiptOutcome::Approved { .. } => "approved",
                    crate::bus::ApprovalReceiptOutcome::Rejected { .. } => "rejected",
                    crate::bus::ApprovalReceiptOutcome::TimedOut => "timeout",
                };
                self.send_message(
                    ctx,
                    AgentServerMessage::ApprovalResult {
                        request_id: receipt.request_id.clone(),
                        approved: receipt.approved(),
                        message: message.to_string(),
                        responder: receipt.responder().map(String::from),
                    },
                );
            }
            _ => {}
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct WsAgentQuery {
    /// Conversation to bind this socket to. Absent = the legacy `web_admin` conversation.
    pub chat_id: Option<String>,
}

pub async fn ws_agent(
    req: HttpRequest,
    stream: web::Payload,
    query: web::Query<WsAgentQuery>,
    state: web::Data<AppState>,
) -> Result<HttpResponse, Error> {
    let session = AgentWsSession::new(WEB_USER_ID.to_string(), query.chat_id.as_deref(), state);
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

    #[test]
    fn chat_ids_are_validated_before_use() {
        assert!(is_valid_chat_id("w-0123456789ab"));
        assert!(is_valid_chat_id("web_admin"));
        assert!(!is_valid_chat_id("w-0123456789AB"));
        assert!(!is_valid_chat_id("w-0123456789a"));
        assert!(!is_valid_chat_id("w-../../etc/passwd"));
        assert!(!is_valid_chat_id("chat_1"));
    }

    #[test]
    fn new_chat_ids_are_unique_and_well_formed() {
        let a = new_chat_id();
        let b = new_chat_id();
        assert!(is_valid_chat_id(&a));
        assert!(is_valid_chat_id(&b));
        assert_ne!(a, b);
    }

    #[test]
    fn chat_message_carries_optional_model_override() {
        let plain: AgentClientMessage =
            serde_json::from_str(r#"{"type":"chat","content":"hi"}"#).unwrap();
        match plain {
            AgentClientMessage::Chat { model, .. } => assert!(model.is_none()),
            other => panic!("unexpected variant: {other:?}"),
        }

        let with_model: AgentClientMessage = serde_json::from_str(
            r#"{"type":"chat","content":"hi","model":{"provider":"openai","model":"gpt-4o-mini"}}"#,
        )
        .unwrap();
        match with_model {
            AgentClientMessage::Chat { model, .. } => {
                let model = model.expect("model must parse");
                assert_eq!(model.provider, "openai");
                assert_eq!(model.model, "gpt-4o-mini");
            }
            other => panic!("unexpected variant: {other:?}"),
        }
    }
}
