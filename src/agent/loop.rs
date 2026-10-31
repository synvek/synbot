//! Agent loop -- the core processing engine.

use crate::rig_provider::{StreamDelta, StreamSession, SynbotCompletionModel};
use anyhow::Result;
use rig::completion::request::CompletionError;
use rig::completion::CompletionRequest;
use rig::message::{AssistantContent, Message, ToolCall, ToolResultContent, UserContent};
use rig::OneOrMany;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, Mutex, RwLock};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn, Instrument};

use crate::agent::agent_registry::AgentRegistry;
use crate::agent::context::ContextBuilder;
use crate::agent::control_commands::{
    busy_hint_commands, parse_control_command, slash_commands_help_text, ControlCommand,
};
use crate::agent::directive::DirectiveParser;
use crate::agent::events::{
    AgentEvent, EventBus, FileChangeKind, ToolCallStatus, TurnFinishReason,
};
use crate::agent::session_state::SharedSessionState;
use crate::agent::skills::{CompositeSkillProvider, SkillProvider};
use crate::agent::subagent::{SubagentManager, SubagentStatus};
use crate::bus::{InboundMessage, OutboundMessage};
use crate::config::{self, Config};
use crate::hooks::{HookEvent, HookRegistry};
use crate::sandbox::types::ToolSandboxExecKind;
use crate::tools::{scope, ToolContext, ToolRegistry};
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

/// Expand a workspace-defined `/name` template (`.synbot/commands/*.md`).
///
/// Built-in control commands (`/stop`, `/status`, …) always win, and only the main agent expands —
/// a role addressed as `@@dev /review` keeps its literal text.
fn expand_custom_command(workspace: &std::path::Path, agent_id: &str, text: String) -> String {
    if agent_id != "main" || parse_control_command(&text).is_some() {
        return text;
    }
    crate::commands::expand(workspace, &text).unwrap_or(text)
}

pub struct AgentLoop {
    model: Arc<dyn SynbotCompletionModel>,
    workspace: PathBuf,
    /// Folders opened by the user; a chat bound to a project works inside that folder.
    projects: Arc<RwLock<crate::project::ProjectStore>>,
    /// Pre-write snapshots of every file a turn mutates, so the UI can diff and undo a turn.
    checkpoints: Arc<crate::checkpoint::CheckpointStore>,
    /// Skills from the skills directory plus any plugin-registered providers (same merge as the web API and startup).
    skills: Arc<CompositeSkillProvider>,
    tools: Arc<ToolRegistry>,
    max_iterations: u32,
    outbound_tx: broadcast::Sender<OutboundMessage>,
    /// UI-facing event stream (streaming deltas, tool lifecycle). Channels keep using `outbound_tx`.
    events: Arc<EventBus>,
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
    /// Live config (memory, compression, embeddings).
    shared_config: Arc<RwLock<Config>>,
    /// Approval manager shared with the `exec` tool; `/pending`, `/approve`, `/reject` answer through it.
    approval_manager: Option<Arc<crate::tools::approval::ApprovalManager>>,
}

impl AgentLoop {
    pub async fn new(
        model: Arc<dyn SynbotCompletionModel>,
        workspace: PathBuf,
        skills: Arc<CompositeSkillProvider>,
        tools: Arc<ToolRegistry>,
        max_iterations: u32,
        outbound_tx: broadcast::Sender<OutboundMessage>,
        events: Arc<EventBus>,
        config: &Config,
        session_state: SharedSessionState,
        agent_registry: Arc<AgentRegistry>,
        tool_sandbox_exec_kind: Option<ToolSandboxExecKind>,
        hooks: Option<Arc<HookRegistry>>,
        shared_config: Arc<RwLock<Config>>,
    ) -> Self {
        let mut subagent_manager = SubagentManager::new(
            config.main_agent.max_concurrent_subagents,
            Some(config.main_agent.subagent_task_timeout_secs),
        );
        // Directive subagents show up in the workbench team panel like `spawn`ed ones.
        subagent_manager.set_events(Arc::clone(&events));
        let tool_result_preview_chars = config.tool_result_preview_chars as usize;
        let workflow_store = WorkflowStore::new(config::workflows_root(config).as_path());
        let workflow_user_input_timeout_secs = config.workflow.user_input_timeout_secs;
        Self {
            workspace,
            projects: Arc::new(RwLock::new(
                crate::project::ProjectStore::with_default_path(),
            )),
            checkpoints: Arc::new(crate::checkpoint::CheckpointStore::with_default_root()),
            model,
            skills,
            tools,
            max_iterations,
            outbound_tx,
            events,
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
            shared_config,
            approval_manager: None,
        }
    }

    /// Project registry shared with the web layer (same Arc, so `/api/projects` and the agent see
    /// the same selection).
    pub fn projects(&self) -> Arc<RwLock<crate::project::ProjectStore>> {
        Arc::clone(&self.projects)
    }

    /// Turn snapshots shared with `/api/checkpoints` (diff + undo).
    pub fn checkpoints(&self) -> Arc<crate::checkpoint::CheckpointStore> {
        Arc::clone(&self.checkpoints)
    }

    /// Subagent pool shared with the team tools and `/api/subagents`.
    pub fn subagent_manager(&self) -> Arc<Mutex<SubagentManager>> {
        Arc::clone(&self.subagent_manager)
    }

    /// Share one subagent pool between directive routing and the tools, so the team panel and
    /// `list_subagents` see every teammate instead of two independent pools.
    pub fn set_subagent_manager(&mut self, manager: Arc<Mutex<SubagentManager>>) {
        self.subagent_manager = manager;
    }

    /// Wire the approval manager so `/pending`, `/approve` and `/reject` can list and answer
    /// requests that the `exec` tool is waiting on (including ones forwarded from another chat).
    pub fn set_approval_manager(&mut self, manager: Arc<crate::tools::approval::ApprovalManager>) {
        self.approval_manager = Some(manager);
    }

    /// Filesystem root for one turn: the project bound to `chat_id`, else the agent workspace.
    ///
    /// The folder is re-checked on every turn, so a project that was moved or deleted falls back
    /// to the agent workspace instead of failing the chat.
    async fn turn_workspace(&self, agent_workspace: &std::path::Path, chat_id: &str) -> PathBuf {
        let bound = self.projects.read().await.project_for_chat(chat_id);
        match bound {
            Some(project) if project.path.is_dir() => project.path,
            Some(project) => {
                warn!(
                    project = %project.path.display(),
                    "Project folder is missing; falling back to the agent workspace"
                );
                agent_workspace.to_path_buf()
            }
            None => agent_workspace.to_path_buf(),
        }
    }

    fn skills_list_body(skills: &CompositeSkillProvider) -> String {
        let summary = skills.build_skills_summary();
        if summary.is_empty() {
            "No skills are currently loaded. Skills are subdirectories containing SKILL.md under the config skills directory."
                .to_string()
        } else {
            summary
        }
    }

    /// Run the agent loop. Requires the loop to be in an `Arc<Mutex<>>` so that /stop (or /cancel)
    /// can cancel a running agent task by spawning and tracking it. Pass the receiver so the
    /// loop can recv without holding the lock (allowing the spawned task to run).
    pub async fn run(
        loop_ref: Arc<Mutex<Self>>,
        inbound_rx: mpsc::Receiver<InboundMessage>,
    ) -> Result<()> {
        Self::run_with_runtime_status(loop_ref, inbound_rx, None).await
    }

