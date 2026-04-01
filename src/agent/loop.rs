//! Agent loop -- the core processing engine.

use anyhow::Result;
use rig::completion::CompletionRequest;
use rig::message::{AssistantContent, Message, ToolResultContent, UserContent};
use rig::OneOrMany;
use crate::rig_provider::SynbotCompletionModel;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn, Instrument};

use crate::agent::agent_registry::AgentRegistry;
use crate::agent::context::ContextBuilder;
use crate::agent::directive::DirectiveParser;
use crate::agent::session_state::SharedSessionState;
use crate::agent::subagent::{SubagentManager, SubagentStatus};
use crate::bus::{InboundMessage, OutboundMessage};
use crate::config::{self, Config};
use crate::sandbox::types::ToolSandboxExecKind;
use crate::hooks::{HookEvent, HookRegistry};
use crate::tools::{scope, ToolContext, ToolRegistry};
use crate::agent::control_commands::{parse_control_command, busy_hint_commands, ControlCommand};
use crate::workflow::{
    generate_workflow, parse_workflow_trigger, run_workflow, PendingConfirmStore,
    PendingWorkflowInputStore, WorkflowDef, WorkflowState, WorkflowStore, WorkflowTrigger,
};

/// Prefix user text when a channel sets `metadata.sender_display` (e.g. DingTalk group: nick + id).
fn apply_sender_display_prefix(content: &str, metadata: &serde_json::Value) -> String {
    match metadata
        .get("sender_display")
        .and_then(|v| v.as_str())
        .map(str::trim)
    {
        Some(label) if !label.is_empty() => format!("[{label}]\n{content}"),
        _ => content.to_string(),
    }
}

pub struct AgentLoop {
    model: Arc<dyn SynbotCompletionModel>,
    workspace: PathBuf,
    tools: Arc<ToolRegistry>,
    max_iterations: u32,
    outbound_tx: broadcast::Sender<OutboundMessage>,
    session_state: SharedSessionState,
    agent_registry: Arc<AgentRegistry>,
    subagent_manager: Arc<Mutex<SubagentManager>>,
    tool_sandbox_exec_kind: Option<ToolSandboxExecKind>,
    hooks: Option<Arc<HookRegistry>>,
    tool_result_preview_chars: usize,
    workflow_store: WorkflowStore,
    pending_workflow_input: PendingWorkflowInputStore,
    pending_workflow_confirm: PendingConfirmStore,
    workflow_user_input_timeout_secs: u64,
}

impl AgentLoop {
    pub async fn new(
        model: Arc<dyn SynbotCompletionModel>,
        workspace: PathBuf,
        tools: Arc<ToolRegistry>,
        max_iterations: u32,
        outbound_tx: broadcast::Sender<OutboundMessage>,
        config: &Config,
        session_state: SharedSessionState,
        agent_registry: Arc<AgentRegistry>,
        tool_sandbox_exec_kind: Option<ToolSandboxExecKind>,
        hooks: Option<Arc<HookRegistry>>,
    ) -> Self {
        let subagent_manager = SubagentManager::new(
            config.main_agent.max_concurrent_subagents,
            Some(config.main_agent.subagent_task_timeout_secs),
        );
        let tool_result_preview_chars = config.tool_result_preview_chars as usize;
        let workflow_store = WorkflowStore::new(config::workflows_root(config).as_path());
        let workflow_user_input_timeout_secs = config.workflow.user_input_timeout_secs;
        Self {
            workspace,
            model,
            tools,
            max_iterations,
            outbound_tx,
            session_state,
            agent_registry,
            subagent_manager: Arc::new(Mutex::new(subagent_manager)),
            tool_sandbox_exec_kind,
            hooks,
            tool_result_preview_chars,
            workflow_store,
            pending_workflow_input: PendingWorkflowInputStore::new(),
            pending_workflow_confirm: PendingConfirmStore::new(),
            workflow_user_input_timeout_secs,
        }
    }

