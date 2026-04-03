//! Context builder — assembles the system prompt from bootstrap files, memory, skills.

use chrono::Local;
use std::path::{Path, PathBuf};
#[cfg(feature = "memory-index")]
use std::sync::Arc;

use crate::agent::memory::MemoryStore;
#[cfg(feature = "memory-index")]
use crate::agent::memory_backend::{FileSqliteMemoryBackend, MemoryBackend, MemoryContextOptions};
#[cfg(feature = "memory-index")]
use crate::config::Config;
use crate::agent::skills::{CompositeSkillProvider, SkillProvider};
use crate::config;
use crate::sandbox::types::ToolSandboxExecKind;

const BOOTSTRAP_FILES: &[&str] = &["AGENTS.md", "SOUL.md", "USER.md", "TOOLS.md", "IDENTITY.md"];

pub struct ContextBuilder {
    workspace: PathBuf,
    agent_id: String,
    memory: MemoryStore,
    #[cfg(feature = "memory-index")]
    full_config: Option<Arc<Config>>,
    skills: CompositeSkillProvider,
    tool_sandbox_exec_kind: Option<ToolSandboxExecKind>,
}

#[cfg(feature = "memory-index")]
impl ContextBuilder {
    /// `full_config`: when `Some`, memory uses file + SQLite hybrid retrieval per [`crate::agent::memory_backend`].
    pub fn new(
        workspace: &Path,
        agent_id: &str,
        skills_dir: &Path,
        tool_sandbox_exec_kind: Option<ToolSandboxExecKind>,
        full_config: Option<Arc<Config>>,
    ) -> Self {
        let agent_id = if agent_id.is_empty() {
            "main".to_string()
        } else {
            agent_id.to_string()
        };
        Self {
            workspace: config::normalize_workspace_path(workspace),
            memory: MemoryStore::new(&agent_id),
            full_config,
            skills: CompositeSkillProvider::default_with_fs(skills_dir),
            agent_id,
            tool_sandbox_exec_kind,
        }
    }
}

#[cfg(not(feature = "memory-index"))]
impl ContextBuilder {
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
}

impl ContextBuilder {
    /// Build the full system prompt (identity + bootstrap from workspace + memory + skills).
    pub fn build_system_prompt(&self) -> String {
        self.build_system_prompt_with_role_prompt(&self.load_bootstrap_files(), None)
    }

    /// Build the full system prompt using a pre-built role prompt instead of loading bootstrap from workspace.
    /// `memory_query`: optional text (e.g. current user message) to drive hybrid memory search.
    pub fn build_system_prompt_with_role_prompt(
        &self,
        role_prompt: &str,
        memory_query: Option<&str>,
    ) -> String {
        let mut parts = Vec::new();

        parts.push(self.identity_section());

        if !role_prompt.trim().is_empty() {
            parts.push(role_prompt.trim().to_string());
        }

        let mem = self.build_memory_section(memory_query);
        if !mem.is_empty() {
            parts.push(format!("# Memory\n\n{}", mem));
        }

        let skills = self.skills.build_skills_summary();
        let skills_section = if skills.is_empty() {
            "No skills are currently loaded. Skills are subdirectories containing SKILL.md under the config skills directory.".to_string()
        } else {
            skills
        };
        parts.push(format!("# Skills\n\n{}", skills_section));

        parts.join("\n\n---\n\n")
    }

    fn build_memory_section(&self, memory_query: Option<&str>) -> String {
        #[cfg(feature = "memory-index")]
        {
            if let Some(ref cfg) = self.full_config {
                let backend = FileSqliteMemoryBackend::new(Arc::clone(cfg));
                let q = memory_query.map(|s| s.chars().take(512).collect::<String>());
                let opts = MemoryContextOptions {
                    recent_days: cfg.memory.recent_days.max(1),
                    query_for_search: q,
                    search_limit: cfg.memory.search_limit.max(1) as usize,
                };
                return backend
                    .get_memory_context(&self.agent_id, &opts)
                    .unwrap_or_default();
            }
        }
        let _ = memory_query;
        self.memory.get_memory_context()
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
        #[cfg(feature = "memory-index")]
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, None, None);
        #[cfg(not(feature = "memory-index"))]
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
        #[cfg(feature = "memory-index")]
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, None, None);
        #[cfg(not(feature = "memory-index"))]
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, None);
        let prompt =
            ctx.build_system_prompt_with_role_prompt("You are a helpful tester.", None);
        assert!(prompt.contains("Synbot"));
        assert!(prompt.contains("You are a helpful tester."));
    }

    #[test]
    fn build_system_prompt_empty_agent_id_uses_main() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        #[cfg(feature = "memory-index")]
        let _ctx = ContextBuilder::new(dir.path(), "", &skills_dir, None, None);
        #[cfg(not(feature = "memory-index"))]
        let _ctx = ContextBuilder::new(dir.path(), "", &skills_dir, None);
    }

    #[test]
    fn identity_host_native_tool_sandbox_mentions_appcontainer_not_docker() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        #[cfg(feature = "memory-index")]
        let ctx = ContextBuilder::new(
            dir.path(),
            "main",
            &skills_dir,
            Some(ToolSandboxExecKind::HostNative),
            None,
        );
        #[cfg(not(feature = "memory-index"))]
        let ctx = ContextBuilder::new(
            dir.path(),
            "main",
            &skills_dir,
            Some(ToolSandboxExecKind::HostNative),
        );
        let id = ctx.build_system_prompt_with_role_prompt("", None);
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
        #[cfg(feature = "memory-index")]
        let ctx = ContextBuilder::new(
            dir.path(),
            "main",
            &skills_dir,
            Some(ToolSandboxExecKind::Docker),
            None,
        );
        #[cfg(not(feature = "memory-index"))]
        let ctx = ContextBuilder::new(
            dir.path(),
            "main",
            &skills_dir,
            Some(ToolSandboxExecKind::Docker),
        );
        let id = ctx.build_system_prompt_with_role_prompt("", None);
        assert!(id.contains("/workspace"), "docker prompt should mention /workspace: {}", id);
        assert!(id.contains("Docker"), "docker prompt should mention Docker: {}", id);
    }
}
