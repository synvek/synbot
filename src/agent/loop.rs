//! Agent loop — the core processing engine.

use anyhow::Result;
use rig::completion::CompletionRequest;
use rig::message::{AssistantContent, Message, ToolResultContent, UserContent};
use rig::OneOrMany;
use rig_dyn::CompletionModel;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn, Instrument};

use crate::agent::context::ContextBuilder;
use crate::agent::session::SessionStore;
use crate::bus::{InboundMessage, OutboundMessage};
use crate::tools::ToolRegistry;

pub struct AgentLoop {
    model: Box<dyn CompletionModel>,
    context: ContextBuilder,
    tools: Arc<ToolRegistry>,
    max_iterations: u32,
    inbound_rx: mpsc::Receiver<InboundMessage>,
    outbound_tx: broadcast::Sender<OutboundMessage>,
    sessions: std::collections::HashMap<String, Vec<Message>>,
    session_store: SessionStore,
}

impl AgentLoop {
    pub async fn new(
        model: Box<dyn CompletionModel>,
        workspace: PathBuf,
        tools: Arc<ToolRegistry>,
        max_iterations: u32,
        inbound_rx: mpsc::Receiver<InboundMessage>,
        outbound_tx: broadcast::Sender<OutboundMessage>,
    ) -> Self {
        let session_store = SessionStore::new(&workspace);

        // Restore previously persisted sessions from disk
        let sessions = match session_store.load_all_sessions().await {
            Ok(s) => {
                if !s.is_empty() {
                    info!(count = s.len(), "Restored persisted sessions");
                }
                s
            }
            Err(e) => {
                warn!(error = %e, "Failed to load persisted sessions, starting fresh");
                std::collections::HashMap::new()
            }
        };

        Self {
            context: ContextBuilder::new(&workspace),
            model,
            tools,
            max_iterations,
            inbound_rx,
            outbound_tx,
            sessions,
            session_store,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Agent loop started");
        while let Some(msg) = self.inbound_rx.recv().await {
            if let Err(e) = self.handle_message(msg).await {
                error!("Error handling message: {e:#}");
            }
        }
        Ok(())
    }

    async fn handle_message(&mut self, msg: InboundMessage) -> Result<()> {
        let session_key = msg.session_key();
        let span = tracing::info_span!(
            "handle_message",
            session_key = %session_key,
            channel = %msg.channel,
            message_length = msg.content.len(),
        );

        async {
            let start = std::time::Instant::now();
            info!(chat = %msg.chat_id, "Processing message");

            let system_prompt = self.context.build_system_prompt();
            let tool_defs = self.tools.rig_definitions();

            let history = self.sessions.entry(session_key.clone()).or_default();
            history.push(Message::user(&msg.content));

            let mut iterations = 0u32;

            loop {
                iterations += 1;
                if iterations > self.max_iterations {
                    warn!("Max iterations ({}) reached", self.max_iterations);
                    break;
                }

                let request = CompletionRequest {
                    preamble: Some(system_prompt.clone()),
                    chat_history: history.clone(),
                    prompt: Message::user(""),
                    tools: tool_defs.clone(),
                    documents: vec![],
                    temperature: None,
                    max_tokens: None,
                    additional_params: None,
                };

                tracing::debug!("Completion request: {:?}", request.tools);

                let response = self.model.completion(request).await?;

                let mut has_tool_calls = false;
                let mut text_parts = Vec::new();
                // Collect assistant content and tool results so we can append the Assistant
                // message (with tool_calls) to history *before* tool results. Without this,
                // the model never sees its own tool-call turn and keeps requesting the same
                // tool in a loop until max iterations.
                let mut assistant_contents = Vec::new();
                let mut tool_results = Vec::new();

                for content in response.iter() {
                    tracing::debug!("Received completion message: {:?}", content);
                    match content {
                        AssistantContent::Text(t) => {
                            text_parts.push(t.text.clone());
                            assistant_contents.push(content.clone());
                        }
                        AssistantContent::ToolCall(tc) => {
                            has_tool_calls = true;
                            assistant_contents.push(content.clone());
                            let args = tc.function.arguments.clone();
                            let result = self.tools.execute(&tc.function.name, args).await;
                            let result_str = match result {
                                Ok(s) => s,
                                Err(e) => format!("Error: {e}"),
                            };
                            tool_results.push((tc.id.clone(), result_str));
                        }
                    }
                }

                if has_tool_calls && !assistant_contents.is_empty() {
                    let content = match assistant_contents.len() {
                        1 => OneOrMany::one(assistant_contents.into_iter().next().unwrap()),
                        _ => OneOrMany::many(assistant_contents).expect("non-empty"),
                    };
                    history.push(Message::Assistant { content });
                    for (id, result_str) in tool_results {
                        history.push(Message::User {
                            content: OneOrMany::one(UserContent::tool_result(
                                id,
                                OneOrMany::one(ToolResultContent::text(result_str)),
                            )),
                        });
                    }
                }

                if !has_tool_calls {
                    let reply = text_parts.join("");
                    if !reply.is_empty() {
                        history.push(Message::assistant(&reply));
                        let _ = self.outbound_tx.send(OutboundMessage {
                            channel: msg.channel.clone(),
                            chat_id: msg.chat_id.clone(),
                            content: reply,
                            reply_to: None,
                            media: vec![],
                        });
                    }
                    break;
                }
            }

            let duration_ms = start.elapsed().as_millis() as u64;
            info!(
                iteration_count = iterations,
                duration_ms = duration_ms,
                "Message processing complete"
            );

            // Persist the updated session to disk.
            // Failures are logged as warnings but do not crash the agent.
            if let Some(messages) = self.sessions.get(&session_key) {
                if let Err(e) = self.session_store.save_session(&session_key, messages).await {
                    warn!(
                        session_key = %session_key,
                        error = %e,
                        "Failed to persist session, will retry on next message"
                    );
                }
            }

            Ok(())
        }
        .instrument(span)
        .await
    }
}
