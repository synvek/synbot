//! Subagent manager — background task execution with full lifecycle management.
//!
//! Each subagent runs in its own tokio task with an independent AgentLoop,
//! communicating results back via a oneshot channel. The manager enforces
//! a configurable concurrency limit and tracks status for all subagents.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use crate::rig_provider::SynbotCompletionModel;
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use rig::completion::CompletionRequest;
use rig::message::{AssistantContent, Message, ToolResultContent, UserContent};
use rig::OneOrMany;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::agent::context::ContextBuilder;
use crate::agent::events::{AgentEvent, EventBus, SubagentInfo, SubagentState};
use crate::config;
use crate::tools::{scope, ToolContext, ToolRegistry};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Status of a subagent.
#[derive(Debug, Clone)]
pub enum SubagentStatus {
    Running,
    Completed,
    Failed(String),
    /// Stopped by `cancel_subagent`, or because the parent turn was cancelled.
    Cancelled,
}

/// Handle exposing the state of a single subagent.
#[derive(Debug, Clone)]
pub struct SubagentHandle {
    pub id: String,
    pub label: String,
    /// Role whose prompt shapes this subagent (`~/.synbot/roles/<role>`), if any.
    pub role: Option<String>,
    pub status: SubagentStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<String>,
    /// Last tool the subagent ran, so the team panel shows progress instead of a spinner.
    pub activity: Option<String>,
}

impl SubagentHandle {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            role: None,
            status: SubagentStatus::Running,
            created_at: Utc::now(),
            completed_at: None,
            result: None,
            activity: None,
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status, SubagentStatus::Running)
    }

    /// UI snapshot for [`AgentEvent::SubagentUpdated`].
    pub fn to_info(&self) -> SubagentInfo {
        SubagentInfo {
            id: self.id.clone(),
            label: self.label.clone(),
            role: self.role.clone(),
            state: match &self.status {
                SubagentStatus::Running => SubagentState::Running,
                SubagentStatus::Completed => SubagentState::Completed,
                SubagentStatus::Failed(_) => SubagentState::Failed,
                SubagentStatus::Cancelled => SubagentState::Cancelled,
            },
            created_at: self.created_at,
            finished_at: self.completed_at,
            activity: match &self.status {
                SubagentStatus::Failed(err) => Some(err.clone()),
                SubagentStatus::Cancelled => Some("cancelled".to_string()),
                _ => self.activity.clone(),
            },
        }
    }
}

/// Which conversation a subagent reports to.
///
/// Events go to the parent chat (so the team panel updates live) and tool approvals are asked
/// there too, which keeps a teammate from turning into an unattended writer.
#[derive(Debug, Clone, Default)]
pub struct SubagentScope {
    pub channel: String,
    pub chat_id: String,
    pub session_id: String,
}

impl SubagentScope {
    /// True when events can be addressed to a conversation.
    pub fn is_addressable(&self) -> bool {
        !self.channel.is_empty() && !self.chat_id.is_empty()
    }
}

/// Cancellation flag shared between the manager (setter) and the running task (observer).
#[derive(Debug, Default)]
pub struct SubagentCancel {
    flag: std::sync::atomic::AtomicBool,
    notify: tokio::sync::Notify,
}

