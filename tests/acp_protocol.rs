//! Integration tests for the ACP adapter: initialize -> session/new -> session/prompt
//! with streamed session/update notifications, plus the permission round-trip.
//!
//! The ACP connection runs over an in-memory channel pair. A fake agent loop
//! stands in for synbot's `AgentLoop`: it echoes inbound messages on the
//! outbound bus and fires the `AgentRunEnd` hook to finish the turn, exactly
//! like the real loop does at the bus boundary.

#![cfg(feature = "acp")]

use std::sync::{Arc, Mutex as StdMutex};

use agent_client_protocol as acp;
use agent_client_protocol::schema::{
    InitializeRequest, NewSessionRequest, PromptRequest, ProtocolVersion,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate, StopReason,
};

use synbot::acp::AcpBridge;
use synbot::bus::{InboundMessage, MessageBus, OutboundMessage};
use synbot::hooks::{Hook, HookEvent};
use synbot::tools::approval::{ApprovalManager, ApprovalOutcome};

/// Fake agent loop: echoes chat messages and resolves turns via the ACP hook,
/// mirroring the real `AgentLoop`'s behavior at the bus boundary.
async fn fake_agent_loop(
    mut inbound_rx: tokio::sync::mpsc::Receiver<InboundMessage>,
    outbound_tx: tokio::sync::broadcast::Sender<OutboundMessage>,
    approvals: Arc<ApprovalManager>,
    hook: Arc<dyn Hook>,
) {
    while let Some(msg) = inbound_rx.recv().await {
        let session_key = msg.session_key();
        if msg.content.contains("approve me") {
            // Simulate a tool requesting approval mid-run.
            let outcome = approvals
                .request_approval(
                    "agent:main:acp".to_string(),
                    msg.channel.clone(),
                    msg.chat_id.clone(),
                    "rm -rf /tmp/x".to_string(),
                    "/tmp".to_string(),
                    "integration test".to_string(),
                    5,
                    None,
                )
                .await
                .expect("request_approval failed");
            let text = match outcome {
                ApprovalOutcome::Approved => "approved",
                ApprovalOutcome::Rejected => "rejected",
                ApprovalOutcome::Timeout => "timeout",
            };
            let _ = outbound_tx.send(OutboundMessage::chat(
                msg.channel.clone(),
                msg.chat_id.clone(),
                text.to_string(),
                vec![],
                None,
            ));
        } else {
            let _ = outbound_tx.send(OutboundMessage::chat(
                msg.channel.clone(),
                msg.chat_id.clone(),
                format!("echo: {}", msg.content),
                vec![],
                None,
            ));
        }
        hook.on_event(HookEvent::AgentRunEnd {
            agent_id: "main".to_string(),
            iteration_count: 1,
            duration_ms: 1,
            session_key,
        })
        .await;
    }
}

struct Harness {
    bridge: AcpBridge,
    loop_task: tokio::task::JoinHandle<()>,
}

fn setup() -> Harness {
    let mut bus = MessageBus::new();
    let inbound_tx = bus.inbound_sender();
    let inbound_rx = bus.take_inbound_receiver().unwrap();
    let outbound_tx = bus.outbound_tx_clone();

    let approvals = Arc::new(ApprovalManager::with_outbound(outbound_tx.clone()));
    let bridge = AcpBridge::new(inbound_tx, outbound_tx.clone(), Arc::clone(&approvals));
    let hook = bridge.turn_hook();

    let loop_task = tokio::spawn(fake_agent_loop(inbound_rx, outbound_tx, approvals, hook));

    Harness { bridge, loop_task }
}

