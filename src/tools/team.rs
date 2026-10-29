//! Agent team tools: `run_team` (parallel subtasks, wait for all), `list_subagents`, `cancel_subagent`.
//!
//! `spawn` is fire-and-forget; `run_team` is the fan-out/fan-in shape the planner actually needs:
//! hand N independent slices of a job to N teammates, wait for every report, and hand the parent one
//! merged answer. Teammates share the workspace, session and task board, but not the conversation.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{Mutex, RwLock};

use crate::agent::subagent::{
    Completion, SubagentHandle, SubagentManager, SubagentScope, SubagentStatus,
};
use crate::tools::spawn::SpawnContext;
use crate::tools::DynTool;

/// Upper bound on one fan-out: more teammates cost more tokens than they save.
pub const MAX_TEAM_SIZE: usize = 8;
/// How much of each teammate's report is kept in the merged answer.
const MAX_REPORT_CHARS: usize = 4000;
/// How often a queued teammate re-checks for a free slot.
const SLOT_POLL: Duration = Duration::from_millis(100);

/// What a real (model-backed) teammate needs, copied out of [`SpawnContext`] so the read lock on the
/// shared context is not held for the whole fan-out.
struct Harness {
    model: Arc<dyn crate::rig_provider::SynbotCompletionModel>,
    tools: Arc<crate::tools::ToolRegistry>,
    agent_id: String,
    max_tokens: u32,
    temperature: f32,
    workspace: PathBuf,
}

#[derive(Debug, Deserialize)]
struct TeamTaskSpec {
    task: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    role: Option<String>,
}

