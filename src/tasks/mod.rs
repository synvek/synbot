//! Task board — the agent's plan, persisted per workspace.
//!
//! The agent writes its todo list with the `plan_write` tool (a whole-list replace, like TodoWrite);
//! the same list is what the workbench panel shows and what a user can tick off from the UI. State
//! lives in `<workspace>/.synbot/plans/tasks.json`, so it travels with the project folder and
//! survives restarts — a plan that only existed in one conversation would be useless the next day.
//!
//! Writes are atomic (tmp file + rename): the panel polls this file while a turn is running.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Folder (relative to the workspace) that holds plan artifacts.
pub const PLANS_DIR: &str = ".synbot/plans";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl TaskStatus {
    /// Accept the spellings models actually emit (`in_progress`, `in-progress`, `done`, …).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw
            .trim()
            .to_ascii_lowercase()
            .replace([' ', '-'], "_")
            .as_str()
        {
            "pending" | "todo" | "open" | "not_started" => Some(Self::Pending),
            "in_progress" | "inprogress" | "doing" | "active" | "started" => Some(Self::InProgress),
            "completed" | "complete" | "done" | "finished" => Some(Self::Completed),
            "cancelled" | "canceled" | "dropped" | "abandoned" | "skipped" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(self, Self::Pending | Self::InProgress)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub status: TaskStatus,
    /// Optional detail: acceptance criteria, file to touch, blocker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One entry of a `plan_write` call.
#[derive(Debug, Clone, Deserialize)]
pub struct TaskInput {
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskBoard {
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

impl TaskBoard {
    /// `<workspace>/.synbot/plans/tasks.json`.
    pub fn path_for(workspace: &Path) -> PathBuf {
        workspace.join(PLANS_DIR).join("tasks.json")
    }

    /// Load the board; a missing or corrupt file is an empty board, never an error — a broken plan
    /// file must not take down the agent loop or the panel.
    pub fn load(workspace: &Path) -> Self {
        let path = Self::path_for(workspace);
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str::<TaskBoard>(&raw) {
            Ok(board) => board,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Ignoring corrupt task board");
                Self::default()
            }
        }
    }

    /// Atomic save: write a sibling temp file, then rename over the target.
    pub fn save(&self, workspace: &Path) -> Result<()> {
        let path = Self::path_for(workspace);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)
            .with_context(|| format!("failed to write {}", tmp.display()))?;
        std::fs::rename(&tmp, &path)
            .with_context(|| format!("failed to replace {}", path.display()))?;
        Ok(())
    }

    /// Replace the whole list. Ids stay stable when the caller repeats them (the UI highlights by
    /// id), otherwise they are derived from the existing ids, so a rewritten plan does not churn.
    pub fn replace(&mut self, inputs: Vec<TaskInput>) -> Result<()> {
        if inputs.is_empty() {
            anyhow::bail!("task list must not be empty (pass the full list, or use plan_list)");
        }
        let now = Utc::now();
        let mut next_id = self.next_id();
        let mut tasks = Vec::with_capacity(inputs.len());
        for input in inputs {
            let title = input.title.trim();
            if title.is_empty() {
                anyhow::bail!("every task needs a non-empty title");
            }
            let status = match input.status.as_deref() {
                Some(raw) => TaskStatus::parse(raw).with_context(|| {
                    format!(
                        "unknown task status '{raw}' (use pending/in_progress/completed/cancelled)"
                    )
                })?,
                None => TaskStatus::Pending,
            };
            // Reuse the previous timestamps when the same task is kept, so "created" means something.
            let previous = input
                .id
                .as_deref()
                .and_then(|id| self.tasks.iter().find(|t| t.id == id));
            let id = match input.id.clone() {
                Some(id) if !id.trim().is_empty() => id,
                _ => {
                    let id = format!("t{next_id}");
                    next_id += 1;
                    id
                }
            };
            tasks.push(Task {
                created_at: previous.map(|t| t.created_at).unwrap_or(now),
                updated_at: now,
                id,
                title: title.to_string(),
                status,
                notes: input.notes.filter(|n| !n.trim().is_empty()),
            });
        }
        self.tasks = tasks;
        self.updated_at = Some(now);
        Ok(())
    }

    /// Tick one task (UI / REST). Returns the updated task.
    pub fn set_status(&mut self, id: &str, status: TaskStatus) -> Result<Task> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .with_context(|| format!("task not found: {id}"))?;
        task.status = status;
        task.updated_at = Utc::now();
        self.updated_at = Some(Utc::now());
        Ok(task.clone())
    }

    pub fn open(&self) -> Vec<&Task> {
        self.tasks.iter().filter(|t| t.status.is_open()).collect()
    }

    /// Progress + one line per task, for tool output and prompts.
    pub fn summary(&self) -> String {
        if self.tasks.is_empty() {
            return "No tasks yet. Use plan_write to publish a plan.".to_string();
        }
        let done = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Completed)
            .count();
        let mut out = format!("{done}/{} done", self.tasks.len());
        for task in &self.tasks {
            let marker = match task.status {
                TaskStatus::Pending => "[ ]",
                TaskStatus::InProgress => "[~]",
                TaskStatus::Completed => "[x]",
                TaskStatus::Cancelled => "[-]",
            };
            out.push_str(&format!("\n{marker} {} — {}", task.id, task.title));
            if let Some(notes) = &task.notes {
                out.push_str(&format!(" ({notes})"));
            }
        }
        out
    }

    /// Steps for `AgentEvent::PlanUpdated`, so the UI can render without reading the file.
    pub fn to_event_steps(&self) -> Vec<crate::agent::events::PlanStep> {
        self.tasks
            .iter()
            .map(|t| crate::agent::events::PlanStep {
                id: t.id.clone(),
                title: t.title.clone(),
                status: match t.status {
                    TaskStatus::Pending => crate::agent::events::PlanStepStatus::Pending,
                    TaskStatus::InProgress => crate::agent::events::PlanStepStatus::InProgress,
                    TaskStatus::Completed => crate::agent::events::PlanStepStatus::Completed,
                    TaskStatus::Cancelled => crate::agent::events::PlanStepStatus::Cancelled,
                },
            })
            .collect()
    }

    fn next_id(&self) -> usize {
        self.tasks
            .iter()
            .filter_map(|t| t.id.strip_prefix('t')?.parse::<usize>().ok())
            .max()
            .map(|n| n + 1)
            .unwrap_or(1)
    }
}