    /// Run the agent loop. Requires the loop to be in an `Arc<Mutex<>>` so that /stop (or /cancel)
    /// can cancel a running agent task by spawning and tracking it. Pass the receiver so the
    /// loop can recv without holding the lock (allowing the spawned task to run).
    pub async fn run(
        loop_ref: Arc<Mutex<Self>>,
        mut inbound_rx: mpsc::Receiver<InboundMessage>,
    ) -> Result<()> {
        info!("Agent loop started");
        type TaskHandle = (tokio::task::JoinHandle<()>, CancellationToken, String);
        let mut current_task: Option<TaskHandle> = None;

        loop {
            if let Some((mut handle, token, running_session_key)) = current_task.take() {
                // Something is running: wait for either a new message or task completion.
                tokio::select! {
                    msg_opt = inbound_rx.recv() => {
                        let msg = match msg_opt {
                            None => { current_task = Some((handle, token, running_session_key)); break; }
                            Some(m) => m,
                        };
                        if let Some(cmd) = parse_control_command(&msg.content) {
                            match cmd {
                                ControlCommand::Stop => {
                                    token.cancel();
                                    let _ = handle.await;
                                    let _ = loop_ref.lock().await.outbound_tx.send(OutboundMessage::chat(
                                        msg.channel.clone(),
                                        msg.chat_id.clone(),
                                        "[Control] Stopped.".to_string(),
                                        vec![],
                                        None,
                                    ));
                                    continue;
                                }
                                ControlCommand::Status => {
                                    loop_ref.lock().await.handle_status(&msg).await.ok();
                                }
                                ControlCommand::Clear => {
                                    loop_ref.lock().await.handle_clear(&msg).await.ok();
                                }
                                ControlCommand::Resume => {
                                    let loop_guard = loop_ref.lock().await;
                                    if let Some(ref k) = loop_guard.resolve_history_session_key(&msg).await {
                                        let _ = loop_guard.session_state.append_user_message_and_save(k, &msg.content).await;
                                    }
                                    if let Err(e) = loop_guard.handle_workflow_continue(&msg, &msg.session_key()).await {
                                        let _ = loop_guard.outbound_tx.send(OutboundMessage::chat(
                                            msg.channel.clone(),
                                            msg.chat_id.clone(),
                                            format!("[Workflow] Resume failed: {}", e),
                                            vec![],
                                            None,
                                        ));
                                    }
                                }
                            }
                        } else if msg.session_key() == running_session_key {
                            let _ = loop_ref.lock().await.outbound_tx.send(OutboundMessage::chat(
                                msg.channel.clone(),
                                msg.chat_id.clone(),
                                format!("[Control] Busy. {}", busy_hint_commands()),
                                vec![],
                                None,
                            ));
                        } else {
                            let _ = loop_ref.lock().await.outbound_tx.send(OutboundMessage::chat(
                                msg.channel.clone(),
                                msg.chat_id.clone(),
                                "Agent is busy with another conversation. Use /status to see state.".to_string(),
                                vec![],
                                None,
                            ));
                        }
                        current_task = Some((handle, token, running_session_key));
                    }
                    _ = &mut handle => {
                        // Task finished
                    }
                }
            } else {
                let msg = match inbound_rx.recv().await {
                    None => break,
                    Some(m) => m,
                };
                let result = {
                    let mut guard = loop_ref.lock().await;
                    guard.handle_message(msg, loop_ref.clone()).await
                };
                match result {
                    Err(e) => error!("Error handling message: {e:#}"),
                    Ok(Some(task_triple)) => current_task = Some(task_triple),
                    Ok(None) => {}
                }
            }
        }
        Ok(())
    }

    /// Returns Ok(Some((handle, token, session_key))) when a workflow or agent run was started (so run() can track for /stop).
    async fn handle_message(
        &mut self,
        msg: InboundMessage,
        loop_ref: Arc<Mutex<AgentLoop>>,
    ) -> Result<Option<(tokio::task::JoinHandle<()>, CancellationToken, String)>> {
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
                return Ok(None);
            }

            let session_key = msg.session_key();

            // If this session is waiting for workflow user input, deliver and exit.
            if self.pending_workflow_input.deliver(&session_key, msg.content.clone()).await {
                return Ok(None);
            }

