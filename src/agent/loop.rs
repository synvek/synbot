//! Agent loop -- the core processing engine.

use anyhow::Result;
use rig::completion::CompletionRequest;
use rig::message::{AssistantContent, Message, ToolResultContent, UserContent};
use rig::OneOrMany;
use crate::rig_provider::SynbotCompletionModel;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::{error, info, warn, Instrument};

use crate::agent::agent_registry::AgentRegistry;
use crate::agent::context::ContextBuilder;
use crate::agent::directive::DirectiveParser;
use crate::agent::session_state::SharedSessionState;
use crate::agent::subagent::SubagentManager;
use crate::bus::{InboundMessage, OutboundMessage};
use crate::config::Config;
use crate::config;
use crate::hooks::{HookEvent, HookRegistry};
use crate::tools::{scope, ToolContext, ToolRegistry};

pub struct AgentLoop {
    model: Arc<dyn SynbotCompletionModel>,
    workspace: PathBuf,
    tools: Arc<ToolRegistry>,
    max_iterations: u32,
    inbound_rx: mpsc::Receiver<InboundMessage>,
    outbound_tx: broadcast::Sender<OutboundMessage>,
    session_state: SharedSessionState,
    agent_registry: Arc<AgentRegistry>,
    subagent_manager: Arc<Mutex<SubagentManager>>,
    tool_sandbox_enabled: bool,
    hooks: Option<Arc<HookRegistry>>,
    tool_result_preview_chars: usize,
}

