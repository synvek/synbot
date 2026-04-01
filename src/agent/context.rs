//! Context builder — assembles the system prompt from bootstrap files, memory, skills.

use chrono::Local;
use std::path::{Path, PathBuf};

use crate::agent::memory::MemoryStore;
use crate::agent::skills::{CompositeSkillProvider, SkillProvider};
use crate::config;
use crate::sandbox::types::ToolSandboxExecKind;

const BOOTSTRAP_FILES: &[&str] = &["AGENTS.md", "SOUL.md", "USER.md", "TOOLS.md", "IDENTITY.md"];

pub struct ContextBuilder {
    workspace: PathBuf,
    /// Used for memory path (~/.synbot/memory/{agent_id}) and later for hybrid search.
    #[allow(dead_code)]
    agent_id: String,
    memory: MemoryStore,
    skills: CompositeSkillProvider,
    /// When set, exec runs in the tool sandbox; Docker vs host-native changes prompt text (paths, `/workspace`, etc.).
    tool_sandbox_exec_kind: Option<ToolSandboxExecKind>,
}

impl ContextBuilder {
    /// `workspace`: where to load bootstrap files (AGENTS.md, SOUL.md, etc.) from.
    /// `agent_id`: which agent's memory to use (e.g. "main" or role name); stored at ~/.synbot/memory/{agent_id}.
    /// `skills_dir`: global skills root (e.g. `config::skills_dir()`), i.e. `~/.synbot/skills/`.
    /// `tool_sandbox_exec_kind`: `Some(Docker)` vs `Some(HostNative)` vs `None` (no tool sandbox) controls environment/workspace hints in the system prompt.
    pub fn new(
        workspace: &Path,
        agent_id: &str,
        skills_dir: &Path,
        tool_sandbox_exec_kind: Option<ToolSandboxExecKind>,
    ) -> Self {
        let agent_id = if agent_id.is_empty() {
            "main".to_string()
        } else {
            agent_id.to_string()
        };
        Self {
            workspace: config::normalize_workspace_path(workspace),
            memory: MemoryStore::new(&agent_id),
            skills: CompositeSkillProvider::default_with_fs(skills_dir),
            agent_id,
            tool_sandbox_exec_kind,
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

        let tool_sandbox_active = self.tool_sandbox_exec_kind.is_some();

        let env_section = if in_app_sandbox || tool_sandbox_active {
            let mut lines = vec!["## Environment".to_string()];
            if in_app_sandbox {
                lines.push(
                    "- The **main process** is running inside the **app sandbox** (restricted environment).".to_string(),
                );
            }
            if let Some(kind) = self.tool_sandbox_exec_kind {
                let exec_hint = match kind {
                    ToolSandboxExecKind::Docker => "- The **exec** tool runs inside the **tool sandbox** (Docker). \
                     **read_file** / **write_file** / **list_dir** (and related file tools) run in the **main process** on the host; \
                     they are scoped to the workspace per config (`tools.exec.restrictToWorkspace`). \
                     For **exec** in the container, use paths under `/workspace` (see Workspace below)."
                        .to_string(),
                    ToolSandboxExecKind::HostNative => "- The **exec** tool runs inside the **tool sandbox** (host-native isolation: \
                     Windows AppContainer, macOS Seatbelt, etc.). \
                     File tools run in the **main process** with the same host workspace path (scoped per `tools.exec.restrictToWorkspace`). \
                     Use **exec** for shell inside the sandbox; use **real host paths** — there is no `/workspace` bind mount (see Workspace below)."
                        .to_string(),
                };
                lines.push(exec_hint);
            }
            format!("{}\n\n", lines.join("\n"))
        } else {
            String::new()
        };

        let workspace_section = match self.tool_sandbox_exec_kind {
            Some(ToolSandboxExecKind::Docker) => format!(
                "## Workspace\n\
                 - **Workspace (host, for reference):** {ws}\n\
                 - **Workspace inside tool sandbox (use this for exec paths):** `/workspace`\n\n\
                 When using the **exec** tool, use paths under `/workspace` (e.g. `/workspace/README.md`, `cd /workspace`).\n"
            ),
            Some(ToolSandboxExecKind::HostNative) => format!(
                "## Workspace\n\
                 **Workspace path (use for exec; same path as on the host):** {ws}\n\n\
                 Use normal paths for this OS in **exec** (e.g. Windows: `dir`, `type file.txt`). \
                 Do not use `/workspace` unless you are on a Docker-based tool sandbox.\n"
            ),
            None => format!("## Workspace\n{ws}\n"),
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
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, None);
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
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, None);
        let prompt = ctx.build_system_prompt_with_role_prompt("You are a helpful tester.");
        assert!(prompt.contains("Synbot"));
        assert!(prompt.contains("You are a helpful tester."));
    }

    #[test]
    fn build_system_prompt_empty_agent_id_uses_main() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        let _ctx = ContextBuilder::new(dir.path(), "", &skills_dir, None);
        // Just ensure it doesn't panic; agent_id "main" is used for memory path
    }

    #[test]
    fn identity_host_native_tool_sandbox_mentions_appcontainer_not_docker() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        let ctx = ContextBuilder::new(
            dir.path(),
            "main",
            &skills_dir,
            Some(ToolSandboxExecKind::HostNative),
        );
        let id = ctx.build_system_prompt_with_role_prompt("");
        assert!(
            id.contains("Windows AppContainer") && id.contains("host-native isolation"),
            "expected host-native sandbox hint, got: {}",
            id
        );
        assert!(
            !id.contains("**tool sandbox** (Docker)"),
            "host-native must not describe exec as Docker-only: {}",
            id
        );
    }

    #[test]
    fn identity_docker_tool_sandbox_mentions_workspace_mount() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        let ctx = ContextBuilder::new(
            dir.path(),
            "main",
            &skills_dir,
            Some(ToolSandboxExecKind::Docker),
        );
        let id = ctx.build_system_prompt_with_role_prompt("");
        assert!(id.contains("/workspace"), "docker prompt should mention /workspace: {}", id);
        assert!(id.contains("Docker"), "docker prompt should mention Docker: {}", id);
    }
}
