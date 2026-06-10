//! Unit tests for ACP message mapping.

use super::*;
use agent_client_protocol::schema::TextContent;

fn approval_request(id: &str, command: &str) -> crate::tools::approval::ApprovalRequest {
    crate::tools::approval::ApprovalRequest {
        id: id.to_string(),
        session_id: "agent:main:acp".to_string(),
        channel: ACP_CHANNEL.to_string(),
        chat_id: "session-1".to_string(),
        command: command.to_string(),
        working_dir: "/tmp".to_string(),
        context: "test".to_string(),
        timestamp: chrono::Utc::now(),
        timeout_secs: 60,
        display_message: None,
    }
}

#[test]
fn prompt_text_blocks_are_joined() {
    let prompt = vec![
        ContentBlock::Text(TextContent::new("hello")),
        ContentBlock::Text(TextContent::new("world")),
    ];
    assert_eq!(prompt_to_text(&prompt), "hello\n\nworld");
}

#[test]
fn prompt_resource_link_is_referenced_by_uri() {
    let prompt = vec![
        ContentBlock::Text(TextContent::new("check this file")),
        ContentBlock::ResourceLink(acp::schema::ResourceLink::new(
            "main.rs",
            "file:///src/main.rs",
        )),
    ];
    let text = prompt_to_text(&prompt);
    assert!(text.contains("check this file"));
    assert!(text.contains("[Resource: file:///src/main.rs]"));
}

#[test]
fn prompt_embedded_text_resource_is_inlined() {
    let prompt = vec![ContentBlock::Resource(acp::schema::EmbeddedResource::new(
        acp::schema::EmbeddedResourceResource::TextResourceContents(
            acp::schema::TextResourceContents::new("fn main() {}", "file:///src/main.rs"),
        ),
    ))];
    let text = prompt_to_text(&prompt);
    assert!(text.contains("file:///src/main.rs"));
    assert!(text.contains("fn main() {}"));
}

#[test]
fn chat_maps_to_agent_message_chunk() {
    let update = chat_to_session_update("hi there".to_string());
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => match chunk.content {
            ContentBlock::Text(t) => assert_eq!(t.text, "hi there"),
            other => panic!("expected text content, got {other:?}"),
        },
        other => panic!("expected AgentMessageChunk, got {other:?}"),
    }
}

#[test]
fn tool_progress_success_maps_to_completed_tool_call() {
    let update = tool_progress_to_session_update("exec", "success", "done");
    match update {
        SessionUpdate::ToolCall(call) => {
            assert_eq!(call.title, "exec");
            assert_eq!(call.status, ToolCallStatus::Completed);
            assert_eq!(call.kind, ToolKind::Execute);
            assert_eq!(
                call.raw_output,
                Some(serde_json::Value::String("done".to_string()))
            );
        }
        other => panic!("expected ToolCall, got {other:?}"),
    }
}

#[test]
fn tool_progress_failure_maps_to_failed_tool_call() {
    let update = tool_progress_to_session_update("web_search", "failure", "Error: boom");
    match update {
        SessionUpdate::ToolCall(call) => {
            assert_eq!(call.status, ToolCallStatus::Failed);
            assert_eq!(call.kind, ToolKind::Other);
        }
        other => panic!("expected ToolCall, got {other:?}"),
    }
}

#[test]
fn approval_maps_to_permission_request_with_allow_reject() {
    let req = approval_request("req-1", "rm -rf /tmp/x");
    let perm = approval_to_permission_request("session-1", &req);
    assert_eq!(perm.session_id.0.as_ref(), "session-1");
    assert_eq!(perm.options.len(), 2);
    assert_eq!(perm.options[0].option_id.0.as_ref(), OPTION_ALLOW);
    assert_eq!(perm.options[0].kind, PermissionOptionKind::AllowOnce);
    assert_eq!(perm.options[1].option_id.0.as_ref(), OPTION_REJECT);
    assert_eq!(perm.options[1].kind, PermissionOptionKind::RejectOnce);
    assert_eq!(perm.tool_call.tool_call_id.0.as_ref(), "approval-req-1");
    let raw_input = perm.tool_call.fields.raw_input.as_ref().unwrap();
    assert_eq!(raw_input["command"], "rm -rf /tmp/x");
}

#[test]
fn approval_uses_display_message_as_title_when_present() {
    let mut req = approval_request("req-2", "git push");
    req.display_message = Some("Push to production?".to_string());
    let perm = approval_to_permission_request("session-1", &req);
    assert_eq!(
        perm.tool_call.fields.title.as_deref(),
        Some("Push to production?")
    );
}

#[test]
fn permission_allow_outcome_approves() {
    let outcome = RequestPermissionOutcome::Selected(
        acp::schema::SelectedPermissionOutcome::new(OPTION_ALLOW),
    );
    let response = permission_outcome_to_approval("req-1", &outcome);
    assert!(response.approved);
    assert_eq!(response.request_id, "req-1");
    assert_eq!(response.responder, ACP_SENDER);
}

#[test]
fn permission_reject_outcome_rejects() {
    let outcome = RequestPermissionOutcome::Selected(
        acp::schema::SelectedPermissionOutcome::new(OPTION_REJECT),
    );
    assert!(!permission_outcome_to_approval("req-1", &outcome).approved);
}

#[test]
fn permission_cancelled_outcome_rejects() {
    let outcome = RequestPermissionOutcome::Cancelled;
    assert!(!permission_outcome_to_approval("req-1", &outcome).approved);
}
