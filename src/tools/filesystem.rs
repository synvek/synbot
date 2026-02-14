//! File-system tools: read_file, write_file, edit_file, list_dir.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tracing::info;
use crate::tools::DynTool;

fn resolve_path(path: &str, workspace: &Path, restrict: bool) -> anyhow::Result<PathBuf> {
    let p = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        workspace.join(path)
    };
    let canonical = p.canonicalize().unwrap_or_else(|_| p.clone());
    if restrict && !canonical.starts_with(workspace) {
        anyhow::bail!("Path {} is outside workspace", path);
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
        info!("Listing {}", path.display());
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
