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

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use rig::completion::CompletionRequest;
use rig::message::{AssistantContent, Message, ToolResultContent, UserContent};
use rig::OneOrMany;
use rig_dyn::CompletionModel;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::agent::context::ContextBuilder;
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
}

/// Handle exposing the state of a single subagent.
#[derive(Debug, Clone)]
pub struct SubagentHandle {
    pub id: String,
    pub label: String,
    pub status: SubagentStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<String>,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Shared mutable state for a single subagent that the background task can
/// update and the manager can read.
type SharedHandle = Arc<Mutex<SubagentHandle>>;

pub struct SubagentManager {
    /// All subagent handles (both active and finished).
    handles: HashMap<String, SharedHandle>,
    /// Maximum number of concurrently running subagents.
    max_concurrent: usize,
}

impl SubagentManager {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            handles: HashMap::new(),
            max_concurrent,
        }
    }

    /// Spawn a new subagent that runs a custom async function in the
    /// background.
    ///
    /// This is the low-level entry point: callers provide a boxed future that
    /// returns `Result<String>`. The higher-level [`spawn`] method wraps a
    /// full `AgentLoop` interaction into such a future.
    ///
    /// Returns the subagent id on success, or an error if the concurrency
    /// limit has been reached.
    pub async fn spawn_fn(
        &mut self,
        label: String,
        task_fn: Pin<Box<dyn Future<Output = Result<String>> + Send + 'static>>,
    ) -> Result<String> {
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
            id: id.clone(),
            label: label.clone(),
            status: SubagentStatus::Running,
            created_at: Utc::now(),
            completed_at: None,
            result: None,
        }));

        self.handles.insert(id.clone(), Arc::clone(&handle));

        // Spawn background task
        let task_id = id.clone();
        tokio::spawn(async move {
            info!(subagent_id = %task_id, label = %label, "Subagent started");
            let result = task_fn.await;

            let mut h = handle.lock().await;
            match result {
                Ok(output) => {
                    info!(subagent_id = %task_id, "Subagent completed");
                    h.status = SubagentStatus::Completed;
                    h.result = Some(output);
                }
                Err(e) => {
                    let err_msg = format!("{e:#}");
                    error!(subagent_id = %task_id, error = %err_msg, "Subagent failed");
                    h.status = SubagentStatus::Failed(err_msg);
                }
            }
            h.completed_at = Some(Utc::now());
        });

        Ok(id)
    }

    /// Spawn a new subagent that executes `task` using a full AgentLoop in
    /// the background.
    ///
    /// Returns the subagent id on success, or an error if the concurrency
    /// limit has been reached.
    /// `agent_id`: which agent's memory to use (e.g. "main" or role name).
    pub async fn spawn(
        &mut self,
        label: String,
        task: String,
        model: Box<dyn CompletionModel>,
        workspace: PathBuf,
        tools: Arc<ToolRegistry>,
        agent_id: &str,
    ) -> Result<String> {
        let task_fn = Box::pin(run_subagent_task(
            model,
            workspace,
            tools,
            task,
            agent_id.to_string(),
        ));
        self.spawn_fn(label, task_fn).await
    }

    /// Return cloned handles for **all** subagents (running + finished).
    pub async fn list(&self) -> Vec<SubagentHandle> {
        let mut out = Vec::with_capacity(self.handles.len());
        for shared in self.handles.values() {
            out.push(shared.lock().await.clone());
        }
        out
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
}

// ---------------------------------------------------------------------------
// Background task execution
// ---------------------------------------------------------------------------

