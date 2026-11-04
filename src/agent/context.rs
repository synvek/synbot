//! Context builder — assembles the system prompt from bootstrap files, memory, skills.

use chrono::Local;
use std::path::{Path, PathBuf};
#[cfg(feature = "memory-index")]
use std::sync::Arc;

use crate::agent::memory::MemoryStore;
#[cfg(feature = "memory-index")]
use crate::agent::memory_backend::{FileSqliteMemoryBackend, MemoryBackend, MemoryContextOptions};
use crate::agent::skills::{CompositeSkillProvider, SkillProvider};
use crate::config;
#[cfg(feature = "memory-index")]
use crate::config::Config;
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
    /// Human name of the project bound to this conversation, if any. `None` means the default
    /// workspace is in use, which is what the model has to say when the user asks where it works.
    workspace_label: Option<String>,
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
            workspace_label: None,
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
            workspace_label: None,
        }
    }
}

impl ContextBuilder {
    /// Name the project bound to this conversation so the prompt can say where the agent works;
    /// `None` keeps the default-workspace wording.
    pub fn with_workspace_label(mut self, label: Option<String>) -> Self {
        self.workspace_label = label;
        self
    }

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

        // Planning: the task board is user-visible, so keep it current instead of "thinking" silently.
        parts.push(
            "# Planning\n\nFor any task that needs more than two steps, publish the plan with \
             `plan_write` before touching files, then rewrite the whole list (keeping ids) as steps \
             start and finish. The user sees this list as the workbench task board."
                .to_string(),
        );
        let board = crate::tasks::TaskBoard::load(&self.workspace);
        if !board.tasks.is_empty() {
            parts.push(format!("# Current plan\n\n{}", board.summary()));
        }

        // Repo map: only the small sidecar is read, so this stays cheap on every turn.
        if let Some(map) = crate::codeindex::repo_map(&self.workspace) {
            parts.push(format!("# Repository map\n\n{map}"));
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

        let workspace_section = self.workspace_section();

        format!(
            "# Synbot 🐈\n\n\
             You are synbot assistant, a helpful AI assistant.\n\n\
             ## Current Time\n{now}\n\n\
             {env}\
             {workspace}",
            env = env_section,
            workspace = workspace_section
        )
    }

    /// The one place that tells the model where "here" is.
    ///
    /// Tool results are part of the transcript, so a `list_dir` from an earlier turn (or from a
    /// chat that was bound to another folder) can outlive the binding. This section is rebuilt for
    /// every turn and explicitly wins over those results.
    fn workspace_section(&self) -> String {
        let ws = self.workspace.display();
        let mut lines = vec!["## Workspace".to_string()];
        match self.tool_sandbox_exec_kind {
            Some(ToolSandboxExecKind::Docker) => {
                lines.push(format!(
                    "- **Current working directory for this turn (host):** {ws}"
                ));
                lines.push(
                    "- **The same folder inside the tool sandbox:** `/workspace` — use this path for **exec**."
                        .to_string(),
                );
                lines.push(
                    "- File tools (`list_dir`, `read_file`, `write_file`, …) run on the host and resolve \
                     relative paths against the host path above."
                        .to_string(),
                );
            }
            Some(ToolSandboxExecKind::HostNative) => {
                lines.push(format!(
                    "- **Current working directory for this turn:** {ws} (the same path inside the tool \
                     sandbox)."
                ));
                lines.push(
                    "- File tools and **exec** resolve relative paths here. Use normal paths for this OS in \
                     **exec** (e.g. Windows: `dir`, `type file.txt`). Do not use `/workspace` unless you are \
                     on a Docker-based tool sandbox."
                        .to_string(),
                );
            }
            None => {
                lines.push(format!(
                    "- **Current working directory for this turn:** {ws}"
                ));
                lines.push(
                    "- File tools (`list_dir`, `read_file`, `write_file`, …) and **exec** resolve relative \
                     paths against this folder."
                        .to_string(),
                );
            }
        }
        let origin = match self.workspace_label.as_deref() {
            Some(label) => format!("This conversation is bound to the workspace \"{label}\"."),
            None => "This conversation is not bound to a workspace folder; it uses the default \
                     workspace."
                .to_string(),
        };
        lines.push(format!("- {origin}"));
        lines.push(
            "- When the user asks where you are working, answer with the path above. An earlier tool \
             result that named another folder is stale; this line wins."
                .to_string(),
        );
        lines.push(
            "- Tools served by an external MCP server keep the root that server was started with, so `.` \
             there does not mean this folder. Use the built-in file tools for work in the workspace."
                .to_string(),
        );
        format!("{}\n", lines.join("\n"))
    }

