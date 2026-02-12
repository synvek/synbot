use crate::tools::approval::ApprovalRequest;
use chrono::Utc;

/// Language for formatting approval messages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    Chinese,
}

/// Approval result type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalResult {
    Approved,
    Rejected,
    Timeout,
}

/// Format an approval request message for non-web channels (Telegram, Discord, Feishu)
/// 
/// # Arguments
/// * `request` - The approval request to format
/// * `lang` - The language to use for formatting
/// 
/// # Returns
/// A formatted string containing all approval request information and instructions
pub fn format_approval_request(request: &ApprovalRequest, lang: Language) -> String {
    match lang {
        Language::English => format_approval_request_en(request),
        Language::Chinese => format_approval_request_zh(request),
    }
}

fn format_approval_request_en(request: &ApprovalRequest) -> String {
    let timestamp = request.timestamp.format("%Y-%m-%d %H:%M:%S");

    format!(
        r#"🔐 **Command Approval Request**

**Command:** `{}`
**Working Directory:** `{}`
**Context:** {}
**Request Time:** {}
**Timeout:** {} seconds

⚠️ This command requires your approval before execution.

**To approve, reply with one of:**
• yes
• y
• approve

**To reject, reply with one of:**
• no
• n
• reject

⏱️ This request will timeout in {} seconds if no response is received."#,
        request.command,
        request.working_dir,
        request.context,
        timestamp,
        request.timeout_secs,
        request.timeout_secs
    )
}

fn format_approval_request_zh(request: &ApprovalRequest) -> String {
    let timestamp = request.timestamp.format("%Y-%m-%d %H:%M:%S");

    format!(
        r#"🔐 **命令审批请求**

**命令：** `{}`
**工作目录：** `{}`
**上下文：** {}
**请求时间：** {}
**超时时间：** {} 秒

⚠️ 此命令需要您的批准才能执行。

**批准命令，请回复以下任一关键词：**
• 同意
• 批准
• yes
• y
• approve

**拒绝命令，请回复以下任一关键词：**
• 拒绝
• 不同意
• no
• n
• reject

⏱️ 如果 {} 秒内未收到回复，此请求将超时。"#,
        request.command,
        request.working_dir,
        request.context,
        timestamp,
        request.timeout_secs,
        request.timeout_secs
    )
}

/// Format an approval result feedback message
/// 
/// # Arguments
/// * `result` - The approval result (Approved, Rejected, or Timeout)
/// * `command` - The command that was approved/rejected
/// * `responder` - Optional name of the person who responded (None for timeout)
/// * `lang` - The language to use for formatting
/// 
/// # Returns
/// A formatted feedback message
pub fn format_approval_result(
    result: ApprovalResult,
    command: &str,
    responder: Option<&str>,
    lang: Language,
) -> String {
    match lang {
        Language::English => format_approval_result_en(result, command, responder),
        Language::Chinese => format_approval_result_zh(result, command, responder),
    }
}

fn format_approval_result_en(
    result: ApprovalResult,
    command: &str,
    responder: Option<&str>,
) -> String {
    match result {
        ApprovalResult::Approved => {
            let responder_text = responder
                .map(|r| format!(" by {}", r))
                .unwrap_or_default();
            format!(
                "✅ **Command Approved{}**\n\nCommand: `{}`\n\nThe command will now be executed.",
                responder_text, command
            )
        }
        ApprovalResult::Rejected => {
            let responder_text = responder
                .map(|r| format!(" by {}", r))
                .unwrap_or_default();
            format!(
                "❌ **Command Rejected{}**\n\nCommand: `{}`\n\nThe command will not be executed.",
                responder_text, command
            )
        }
        ApprovalResult::Timeout => {
            format!(
                "⏱️ **Approval Request Timeout**\n\nCommand: `{}`\n\nNo response was received within the timeout period. The command will not be executed.",
                command
            )
        }
    }
}