impl AgentLoop {
    pub async fn new(
        model: Arc<dyn SynbotCompletionModel>,
        workspace: PathBuf,
        tools: Arc<ToolRegistry>,
        max_iterations: u32,
        inbound_rx: mpsc::Receiver<InboundMessage>,
        outbound_tx: broadcast::Sender<OutboundMessage>,
        config: &Config,
        session_state: SharedSessionState,
        agent_registry: Arc<AgentRegistry>,
        tool_sandbox_enabled: bool,
        hooks: Option<Arc<HookRegistry>>,
    ) -> Self {
        let subagent_manager = SubagentManager::new(config.main_agent.max_concurrent_subagents);
        let tool_result_preview_chars = config.tool_result_preview_chars as usize;
        Self {
            workspace,
            model,
            tools,
            max_iterations,
            inbound_rx,
            outbound_tx,
            session_state,
            agent_registry,
            subagent_manager: Arc::new(Mutex::new(subagent_manager)),
            tool_sandbox_enabled,
            hooks,
            tool_result_preview_chars,
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
        let span = tracing::info_span!(
            "handle_message",
            channel = %msg.channel,
            message_length = msg.content.len(),
        );
        async {
            // When trigger_agent is false (e.g. not in allowlist, or group message not @bot),
            // still append the message to session and persist so the user can see history and update allowlist.
            let trigger_agent = msg
                .metadata
                .get("trigger_agent")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if !trigger_agent {
                self.save_message_only(&msg).await?;
                return Ok(());
            }

            let start = std::time::Instant::now();
            if let Some(ref h) = self.hooks {
                h.dispatch(HookEvent::MessageReceived(msg.clone())).await;
            }
            info!(chat = %msg.chat_id, "Processing message");
            let directives = DirectiveParser::parse(&msg.content);
            if directives.len() <= 1 {
                self.process_directives_sequential(&msg, &directives, start).await?;
            } else {
                self.process_directives_parallel(&msg, &directives, start).await?;
            }
            Ok(())
        }
        .instrument(span)
        .await
    }

    /// Append the message to the main agent's session and persist to disk, without running completion.
    /// Used when the chat is not in allowlist or when it's a group message not directed at the bot.
    async fn save_message_only(&mut self, msg: &InboundMessage) -> Result<()> {
        let agent_id = "main";
        let session_id = {
            let sm = self.session_state.session_manager.write().await;
            sm.resolve_session(agent_id, &msg.channel, &msg.chat_id, &msg.metadata)
        };
        let session_key = session_id.format();

        let user_content = msg.content.clone();
        let session_messages = self.session_state.get_or_create_session_messages(&session_key).await;
        {
            let mut history = session_messages.lock().await;
            history.push(Message::user(&user_content));
        }
        {
            let mut sm = self.session_state.session_manager.write().await;
            let user_msg = crate::agent::session::SessionMessage {
                role: "user".to_string(),
                content: user_content.clone(),
                timestamp: chrono::Utc::now(),
            };
            sm.append(&session_id, user_msg);
        }

        let messages = session_messages.lock().await.clone();
        let now = chrono::Utc::now();
        let meta = crate::agent::session_manager::SessionMeta {
            id: session_id.clone(),
            participants: vec![
                format!("{}:{}", msg.channel, msg.sender_id),
                format!("agent:{}", agent_id),
            ],
            created_at: now,
            updated_at: now,
        };
        {
            let mut sm = self.session_state.session_manager.write().await;
            let session_messages_sm: Vec<crate::agent::session::SessionMessage> =
                messages.iter().map(crate::agent::session::SessionMessage::from_message).collect();
            let history = sm.get_or_create(&session_id);
            history.clear();
            history.extend(session_messages_sm);
        }
        if let Err(e) = self.session_state.session_store.save_session(&session_key, &messages, Some(&meta)).await {
            warn!(
                session_key = %session_key,
                error = %e,
                "Failed to persist session (save_message_only)"
            );
        }
        Ok(())
    }

    async fn process_directives_sequential(
        &mut self,
        msg: &InboundMessage,
        directives: &[crate::agent::directive::Directive],
        start: std::time::Instant,
    ) -> Result<()> {
        for directive in directives {
            let agent_id = match &directive.target {
                None => "main".to_string(),
                Some(name) => {
                    if !self.agent_registry.contains(name) {
                        self.send_unknown_agent_error(msg, name).await;
                        continue;
                    }
                    name.clone()
                }
            };

            let agent_ctx = self.agent_registry.get(&agent_id).unwrap();
            let context_builder = ContextBuilder::new(
                &agent_ctx.workspace_dir,
                &agent_id,
                config::skills_dir().as_path(),
                self.tool_sandbox_enabled,
            );
            let system_prompt = context_builder.build_system_prompt_with_role_prompt(&agent_ctx.system_prompt);
            let model_max_iterations = agent_ctx.params.max_iterations;

            let session_id = {
                let sm = self.session_state.session_manager.write().await;
                sm.resolve_session(&agent_id, &msg.channel, &msg.chat_id, &msg.metadata)
            };
            let session_key = session_id.format();

            let agent_workspace = agent_ctx.workspace_dir.clone();
            let tool_ctx = ToolContext {
                agent_id: agent_id.clone(),
                workspace: agent_workspace,
            };

            // When message is a response to a pending approval, prepend instruction so the agent calls submit_approval_response
            let base_content = if agent_id == "main" {
                directive.content.clone()
            } else {
                format!("@@{} {}", agent_id, directive.content)
            };
            let user_content = if let Some(rid) = msg.metadata.get("pending_approval_request_id").and_then(|v| v.as_str()) {
                format!(
                    "[Context: The user is responding to a pending command approval request (request_id: {}). Interpret their message as approve or reject and call submit_approval_response with request_id \"{}\" and approved (true or false).]\n\nUser: {}",
                    rid, rid, base_content
                )
            } else {
                base_content
            };

            let tool_defs = self.tools.rig_definitions();
            let session_messages = self.session_state.get_or_create_session_messages(&session_key).await;
            {
                let mut history = session_messages.lock().await;
                history.push(Message::user(&user_content));
            }
            // Update session_manager with the user message
            {
                let mut sm = self.session_state.session_manager.write().await;
                let user_msg = crate::agent::session::SessionMessage {
                    role: "user".to_string(),
                    content: user_content.clone(),
                    timestamp: chrono::Utc::now(),
                };
                sm.append(&session_id, user_msg);
            }

            let mut history_guard = session_messages.lock().await;
            tracing::debug!("History check: {:?}", *history_guard);
            self.session_state.set_active(&session_key, "processing").await;
            if let Some(ref h) = self.hooks {
                let directive_preview = if directive.content.len() > 200 {
                    format!("{}...", &directive.content[..200])
                } else {
                    directive.content.clone()
                };
                h.dispatch(HookEvent::AgentRunStart {
                    agent_id: agent_id.clone(),
                    directive_preview,
                })
                .await;
            }
            let run_result = scope(tool_ctx, async {
                run_completion_loop(
                    &*self.model,
                    &system_prompt,
                    model_max_iterations,
                    &agent_id,
                    &mut *history_guard,
                    &tool_defs,
                    &self.tools,
                    &msg.channel,
                    &msg.chat_id,
                    &msg.sender_id,
                    &session_key,
                    &self.outbound_tx,
                    self.hooks.clone(),
                    self.tool_result_preview_chars,
                )
                .await
            })
            .await;
            if let Some(ref h) = self.hooks {
                let iterations = run_result.as_ref().copied().unwrap_or(0);
                h.dispatch(HookEvent::AgentRunEnd {
                    agent_id: agent_id.clone(),
                    iteration_count: iterations,
                    duration_ms: start.elapsed().as_millis() as u64,
                })
                .await;
            }
            if let Err(ref e) = run_result {
                self.session_state.clear_active(&session_key).await;
                return Err(anyhow::anyhow!("{}", e));
            }
            let iterations = run_result?;
            self.session_state.clear_active(&session_key).await;

            drop(history_guard);

            info!(
                agent_id = %agent_id,
                iteration_count = iterations,
                duration_ms = start.elapsed().as_millis() as u64,
                "Directive processing complete"
            );

            // Persist session with metadata and sync to session_manager
            let messages = session_messages.lock().await.clone();
            let now = chrono::Utc::now();
            let meta = crate::agent::session_manager::SessionMeta {
                id: session_id.clone(),
                participants: vec![
                    format!("{}:{}", msg.channel, msg.sender_id),
                    format!("agent:{}", agent_id),
                ],
                created_at: now,
                updated_at: now,
            };
            {
                let mut sm = self.session_state.session_manager.write().await;
                let session_messages_sm: Vec<crate::agent::session::SessionMessage> =
                    messages.iter().map(crate::agent::session::SessionMessage::from_message).collect();
                let history = sm.get_or_create(&session_id);
                history.clear();
                history.extend(session_messages_sm);
            }
            if let Err(e) = self.session_state.session_store.save_session(&session_key, &messages, Some(&meta)).await {
                warn!(
                    session_key = %session_key,
                    error = %e,
                    "Failed to persist session, will retry on next message"
                );
            }
        }
        Ok(())
    }

    async fn process_directives_parallel(
        &mut self,
        msg: &InboundMessage,
        directives: &[crate::agent::directive::Directive],
        _start: std::time::Instant,
    ) -> Result<()> {
        let mut spawned_ids: Vec<(String, String, String)> = Vec::new();

        for directive in directives {
            let agent_id = match &directive.target {
                None => "main".to_string(),
                Some(name) => {
                    if !self.agent_registry.contains(name) {
                        self.send_unknown_agent_error(msg, name).await;
                        continue;
                    }
                    name.clone()
                }
            };

            let agent_ctx = self.agent_registry.get(&agent_id).unwrap();
            let context_builder = ContextBuilder::new(
                &agent_ctx.workspace_dir,
                &agent_id,
                config::skills_dir().as_path(),
                self.tool_sandbox_enabled,
            );
            let system_prompt = context_builder.build_system_prompt_with_role_prompt(&agent_ctx.system_prompt);
            let model_max_iterations = agent_ctx.params.max_iterations;

            let session_id = {
                let sm = self.session_state.session_manager.write().await;
                sm.resolve_session(&agent_id, &msg.channel, &msg.chat_id, &msg.metadata)
            };
            let session_key = session_id.format();

            let tool_defs = self.tools.rig_definitions();

            let base_content = if agent_id == "main" {
                directive.content.clone()
            } else {
                format!("@@{} {}", agent_id, directive.content)
            };
            let user_content = if let Some(rid) = msg.metadata.get("pending_approval_request_id").and_then(|v| v.as_str()) {
                format!(
                    "[Context: The user is responding to a pending command approval request (request_id: {}). Interpret their message as approve or reject and call submit_approval_response with request_id \"{}\" and approved (true or false).]\n\nUser: {}",
                    rid, rid, base_content
                )
            } else {
                base_content
            };

            // Push user message into session history before spawning
            let session_messages = self.session_state.get_or_create_session_messages(&session_key).await;
            {
                let mut history = session_messages.lock().await;
                history.push(Message::user(&user_content));
            }
            {
                let mut sm = self.session_state.session_manager.write().await;
                let user_msg = crate::agent::session::SessionMessage {
                    role: "user".to_string(),
                    content: user_content.clone(),
                    timestamp: chrono::Utc::now(),
                };
                sm.append(&session_id, user_msg);
            }

            let agent_workspace = agent_ctx.workspace_dir.clone();

            let model = Arc::clone(&self.model);
            let tools = Arc::clone(&self.tools);
            let session_state = self.session_state.clone();
            let outbound_tx = self.outbound_tx.clone();
            let hooks = self.hooks.clone();
            let channel = msg.channel.clone();
            let channel_for_meta = channel.clone();
            let chat_id = msg.chat_id.clone();
            let sender_id = msg.sender_id.clone();
            let sender_id_for_loop = msg.sender_id.clone();
            let sk = session_key.clone();
            let aid = agent_id.clone();
            let aid_for_meta = aid.clone();
            let sid = session_id.clone();
            let tool_ctx = ToolContext {
                agent_id: aid.clone(),
                workspace: agent_workspace,
            };
            let tool_result_preview_chars = self.tool_result_preview_chars;

            let session_messages_clone = session_messages.clone();
            let task_future = Box::pin(async move {
                session_state.set_active(&sk, "processing").await;
                let mut history_guard = session_messages_clone.lock().await;
                let session_id_str = sk.clone();
                let run_result = scope(tool_ctx, async move {
                    let it = run_completion_loop(
                        &*model,
                        &system_prompt,
                        model_max_iterations,
                        &aid,
                        &mut *history_guard,
                        &tool_defs,
                        &tools,
                        &channel,
                        &chat_id,
                        &sender_id_for_loop,
                        &session_id_str,
                        &outbound_tx,
                        hooks.clone(),
                        tool_result_preview_chars,
                    )
                    .await?;
                    let messages = history_guard.clone();
                    Ok::<_, anyhow::Error>((it, messages))
                })
                .await;
                if let Err(ref e) = run_result {
                    session_state.clear_active(&sk).await;
                    return Err(anyhow::anyhow!("{}", e));
                }
                let (iterations, messages) = run_result?;

                // Persist session with metadata and sync to session_manager
                session_state.clear_active(&sk).await;
                let now = chrono::Utc::now();
                let meta = crate::agent::session_manager::SessionMeta {
                    id: sid.clone(),
                    participants: vec![
                        format!("{}:{}", channel_for_meta, sender_id),
                        format!("agent:{}", aid_for_meta),
                    ],
                    created_at: now,
                    updated_at: now,
                };
                {
                    let mut sm = session_state.session_manager.write().await;
                    let session_messages_sm: Vec<crate::agent::session::SessionMessage> =
                        messages.iter().map(crate::agent::session::SessionMessage::from_message).collect();
                    let history = sm.get_or_create(&sid);
                    history.clear();
                    history.extend(session_messages_sm);
                }
                if let Err(e) = session_state.session_store.save_session(&sk, &messages, Some(&meta)).await {
                    warn!(
                        session_key = %sk,
                        error = %e,
                        "Failed to persist session, will retry on next message"
                    );
                }

                Ok(format!("agent={}, iterations={}", aid_for_meta, iterations))
            });

            let label = format!(
                "directive:{}:{}",
                agent_id,
                directive.content.chars().take(30).collect::<String>()
            );

            let subagent_id = {
                let mut mgr = self.subagent_manager.lock().await;
                mgr.spawn_fn(label, task_future).await
            };

            match subagent_id {
                Ok(id) => {
                    spawned_ids.push((id, agent_id.clone(), session_key));
                }
                Err(e) => {
                    warn!(
                        agent_id = %agent_id,
                        error = %e,
                        "Failed to spawn parallel directive task"
                    );
                    let _ = self.outbound_tx.send(OutboundMessage::chat(
                        msg.channel.clone(),
                        msg.chat_id.clone(),
                        format!("Agent '{}' is busy, please retry later", agent_id),
                        vec![],
                        None,
                    ));
                }
            }
        }

        // Wait for all spawned tasks to complete
        if !spawned_ids.is_empty() {
            let ids: Vec<String> = spawned_ids.iter().map(|(id, _, _)| id.clone()).collect();
            loop {
                let mut all_done = true;
                {
                    let mgr = self.subagent_manager.lock().await;
                    for id in &ids {
                        if let Some(handle) = mgr.get_result(id).await {
                            if matches!(handle.status, crate::agent::subagent::SubagentStatus::Running) {
                                all_done = false;
                                break;
                            }
                        }
                    }
                }
                if all_done {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }

        Ok(())
    }

    async fn send_unknown_agent_error(&self, msg: &InboundMessage, unknown_agent: &str) {
        let available = self.agent_registry.list_names();
        let agent_list = if available.is_empty() {
            "(no registered agents)".to_string()
        } else {
            available.join(", ")
        };
        let _ = self.outbound_tx.send(OutboundMessage::chat(
            msg.channel.clone(),
            msg.chat_id.clone(),
            format!("Unknown agent '{}'. Available: {}", unknown_agent, agent_list),
            vec![],
            None,
        ));
    }
}

// ---------------------------------------------------------------------------
// Standalone completion loop
// ---------------------------------------------------------------------------

async fn run_completion_loop(
    model: &dyn SynbotCompletionModel,
    system_prompt: &str,
    max_iterations: u32,
    agent_id: &str,
    history: &mut Vec<Message>,
    tool_defs: &[rig::completion::ToolDefinition],
    tools: &ToolRegistry,
    channel: &str,
    chat_id: &str,
    sender_id: &str,
    session_id: &str,
    outbound_tx: &broadcast::Sender<OutboundMessage>,
    hooks: Option<Arc<HookRegistry>>,
    tool_result_preview_chars: usize,
) -> Result<u32> {
    let message_ctx = Some((channel, chat_id, sender_id, session_id));
    let mut iterations = 0u32;

    loop {
        iterations += 1;
        if iterations > max_iterations {
            warn!("Max iterations ({}) reached for agent '{}'", max_iterations, agent_id);
            break;
        }

        tracing::debug!("History check again: {:?}", history);

        let chat_history = if history.is_empty() {
            OneOrMany::one(Message::user(""))
        } else {
            OneOrMany::many(history.clone()).expect("non-empty history")
        };
        let request = CompletionRequest {
            preamble: Some(system_prompt.to_string()),
            chat_history,
            tools: tool_defs.to_vec(),
            documents: vec![],
            temperature: None,
            max_tokens: None,
            tool_choice: None,
            additional_params: None,
        };

        tracing::debug!("Request prompt: {:?}", request);

        let response = model
            .completion(request)
            .await
            .map_err(|e| anyhow::anyhow!("completion failed (agent_id={}): {}", agent_id, e))?;

        let mut has_tool_calls = false;
        let mut text_parts = Vec::new();
        let mut assistant_contents = Vec::new();
        let mut tool_results = Vec::new();

        for content in response.choice.clone().into_iter() {
            match &content {
                AssistantContent::Text(t) => {
                    text_parts.push(t.text.clone());
                    assistant_contents.push(content.clone());
                }
                AssistantContent::Reasoning(_) | AssistantContent::Image(_) => {
                    // Preserve reasoning/image in history (e.g. DeepSeek requires reasoning_content in assistant messages)
                    assistant_contents.push(content.clone());
                }
                AssistantContent::ToolCall(tc) => {
                    has_tool_calls = true;
                    assistant_contents.push(content.clone());
                    let args = tc.function.arguments.clone();
                    let args_str = serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_string());
                    let args_preview = if args_str.len() > 200 {
                        let mut end = 200;
                        while end > 0 && !args_str.is_char_boundary(end) {
                            end -= 1;
                        }
                        format!("{}...", &args_str[..end])
                    } else {
                        args_str
                    };
                    if let Some(ref h) = hooks {
                        h.dispatch(HookEvent::ToolRunStart {
                            tool_name: tc.function.name.clone(),
                            args_preview,
                            channel: channel.to_string(),
                            chat_id: chat_id.to_string(),
                            session_id: session_id.to_string(),
                        })
                        .await;
                    }
                    let result = tools.execute(&tc.function.name, args, message_ctx).await;
                    let result_str = match &result {
                        Ok(s) => s.clone(),
                        Err(e) => format!("Error: {e}"),
                    };
                    let status = if result.is_ok() {
                        "success"
                    } else {
                        "failure"
                    };
                    let preview = if result_str.len() > tool_result_preview_chars {
                        let mut end = tool_result_preview_chars;
                        while end > 0 && !result_str.is_char_boundary(end) {
                            end -= 1;
                        }
                        format!("{}...", &result_str[..end])
                    } else {
                        result_str.clone()
                    };
                    if let Some(ref h) = hooks {
                        h.dispatch(HookEvent::ToolRunEnd {
                            tool_name: tc.function.name.clone(),
                            result_preview: preview.clone(),
                            success: result.is_ok(),
                        })
                        .await;
                    }
                    let _ = outbound_tx.send(OutboundMessage::tool_progress(
                        channel.to_string(),
                        chat_id.to_string(),
                        tc.function.name.clone(),
                        status.to_string(),
                        preview,
                    ));
                    tool_results.push((tc.id.clone(), result_str));
                }
            }
        }

        if has_tool_calls && !assistant_contents.is_empty() {
            let content = match assistant_contents.len() {
                1 => OneOrMany::one(assistant_contents.into_iter().next().unwrap()),
                _ => OneOrMany::many(assistant_contents).expect("non-empty"),
            };
            history.push(Message::Assistant {
                id: None,
                content,
            });
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
                let out_msg = OutboundMessage::chat(
                    channel.to_string(),
                    chat_id.to_string(),
                    reply.clone(),
                    vec![],
                    None,
                );
                if let Some(ref h) = hooks {
                    h.dispatch(HookEvent::MessageSent(out_msg.clone())).await;
                }
                let _ = outbound_tx.send(out_msg);
            }
            break;
        }
    }

    Ok(iterations)
}

