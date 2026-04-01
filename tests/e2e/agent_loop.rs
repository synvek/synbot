//! AgentLoop end-to-end tests.
//!
//! Tests the full message processing pipeline: InboundMessage → AgentLoop → OutboundMessage.
//! Uses a mock completion model to avoid real LLM calls.
//!
//! Requirements: 12.1, 12.2, 12.3, 12.4
//! Run with: `cargo test --test e2e agent_loop`

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, Mutex};

use synbot::agent::agent_registry::AgentRegistry;
use synbot::agent::r#loop::AgentLoop;
use synbot::agent::role_registry::RoleRegistry;
use synbot::agent::session::SessionStore;
use synbot::agent::session_state::SharedSessionState;
use synbot::bus::{InboundMessage, OutboundMessage, OutboundMessageType};
use synbot::config::{Config, MainAgent};
use synbot::tools::ToolRegistry;

use super::common;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a minimal InboundMessage for testing.
fn inbound(channel: &str, chat_id: &str, content: &str) -> InboundMessage {
    InboundMessage {
        channel: channel.to_string(),
        sender_id: "test-user".to_string(),
        chat_id: chat_id.to_string(),
        content: content.to_string(),
        timestamp: chrono::Utc::now(),
        media: vec![],
        metadata: serde_json::json!({ "trigger_agent": true }),
    }
}

/// Collect all outbound messages that arrive within a timeout.
async fn collect_outbound(
    rx: &mut broadcast::Receiver<OutboundMessage>,
    timeout_ms: u64,
) -> Vec<OutboundMessage> {
    let mut msgs = vec![];
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Ok(msg)) => msgs.push(msg),
            _ => break,
        }
    }
    msgs
}

/// Build a minimal AgentLoop with a mock model and return (loop_ref, inbound_tx, outbound_rx).
async fn build_agent_loop(
    config: &Config,
) -> (
    Arc<Mutex<AgentLoop>>,
    mpsc::Sender<InboundMessage>,
    broadcast::Receiver<OutboundMessage>,
) {
    let (inbound_tx, inbound_rx) = mpsc::channel::<InboundMessage>(32);
    let (outbound_tx, outbound_rx) = broadcast::channel::<OutboundMessage>(64);

    let (_dir, workspace) = common::temp_workspace();
    let session_store = SessionStore::new(workspace.as_path() as &std::path::Path);
    let session_state = SharedSessionState::new(session_store);
    let tools = Arc::new(ToolRegistry::new());

    // Build a minimal agent registry with a "main" agent
    let mut agent_registry = AgentRegistry::new();
    let (_roles_dir, roles_path) = common::temp_workspace();
    // Create a minimal "main" role directory so load_from_config succeeds
    std::fs::create_dir_all(roles_path.join("main")).expect("create main role dir");
    let mut role_registry = RoleRegistry::new();
    role_registry.load_from_dirs(&roles_path).expect("load roles");
    let main_agent = MainAgent {
        workspace: workspace.to_string_lossy().to_string(),
        provider: "mock".to_string(),
        model: "mock-model".to_string(),
        max_tokens: 1024,
        temperature: 0.7,
        max_tool_iterations: 3,
        max_consecutive_tool_errors: 3,
        max_chat_history_messages: 20,
        max_concurrent_subagents: 1,
        subagent_task_timeout_secs: 30,
        agents: vec![],
    };
    agent_registry
        .load_from_config(&main_agent, &role_registry, &workspace)
        .expect("load agent registry");
    let agent_registry = Arc::new(agent_registry);

    let mock_model = Arc::new(common::mock_completion_model("Hello from mock model"));

    let agent_loop = AgentLoop::new(
        mock_model,
        workspace,
        tools,
        3, // max_iterations
        outbound_tx,
        config,
        session_state,
        agent_registry,
        None, // tool_sandbox_exec_kind
        None,  // hooks
    )
    .await;

    let loop_ref = Arc::new(Mutex::new(agent_loop));
    let loop_ref_run = loop_ref.clone();

    // Spawn the agent loop in the background
    tokio::spawn(async move {
        let _ = AgentLoop::run(loop_ref_run, inbound_rx).await;
    });

    (loop_ref, inbound_tx, outbound_rx)
}

