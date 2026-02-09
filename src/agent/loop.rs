//! Agent loop — the core processing engine.

use anyhow::Result;
use rig::completion::CompletionRequest;
use rig::message::{AssistantContent, Message, ToolResultContent, UserContent};
use rig::OneOrMany;
use rig_dyn::CompletionModel;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};

use crate::agent::context::ContextBuilder;
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
}

impl AgentLoop {
    pub fn new(
        model: Box<dyn CompletionModel>,
        workspace: PathBuf,
        tools: Arc<ToolRegistry>,
        max_iterations: u32,
        inbound_rx: mpsc::Receiver<InboundMessage>,
        outbound_tx: broadcast::Sender<OutboundMessage>,
    ) -> Self {
        Self {
            context: ContextBuilder::new(&workspace),
            model,
            tools,
            max_iterations,
            inbound_rx,
            outbound_tx,
            sessions: std::collections::HashMap::new(),
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
        info!(channel = %msg.channel, chat = %msg.chat_id, "Processing message");

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

            let response = self.model.completion(request).await?;

            let mut has_tool_calls = false;
            let mut text_parts = Vec::new();

            for content in response.iter() {
                match content {
                    AssistantContent::Text(t) => {
                        text_parts.push(t.text.clone());
                    }
                    AssistantContent::ToolCall(tc) => {
                        has_tool_calls = true;
                        // tc.function.arguments is already a Value, not a string
                        let args = tc.function.arguments.clone();
                        let result = self.tools.execute(&tc.function.name, args).await;
                        let result_str = match result {
                            Ok(s) => s,
                            Err(e) => format!("Error: {e}"),
                        };
                        // Tool results are sent as User messages with ToolResult content
                        history.push(Message::User {
                            content: OneOrMany::one(UserContent::tool_result(
                                tc.id.clone(),
                                OneOrMany::one(ToolResultContent::text(result_str)),
                            )),
                        });
                    }
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

        Ok(())
    }
}
