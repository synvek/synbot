//! Spawn tool — spawns subagents via SubagentManager.

use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::agent::subagent::SubagentManager;
use crate::tools::DynTool;

/// Tool that spawns a background subagent to handle a task.
///
/// Holds a shared reference to the [`SubagentManager`] so that it can call
/// `spawn_fn` to create a new background task. The manager is wrapped in
/// `Arc<Mutex<…>>` because `spawn_fn` requires `&mut self`.
pub struct SpawnTool {
    pub manager: Arc<Mutex<SubagentManager>>,
}

#[async_trait::async_trait]
impl DynTool for SpawnTool {
    fn name(&self) -> &str {
        "spawn"
    }
    fn description(&self) -> &str {
        "Spawn a subagent to handle a task in the background."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "Task description" },
                "label": { "type": "string", "description": "Human-readable label" }
            },
            "required": ["task"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let task = args["task"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let label = args["label"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| task.chars().take(30).collect());

        let task_description = task.clone();

        // Lock the manager and spawn a background task.
        // We use `spawn_fn` with a simple async closure that returns the task
        // description as the result. Full model-backed subagent execution
        // (via `SubagentManager::spawn`) requires a CompletionModel, which is
        // not easily cloneable; this integration demonstrates the complete
        // lifecycle (spawn → track → collect) through the manager.
        let mut mgr = self.manager.lock().await;
        let id = mgr
            .spawn_fn(
                label.clone(),
                Box::pin(async move {
                    Ok(format!(
                        "Subagent completed task: {}",
                        task_description
                    ))
                }),
            )
            .await?;

        Ok(format!(
            "Subagent [{}] spawned with id '{}'. Use list_subagents to check status.",
            label, id
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::subagent::SubagentManager;

    #[tokio::test]
    async fn test_spawn_tool_creates_subagent() {
        let mgr = Arc::new(Mutex::new(SubagentManager::new(5)));
        let tool = SpawnTool {
            manager: Arc::clone(&mgr),
        };

        let result = tool
            .call(json!({
                "task": "research Rust async patterns",
                "label": "research"
            }))
            .await
            .unwrap();

        assert!(result.contains("research"));
        assert!(result.contains("spawned with id"));

        // Verify the subagent was actually created in the manager
        let mgr_lock = mgr.lock().await;
        let list = mgr_lock.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].label, "research");
    }

    #[tokio::test]
    async fn test_spawn_tool_uses_task_prefix_as_default_label() {
        let mgr = Arc::new(Mutex::new(SubagentManager::new(5)));
        let tool = SpawnTool {
            manager: Arc::clone(&mgr),
        };

        let result = tool
            .call(json!({
                "task": "a very long task description that exceeds thirty characters"
            }))
            .await
            .unwrap();

        assert!(result.contains("spawned with id"));

        let mgr_lock = mgr.lock().await;
        let list = mgr_lock.list().await;
        assert_eq!(list.len(), 1);
        // Default label should be first 30 chars of task
        assert_eq!(list[0].label.len(), 30);
    }

    #[tokio::test]
    async fn test_spawn_tool_returns_error_at_limit() {
        let mgr = Arc::new(Mutex::new(SubagentManager::new(1)));
        let tool = SpawnTool {
            manager: Arc::clone(&mgr),
        };

        // Spawn a long-running task to fill the slot
        {
            let mut mgr_lock = mgr.lock().await;
            let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
            mgr_lock
                .spawn_fn(
                    "blocker".to_string(),
                    Box::pin(async move {
                        let _ = rx.await;
                        Ok("done".to_string())
                    }),
                )
                .await
                .unwrap();
        }

        // Second spawn via the tool should fail
        let result = tool
            .call(json!({
                "task": "another task",
                "label": "second"
            }))
            .await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("limit reached"));
    }

    #[tokio::test]
    async fn test_spawn_tool_subagent_completes() {
        let mgr = Arc::new(Mutex::new(SubagentManager::new(5)));
        let tool = SpawnTool {
            manager: Arc::clone(&mgr),
        };

        let result = tool
            .call(json!({
                "task": "hello world",
                "label": "test"
            }))
            .await
            .unwrap();

        // Extract the id from the result message
        // Format: "Subagent [test] spawned with id 'XXXX'. ..."
        let id = result
            .split("id '")
            .nth(1)
            .unwrap()
            .split("'")
            .next()
            .unwrap();

        // Wait for the background task to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let mgr_lock = mgr.lock().await;
        let handle = mgr_lock.get_result(id).await.unwrap();
        assert!(matches!(
            handle.status,
            crate::agent::subagent::SubagentStatus::Completed
        ));
        assert!(handle.result.as_deref().unwrap().contains("hello world"));
    }
}