fn format_approval_result_zh(
    result: ApprovalResult,
    command: &str,
    responder: Option<&str>,
) -> String {
    match result {
        ApprovalResult::Approved => {
            let responder_text = responder
                .map(|r| format!("（由 {} 批准）", r))
                .unwrap_or_default();
            format!(
                "✅ **命令已批准{}**\n\n命令：`{}`\n\n命令将立即执行。",
                responder_text, command
            )
        }
        ApprovalResult::Rejected => {
            let responder_text = responder
                .map(|r| format!("（由 {} 拒绝）", r))
                .unwrap_or_default();
            format!(
                "❌ **命令已拒绝{}**\n\n命令：`{}`\n\n命令将不会执行。",
                responder_text, command
            )
        }
        ApprovalResult::Timeout => {
            format!(
                "⏱️ **审批请求超时**\n\n命令：`{}`\n\n超时时间内未收到回复。命令将不会执行。",
                command
            )
        }
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
        }
    }

    #[test]
    fn test_format_approval_request_english() {
        let request = create_test_request();
        let formatted = format_approval_request(&request, Language::English);

        // Check that all key information is present
        assert!(formatted.contains("Command Approval Request"));
        assert!(formatted.contains("rm -rf /tmp/test"));
        assert!(formatted.contains("/home/user/project"));
        assert!(formatted.contains("User requested to clean temporary files"));
        assert!(formatted.contains("300 seconds"));
        
        // Check that approval keywords are listed
        assert!(formatted.contains("yes"));
        assert!(formatted.contains("approve"));
        assert!(formatted.contains("no"));
        assert!(formatted.contains("reject"));
        
        // Check timeout warning
        assert!(formatted.contains("timeout"));
    }

    #[test]
    fn test_format_approval_request_chinese() {
        let request = create_test_request();
        let formatted = format_approval_request(&request, Language::Chinese);

        // Check that all key information is present
        assert!(formatted.contains("命令审批请求"));
        assert!(formatted.contains("rm -rf /tmp/test"));
        assert!(formatted.contains("/home/user/project"));
        assert!(formatted.contains("User requested to clean temporary files"));
        assert!(formatted.contains("300 秒"));
        
        // Check that approval keywords are listed (both Chinese and English)
        assert!(formatted.contains("同意"));
        assert!(formatted.contains("批准"));
        assert!(formatted.contains("拒绝"));
        assert!(formatted.contains("不同意"));
        assert!(formatted.contains("yes"));
        assert!(formatted.contains("no"));
        
        // Check timeout warning
        assert!(formatted.contains("超时"));
    }

    #[test]
    fn test_format_includes_all_fields() {
        let request = create_test_request();
        
        // Test English
        let formatted_en = format_approval_request(&request, Language::English);
        assert!(formatted_en.contains(&request.command));
        assert!(formatted_en.contains(&request.working_dir));
        assert!(formatted_en.contains(&request.context));
        assert!(formatted_en.contains(&request.timeout_secs.to_string()));
        
        // Test Chinese
        let formatted_zh = format_approval_request(&request, Language::Chinese);
        assert!(formatted_zh.contains(&request.command));
        assert!(formatted_zh.contains(&request.working_dir));
        assert!(formatted_zh.contains(&request.context));
        assert!(formatted_zh.contains(&request.timeout_secs.to_string()));
    }

    #[test]
    fn test_format_with_special_characters() {
        let request = ApprovalRequest {
            id: "test-456".to_string(),
            session_id: "session-456".to_string(),
            channel: "test".to_string(),
            chat_id: "chat-789".to_string(),
            command: "echo \"Hello World\" && ls -la".to_string(),
            working_dir: "/path/with spaces/dir".to_string(),
            context: "Testing with special chars: <>&\"'".to_string(),
            timestamp: Utc::now(),
            timeout_secs: 60,
        };

        let formatted_en = format_approval_request(&request, Language::English);
        let formatted_zh = format_approval_request(&request, Language::Chinese);

        // Ensure special characters are preserved
        assert!(formatted_en.contains("echo \"Hello World\" && ls -la"));
        assert!(formatted_en.contains("/path/with spaces/dir"));
        assert!(formatted_en.contains("Testing with special chars: <>&\"'"));
        
        assert!(formatted_zh.contains("echo \"Hello World\" && ls -la"));
        assert!(formatted_zh.contains("/path/with spaces/dir"));
        assert!(formatted_zh.contains("Testing with special chars: <>&\"'"));
    }

    #[test]
    fn test_format_with_long_context() {
        let long_context = "This is a very long context message that contains multiple sentences. \
                           It describes in detail what the user is trying to accomplish. \
                           The context should be fully preserved in the formatted message.";
        
        let request = ApprovalRequest {
            id: "test-789".to_string(),
            session_id: "session-456".to_string(),
            channel: "test".to_string(),
            chat_id: "chat-789".to_string(),
            command: "make clean && make build".to_string(),
            working_dir: "/home/user/project".to_string(),
            context: long_context.to_string(),
            timestamp: Utc::now(),
            timeout_secs: 600,
        };

        let formatted_en = format_approval_request(&request, Language::English);
        let formatted_zh = format_approval_request(&request, Language::Chinese);

        assert!(formatted_en.contains(long_context));
        assert!(formatted_zh.contains(long_context));
    }

    #[test]
    fn test_format_approval_result_approved_english() {
        let result = format_approval_result(
            ApprovalResult::Approved,
            "rm -rf /tmp/test",
            Some("user123"),
            Language::English,
        );

        assert!(result.contains("Command Approved"));
        assert!(result.contains("by user123"));
        assert!(result.contains("rm -rf /tmp/test"));
        assert!(result.contains("will now be executed"));
    }

    #[test]
    fn test_format_approval_result_approved_chinese() {
        let result = format_approval_result(
            ApprovalResult::Approved,
            "rm -rf /tmp/test",
            Some("user123"),
            Language::Chinese,
        );

        assert!(result.contains("命令已批准"));
        assert!(result.contains("由 user123 批准"));
        assert!(result.contains("rm -rf /tmp/test"));
        assert!(result.contains("将立即执行"));
    }

    #[test]
    fn test_format_approval_result_rejected_english() {
        let result = format_approval_result(
            ApprovalResult::Rejected,
            "rm -rf /tmp/test",
            Some("admin"),
            Language::English,
        );

        assert!(result.contains("Command Rejected"));
        assert!(result.contains("by admin"));
        assert!(result.contains("rm -rf /tmp/test"));
        assert!(result.contains("will not be executed"));
    }

    #[test]
    fn test_format_approval_result_rejected_chinese() {
        let result = format_approval_result(
            ApprovalResult::Rejected,
            "rm -rf /tmp/test",
            Some("admin"),
            Language::Chinese,
        );

        assert!(result.contains("命令已拒绝"));
        assert!(result.contains("由 admin 拒绝"));
        assert!(result.contains("rm -rf /tmp/test"));
        assert!(result.contains("不会执行"));
    }

    #[test]
    fn test_format_approval_result_timeout_english() {
        let result = format_approval_result(
            ApprovalResult::Timeout,
            "make build",
            None,
            Language::English,
        );

        assert!(result.contains("Approval Request Timeout"));
        assert!(result.contains("make build"));
        assert!(result.contains("No response was received"));
        assert!(result.contains("will not be executed"));
        assert!(!result.contains("by"));
    }

    #[test]
    fn test_format_approval_result_timeout_chinese() {
        let result = format_approval_result(
            ApprovalResult::Timeout,
            "make build",
            None,
            Language::Chinese,
        );

        assert!(result.contains("审批请求超时"));
        assert!(result.contains("make build"));
        assert!(result.contains("未收到回复"));
        assert!(result.contains("不会执行"));
        assert!(!result.contains("由"));
    }

    #[test]
    fn test_format_approval_result_without_responder() {
        // Test approved without responder name
        let result_en = format_approval_result(
            ApprovalResult::Approved,
            "ls -la",
            None,
            Language::English,
        );
        assert!(result_en.contains("Command Approved"));
        assert!(!result_en.contains("by"));

        let result_zh = format_approval_result(
            ApprovalResult::Approved,
            "ls -la",
            None,
            Language::Chinese,
        );
        assert!(result_zh.contains("命令已批准"));
        assert!(!result_zh.contains("由"));
    }
}