/// Workspace the running turn is allowed to use; `None` outside a tool context.
pub fn current_workspace() -> Option<PathBuf> {
    crate::tools::context::current_allowed_roots()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(title: &str, status: Option<&str>) -> TaskInput {
        TaskInput {
            id: None,
            title: title.to_string(),
            status: status.map(str::to_string),
            notes: None,
        }
    }

    #[test]
    fn replace_assigns_ids_and_saves_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let mut board = TaskBoard::load(dir.path());
        board
            .replace(vec![
                input("read code", Some("in_progress")),
                input("write tests", None),
            ])
            .unwrap();
        board.save(dir.path()).unwrap();

        let reloaded = TaskBoard::load(dir.path());
        assert_eq!(reloaded.tasks.len(), 2);
        assert_eq!(reloaded.tasks[0].id, "t1");
        assert_eq!(reloaded.tasks[0].status, TaskStatus::InProgress);
        assert_eq!(reloaded.tasks[1].status, TaskStatus::Pending);
        assert!(TaskBoard::path_for(dir.path()).exists());
        // No leftover temp file.
        assert!(!TaskBoard::path_for(dir.path())
            .with_extension("json.tmp")
            .exists());
    }

    #[test]
    fn rewriting_keeps_ids_and_created_at() {
        let dir = tempfile::tempdir().unwrap();
        let mut board = TaskBoard::load(dir.path());
        board
            .replace(vec![input("one", None), input("two", None)])
            .unwrap();
        let created = board.tasks[0].created_at;

        board
            .replace(vec![
                TaskInput {
                    id: Some("t1".into()),
                    ..input("one", Some("completed"))
                },
                input("three", None),
            ])
            .unwrap();
        assert_eq!(board.tasks[0].id, "t1");
        assert_eq!(board.tasks[0].created_at, created);
        // The new task continues the id sequence instead of reusing `t2`.
        assert_eq!(board.tasks[1].id, "t3");
    }

    #[test]
    fn status_parsing_covers_model_spellings() {
        assert_eq!(
            TaskStatus::parse("in-progress"),
            Some(TaskStatus::InProgress)
        );
        assert_eq!(TaskStatus::parse("DONE"), Some(TaskStatus::Completed));
        assert_eq!(TaskStatus::parse("canceled"), Some(TaskStatus::Cancelled));
        assert_eq!(TaskStatus::parse("weird"), None);
    }

    #[test]
    fn invalid_input_is_rejected_without_touching_the_board() {
        let dir = tempfile::tempdir().unwrap();
        let mut board = TaskBoard::load(dir.path());
        board.replace(vec![input("one", None)]).unwrap();

        assert!(board.replace(vec![]).is_err());
        assert!(board.replace(vec![input("  ", None)]).is_err());
        assert!(board.replace(vec![input("x", Some("nope"))]).is_err());
        assert_eq!(board.tasks.len(), 1);
    }

    #[test]
    fn set_status_and_summary_track_progress() {
        let dir = tempfile::tempdir().unwrap();
        let mut board = TaskBoard::load(dir.path());
        board
            .replace(vec![input("a", None), input("b", None)])
            .unwrap();
        board.set_status("t1", TaskStatus::Completed).unwrap();
        assert!(board.set_status("nope", TaskStatus::Completed).is_err());

        let summary = board.summary();
        assert!(summary.starts_with("1/2 done"), "summary: {summary}");
        assert!(summary.contains("[x] t1 — a"));
        assert!(summary.contains("[ ] t2 — b"));
        assert_eq!(board.open().len(), 1);
    }

    #[test]
    fn corrupt_board_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = TaskBoard::path_for(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();
        assert!(TaskBoard::load(dir.path()).tasks.is_empty());
    }

    #[test]
    fn event_steps_map_every_status() {
        let dir = tempfile::tempdir().unwrap();
        let mut board = TaskBoard::load(dir.path());
        board
            .replace(vec![
                input("a", Some("pending")),
                input("b", Some("in_progress")),
                input("c", Some("completed")),
                input("d", Some("cancelled")),
            ])
            .unwrap();
        let steps = board.to_event_steps();
        assert_eq!(steps.len(), 4);
        assert_eq!(
            steps[3].status,
            crate::agent::events::PlanStepStatus::Cancelled
        );
    }
}
