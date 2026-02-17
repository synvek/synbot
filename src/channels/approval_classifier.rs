//! LLM-based classification of user messages as approval (approve/reject) for command execution.
//! Used by Feishu and other channels that receive free-text approval replies.

use rig::completion::CompletionRequest;
use rig::message::{AssistantContent, Message};
use rig_dyn::CompletionModel;
use tracing::warn;

const SYSTEM_PROMPT: &str = r#"You are a classifier. The user was asked to approve or reject running a command (e.g. "Run python script X?"). They replied with a short message.

Classify their reply into exactly one of:
- APPROVE: they agree, allow, or confirm (in any language: yes, 好, 批准, ok, 行, etc.)
- REJECT: they disagree, deny, or refuse (no, 不, 拒绝, reject, etc.)
- UNKNOWN: unclear or unrelated (e.g. "what command?", "later", or empty)

Reply with exactly one word: APPROVE, REJECT, or UNKNOWN. No other text."#;

/// Classify user message as approval intent using the LLM.
/// Returns `Some(true)` = approve, `Some(false)` = reject, `None` = unknown or error.
pub async fn classify_approval_response(
    model: &dyn CompletionModel,
    user_message: &str,
) -> Option<bool> {
    let trimmed = user_message.trim();
    if trimmed.is_empty() {
        return None;
    }

    let request = CompletionRequest {
        preamble: Some(SYSTEM_PROMPT.to_string()),
        chat_history: vec![],
        prompt: Message::user(trimmed),
        tools: vec![],
        documents: vec![],
        temperature: Some(0.0),
        max_tokens: Some(20),
        additional_params: None,
    };

    let response = match model.completion(request).await {
        Ok(r) => r,
        Err(e) => {
            warn!(
                error = %e,
                "Approval classifier LLM call failed, treating as unknown"
            );
            return None;
        }
    };

    let mut first_text = String::new();
    for content in response.iter() {
        if let AssistantContent::Text(t) = content {
            first_text.push_str(&t.text);
            break;
        }
    }

    let word = first_text.trim().to_uppercase();
    if word.starts_with("APPROVE") {
        return Some(true);
    }
    if word.starts_with("REJECT") {
        return Some(false);
    }
    None
}