fn chunk_texts(updates: &[SessionNotification]) -> Vec<String> {
    updates
        .iter()
        .filter_map(|n| match &n.update {
            SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
                acp::schema::ContentBlock::Text(t) => Some(t.text.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn initialize_new_session_prompt_and_end_turn() {
    let harness = setup();
    let (client_side, agent_side) = acp::Channel::duplex();

    let agent_task = tokio::spawn(synbot::acp::serve(harness.bridge.clone(), agent_side));

    let updates: Arc<StdMutex<Vec<SessionNotification>>> = Arc::default();
    let updates_in_handler = Arc::clone(&updates);

    let client_result = acp::Client
        .builder()
        .name("test-client")
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                updates_in_handler.lock().unwrap().push(notification);
                Ok(())
            },
            acp::on_receive_notification!(),
        )
        .connect_with(client_side, async |cx| {
            // initialize
            let init = cx
                .send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;
            assert_eq!(init.protocol_version, ProtocolVersion::V1);
            assert_eq!(
                init.agent_info.as_ref().map(|i| i.name.as_str()),
                Some("synbot")
            );
            assert!(init.auth_methods.is_empty());

            // session/new
            let session = cx
                .send_request(NewSessionRequest::new("/tmp"))
                .block_task()
                .await?;
            let session_id = session.session_id.clone();

            // session/prompt
            let prompt = cx
                .send_request(PromptRequest::new(
                    session_id.clone(),
                    vec!["hello synbot".into()],
                ))
                .block_task()
                .await?;
            assert_eq!(prompt.stop_reason, StopReason::EndTurn);

            Ok(session_id)
        })
        .await;

    let session_id = client_result.expect("client run failed");
    assert!(!session_id.0.is_empty());

    let texts = chunk_texts(&updates.lock().unwrap());
    assert_eq!(texts, vec!["echo: hello synbot".to_string()]);

    agent_task.abort();
    harness.loop_task.abort();
}

#[tokio::test]
async fn prompt_with_unknown_session_is_rejected() {
    let harness = setup();
    let (client_side, agent_side) = acp::Channel::duplex();
    let agent_task = tokio::spawn(synbot::acp::serve(harness.bridge.clone(), agent_side));

    let client_result = acp::Client
        .builder()
        .name("test-client")
        .connect_with(client_side, async |cx| {
            cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;
            let result = cx
                .send_request(PromptRequest::new("no-such-session", vec!["hi".into()]))
                .block_task()
                .await;
            assert!(result.is_err(), "prompt for unknown session must fail");
            Ok(())
        })
        .await;

    client_result.expect("client run failed");
    agent_task.abort();
    harness.loop_task.abort();
}

#[tokio::test]
async fn permission_request_round_trip_approves_command() {
    let harness = setup();
    let (client_side, agent_side) = acp::Channel::duplex();
    let agent_task = tokio::spawn(synbot::acp::serve(harness.bridge.clone(), agent_side));

    let updates: Arc<StdMutex<Vec<SessionNotification>>> = Arc::default();
    let updates_in_handler = Arc::clone(&updates);
    let permission_requests: Arc<StdMutex<Vec<RequestPermissionRequest>>> = Arc::default();
    let permissions_in_handler = Arc::clone(&permission_requests);

    let client_result = acp::Client
        .builder()
        .name("test-client")
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                updates_in_handler.lock().unwrap().push(notification);
                Ok(())
            },
            acp::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _cx| {
                permissions_in_handler.lock().unwrap().push(request);
                // The user clicks "Allow".
                responder.respond(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new("allow")),
                ))
            },
            acp::on_receive_request!(),
        )
        .connect_with(client_side, async |cx| {
            cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;
            let session = cx
                .send_request(NewSessionRequest::new("/tmp"))
                .block_task()
                .await?;
            let prompt = cx
                .send_request(PromptRequest::new(
                    session.session_id.clone(),
                    vec!["please approve me".into()],
                ))
                .block_task()
                .await?;
            assert_eq!(prompt.stop_reason, StopReason::EndTurn);
            Ok(())
        })
        .await;

    client_result.expect("client run failed");

    let requests = permission_requests.lock().unwrap();
    assert_eq!(requests.len(), 1, "expected one permission request");
    let raw_input = requests[0].tool_call.fields.raw_input.as_ref().unwrap();
    assert_eq!(raw_input["command"], "rm -rf /tmp/x");
    assert_eq!(requests[0].options.len(), 2);

    let texts = chunk_texts(&updates.lock().unwrap());
    assert_eq!(texts, vec!["approved".to_string()]);

    agent_task.abort();
    harness.loop_task.abort();
}

#[tokio::test]
async fn cancel_resolves_prompt_with_cancelled_stop_reason() {
    // A dedicated slow loop: replies only to /stop (like the real loop's control
    // path) so the prompt stays pending until session/cancel arrives.
    let mut bus = MessageBus::new();
    let inbound_tx = bus.inbound_sender();
    let mut inbound_rx = bus.take_inbound_receiver().unwrap();
    let outbound_tx = bus.outbound_tx_clone();
    let approvals = Arc::new(ApprovalManager::with_outbound(outbound_tx.clone()));
    let bridge = AcpBridge::new(inbound_tx, outbound_tx.clone(), Arc::clone(&approvals));
    let hook = bridge.turn_hook();

    let loop_task = tokio::spawn(async move {
        while let Some(msg) = inbound_rx.recv().await {
            if msg.content == "/stop" {
                // The real loop cancels the running task; the run then ends and
                // fires AgentRunEnd. Simulate both.
                let _ = outbound_tx.send(OutboundMessage::chat(
                    msg.channel.clone(),
                    msg.chat_id.clone(),
                    "[Control] Stopped.".to_string(),
                    vec![],
                    None,
                ));
                hook.on_event(HookEvent::AgentRunEnd {
                    agent_id: "main".to_string(),
                    iteration_count: 0,
                    duration_ms: 1,
                    session_key: msg.session_key(),
                })
                .await;
            }
            // Other messages: never reply (simulates a long-running agent turn).
        }
    });

    let (client_side, agent_side) = acp::Channel::duplex();
    let agent_task = tokio::spawn(synbot::acp::serve(bridge, agent_side));

    let client_result = acp::Client
        .builder()
        .name("test-client")
        .on_receive_notification(
            async move |_notification: SessionNotification, _cx| Ok(()),
            acp::on_receive_notification!(),
        )
        .connect_with(client_side, async |cx| {
            cx.send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;
            let session = cx
                .send_request(NewSessionRequest::new("/tmp"))
                .block_task()
                .await?;
            let session_id = session.session_id.clone();

            // Start a prompt that never finishes on its own, then cancel it.
            let sent_prompt = cx.send_request(PromptRequest::new(
                session_id.clone(),
                vec!["work forever".into()],
            ));
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            cx.send_notification(acp::schema::CancelNotification::new(session_id.clone()))?;

            let prompt = sent_prompt.block_task().await?;
            assert_eq!(prompt.stop_reason, StopReason::Cancelled);
            Ok(())
        })
        .await;

    client_result.expect("client run failed");
    agent_task.abort();
    loop_task.abort();
}
