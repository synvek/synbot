//! Spawn tool — spawns subagents via SubagentManager.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Value};
use tokio::sync::{Mutex, RwLock};

use tokio::sync::broadcast;

use crate::agent::subagent::{Completion, SubagentManager, SubagentScope};
use crate::bus::OutboundMessage;
use crate::rig_provider::SynbotCompletionModel;
use crate::tools::{DynTool, ToolRegistry};

/// Context required to run a real subagent (model + tools). When set, the spawn tool
/// uses `SubagentManager::spawn` so the subagent runs the task with the LLM and tools.
/// `outbound_tx` is used to send the subagent result to the user when the tool is
/// called with `_channel` and `_chat_id` (injected by the executor).
/// When unset (e.g. in tests), the tool spawns a no-op that returns immediately.
pub struct SpawnContext {
    pub model: Arc<dyn SynbotCompletionModel>,
    pub workspace: PathBuf,
    pub tools: Arc<ToolRegistry>,
    pub agent_id: String,
    /// Same as `mainAgent.maxTokens` — required for Anthropic-family API completions.
    pub max_tokens: u32,
    pub temperature: f32,
    pub outbound_tx: broadcast::Sender<OutboundMessage>,
}

/// Tool that spawns a background subagent to handle a task.
///
/// If [`SpawnContext`] is set (via the shared `context`), the subagent runs the task
/// with the LLM and tools (`SubagentManager::spawn`). Otherwise it spawns a no-op that
/// returns immediately (for tests or before context is set).
pub struct SpawnTool {
    pub manager: Arc<Mutex<SubagentManager>>,
    /// When Some, subagent runs the task with model and tools; when None, no-op.
    pub context: Arc<RwLock<Option<SpawnContext>>>,
}

#[async_trait::async_trait]
impl DynTool for SpawnTool {
    fn name(&self) -> &str {
        "spawn"
    }
    fn description(&self) -> &str {
        "Spawn one background subagent (a teammate) to handle a task while you keep working. \
         Optional `role` picks a role prompt from the roles directory. Use `run_team` to fan out \
         several tasks in parallel and wait for all of them, `list_subagents` to check progress and \
         `cancel_subagent` to stop one."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "Task description, including enough context for a teammate that does not see this conversation" },
                "label": { "type": "string", "description": "Human-readable label" },
                "role": { "type": "string", "description": "Optional role name (subdirectory of the roles directory) whose prompt frames the subagent" }
            },
            "required": ["task"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let task = args["task"].as_str().unwrap_or("").to_string();
        let label = args["label"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| task.chars().take(30).collect());
        let role = args["role"]
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let channel = args["_channel"].as_str().map(String::from);
        let chat_id = args["_chat_id"].as_str().map(String::from);
        // Events and approvals for this subagent go back to the conversation that spawned it.
        let scope = SubagentScope {
            channel: channel.clone().unwrap_or_default(),
            chat_id: chat_id.clone().unwrap_or_default(),
            session_id: args["_session_id"].as_str().unwrap_or_default().to_string(),
        };
        let ctx_guard = self.context.read().await;
        let id = if let Some(ref ctx) = *ctx_guard {
            // Real subagent: run task with model and tools in the background.
            let on_complete: Option<Completion> = if let (Some(ch), Some(cid)) =
                (channel.clone(), chat_id.clone())
            {
                let tx = ctx.outbound_tx.clone();
                Some(Box::new(move |completed_label, result| {
                    let (status, body) = match result {
                        Ok(out) => ("Result", out),
                        Err(e) => ("Failed", e.to_string()),
                    };
                    let content = format!("[Subagent {}] {}:\n\n{}", completed_label, status, body);
                    let _ = tx.send(OutboundMessage::chat(ch, cid, content, vec![], None));
                }))
            } else {
                None
            };
            let mut mgr = self.manager.lock().await;
            // The turn's workspace (project-bound chats included), not the startup default.
            let workspace = crate::tools::context::current_allowed_roots()
                .unwrap_or_else(|| ctx.workspace.clone());
            mgr.spawn(
                label.clone(),
                role.clone(),
                scope.clone(),
                task,
                ctx.model.clone(),
                workspace,
                ctx.tools.clone(),
                &ctx.agent_id,
                ctx.max_tokens,
                ctx.temperature,
                on_complete,
            )
            .await?
        } else {
            // No context (e.g. tests): spawn a no-op that returns immediately.
            let task_description = task.clone();
            let mut mgr = self.manager.lock().await;
            mgr.spawn_fn(
                label.clone(),
                role.clone(),
                scope.clone(),
                Box::pin(
                    async move { Ok(format!("Subagent completed task: {}", task_description)) },
                ),
                None,
            )
            .await?
        };

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

    fn no_ctx() -> Arc<RwLock<Option<SpawnContext>>> {
        Arc::new(RwLock::new(None))
    }

    #[tokio::test]
    async fn test_spawn_tool_creates_subagent() {
        let mgr = Arc::new(Mutex::new(SubagentManager::new(5, None)));
        let tool = SpawnTool {
            manager: Arc::clone(&mgr),
            context: no_ctx(),
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
        let mgr = Arc::new(Mutex::new(SubagentManager::new(5, None)));
        let tool = SpawnTool {
            manager: Arc::clone(&mgr),
            context: no_ctx(),
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
        let mgr = Arc::new(Mutex::new(SubagentManager::new(1, None)));
        let tool = SpawnTool {
            manager: Arc::clone(&mgr),
            context: no_ctx(),
        };

        // Spawn a long-running task to fill the slot
        {
            let mut mgr_lock = mgr.lock().await;
            let (_tx, rx) = tokio::sync::oneshot::channel::<()>();
            mgr_lock
                .spawn_fn(
                    "blocker".to_string(),
                    None,
                    SubagentScope::default(),
                    Box::pin(async move {
                        let _ = rx.await;
                        Ok("done".to_string())
                    }),
                    None,
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
        let mgr = Arc::new(Mutex::new(SubagentManager::new(5, None)));
        let tool = SpawnTool {
            manager: Arc::clone(&mgr),
            context: no_ctx(),
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
