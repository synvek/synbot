//! Code index tools: `find_symbols` (read-only lookup) and `index_repo` (build / write the wiki).
//!
//! `find_symbols` answers "where is this defined?" in one call instead of a chain of greps, and it
//! builds the index lazily on first use so the agent never has to know about a build step. Building
//! writes only `.synbot/index/`, which is a cache rather than project content — that is why the
//! lookup tool stays `read_only` (and therefore runs concurrently with other reads).
//!
//! `index_repo` is the explicit half: refresh after a large change, or regenerate
//! `.synbot/repowiki/*.md` for a human-readable map.

use anyhow::Result;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::codeindex::{CodeIndex, Hit};
use crate::tools::context;
use crate::tools::DynTool;

/// Folder the tools operate on: the turn's workspace (project-bound chats included).
fn workspace(default: &Path) -> PathBuf {
    context::current_allowed_roots().unwrap_or_else(|| default.to_path_buf())
}

/// Load the index, building it first when this workspace has never been indexed.
fn load_or_build(root: &Path) -> Result<(CodeIndex, bool)> {
    let mut index = CodeIndex::load(root);
    if index.files.is_empty() {
        index.refresh(root)?;
        return Ok((index, true));
    }
    Ok((index, false))
}

fn format_hit(hit: &Hit) -> String {
    let qualified = match hit.container.as_deref() {
        Some(container) => format!("{container}::{}", hit.name),
        None => hit.name.clone(),
    };
    format!(
        "{}:{}  {} {}  —  {}",
        hit.path, hit.line, hit.kind, qualified, hit.signature
    )
}

pub struct FindSymbolsTool {
    pub workspace: PathBuf,
}

#[async_trait::async_trait]
impl DynTool for FindSymbolsTool {
    fn name(&self) -> &str {
        "find_symbols"
    }

    fn description(&self) -> &str {
        "Find where functions, types, classes and other named items are defined, across the whole workspace. Prefer this over grepping for a definition. Accepts a plain name (`run_turn`) or a scoped one (`AgentLoop::run_turn`). The index is built automatically the first time and refreshed incrementally afterwards."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Symbol name, optionally scoped as `Container::name`" },
                "limit": { "type": "integer", "description": "Maximum hits (default 20, max 50)", "default": 20 }
            },
            "required": ["query"]
        })
    }

    fn read_only(&self) -> bool {
        true
    }

    async fn call(&self, args: Value) -> Result<String> {
        let query = args["query"].as_str().unwrap_or("").trim();
        if query.is_empty() {
            anyhow::bail!("`query` is required");
        }
        let limit = args["limit"].as_u64().unwrap_or(20) as usize;
        let root = workspace(&self.workspace);
        let (index, built) = load_or_build(&root)?;

        let hits = index.search(query, limit);
        let mut out = String::new();
        if built {
            out.push_str(&format!(
                "(built index for {} files / {} symbols)\n",
                index.files.len(),
                index.symbol_count()
            ));
        }
        if hits.is_empty() {
            out.push_str(&format!(
                "No symbol matched \"{query}\". Try `search_text` for call sites or `search_files` for file names."
            ));
        } else {
            out.push_str(&format!("{} match(es) for \"{query}\":\n", hits.len()));
            for hit in &hits {
                out.push_str(&format!("{}\n", format_hit(hit)));
            }
        }
        Ok(out)
    }
}

pub struct IndexRepoTool {
    pub workspace: PathBuf,
}

#[async_trait::async_trait]
impl DynTool for IndexRepoTool {
    fn name(&self) -> &str {
        "index_repo"
    }

    fn description(&self) -> &str {
        "Rebuild the workspace symbol index (incremental: only changed files are re-scanned), or write the repo wiki to .synbot/repowiki. Use `refresh` after large edits or several new files; use `wiki` when the user asks for a repository overview."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["refresh", "wiki"],
                    "description": "refresh = update the index; wiki = also write .synbot/repowiki pages",
                    "default": "refresh"
                }
            }
        })
    }

    async fn call(&self, args: Value) -> Result<String> {
        let action = args["action"].as_str().unwrap_or("refresh");
        let root = workspace(&self.workspace);
        match action {
            "refresh" => {
                let mut index = CodeIndex::load(&root);
                let stats = index.refresh(&root)?;
                Ok(format!(
                    "Indexed {} files / {} symbols (parsed {}, reused {}, removed {}{}) → {}",
                    stats.files,
                    stats.symbols,
                    stats.parsed,
                    stats.reused,
                    stats.removed,
                    if stats.truncated { ", truncated" } else { "" },
                    stats.path
                ))
            }
            "wiki" => {
                let mut index = CodeIndex::load(&root);
                if index.files.is_empty() {
                    index.refresh(&root)?;
                }
                let report = index.write_wiki(&root)?;
                Ok(format!(
                    "Wrote {} page(s) for {} module(s) to {}{}",
                    report.files.len(),
                    report.modules,
                    report.dir,
                    if report.truncated {
                        " (module pages truncated)"
                    } else {
                        ""
                    }
                ))
            }
            other => anyhow::bail!("unknown action `{other}` (expected refresh or wiki)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, path: &str, body: &str) {
        let full = root.join(path);
        if let Some(dir) = full.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(full, body).unwrap();
    }

    #[tokio::test]
    async fn find_symbols_builds_lazily_and_reports_the_project_folder() {
        let global = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        write(
            project.path(),
            "src/agent.rs",
            "pub struct AgentLoop;\nimpl AgentLoop {\n    pub async fn run_turn(&self) {}\n}\n",
        );
        let tool = FindSymbolsTool {
            workspace: global.path().to_path_buf(),
        };
        let ctx = context::ToolContext {
            agent_id: "main".to_string(),
            workspace: project.path().to_path_buf(),
        };

        let out = context::scope(ctx, async {
            let first = tool.call(json!({ "query": "run_turn" })).await.unwrap();
            let second = tool
                .call(json!({ "query": "AgentLoop::run_turn" }))
                .await
                .unwrap();
            let missing = tool
                .call(json!({ "query": "nope_not_here" }))
                .await
                .unwrap();
            (first, second, missing)
        })
        .await;

        assert!(out.0.contains("built index for 1 files"), "{}", out.0);
        assert!(out.0.contains("src/agent.rs:3"), "{}", out.0);
        assert!(out.0.contains("AgentLoop::run_turn"), "{}", out.0);
        assert!(out.1.contains("1 match(es)"), "{}", out.1);
        assert!(out.2.contains("No symbol matched"), "{}", out.2);
        // The index landed in the project directory, not the tool's default workspace.
        assert!(CodeIndex::is_built(project.path()));
        assert!(!CodeIndex::is_built(global.path()));
    }

    #[tokio::test]
    async fn index_repo_refreshes_and_writes_the_wiki() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "src/lib.rs", "pub fn api() {}\n");
        let tool = IndexRepoTool {
            workspace: dir.path().to_path_buf(),
        };

        let refreshed = tool.call(json!({ "action": "refresh" })).await.unwrap();
        assert!(
            refreshed.contains("Indexed 1 files / 1 symbols"),
            "{refreshed}"
        );

        let wiki = tool.call(json!({ "action": "wiki" })).await.unwrap();
        assert!(wiki.contains("Wrote 2 page(s)"), "{wiki}");
        assert!(dir.path().join(".synbot/repowiki/index.md").exists());

        let err = tool.call(json!({ "action": "explode" })).await.unwrap_err();
        assert!(err.to_string().contains("unknown action"), "{err}");
    }
}
