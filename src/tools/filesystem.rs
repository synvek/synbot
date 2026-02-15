//! File-system tools: read_file, write_file, edit_file, list_dir.
//!
//! When a role runs, paths are restricted to that agent's workspace and memory dir
//! via the tool execution context (~/.synbot/memory/{agent_id} and workspace/roles/{role}).

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use crate::tools::context;
use crate::tools::DynTool;

/// Resolve path and enforce scope: when restrict is true, path must be under
/// the current agent's allowed roots (workspace and memory_dir from context, or default workspace).
fn resolve_path(path: &str, workspace: &Path, restrict: bool) -> anyhow::Result<PathBuf> {
    let p = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        workspace.join(path)
    };
    let canonical = p.canonicalize().unwrap_or_else(|_| p.clone());

    let allowed = if let Some((ctx_workspace, ctx_memory)) = context::current_allowed_roots() {
        let ws_canon = ctx_workspace.canonicalize().unwrap_or_else(|_| ctx_workspace.clone());
        let mem_canon = ctx_memory.canonicalize().unwrap_or_else(|_| ctx_memory.clone());
        if canonical.starts_with(&ws_canon) || canonical.starts_with(&mem_canon) {
            true
        } else {
            warn!(
                path = %path,
                resolved = %canonical.display(),
                allowed_workspace = %ws_canon.display(),
                allowed_memory = %mem_canon.display(),
                "Path is outside current agent scope (workspace or memory); access denied"
            );
            false
        }
    } else if restrict {
        let ws_canon = workspace.canonicalize().unwrap_or_else(|_| workspace.to_path_buf());
        canonical.starts_with(&ws_canon)
    } else {
        true
    };
    if !allowed {
        anyhow::bail!(
            "Path {} is outside current agent scope (allowed: this agent's workspace and memory only)",
            path
        );
    }
    Ok(p)
}

// ---- ReadFile ----

pub struct ReadFileTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

#[async_trait::async_trait]
impl DynTool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str { "Read the contents of a file." }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let path = resolve_path(args["path"].as_str().unwrap_or(""), &self.workspace, self.restrict)?;
        info!(path = %path.display(), "read_file");
        Ok(tokio::fs::read_to_string(&path).await?)
    }
}

// ---- WriteFile ----

pub struct WriteFileTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

#[async_trait::async_trait]
impl DynTool for WriteFileTool {
    fn name(&self) -> &str { "write_file" }
    fn description(&self) -> &str { "Write content to a file (creates dirs if needed)." }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]})
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let path = resolve_path(args["path"].as_str().unwrap_or(""), &self.workspace, self.restrict)?;
        let content = args["content"].as_str().unwrap_or("");
        info!(path = %path.display(), len = content.len(), "write_file");
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, content).await?;
        Ok(format!("Wrote {} bytes to {}", content.len(), path.display()))
    }
}

// ---- EditFile ----

pub struct EditFileTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

#[async_trait::async_trait]
impl DynTool for EditFileTool {
    fn name(&self) -> &str { "edit_file" }
    fn description(&self) -> &str { "Edit a file by replacing specific text." }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string"},"old_text":{"type":"string"},"new_text":{"type":"string"}},"required":["path","old_text","new_text"]})
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let path = resolve_path(args["path"].as_str().unwrap_or(""), &self.workspace, self.restrict)?;
        let old = args["old_text"].as_str().unwrap_or("");
        let new = args["new_text"].as_str().unwrap_or("");
        info!(path = %path.display(), "edit_file");
        let content = tokio::fs::read_to_string(&path).await?;
        if !content.contains(old) {
            anyhow::bail!("old_text not found in {}", path.display());
        }
        tokio::fs::write(&path, content.replacen(old, new, 1)).await?;
        Ok(format!("Edited {}", path.display()))
    }
}

// ---- ListDir ----

pub struct ListDirTool {
    pub workspace: PathBuf,
    pub restrict: bool,
}

#[async_trait::async_trait]
impl DynTool for ListDirTool {
    fn name(&self) -> &str { "list_dir" }
    fn description(&self) -> &str { "List contents of a directory." }
    fn parameters_schema(&self) -> Value {
        json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})
    }
    async fn call(&self, args: Value) -> anyhow::Result<String> {
        let path = resolve_path(args["path"].as_str().unwrap_or("."), &self.workspace, self.restrict)?;
        info!(path = %path.display(), "list_dir");
        let mut entries = tokio::fs::read_dir(&path).await?;
        let mut lines = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let ft = entry.file_type().await?;
            let prefix = if ft.is_dir() { "d" } else { "-" };
            lines.push(format!("{} {}", prefix, entry.file_name().to_string_lossy()));
        }
        Ok(lines.join("\n"))
    }
}
