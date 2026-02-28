//! Tool execution context — current agent's scope for path isolation.
//!
//! When a role (e.g. `dev`) runs, filesystem tools must only access that role's
//! workspace. Memory is accessed via dedicated remember/list_memory tools.
//! This module provides a task-local context so that tools can restrict access
//! without changing the tool trait.

use std::path::PathBuf;

tokio::task_local! {
    /// Current agent's tool context: agent_id and allowed workspace.
    /// Set by the agent loop before running the completion loop for main or a role.
    pub static TOOL_CONTEXT: ToolContext;
}

/// Scope for one agent's tool execution. Filesystem paths must stay under `workspace`.
#[derive(Clone)]
pub struct ToolContext {
    pub agent_id: String,
    pub workspace: PathBuf,
}

impl ToolContext {
    /// Run a future with this context set. Tools executed inside will see this agent's scope.
    pub async fn scope<F, R>(self, f: F) -> R
    where
        F: std::future::Future<Output = R>,
    {
        TOOL_CONTEXT.scope(self, f).await
    }
}

/// Execute a future with the given tool context (for current agent isolation).
pub async fn scope<F, R>(ctx: ToolContext, f: F) -> R
where
    F: std::future::Future<Output = R>,
{
    ctx.scope(f).await
}

/// Try to get the current agent id. Returns None if not in a tool context (e.g. tests).
pub fn current_agent_id() -> Option<String> {
    TOOL_CONTEXT.try_with(|c| c.agent_id.clone()).ok()
}

/// Try to get the current agent's allowed workspace root for path checks.
/// Returns None if not in context.
pub fn current_allowed_roots() -> Option<PathBuf> {
    TOOL_CONTEXT.try_with(|c| c.workspace.clone()).ok()
}