    /// Run the agent loop while recording inbound activity for channel health snapshots.
    pub async fn run_with_runtime_status(
        loop_ref: Arc<Mutex<Self>>,
        mut inbound_rx: mpsc::Receiver<InboundMessage>,
        _runtime_status: Option<Arc<crate::channels::ChannelStatusRegistry>>,
    ) -> Result<()> {
        info!("Agent loop started");
        let mut join_set: JoinSet<String> = JoinSet::new();
        let mut cancel_by_session: HashMap<String, CancellationToken> = HashMap::new();
        // `/clear` that arrived while a turn held the loop lock. Flushed at the top of every
        // iteration, i.e. before the next inbound message, so a fresh turn can never start inside
        // a session that is about to be wiped.
        let mut deferred_clears: VecDeque<InboundMessage> = VecDeque::new();
        // Clones the dispatcher needs to answer control commands. A running turn holds the loop
        // lock until it ends, so `/stop` (and busy notices) must work without it — otherwise the
        // only escape from a long turn would queue up behind that very turn.
        let (outbound_tx, events, tools, skills, approval_manager) = {
            let guard = loop_ref.lock().await;
            (
                guard.outbound_tx.clone(),
                Arc::clone(&guard.events),
                Arc::clone(&guard.tools),
                Arc::clone(&guard.skills),
                guard.approval_manager.clone(),
            )
        };

        loop {
            Self::flush_deferred_clears(&loop_ref, &mut deferred_clears).await;

            tokio::select! {
                Some(join_result) = join_set.join_next(), if !join_set.is_empty() => {
                    match join_result {
                        Ok(session_key) => {
                            cancel_by_session.remove(&session_key);
                        }
                        Err(e) => {
                            warn!(error = %e, "Agent background task ended with error");
                        }
                    }
                }
                // A queued /clear only needs the lock to be free, and a turn that unwinds through
                // a non-tracked path (or another handler) publishes no event to wake us up, so
                // poll — but only while something is actually pending.
                _ = tokio::time::sleep(std::time::Duration::from_millis(150)),
                    if !deferred_clears.is_empty() => {}
                msg_opt = inbound_rx.recv() => {
                    let msg = match msg_opt {
                        None => break,
                        Some(m) => m,
                    };
                    let sk = msg.session_key();

                    if let Some(cmd) = parse_control_command(&msg.content) {
                        match cmd {
                            ControlCommand::Stop => {
                                // No loop lock: the turn we are stopping is holding it.
                                if let Some(token) = cancel_by_session.remove(&sk) {
                                    token.cancel();
                                    Self::send_control_reply(
                                        &outbound_tx,
                                        &events,
                                        &msg,
                                        "[Control] Stopped.".to_string(),
                                    );
                                } else {
                                    Self::send_control_reply(
                                        &outbound_tx,
                                        &events,
                                        &msg,
                                        "[Control] Nothing is currently running. Use /status to check."
                                            .to_string(),
                                    );
                                }
                                continue;
                            }
                            ControlCommand::Status => {
                                match loop_ref.try_lock() {
                                    Ok(guard) => {
                                        guard.handle_status(&msg).await.ok();
                                    }
                                    Err(_) => Self::busy_notice(&outbound_tx, &events, &msg),
                                }
                                continue;
                            }
                            ControlCommand::Clear => {
                                match loop_ref.try_lock() {
                                    Ok(guard) => {
                                        guard.handle_clear(&msg).await.ok();
                                    }
                                    Err(_) => {
                                        // A turn holds the lock. Abort it and queue the clear instead
                                        // of dropping it: letting that turn finish would only write
                                        // an answer into the session we are about to reset. The token
                                        // stays registered so messages arriving during shutdown keep
                                        // getting the busy notice rather than starting a doomed turn.
                                        let stopping = cancel_by_session
                                            .get(&sk)
                                            .map(|token| token.cancel())
                                            .is_some();
                                        let already_queued = deferred_clears
                                            .iter()
                                            .any(|m| m.session_key() == sk);
                                        let reply = if already_queued {
                                            "[Clear] Already queued; the session is cleared as soon as the running turn stops."
                                        } else if stopping {
                                            "[Clear] Stopping the running turn; the session is cleared as soon as it stops."
                                        } else {
                                            "[Clear] Agent is busy with another chat; the session is cleared as soon as it is free."
                                        };
                                        Self::send_control_reply(
                                            &outbound_tx,
                                            &events,
                                            &msg,
                                            reply.to_string(),
                                        );
                                        if !already_queued {
                                            deferred_clears.push_back(msg);
                                        }
                                    }
                                }
                                continue;
                            }
                            ControlCommand::Commands => {
                                Self::send_control_reply(
                                    &outbound_tx,
                                    &events,
                                    &msg,
                                    format!("[Commands]\n{}", slash_commands_help_text()),
                                );
                                continue;
                            }
                            ControlCommand::Skills => {
                                let body = Self::skills_list_body(&skills);
                                Self::send_control_reply(
                                    &outbound_tx,
                                    &events,
                                    &msg,
                                    format!("[Skills]\n{}", body),
                                );
                                continue;
                            }
                            ControlCommand::Tools => {
                                let body = crate::tools::list_tools::format_tool_list_markdown(
                                    tools.list_tools(),
                                );
                                Self::send_control_reply(
                                    &outbound_tx,
                                    &events,
                                    &msg,
                                    format!("[Tools]\n{}", body),
                                );
                                continue;
                            }
                            ControlCommand::Pending => {
                                Self::reply_pending_approvals(
                                    &approval_manager,
                                    &outbound_tx,
                                    &events,
                                    &msg,
                                )
                                .await
                                .ok();
                                continue;
                            }
                            ControlCommand::Approve { target } => {
                                Self::reply_approval_answer(
                                    &approval_manager,
                                    &outbound_tx,
                                    &events,
                                    &msg,
                                    target,
                                    true,
                                )
                                .await
                                .ok();
                                continue;
                            }
                            ControlCommand::Reject { target } => {
                                Self::reply_approval_answer(
                                    &approval_manager,
                                    &outbound_tx,
                                    &events,
                                    &msg,
                                    target,
                                    false,
                                )
                                .await
                                .ok();
                                continue;
                            }
                            ControlCommand::Tasks => {
                                match loop_ref.try_lock() {
                                    Ok(guard) => {
                                        guard.handle_tasks(&msg).await.ok();
                                    }
                                    Err(_) => Self::busy_notice(&outbound_tx, &events, &msg),
                                }
                                continue;
                            }
                            ControlCommand::Resume => {
                                let loop_guard = match loop_ref.try_lock() {
                                    Ok(guard) => guard,
                                    Err(_) => {
                                        Self::busy_notice(&outbound_tx, &events, &msg);
                                        continue;
                                    }
                                };
                                let wf_result = {
                                    if let Some(ref k) =
                                        loop_guard.resolve_history_session_key(&msg).await
                                    {
                                        let _ = loop_guard
                                            .session_state
                                            .append_user_message_and_save(k, &msg.content)
                                            .await;
                                    }
                                    loop_guard.handle_workflow_continue(&msg, &sk).await
                                };
                                drop(loop_guard);
                                match wf_result {
                                    Ok(Some((handle, token, run_sk))) => {
                                        cancel_by_session.insert(run_sk.clone(), token);
                                        join_set.spawn(async move {
                                            let _ = handle.await;
                                            run_sk
                                        });
                                    }
                                    Ok(None) => {}
                                    Err(e) => Self::send_control_reply(
                                        &outbound_tx,
                                        &events,
                                        &msg,
                                        format!("[Workflow] Resume failed: {}", e),
                                    ),
                                }
                                continue;
                            }
                        }
                    }

                    if deferred_clears.iter().any(|m| m.session_key() == sk) {
                        Self::send_control_reply(
                            &outbound_tx,
                            &events,
                            &msg,
                            "[Clear] This session is being cleared; please resend in a moment."
                                .to_string(),
                        );
                        continue;
                    }

                    if cancel_by_session.contains_key(&sk) {
                        Self::busy_notice(&outbound_tx, &events, &msg);
                        continue;
                    }

                    let (channel, chat_id) = (msg.channel.clone(), msg.chat_id.clone());
                    let result = {
                        let mut guard = loop_ref.lock().await;
                        guard.handle_message(msg, loop_ref.clone()).await
                    };
                    match result {
                        Err(e) => error!("Error handling message: {e:#}"),
                        Ok(Some((handle, token, run_sk))) => {
                            cancel_by_session.insert(run_sk.clone(), token);
                            join_set.spawn(async move {
                                let _ = handle.await;
                                run_sk
                            });
                        }
                        Ok(None) => {
                            // The message was consumed without starting a turn (workflow pending
                            // input, save-only message, confirmation prompt). Nothing will publish
                            // `turn_finished`, so tell the UI to leave its running state.
                            events.publish(&channel, &chat_id, AgentEvent::TurnSkipped);
                        }
                    }
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
            if self
                .pending_workflow_input
                .deliver(&session_key, msg.content.clone())
                .await
            {
                return Ok(None);
            }

            // If user previously got "confirm?" and now confirms (intent-based, same as approval), start workflow.
            if let Some(def) = self
                .pending_workflow_confirm
                .take_if_confirm(&session_key, &msg.content, self.model.as_ref())
                .await
            {
                if let Some(ref k) = self.resolve_history_session_key(&msg).await {
                    let _ = self
                        .session_state
                        .append_user_message_and_save(k, &msg.content)
                        .await;
                }
                let task = self
                    .start_workflow_with_def(&msg, &session_key, def)
                    .await?;
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
                    ControlCommand::Commands => {
                        self.reply_control(
                            &msg,
                            format!("[Commands]\n{}", slash_commands_help_text()),
                        )?;
                        return Ok(None);
                    }
                    ControlCommand::Skills => {
                        let body = Self::skills_list_body(&self.skills);
                        self.reply_control(&msg, format!("[Skills]\n{}", body))?;
                        return Ok(None);
                    }
                    ControlCommand::Tools => {
                        let body = crate::tools::list_tools::format_tool_list_markdown(
                            self.tools.list_tools(),
                        );
                        self.reply_control(&msg, format!("[Tools]\n{}", body))?;
                        return Ok(None);
                    }
                    ControlCommand::Resume => {
                        if let Some(ref k) = self.resolve_history_session_key(&msg).await {
                            let _ = self
                                .session_state
                                .append_user_message_and_save(k, &msg.content)
                                .await;
                        }
                        match self.handle_workflow_continue(&msg, &session_key).await {
                            Ok(Some(task)) => return Ok(Some(task)),
                            Ok(None) => {}
                            Err(e) => {
                                self.reply_control(
                                    &msg,
                                    format!("[Workflow] Resume failed: {}", e),
                                )?;
                            }
                        }
                        return Ok(None);
                    }
                    ControlCommand::Stop => {
                        self.reply_control(
                            &msg,
                            "[Control] Nothing is currently running. Use /status to check."
                                .to_string(),
                        )?;
                        return Ok(None);
                    }
                    ControlCommand::Pending => {
                        self.handle_pending_approvals(&msg).await?;
                        return Ok(None);
                    }
                    ControlCommand::Approve { target } => {
                        self.handle_approval_answer(&msg, target, true).await?;
                        return Ok(None);
                    }
                    ControlCommand::Reject { target } => {
                        self.handle_approval_answer(&msg, target, false).await?;
                        return Ok(None);
                    }
                    ControlCommand::Tasks => {
                        self.handle_tasks(&msg).await?;
                        return Ok(None);
                    }
                }
            }

            match parse_workflow_trigger(&msg.content) {
                WorkflowTrigger::Continue => {
                    if let Some(ref k) = self.resolve_history_session_key(&msg).await {
                        let _ = self
                            .session_state
                            .append_user_message_and_save(k, &msg.content)
                            .await;
                    }
                    match self.handle_workflow_continue(&msg, &session_key).await {
                        Ok(Some(task)) => return Ok(Some(task)),
                        Ok(None) => {}
                        Err(e) => {
                            self.reply_control(&msg, format!("[Workflow] Continue failed: {}", e))?;
                        }
                    }
                    return Ok(None);
                }
                WorkflowTrigger::Create {
                    description,
                    user_provided_def,
                } => {
                    if let Some(ref k) = self.resolve_history_session_key(&msg).await {
                        let _ = self
                            .session_state
                            .append_user_message_and_save(k, &msg.content)
                            .await;
                    }
                    match self
                        .handle_workflow_create(&msg, &session_key, &description, user_provided_def)
                        .await
                    {
                        Ok(Some(task)) => return Ok(Some(task)),
                        Ok(None) => {}
                        Err(e) => {
                            self.reply_control(&msg, format!("[Workflow] Create failed: {}", e))?;
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
                        .process_directives_sequential(
                            &msg_clone,
                            &directives_clone,
                            start,
                            Some(token_for_spawn),
                        )
                        .await
                    {
                        tracing::debug!("Agent run ended (cancelled or error): {}", e);
                    }
                });
                return Ok(Some((handle, token, session_key)));
            } else {
                self.process_directives_parallel(&msg, &directives, start)
                    .await?;
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
                // Control-style reply: must reach the event bus too, or the workbench sees nothing.
                self.reply_control(
                    msg,
                    "[Workflow] No workflow in progress for this session.".to_string(),
                )?;
                return Ok(None);
            }
        };
        if state.is_finished() {
            self.reply_control(msg, "[Workflow] Workflow already ended.".to_string())?;
            return Ok(None);
        }
        let history_session_key = self.resolve_history_session_key(msg).await;
        let token = CancellationToken::new();
        let handle = self.spawn_workflow_run(
            msg,
            session_key,
            history_session_key,
            state,
            Some(token.clone()),
        );
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
                self.reply_control(
                    msg,
                    format!(
                        "[Workflow] Definition validation failed: {}. Please fix and retry.",
                        e
                    ),
                )?;
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
            self.reply_control(
                msg,
                format!(
                    "[Workflow] Parsed steps:\n{}\nReply to approve and start (any language).",
                    steps_preview
                ),
            )?;
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
        self.reply_control(msg, created_msg.clone())?;
        if let Some(ref k) = history_session_key {
            let _ = self
                .session_state
                .append_assistant_message_and_save(k, &created_msg)
                .await;
        }
        let token = CancellationToken::new();
        let handle = self.spawn_workflow_run(
            msg,
            session_key,
            history_session_key,
            state,
            Some(token.clone()),
        );
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
                return self.reply_control(msg, "[Status] Could not resolve session.".to_string());
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
                lines.push(format!(
                    "  channel={}  scope={:?}  identifier={:?}  messages={}",
                    meta.id.channel, meta.id.scope, meta.id.identifier, msg_count
                ));
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
            lines.push(format!(
                "  workflow_id={}  status={:?}  current_step_index={}",
                state.workflow_id, state.status, state.current_step_index
            ));
            lines.push(format!(
                "  steps_total={}  step_outputs={}",
                state.definition.steps.len(),
                state.step_outputs.len()
            ));
        } else {
            lines.push("Workflow: none for this session.".to_string());
        }
        self.reply_control(msg, format!("[Status]\n{}", lines.join("\n")))
    }

    /// Handle /pending: list the approvals this chat is allowed to answer.
    async fn handle_pending_approvals(&self, msg: &InboundMessage) -> Result<()> {
        Self::reply_pending_approvals(&self.approval_manager, &self.outbound_tx, &self.events, msg)
            .await
    }

    /// [`Self::handle_pending_approvals`] for callers that only hold clones: answering approvals
    /// must not need the loop lock, because the turn waiting on the approval is holding it.
    async fn reply_pending_approvals(
        approval_manager: &Option<Arc<crate::tools::approval::ApprovalManager>>,
        outbound_tx: &broadcast::Sender<OutboundMessage>,
        events: &EventBus,
        msg: &InboundMessage,
    ) -> Result<()> {
        let Some(manager) = approval_manager else {
            Self::send_control_reply(
                outbound_tx,
                events,
                msg,
                "[Pending] Approval manager is not wired in this instance.".to_string(),
            );
            return Ok(());
        };
        let pending = manager
            .visible_pending(&msg.session_key(), &msg.channel, &msg.chat_id)
            .await;
        Self::send_control_reply(
            outbound_tx,
            events,
            msg,
            format!("[Pending]\n{}", format_pending_approvals(&pending)),
        );
        Ok(())
    }

    /// Handle /approve and /reject: answer a waiting request from plain text, so a phone without
    /// buttons (or a forwarded request from the desktop) can still unblock the turn.
    async fn handle_approval_answer(
        &self,
        msg: &InboundMessage,
        target: Option<String>,
        approved: bool,
    ) -> Result<()> {
        Self::reply_approval_answer(
            &self.approval_manager,
            &self.outbound_tx,
            &self.events,
            msg,
            target,
            approved,
        )
        .await
    }

    /// [`Self::handle_approval_answer`] without the loop lock: the turn that is waiting for this
    /// answer is the one holding that lock, so `/approve` typed as text would otherwise deadlock
    /// until the request timed out.
    async fn reply_approval_answer(
        approval_manager: &Option<Arc<crate::tools::approval::ApprovalManager>>,
        outbound_tx: &broadcast::Sender<OutboundMessage>,
        events: &EventBus,
        msg: &InboundMessage,
        target: Option<String>,
        approved: bool,
    ) -> Result<()> {
        let Some(manager) = approval_manager else {
            Self::send_control_reply(
                outbound_tx,
                events,
                msg,
                "[Approval] Approval manager is not wired in this instance.".to_string(),
            );
            return Ok(());
        };
        let pending = manager
            .visible_pending(&msg.session_key(), &msg.channel, &msg.chat_id)
            .await;
        let matched: Vec<crate::tools::approval::ApprovalRequest> = match &target {
            Some(prefix) => pending
                .into_iter()
                .filter(|request| request.id.to_ascii_lowercase().starts_with(prefix))
                .collect(),
            None => pending,
        };
        let reply = match matched.as_slice() {
            [] => match &target {
                Some(prefix) => format!("[Approval] No waiting request matches `{prefix}`."),
                None => "[Approval] Nothing is waiting for an answer.".to_string(),
            },
            [one] => {
                let response = crate::tools::approval::ApprovalResponse {
                    request_id: one.id.clone(),
                    approved,
                    responder: format!("{}:{}", msg.channel, msg.chat_id),
                    timestamp: chrono::Utc::now(),
                };
                match manager.submit_response(response).await {
                    Ok(crate::tools::approval::SubmitOutcome::Delivered) => format!(
                        "[Approval] {} {} — `{}`",
                        if approved { "approved" } else { "rejected" },
                        short_request_id(&one.id),
                        one.command
                    ),
                    Ok(crate::tools::approval::SubmitOutcome::NotPending) => {
                        "[Approval] That request was already answered elsewhere; nothing to do."
                            .to_string()
                    }
                    Err(e) => format!("[Approval] Could not submit the answer: {e:#}"),
                }
            }
            many => format!(
                "[Approval] {} requests are waiting; reply `/approve <id-prefix>` or `/reject <id-prefix>`:\n{}",
                many.len(),
                format_pending_approvals(many)
            ),
        };
        Self::send_control_reply(outbound_tx, events, msg, reply);
        Ok(())
    }

    /// Handle /tasks: show the task board of the project this chat works in.
    async fn handle_tasks(&self, msg: &InboundMessage) -> Result<()> {
        let workspace = self.turn_workspace(&self.workspace, &msg.chat_id).await;
        let board = crate::tasks::TaskBoard::load(&workspace);
        self.reply_control(
            msg,
            format!("[Tasks] {}\n{}", workspace.display(), board.summary()),
        )
    }

    /// Send a control-command reply back to the chat that asked.
    fn reply_control(&self, msg: &InboundMessage, text: String) -> Result<()> {
        Self::send_control_reply(&self.outbound_tx, &self.events, msg, text);
        Ok(())
    }

    /// Control replies travel on two paths: the outbound bus (channels such as Feishu/DingTalk)
    /// and the event bus (the web workbench, which drops plain outbound chat text so streamed
    /// turn output is not duplicated). Without the event, `/clear`, `/stop` and `/tools` would
    /// reply into a void and the UI would look frozen.
    fn send_control_reply(
        outbound_tx: &broadcast::Sender<OutboundMessage>,
        events: &EventBus,
        msg: &InboundMessage,
        text: String,
    ) {
        events.publish(
            &msg.channel,
            &msg.chat_id,
            AgentEvent::Notice {
                message: text.clone(),
            },
        );
        let _ = outbound_tx.send(OutboundMessage::chat(
            msg.channel.clone(),
            msg.chat_id.clone(),
            text,
            vec![],
            None,
        ));
    }

    /// Runs queued `/clear` requests once the turn that blocked them has released the loop lock.
    ///
    /// `/clear` cannot wait for the lock (that would freeze the dispatcher and with it `/stop`),
    /// and it must not be dropped either, or a user who clears a long turn would silently keep the
    /// old session. Queued clears run in arrival order, one per lock acquisition.
    async fn flush_deferred_clears(
        loop_ref: &Arc<Mutex<AgentLoop>>,
        deferred: &mut VecDeque<InboundMessage>,
    ) {
        while !deferred.is_empty() {
            let Ok(guard) = loop_ref.try_lock() else {
                return;
            };
            let Some(msg) = deferred.pop_front() else {
                return;
            };
            info!(chat = %msg.chat_id, "Running queued /clear");
            guard.handle_clear(&msg).await.ok();
            drop(guard);
        }
    }

    /// Tells the user this chat is busy instead of queueing behind a running turn.
    ///
    /// A turn holds the loop lock until it ends, so blocking here would freeze the receive loop
    /// and with it the very `/stop` that is supposed to end the turn.
    fn busy_notice(
        outbound_tx: &broadcast::Sender<OutboundMessage>,
        events: &EventBus,
        msg: &InboundMessage,
    ) {
        Self::send_control_reply(
            outbound_tx,
            events,
            msg,
            format!("[Control] Busy. {}", busy_hint_commands()),
        );
    }

    /// Handle /clear: clear session (same as reset_session tool) and workflow state for this chat.
    async fn handle_clear(&self, msg: &InboundMessage) -> Result<()> {
        let history_key = match self.resolve_history_session_key(msg).await {
            Some(k) => k,
            None => {
                return self.reply_control(msg, "[Clear] Could not resolve session.".to_string());
            }
        };
        let session_key = msg.session_key();
        if let Err(e) = self.session_state.clear_session(&history_key).await {
            return self.reply_control(msg, format!("[Clear] Failed: {}.", e));
        }
        let _ = self.workflow_store.delete_state(&session_key).await;
        self.reply_control(
            msg,
            "[Clear] Session cleared. Conversation will continue as a fresh chat.".to_string(),
        )
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
        self.spawn_workflow_run_parts(
            &msg.channel,
            &msg.chat_id,
            history_session_key,
            session_key,
            state,
            cancel,
        )
    }

    /// Channel/chat form of [`Self::spawn_workflow_run`], so the workbench can resume a workflow
    /// without fabricating an `InboundMessage` (and without knowing this daemon's metadata).
    fn spawn_workflow_run_parts(
        &self,
        channel: &str,
        chat_id: &str,
        history_session_key: Option<String>,
        session_key: &str,
        state: WorkflowState,
        cancel: Option<CancellationToken>,
    ) -> tokio::task::JoinHandle<()> {
        let store = self.workflow_store.clone();
        let model = Arc::clone(&self.model);
        let outbound_tx = self.outbound_tx.clone();
        let events = Arc::clone(&self.events);
        let pending_input = self.pending_workflow_input.clone();
        let session_state = self.session_state.clone();
        let channel = channel.to_string();
        let chat_id = chat_id.to_string();
        let session_key = session_key.to_string();
        let cancel_for_run = cancel.clone();
        tokio::spawn(async move {
            if let Err(e) = run_workflow(
                &store,
                model.as_ref(),
                &outbound_tx,
                &events,
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

    /// Run or restart the persisted workflow of `session_key` from the workbench
    /// (`POST /api/workflows/run`).
    ///
    /// Deliberately does **not** append to the chat history: the web UI is not a chat client for
    /// that session, and re-deriving a history key from a synthetic message would risk mixing the
    /// workflow output into the wrong conversation. Progress still reaches the originating chat
    /// through `outbound_tx`, which is where the user expects to see it.
    ///
    /// `restart` clears inputs/step outputs and runs from step 0; otherwise the run continues from
    /// `current_step_index`. A finished workflow cannot be resumed — that would silently produce a
    /// "completed" state with no work done — so callers must ask for `restart` explicitly.
    pub async fn web_run_workflow(&mut self, session_key: &str, restart: bool) -> Result<String> {
        let (channel, chat_id) = split_session_key(session_key)
            .ok_or_else(|| anyhow::anyhow!("session key must look like 'channel:chat_id'"))?;
        let state = self
            .workflow_store
            .load_state(session_key)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no workflow for this session"))?;
        let state = if restart {
            WorkflowState::new(
                session_key.to_string(),
                state.definition.clone(),
                std::collections::HashMap::new(),
                self.workflow_user_input_timeout_secs,
            )
        } else {
            if state.is_finished() {
                anyhow::bail!(
                    "workflow already ended ({:?}); restart it instead of resuming",
                    state.status
                );
            }
            state
        };
        self.workflow_store.save_state(session_key, &state).await?;
        let _ = self.spawn_workflow_run_parts(&channel, &chat_id, None, session_key, state, None);
        Ok(if restart { "restarted" } else { "resumed" }.to_string())
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
        let session_messages = self
            .session_state
            .get_or_create_session_messages(&session_key)
            .await;
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
            let session_messages_sm: Vec<crate::agent::session::SessionMessage> = messages
                .iter()
                .map(crate::agent::session::SessionMessage::from_message)
                .collect();
            let history = sm.get_or_create(&session_id);
            history.clear();
            history.extend(session_messages_sm);
        }
        if let Err(e) = self
            .session_state
            .session_store
            .save_session(&session_key, &messages, Some(&meta))
            .await
        {
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
            let model_max_iterations = agent_ctx.params.max_iterations;

            let session_id = {
                let sm = self.session_state.session_manager.write().await;
                sm.resolve_session(&agent_id, &msg.channel, &msg.chat_id, &msg.metadata)
            };
            let session_key = session_id.format();

            // A chat bound to a project works inside that folder (tools, AGENTS.md and rules).
            let agent_workspace = self
                .turn_workspace(&agent_ctx.workspace_dir, &msg.chat_id)
                .await;
            let tool_ctx = ToolContext {
                agent_id: agent_id.clone(),
                workspace: agent_workspace.clone(),
            };

            // When message is a response to a pending approval, prepend instruction so the agent calls submit_approval_response
            let base_content = if agent_id == "main" {
                directive.content.clone()
            } else {
                format!("@@{} {}", agent_id, directive.content)
            };
            let base_content = apply_sender_display_prefix(&base_content, &msg.metadata);
            let base_content = expand_custom_command(&agent_workspace, &agent_id, base_content);
            let user_content = if let Some(rid) = msg
                .metadata
                .get("pending_approval_request_id")
                .and_then(|v| v.as_str())
            {
                format!(
                    "[Context: The user is responding to a pending command approval request (request_id: {}). Interpret their message as approve or reject and call submit_approval_response with request_id \"{}\" and approved (true or false).]\n\nUser: {}",
                    rid, rid, base_content
                )
            } else {
                base_content
            };

            let cfg_snapshot = self.shared_config.read().await.clone();
            #[cfg(feature = "memory-index")]
            if cfg_snapshot.memory.auto_index {
                let _ =
                    crate::agent::memory_index::reindex_if_changed_async(&agent_id, &cfg_snapshot)
                        .await;
            }
            let memory_cfg_arc = std::sync::Arc::new(cfg_snapshot);
            #[cfg(feature = "memory-index")]
            let system_prompt = {
                let context_builder = ContextBuilder::new(
                    &agent_workspace,
                    &agent_id,
                    config::skills_dir().as_path(),
                    self.tool_sandbox_exec_kind,
                    Some(std::sync::Arc::clone(&memory_cfg_arc)),
                );
                context_builder.build_system_prompt_with_role_prompt(
                    &agent_ctx.system_prompt,
                    Some(user_content.as_str()),
                )
            };
            #[cfg(not(feature = "memory-index"))]
            let system_prompt = {
                let context_builder = ContextBuilder::new(
                    &agent_workspace,
                    &agent_id,
                    config::skills_dir().as_path(),
                    self.tool_sandbox_exec_kind,
                );
                context_builder.build_system_prompt_with_role_prompt(&agent_ctx.system_prompt, None)
            };

            let tool_defs = self.tools.rig_definitions();
            let session_messages = self
                .session_state
                .get_or_create_session_messages(&session_key)
                .await;
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
            self.session_state
                .set_active(&session_key, "processing")
                .await;
            if let Some(ref h) = self.hooks {
                let directive_preview = if directive.content.len() > 200 {
                    format!("{}...", &directive.content[..200])
                } else {
                    directive.content.clone()
                };
                h.dispatch(HookEvent::AgentRunStart {
                    agent_id: agent_id.clone(),
                    directive_preview,
                    session_key: msg.session_key(),
                })
                .await;
            }
            let max_chat_history_messages = agent_ctx.params.max_chat_history_messages;
            let run_result = scope(tool_ctx, async {
                run_completion_loop(
                    &*self.model,
                    &system_prompt,
                    agent_ctx.params.max_tokens,
                    agent_ctx.params.temperature,
                    model_max_iterations,
                    agent_ctx.params.max_idle_iterations,
                    agent_ctx.params.max_consecutive_tool_errors,
                    max_chat_history_messages,
                    &agent_id,
                    &mut *history_guard,
                    &tool_defs,
                    &self.tools,
                    self.checkpoints.as_ref(),
                    &msg.channel,
                    &msg.chat_id,
                    &msg.sender_id,
                    &session_key,
                    &self.outbound_tx,
                    self.events.as_ref(),
                    self.hooks.clone(),
                    self.tool_result_preview_chars,
                    cancel.as_ref(),
                    Some(memory_cfg_arc),
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
                    session_key: msg.session_key(),
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
                let session_messages_sm: Vec<crate::agent::session::SessionMessage> = messages
                    .iter()
                    .map(crate::agent::session::SessionMessage::from_message)
                    .collect();
                let history = sm.get_or_create(&session_id);
                history.clear();
                history.extend(session_messages_sm);
            }
            if let Err(e) = self
                .session_state
                .session_store
                .save_session(&session_key, &messages, Some(&meta))
                .await
            {
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
            let model_max_iterations = agent_ctx.params.max_iterations;

            let session_id = {
                let sm = self.session_state.session_manager.write().await;
                sm.resolve_session(&agent_id, &msg.channel, &msg.chat_id, &msg.metadata)
            };
            let session_key = session_id.format();

            let tool_defs = self.tools.rig_definitions();

            // A chat bound to a project works inside that folder (tools, AGENTS.md and rules).
            let agent_workspace = self
                .turn_workspace(&agent_ctx.workspace_dir, &msg.chat_id)
                .await;

            let base_content = if agent_id == "main" {
                directive.content.clone()
            } else {
                format!("@@{} {}", agent_id, directive.content)
            };
            let base_content = apply_sender_display_prefix(&base_content, &msg.metadata);
            let base_content = expand_custom_command(&agent_workspace, &agent_id, base_content);
            let user_content = if let Some(rid) = msg
                .metadata
                .get("pending_approval_request_id")
                .and_then(|v| v.as_str())
            {
                format!(
                    "[Context: The user is responding to a pending command approval request (request_id: {}). Interpret their message as approve or reject and call submit_approval_response with request_id \"{}\" and approved (true or false).]\n\nUser: {}",
                    rid, rid, base_content
                )
            } else {
                base_content
            };

            let cfg_snapshot = self.shared_config.read().await.clone();
            #[cfg(feature = "memory-index")]
            if cfg_snapshot.memory.auto_index {
                let _ =
                    crate::agent::memory_index::reindex_if_changed_async(&agent_id, &cfg_snapshot)
                        .await;
            }
            let memory_cfg_arc = std::sync::Arc::new(cfg_snapshot);
            #[cfg(feature = "memory-index")]
            let system_prompt = {
                let context_builder = ContextBuilder::new(
                    &agent_workspace,
                    &agent_id,
                    config::skills_dir().as_path(),
                    self.tool_sandbox_exec_kind,
                    Some(std::sync::Arc::clone(&memory_cfg_arc)),
                );
                context_builder.build_system_prompt_with_role_prompt(
                    &agent_ctx.system_prompt,
                    Some(user_content.as_str()),
                )
            };
            #[cfg(not(feature = "memory-index"))]
            let system_prompt = {
                let context_builder = ContextBuilder::new(
                    &agent_workspace,
                    &agent_id,
                    config::skills_dir().as_path(),
                    self.tool_sandbox_exec_kind,
                );
                context_builder.build_system_prompt_with_role_prompt(&agent_ctx.system_prompt, None)
            };

            // Push user message into session history before spawning
            let session_messages = self
                .session_state
                .get_or_create_session_messages(&session_key)
                .await;
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

            let model = Arc::clone(&self.model);
            let tools = Arc::clone(&self.tools);
            let checkpoints = Arc::clone(&self.checkpoints);
            let session_state = self.session_state.clone();
            let outbound_tx = self.outbound_tx.clone();
            let events = Arc::clone(&self.events);
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
            let max_idle_iterations = agent_ctx.params.max_idle_iterations;
            let max_tokens = agent_ctx.params.max_tokens;
            let temperature = agent_ctx.params.temperature;

            let session_messages_clone = session_messages.clone();
            let memory_cfg_for_task = memory_cfg_arc.clone();
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
                        max_tokens,
                        temperature,
                        model_max_iterations,
                        max_idle_iterations,
                        max_consecutive_tool_errors,
                        max_chat_history_messages,
                        &aid,
                        &mut *history_guard,
                        &tool_defs,
                        &tools,
                        checkpoints.as_ref(),
                        &channel,
                        &chat_id,
                        &sender_id_for_loop,
                        &session_id_str,
                        &outbound_tx,
                        events.as_ref(),
                        hooks.clone(),
                        tool_result_preview_chars,
                        None, // subagent tasks use timeout; no /stop cancel
                        Some(memory_cfg_for_task),
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
                    let session_messages_sm: Vec<crate::agent::session::SessionMessage> = messages
                        .iter()
                        .map(crate::agent::session::SessionMessage::from_message)
                        .collect();
                    let history = sm.get_or_create(&sid);
                    history.clear();
                    history.extend(session_messages_sm);
                }
                if let Err(e) = session_state
                    .session_store
                    .save_session(&sk, &messages, Some(&meta))
                    .await
                {
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
                let scope = crate::agent::subagent::SubagentScope {
                    channel: msg.channel.clone(),
                    chat_id: msg.chat_id.clone(),
                    session_id: session_key.clone(),
                };
                mgr.spawn_fn(label, None, scope, task_future, None).await
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
                            let body = handle.result.as_deref().unwrap_or("(no output)");
                            let _ = self.outbound_tx.send(OutboundMessage::chat(
                                msg.channel.clone(),
                                msg.chat_id.clone(),
                                format!("[Subagent {}] Result:\n\n{}", handle.label, body),
                                vec![],
                                None,
                            ));
                        }
                        SubagentStatus::Cancelled => {
                            let _ = self.outbound_tx.send(OutboundMessage::chat(
                                msg.channel.clone(),
                                msg.chat_id.clone(),
                                format!("[Subagent {}] Cancelled.", handle.label),
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
            format!(
                "Unknown agent '{}'. Available: {}",
                unknown_agent, agent_list
            ),
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

fn parse_provider_error_message(raw: &str) -> String {
    // The provider error often looks like:
    //   "... with message: {\"error\":{\"message\":\"Invalid max_tokens ...\" ...}}"
    // We try to extract `error.message` from the trailing JSON if present, otherwise fallback to the raw text.
    let suffix = raw
        .splitn(2, "with message:")
        .nth(1)
        .map(str::trim)
        .unwrap_or(raw)
        .trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(suffix) {
        if let Some(msg) = v
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return msg.to_string();
        }
    }
    suffix.to_string()
}

fn user_facing_completion_error(
    agent_id: &str,
    err: &CompletionError,
    requested_max_tokens: u32,
) -> Option<String> {
    match err {
        CompletionError::ProviderError(raw) => {
            let msg = parse_provider_error_message(raw);
            // Special-case the most common "config typo" class so users know what to change.
            if msg.to_lowercase().contains("max_tokens") {
                return Some(format!(
                    "[Agent '{}'] The LLM request was rejected by the provider: {}\n\nLikely cause: `max_tokens` is invalid or exceeds the model/provider limit.\nHow to fix: adjust `mainAgent.max_tokens` or `mainAgent.agents[].max_tokens` to a valid range and retry. (requested max_tokens={})",
                    agent_id, msg, requested_max_tokens
                ));
            }
            Some(format!(
                "[Agent '{}'] LLM provider request failed: {}\n\nHow to fix: verify provider/model/API key/API base and related request parameters, then retry.",
                agent_id, msg
            ))
        }
        // Other completion errors are often transient or internal; keep them terse but visible.
        CompletionError::ResponseError(raw) => Some(format!(
            "[Agent '{}'] Failed to parse the LLM response: {}\n\nHow to fix: retry later; if it persists, check provider compatibility/version.",
            agent_id,
            raw.trim()
        )),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Chat history window helpers
// ---------------------------------------------------------------------------

/// True if this user message carries tool results (serialized as `role: tool` for DeepSeek/OpenAI).
pub(crate) fn user_message_contains_tool_results(msg: &Message) -> bool {
    match msg {
        Message::User { content } => content
            .iter()
            .any(|c| matches!(c, UserContent::ToolResult(_))),
        _ => false,
    }
}

pub(crate) fn assistant_message_has_tool_calls(msg: &Message) -> bool {
    match msg {
        Message::Assistant { content, .. } => content
            .iter()
            .any(|c| matches!(c, AssistantContent::ToolCall(_))),
        _ => false,
    }
}

/// When the last-N message window is taken, it can start at a `User` message that only contains
/// tool results while the preceding `Assistant` (with `tool_calls`) falls **outside** the slice.
/// DeepSeek rejects `role: tool` without that assistant in the same request. Walk the window start
/// backward until tool results are preceded by the issuing assistant inside the slice, or we hit the
/// beginning of history.
pub(crate) fn fix_window_start_for_tool_results(history: &[Message], mut start: usize) -> usize {
    loop {
        if start >= history.len() {
            return start;
        }
        if !user_message_contains_tool_results(&history[start]) {
            return start;
        }
        if start == 0 {
            warn!(
                "Chat history window starts with tool results but no preceding assistant; \
                 include more history or reset the session to avoid provider errors"
            );
            return start;
        }
        if assistant_message_has_tool_calls(&history[start - 1]) {
            // Include `history[start - 1]` so the request is never `tool` without prior `tool_calls`.
            return start.saturating_sub(1);
        }
        start -= 1;
    }
}

// ---------------------------------------------------------------------------
// Standalone completion loop
// ---------------------------------------------------------------------------

async fn run_completion_loop(
    model: &dyn SynbotCompletionModel,
    system_prompt: &str,
    max_tokens: u32,
    temperature: f32,
    max_iterations: u32,
    max_idle_iterations: u32,
    max_consecutive_tool_errors: u32,
    max_chat_history_messages: u32,
    agent_id: &str,
    history: &mut Vec<Message>,
    tool_defs: &[rig::completion::ToolDefinition],
    tools: &ToolRegistry,
    checkpoints: &crate::checkpoint::CheckpointStore,
    channel: &str,
    chat_id: &str,
    sender_id: &str,
    session_id: &str,
    outbound_tx: &broadcast::Sender<OutboundMessage>,
    events: &EventBus,
    hooks: Option<Arc<HookRegistry>>,
    tool_result_preview_chars: usize,
    cancel: Option<&CancellationToken>,
    memory_cfg: Option<Arc<Config>>,
) -> Result<u32> {
    if let Some(ref c) = memory_cfg {
        if let Err(e) = crate::agent::session_compactor::maybe_compact_history(
            model,
            c.as_ref(),
            agent_id,
            history,
            max_chat_history_messages,
        )
        .await
        {
            tracing::warn!(error = %e, "session compaction skipped");
        }
    }
    let message_ctx = Some((channel, chat_id, sender_id, session_id));
    // One user message maps to one turn; the UI groups every event by this id.
    let turn_id = uuid::Uuid::new_v4().to_string();
    events.publish(
        channel,
        chat_id,
        AgentEvent::TurnStarted {
            turn_id: turn_id.clone(),
            agent_id: agent_id.to_string(),
        },
    );
    // Rounds actually spent doing new work, and rounds that only repeated what the turn already
    // did. Only the former draws on `max_iterations`, so a turn that waits on background work is
    // not killed early while a turn that spins in place still stops.
    let mut iterations = 0u32;
    let mut productive_rounds = 0u32;
    let mut idle_rounds = 0u32;
    let mut consecutive_idle = 0u32;
    // Results of read-only calls already executed in this turn, keyed by (tool, args). Mutations
    // clear it, so a re-read after a write still hits the filesystem.
    let mut read_cache = std::collections::HashMap::<(String, String), String>::new();
    // Last output per status-poll call, so an unchanged poll (`list_subagents` while a teammate
    // keeps running) is recognised as "no progress".
    let mut poll_outputs = std::collections::HashMap::<String, String>::new();
    // Appended to the system prompt once the model keeps spinning, without touching history roles.
    let mut progress_note: Option<String> = None;
    let mut consecutive_tool_errors: u32 = 0;
    // Media paths from "message" tool calls in this run; sent with the final reply so channels (e.g. DingTalk) get one message with text + files.
    let mut pending_media = Vec::<String>::new();

    loop {
        if let Some(token) = cancel {
            if token.is_cancelled() {
                info!(agent_id = %agent_id, "Agent run cancelled by user (/stop or /cancel)");
                publish_turn_finished(
                    events,
                    channel,
                    chat_id,
                    &turn_id,
                    TurnFinishReason::Cancelled,
                    iterations,
                );
                return Err(anyhow::anyhow!("Cancelled by user (/stop or /cancel)"));
            }
        }
        if productive_rounds >= max_iterations {
            warn!(
                productive_rounds,
                idle_rounds,
                max_iterations,
                agent_id = %agent_id,
                "Max productive rounds reached; stopping turn"
            );
            let msg = format!(
                "[Agent '{}'] 本轮已用完 {} 个有效迭代额度（模型往返次数，不含等待与重复调用），已暂停。\n\n已完成的工作都已保存。可以继续发消息接手剩余部分，或在设置 → 主 Agent → 「最大工具迭代次数」调大额度。",
                agent_id, max_iterations
            );
            let _ = outbound_tx.send(OutboundMessage::chat(
                channel.to_string(),
                chat_id.to_string(),
                msg,
                vec![],
                None,
            ));
            publish_turn_finished(
                events,
                channel,
                chat_id,
                &turn_id,
                TurnFinishReason::MaxIterations,
                iterations,
            );
            break;
        }
        // Only a *streak* of idle rounds stops the turn: an occasional repeated read between
        // productive rounds is normal, a run of them means the model is going in circles.
        if consecutive_idle >= max_idle_iterations {
            warn!(
                idle_rounds,
                consecutive_idle,
                max_idle_iterations,
                agent_id = %agent_id,
                "Turn stalled without progress; stopping"
            );
            let msg = format!(
                "[Agent '{}'] 连续 {} 轮没有新进展（重复调用已有工具或反复轮询状态），已停止，避免继续消耗 token。\n\n已完成的工作都已保存；换个说法或补充信息后可以继续。",
                agent_id, consecutive_idle
            );
            let _ = outbound_tx.send(OutboundMessage::chat(
                channel.to_string(),
                chat_id.to_string(),
                msg,
                vec![],
                None,
            ));
            publish_turn_finished(
                events,
                channel,
                chat_id,
                &turn_id,
                TurnFinishReason::NoProgress,
                iterations,
            );
            break;
        }
        iterations += 1;

        tracing::debug!("History check again: {:?}", history);

        // Send at most the last N messages to the model; full history remains in session for persistence.
        let history_for_model: Vec<Message> = if history.len() <= max_chat_history_messages as usize
        {
            history.clone()
        } else {
            let mut start = history.len() - max_chat_history_messages as usize;
            start = fix_window_start_for_tool_results(history, start);
            history[start..].to_vec()
        };
        let chat_history = if history_for_model.is_empty() {
            OneOrMany::one(Message::user(""))
        } else {
            OneOrMany::many(history_for_model).expect("non-empty history")
        };
        let request = CompletionRequest {
            preamble: Some(match progress_note.as_deref() {
                Some(note) => format!("{system_prompt}\n\n{note}"),
                None => system_prompt.to_string(),
            }),
            chat_history,
            tools: tool_defs.to_vec(),
            documents: vec![],
            temperature: Some(temperature as f64),
            max_tokens: Some(max_tokens as u64),
            tool_choice: None,
            additional_params: None,
        };

        tracing::debug!("Request prompt: {:?}", request);

        let round = match run_model_round(model, request, &turn_id, events, channel, chat_id).await
        {
            Ok(r) => r,
            Err(e) => {
                if let Some(text) = user_facing_completion_error(agent_id, &e, max_tokens) {
                    // Make the error user-visible (channels otherwise look like "no reply").
                    events.publish(
                        channel,
                        chat_id,
                        AgentEvent::Error {
                            turn_id: turn_id.clone(),
                            message: text.clone(),
                        },
                    );
                    publish_turn_finished(
                        events,
                        channel,
                        chat_id,
                        &turn_id,
                        TurnFinishReason::Error,
                        iterations,
                    );
                    history.push(Message::assistant(&text));
                    let out_msg = OutboundMessage::chat(
                        channel.to_string(),
                        chat_id.to_string(),
                        text,
                        vec![],
                        None,
                    );
                    if let Some(ref h) = hooks {
                        h.dispatch(HookEvent::MessageSent(out_msg.clone())).await;
                    }
                    let _ = outbound_tx.send(out_msg);
                    break;
                }
                events.publish(
                    channel,
                    chat_id,
                    AgentEvent::Error {
                        turn_id: turn_id.clone(),
                        message: e.to_string(),
                    },
                );
                publish_turn_finished(
                    events,
                    channel,
                    chat_id,
                    &turn_id,
                    TurnFinishReason::Error,
                    iterations,
                );
                return Err(anyhow::anyhow!(
                    "completion failed (agent_id={}): {}",
                    agent_id,
                    e
                ));
            }
        };
        if let Some(usage) = round.usage {
            events.publish(
                channel,
                chat_id,
                AgentEvent::Usage {
                    turn_id: turn_id.clone(),
                    prompt_tokens: usage.input_tokens,
                    completion_tokens: usage.output_tokens,
                    total_tokens: usage.total_tokens,
                },
            );
        }

        let normalized_choice: Vec<AssistantContent> =
            crate::agent::embedded_tool_calls::normalize_embedded_tool_calls(round.choice);

        for (i, c) in normalized_choice.iter().enumerate() {
            match c {
                AssistantContent::Text(t) => {
                    let preview = truncate_debug(&t.text, 800);
                    tracing::debug!(
                        "Completion response [{}] Text ({} chars): {}",
                        i,
                        t.text.len(),
                        preview
                    );
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
        let text_preview: String = normalized_choice
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
                format!(
                    "{}... [truncated, total {} chars]",
                    &text_preview[..end],
                    text_preview.len()
                )
            } else {
                text_preview.clone()
            };
            tracing::debug!("Completion output (text): {}", trunc);
        }

        let mut has_tool_calls = false;
        let mut text_parts = Vec::new();
        let mut assistant_contents = Vec::new();
        let mut calls: Vec<ToolCall> = Vec::new();

        for content in normalized_choice.into_iter() {
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
                        if let Some(arr) = tc
                            .function
                            .arguments
                            .get("files")
                            .and_then(|a| a.as_array())
                        {
                            for v in arr {
                                if let Some(s) = v.as_str().filter(|x| !x.trim().is_empty()) {
                                    pending_media.push(s.to_string());
                                }
                            }
                        }
                    }
                    calls.push(tc.clone());
                }
            }
        }

        // A call the turn already made with the same arguments has the same answer; mutating tools
        // clear the cache, so a read after a write still hits the filesystem. Serving duplicates
        // from the cache saves a round trip and tells the model it has already looked.
        // Calls run in model order, so once *this* round contains a mutation, nothing after it may
        // be served from the cache — `write_file` → `read_file` must observe the new content.
        let mut cache_usable = true;
        let reused: Vec<Option<String>> = calls
            .iter()
            .map(|tc| {
                if !tools.is_read_only(&tc.function.name) {
                    cache_usable = false;
                    return None;
                }
                if !cache_usable || is_status_poll(&tc.function.name) {
                    return None;
                }
                let key = (tc.function.name.clone(), tc.function.arguments.to_string());
                read_cache.get(&key).cloned()
            })
            .collect();

        // Consecutive read-only calls (reads, searches, fetches) run concurrently — a round that
        // reads eight files should not pay eight sequential round trips. Everything else keeps the
        // model's order, so `write_file` → `read_file` still observes the new content.
        let outcomes = run_tool_calls(
            &calls,
            tools,
            message_ctx,
            checkpoints,
            hooks.as_deref(),
            events,
            channel,
            chat_id,
            session_id,
            &turn_id,
            tool_result_preview_chars,
            &reused,
        )
        .await;

        // Channel progress messages follow the model's call order, not the completion order.
        // A round counts as productive as soon as one call brought new information in.
        let mut tool_results = Vec::with_capacity(calls.len());
        let mut made_progress = false;
        let mut repeated = Vec::<String>::new();
        for (i, (tc, outcome)) in calls.iter().zip(outcomes.into_iter()).enumerate() {
            let name = tc.function.name.as_str();
            let args_key = tc.function.arguments.to_string();
            let _ = outbound_tx.send(OutboundMessage::tool_progress(
                channel.to_string(),
                chat_id.to_string(),
                tc.function.name.clone(),
                outcome.status.to_string(),
                outcome.preview,
            ));

            if reused.get(i).and_then(|r| r.as_deref()).is_some() {
                repeated.push(name.to_string());
            } else if is_status_poll(name) {
                let key = format!("{name}:{args_key}");
                let unchanged = poll_outputs.get(&key) == Some(&outcome.result_text);
                poll_outputs.insert(key, outcome.result_text.clone());
                if unchanged {
                    repeated.push(name.to_string());
                } else {
                    made_progress = true;
                }
            } else {
                if tools.is_read_only(name) {
                    if outcome.status == "success" {
                        read_cache
                            .insert((name.to_string(), args_key), outcome.result_text.clone());
                    }
                } else if outcome.status == "success" {
                    // Anything written invalidates the reads served earlier in this turn.
                    read_cache.clear();
                }
                made_progress = true;
            }

            tool_results.push((tc.id.clone(), outcome.result_text));
        }

        if !calls.is_empty() {
            if made_progress {
                productive_rounds += 1;
                consecutive_idle = 0;
                progress_note = None;
            } else {
                idle_rounds += 1;
                consecutive_idle += 1;
                repeated.sort();
                repeated.dedup();
                tracing::info!(
                    agent_id = %agent_id,
                    idle_rounds,
                    consecutive_idle,
                    repeated = ?repeated,
                    "Round made no progress"
                );
                // Two rounds in a row without new information is the cheapest moment to say so:
                // the model is still mid-turn and can act on it, unlike a post-hoc stop message.
                progress_note = (consecutive_idle >= 2).then(|| {
                    format!(
                        "## Progress check\nThe previous {consecutive_idle} round(s) produced no new information \
                         (repeated calls: {}). Stop re-reading files or re-polling status you already have. \
                         Either change approach, or reply now with what is done and what is still missing.",
                        repeated.join(", ")
                    )
                });
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
                publish_turn_finished(
                    events,
                    channel,
                    chat_id,
                    &turn_id,
                    TurnFinishReason::MaxToolErrors,
                    iterations,
                );
                break;
            }
            let content = match assistant_contents.len() {
                1 => OneOrMany::one(assistant_contents.into_iter().next().unwrap()),
                _ => OneOrMany::many(assistant_contents).expect("non-empty"),
            };
            history.push(Message::Assistant { id: None, content });
            if !tool_results.is_empty() {
                let user_parts: Vec<UserContent> = tool_results
                    .into_iter()
                    .map(|(id, result_str)| {
                        UserContent::tool_result(
                            id,
                            OneOrMany::one(ToolResultContent::text(result_str)),
                        )
                    })
                    .collect();
                let user_content = match user_parts.len() {
                    1 => OneOrMany::one(user_parts.into_iter().next().unwrap()),
                    _ => OneOrMany::many(user_parts).expect("non-empty tool results"),
                };
                history.push(Message::User {
                    content: user_content,
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
            publish_turn_finished(
                events,
                channel,
                chat_id,
                &turn_id,
                TurnFinishReason::Completed,
                iterations,
            );
            break;
        }
    }

    Ok(iterations)
}

/// Result of one tool call, once its hooks and UI events have been emitted.
struct ToolOutcome {
    /// `"success"` or `"failure"`, forwarded to channels.
    status: &'static str,
    /// Tool output; failures are stored as `Error: <message>`.
    result_text: String,
    /// Short form used for events and channel progress messages.
    preview: String,
}

/// Upper bound on read-only tool calls executed in parallel within one model round.
const MAX_CONCURRENT_TOOL_CALLS: usize = 8;

/// Map a successful file-mutating call to the `(path, change_kind)` reported to the UI.
fn file_change(tool_name: &str, args: &serde_json::Value) -> Option<(String, FileChangeKind)> {
    let (key, kind) = match tool_name {
        "write_file" => ("path", FileChangeKind::Modified),
        "edit_file" => ("path", FileChangeKind::Modified),
        "create_dir" => ("path", FileChangeKind::Created),
        "move_file" => ("destination", FileChangeKind::Renamed),
        _ => return None,
    };
    let path = args.get(key)?.as_str()?.trim();
    (!path.is_empty()).then(|| (path.to_string(), kind))
}

/// Truncate to at most `max` bytes on a char boundary, appending `...` when shortened.
fn truncate_preview(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

/// Tools whose only job is to report the state of background work. They are cheap to re-run, so
/// they are never served from the read cache; an unchanged output marks the round idle instead.
fn is_status_poll(tool_name: &str) -> bool {
    matches!(tool_name, "list_subagents")
}

/// Execute all tool calls of one model round. Only *consecutive* read-only calls are parallelised —
/// a `write_file` followed by `read_file` must stay in that order — everything else runs serially in
/// model order. Outcomes are returned in call order.
#[allow(clippy::too_many_arguments)]
async fn run_tool_calls(
    calls: &[ToolCall],
    tools: &ToolRegistry,
    message_ctx: Option<(&str, &str, &str, &str)>,
    checkpoints: &crate::checkpoint::CheckpointStore,
    hooks: Option<&HookRegistry>,
    events: &EventBus,
    channel: &str,
    chat_id: &str,
    session_id: &str,
    turn_id: &str,
    tool_result_preview_chars: usize,
    reused: &[Option<String>],
) -> Vec<ToolOutcome> {
    let mut outcomes: Vec<Option<ToolOutcome>> = calls.iter().map(|_| None).collect();
    let mut next = 0usize;
    while next < calls.len() {
        let mut run_end = next;
        while run_end < calls.len() && tools.is_read_only(&calls[run_end].function.name) {
            run_end += 1;
        }
        if run_end == next {
            outcomes[next] = Some(
                execute_tool_call(
                    &calls[next],
                    tools,
                    message_ctx,
                    checkpoints,
                    hooks,
                    events,
                    channel,
                    chat_id,
                    session_id,
                    turn_id,
                    tool_result_preview_chars,
                    reused.get(next).and_then(|r| r.as_deref()),
                )
                .await,
            );
            next += 1;
            continue;
        }
        let run: Vec<usize> = (next..run_end).collect();
        for batch in run.chunks(MAX_CONCURRENT_TOOL_CALLS) {
            let futures = batch.iter().map(|&i| {
                let tc = calls[i].clone();
                let cached = reused.get(i).and_then(|r| r.clone());
                async move {
                    let outcome = execute_tool_call(
                        &tc,
                        tools,
                        message_ctx,
                        checkpoints,
                        hooks,
                        events,
                        channel,
                        chat_id,
                        session_id,
                        turn_id,
                        tool_result_preview_chars,
                        cached.as_deref(),
                    )
                    .await;
                    (i, outcome)
                }
            });
            for (i, outcome) in futures_util::future::join_all(futures).await {
                outcomes[i] = Some(outcome);
            }
        }
        next = run_end;
    }
    outcomes
        .into_iter()
        .map(|o| o.expect("tool outcome is recorded for every call"))
        .collect()
}

/// Execute one tool call, emitting hook callbacks and UI events around it.
///
/// `reused` carries the earlier result of an identical read-only call in this turn; when set, the
/// tool is not run again and the model is told the answer is unchanged.
#[allow(clippy::too_many_arguments)]
async fn execute_tool_call(
    tc: &ToolCall,
    tools: &ToolRegistry,
    message_ctx: Option<(&str, &str, &str, &str)>,
    checkpoints: &crate::checkpoint::CheckpointStore,
    hooks: Option<&HookRegistry>,
    events: &EventBus,
    channel: &str,
    chat_id: &str,
    session_id: &str,
    turn_id: &str,
    tool_result_preview_chars: usize,
    reused: Option<&str>,
) -> ToolOutcome {
    let args = tc.function.arguments.clone();
    let args_str = serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_string());
    let args_preview = truncate_preview(&args_str, 200);
    if let Some(h) = hooks {
        h.dispatch(HookEvent::ToolRunStart {
            tool_name: tc.function.name.clone(),
            args_preview: args_preview.clone(),
            channel: channel.to_string(),
            chat_id: chat_id.to_string(),
            session_id: session_id.to_string(),
        })
        .await;
    }
    events.publish(
        channel,
        chat_id,
        AgentEvent::ToolCallStarted {
            turn_id: turn_id.to_string(),
            call_id: tc.id.clone(),
            tool_name: tc.function.name.clone(),
            args_preview,
        },
    );
    let started = std::time::Instant::now();
    // Snapshot the pre-image *before* the tool runs; `finish` records the post-image and diff.
    // Only failures that mutate nothing (arg errors, sandbox denials) leave the checkpoint alone.
    let pending_snapshot = checkpoints
        .capture_before(turn_id, &tc.function.name, &tc.function.arguments)
        .await;
    let result = match reused {
        Some(previous) => Ok(format!(
            "{previous}\n\n[unchanged: the same call was made earlier in this turn — reuse the result above instead of calling again]"
        )),
        None => tools.execute(&tc.function.name, args, message_ctx).await,
    };
    let duration_ms = started.elapsed().as_millis() as u64;
    let result_text = match &result {
        Ok(s) => s.clone(),
        Err(e) => format!("Error: {e}"),
    };
    let success = result.is_ok();
    if let Some(pending) = pending_snapshot {
        // A tool can report failure after writing (e.g. `edit_file` with a bad follow-up arg);
        // the post-image is recorded either way so the change is never invisible to undo.
        checkpoints.finish(pending).await;
    }
    if success {
        if let Some((path, change_kind)) = file_change(&tc.function.name, &tc.function.arguments) {
            events.publish(
                channel,
                chat_id,
                AgentEvent::FileChanged {
                    turn_id: turn_id.to_string(),
                    path,
                    change_kind,
                },
            );
        }
    }
    // A plan update is a UI event as much as a tool result: publish the new list so the board
    // refreshes without polling the plan file.
    if success && tc.function.name == "plan_write" {
        if let Some(root) = crate::tasks::current_workspace() {
            let steps = crate::tasks::TaskBoard::load(&root).to_event_steps();
            events.publish(
                channel,
                chat_id,
                AgentEvent::PlanUpdated {
                    turn_id: turn_id.to_string(),
                    steps,
                },
            );
        }
    }
    // list_tools result is meant to be shown in full to the user; do not truncate
    let preview = if tc.function.name == "list_tools" {
        result_text.clone()
    } else {
        truncate_preview(&result_text, tool_result_preview_chars)
    };
    if let Some(h) = hooks {
        h.dispatch(HookEvent::ToolRunEnd {
            tool_name: tc.function.name.clone(),
            result_preview: preview.clone(),
            success,
        })
        .await;
    }
    events.publish(
        channel,
        chat_id,
        AgentEvent::ToolCallFinished {
            turn_id: turn_id.to_string(),
            call_id: tc.id.clone(),
            tool_name: tc.function.name.clone(),
            status: if success {
                ToolCallStatus::Success
            } else {
                ToolCallStatus::Failure
            },
            result_preview: preview.clone(),
            duration_ms,
        },
    );
    ToolOutcome {
        status: if success { "success" } else { "failure" },
        result_text,
        preview,
    }
}

/// Result of one model round (one request → one aggregated assistant message).
struct ModelRound {
    choice: Vec<AssistantContent>,
    usage: Option<rig::completion::Usage>,
}

/// Publish a terminal [`AgentEvent::TurnFinished`].
fn publish_turn_finished(
    events: &EventBus,
    channel: &str,
    chat_id: &str,
    turn_id: &str,
    reason: TurnFinishReason,
    iterations: u32,
) {
    events.publish(
        channel,
        chat_id,
        AgentEvent::TurnFinished {
            turn_id: turn_id.to_string(),
            reason,
            iterations,
        },
    );
}

/// Run one model round, preferring streaming.
///
/// Providers without streaming support (including plugin-provided models, via the trait default
/// implementation), or a stream that dies before producing a final answer, fall back to the
/// blocking [`SynbotCompletionModel::completion`] path. Channels therefore see exactly the same
/// result as before while capable providers stream deltas to the UI.
async fn run_model_round(
    model: &dyn SynbotCompletionModel,
    request: CompletionRequest,
    turn_id: &str,
    events: &EventBus,
    channel: &str,
    chat_id: &str,
) -> Result<ModelRound, CompletionError> {
    match model.completion_stream(request.clone()).await {
        Ok(session) => match consume_stream(session, turn_id, events, channel, chat_id).await {
            Ok(round) => return Ok(round),
            Err((e, streamed_any)) => {
                // Once text reached the UI, retrying would duplicate it: surface the error instead.
                if streamed_any {
                    return Err(e);
                }
                warn!(error = %e, "Streaming completion failed, falling back to non-streaming")
            }
        },
        Err(e) => {
            tracing::debug!(error = %e, "Streaming unsupported by provider, using blocking completion")
        }
    }

    let response = model.completion(request).await?;
    let usage = (response.usage.total_tokens > 0).then_some(response.usage);
    Ok(ModelRound {
        choice: response.choice.into_iter().collect(),
        usage,
    })
}

/// Forward streamed deltas to the [`EventBus`], then await the aggregated choice.
///
/// On failure the error is paired with whether any delta already reached the UI, so the caller
/// can decide between falling back (nothing was shown) and failing the turn (avoid duplicate text).
async fn consume_stream(
    mut session: StreamSession,
    turn_id: &str,
    events: &EventBus,
    channel: &str,
    chat_id: &str,
) -> Result<ModelRound, (CompletionError, bool)> {
    let mut streamed_any = false;
    while let Some(item) = session.deltas.recv().await {
        match item {
            Ok(StreamDelta::Text(delta)) => {
                streamed_any = true;
                events.publish(
                    channel,
                    chat_id,
                    AgentEvent::MessageDelta {
                        turn_id: turn_id.to_string(),
                        delta,
                    },
                );
            }
            Ok(StreamDelta::Reasoning(delta)) => {
                streamed_any = true;
                events.publish(
                    channel,
                    chat_id,
                    AgentEvent::ReasoningDelta {
                        turn_id: turn_id.to_string(),
                        delta,
                    },
                );
            }
            // Tool-call fragments are aggregated into the final choice; nothing to show yet.
            Ok(StreamDelta::ToolCallDelta { .. }) => {}
            Err(e) => return Err((e, streamed_any)),
        };
    }

    match session.final_choice.await {
        Ok(Ok(final_result)) => Ok(ModelRound {
            choice: final_result.choice.into_iter().collect(),
            usage: final_result.usage,
        }),
        Ok(Err(e)) => Err((e, streamed_any)),
        Err(_) => Err((
            CompletionError::ProviderError(
                "streaming task ended without a final response".to_string(),
            ),
            streamed_any,
        )),
    }
}

// ---------------------------------------------------------------------------
// Control-command formatting helpers
// ---------------------------------------------------------------------------

/// Split `channel:chat_id` (what [`InboundMessage::session_key`] produces, and what the workflow
/// store keys on) back into its two halves. Only the **first** colon separates them: chat ids in
/// the wild contain colons of their own (DingTalk conversation ids are the usual offender).
fn split_session_key(session_key: &str) -> Option<(String, String)> {
    let (channel, chat_id) = session_key.split_once(':')?;
    let (channel, chat_id) = (channel.trim(), chat_id.trim());
    if channel.is_empty() || chat_id.is_empty() {
        return None;
    }
    Some((channel.to_string(), chat_id.to_string()))
}

/// First 8 characters of a request id: enough to disambiguate inside one chat.
fn short_request_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

/// Human readable list of waiting approvals, shared by `/pending` and the "which one?" reply.
fn format_pending_approvals(pending: &[crate::tools::approval::ApprovalRequest]) -> String {
    if pending.is_empty() {
        return "No approval is waiting for an answer.".to_string();
    }
    let mut out = format!("{} approval(s) waiting:", pending.len());
    for request in pending {
        let command: String = request.command.chars().take(160).collect();
        out.push_str(&format!(
            "\n- {} | timeout {}s | {}",
            short_request_id(&request.id),
            request.timeout_secs,
            command
        ));
    }
    out
}

#[cfg(test)]
mod control_command_format_tests {
    use super::*;
    use chrono::Utc;

    fn request(id: &str, command: &str) -> crate::tools::approval::ApprovalRequest {
        crate::tools::approval::ApprovalRequest {
            id: id.to_string(),
            session_id: "web:chat".to_string(),
            channel: "web".to_string(),
            chat_id: "chat".to_string(),
            command: command.to_string(),
            working_dir: "/tmp".to_string(),
            context: String::new(),
            timestamp: Utc::now(),
            timeout_secs: 60,
            display_message: None,
        }
    }

    #[test]
    fn formats_waiting_approvals_with_short_ids() {
        let text = format_pending_approvals(&[request("01234567-89ab-cdef", "rm -rf /tmp/x")]);
        assert!(text.contains("1 approval(s) waiting"));
        assert!(text.contains("01234567"));
        assert!(!text.contains("01234567-89ab-cdef"));
        assert!(text.contains("rm -rf /tmp/x"));
    }

    #[test]
    fn empty_pending_says_nothing_is_waiting() {
        assert_eq!(
            format_pending_approvals(&[]),
            "No approval is waiting for an answer."
        );
    }

    #[test]
    fn session_key_splits_on_the_first_colon_only() {
        assert_eq!(
            split_session_key("telegram:12345"),
            Some(("telegram".to_string(), "12345".to_string()))
        );
        // DingTalk conversation ids carry colons of their own.
        assert_eq!(
            split_session_key("dingtalk:cidABC==:staff:xyz"),
            Some(("dingtalk".to_string(), "cidABC==:staff:xyz".to_string()))
        );
        assert_eq!(split_session_key("nocolon"), None);
        assert_eq!(split_session_key(":chat"), None);
        assert_eq!(split_session_key("web:"), None);
    }
}

#[cfg(test)]
mod streaming_round_tests {
    use super::*;
    use crate::rig_provider::{StreamFinal, StreamSession};
    use rig::completion::{CompletionResponse, Usage};
    use std::future::Future;
    use std::pin::Pin;
    use tokio::sync::{mpsc, oneshot};

    /// Model whose streaming behaviour is scripted per test.
    struct MockModel {
        stream_supported: bool,
        delta_texts: Vec<&'static str>,
        stream_text: &'static str,
        /// When false the stream ends without a final response (simulates a broken stream).
        stream_completes: bool,
        completion_text: &'static str,
    }

    impl SynbotCompletionModel for MockModel {
        fn completion(
            &self,
            _request: CompletionRequest,
        ) -> Pin<
            Box<dyn Future<Output = Result<CompletionResponse<()>, CompletionError>> + Send + '_>,
        > {
            let text = self.completion_text;
            Box::pin(async move {
                Ok(CompletionResponse {
                    choice: rig::OneOrMany::one(AssistantContent::text(text)),
                    usage: Usage::new(),
                    raw_response: (),
                })
            })
        }

        fn completion_stream(
            &self,
            _request: CompletionRequest,
        ) -> Pin<Box<dyn Future<Output = Result<StreamSession, CompletionError>> + Send + '_>>
        {
            if !self.stream_supported {
                return Box::pin(async {
                    Err(CompletionError::ProviderError(
                        "streaming is not supported by this model".to_string(),
                    ))
                });
            }
            let deltas = self.delta_texts.clone();
            let text = self.stream_text;
            let completes = self.stream_completes;
            Box::pin(async move {
                let (tx, rx) = mpsc::channel(16);
                let (final_tx, final_rx) = oneshot::channel();
                tokio::spawn(async move {
                    for delta in deltas {
                        if tx
                            .send(Ok(StreamDelta::Text(delta.to_string())))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                    drop(tx);
                    if completes {
                        let _ = final_tx.send(Ok(StreamFinal {
                            choice: rig::OneOrMany::one(AssistantContent::text(text)),
                            usage: None,
                        }));
                    }
                });
                Ok(StreamSession {
                    deltas: rx,
                    final_choice: final_rx,
                })
            })
        }
    }

    fn request() -> CompletionRequest {
        CompletionRequest {
            preamble: None,
            chat_history: OneOrMany::one(Message::user("hi")),
            tools: vec![],
            documents: vec![],
            temperature: None,
            max_tokens: None,
            tool_choice: None,
            additional_params: None,
        }
    }

    fn choice_text(choice: &[AssistantContent]) -> String {
        choice
            .iter()
            .filter_map(|c| match c {
                AssistantContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .collect()
    }

    fn drain_deltas(bus: &EventBus) -> Vec<String> {
        bus.replay_from("web", "u1", 0)
            .into_iter()
            .filter_map(|e| match e.event {
                AgentEvent::MessageDelta { delta, .. } => Some(delta),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn streams_deltas_and_uses_aggregated_choice() {
        let bus = EventBus::with_capacity(32);
        let model = MockModel {
            stream_supported: true,
            delta_texts: vec!["Hel", "lo"],
            stream_text: "Hello",
            stream_completes: true,
            completion_text: "unused",
        };

        let round = run_model_round(&model, request(), "turn-1", &bus, "web", "u1")
            .await
            .expect("round");

        assert_eq!(choice_text(&round.choice), "Hello");
        assert!(round.usage.is_none());
        assert_eq!(
            drain_deltas(&bus),
            vec!["Hel".to_string(), "lo".to_string()]
        );
    }

    #[tokio::test]
    async fn falls_back_when_streaming_is_unsupported() {
        let bus = EventBus::with_capacity(32);
        let model = MockModel {
            stream_supported: false,
            delta_texts: vec![],
            stream_text: "unused",
            stream_completes: true,
            completion_text: "from blocking",
        };

        let round = run_model_round(&model, request(), "turn-1", &bus, "web", "u1")
            .await
            .expect("round");

        assert_eq!(choice_text(&round.choice), "from blocking");
        assert!(drain_deltas(&bus).is_empty());
    }

    #[tokio::test]
    async fn falls_back_when_stream_fails_before_any_delta() {
        let bus = EventBus::with_capacity(32);
        let model = MockModel {
            stream_supported: true,
            delta_texts: vec![],
            stream_text: "unused",
            stream_completes: false,
            completion_text: "from blocking",
        };

        let round = run_model_round(&model, request(), "turn-1", &bus, "web", "u1")
            .await
            .expect("round");

        assert_eq!(choice_text(&round.choice), "from blocking");
    }

    #[tokio::test]
    async fn broken_stream_after_deltas_surfaces_error_instead_of_duplicating() {
        let bus = EventBus::with_capacity(32);
        let model = MockModel {
            stream_supported: true,
            delta_texts: vec!["partial"],
            stream_text: "unused",
            stream_completes: false,
            completion_text: "from blocking",
        };

        let result = run_model_round(&model, request(), "turn-1", &bus, "web", "u1").await;

        assert!(
            result.is_err(),
            "a broken stream after output must fail the turn"
        );
        assert_eq!(drain_deltas(&bus), vec!["partial".to_string()]);
    }
}

#[cfg(test)]
mod parallel_tool_tests {
    use super::*;
    use crate::tools::DynTool;
    use std::sync::Mutex as StdMutex;
    use std::time::{Duration, Instant};

    /// Tool that records start/end into a shared log and sleeps for `delay`.
    struct ProbeTool {
        name: &'static str,
        read_only: bool,
        delay: Duration,
        log: Arc<StdMutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl DynTool for ProbeTool {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            "probe tool"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }
        fn read_only(&self) -> bool {
            self.read_only
        }
        async fn call(&self, _args: serde_json::Value) -> anyhow::Result<String> {
            self.log
                .lock()
                .unwrap()
                .push(format!("start:{}", self.name));
            tokio::time::sleep(self.delay).await;
            self.log.lock().unwrap().push(format!("end:{}", self.name));
            Ok(format!("{} ok", self.name))
        }
    }

    fn registry(
        specs: &[(&'static str, bool, u64)],
        log: &Arc<StdMutex<Vec<String>>>,
    ) -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        for (name, read_only, delay_ms) in specs {
            reg.register(Arc::new(ProbeTool {
                name,
                read_only: *read_only,
                delay: Duration::from_millis(*delay_ms),
                log: Arc::clone(log),
            }))
            .unwrap();
        }
        reg
    }

    fn call(id: &str, name: &str) -> ToolCall {
        ToolCall::new(
            id.to_string(),
            rig::message::ToolFunction::new(name.to_string(), serde_json::json!({})),
        )
    }

    async fn run(reg: &ToolRegistry, calls: Vec<ToolCall>) -> Vec<ToolOutcome> {
        // Probe tools are read-only, so nothing is ever snapshotted here.
        let checkpoints = crate::checkpoint::CheckpointStore::new(
            std::env::temp_dir().join("synbot-loop-test-checkpoints"),
        );
        run_tool_calls(
            &calls,
            reg,
            Some(("web", "chat", "user", "sess")),
            &checkpoints,
            None,
            &EventBus::new(),
            "web",
            "chat",
            "sess",
            "turn-1",
            2000,
            &[],
        )
        .await
    }

    /// A read-only call the turn already made is served from the cache instead of running again.
    #[tokio::test]
    async fn reused_read_only_call_skips_the_tool() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let reg = registry(&[("read_a", true, 0)], &log);
        let checkpoints = crate::checkpoint::CheckpointStore::new(
            std::env::temp_dir().join("synbot-loop-test-checkpoints-reuse"),
        );

        let outcomes = run_tool_calls(
            &[call("1", "read_a")],
            &reg,
            Some(("web", "chat", "user", "sess")),
            &checkpoints,
            None,
            &EventBus::new(),
            "web",
            "chat",
            "sess",
            "turn-2",
            2000,
            &[Some("read_a ok".to_string())],
        )
        .await;

        assert!(
            log.lock().unwrap().is_empty(),
            "a reused call must not execute the tool"
        );
        assert_eq!(outcomes[0].status, "success");
        assert!(outcomes[0].result_text.contains("unchanged"));
    }

    #[tokio::test]
    async fn consecutive_read_only_calls_run_concurrently() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let reg = registry(
            &[
                ("read_a", true, 150),
                ("read_b", true, 150),
                ("read_c", true, 150),
            ],
            &log,
        );

        let started = Instant::now();
        let outcomes = run(
            &reg,
            vec![
                call("1", "read_a"),
                call("2", "read_b"),
                call("3", "read_c"),
            ],
        )
        .await;
        let elapsed = started.elapsed();

        assert_eq!(outcomes.len(), 3);
        assert!(outcomes.iter().all(|o| o.status == "success"));
        assert!(
            elapsed < Duration::from_millis(400),
            "reads must overlap, took {elapsed:?}"
        );
        let entries = log.lock().unwrap().clone();
        let first_end = entries.iter().position(|e| e.starts_with("end:")).unwrap();
        let starts_before_first_end = entries[..first_end]
            .iter()
            .filter(|e| e.starts_with("start:"))
            .count();
        assert_eq!(
            starts_before_first_end, 3,
            "all reads should start before the first one finishes: {entries:?}"
        );
    }

    #[tokio::test]
    async fn mutating_calls_keep_model_order_and_stay_serial() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let reg = registry(
            &[
                ("read_a", true, 10),
                ("write_x", false, 10),
                ("read_b", true, 10),
            ],
            &log,
        );

        run(
            &reg,
            vec![
                call("1", "read_a"),
                call("2", "write_x"),
                call("3", "read_b"),
            ],
        )
        .await;

        assert_eq!(
            *log.lock().unwrap(),
            vec![
                "start:read_a",
                "end:read_a",
                "start:write_x",
                "end:write_x",
                "start:read_b",
                "end:read_b",
            ],
            "a read after a write must observe the write's effect"
        );
    }

    #[test]
    fn file_mutations_map_to_change_events() {
        let write = file_change("write_file", &serde_json::json!({"path": "a.txt"})).unwrap();
        assert_eq!(write, ("a.txt".to_string(), FileChangeKind::Modified));
        let moved = file_change("move_file", &serde_json::json!({"destination": "b.txt"})).unwrap();
        assert_eq!(moved.1, FileChangeKind::Renamed);
        assert!(file_change("read_file", &serde_json::json!({"path": "a.txt"})).is_none());
        assert!(file_change("write_file", &serde_json::json!({"path": "  "})).is_none());
    }

    #[test]
    fn unknown_tools_are_treated_as_mutating() {
        let log = Arc::new(StdMutex::new(Vec::new()));
        let reg = registry(&[("read_a", true, 1), ("write_x", false, 1)], &log);
        assert!(reg.is_read_only("read_a"));
        assert!(!reg.is_read_only("write_x"));
        assert!(!reg.is_read_only("plugin_tool_not_registered"));
    }
}

#[cfg(test)]
mod history_window_tests {
    use super::*;
    use rig::message::{AssistantContent, ToolResultContent, UserContent};

    fn user_tool(id: &str, text: &str) -> Message {
        Message::User {
            content: OneOrMany::one(UserContent::tool_result(
                id.to_string(),
                OneOrMany::one(ToolResultContent::text(text.to_string())),
            )),
        }
    }

    fn assistant_with_tools() -> Message {
        Message::Assistant {
            id: None,
            content: OneOrMany::one(AssistantContent::tool_call(
                "call_1",
                "echo",
                serde_json::json!({}),
            )),
        }
    }

    #[test]
    fn fix_window_extends_back_to_include_assistant_before_tool_results() {
        // 25 messages: indices 0–3 filler, 4 = assistant+tool_calls, 5 = tool result, 6–24 filler.
        // Last-20 window starts at index 5 (tool user) and would drop the assistant at index 4.
        let mut history: Vec<Message> = (0..4)
            .map(|i| Message::user(&format!("filler {i}")))
            .collect();
        history.push(assistant_with_tools());
        history.push(user_tool("a", "result a"));
        history.extend((6..25).map(|i| Message::user(&format!("filler {i}"))));
        assert_eq!(history.len(), 25);
        let naive_start = history.len() - 20; // 5: first message in window is tool-only user
        let fixed = fix_window_start_for_tool_results(&history, naive_start);
        assert_eq!(
            fixed, 4,
            "window must include the assistant before tool results"
        );
        assert!(assistant_message_has_tool_calls(&history[fixed]));
    }

    #[test]
    fn fix_window_unchanged_when_first_message_is_plain_user() {
        let history = vec![
            Message::user("hello"),
            assistant_with_tools(),
            user_tool("x", "y"),
        ];
        let start = fix_window_start_for_tool_results(&history, 1);
        assert_eq!(start, 1);
    }
}

#[cfg(test)]
mod plan_event_tests {
    use super::*;

    fn write_command(dir: &std::path::Path, name: &str, body: &str) {
        let path = dir.join(crate::commands::COMMANDS_DIR);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join(format!("{name}.md")), body).unwrap();
    }

    #[test]
    fn custom_commands_expand_for_main_and_never_shadow_builtins() {
        let dir = tempfile::tempdir().unwrap();
        write_command(dir.path(), "review", "Review $ARGUMENTS.");
        write_command(dir.path(), "stop", "This must never win.");

        assert_eq!(
            expand_custom_command(dir.path(), "main", "/review src/a.rs".to_string()),
            "Review src/a.rs."
        );
        // Built-in control commands win over a template with the same name.
        assert_eq!(
            expand_custom_command(dir.path(), "main", "/stop".to_string()),
            "/stop"
        );
        // Roles keep the literal text: only the main agent owns slash commands.
        assert_eq!(
            expand_custom_command(dir.path(), "dev", "/review src/a.rs".to_string()),
            "/review src/a.rs"
        );
        // Unknown commands pass through untouched.
        assert_eq!(
            expand_custom_command(dir.path(), "main", "/nope".to_string()),
            "/nope"
        );
    }

    /// A successful `plan_write` must reach the UI as `plan_updated`, not only as a tool result.
    #[tokio::test]
    async fn plan_write_publishes_plan_updated() {
        let dir = tempfile::tempdir().unwrap();
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(crate::tools::tasks::PlanWriteTool {
            workspace: dir.path().to_path_buf(),
        }))
        .unwrap();

        let call = ToolCall::new(
            "1".to_string(),
            rig::message::ToolFunction::new(
                "plan_write".to_string(),
                serde_json::json!({
                    "tasks": [
                        { "title": "read code", "status": "in_progress" },
                        { "title": "write tests" }
                    ]
                }),
            ),
        );
        let bus = EventBus::new();
        let checkpoints = crate::checkpoint::CheckpointStore::new(dir.path().join("checkpoints"));
        let ctx = crate::tools::context::ToolContext {
            agent_id: "main".to_string(),
            workspace: dir.path().to_path_buf(),
        };

        let outcomes = crate::tools::context::scope(ctx, async {
            run_tool_calls(
                &[call],
                &reg,
                Some(("web", "chat", "user", "sess")),
                &checkpoints,
                None,
                &bus,
                "web",
                "chat",
                "sess",
                "turn-9",
                2000,
                &[],
            )
            .await
        })
        .await;
        assert_eq!(outcomes[0].status, "success");

        let events = bus.replay_from("web", "chat", 0);
        let plan = events
            .iter()
            .find_map(|e| match &e.event {
                AgentEvent::PlanUpdated { turn_id, steps } => {
                    Some((turn_id.clone(), steps.clone()))
                }
                _ => None,
            })
            .expect("plan_updated should be published");
        assert_eq!(plan.0, "turn-9");
        assert_eq!(plan.1.len(), 2);
        assert_eq!(
            plan.1[0].status,
            crate::agent::events::PlanStepStatus::InProgress
        );
        assert_eq!(
            plan.1[1].status,
            crate::agent::events::PlanStepStatus::Pending
        );
    }
}

#[cfg(test)]
mod control_reply_tests {
    use super::*;

    fn inbound(content: &str) -> InboundMessage {
        InboundMessage {
            channel: "web".to_string(),
            sender_id: "web_admin".to_string(),
            chat_id: "web_admin".to_string(),
            content: content.to_string(),
            timestamp: chrono::Utc::now(),
            media: vec![],
            metadata: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn control_replies_reach_the_event_bus_and_the_outbound_channel() {
        let events = EventBus::new();
        let (outbound_tx, mut outbound_rx) = broadcast::channel(8);
        let mut event_rx = events.subscribe();

        AgentLoop::send_control_reply(
            &outbound_tx,
            &events,
            &inbound("/clear"),
            "[Clear] Session cleared.".to_string(),
        );

        let chat = outbound_rx.try_recv().expect("outbound chat message");
        assert_eq!(chat.chat_id, "web_admin");
        match chat.message_type {
            crate::bus::OutboundMessageType::Chat { content, .. } => {
                assert_eq!(content, "[Clear] Session cleared.");
            }
            other => panic!("unexpected outbound message: {other:?}"),
        }

        // The web workbench drops outbound chat text, so the notice is the only thing it sees.
        let published = event_rx.try_recv().expect("notice event");
        match published.event {
            AgentEvent::Notice { message } => assert_eq!(message, "[Clear] Session cleared."),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn busy_notices_name_the_commands_that_still_work() {
        let hint = busy_hint_commands();
        for command in ["/stop", "/status", "/clear"] {
            assert!(hint.contains(command), "{command} missing from: {hint}");
        }
    }
}

#[cfg(test)]
mod provider_error_parse_tests {
    use super::*;

    #[test]
    fn parse_provider_error_extracts_nested_error_message() {
        let raw = r#"Invalid status code 400 Bad Request (url=https://api.deepseek.com/chat/completions, model=deepseek-reasoner) with message: {"error":{"message":"Invalid max_tokens value, the valid range of max_tokens is [1, 65536]","type":"invalid_request_error","param":null,"code":"invalid_request_error"}}"#;
        let msg = parse_provider_error_message(raw);
        assert_eq!(
            msg,
            "Invalid max_tokens value, the valid range of max_tokens is [1, 65536]"
        );
    }
}
