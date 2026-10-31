//! Execute a workflow: run steps serially, persist after each, wait for user input when needed.

use anyhow::Result;
use chrono::Utc;
use rig::completion::CompletionRequest;
use rig::message::{AssistantContent, Message};
use rig::OneOrMany;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::agent::events::{AgentEvent, EventBus};
use crate::agent::session_state::SharedSessionState;
use crate::bus::OutboundMessage;
use crate::rig_provider::SynbotCompletionModel;
use crate::workflow::pending_input::PendingWorkflowInputStore;
use crate::workflow::store::WorkflowStore;
use crate::workflow::types::{WorkflowState, WorkflowStatus};

/// Max chars for "previous step output" preview when asking for user input (avoid huge messages).
const PREVIEW_MAX_CHARS: usize = 3500;

fn truncate_preview(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.len() <= max_chars {
        return s.to_string();
    }
    let mut end = max_chars;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n\n… (truncated; see full content in chat history or workflow state)",
        &s[..end]
    )
}

/// Send a chat message to the user and append to session history when history_session_key is set.
/// Session persist is spawned so we don't block the next step on disk I/O.
///
/// Progress goes out on two paths: the outbound bus (Feishu/DingTalk/channels) and the event bus
/// as a `Notice` — the web workbench drops plain outbound chat text, so without the event the
/// chat that started the workflow would see nothing until the run finished.
async fn send_workflow_message(
    outbound_tx: &broadcast::Sender<OutboundMessage>,
    events: &EventBus,
    session_state: &SharedSessionState,
    history_session_key: Option<&str>,
    channel: &str,
    chat_id: &str,
    content: &str,
) {
    events.publish(
        channel,
        chat_id,
        AgentEvent::Notice {
            message: content.to_string(),
        },
    );
    let _ = outbound_tx.send(OutboundMessage::chat(
        channel.to_string(),
        chat_id.to_string(),
        content.to_string(),
        vec![],
        None,
    ));
    if let Some(key) = history_session_key {
        let session_state = session_state.clone();
        let key = key.to_string();
        let content = content.to_string();
        tokio::spawn(async move {
            if let Err(e) = session_state
                .append_assistant_message_and_save(&key, &content)
                .await
            {
                warn!(key = %key, "Workflow: persist message to session failed: {}", e);
            }
        });
    }
}