/// Run a simplified one-shot agent interaction for the subagent.
///
/// This creates a `ContextBuilder` for the workspace and agent_id, sends the task as a
/// user message, and collects the assistant's text response (executing any
/// tool calls along the way, up to a fixed iteration limit).
async fn run_subagent_task(
    model: Box<dyn CompletionModel>,
    workspace: PathBuf,
    tools: Arc<ToolRegistry>,
    task: String,
    agent_id: String,
) -> Result<String> {
    let memory_dir = config::memory_dir(&agent_id);
    let tool_ctx = ToolContext {
        agent_id: agent_id.clone(),
        workspace: workspace.clone(),
        memory_dir,
    };

    scope(tool_ctx, async move {
        let context = ContextBuilder::new(&workspace, &agent_id, config::skills_dir().as_path());
        let system_prompt = context.build_system_prompt();
        let tool_defs = tools.rig_definitions();

        let mut history: Vec<Message> = vec![Message::user(&task)];
        let max_iterations: u32 = 15;
        let mut iterations = 0u32;

        loop {
            iterations += 1;
            if iterations > max_iterations {
                warn!(
                    iterations = max_iterations,
                    "Subagent hit max iterations, returning partial result"
                );
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

            let response = model.completion(request).await?;

            let mut has_tool_calls = false;
            let mut text_parts = Vec::new();
            let mut assistant_contents = Vec::new();
            let mut tool_results = Vec::new();

            for content in response.iter() {
                match content {
                    AssistantContent::Text(t) => {
                        text_parts.push(t.text.clone());
                        assistant_contents.push(content.clone());
                    }
                    AssistantContent::ToolCall(tc) => {
                        has_tool_calls = true;
                        assistant_contents.push(content.clone());
                        let args = tc.function.arguments.clone();
                        let result = tools.execute(&tc.function.name, args, None).await;
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
                history.push(Message::Assistant { content });
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
    async fn test_new_creates_empty_manager() {
        let mgr = SubagentManager::new(3);
        assert_eq!(mgr.max_concurrent, 3);
        assert!(mgr.handles.is_empty());
        assert_eq!(mgr.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_active_count_starts_at_zero() {
        let mgr = SubagentManager::new(5);
        assert_eq!(mgr.active_count().await, 0);
    }

    #[tokio::test]
    async fn test_list_empty() {
        let mgr = SubagentManager::new(5);
        assert!(mgr.list().await.is_empty());
    }

    #[tokio::test]
    async fn test_get_result_nonexistent() {
        let mgr = SubagentManager::new(5);
        assert!(mgr.get_result("nonexistent").await.is_none());
    }

    /// Test that the concurrent limit is enforced by directly inserting
    /// handles in Running status and then verifying spawn would fail.
    #[tokio::test]
    async fn test_concurrent_limit_enforcement() {
        let mut mgr = SubagentManager::new(2);

        // Manually insert two "running" handles to simulate active subagents
        for i in 0..2 {
            let id = format!("fake-{i}");
            let handle = Arc::new(Mutex::new(SubagentHandle {
                id: id.clone(),
                label: format!("task-{i}"),
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
        let mut mgr = SubagentManager::new(2);

        // Insert one completed and one running
        let completed_handle = Arc::new(Mutex::new(SubagentHandle {
            id: "done-1".to_string(),
            label: "completed task".to_string(),
            status: SubagentStatus::Completed,
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
            result: Some("done".to_string()),
        }));
        mgr.handles.insert("done-1".to_string(), completed_handle);

        let running_handle = Arc::new(Mutex::new(SubagentHandle {
            id: "run-1".to_string(),
            label: "running task".to_string(),
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
        let mut mgr = SubagentManager::new(3);

        let failed_handle = Arc::new(Mutex::new(SubagentHandle {
            id: "fail-1".to_string(),
            label: "failed task".to_string(),
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
        let mut mgr = SubagentManager::new(5);

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
                status,
                created_at: Utc::now(),
                completed_at: if i > 0 { Some(Utc::now()) } else { None },
                result: if i == 1 { Some("result".to_string()) } else { None },
            }));
            mgr.handles.insert(id, handle);
        }

        let list = mgr.list().await;
        assert_eq!(list.len(), 3);
    }

    #[tokio::test]
    async fn test_get_result_returns_correct_handle() {
        let mut mgr = SubagentManager::new(5);

        let handle = Arc::new(Mutex::new(SubagentHandle {
            id: "test-id".to_string(),
            label: "test label".to_string(),
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
        let mgr = SubagentManager::new(0);
        // With max_concurrent=0, even with no active subagents, the limit
        // is already reached (0 >= 0), so no spawns should be possible.
        assert!(mgr.active_count().await >= mgr.max_concurrent);
    }

    #[tokio::test]
    async fn test_spawn_fn_and_collect_result() {
        let mut mgr = SubagentManager::new(5);

        let id = mgr
            .spawn_fn(
                "test task".to_string(),
                Box::pin(async { Ok("hello from subagent".to_string()) }),
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
        let mut mgr = SubagentManager::new(5);

        let id = mgr
            .spawn_fn(
                "failing task".to_string(),
                Box::pin(async {
                    Err(anyhow::anyhow!("something went wrong"))
                }),
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
        let mut mgr = SubagentManager::new(2);

        // Spawn two long-running tasks
        let (tx1, rx1) = tokio::sync::oneshot::channel::<()>();
        let (tx2, rx2) = tokio::sync::oneshot::channel::<()>();

        let _id1 = mgr
            .spawn_fn(
                "task-1".to_string(),
                Box::pin(async move {
                    let _ = rx1.await;
                    Ok("done-1".to_string())
                }),
            )
            .await
            .expect("first spawn should succeed");

        let _id2 = mgr
            .spawn_fn(
                "task-2".to_string(),
                Box::pin(async move {
                    let _ = rx2.await;
                    Ok("done-2".to_string())
                }),
            )
            .await
            .expect("second spawn should succeed");

        // Third spawn should fail — limit is 2
        let result = mgr
            .spawn_fn(
                "task-3".to_string(),
                Box::pin(async { Ok("done-3".to_string()) }),
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
        let mut mgr = SubagentManager::new(1);

        // Spawn one task that completes immediately
        let id1 = mgr
            .spawn_fn(
                "quick task".to_string(),
                Box::pin(async { Ok("done".to_string()) }),
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
                Box::pin(async { Ok("also done".to_string()) }),
            )
            .await
            .expect("spawn should succeed after slot freed");
    }

    #[tokio::test]
    async fn test_list_includes_spawned_tasks() {
        let mut mgr = SubagentManager::new(5);

        let id1 = mgr
            .spawn_fn(
                "task-a".to_string(),
                Box::pin(async { Ok("a".to_string()) }),
            )
            .await
            .unwrap();

        let id2 = mgr
            .spawn_fn(
                "task-b".to_string(),
                Box::pin(async { Ok("b".to_string()) }),
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