    fn load_bootstrap_files(&self) -> String {
        let mut parts = Vec::new();
        for name in BOOTSTRAP_FILES {
            let path = self.workspace.join(name);
            if let Ok(content) = std::fs::read_to_string(&path) {
                parts.push(content);
            }
        }
        // Project rules (`.synbot/rules/*.md`) complement AGENTS.md; skip AGENTS.md here because
        // it is already part of BOOTSTRAP_FILES above.
        for (name, content) in crate::project::read_rules(&self.workspace, 32 * 1024) {
            if name == "AGENTS.md" {
                continue;
            }
            parts.push(format!("# Project rule: {name}\n\n{content}"));
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
        assert!(prompt.contains("plan_write"));
    }

    /// The model must be able to say where it works, and must distrust a tool result from an older
    /// turn that named another folder — both halves are what the `## Workspace` section exists for.
    #[test]
    fn workspace_section_names_the_bound_workspace_and_wins_over_tool_results() {
        let dir = tempfile::tempdir().unwrap();
        let shop = dir.path().join("shop");
        std::fs::create_dir_all(&shop).unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        #[cfg(feature = "memory-index")]
        let ctx = ContextBuilder::new(&shop, "main", &skills_dir, None, None);
        #[cfg(not(feature = "memory-index"))]
        let ctx = ContextBuilder::new(&shop, "main", &skills_dir, None);
        let prompt = ctx
            .with_workspace_label(Some("shop".to_string()))
            .build_system_prompt();

        assert!(
            prompt.contains("Current working directory for this turn"),
            "{prompt}"
        );
        assert!(prompt.contains(&shop.display().to_string()), "{prompt}");
        assert!(
            prompt.contains("bound to the workspace \"shop\""),
            "{prompt}"
        );
        assert!(prompt.contains("this line wins"), "{prompt}");

        // An unbound chat must not claim a project name it does not have.
        #[cfg(feature = "memory-index")]
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, None, None);
        #[cfg(not(feature = "memory-index"))]
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, None);
        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("uses the default workspace"), "{prompt}");
    }

    #[test]
    fn build_system_prompt_injects_the_current_plan() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        let mut board = crate::tasks::TaskBoard::load(dir.path());
        board
            .replace(vec![crate::tasks::TaskInput {
                id: None,
                title: "read the code".to_string(),
                status: None,
                notes: None,
            }])
            .unwrap();
        board.save(dir.path()).unwrap();
        #[cfg(feature = "memory-index")]
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, None, None);
        #[cfg(not(feature = "memory-index"))]
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, None);
        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("# Current plan"));
        assert!(prompt.contains("read the code"));
    }

    #[test]
    fn build_system_prompt_injects_the_repository_map_when_an_index_exists() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "pub fn api() {}\n").unwrap();
        let mut index = crate::codeindex::CodeIndex::empty(dir.path());
        index.refresh(dir.path()).unwrap();
        #[cfg(feature = "memory-index")]
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, None, None);
        #[cfg(not(feature = "memory-index"))]
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, None);
        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("# Repository map"), "{prompt}");
        assert!(prompt.contains("1 files / 1 symbols"), "{prompt}");
        assert!(prompt.contains("find_symbols"), "{prompt}");
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
        let prompt = ctx.build_system_prompt_with_role_prompt("You are a helpful tester.", None);
        assert!(prompt.contains("Synbot"));
        assert!(prompt.contains("You are a helpful tester."));
    }

    #[test]
    fn build_system_prompt_injects_project_rules() {
        let dir = tempfile::tempdir().unwrap();
        let skills_dir = dir.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        std::fs::create_dir_all(dir.path().join(".synbot/rules")).unwrap();
        std::fs::write(dir.path().join(".synbot/rules/style.md"), "prefer tabs").unwrap();
        #[cfg(feature = "memory-index")]
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, None, None);
        #[cfg(not(feature = "memory-index"))]
        let ctx = ContextBuilder::new(dir.path(), "main", &skills_dir, None);
        let prompt = ctx.build_system_prompt();
        assert!(prompt.contains("# Project rule: .synbot/rules/style.md"));
        assert!(prompt.contains("prefer tabs"));
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
        assert!(
            id.contains("/workspace"),
            "docker prompt should mention /workspace: {}",
            id
        );
        assert!(
            id.contains("Docker"),
            "docker prompt should mention Docker: {}",
            id
        );
    }
}