impl SubagentCancel {
    pub fn cancel(&self) {
        self.flag.store(true, std::sync::atomic::Ordering::SeqCst);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Resolves once cancellation is requested (immediately when it already was).
    pub async fn cancelled(&self) {
        loop {
            if self.is_cancelled() {
                return;
            }
            let notified = self.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Shared mutable state for a single subagent that the background task can
/// update and the manager can read.
type SharedHandle = Arc<Mutex<SubagentHandle>>;

/// Final answer of a teammate, boxed so the manager can own the future in a background task.
pub type TaskFuture = Pin<Box<dyn Future<Output = Result<String>> + Send + 'static>>;

/// Invoked once when a teammate finishes, with its label and result.
pub type Completion = Box<dyn FnOnce(String, Result<String>) + Send>;

/// Progress callback handed to a running subagent: publishes activity without exposing the manager.
#[derive(Clone)]
pub struct Reporter {
    handle: SharedHandle,
    events: Option<Arc<EventBus>>,
    scope: SubagentScope,
}

impl Reporter {
    /// Record the latest activity and push one `subagent_updated` event.
    pub async fn activity(&self, text: impl Into<String>) {
        let info = {
            let mut handle = self.handle.lock().await;
            if !handle.is_running() {
                return;
            }
            handle.activity = Some(text.into());
            handle.to_info()
        };
        self.publish(&info);
    }

    fn publish(&self, info: &SubagentInfo) {
        if let (Some(bus), true) = (&self.events, self.scope.is_addressable()) {
            bus.publish(
                &self.scope.channel,
                &self.scope.chat_id,
                AgentEvent::SubagentUpdated {
                    subagent: info.clone(),
                },
            );
        }
    }
}

pub struct SubagentManager {
    /// All subagent handles (both active and finished).
    handles: HashMap<String, SharedHandle>,
    /// Cancellation flag per subagent id (running subagents only).
    cancels: HashMap<String, Arc<SubagentCancel>>,
    /// Maximum number of concurrently running subagents.
    max_concurrent: usize,
    /// If set, each spawned task is limited to this many seconds; on timeout the task is marked failed.
    timeout_secs: Option<u64>,
    /// UI event stream; every state change is published to the subagent's parent conversation.
    events: Option<Arc<EventBus>>,
    /// Roles directory (`~/.synbot/roles`) used to build a `role` subagent's prompt.
    roles_dir: PathBuf,
}

impl SubagentManager {
    /// Create a new manager. `timeout_secs`: if `Some(n)`, each spawned task is limited to `n` seconds.
    pub fn new(max_concurrent: usize, timeout_secs: Option<u64>) -> Self {
        Self {
            handles: HashMap::new(),
            cancels: HashMap::new(),
            max_concurrent,
            timeout_secs,
            events: None,
            roles_dir: config::roles_dir(),
        }
    }

    /// Publish `subagent_updated` events on this bus (the workbench team panel).
    pub fn set_events(&mut self, events: Arc<EventBus>) {
        self.events = Some(events);
    }

    /// Configured concurrency limit.
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
    }

    fn publish(&self, scope: &SubagentScope, info: &SubagentInfo) {
        if let (Some(bus), true) = (&self.events, scope.is_addressable()) {
            bus.publish(
                &scope.channel,
                &scope.chat_id,
                AgentEvent::SubagentUpdated {
                    subagent: info.clone(),
                },
            );
        }
    }

    /// Spawn a new subagent that runs a custom async function in the
    /// background.
    ///
    /// This is the low-level entry point: callers provide a boxed future that
    /// returns `Result<String>`. The higher-level [`spawn`] method wraps a
    /// full `AgentLoop` interaction into such a future.
    ///
    /// If `on_complete` is provided, it is called when the task finishes (with
    /// label and result) so the caller can e.g. send the result to the user.
    ///
    /// Returns the subagent id on success, or an error if the concurrency
    /// limit has been reached.
    pub async fn spawn_fn(
        &mut self,
        label: String,
        role: Option<String>,
        scope: SubagentScope,
        task_fn: TaskFuture,
        on_complete: Option<Completion>,
    ) -> Result<String> {
        self.spawn_with(
            label,
            role,
            scope,
            move |_reporter, _cancel| task_fn,
            on_complete,
        )
        .await
    }

    /// Shared spawn path: enforces the limit, registers the handle + cancel flag, then runs `make_task`
    /// in the background with cancellation and an optional timeout wrapped around it.
    async fn spawn_with<F>(
        &mut self,
        label: String,
        role: Option<String>,
        scope: SubagentScope,
        make_task: F,
        on_complete: Option<Completion>,
    ) -> Result<String>
    where
        F: FnOnce(Reporter, Arc<SubagentCancel>) -> TaskFuture,
    {
        // Enforce concurrency limit
        let active = self.active_count().await;
        if active >= self.max_concurrent {
            bail!(
                "Concurrent subagent limit reached ({}/{}). \
                 Wait for a running subagent to finish before spawning a new one.",
                active,
                self.max_concurrent
            );
        }

        let id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let handle = Arc::new(Mutex::new(SubagentHandle {
            role: role.clone(),
            ..SubagentHandle::new(id.clone(), label.clone())
        }));
        let cancel = Arc::new(SubagentCancel::default());

        self.handles.insert(id.clone(), Arc::clone(&handle));
        self.cancels.insert(id.clone(), Arc::clone(&cancel));
        self.publish(&scope, &handle.lock().await.to_info());
        // Keep the handle map bounded across a long session.
        self.prune_finished(50);

        let reporter = Reporter {
            handle: Arc::clone(&handle),
            events: self.events.clone(),
            scope: scope.clone(),
        };
        let task_fn = make_task(reporter, Arc::clone(&cancel));

        let timeout_secs = self.timeout_secs;
        let label_for_cb = label.clone();
        let events = self.events.clone();
        // Spawn background task (with optional timeout so long-running tasks don't block the slot forever)
        let task_id = id.clone();
        tokio::spawn(async move {
            info!(subagent_id = %task_id, label = %label, timeout_secs = ?timeout_secs, "Subagent started");
            let run = async {
                if let Some(secs) = timeout_secs {
                    match tokio::time::timeout(std::time::Duration::from_secs(secs), task_fn).await
                    {
                        Ok(Ok(out)) => Ok(out),
                        Ok(Err(e)) => Err(e),
                        Err(_) => {
                            warn!(subagent_id = %task_id, label = %label, secs = secs, "Subagent task timed out");
                            Err(anyhow::anyhow!("Task timed out after {} seconds", secs))
                        }
                    }
                } else {
                    task_fn.await
                }
            };
            // Cancellation wins over the task, so a stopped teammate does not keep a model round alive.
            let result = tokio::select! {
                biased;
                _ = cancel.cancelled() => Err(anyhow::anyhow!("cancelled")),
                result = run => result,
            };

            let mut h = handle.lock().await;
            match &result {
                Ok(output) => {
                    info!(subagent_id = %task_id, "Subagent completed");
                    h.status = SubagentStatus::Completed;
                    h.result = Some(output.clone());
                }
                Err(e) => {
                    let err_msg = format!("{e:#}");
                    if cancel.is_cancelled() {
                        warn!(subagent_id = %task_id, "Subagent cancelled");
                        h.status = SubagentStatus::Cancelled;
                    } else {
                        error!(subagent_id = %task_id, error = %err_msg, "Subagent failed");
                        h.status = SubagentStatus::Failed(err_msg.clone());
                    }
                }
            }
            h.completed_at = Some(Utc::now());
            let info = h.to_info();
            drop(h);

            if let (Some(bus), true) = (&events, scope.is_addressable()) {
                bus.publish(
                    &scope.channel,
                    &scope.chat_id,
                    AgentEvent::SubagentUpdated { subagent: info },
                );
            }

            if let Some(cb) = on_complete {
                let result_for_cb = result.map_err(|e| anyhow::anyhow!("{}", e));
                cb(label_for_cb, result_for_cb);
            }
        });

        Ok(id)
    }

    /// Spawn a subagent that executes `task` with its own model + tool loop.
    ///
    /// `role`: optional role name whose prompt (`~/.synbot/roles/<role>`) frames the subagent;
    /// `scope` decides where events and approval requests go.
    ///
    /// Returns the subagent id on success, or an error if the concurrency
    /// limit has been reached.
    /// `agent_id`: which agent's memory to use (e.g. "main" or role name).
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn(
        &mut self,
        label: String,
        role: Option<String>,
        scope: SubagentScope,
        task: String,
        model: Arc<dyn SynbotCompletionModel>,
        workspace: PathBuf,
        tools: Arc<ToolRegistry>,
        agent_id: &str,
        max_tokens: u32,
        temperature: f32,
        on_complete: Option<Completion>,
    ) -> Result<String> {
        let agent_id = agent_id.to_string();
        let roles_dir = self.roles_dir.clone();
        let role_for_prompt = role.clone();
        let scope_for_task = scope.clone();
        self.spawn_with(
            label,
            role,
            scope,
            move |reporter, cancel| {
                Box::pin(run_subagent_task(
                    model,
                    workspace,
                    tools,
                    task,
                    role_for_prompt,
                    agent_id,
                    max_tokens,
                    temperature,
                    scope_for_task,
                    roles_dir,
                    reporter,
                    cancel,
                ))
            },
            on_complete,
        )
        .await
    }

    /// Return cloned handles for **all** subagents (running + finished), oldest first.
    pub async fn list(&self) -> Vec<SubagentHandle> {
        let mut out = Vec::with_capacity(self.handles.len());
        for shared in self.handles.values() {
            out.push(shared.lock().await.clone());
        }
        out.sort_by_key(|h| h.created_at);
        out
    }

    /// Drop the oldest finished subagents beyond `keep`, so a long-lived daemon's handle map stays
    /// bounded. Running subagents are never pruned.
    fn prune_finished(&mut self, keep: usize) {
        let mut finished: Vec<(DateTime<Utc>, String)> = Vec::new();
        for (id, shared) in &self.handles {
            if let Ok(handle) = shared.try_lock() {
                if !handle.is_running() {
                    finished.push((handle.completed_at.unwrap_or(handle.created_at), id.clone()));
                }
            }
        }
        if finished.len() <= keep {
            return;
        }
        finished.sort();
        let drop_count = finished.len() - keep;
        for (_, id) in finished.into_iter().take(drop_count) {
            self.handles.remove(&id);
            self.cancels.remove(&id);
        }
    }

    /// Return a cloned handle for a specific subagent by id.
    pub async fn get_result(&self, id: &str) -> Option<SubagentHandle> {
        match self.handles.get(id) {
            Some(shared) => Some(shared.lock().await.clone()),
            None => None,
        }
    }

    /// Number of currently running subagents.
    pub async fn active_count(&self) -> usize {
        let mut count = 0;
        for shared in self.handles.values() {
            if matches!(shared.lock().await.status, SubagentStatus::Running) {
                count += 1;
            }
        }
        count
    }

    /// Request cancellation of one running subagent. Returns false when the id is unknown, the
    /// subagent already finished, or it was already asked to stop (a repeat is not a new cancel).
    pub async fn cancel(&mut self, id: &str) -> bool {
        let Some(cancel) = self.cancels.get(id) else {
            return false;
        };
        if cancel.is_cancelled() {
            return false;
        }
        let running = match self.handles.get(id) {
            Some(shared) => shared.lock().await.is_running(),
            None => false,
        };
        if !running {
            return false;
        }
        cancel.cancel();
        true
    }

    /// Request cancellation of every running subagent (a stopped parent turn stops its teammates too).
    /// Returns how many were asked to stop.
    pub async fn cancel_all(&mut self) -> usize {
        let mut cancelled = 0;
        for (id, cancel) in &self.cancels {
            if cancel.is_cancelled() {
                continue;
            }
            let running = match self.handles.get(id) {
                Some(shared) => shared.lock().await.is_running(),
                None => false,
            };
            if running {
                cancel.cancel();
                cancelled += 1;
            }
        }
        cancelled
    }
}

// ---------------------------------------------------------------------------
// Background task execution
// ---------------------------------------------------------------------------

/// Run a simplified one-shot agent interaction for the subagent.
///
/// The teammate gets its own context (workspace + memory + skills + optional role prompt), the
/// parent's task board as a read-only shared plan, and the parent conversation as its approval
/// channel. It cannot re-delegate (`spawn` / `run_team`) or rewrite the plan, so one delegated task
/// can never turn into an untracked fan-out.
#[allow(clippy::too_many_arguments)]
async fn run_subagent_task(
    model: Arc<dyn SynbotCompletionModel>,
    workspace: PathBuf,
    tools: Arc<ToolRegistry>,
    task: String,
    role: Option<String>,
    agent_id: String,
    max_tokens: u32,
    temperature: f32,
    chat_scope: SubagentScope,
    roles_dir: PathBuf,
    reporter: Reporter,
    cancel: Arc<SubagentCancel>,
) -> Result<String> {
    let tools = Arc::new(tools.without(&["spawn", "run_team", "plan_write", "cancel_subagent"]));
    let tool_ctx = ToolContext {
        agent_id: agent_id.clone(),
        workspace: workspace.clone(),
    };

    scope(tool_ctx, async move {
        let role_prompt = match &role {
            Some(name) => {
                let prompt = crate::agent::role_registry::build_system_prompt_from_role_dir(
                    &roles_dir, name,
                );
                if prompt.trim().is_empty() {
                    format!("# Role: {name}")
                } else {
                    prompt
                }
            }
            None => String::new(),
        };

        #[cfg(feature = "memory-index")]
        let context = ContextBuilder::new(
            &workspace,
            &agent_id,
            config::skills_dir().as_path(),
            None,
            None,
        );
        #[cfg(not(feature = "memory-index"))]
        let context =
            ContextBuilder::new(&workspace, &agent_id, config::skills_dir().as_path(), None);
        // The task doubles as the memory query, so a teammate recalls what the parent recalled.
        let mut system_prompt = context.build_system_prompt_with_role_prompt(&role_prompt, Some(&task));
        system_prompt.push_str(&format!(
            "\n\n# Delegated task\n\n{task}\n\nYou are one teammate working for a parent agent. \
             Stay inside this task, do not publish or rewrite the shared plan (`plan_write` is not \
             available; the parent owns the board), and finish with a compact report: findings, \
             files touched, and what still needs doing. The parent merges your report with the other \
             teammates'."
        ));
        let tool_defs = tools.rig_definitions();

        let mut history: Vec<Message> = vec![Message::user(&task)];
        let max_iterations: u32 = 15;
        let mut iterations = 0u32;

        loop {
            if cancel.is_cancelled() {
                return Err(anyhow::anyhow!("cancelled"));
            }
            iterations += 1;
            if iterations > max_iterations {
                warn!(
                    iterations = max_iterations,
                    "Subagent hit max iterations, returning partial result"
                );
                break;
            }

            let chat_history = if history.is_empty() {
                rig::OneOrMany::one(Message::user(""))
            } else {
                rig::OneOrMany::many(history.clone()).expect("non-empty history")
            };
            let request = CompletionRequest {
                preamble: Some(system_prompt.clone()),
                chat_history,
                tools: tool_defs.clone(),
                documents: vec![],
                temperature: Some(temperature as f64),
                max_tokens: Some(max_tokens as u64),
                tool_choice: None,
                additional_params: None,
            };

            // A cancelled teammate must not keep a model round (or a tool) alive.
            let response = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(anyhow::anyhow!("cancelled")),
                response = model.completion(request) => response?,
            };

            let normalized_choice: Vec<AssistantContent> =
                crate::agent::embedded_tool_calls::normalize_embedded_tool_calls(
                    response.choice.clone().into_iter().collect(),
                );

            let mut has_tool_calls = false;
            let mut text_parts = Vec::new();
            let mut assistant_contents = Vec::new();
            let mut tool_results = Vec::new();

            for content in normalized_choice.into_iter() {
                match &content {
                    AssistantContent::Text(t) => {
                        text_parts.push(t.text.clone());
                        assistant_contents.push(content.clone());
                    }
                    AssistantContent::Reasoning(_) | AssistantContent::Image(_) => {
                        assistant_contents.push(content.clone());
                    }
                    AssistantContent::ToolCall(tc) => {
                        has_tool_calls = true;
                        assistant_contents.push(content.clone());
                        reporter.activity(format!("tool: {}", tc.function.name)).await;
                        let args = tc.function.arguments.clone();
                        // Approvals go to the parent conversation, so a teammate cannot write
                        // unattended when policy says a command needs a human.
                        let message_ctx = if chat_scope.is_addressable() {
                            Some((
                                chat_scope.channel.as_str(),
                                chat_scope.chat_id.as_str(),
                                "subagent",
                                chat_scope.session_id.as_str(),
                            ))
                        } else {
                            None
                        };
                        let result = tokio::select! {
                            biased;
                            _ = cancel.cancelled() => return Err(anyhow::anyhow!("cancelled")),
                            result = tools.execute(&tc.function.name, args, message_ctx) => result,
                        };
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
                if !reply.is_empty() {
                    return Ok(reply);
                }
                break;
            }
        }

        // If we got here without a text reply, return a summary
        Ok("Subagent completed without producing a text response.".to_string())
    })
    .await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn spawn_publishes_started_and_finished_events_to_the_parent_chat() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();
        let mut mgr = SubagentManager::new(2, None);
        mgr.set_events(Arc::clone(&bus));
        let scope = SubagentScope {
            channel: "web".to_string(),
            chat_id: "u1".to_string(),
            session_id: "web:u1".to_string(),
        };

        let id = mgr
            .spawn_fn(
                "scan auth".to_string(),
                Some("reviewer".to_string()),
                scope,
                Box::pin(async { Ok("found 2 issues".to_string()) }),
                None,
            )
            .await
            .unwrap();

        let started = rx.recv().await.unwrap();
        assert_eq!(started.chat_id, "u1");
        match started.event {
            AgentEvent::SubagentUpdated { subagent } => {
                assert_eq!(subagent.id, id);
                assert_eq!(subagent.state, SubagentState::Running);
                assert_eq!(subagent.role.as_deref(), Some("reviewer"));
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let finished = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("a finish event must be published")
            .unwrap();
        match finished.event {
            AgentEvent::SubagentUpdated { subagent } => {
                assert_eq!(subagent.state, SubagentState::Completed);
                assert!(subagent.finished_at.is_some());
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn cancel_marks_a_running_subagent_as_cancelled() {
        let mut mgr = SubagentManager::new(2, None);
        let id = mgr
            .spawn_fn(
                "slow".to_string(),
                None,
                SubagentScope::default(),
                Box::pin(async {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    Ok("never".to_string())
                }),
                None,
            )
            .await
            .unwrap();

        // A finished subagent cannot be cancelled twice.
        assert!(mgr.cancel(&id).await);
        assert!(!mgr.cancel(&id).await);
        assert!(!mgr.cancel("unknown").await);

        for _ in 0..50 {
            let handle = mgr.get_result(&id).await.unwrap();
            if matches!(handle.status, SubagentStatus::Cancelled) {
                assert_eq!(mgr.active_count().await, 0);
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("subagent was not marked cancelled");
    }

    #[tokio::test]
    async fn test_new_creates_empty_manager() {
        let mgr = SubagentManager::new(3, None);
        assert_eq!(mgr.max_concurrent, 3);
        assert!(mgr.handles.is_empty());
        assert_eq!(mgr.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_active_count_starts_at_zero() {
        let mgr = SubagentManager::new(5, None);
        assert_eq!(mgr.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_list_empty() {
        let mgr = SubagentManager::new(5, None);
        assert!(mgr.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_get_result_nonexistent() {
        let mgr = SubagentManager::new(5, None);
        assert!(mgr.get_result("nonexistent").await.is_none());
    }

    /// Test that the concurrent limit is enforced by directly inserting
    /// handles in Running status and then verifying spawn would fail.
    #[tokio::test]
    async fn test_concurrent_limit_enforcement() {
        let mut mgr = SubagentManager::new(2, None);

        // Manually insert two "running" handles to simulate active subagents
        for i in 0..2 {
            let id = format!("fake-{i}");
            let handle = Arc::new(Mutex::new(SubagentHandle {
                id: id.clone(),
                label: format!("task-{i}"),
                role: None,
                activity: None,
                status: SubagentStatus::Running,
                created_at: Utc::now(),
                completed_at: None,
                result: None,
            }));
            mgr.handles.insert(id, handle);
        }

        assert_eq!(mgr.active_count().await, 2);

        // Attempting to spawn a third should fail because we can't create a
        // real model in tests, but we can verify the limit check by trying
        // to call spawn with a dummy — the limit check happens before model use.
        // We'll test this by checking active_count directly.
        // The spawn method checks active_count first, so with 2 active and
        // max_concurrent=2, it would bail.

        // Verify the limit is at capacity
        assert!(mgr.active_count().await >= mgr.max_concurrent);
    }

    #[tokio::test]
    async fn test_completed_subagent_not_counted_as_active() {
        let mut mgr = SubagentManager::new(2, None);

        // Insert one completed and one running
        let completed_handle = Arc::new(Mutex::new(SubagentHandle {
            id: "done-1".to_string(),
            label: "completed task".to_string(),
            role: None,
            activity: None,
            status: SubagentStatus::Completed,
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
            result: Some("done".to_string()),
        }));
        mgr.handles.insert("done-1".to_string(), completed_handle);

        let running_handle = Arc::new(Mutex::new(SubagentHandle {
            id: "run-1".to_string(),
            label: "running task".to_string(),
            role: None,
            activity: None,
            status: SubagentStatus::Running,
            created_at: Utc::now(),
            completed_at: None,
            result: None,
        }));
        mgr.handles.insert("run-1".to_string(), running_handle);

        // Only the running one should count
        assert_eq!(mgr.active_count().await, 1);
    }

    #[tokio::test]
    async fn test_failed_subagent_not_counted_as_active() {
        let mut mgr = SubagentManager::new(3, None);

        let failed_handle = Arc::new(Mutex::new(SubagentHandle {
            id: "fail-1".to_string(),
            label: "failed task".to_string(),
            role: None,
            activity: None,
            status: SubagentStatus::Failed("some error".to_string()),
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
            result: None,
        }));
        mgr.handles.insert("fail-1".to_string(), failed_handle);

        assert_eq!(mgr.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_list_returns_all_handles() {
        let mut mgr = SubagentManager::new(5, None);

        for i in 0..3 {
            let id = format!("sa-{i}");
            let status = match i {
                0 => SubagentStatus::Running,
                1 => SubagentStatus::Completed,
                _ => SubagentStatus::Failed("err".to_string()),
            };
            let handle = Arc::new(Mutex::new(SubagentHandle {
                id: id.clone(),
                label: format!("task-{i}"),
                role: None,
                activity: None,
                status,
                created_at: Utc::now(),
                completed_at: if i > 0 { Some(Utc::now()) } else { None },
                result: if i == 1 {
                    Some("result".to_string())
                } else {
                    None
                },
            }));
            mgr.handles.insert(id, handle);
        }

        let list = mgr.list().await;
        assert_eq!(list.len(), 3);
    }

    #[tokio::test]
    async fn test_get_result_returns_correct_handle() {
        let mut mgr = SubagentManager::new(5, None);

        let handle = Arc::new(Mutex::new(SubagentHandle {
            id: "test-id".to_string(),
            label: "test label".to_string(),
            role: None,
            activity: None,
            status: SubagentStatus::Completed,
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
            result: Some("test result".to_string()),
        }));
        mgr.handles.insert("test-id".to_string(), handle);

        let result = mgr.get_result("test-id").await;
        assert!(result.is_some());
        let h = result.unwrap();
        assert_eq!(h.id, "test-id");
        assert_eq!(h.label, "test label");
        assert!(matches!(h.status, SubagentStatus::Completed));
        assert_eq!(h.result.as_deref(), Some("test result"));
    }

    #[tokio::test]
    async fn test_max_concurrent_zero_blocks_all_spawns() {
        let mgr = SubagentManager::new(0, None);
        // With max_concurrent=0, even with no active subagents, the limit
        // is already reached (0 >= 0), so no spawns should be possible.
        assert!(mgr.active_count().await >= mgr.max_concurrent);
    }

    #[tokio::test]
    async fn test_spawn_fn_and_collect_result() {
        let mut mgr = SubagentManager::new(5, None);

        let id = mgr
            .spawn_fn(
                "test task".to_string(),
                None,
                SubagentScope::default(),
                Box::pin(async { Ok("hello from subagent".to_string()) }),
                None,
            )
            .await
            .expect("spawn should succeed");

        // Give the background task a moment to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let handle = mgr.get_result(&id).await.expect("handle should exist");
        assert!(matches!(handle.status, SubagentStatus::Completed));
        assert_eq!(handle.result.as_deref(), Some("hello from subagent"));
        assert!(handle.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_spawn_fn_failed_task() {
        let mut mgr = SubagentManager::new(5, None);

        let id = mgr
            .spawn_fn(
                "failing task".to_string(),
                None,
                SubagentScope::default(),
                Box::pin(async { Err(anyhow::anyhow!("something went wrong")) }),
                None,
            )
            .await
            .expect("spawn should succeed");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let handle = mgr.get_result(&id).await.expect("handle should exist");
        match &handle.status {
            SubagentStatus::Failed(msg) => assert!(msg.contains("something went wrong")),
            other => panic!("expected Failed, got {:?}", other),
        }
        assert!(handle.completed_at.is_some());
    }

    #[tokio::test]
    async fn test_spawn_fn_concurrent_limit_enforced() {
        let mut mgr = SubagentManager::new(2, None);

        // Spawn two long-running tasks
        let (tx1, rx1) = tokio::sync::oneshot::channel::<()>();
        let (tx2, rx2) = tokio::sync::oneshot::channel::<()>();

        let _id1 = mgr
            .spawn_fn(
                "task-1".to_string(),
                None,
                SubagentScope::default(),
                Box::pin(async move {
                    let _ = rx1.await;
                    Ok("done-1".to_string())
                }),
                None,
            )
            .await
            .expect("first spawn should succeed");

        let _id2 = mgr
            .spawn_fn(
                "task-2".to_string(),
                None,
                SubagentScope::default(),
                Box::pin(async move {
                    let _ = rx2.await;
                    Ok("done-2".to_string())
                }),
                None,
            )
            .await
            .expect("second spawn should succeed");

        // Third spawn should fail — limit is 2
        let result = mgr
            .spawn_fn(
                "task-3".to_string(),
                None,
                SubagentScope::default(),
                Box::pin(async { Ok("done-3".to_string()) }),
                None,
            )
            .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Concurrent subagent limit reached"));

        // Clean up: signal the tasks to finish
        let _ = tx1.send(());
        let _ = tx2.send(());
    }

    #[tokio::test]
    async fn test_spawn_fn_slot_freed_after_completion() {
        let mut mgr = SubagentManager::new(1, None);

        // Spawn one task that completes immediately
        let id1 = mgr
            .spawn_fn(
                "quick task".to_string(),
                None,
                SubagentScope::default(),
                Box::pin(async { Ok("done".to_string()) }),
                None,
            )
            .await
            .expect("spawn should succeed");

        // Wait for it to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        assert!(matches!(
            mgr.get_result(&id1).await.unwrap().status,
            SubagentStatus::Completed
        ));
        assert_eq!(mgr.active_count().await, 0);

        // Now we should be able to spawn another one
        let _id2 = mgr
            .spawn_fn(
                "second task".to_string(),
                None,
                SubagentScope::default(),
                Box::pin(async { Ok("also done".to_string()) }),
                None,
            )
            .await
            .expect("spawn should succeed after slot freed");
    }

    #[tokio::test]
    async fn test_list_includes_spawned_tasks() {
        let mut mgr = SubagentManager::new(5, None);

        let id1 = mgr
            .spawn_fn(
                "task-a".to_string(),
                None,
                SubagentScope::default(),
                Box::pin(async { Ok("a".to_string()) }),
                None,
            )
            .await
            .unwrap();

        let id2 = mgr
            .spawn_fn(
                "task-b".to_string(),
                None,
                SubagentScope::default(),
                Box::pin(async { Ok("b".to_string()) }),
                None,
            )
            .await
            .unwrap();

        // Wait for completion
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let list = mgr.list().await;
        assert_eq!(list.len(), 2);

        let ids: Vec<&str> = list.iter().map(|h| h.id.as_str()).collect();
        assert!(ids.contains(&id1.as_str()));
        assert!(ids.contains(&id2.as_str()));
    }
}
