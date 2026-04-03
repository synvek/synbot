//! Compress long chat history: LLM summary + optional append to MEMORY.md.

use std::sync::Arc;

use anyhow::Result;
use rig::completion::CompletionRequest;
use rig::message::{AssistantContent, Message};
use rig::OneOrMany;

use crate::agent::memory_backend::{should_compress, FileSqliteMemoryBackend, MemoryBackend};
use crate::config::Config;
use crate::rig_provider::SynbotCompletionModel;

fn messages_to_text(msgs: &[Message]) -> String {
    let mut out = String::new();
    for m in msgs {
        match m {
            Message::User { content } => {
                for c in content.clone() {
                    if let rig::message::UserContent::Text(t) = c {
                        out.push_str("User: ");
                        out.push_str(&t.text);
                        out.push('\n');
                    }
                }
            }
            Message::Assistant { content, .. } => {
                for c in content.clone() {
                    if let AssistantContent::Text(t) = c {
                        out.push_str("Assistant: ");
                        out.push_str(&t.text);
                        out.push('\n');
                    }
                }
            }
        }
    }
    out
}

/// If enabled and history is long enough, summarize the oldest segment and prepend a single user message.
pub async fn maybe_compact_history(
    model: &dyn SynbotCompletionModel,
    cfg: &Config,
    agent_id: &str,
    history: &mut Vec<Message>,
    max_chat_history_messages: u32,
) -> Result<()> {
    let comp = &cfg.memory.compression;
    if !should_compress(&cfg.memory, history.len()) {
        return Ok(());
    }

    let keep = comp
        .keep_recent_messages
        .unwrap_or(max_chat_history_messages)
        .max(1) as usize;
    if history.len() <= keep {
        return Ok(());
    }

    let remove_n = history.len() - keep;
    if remove_n == 0 {
        return Ok(());
    }

    let prefix: Vec<Message> = history.drain(..remove_n).collect();
    let transcript = messages_to_text(&prefix);
    if transcript.trim().is_empty() {
        return Ok(());
    }

    let summarize_prompt = format!(
        "Summarize the following conversation segment in 5-12 short bullet points. \
         Preserve facts, names, file paths, and decisions. Omit filler.\n\n{}",
        transcript
    );

    let request = CompletionRequest {
        preamble: Some(
            "You write concise bullet-point summaries of chat history. Output only the summary, no preamble."
                .to_string(),
        ),
        chat_history: OneOrMany::one(Message::user(summarize_prompt)),
        tools: vec![],
        documents: vec![],
        temperature: Some(0.2),
        max_tokens: Some(1024),
        tool_choice: None,
        additional_params: None,
    };

    let response = model.completion(request).await?;
    let mut summary_text = String::new();
    for c in response.choice.clone() {
        if let AssistantContent::Text(t) = c {
            summary_text.push_str(&t.text);
        }
    }
    let summary_text = summary_text.trim().to_string();
    if summary_text.is_empty() {
        return Ok(());
    }

    let block = format!(
        "[Conversation summary — earlier messages compressed]\n\n{}",
        summary_text
    );
    history.insert(0, Message::user(&block));

    if comp.summary_write_to_memory {
        let backend = FileSqliteMemoryBackend::new(Arc::new(cfg.clone()));
        let note = format!(
            "\n## Auto summary ({})\n\n{}",
            chrono::Local::now().format("%Y-%m-%d %H:%M"),
            summary_text
        );
        let _ = backend.append_long_term(agent_id, &note);
    }

    Ok(())
}