            // If user previously got "confirm?" and now confirms (intent-based, same as approval), start workflow.
            if let Some(def) = self
                .pending_workflow_confirm
                .take_if_confirm(&session_key, &msg.content, self.model.as_ref())
                .await
            {
                if let Some(ref k) = self.resolve_history_session_key(&msg).await {
                    let _ = self.session_state.append_user_message_and_save(k, &msg.content).await;
                }
                let task = self.start_workflow_with_def(&msg, &session_key, def).await?;
                return Ok(task);
            }

            // Control commands: /status, /clear, /resume, /stop (handled in run() when task is running).
            if let Some(cmd) = parse_control_command(&msg.content) {
                match cmd {
                    ControlCommand::Status => {
                        self.handle_status(&msg).await?;
                        return Ok(None);
                    }
                    ControlCommand::Clear => {
                        self.handle_clear(&msg).await?;
                        return Ok(None);
                    }
                    ControlCommand::Resume => {
                        if let Some(ref k) = self.resolve_history_session_key(&msg).await {
                            let _ = self.session_state.append_user_message_and_save(k, &msg.content).await;
                        }
                        match self.handle_workflow_continue(&msg, &session_key).await {
                            Ok(Some(task)) => return Ok(Some(task)),
                            Ok(None) => {}
                            Err(e) => {
                                let _ = self.outbound_tx.send(OutboundMessage::chat(
                                    msg.channel.clone(),
                                    msg.chat_id.clone(),
                                    format!("[Workflow] Resume failed: {}", e),
                                    vec![],
                                    None,
                                ));
                            }
                        }
                        return Ok(None);
                    }
                    ControlCommand::Stop => {
                        let _ = self.outbound_tx.send(OutboundMessage::chat(
                            msg.channel.clone(),
                            msg.chat_id.clone(),
                            "[Control] Nothing is currently running. Use /status to check.".to_string(),
                            vec![],
                            None,
                        ));
                        return Ok(None);
                    }
                }
            }

            match parse_workflow_trigger(&msg.content) {
                WorkflowTrigger::Continue => {
                    if let Some(ref k) = self.resolve_history_session_key(&msg).await {
                        let _ = self.session_state.append_user_message_and_save(k, &msg.content).await;
                    }
                    match self.handle_workflow_continue(&msg, &session_key).await {
                        Ok(Some(task)) => return Ok(Some(task)),
                        Ok(None) => {}
                        Err(e) => {
                            let _ = self.outbound_tx.send(OutboundMessage::chat(
                                msg.channel.clone(),
                                msg.chat_id.clone(),
                                format!("[Workflow] Continue failed: {}", e),
                                vec![],
                                None,
                            ));
                        }
                    }
                    return Ok(None);
                }
                WorkflowTrigger::Create { description, user_provided_def } => {
                    if let Some(ref k) = self.resolve_history_session_key(&msg).await {
                        let _ = self.session_state.append_user_message_and_save(k, &msg.content).await;
                    }
                    match self.handle_workflow_create(&msg, &session_key, &description, user_provided_def).await {
                        Ok(Some(task)) => return Ok(Some(task)),
                        Ok(None) => {}
                        Err(e) => {
                            let _ = self.outbound_tx.send(OutboundMessage::chat(
                                msg.channel.clone(),
                                msg.chat_id.clone(),
                                format!("[Workflow] Create failed: {}", e),
                                vec![],
                                None,
                            ));
                        }
                    }
                    return Ok(None);
                }
                WorkflowTrigger::None => {}
            }