impl TeamTaskSpec {
    fn label(&self, index: usize) -> String {
        let base = self
            .label
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.task.chars().take(30).collect::<String>());
        format!("{base} #{index}")
    }

    fn role(&self) -> Option<String> {
        self.role
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

#[derive(Debug, Deserialize)]
struct TeamArgs {
    tasks: Vec<TeamTaskSpec>,
    #[serde(default)]
    max_parallel: Option<usize>,
}

/// Conversation a spawn reports to, built from the args the executor injects.
fn scope_from_args(args: &Value) -> SubagentScope {
    SubagentScope {
        channel: args["_channel"].as_str().unwrap_or_default().to_string(),
        chat_id: args["_chat_id"].as_str().unwrap_or_default().to_string(),
        session_id: args["_session_id"].as_str().unwrap_or_default().to_string(),
    }
}

/// Wait until the manager has a free slot (instead of failing the whole fan-out).
async fn wait_for_slot(manager: &Arc<Mutex<SubagentManager>>) {
    loop {
        let free = {
            let mgr = manager.lock().await;
            mgr.active_count().await < mgr.max_concurrent()
        };
        if free {
            return;
        }
        tokio::time::sleep(SLOT_POLL).await;
    }
}

/// Spawn one teammate. `harness = None` spawns an immediately-completing stub (tests, or before the
/// spawn context is wired).
async fn spawn_one(
    manager: &Arc<Mutex<SubagentManager>>,
    harness: Option<&Harness>,
    scope: &SubagentScope,
    label: String,
    spec: &TeamTaskSpec,
    on_complete: Option<Completion>,
) -> Result<String> {
    let mut mgr = manager.lock().await;
    match harness {
        Some(h) => {
            // The turn's workspace (project-bound chats included), not the startup default.
            let workspace = crate::tools::context::current_allowed_roots()
                .unwrap_or_else(|| h.workspace.clone());
            mgr.spawn(
                label,
                spec.role(),
                scope.clone(),
                spec.task.clone(),
                h.model.clone(),
                workspace,
                h.tools.clone(),
                &h.agent_id,
                h.max_tokens,
                h.temperature,
                on_complete,
            )
            .await
        }
        None => {
            let text = format!("{}: {}", spec.label.as_deref().unwrap_or(""), spec.task);
            mgr.spawn_fn(
                label,
                spec.role(),
                scope.clone(),
                Box::pin(async move { Ok(text) }),
                on_complete,
            )
            .await
        }
    }
}

/// `run_team` — fan out several subtasks to teammates, wait for all of them, merge the reports.
pub struct RunTeamTool {
    pub manager: Arc<Mutex<SubagentManager>>,
    pub context: Arc<RwLock<Option<SpawnContext>>>,
}

impl RunTeamTool {
    fn harness(&self, ctx: &SpawnContext) -> Harness {
        Harness {
            model: ctx.model.clone(),
            tools: ctx.tools.clone(),
            agent_id: ctx.agent_id.clone(),
            max_tokens: ctx.max_tokens,
            temperature: ctx.temperature,
            workspace: ctx.workspace.clone(),
        }
    }
}

#[async_trait::async_trait]
impl DynTool for RunTeamTool {
    fn name(&self) -> &str {
        "run_team"
    }

    fn description(&self) -> &str {
        "Fan out independent subtasks to teammates and wait for all of them (one merged answer). \
         Use it when the job splits into parallel slices (scan several modules, compare several \
         options, verify several fixes); the parent stays the only writer of the plan. Each task \
         needs enough context on its own, because teammates do not see this conversation. Optional \
         `role` picks a role prompt for that teammate; `max_parallel` caps simultaneous teammates."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tasks": {
                    "type": "array",
                    "description": "Independent subtasks, at most 8",
                    "items": {
                        "type": "object",
                        "properties": {
                            "task": { "type": "string", "description": "Self-contained task description" },
                            "label": { "type": "string", "description": "Short label for the team panel" },
                            "role": { "type": "string", "description": "Optional role name whose prompt frames this teammate" }
                        },
                        "required": ["task"]
                    }
                },
                "max_parallel": { "type": "integer", "description": "Maximum teammates running at once (default: all of them)" }
            },
            "required": ["tasks"]
        })
    }

    async fn call(&self, args: Value) -> Result<String> {
        let parsed: TeamArgs = serde_json::from_value(args.clone())
            .map_err(|e| anyhow::anyhow!("invalid run_team arguments: {e}"))?;
        if parsed.tasks.is_empty() {
            anyhow::bail!("run_team needs at least one task");
        }
        if parsed.tasks.len() > MAX_TEAM_SIZE {
            anyhow::bail!(
                "run_team accepts at most {MAX_TEAM_SIZE} tasks (got {})",
                parsed.tasks.len()
            );
        }
        if parsed.tasks.iter().any(|t| t.task.trim().is_empty()) {
            anyhow::bail!("every run_team task needs a non-empty description");
        }

        let scope = scope_from_args(&args);
        let harness = self.context.read().await.as_ref().map(|c| self.harness(c));
        let parallel = parsed
            .max_parallel
            .unwrap_or(parsed.tasks.len())
            .clamp(1, parsed.tasks.len());

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(usize, Result<String>)>();
        let mut reports: Vec<Option<Result<String>>> =
            (0..parsed.tasks.len()).map(|_| None).collect();
        let mut pending: Vec<usize> = Vec::new();
        let mut queue: std::collections::VecDeque<usize> = (0..parsed.tasks.len()).collect();
        let mut labels: Vec<String> = Vec::with_capacity(parsed.tasks.len());

        for (index, spec) in parsed.tasks.iter().enumerate() {
            labels.push(spec.label(index + 1));
        }

        loop {
            while pending.len() < parallel {
                let Some(index) = queue.pop_front() else {
                    break;
                };
                wait_for_slot(&self.manager).await;
                let spec = &parsed.tasks[index];
                let sender = tx.clone();
                let on_complete: Completion = Box::new(move |_label, result| {
                    // The index — not the label — identifies the report: labels may repeat.
                    let _ = sender.send((index, result));
                });
                match spawn_one(
                    &self.manager,
                    harness.as_ref(),
                    &scope,
                    labels[index].clone(),
                    spec,
                    Some(on_complete),
                )
                .await
                {
                    Ok(_) => pending.push(index),
                    Err(e) => reports[index] = Some(Err(e)),
                }
            }

            if pending.is_empty() {
                break;
            }
            match rx.recv().await {
                Some((index, result)) => {
                    reports[index] = Some(result);
                    pending.retain(|i| *i != index);
                }
                // Every sender is gone: no callback will ever arrive.
                None => break,
            }
        }

        let mut completed = 0usize;
        let mut failed = 0usize;
        let mut body = String::new();
        for (index, report) in reports.into_iter().enumerate() {
            let spec = &parsed.tasks[index];
            let role = spec
                .role()
                .map(|r| format!(" · role: {r}"))
                .unwrap_or_default();
            match report {
                Some(Ok(text)) => {
                    completed += 1;
                    body.push_str(&format!(
                        "\n## {}. {} — completed{}\n\n{}\n",
                        index + 1,
                        labels[index],
                        role,
                        truncate_report(&text)
                    ));
                }
                Some(Err(e)) => {
                    failed += 1;
                    body.push_str(&format!(
                        "\n## {}. {} — failed{}\n\n{}\n",
                        index + 1,
                        labels[index],
                        role,
                        truncate_report(&format!("{e:#}"))
                    ));
                }
                None => {
                    failed += 1;
                    body.push_str(&format!(
                        "\n## {}. {} — no report{}\n\n(teammate produced no result)\n",
                        index + 1,
                        labels[index],
                        role
                    ));
                }
            }
        }

        Ok(format!(
            "Team run: {} teammates ({completed} completed, {failed} failed)\n{body}",
            parsed.tasks.len()
        ))
    }
}