/// Run the workflow from current state until it completes, fails, or waits for user input (with timeout).
/// If `cancel` is provided and gets cancelled (e.g. /stop), the workflow stops and state is persisted.
pub async fn run_workflow(
    workflow_store: &WorkflowStore,
    model: &dyn SynbotCompletionModel,
    outbound_tx: &broadcast::Sender<OutboundMessage>,
    events: &EventBus,
    pending_input: &PendingWorkflowInputStore,
    session_state: &SharedSessionState,
    history_session_key: Option<&str>,
    channel: &str,
    chat_id: &str,
    session_key: &str,
    mut state: WorkflowState,
    cancel: Option<CancellationToken>,
) -> Result<()> {
    loop {
        if let Some(ref token) = cancel {
            if token.is_cancelled() {
                state.status = WorkflowStatus::Cancelled;
                state.updated_at = Utc::now();
                let store = workflow_store.clone();
                let key = session_key.to_string();
                let state_save = state.clone();
                tokio::spawn(async move {
                    let _ = store.save_state(&key, &state_save).await;
                });
                info!(session_key = %session_key, "Workflow cancelled via /stop");
                break;
            }
        }
        if state.is_finished() {
            break;
        }

        let step = match state.current_step() {
            Some(s) => s.clone(),
            None => {
                state.status = WorkflowStatus::Completed;
                state.updated_at = Utc::now();
                let store = workflow_store.clone();
                let key = session_key.to_string();
                let state_save = state.clone();
                tokio::spawn(async move {
                    let _ = store.save_state(&key, &state_save).await;
                });
                // Send final result to user so they see the last step output in chat.
                if let Some(last_output) = state.step_outputs.last() {
                    let preview = truncate_preview(last_output, PREVIEW_MAX_CHARS);
                    let content = format!("[Workflow] Completed. Final result:\n\n{}", preview);
                    send_workflow_message(
                        outbound_tx,
                        events,
                        session_state,
                        history_session_key,
                        channel,
                        chat_id,
                        &content,
                    )
                    .await;
                } else {
                    send_workflow_message(
                        outbound_tx,
                        events,
                        session_state,
                        history_session_key,
                        channel,
                        chat_id,
                        "[Workflow] Completed.",
                    )
                    .await;
                }
                info!(session_key = %session_key, "Workflow completed");
                break;
            }
        };

        let step_type = step.step_type.to_lowercase();
        if step_type == "llm" {
            let context = format!(
                "Inputs: {:?}\nPrevious step outputs: {:?}",
                state.inputs, state.step_outputs
            );
            let user_msg = format!("Task: {}\n\nContext:\n{}", step.description, context);
            let request = CompletionRequest {
                preamble: Some("You are a workflow step executor. Complete the task concisely. Reply with the result only, no meta commentary.".to_string()),
                chat_history: OneOrMany::one(Message::user(&user_msg)),
                tools: vec![],
                documents: vec![],
                temperature: Some(0.3),
                max_tokens: Some(2048),
                tool_choice: None,
                additional_params: None,
            };

            let step_start = Instant::now();
            let response = model
                .completion(request)
                .await
                .map_err(|e| anyhow::anyhow!("workflow step completion failed: {}", e))?;
            info!(
                session_key = %session_key,
                step = %step.id,
                elapsed_ms = step_start.elapsed().as_millis(),
                "Workflow LLM step completed (delay is mainly model API)"
            );

            let mut output = String::new();
            for content in response.choice.clone().into_iter() {
                if let AssistantContent::Text(t) = content {
                    output.push_str(&t.text);
                }
            }
            let output = output.trim().to_string();
            let output = if output.is_empty() {
                "[No text output]".to_string()
            } else {
                output
            };

            state.step_outputs.push(output);
            state.current_step_index += 1;
            state.updated_at = Utc::now();
            // Persist in background so next step can start without waiting for disk
            let store = workflow_store.clone();
            let session_key_s = session_key.to_string();
            let state_save = state.clone();
            tokio::spawn(async move {
                if let Err(e) = store.save_state(&session_key_s, &state_save).await {
                    warn!(session_key = %session_key_s, "Workflow: persist state failed: {}", e);
                }
            });
            info!(session_key = %session_key, step = %step.id, "Workflow step completed");
        } else if step_type == "user_input" {
            let prompt = if step.description.is_empty() {
                "Please provide your input."
            } else {
                &step.description
            };
            let input_key = step.input_key.as_deref().unwrap_or("user_input");

            // Send previous step output preview first so user can see what was generated before replying.
            if let Some(last_output) = state.step_outputs.last() {
                let preview = truncate_preview(last_output, PREVIEW_MAX_CHARS);
                let content = format!("[Workflow] Previous step output:\n\n{}", preview);
                send_workflow_message(
                    outbound_tx,
                    events,
                    session_state,
                    history_session_key,
                    channel,
                    chat_id,
                    &content,
                )
                .await;
            }

            let prompt_msg = format!(
                "[Workflow] {} (please reply within {} minutes)",
                prompt,
                state.user_input_timeout_secs / 60
            );
            send_workflow_message(
                outbound_tx,
                events,
                session_state,
                history_session_key,
                channel,
                chat_id,
                &prompt_msg,
            )
            .await;

            let rx = pending_input.register(session_key).await;
            let timeout_dur = Duration::from_secs(state.user_input_timeout_secs);

            enum UserInputResult {
                Received(String),
                Timeout,
                Closed,
                Cancelled,
            }
            let wait_result = if let Some(ref token) = cancel {
                tokio::select! {
                    _ = token.cancelled() => UserInputResult::Cancelled,
                    r = timeout(timeout_dur, rx) => match r {
                        Ok(Ok(c)) => UserInputResult::Received(c),
                        Ok(Err(_)) => UserInputResult::Closed,
                        Err(_) => UserInputResult::Timeout,
                    },
                }
            } else {
                match timeout(timeout_dur, rx).await {
                    Ok(Ok(c)) => UserInputResult::Received(c),
                    Ok(Err(_)) => UserInputResult::Closed,
                    Err(_) => UserInputResult::Timeout,
                }
            };

            match wait_result {
                UserInputResult::Received(content) => {
                    if let Some(key) = history_session_key {
                        let _ = session_state
                            .append_user_message_and_save(key, &content)
                            .await;
                    }
                    state.inputs.insert(input_key.to_string(), content);
                    state.step_outputs.push("[User input received]".to_string());
                    state.current_step_index += 1;
                    state.status = WorkflowStatus::Running;
                    state.updated_at = Utc::now();
                    let store = workflow_store.clone();
                    let key = session_key.to_string();
                    let state_save = state.clone();
                    tokio::spawn(async move {
                        let _ = store.save_state(&key, &state_save).await;
                    });
                    info!(session_key = %session_key, step = %step.id, "Workflow user input received");
                }
                UserInputResult::Closed => {
                    let _ = pending_input.remove(session_key).await;
                    state.status = WorkflowStatus::Cancelled;
                    state.updated_at = Utc::now();
                    let store = workflow_store.clone();
                    let key = session_key.to_string();
                    let state_save = state.clone();
                    tokio::spawn(async move {
                        let _ = store.save_state(&key, &state_save).await;
                    });
                    warn!(session_key = %session_key, "Workflow user input channel closed");
                    break;
                }
                UserInputResult::Cancelled => {
                    let _ = pending_input.remove(session_key).await;
                    state.status = WorkflowStatus::Cancelled;
                    state.updated_at = Utc::now();
                    let store = workflow_store.clone();
                    let key = session_key.to_string();
                    let state_save = state.clone();
                    tokio::spawn(async move {
                        let _ = store.save_state(&key, &state_save).await;
                    });
                    info!(session_key = %session_key, "Workflow cancelled via /stop during user input");
                    break;
                }
                UserInputResult::Timeout => {
                    let _ = pending_input.remove(session_key).await;
                    state.status = WorkflowStatus::WaitingTimeout;
                    state.updated_at = Utc::now();
                    let store = workflow_store.clone();
                    let key = session_key.to_string();
                    let state_save = state.clone();
                    tokio::spawn(async move {
                        let _ = store.save_state(&key, &state_save).await;
                    });
                    send_workflow_message(
                        outbound_tx,
                        events,
                        session_state,
                        history_session_key,
                        channel,
                        chat_id,
                        "[Workflow] Waiting for user input timed out; paused. Send /resume or /workflow continue to resume.",
                    )
                    .await;
                    warn!(session_key = %session_key, "Workflow user input timeout");
                    break;
                }
            }
        } else {
            warn!(step_type = %step.step_type, "Unknown step type, skipping");
            state.current_step_index += 1;
            state.step_outputs.push("[Skipped]".to_string());
            state.updated_at = Utc::now();
            let store = workflow_store.clone();
            let key = session_key.to_string();
            let state_save = state.clone();
            tokio::spawn(async move {
                let _ = store.save_state(&key, &state_save).await;
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod notice_tests {
    use super::*;
    use crate::agent::session::SessionStore;
    use crate::bus::OutboundMessageType;

    #[tokio::test]
    async fn workflow_progress_reaches_the_event_bus_as_a_notice() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session_state = SharedSessionState::new(SessionStore::new(dir.path()));
        let (outbound_tx, mut outbound_rx) = broadcast::channel(8);
        let events = EventBus::new();
        let mut event_rx = events.subscribe();

        // No history key: this test only cares about the two delivery paths.
        send_workflow_message(
            &outbound_tx,
            &events,
            &session_state,
            None,
            "web",
            "web_admin",
            "[Workflow] Step 1 completed.",
        )
        .await;

        // Channel clients keep getting the outbound chat message…
        let outbound = outbound_rx.try_recv().expect("outbound chat message");
        assert_eq!(outbound.channel, "web");
        assert_eq!(outbound.chat_id, "web_admin");
        match outbound.message_type {
            OutboundMessageType::Chat { content, .. } => {
                assert_eq!(content, "[Workflow] Step 1 completed.");
            }
            other => panic!("unexpected outbound message: {other:?}"),
        }

        // …and the workbench, which drops outbound chat text, sees it as a notice.
        let published = event_rx.try_recv().expect("notice event");
        assert_eq!(published.channel, "web");
        assert_eq!(published.chat_id, "web_admin");
        match published.event {
            AgentEvent::Notice { message } => {
                assert_eq!(message, "[Workflow] Step 1 completed.");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
