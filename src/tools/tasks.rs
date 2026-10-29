//! Plan tools: `plan_write` (publish/replace the task list) and `plan_list` (read it back).
//!
//! A whole-list replace is deliberate: models are good at rewriting a short checklist and bad at
//! juggling per-item CRUD, and the UI wants one coherent snapshot per update. The board lives in
//! `<workspace>/.synbot/plans/tasks.json` — the project folder when the chat is bound to one — so
//! the workbench panel and the next session see the same plan.

use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::tasks::{TaskBoard, TaskInput};
use crate::tools::context;
use crate::tools::DynTool;

/// Folder the tools operate on: the turn's workspace (project-bound chats included).
fn workspace(default: &Path) -> PathBuf {
    context::current_allowed_roots().unwrap_or_else(|| default.to_path_buf())
}

#[derive(Debug, Deserialize)]
struct WriteArgs {
    tasks: Vec<TaskInput>,
}

pub struct PlanWriteTool {
    pub workspace: PathBuf,
}

#[async_trait::async_trait]
impl DynTool for PlanWriteTool {
    fn name(&self) -> &str {
        "plan_write"
    }

    fn description(&self) -> &str {
        "Publish or update your task plan. Pass the COMPLETE list every time (it replaces the previous plan); keep ids of tasks you carry over, and mark finished tasks as completed instead of removing them. Use it for any task with more than two steps, and keep it current as you work."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tasks": {
                    "type": "array",
                    "description": "The full task list, in execution order",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string", "description": "Keep the id returned by a previous plan_write to preserve continuity" },
                            "title": { "type": "string", "description": "Short imperative title" },
                            "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "cancelled"], "description": "Defaults to pending" },
                            "notes": { "type": "string", "description": "Optional detail: acceptance criteria, files, blockers" }
                        },
                        "required": ["title"]
                    }
                }
            },
            "required": ["tasks"]
        })
    }

    async fn call(&self, args: Value) -> Result<String> {
        let root = workspace(&self.workspace);
        let parsed: WriteArgs = serde_json::from_value(args)
            .map_err(|e| anyhow::anyhow!("invalid plan_write arguments: {e}"))?;
        let mut board = TaskBoard::load(&root);
        board.replace(parsed.tasks)?;
        board.save(&root)?;
        Ok(format!(
            "Plan saved to {} ({} tasks)\n{}",
            TaskBoard::path_for(&root).display(),
            board.tasks.len(),
            board.summary()
        ))
    }
}

pub struct PlanListTool {
    pub workspace: PathBuf,
}

#[async_trait::async_trait]
impl DynTool for PlanListTool {
    fn name(&self) -> &str {
        "plan_list"
    }

    fn description(&self) -> &str {
        "Read the current task plan for this workspace (ids, titles, statuses, notes)."
    }

    fn parameters_schema(&self) -> Value {
        json!({ "type": "object", "properties": {}, "additionalProperties": false })
    }

    fn read_only(&self) -> bool {
        true
    }

    async fn call(&self, _args: Value) -> Result<String> {
        let root = workspace(&self.workspace);
        Ok(TaskBoard::load(&root).summary())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn plan_write_then_list_roundtrips_inside_the_tool_scope() {
        let global = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let write = PlanWriteTool {
            workspace: global.path().to_path_buf(),
        };
        let list = PlanListTool {
            workspace: global.path().to_path_buf(),
        };
        let ctx = context::ToolContext {
            agent_id: "main".to_string(),
            workspace: project.path().to_path_buf(),
        };

        let out = context::scope(ctx.clone(), async {
            let out = write
                .call(json!({
                    "tasks": [
                        { "title": "read code", "status": "in_progress" },
                        { "title": "write tests" }
                    ]
                }))
                .await
                .unwrap();
            let listed = list.call(json!({})).await.unwrap();
            (out, listed)
        })
        .await;

        assert!(out.0.contains("2 tasks"), "out: {}", out.0);
        assert!(out.1.contains("[~] t1 — read code"));
        // The plan is stored in the project folder, not in the tool's default workspace.
        assert!(TaskBoard::path_for(project.path()).exists());
        assert!(!TaskBoard::path_for(global.path()).exists());
    }

    #[tokio::test]
    async fn plan_write_rejects_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        let write = PlanWriteTool {
            workspace: dir.path().to_path_buf(),
        };
        let err = write.call(json!({ "tasks": [] })).await.unwrap_err();
        assert!(err.to_string().contains("must not be empty"));
    }

    #[tokio::test]
    async fn plan_list_is_read_only_and_empty_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let list = PlanListTool {
            workspace: dir.path().to_path_buf(),
        };
        assert!(list.read_only());
        assert!(list.call(json!({})).await.unwrap().contains("No tasks yet"));
    }
}
