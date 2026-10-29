//! Read-only git tools: `git_status`, `git_diff`, `git_log`.
//!
//! All three are marked read-only, so a round of git inspection runs concurrently with file reads
//! and never asks for approval. Anything that *writes* to the repository (`add`, `commit`,
//! `checkout`, `branch -d`, …) goes through `exec`, where the approval flow, the command policy and
//! the audit log already live.
//!
//! The repository root is the turn's workspace (`ToolContext::workspace`), i.e. the project folder
//! when a chat is bound to one — same resolution as the filesystem tools.

use anyhow::Result;
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::tools::context;
use crate::tools::DynTool;

/// Repository the current turn is working in.
fn repo_root(workspace: &std::path::Path) -> PathBuf {
    context::current_allowed_roots().unwrap_or_else(|| workspace.to_path_buf())
}

pub struct GitStatusTool {
    pub workspace: PathBuf,
}

#[async_trait::async_trait]
impl DynTool for GitStatusTool {
    fn name(&self) -> &str {
        "git_status"
    }

    fn description(&self) -> &str {
        "Show the git status of the current workspace: branch, ahead/behind, and every staged, unstaged or untracked file. Use it before deciding which files to review or commit."
    }

    fn parameters_schema(&self) -> Value {
        json!({ "type": "object", "properties": {}, "additionalProperties": false })
    }

    fn read_only(&self) -> bool {
        true
    }

    async fn call(&self, _args: Value) -> Result<String> {
        let root = repo_root(&self.workspace);
        let status = crate::git::status(&root)?;
        if !status.is_repo {
            return Ok(format!("{} is not a git repository", root.display()));
        }
        let mut out = status.summary();
        if let Some(head) = &status.head {
            out.push_str(&format!("\nHEAD {head}"));
        }
        for file in &status.files {
            out.push_str(&format!(
                "\n{:<3} {}{}",
                file.status,
                if file.staged { "[staged] " } else { "" },
                file.path
            ));
        }
        Ok(out)
    }
}

pub struct GitDiffTool {
    pub workspace: PathBuf,
    pub max_lines: usize,
}

#[async_trait::async_trait]
impl DynTool for GitDiffTool {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn description(&self) -> &str {
        "Show the git diff of the current workspace (unified patch). Pass `path` to narrow it to one file and `staged: true` to see the index instead of the worktree."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Optional path relative to the workspace" },
                "staged": { "type": "boolean", "description": "Diff the staged/index changes instead of the working tree (default false)" }
            },
            "additionalProperties": false
        })
    }

    fn read_only(&self) -> bool {
        true
    }

    async fn call(&self, args: Value) -> Result<String> {
        let root = repo_root(&self.workspace);
        let path = args.get("path").and_then(|v| v.as_str()).map(str::trim);
        let path = path.filter(|p| !p.is_empty());
        let staged = args
            .get("staged")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        crate::git::diff(&root, path, staged, self.max_lines)
    }
}

pub struct GitLogTool {
    pub workspace: PathBuf,
}

#[async_trait::async_trait]
impl DynTool for GitLogTool {
    fn name(&self) -> &str {
        "git_log"
    }

    fn description(&self) -> &str {
        "Show recent commits of the current workspace (hash, author, date, subject), newest first. Pass `path` to see only the commits that touched one file."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "description": "How many commits to show (default 10, max 200)" },
                "path": { "type": "string", "description": "Optional path relative to the workspace" }
            },
            "additionalProperties": false
        })
    }

    fn read_only(&self) -> bool {
        true
    }

    async fn call(&self, args: Value) -> Result<String> {
        let root = repo_root(&self.workspace);
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(10);
        let path = args.get("path").and_then(|v| v.as_str()).map(str::trim);
        let path = path.filter(|p| !p.is_empty());
        let commits = crate::git::log(&root, limit, path)?;
        if commits.is_empty() {
            return Ok("(no commits)".to_string());
        }
        Ok(commits
            .iter()
            .map(|c| format!("{} {} {} — {}", c.hash, c.date, c.author, c.subject))
            .collect::<Vec<_>>()
            .join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tool scope wins over the workspace the tool was built with (project-bound chat).
    #[tokio::test]
    async fn git_status_reports_non_repository_in_the_tool_scope() {
        let global = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let tool = GitStatusTool {
            workspace: global.path().to_path_buf(),
        };
        let ctx = context::ToolContext {
            agent_id: "main".to_string(),
            workspace: project.path().to_path_buf(),
        };
        let out = context::scope(ctx, async { tool.call(json!({})).await })
            .await
            .unwrap();
        assert!(out.contains("not a git repository"));
        assert!(!out.contains("main"));
    }

    #[tokio::test]
    async fn git_diff_without_repository_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let tool = GitDiffTool {
            workspace: dir.path().to_path_buf(),
            max_lines: 20,
        };
        let err = tool.call(json!({})).await.unwrap_err();
        assert!(err.to_string().contains("not a git repository"));
    }
}
