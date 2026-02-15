//! Remember tool — save content to long-term memory (MEMORY.md) or daily note (memory/YYYY-MM-DD.md).

use anyhow::Result;
use chrono::Local;
use serde_json::{json, Value};

use crate::config;
use crate::tools::context;
use crate::tools::DynTool;

/// Tool that appends text to the agent's long-term memory (MEMORY.md) or today's daily note (memory/YYYY-MM-DD.md).
/// When running in a tool context (e.g. a role), writes only to that agent's memory; ignores any agent_id in args.
pub struct RememberTool {
    /// Default agent id when no context is set (e.g. "main").
    agent_id: String,
}

impl RememberTool {
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
        }
    }

    fn append_long_term_for(agent_id: &str, content: &str) -> Result<()> {
        let dir = config::memory_dir(agent_id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("MEMORY.md");
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let new_content = if existing.trim().is_empty() {
            content.trim().to_string()
        } else {
            format!("{}\n\n{}", existing.trim_end(), content.trim())
        };
        std::fs::write(path, new_content)?;
        Ok(())
    }

    fn append_daily_note_for(agent_id: &str, content: &str) -> Result<()> {
        let dir = config::memory_dir(agent_id);
        let notes_dir = dir.join("memory");
        std::fs::create_dir_all(&notes_dir)?;
        let today = Local::now().format("%Y-%m-%d");
        let path = notes_dir.join(format!("{}.md", today));
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let new_content = if existing.trim().is_empty() {
            content.trim().to_string()
        } else {
            format!("{}\n\n{}", existing.trim_end(), content.trim())
        };
        std::fs::write(path, new_content)?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl DynTool for RememberTool {
    fn name(&self) -> &str {
        "remember"
    }

    fn description(&self) -> &str {
        "Save a fact or note to memory. Use 'content' for the text and optionally 'daily' to choose where: (1) daily=false or omit: save to long-term memory (MEMORY.md), e.g. when user says '记住…' or 'remember that'. (2) daily=true: save to today's daily note (memory/YYYY-MM-DD.md), e.g. when user says '记一下今天的…' or '今天做了…' or wants a dated log."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The fact or note to save. Write clearly and concisely."
                },
                "daily": {
                    "type": "boolean",
                    "description": "If true, save to today's daily note (memory/YYYY-MM-DD.md). If false or omitted, save to long-term memory (MEMORY.md)."
                }
            },
            "required": ["content"]
        })
    }

    async fn call(&self, args: Value) -> Result<String> {
        let agent_id = context::current_agent_id().unwrap_or_else(|| self.agent_id.clone());
        let content = args["content"].as_str().unwrap_or("").trim();
        if content.is_empty() {
            return Ok("No content to remember. Please provide 'content' with the fact to save.".to_string());
        }
        let daily = args["daily"].as_bool().unwrap_or(false);
        if daily {
            Self::append_daily_note_for(&agent_id, content)?;
            let today = Local::now().format("%Y-%m-%d");
            Ok(format!("已写入今日笔记（{}）：{}", today, content))
        } else {
            Self::append_long_term_for(&agent_id, content)?;
            Ok(format!("已写入长期记忆：{}", content))
        }
    }
}

// ---------------------------------------------------------------------------
// list_memory — list memory files so the model does not need to use shell (dir) for retrieval
// ---------------------------------------------------------------------------

/// Tool to list memory files (MEMORY.md and memory/YYYY-MM-DD.md) for the current agent only.
/// When running as a role, lists only that role's memory; args do not allow listing another agent's memory.
pub struct ListMemoryTool {
    /// Default agent id when no context is set (e.g. "main").
    agent_id: String,
}

impl ListMemoryTool {
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
        }
    }
}

#[async_trait::async_trait]
impl DynTool for ListMemoryTool {
    fn name(&self) -> &str {
        "list_memory"
    }

    fn description(&self) -> &str {
        "List memory files for this agent only: MEMORY.md (long-term) and memory/YYYY-MM-DD.md (daily notes). Use this to see what memory files exist; do not use exec/shell to run 'dir' on the memory directory."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(&self, _args: Value) -> Result<String> {
        let id = context::current_agent_id().unwrap_or_else(|| self.agent_id.clone());
        let dir = config::memory_dir(&id);
        let mut lines = Vec::new();

        let memory_md = dir.join("MEMORY.md");
        if memory_md.exists() {
            lines.push(format!("MEMORY.md (long-term)"));
        }
        let notes_dir = dir.join("memory");
        if notes_dir.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(&notes_dir)
                .map_err(|e| anyhow::anyhow!("read memory dir: {}", e))?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().extension().map_or(false, |x| x == "md")
                })
                .collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let name = e.file_name().to_string_lossy().into_owned();
                lines.push(format!("memory/{}", name));
            }
        }
        if lines.is_empty() {
            Ok(format!("Memory dir: {}. No MEMORY.md or memory/*.md yet.", dir.display()))
        } else {
            Ok(format!("Memory dir: {}\n\n{}", dir.display(), lines.join("\n")))
        }
    }
}
