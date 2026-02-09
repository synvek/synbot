//! Memory system — daily notes + long-term MEMORY.md.

use chrono::Local;
use std::path::{Path, PathBuf};

pub struct MemoryStore {
    memory_dir: PathBuf,
}

impl MemoryStore {
    pub fn new(workspace: &Path) -> Self {
        let memory_dir = workspace.join("memory");
        std::fs::create_dir_all(&memory_dir).ok();
        Self { memory_dir }
    }

    pub fn memory_file(&self) -> PathBuf {
        self.memory_dir.join("MEMORY.md")
    }

    fn today_file(&self) -> PathBuf {
        self.memory_dir.join(format!("{}.md", Local::now().format("%Y-%m-%d")))
    }

    pub fn read_long_term(&self) -> String {
        std::fs::read_to_string(self.memory_file()).unwrap_or_default()
    }

    pub fn read_today(&self) -> String {
        std::fs::read_to_string(self.today_file()).unwrap_or_default()
    }

    pub fn get_recent_memories(&self, days: u32) -> String {
        let today = Local::now().date_naive();
        let mut parts = Vec::new();
        for i in 0..days {
            let date = today - chrono::Duration::days(i as i64);
            let path = self.memory_dir.join(format!("{}.md", date.format("%Y-%m-%d")));
            if let Ok(content) = std::fs::read_to_string(&path) {
                parts.push(content);
            }
        }
        parts.join("\n\n---\n\n")
    }

    /// Build the memory context section for the system prompt.
    pub fn get_memory_context(&self) -> String {
        let mut parts = Vec::new();
        let lt = self.read_long_term();
        if !lt.is_empty() {
            parts.push(format!("## Long-term Memory\n\n{}", lt));
        }
        let today = self.read_today();
        if !today.is_empty() {
            parts.push(format!("## Today's Notes\n\n{}", today));
        }
        parts.join("\n\n")
    }
}
