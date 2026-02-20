//! Context builder — assembles the system prompt from bootstrap files, memory, skills.

use chrono::Local;
use std::path::{Path, PathBuf};

use crate::agent::memory::MemoryStore;
use crate::agent::skills::SkillsLoader;

const BOOTSTRAP_FILES: &[&str] = &["AGENTS.md", "SOUL.md", "USER.md", "TOOLS.md", "IDENTITY.md"];

pub struct ContextBuilder {
    workspace: PathBuf,
    /// Used for memory path (~/.synbot/memory/{agent_id}) and later for hybrid search.
    #[allow(dead_code)]
    agent_id: String,
    memory: MemoryStore,
    skills: SkillsLoader,
    /// When true, exec runs in tool sandbox (Docker); workspace is mounted at /workspace. Used to add workspace hints in identity_section.
    tool_sandbox_enabled: bool,
}

impl ContextBuilder {
    /// `workspace`: where to load bootstrap files (AGENTS.md, SOUL.md, etc.) from.
    /// `agent_id`: which agent's memory to use (e.g. "main" or role name); stored at ~/.synbot/memory/{agent_id}.
    /// `skills_dir`: global skills root (e.g. `config::skills_dir()`), i.e. `~/.synbot/skills/`.
    /// `tool_sandbox_enabled`: when true, exec runs in tool sandbox and workspace is at /workspace in container; identity_section will add environment and workspace hints.
    pub fn new(
        workspace: &Path,
        agent_id: &str,
        skills_dir: &Path,
        tool_sandbox_enabled: bool,
    ) -> Self {
        let agent_id = if agent_id.is_empty() {
            "main".to_string()
        } else {
            agent_id.to_string()
        };
        Self {
            workspace: workspace.to_path_buf(),
            memory: MemoryStore::new(&agent_id),
            skills: SkillsLoader::new(skills_dir),
            agent_id,
            tool_sandbox_enabled,
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
        let in_app_sandbox = std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some();

        let env_section = if in_app_sandbox || self.tool_sandbox_enabled {
            let mut lines = vec!["## Environment".to_string()];
            if in_app_sandbox {
                lines.push(
                    "- The **main process** is running inside the **app sandbox** (restricted environment).".to_string(),
                );
            }
            if self.tool_sandbox_enabled {
                lines.push(
                    "- The **exec** tool runs inside the **tool sandbox** (Docker container). \
                     File read/write/list tools are not available; use **exec** to run shell commands. \
                     The workspace is mounted inside the container at a fixed path (see Workspace below).".to_string(),
                );
            }
            format!("{}\n\n", lines.join("\n"))
        } else {
            String::new()
        };

        let workspace_section = if self.tool_sandbox_enabled {
            format!(
                "## Workspace\n\
                 - **Workspace (host, for reference):** {ws}\n\
                 - **Workspace inside tool sandbox (use this for exec paths):** `/workspace`\n\n\
                 When using the **exec** tool, use paths under `/workspace` (e.g. `/workspace/README.md`, `cd /workspace`).\n"
            )
        } else {
            format!("## Workspace\n{ws}\n")
        };

        format!(
            "# Synbot 🐈\n\n\
             You are synbot assistant, a helpful AI assistant.\n\n\
             ## Current Time\n{now}\n\n\
             {env}\
             {workspace}"
        , env = env_section, workspace = workspace_section)
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
