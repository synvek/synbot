//! Remember tool — save content to long-term memory (MEMORY.md) or daily note (memory/YYYY-MM-DD.md).

use anyhow::Result;
use chrono::Local;
use serde_json::{json, Value};

use crate::config;
use crate::tools::DynTool;

/// Tool that appends text to the agent's long-term memory (MEMORY.md) or today's daily note (memory/YYYY-MM-DD.md).
pub struct RememberTool {
    /// Agent id whose memory to write to (e.g. "main").
    agent_id: String,
}

impl RememberTool {
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
        }
    }

    fn append_long_term(&self, content: &str) -> Result<()> {
        let dir = config::memory_dir(&self.agent_id);
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

    fn append_daily_note(&self, content: &str) -> Result<()> {
        let dir = config::memory_dir(&self.agent_id);
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
        let content = args["content"].as_str().unwrap_or("").trim();
        if content.is_empty() {
            return Ok("No content to remember. Please provide 'content' with the fact to save.".to_string());
        }
        let daily = args["daily"].as_bool().unwrap_or(false);
        if daily {
            self.append_daily_note(content)?;
            let today = Local::now().format("%Y-%m-%d");
            Ok(format!("已写入今日笔记（{}）：{}", today, content))
        } else {
            self.append_long_term(content)?;
            Ok(format!("已写入长期记忆：{}", content))
        }
    }
}
