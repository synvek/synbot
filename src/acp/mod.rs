//! ACP (Agent Client Protocol) adapter — exposes synbot as an ACP agent over stdio.
//!
//! Editors like Zed spawn `synbot acp` as a subprocess and speak JSON-RPC over
//! stdin/stdout. This module is a pure front-end adapter on top of the
//! [`MessageBus`](crate::bus::MessageBus):
//!
//! - `session/new` creates an ACP session mapped to the bus session `acp:<sessionId>`
//! - `session/prompt` is converted to an [`InboundMessage`] and held open until the
//!   agent run for that session finishes (signalled via [`HookEvent::AgentRunEnd`])
//! - Outbound `Chat` / `ToolProgress` messages are streamed back as `session/update`
//!   notifications
//! - `ApprovalRequest` messages are bridged to ACP `session/request_permission`
//!   round-trips, feeding the answer back into the [`ApprovalManager`]
//! - `session/cancel` is translated to synbot's `/stop` control command
//!
//! The agent core is never touched: this module only talks to the bus boundary.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use agent_client_protocol as acp;
use agent_client_protocol::schema::{
    AgentCapabilities, CancelNotification, ContentBlock, ContentChunk, Implementation,
    InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse, PermissionOption,
    PermissionOptionKind, PromptRequest, PromptResponse, ProtocolVersion,
    RequestPermissionOutcome, RequestPermissionRequest, SessionNotification, SessionUpdate,
    StopReason, ToolCall, ToolCallStatus, ToolKind,
};
use async_trait::async_trait;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::{debug, warn};

use crate::bus::{InboundMessage, OutboundMessage, OutboundMessageType};
use crate::hooks::{Hook, HookEvent};
use crate::tools::approval::{ApprovalManager, ApprovalResponse};

/// Channel name used for ACP sessions on the message bus.
pub const ACP_CHANNEL: &str = "acp";

/// Sender id used for inbound messages originating from the ACP client.
pub const ACP_SENDER: &str = "acp";

/// Grace period to drain trailing outbound messages after a turn ends, so
/// `session/update` notifications are delivered before the `session/prompt`
/// response resolves.
const TURN_DRAIN_MS: u64 = 200;

/// Permission option ids presented to ACP clients for approval requests.
const OPTION_ALLOW: &str = "allow";
const OPTION_REJECT: &str = "reject";

// ---------------------------------------------------------------------------
// Bridge state
// ---------------------------------------------------------------------------

type TurnWaiters = Arc<StdMutex<HashMap<String, oneshot::Sender<()>>>>;

/// Shared state connecting the ACP connection to the synbot message bus.
#[derive(Clone)]
pub struct AcpBridge {
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_tx: broadcast::Sender<OutboundMessage>,
    approvals: Arc<ApprovalManager>,
    /// ACP session id -> cancellation flag for the active turn.
    sessions: Arc<StdMutex<HashMap<String, Arc<AtomicBool>>>>,
    /// Bus session key ("acp:<sessionId>") -> turn-completion waiter.
    turn_waiters: TurnWaiters,
}

impl AcpBridge {
    pub fn new(
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_tx: broadcast::Sender<OutboundMessage>,
        approvals: Arc<ApprovalManager>,
    ) -> Self {
        Self {
            inbound_tx,
            outbound_tx,
            approvals,
            sessions: Arc::new(StdMutex::new(HashMap::new())),
            turn_waiters: Arc::new(StdMutex::new(HashMap::new())),
        }
    }

    /// Hook to register with the agent loop's `HookRegistry` so prompt turns can
    /// resolve when their agent run finishes.
    pub fn turn_hook(&self) -> Arc<dyn Hook> {
        Arc::new(AcpTurnHook {
            turn_waiters: Arc::clone(&self.turn_waiters),
        })
    }

    fn register_session(&self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.sessions
            .lock()
            .unwrap()
            .insert(id.clone(), Arc::new(AtomicBool::new(false)));
        id
    }

