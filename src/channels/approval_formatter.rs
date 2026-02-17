use crate::tools::approval::ApprovalRequest;
use chrono::Utc;

/// Approval result type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalResult {
    Approved,
    Rejected,
    Timeout,
}

/// Format a fallback approval request message (neutral, no language).
/// Used when display_message is not provided by the agent.
pub fn format_approval_request(request: &ApprovalRequest) -> String {
    let ts = request.timestamp.format("%Y-%m-%d %H:%M:%S");
    format!(
        "command: `{}`\nworking_dir: `{}`\ncontext: {}\ntimestamp: {}\ntimeout_secs: {}",
        request.command,
        request.working_dir,
        request.context,
        ts,
        request.timeout_secs
    )
}

/// Format approval result feedback (neutral).
pub fn format_approval_result(
    result: ApprovalResult,
    command: &str,
    responder: Option<&str>,
) -> String {
    match result {
        ApprovalResult::Approved => {
            let who = responder.map(|r| format!(" (responder: {})", r)).unwrap_or_default();
            format!("approved{}\ncommand: `{}`", who, command)
        }
        ApprovalResult::Rejected => {
            let who = responder.map(|r| format!(" (responder: {})", r)).unwrap_or_default();
            format!("rejected{}\ncommand: `{}`", who, command)
        }
        ApprovalResult::Timeout => format!("timeout\ncommand: `{}`", command),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_request() -> ApprovalRequest {
        ApprovalRequest {
            id: "test-123".to_string(),
            session_id: "session-456".to_string(),
            channel: "test".to_string(),
            chat_id: "chat-789".to_string(),
            command: "rm -rf /tmp/test".to_string(),
            working_dir: "/home/user/project".to_string(),
            context: "User requested to clean temporary files".to_string(),
            timestamp: Utc::now(),
            timeout_secs: 300,
            display_message: None,
        }
    }

    #[test]
    fn test_format_includes_all_fields() {
        let request = create_test_request();
        let formatted = format_approval_request(&request);
        assert!(formatted.contains(&request.command));
        assert!(formatted.contains(&request.working_dir));
        assert!(formatted.contains(&request.context));
        assert!(formatted.contains(&request.timeout_secs.to_string()));
    }

    #[test]
    fn test_format_approval_result_approved() {
        let result = format_approval_result(
            ApprovalResult::Approved,
            "rm -rf /tmp/test",
            Some("user123"),
        );
        assert!(result.contains("approved"));
        assert!(result.contains("user123"));
        assert!(result.contains("rm -rf /tmp/test"));
    }

    #[test]
    fn test_format_approval_result_rejected() {
        let result = format_approval_result(
            ApprovalResult::Rejected,
            "rm -rf /tmp/test",
            Some("admin"),
        );
        assert!(result.contains("rejected"));
        assert!(result.contains("admin"));
    }

    #[test]
    fn test_format_approval_result_timeout() {
        let result = format_approval_result(ApprovalResult::Timeout, "make build", None);
        assert!(result.contains("timeout"));
        assert!(result.contains("make build"));
    }
}