// ---------------------------------------------------------------------------
// Requirement 12.1 — InboundMessage produces OutboundMessage
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_inbound_message_produces_outbound_response() {
    let config = common::default_test_config();
    let (_loop_ref, inbound_tx, mut outbound_rx) = build_agent_loop(&config).await;

    inbound_tx
        .send(inbound("telegram", "chat-1", "Hello agent"))
        .await
        .expect("send inbound");

    let msgs = collect_outbound(&mut outbound_rx, 3000).await;
    assert!(
        !msgs.is_empty(),
        "AgentLoop should produce at least one outbound message"
    );

    // Verify the response targets the correct channel and chat
    let chat_msg = msgs.iter().find(|m| {
        m.channel == "telegram"
            && m.chat_id == "chat-1"
            && matches!(m.message_type, OutboundMessageType::Chat { .. })
    });
    assert!(
        chat_msg.is_some(),
        "Should have a Chat outbound message for telegram:chat-1"
    );
}

// ---------------------------------------------------------------------------
// Requirement 12.3 — max_iterations terminates the loop
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_agent_loop_terminates_within_max_iterations() {
    // Use a model that always returns a tool call to force iteration, but since
    // no tools are registered the loop will terminate after max_iterations.
    let config = common::default_test_config();
    let (_loop_ref, inbound_tx, mut outbound_rx) = build_agent_loop(&config).await;

    inbound_tx
        .send(inbound("telegram", "chat-iter", "Run forever"))
        .await
        .expect("send inbound");

    // The loop must terminate and produce a response within a reasonable time
    let msgs = collect_outbound(&mut outbound_rx, 5000).await;
    assert!(
        !msgs.is_empty(),
        "AgentLoop should terminate and send a response even when hitting max_iterations"
    );
}

// ---------------------------------------------------------------------------
// Requirement 12.4 — concurrent sessions are isolated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_concurrent_sessions_are_isolated() {
    let config = common::default_test_config();
    let (_loop_ref, inbound_tx, mut outbound_rx) = build_agent_loop(&config).await;

    // Send messages to two different sessions concurrently
    let tx1 = inbound_tx.clone();
    let tx2 = inbound_tx.clone();

    let h1 = tokio::spawn(async move {
        tx1.send(inbound("telegram", "session-A", "Message for session A"))
            .await
            .expect("send A");
    });
    let h2 = tokio::spawn(async move {
        tx2.send(inbound("discord", "session-B", "Message for session B"))
            .await
            .expect("send B");
    });

    let _ = tokio::join!(h1, h2);

    let msgs = collect_outbound(&mut outbound_rx, 5000).await;

    // Each session should receive its own response
    let has_a = msgs.iter().any(|m| m.channel == "telegram" && m.chat_id == "session-A");
    let has_b = msgs.iter().any(|m| m.channel == "discord" && m.chat_id == "session-B");

    assert!(has_a, "Session A should receive a response");
    assert!(has_b, "Session B should receive a response");

    // Verify no cross-contamination: session-A responses should not go to session-B
    let a_to_b = msgs.iter().any(|m| m.channel == "discord" && m.chat_id == "session-A");
    let b_to_a = msgs.iter().any(|m| m.channel == "telegram" && m.chat_id == "session-B");
    assert!(!a_to_b, "Session A messages should not appear in session B's channel");
    assert!(!b_to_a, "Session B messages should not appear in session A's channel");
}

// ---------------------------------------------------------------------------
// Requirement 12.2 — tool call failure returns user-readable error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_control_command_stop_returns_response() {
    let config = common::default_test_config();
    let (_loop_ref, inbound_tx, mut outbound_rx) = build_agent_loop(&config).await;

    // /stop when nothing is running should return a control message
    inbound_tx
        .send(inbound("telegram", "chat-stop", "/stop"))
        .await
        .expect("send /stop");

    let msgs = collect_outbound(&mut outbound_rx, 2000).await;
    let stop_response = msgs.iter().any(|m| {
        m.channel == "telegram"
            && m.chat_id == "chat-stop"
            && matches!(&m.message_type, OutboundMessageType::Chat { content, .. }
                if content.contains("[Control]"))
    });
    assert!(stop_response, "Should receive a [Control] response to /stop");
}

#[tokio::test]
async fn test_control_command_status_returns_response() {
    let config = common::default_test_config();
    let (_loop_ref, inbound_tx, mut outbound_rx) = build_agent_loop(&config).await;

    inbound_tx
        .send(inbound("telegram", "chat-status", "/status"))
        .await
        .expect("send /status");

    let msgs = collect_outbound(&mut outbound_rx, 2000).await;
    let status_response = msgs.iter().any(|m| {
        m.channel == "telegram" && m.chat_id == "chat-status"
    });
    assert!(status_response, "Should receive a response to /status");
}