fn truncate_report(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= MAX_REPORT_CHARS {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(MAX_REPORT_CHARS).collect();
    out.push_str("\n\n… (truncated)");
    out
}

/// `list_subagents` — status of every teammate (running + finished).
pub struct ListSubagentsTool {
    pub manager: Arc<Mutex<SubagentManager>>,
}

fn format_handle(handle: &SubagentHandle) -> String {
    let state = match &handle.status {
        SubagentStatus::Running => "running".to_string(),
        SubagentStatus::Completed => "completed".to_string(),
        SubagentStatus::Cancelled => "cancelled".to_string(),
        SubagentStatus::Failed(err) => format!("failed: {err}"),
    };
    let role = handle
        .role
        .as_ref()
        .map(|r| format!("  role={r}"))
        .unwrap_or_default();
    let elapsed = match handle.completed_at {
        Some(end) => (end - handle.created_at).num_seconds().max(0),
        None => (chrono::Utc::now() - handle.created_at)
            .num_seconds()
            .max(0),
    };
    let activity = handle
        .activity
        .as_ref()
        .map(|a| format!("  activity={a}"))
        .unwrap_or_default();
    let result = handle
        .result
        .as_ref()
        .map(|r| {
            let preview: String = r.chars().take(200).collect();
            format!("\n    result: {}", preview.replace('\n', " "))
        })
        .unwrap_or_default();
    format!(
        "- {} [{}] {}{}  {}s{}{}",
        handle.id, state, handle.label, role, elapsed, activity, result
    )
}

#[async_trait::async_trait]
impl DynTool for ListSubagentsTool {
    fn name(&self) -> &str {
        "list_subagents"
    }

    fn description(&self) -> &str {
        "List every subagent (running and finished) with label, role, status, elapsed time and the \
         first line of its result. Use it to check on `spawn`ed work before deciding to wait or cancel."
    }

    fn parameters_schema(&self) -> Value {
        json!({ "type": "object", "properties": {}, "additionalProperties": false })
    }

    fn read_only(&self) -> bool {
        true
    }

    async fn call(&self, _args: Value) -> Result<String> {
        let mgr = self.manager.lock().await;
        let handles = mgr.list().await;
        if handles.is_empty() {
            return Ok(
                "No subagents yet. Use spawn (background) or run_team (parallel, waits)."
                    .to_string(),
            );
        }
        let running = handles.iter().filter(|h| h.is_running()).count();
        let lines: Vec<String> = handles.iter().map(format_handle).collect();
        Ok(format!(
            "{running} running / {} total\n{}",
            handles.len(),
            lines.join("\n")
        ))
    }
}

/// `cancel_subagent` — stop one teammate (`id`) or all of them (`all: true`).
pub struct CancelSubagentTool {
    pub manager: Arc<Mutex<SubagentManager>>,
}

#[async_trait::async_trait]
impl DynTool for CancelSubagentTool {
    fn name(&self) -> &str {
        "cancel_subagent"
    }

    fn description(&self) -> &str {
        "Stop a running subagent by id, or every running subagent with all=true. Their work is \
         aborted at the next model round or tool boundary and reported as cancelled."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Subagent id from spawn / list_subagents" },
                "all": { "type": "boolean", "description": "Cancel every running subagent" }
            },
            "required": []
        })
    }

    async fn call(&self, args: Value) -> Result<String> {
        let all = args["all"].as_bool().unwrap_or(false);
        let id = args["id"].as_str().map(str::trim).filter(|s| !s.is_empty());
        if !all && id.is_none() {
            anyhow::bail!("cancel_subagent needs an `id` (or all=true)");
        }

        let mut mgr = self.manager.lock().await;
        if all {
            let cancelled = mgr.cancel_all().await;
            return Ok(if cancelled == 0 {
                "No running subagents to cancel.".to_string()
            } else {
                format!("Cancellation requested for {cancelled} subagent(s).")
            });
        }
        let id = id.unwrap_or_default();
        if mgr.cancel(id).await {
            Ok(format!("Cancellation requested for subagent '{id}'."))
        } else {
            Ok(format!(
                "Subagent '{id}' is not running (unknown id, or it already finished)."
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_manager(max_concurrent: usize) -> Arc<Mutex<SubagentManager>> {
        Arc::new(Mutex::new(SubagentManager::new(max_concurrent, None)))
    }

    fn no_ctx() -> Arc<RwLock<Option<SpawnContext>>> {
        Arc::new(RwLock::new(None))
    }

    #[tokio::test]
    async fn run_team_waits_for_every_teammate_and_merges_reports() {
        let tool = RunTeamTool {
            manager: stub_manager(4),
            context: no_ctx(),
        };
        let out = tool
            .call(json!({
                "tasks": [
                    { "task": "scan auth module", "label": "auth" },
                    { "task": "scan billing module" },
                    { "task": "scan storage module", "role": "reviewer" }
                ]
            }))
            .await
            .unwrap();

        assert!(
            out.contains("Team run: 3 teammates (3 completed, 0 failed)"),
            "out: {out}"
        );
        assert!(out.contains("## 1. auth"));
        assert!(out.contains("scan billing module"));
        assert!(out.contains("role: reviewer"));
        // Every teammate is accounted for in the manager, not just in the text.
        let mgr = tool.manager.lock().await;
        assert_eq!(mgr.list().await.len(), 3);
    }

    #[tokio::test]
    async fn run_team_rejects_empty_and_oversized_batches() {
        let tool = RunTeamTool {
            manager: stub_manager(4),
            context: no_ctx(),
        };
        let empty = tool.call(json!({ "tasks": [] })).await.unwrap_err();
        assert!(empty.to_string().contains("at least one task"));

        let tasks: Vec<Value> = (0..MAX_TEAM_SIZE + 1)
            .map(|i| json!({ "task": format!("t{i}") }))
            .collect();
        let too_many = tool.call(json!({ "tasks": tasks })).await.unwrap_err();
        assert!(too_many.to_string().contains("at most"));

        let blank = tool
            .call(json!({ "tasks": [{ "task": "   " }] }))
            .await
            .unwrap_err();
        assert!(blank.to_string().contains("non-empty description"));
    }

    #[tokio::test]
    async fn list_and_cancel_subagents_follow_the_manager_state() {
        let manager = stub_manager(2);
        let list = ListSubagentsTool {
            manager: Arc::clone(&manager),
        };
        assert!(list.call(json!({})).await.unwrap().contains("No subagents"));

        // A teammate that blocks until cancelled keeps the "running" state observable.
        let id = {
            let mut mgr = manager.lock().await;
            mgr.spawn_fn(
                "slow".to_string(),
                None,
                SubagentScope::default(),
                Box::pin(async {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    Ok("never".to_string())
                }),
                None,
            )
            .await
            .unwrap()
        };

        let running = list.call(json!({})).await.unwrap();
        assert!(running.contains("1 running / 1 total"), "list: {running}");
        assert!(running.contains("[running] slow"));

        let cancel = CancelSubagentTool {
            manager: Arc::clone(&manager),
        };
        let message = cancel.call(json!({ "id": id })).await.unwrap();
        assert!(message.contains("Cancellation requested"));

        // Cancellation is observed asynchronously; the status flips without waiting for the sleep.
        for _ in 0..50 {
            let handle = manager.lock().await.get_result(&id).await.unwrap();
            if matches!(handle.status, SubagentStatus::Cancelled) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("subagent was not marked cancelled");
    }

    #[tokio::test]
    async fn cancel_without_id_or_all_is_rejected() {
        let cancel = CancelSubagentTool {
            manager: stub_manager(2),
        };
        let err = cancel.call(json!({})).await.unwrap_err();
        assert!(err.to_string().contains("needs an `id`"));
        assert!(cancel
            .call(json!({ "all": true }))
            .await
            .unwrap()
            .contains("No running subagents"));
    }
}
