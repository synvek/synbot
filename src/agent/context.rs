//! Context builder — assembles the system prompt from bootstrap files, memory, skills.

use chrono::Local;
use std::path::{Path, PathBuf};

use crate::agent::memory::MemoryStore;
use crate::agent::skills::{CompositeSkillProvider, SkillProvider};

const BOOTSTRAP_FILES: &[&str] = &["AGENTS.md", "SOUL.md", "USER.md", "TOOLS.md", "IDENTITY.md"];

pub struct ContextBuilder {
    workspace: PathBuf,
    /// Used for memory path (~/.synbot/memory/{agent_id}) and later for hybrid search.
    #[allow(dead_code)]
    agent_id: String,
    memory: MemoryStore,
    skills: CompositeSkillProvider,
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
            skills: CompositeSkillProvider::default_with_fs(skills_dir),
            agent_id,
            tool_sandbox_enabled,
        }
    }

    /// Build the full system prompt (identity + bootstrap from workspace + memory + skills).
    pub fn build_system_prompt(&self) -> String {
        self.build_system_prompt_with_role_prompt(&self.load_bootstrap_files())
    }

    /// Build the full system prompt using a pre-built role prompt instead of loading bootstrap from workspace.
    /// Used when all agents get their behavior from a role (identity + role_prompt + memory + skills).
    pub fn build_system_prompt_with_role_prompt(&self, role_prompt: &str) -> String {
        let mut parts = Vec::new();

        parts.push(self.identity_section());

        if !role_prompt.trim().is_empty() {
            parts.push(role_prompt.trim().to_string());
        }

        let mem = self.memory.get_memory_context();
        if !mem.is_empty() {
            parts.push(format!("# Memory\n\n{}", mem));
        }

        // Always include Skills section so the model knows about the skill list (from ~/.synbot/skills or config root).
        let skills = self.skills.build_skills_summary();
        let skills_section = if skills.is_empty() {
            "No skills are currently loaded. Skills are subdirectories containing SKILL.md under the config skills directory.".to_string()
        } else {
            skills
        };
        parts.push(format!("# Skills\n\n{}", skills_section));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_system_prompt_contains_synbot_and_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, false);
        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("Synbot"));
        assert!(prompt.contains("Workspace"));
        assert!(prompt.contains("# Skills"));
    }

    #[test]
    fn build_system_prompt_with_role_prompt_includes_role() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, false);
        let prompt = ctx.build_system_prompt_with_role_prompt("You are a helpful tester.");
        assert!(prompt.contains("Synbot"));
        assert!(prompt.contains("You are a helpful tester."));
    }

    #[test]
    fn build_system_prompt_empty_agent_id_uses_main() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        let _ctx = ContextBuilder::new(dir.path(), "", &skills_dir, false);
        // Just ensure it doesn't panic; agent_id "main" is used for memory path
    }
}
