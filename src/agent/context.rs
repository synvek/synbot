//! Context builder — assembles the system prompt from bootstrap files, memory, skills.

use chrono::Local;
use std::path::{Path, PathBuf};

use crate::agent::memory::MemoryStore;
use crate::agent::skills::SkillsLoader;

const BOOTSTRAP_FILES: &[&str] = &["AGENTS.md", "SOUL.md", "USER.md", "TOOLS.md", "IDENTITY.md"];

pub struct ContextBuilder {
    workspace: PathBuf,
    memory: MemoryStore,
    skills: SkillsLoader,
}

impl ContextBuilder {
    pub fn new(workspace: &Path) -> Self {
        Self {
            workspace: workspace.to_path_buf(),
            memory: MemoryStore::new(workspace),
            skills: SkillsLoader::new(workspace),
        }
    }

    /// Build the full system prompt.
    pub fn build_system_prompt(&self) -> String {
        let mut parts = Vec::new();

        // Identity header
        parts.push(self.identity_section());

        // Bootstrap files
        let bootstrap = self.load_bootstrap_files();
        if !bootstrap.is_empty() {
            parts.push(bootstrap);
        }

        // Memory
        let mem = self.memory.get_memory_context();
        if !mem.is_empty() {
            parts.push(format!("# Memory\n\n{}", mem));
        }

        // Skills summary
        let skills = self.skills.build_skills_summary();
        if !skills.is_empty() {
            parts.push(format!("# Skills\n\n{}", skills));
        }

        parts.join("\n\n---\n\n")
    }

    fn identity_section(&self) -> String {
        let now = Local::now().format("%Y-%m-%d %H:%M (%A)");
        let ws = self.workspace.display();
        format!(
            "# synbot 🐈\n\n\
             You are synvek assistant, a helpful AI assistant.\n\n\
             ## Current Time\n{now}\n\n\
             ## Workspace\n{ws}\n"
        )
    }

    fn load_bootstrap_files(&self) -> String {
        let mut parts = Vec::new();
        for name in BOOTSTRAP_FILES {
            let path = self.workspace.join(name);
            if let Ok(content) = std::fs::read_to_string(&path) {
                parts.push(content);
            }
        }
        parts.join("\n\n")
    }
}