            let start = std::time::Instant::now();
            if let Some(ref h) = self.hooks {
                h.dispatch(HookEvent::MessageReceived(msg.clone())).await;
            }
            info!(chat = %msg.chat_id, "Processing message");
            let directives = DirectiveParser::parse(&msg.content);
            if directives.len() <= 1 {
                // Spawn agent run so /stop or /cancel can cancel it; run() will track (handle, token, session_key).
                let token = CancellationToken::new();
                let token_for_spawn = token.clone();
                let session_key = msg.session_key();
                let msg_clone = msg.clone();
                let directives_clone = directives.clone();
                let loop_ref_spawn = loop_ref.clone();
                let handle = tokio::spawn(async move {
                    let mut guard = loop_ref_spawn.lock().await;
                    if let Err(e) = guard
                        .process_directives_sequential(&msg_clone, &directives_clone, start, Some(token_for_spawn))
                        .await
                    {
                        tracing::debug!("Agent run ended (cancelled or error): {}", e);
                    }
                });
                return Ok(Some((handle, token, session_key)));
            } else {
                self.process_directives_parallel(&msg, &directives, start).await?;
            }
            Ok(None)
        }
        .instrument(span)
        .await
    }

    /// Returns Some((handle, token, session_key)) when a workflow was started (caller can track for /stop).
    async fn handle_workflow_continue(
        &self,
        msg: &InboundMessage,
        session_key: &str,
    ) -> Result<Option<(tokio::task::JoinHandle<()>, CancellationToken, String)>> {
        let state: WorkflowState = match self.workflow_store.load_state(session_key).await? {
            Some(s) => s,
            None => {
                let _ = self.outbound_tx.send(OutboundMessage::chat(
                    msg.channel.clone(),
                    msg.chat_id.clone(),
                    "[Workflow] No workflow in progress for this session.".to_string(),
                    vec![],
                    None,
                ));
                return Ok(None);
            }
        };
        if state.is_finished() {
            let _ = self.outbound_tx.send(OutboundMessage::chat(
                msg.channel.clone(),
                msg.chat_id.clone(),
                "[Workflow] Workflow already ended.".to_string(),
                vec![],
                None,
            ));
            return Ok(None);
        }
        let history_session_key = self.resolve_history_session_key(msg).await;
        let token = CancellationToken::new();
        let handle = self.spawn_workflow_run(msg, session_key, history_session_key, state, Some(token.clone()));
        Ok(Some((handle, token, session_key.to_string())))
    }

    async fn handle_workflow_create(
        &self,
        msg: &InboundMessage,
        session_key: &str,
        description: &str,
        user_provided_def: Option<WorkflowDef>,
    ) -> Result<Option<(tokio::task::JoinHandle<()>, CancellationToken, String)>> {
        if let Some(def) = user_provided_def {
            if let Err(e) = def.validate() {
                let _ = self.outbound_tx.send(OutboundMessage::chat(
                    msg.channel.clone(),
                    msg.chat_id.clone(),
                    format!("[Workflow] Definition validation failed: {}. Please fix and retry.", e),
                    vec![],
                    None,
                ));
                return Ok(None);
            }
            let steps_preview: String = def
                .steps
                .iter()
                .enumerate()
                .map(|(i, s)| format!("{}. {} ({})", i + 1, s.id, s.step_type))
                .collect::<Vec<_>>()
                .join("\n");
            self.pending_workflow_confirm.set(session_key, def).await;
            let _ = self.outbound_tx.send(OutboundMessage::chat(
                msg.channel.clone(),
                msg.chat_id.clone(),
                format!(
                    "[Workflow] Parsed steps:\n{}\nReply to approve and start (any language).",
                    steps_preview
                ),
                vec![],
                None,
            ));
            return Ok(None);
        }
        let def = generate_workflow(self.model.as_ref(), description).await?;
        self.start_workflow_with_def(msg, session_key, def).await
    }

    /// Returns Some((handle, token, session_key)) when workflow was started (caller can track for /stop).
    async fn start_workflow_with_def(
        &self,
        msg: &InboundMessage,
        session_key: &str,
        def: WorkflowDef,
    ) -> Result<Option<(tokio::task::JoinHandle<()>, CancellationToken, String)>> {
        let initial_inputs = std::collections::HashMap::new();
        let state = WorkflowState::new(
            session_key.to_string(),
            def,
            initial_inputs,
            self.workflow_user_input_timeout_secs,
        );
        self.workflow_store.save_state(session_key, &state).await?;
        let history_session_key = self.resolve_history_session_key(msg).await;
        let created_msg = "[Workflow] Created and started.".to_string();
        let _ = self.outbound_tx.send(OutboundMessage::chat(
            msg.channel.clone(),
            msg.chat_id.clone(),
            created_msg.clone(),
            vec![],
            None,
        ));
        if let Some(ref k) = history_session_key {
            let _ = self
                .session_state
                .append_assistant_message_and_save(k, &created_msg)
                .await;
        }
        let token = CancellationToken::new();
        let handle = self.spawn_workflow_run(msg, session_key, history_session_key, state, Some(token.clone()));
        Ok(Some((handle, token, session_key.to_string())))
    }

    /// Resolve the default agent id for this message (from channel's default_agent in metadata, or "main").
    fn default_agent_for_msg(&self, msg: &InboundMessage) -> String {
        let id = msg
            .metadata
            .get("default_agent")
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .trim();
        if id.is_empty() || !self.agent_registry.contains(id) {
            "main".to_string()
        } else {
            id.to_string()
        }
    }

    /// Resolve session key used for conversation history (agent:channel:scope:id).
    async fn resolve_history_session_key(&self, msg: &InboundMessage) -> Option<String> {
        let agent_id = self.default_agent_for_msg(msg);
        let session_id = {
            let sm = self.session_state.session_manager.write().await;
            sm.resolve_session(&agent_id, &msg.channel, &msg.chat_id, &msg.metadata)
        };
        Some(session_id.format())
    }

    /// Handle /status: show current session info and workflow state if any.
    async fn handle_status(&self, msg: &InboundMessage) -> Result<()> {
        let history_key = match self.resolve_history_session_key(msg).await {
            Some(k) => k,
            None => {
                let _ = self.outbound_tx.send(OutboundMessage::chat(
                    msg.channel.clone(),
                    msg.chat_id.clone(),
                    "[Status] Could not resolve session.".to_string(),
                    vec![],
                    None,
                ));
                return Ok(());
            }
        };
        let session_key = msg.session_key();
        let mut lines: Vec<String> = Vec::new();
        if let Ok(sid) = crate::agent::session_id::SessionId::parse(&history_key) {
            let (meta_opt, msg_count) = {
                let sm = self.session_state.session_manager.read().await;
                let count = sm.get_history(&sid).map(|h| h.len()).unwrap_or(0);
                let meta = sm.get_meta(&sid).cloned();
                (meta, count)
            };
            lines.push(format!("Session: {}", history_key));
            if let Some(meta) = meta_opt {
                lines.push(format!("  channel={}  scope={:?}  identifier={:?}  messages={}", meta.id.channel, meta.id.scope, meta.id.identifier, msg_count));
                lines.push(format!("  updated_at={}", meta.updated_at));
            } else {
                lines.push(format!("  messages={}", msg_count));
            }
            let active = self.session_state.get_active_snapshot().await;
            if let Some(activity) = active.get(&history_key) {
                lines.push(format!("  running=true  activity={}", activity));
            } else {
                lines.push("  running=false".to_string());
            }
        }
        if let Ok(Some(state)) = self.workflow_store.load_state(&session_key).await {
            lines.push("Workflow:".to_string());
            lines.push(format!("  workflow_id={}  status={:?}  current_step_index={}", state.workflow_id, state.status, state.current_step_index));
            lines.push(format!("  steps_total={}  step_outputs={}", state.definition.steps.len(), state.step_outputs.len()));
        } else {
            lines.push("Workflow: none for this session.".to_string());
        }
        let text = lines.join("\n");
        let _ = self.outbound_tx.send(OutboundMessage::chat(
            msg.channel.clone(),
            msg.chat_id.clone(),
            format!("[Status]\n{}", text),
            vec![],
            None,
        ));
        Ok(())
    }

    /// Handle /clear: clear session (same as reset_session tool) and workflow state for this chat.
    async fn handle_clear(&self, msg: &InboundMessage) -> Result<()> {
        let history_key = match self.resolve_history_session_key(msg).await {
            Some(k) => k,
            None => {
                let _ = self.outbound_tx.send(OutboundMessage::chat(
                    msg.channel.clone(),
                    msg.chat_id.clone(),
                    "[Clear] Could not resolve session.".to_string(),
                    vec![],
                    None,
                ));
                return Ok(());
            }
        };
        let session_key = msg.session_key();
        if let Err(e) = self.session_state.clear_session(&history_key).await {
            let _ = self.outbound_tx.send(OutboundMessage::chat(
                msg.channel.clone(),
                msg.chat_id.clone(),
                format!("[Clear] Failed: {}.", e),
                vec![],
                None,
            ));
            return Ok(());
        }
        let _ = self.workflow_store.delete_state(&session_key).await;
        let _ = self.outbound_tx.send(OutboundMessage::chat(
            msg.channel.clone(),
            msg.chat_id.clone(),
            "[Clear] Session cleared. Conversation will continue as a fresh chat.".to_string(),
            vec![],
            None,
        ));
        Ok(())
    }

    /// Spawns workflow run and returns the JoinHandle so the caller can wait or cancel via token.
    fn spawn_workflow_run(
        &self,
        msg: &InboundMessage,
        session_key: &str,
        history_session_key: Option<String>,
        state: WorkflowState,
        cancel: Option<CancellationToken>,
    ) -> tokio::task::JoinHandle<()> {
        let store = self.workflow_store.clone();
        let model = Arc::clone(&self.model);
        let outbound_tx = self.outbound_tx.clone();
        let pending_input = self.pending_workflow_input.clone();
        let session_state = self.session_state.clone();
        let channel = msg.channel.clone();
        let chat_id = msg.chat_id.clone();
        let session_key = session_key.to_string();
        let cancel_for_run = cancel.as_ref().map(|t| t.clone());
        tokio::spawn(async move {
            if let Err(e) = run_workflow(
                &store,
                model.as_ref(),
                &outbound_tx,
                &pending_input,
                &session_state,
                history_session_key.as_deref(),
                &channel,
                &chat_id,
                &session_key,
                state,
                cancel_for_run,
            )
            .await
            {
                tracing::error!(session_key = %session_key, "Workflow run error: {e:#}");
            }
        })
    }

    /// Append the message to the channel's default agent session and persist to disk, without running completion.
    /// Used when the chat is not in allowlist or when it's a group message not directed at the bot.
    async fn save_message_only(&mut self, msg: &InboundMessage) -> Result<()> {
        let agent_id = self.default_agent_for_msg(msg);
        let session_id = {
            let sm = self.session_state.session_manager.write().await;
            sm.resolve_session(&agent_id, &msg.channel, &msg.chat_id, &msg.metadata)
        };
        let session_key = session_id.format();

        let user_content = apply_sender_display_prefix(&msg.content, &msg.metadata);
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
        cancel: Option<CancellationToken>,
    ) -> Result<()> {
        for directive in directives {
            let agent_id = match &directive.target {
                None => self.default_agent_for_msg(msg),
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
                self.tool_sandbox_exec_kind,
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
            let base_content = apply_sender_display_prefix(&base_content, &msg.metadata);
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
            let max_chat_history_messages = agent_ctx.params.max_chat_history_messages;
            let run_result = scope(tool_ctx, async {
                run_completion_loop(
                    &*self.model,
                    &system_prompt,
                    model_max_iterations,
                    agent_ctx.params.max_consecutive_tool_errors,
                    max_chat_history_messages,
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
                    cancel.as_ref(),
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
                None => self.default_agent_for_msg(msg),
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
                self.tool_sandbox_exec_kind,
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
            let base_content = apply_sender_display_prefix(&base_content, &msg.metadata);
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
            let max_chat_history_messages = agent_ctx.params.max_chat_history_messages;
            let max_consecutive_tool_errors = agent_ctx.params.max_consecutive_tool_errors;

            let session_messages_clone = session_messages.clone();
            let label = format!(
                "directive:{}:{}",
                agent_id,
                directive.content.chars().take(30).collect::<String>()
            );
            let label_log = label.clone();
            let task_future = Box::pin(async move {
                tracing::info!(agent_id = %aid, label = %label_log, "Directive task started");
                session_state.set_active(&sk, "processing").await;
                let mut history_guard = session_messages_clone.lock().await;
                let session_id_str = sk.clone();
                let run_result = scope(tool_ctx, async move {
                    let it = run_completion_loop(
                        &*model,
                        &system_prompt,
                        model_max_iterations,
                        max_consecutive_tool_errors,
                        max_chat_history_messages,
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
                        None, // subagent tasks use timeout; no /stop cancel
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

            let subagent_id = {
                let mut mgr = self.subagent_manager.lock().await;
                mgr.spawn_fn(label, task_future, None).await
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
                            if matches!(handle.status, SubagentStatus::Running) {
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
            // Notify user if any directive task timed out, and send all subagent results to the user
            let mgr = self.subagent_manager.lock().await;
            for (id, agent_id, _) in &spawned_ids {
                if let Some(handle) = mgr.get_result(id).await {
                    match &handle.status {
                        SubagentStatus::Failed(ref err_msg) => {
                            if err_msg.contains("timed out") || err_msg.contains("Timeout") {
                                let _ = self.outbound_tx.send(OutboundMessage::chat(
                                    msg.channel.clone(),
                                    msg.chat_id.clone(),
                                    format!(
                                        "Subagent '{}' timed out and was stopped. You can retry with a simpler request or increase mainAgent.subagent_task_timeout_secs in config (default 600s).",
                                        agent_id
                                    ),
                                    vec![],
                                    None,
                                ));
                            } else {
                                let _ = self.outbound_tx.send(OutboundMessage::chat(
                                    msg.channel.clone(),
                                    msg.chat_id.clone(),
                                    format!("[Subagent {}] Failed: {}", handle.label, err_msg),
                                    vec![],
                                    None,
                                ));
                            }
                        }
                        SubagentStatus::Completed => {
                            let body = handle
                                .result
                                .as_deref()
                                .unwrap_or("(no output)");
                            let _ = self.outbound_tx.send(OutboundMessage::chat(
                                msg.channel.clone(),
                                msg.chat_id.clone(),
                                format!("[Subagent {}] Result:\n\n{}", handle.label, body),
                                vec![],
                                None,
                            ));
                        }
                        SubagentStatus::Running => {
                            // Should not happen after wait loop; log if it does
                            warn!(
                                subagent_id = %id,
                                agent_id = %agent_id,
                                "Subagent still Running after wait loop"
                            );
                        }
                    }
                }
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
// Helpers for completion debug logging
// ---------------------------------------------------------------------------

fn truncate_debug(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut end = max_len.min(s.len());
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}... ({} chars total)", &s[..end], s.len())
    }
}

// ---------------------------------------------------------------------------
// Standalone completion loop
// ---------------------------------------------------------------------------

async fn run_completion_loop(
    model: &dyn SynbotCompletionModel,
    system_prompt: &str,
    max_iterations: u32,
    max_consecutive_tool_errors: u32,
    max_chat_history_messages: u32,
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
    cancel: Option<&CancellationToken>,
) -> Result<u32> {
    let message_ctx = Some((channel, chat_id, sender_id, session_id));
    let mut iterations = 0u32;
    let mut consecutive_tool_errors: u32 = 0;
    // Media paths from "message" tool calls in this run; sent with the final reply so channels (e.g. DingTalk) get one message with text + files.
    let mut pending_media = Vec::<String>::new();

    loop {
        if let Some(token) = cancel {
            if token.is_cancelled() {
                info!(agent_id = %agent_id, "Agent run cancelled by user (/stop or /cancel)");
                return Err(anyhow::anyhow!("Cancelled by user (/stop or /cancel)"));
            }
        }
        iterations += 1;
        if iterations > max_iterations {
            warn!("Max iterations ({}) reached for agent '{}'", max_iterations, agent_id);
            let msg = format!(
                "[Agent '{}'] 已达到最大 tool 执行次数（{}），已停止。请简化请求或增加配置 mainAgent.maxToolIterations（当前默认 99）。",
                agent_id, max_iterations
            );
            let _ = outbound_tx.send(OutboundMessage::chat(
                channel.to_string(),
                chat_id.to_string(),
                msg,
                vec![],
                None,
            ));
            break;
        }

        tracing::debug!("History check again: {:?}", history);

        // Send at most the last N messages to the model; full history remains in session for persistence.
        let history_for_model: Vec<Message> = if history.len() <= max_chat_history_messages as usize {
            history.clone()
        } else {
            history[history.len() - max_chat_history_messages as usize..].to_vec()
        };
        let chat_history = if history_for_model.is_empty() {
            OneOrMany::one(Message::user(""))
        } else {
            OneOrMany::many(history_for_model).expect("non-empty history")
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

        for (i, c) in response.choice.iter().enumerate() {
            match c {
                AssistantContent::Text(t) => {
                    let preview = truncate_debug(&t.text, 800);
                    tracing::debug!("Completion response [{}] Text ({} chars): {}", i, t.text.len(), preview);
                }
                AssistantContent::ToolCall(tc) => {
                    let name = &tc.function.name;
                    let args_str = tc.function.arguments.to_string();
                    let args_preview = truncate_debug(&args_str, 500);
                    tracing::debug!(
                        "Completion response [{}] ToolCall name={} args={}",
                        i,
                        name,
                        args_preview
                    );
                }
                AssistantContent::Reasoning(r) => {
                    let reasoning_text = r.reasoning.join("\n");
                    let preview = truncate_debug(&reasoning_text, 800);
                    tracing::debug!(
                        "Completion response [{}] Reasoning ({} chars): {}",
                        i,
                        reasoning_text.len(),
                        preview
                    );
                }
                AssistantContent::Image(_) => {
                    tracing::debug!("Completion response [{}] Image", i);
                }
            }
        }
        let text_preview: String = response
            .choice
            .iter()
            .filter_map(|c| match c {
                AssistantContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        if !text_preview.is_empty() {
            let max_preview = 1000;
            let trunc = if text_preview.len() > max_preview {
                let mut end = max_preview;
                while end > 0 && !text_preview.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}... [truncated, total {} chars]", &text_preview[..end], text_preview.len())
            } else {
                text_preview.clone()
            };
            tracing::debug!("Completion output (text): {}", trunc);
        }

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
                    if tc.function.name == "message" {
                        if let Some(arr) = tc.function.arguments.get("files").and_then(|a| a.as_array()) {
                            for v in arr {
                                if let Some(s) = v.as_str().filter(|x| !x.trim().is_empty()) {
                                    pending_media.push(s.to_string());
                                }
                            }
                        }
                    }
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
                    // list_tools result is meant to be shown in full to the user; do not truncate
                    let preview = if tc.function.name == "list_tools" {
                        result_str.clone()
                    } else if result_str.len() > tool_result_preview_chars {
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
            let any_tool_failed = tool_results.iter().any(|(_, s)| s.starts_with("Error:"));
            if any_tool_failed {
                consecutive_tool_errors += 1;
            } else {
                consecutive_tool_errors = 0;
            }
            if consecutive_tool_errors >= max_consecutive_tool_errors {
                warn!(
                    "Max consecutive tool errors ({}) reached for agent '{}'",
                    max_consecutive_tool_errors, agent_id
                );
                let msg = format!(
                    "[Agent '{}'] 连续 {} 次 tool 执行失败，已停止。请检查工具配置或简化请求。可调整配置 mainAgent.maxConsecutiveToolErrors（当前默认 8）。",
                    agent_id, max_consecutive_tool_errors
                );
                let _ = outbound_tx.send(OutboundMessage::chat(
                    channel.to_string(),
                    chat_id.to_string(),
                    msg,
                    vec![],
                    None,
                ));
                break;
            }
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
            if !reply.is_empty() || !pending_media.is_empty() {
                if !reply.is_empty() {
                    history.push(Message::assistant(&reply));
                }
                let out_msg = OutboundMessage::chat(
                    channel.to_string(),
                    chat_id.to_string(),
                    reply.clone(),
                    pending_media.clone(),
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