    fn session_cancel_flag(&self, session_id: &str) -> Option<Arc<AtomicBool>> {
        self.sessions.lock().unwrap().get(session_id).cloned()
    }

    fn register_turn_waiter(&self, session_key: &str) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        self.turn_waiters
            .lock()
            .unwrap()
            .insert(session_key.to_string(), tx);
        rx
    }

    fn remove_turn_waiter(&self, session_key: &str) {
        self.turn_waiters.lock().unwrap().remove(session_key);
    }

    fn inbound_message(&self, session_id: &str, content: String) -> InboundMessage {
        InboundMessage {
            channel: ACP_CHANNEL.to_string(),
            sender_id: ACP_SENDER.to_string(),
            chat_id: session_id.to_string(),
            content,
            timestamp: chrono::Utc::now(),
            media: vec![],
            metadata: serde_json::Value::Null,
        }
    }
}

/// Hook that resolves pending prompt turns when their agent run ends.
struct AcpTurnHook {
    turn_waiters: TurnWaiters,
}

#[async_trait]
impl Hook for AcpTurnHook {
    async fn on_event(&self, event: HookEvent) {
        if let HookEvent::AgentRunEnd { session_key, .. } = event {
            let waiter = self.turn_waiters.lock().unwrap().remove(&session_key);
            if let Some(tx) = waiter {
                let _ = tx.send(());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Message mapping (pure functions, unit-testable)
// ---------------------------------------------------------------------------

/// Flatten ACP prompt content blocks into the plain-text message synbot's agent
/// loop expects. Text and embedded resources are inlined; resource links are
/// referenced by URI. Image/audio blocks are not supported (the agent
/// advertises no image/audio prompt capabilities).
pub fn prompt_to_text(prompt: &[ContentBlock]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for block in prompt {
        match block {
            ContentBlock::Text(t) => parts.push(t.text.clone()),
            ContentBlock::ResourceLink(link) => parts.push(format!("[Resource: {}]", link.uri)),
            ContentBlock::Resource(res) => {
                match &res.resource {
                    acp::schema::EmbeddedResourceResource::TextResourceContents(text) => {
                        parts.push(format!(
                            "[Resource: {}]\n{}",
                            text.uri, text.text
                        ));
                    }
                    _ => debug!("Skipping non-text embedded resource in prompt"),
                }
            }
            _ => debug!("Skipping unsupported content block in prompt"),
        }
    }
    parts.join("\n\n")
}

/// Map an outbound chat message to an ACP agent message chunk update.
pub fn chat_to_session_update(content: String) -> SessionUpdate {
    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::from(content)))
}

/// Map an outbound tool progress message to an ACP tool call update.
///
/// Synbot's bus emits a single `ToolProgress` per tool execution (after the
/// tool finished), so each maps to one completed/failed tool call.
pub fn tool_progress_to_session_update(
    tool_name: &str,
    status: &str,
    result_preview: &str,
) -> SessionUpdate {
    let tool_call_id = format!("{}-{}", tool_name, uuid::Uuid::new_v4());
    let call_status = if status == "success" {
        ToolCallStatus::Completed
    } else {
        ToolCallStatus::Failed
    };
    let kind = if tool_name == "exec" {
        ToolKind::Execute
    } else {
        ToolKind::Other
    };
    let mut tool_call = ToolCall::new(tool_call_id, tool_name.to_string())
        .kind(kind)
        .status(call_status);
    if !result_preview.is_empty() {
        tool_call = tool_call.raw_output(serde_json::Value::String(result_preview.to_string()));
    }
    SessionUpdate::ToolCall(tool_call)
}

/// Map a synbot approval request to an ACP permission request.
pub fn approval_to_permission_request(
    acp_session_id: &str,
    request: &crate::tools::approval::ApprovalRequest,
) -> RequestPermissionRequest {
    let title = request
        .display_message
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("Run command: {}", request.command));
    let fields = acp::schema::ToolCallUpdateFields::new()
        .title(title)
        .kind(ToolKind::Execute)
        .status(ToolCallStatus::Pending)
        .raw_input(serde_json::json!({
            "command": request.command,
            "working_dir": request.working_dir,
            "context": request.context,
        }));
    let tool_call =
        acp::schema::ToolCallUpdate::new(format!("approval-{}", request.id), fields);
    RequestPermissionRequest::new(
        acp_session_id.to_string(),
        tool_call,
        vec![
            PermissionOption::new(OPTION_ALLOW, "Allow", PermissionOptionKind::AllowOnce),
            PermissionOption::new(OPTION_REJECT, "Reject", PermissionOptionKind::RejectOnce),
        ],
    )
}

/// Translate the client's permission outcome to a synbot approval response.
pub fn permission_outcome_to_approval(
    request_id: &str,
    outcome: &RequestPermissionOutcome,
) -> ApprovalResponse {
    let approved = match outcome {
        RequestPermissionOutcome::Selected(selected) => selected.option_id.0.as_ref() == OPTION_ALLOW,
        _ => false,
    };
    ApprovalResponse {
        request_id: request_id.to_string(),
        approved,
        responder: ACP_SENDER.to_string(),
        timestamp: chrono::Utc::now(),
    }
}

// ---------------------------------------------------------------------------
// Connection serving
// ---------------------------------------------------------------------------

/// Serve the ACP connection over stdio (stdin/stdout). Returns when the client
/// disconnects (stdin EOF) or a fatal connection error occurs.
pub async fn serve_stdio(bridge: AcpBridge) -> anyhow::Result<()> {
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
    serve(
        bridge,
        acp::ByteStreams::new(tokio::io::stdout().compat_write(), tokio::io::stdin().compat()),
    )
    .await
}

/// Serve the ACP connection over an arbitrary transport (used by tests with an
/// in-memory [`acp::Channel`]).
pub async fn serve(
    bridge: AcpBridge,
    transport: impl acp::ConnectTo<acp::Agent>,
) -> anyhow::Result<()> {
    let new_session_bridge = bridge.clone();
    let prompt_bridge = bridge.clone();
    let cancel_bridge = bridge;

    acp::Agent
        .builder()
        .name("synbot")
        .on_receive_request(
            async move |request: InitializeRequest, responder, _cx| {
                let version = if request.protocol_version > ProtocolVersion::LATEST {
                    ProtocolVersion::LATEST
                } else {
                    request.protocol_version
                };
                responder.respond(
                    InitializeResponse::new(version)
                        .agent_capabilities(AgentCapabilities::new())
                        .agent_info(Implementation::new("synbot", env!("CARGO_PKG_VERSION"))),
                )
            },
            acp::on_receive_request!(),
        )
        .on_receive_request(
            async move |_request: NewSessionRequest, responder, _cx| {
                let session_id = new_session_bridge.register_session();
                debug!(session_id = %session_id, "ACP session created");
                responder.respond(NewSessionResponse::new(session_id))
            },
            acp::on_receive_request!(),
        )
        .on_receive_request(
            async move |request: PromptRequest, responder, cx| {
                prompt_bridge.start_prompt(request, responder, &cx)
            },
            acp::on_receive_request!(),
        )
        .on_receive_notification(
            async move |notification: CancelNotification, _cx| {
                cancel_bridge.handle_cancel(notification).await;
                Ok(())
            },
            acp::on_receive_notification!(),
        )
        .connect_to(transport)
        .await
        .map_err(|e| anyhow::anyhow!("ACP connection error: {e}"))
}

impl AcpBridge {
    /// Handle `session/prompt`: spawn the turn task and respond when the agent
    /// run finishes. The responder is moved into the spawned task so the event
    /// loop keeps processing messages (e.g. `session/cancel`) meanwhile.
    fn start_prompt(
        &self,
        request: PromptRequest,
        responder: acp::Responder<PromptResponse>,
        cx: &acp::ConnectionTo<acp::Client>,
    ) -> Result<(), acp::Error> {
        let session_id = request.session_id.0.to_string();
        let Some(cancelled) = self.session_cancel_flag(&session_id) else {
            return responder.respond_with_error(
                acp::Error::invalid_params().data(format!("unknown session id: {session_id}")),
            );
        };
        cancelled.store(false, Ordering::SeqCst);

        let text = prompt_to_text(&request.prompt);
        if text.trim().is_empty() {
            return responder
                .respond_with_error(acp::Error::invalid_params().data("empty prompt"));
        }

        // Control commands (e.g. "/status") are answered by the agent loop with a
        // single chat message and never start an agent run, so the turn resolves
        // on the first reply instead of waiting for AgentRunEnd.
        let is_control = crate::agent::control_commands::parse_control_command(&text).is_some();

        let session_key = format!("{}:{}", ACP_CHANNEL, session_id);
        let turn_rx = self.register_turn_waiter(&session_key);
        // Subscribe before sending the inbound message so no outbound reply is missed.
        let outbound_rx = self.outbound_tx.subscribe();

        let bridge = self.clone();
        let cx_for_task = cx.clone();
        cx.spawn(async move {
            bridge
                .run_prompt_turn(
                    session_id,
                    session_key,
                    text,
                    is_control,
                    cancelled,
                    turn_rx,
                    outbound_rx,
                    responder,
                    cx_for_task,
                )
                .await;
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_prompt_turn(
        &self,
        session_id: String,
        session_key: String,
        text: String,
        is_control: bool,
        cancelled: Arc<AtomicBool>,
        mut turn_rx: oneshot::Receiver<()>,
        mut outbound_rx: broadcast::Receiver<OutboundMessage>,
        responder: acp::Responder<PromptResponse>,
        cx: acp::ConnectionTo<acp::Client>,
    ) {
        if self
            .inbound_tx
            .send(self.inbound_message(&session_id, text))
            .await
            .is_err()
        {
            self.remove_turn_waiter(&session_key);
            let _ = responder
                .respond_with_error(acp::Error::internal_error().data("agent loop unavailable"));
            return;
        }

        let stop_reason = loop {
            tokio::select! {
                _ = &mut turn_rx => {
                    // Agent run finished: deliver any trailing updates before resolving.
                    self.drain_outbound(&mut outbound_rx, &session_id, &cx).await;
                    break if cancelled.load(Ordering::SeqCst) {
                        StopReason::Cancelled
                    } else {
                        StopReason::EndTurn
                    };
                }
                out = outbound_rx.recv() => {
                    match out {
                        Ok(msg) => {
                            if msg.channel != ACP_CHANNEL || msg.chat_id != session_id {
                                continue;
                            }
                            if let Some(stop) = self.handle_outbound(msg, &session_id, is_control, &cancelled, &cx) {
                                break stop;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            warn!(skipped, "ACP outbound subscriber lagged; some updates were dropped");
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            break StopReason::EndTurn;
                        }
                    }
                }
            }
        };

        self.remove_turn_waiter(&session_key);
        let _ = responder.respond(PromptResponse::new(stop_reason));
    }

    /// Forward one outbound bus message to the ACP client. Returns a stop reason
    /// when the message implies the turn is over.
    fn handle_outbound(
        &self,
        msg: OutboundMessage,
        session_id: &str,
        is_control: bool,
        cancelled: &Arc<AtomicBool>,
        cx: &acp::ConnectionTo<acp::Client>,
    ) -> Option<StopReason> {
        match msg.message_type {
            OutboundMessageType::Chat { content, .. } => {
                let from_loop_control = content.starts_with("[Control]");
                self.send_session_update(cx, session_id, chat_to_session_update(content));
                if is_control {
                    // Control prompts get exactly one direct reply and no agent run.
                    Some(StopReason::EndTurn)
                } else if from_loop_control {
                    // Direct loop replies ("[Control] Stopped.", "[Control] Busy. ...")
                    // are sent outside an agent run; no AgentRunEnd will follow.
                    Some(if cancelled.load(Ordering::SeqCst) {
                        StopReason::Cancelled
                    } else {
                        StopReason::EndTurn
                    })
                } else {
                    None
                }
            }
            OutboundMessageType::ToolProgress {
                tool_name,
                status,
                result_preview,
            } => {
                self.send_session_update(
                    cx,
                    session_id,
                    tool_progress_to_session_update(&tool_name, &status, &result_preview),
                );
                None
            }
            OutboundMessageType::ApprovalRequest { request } => {
                let permission_request = approval_to_permission_request(session_id, &request);
                let approvals = Arc::clone(&self.approvals);
                let request_id = request.id.clone();
                let spawn_result = cx.spawn({
                    let cx = cx.clone();
                    async move {
                        let result = cx.send_request(permission_request).block_task().await;
                        let response = match &result {
                            Ok(resp) => permission_outcome_to_approval(&request_id, &resp.outcome),
                            Err(e) => {
                                warn!(error = %e, "ACP permission request failed; rejecting");
                                ApprovalResponse {
                                    request_id: request_id.clone(),
                                    approved: false,
                                    responder: ACP_SENDER.to_string(),
                                    timestamp: chrono::Utc::now(),
                                }
                            }
                        };
                        if let Err(e) = approvals.submit_response(response).await {
                            warn!(error = %e, "Failed to submit approval response");
                        }
                        Ok(())
                    }
                });
                if let Err(e) = spawn_result {
                    warn!(error = %e, "Failed to spawn ACP permission round-trip");
                }
                None
            }
        }
    }

    /// Forward trailing outbound messages for a short grace period after the
    /// turn ended, so streamed updates are not lost to the prompt response race.
    async fn drain_outbound(
        &self,
        outbound_rx: &mut broadcast::Receiver<OutboundMessage>,
        session_id: &str,
        cx: &acp::ConnectionTo<acp::Client>,
    ) {
        let drain_window = std::time::Duration::from_millis(TURN_DRAIN_MS);
        loop {
            match tokio::time::timeout(drain_window, outbound_rx.recv()).await {
                Ok(Ok(msg)) => {
                    if msg.channel != ACP_CHANNEL || msg.chat_id != session_id {
                        continue;
                    }
                    match msg.message_type {
                        OutboundMessageType::Chat { content, .. } => {
                            self.send_session_update(cx, session_id, chat_to_session_update(content));
                        }
                        OutboundMessageType::ToolProgress {
                            tool_name,
                            status,
                            result_preview,
                        } => {
                            self.send_session_update(
                                cx,
                                session_id,
                                tool_progress_to_session_update(&tool_name, &status, &result_preview),
                            );
                        }
                        // Approvals after the run ended cannot be answered meaningfully.
                        OutboundMessageType::ApprovalRequest { .. } => {}
                    }
                }
                _ => break,
            }
        }
    }

    fn send_session_update(
        &self,
        cx: &acp::ConnectionTo<acp::Client>,
        session_id: &str,
        update: SessionUpdate,
    ) {
        let notification = SessionNotification::new(session_id.to_string(), update);
        if let Err(e) = cx.send_notification(notification) {
            warn!(error = %e, "Failed to send ACP session update");
        }
    }

    async fn handle_cancel(&self, notification: CancelNotification) {
        let session_id = notification.session_id.0.to_string();
        if let Some(flag) = self.session_cancel_flag(&session_id) {
            flag.store(true, Ordering::SeqCst);
        }
        debug!(session_id = %session_id, "ACP cancel received; sending /stop");
        let _ = self
            .inbound_tx
            .send(self.inbound_message(&session_id, "/stop".to_string()))
            .await;
    }
}

#[cfg(test)]
mod tests;
