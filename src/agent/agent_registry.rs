//! Agent registry — runtime agents that reference a role.
//!
//! Each agent has a name, references one role (for system prompt), and has
//! workspace_dir and resolved params. All agents share the same workspace root
//! (user documents only); memory and skills live under ~/.synbot, not under workspace.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::{AgentConfig, MainAgent};
use crate::agent::role_registry::RoleRegistry;

// ---------------------------------------------------------------------------
// Resolved agent parameters
// ---------------------------------------------------------------------------

/// Resolved agent parameters after applying defaults from `MainAgent`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedAgentParams {
    pub provider: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub max_iterations: u32,
    /// Maximum number of chat history messages to send to the model (most recent N).
    pub max_chat_history_messages: u32,
}

impl ResolvedAgentParams {
    /// Params for the implicit main agent (all from MainAgent).
    pub fn from_main_defaults(main_agent: &MainAgent) -> Self {
        Self {
            provider: main_agent.provider.clone(),
            model: main_agent.model.clone(),
            max_tokens: main_agent.max_tokens,
            temperature: main_agent.temperature,
            max_iterations: main_agent.max_tool_iterations,
            max_chat_history_messages: main_agent.max_chat_history_messages,
        }
    }

    pub fn from_config(agent: &AgentConfig, defaults: &MainAgent) -> Self {
        Self {
            provider: agent
                .provider
                .clone()
                .unwrap_or_else(|| defaults.provider.clone()),
            model: agent
                .model
                .clone()
                .unwrap_or_else(|| defaults.model.clone()),
            max_tokens: agent.max_tokens.unwrap_or(defaults.max_tokens),
            temperature: agent.temperature.unwrap_or(defaults.temperature),
            max_iterations: agent.max_iterations.unwrap_or(defaults.max_tool_iterations),
            max_chat_history_messages: defaults.max_chat_history_messages,
        }
    }
}

// ---------------------------------------------------------------------------
// Agent context
// ---------------------------------------------------------------------------

/// Runtime agent context after registration.
#[derive(Debug, Clone)]
pub struct AgentContext {
    pub name: String,
    pub role_name: String,
    pub system_prompt: String,
    pub skills: Vec<String>,
    pub tools: Vec<String>,
    pub params: ResolvedAgentParams,
    pub workspace_dir: PathBuf,
}

// ---------------------------------------------------------------------------
// Agent registry
// ---------------------------------------------------------------------------

/// Registry that manages all configured agents (runtime entities referencing roles).
pub struct AgentRegistry {
    agents: HashMap<String, AgentContext>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Load and register all agents from config.
    /// The main agent is implicit (role "main", from main_agent settings). Additional agents from main_agent.agents.
    /// All agents share the same workspace root (for user documents only); memory is under ~/.synbot/memory/{id}, skills under ~/.synbot/skills/.
    pub fn load_from_config(
        &mut self,
        main_agent: &MainAgent,
        role_registry: &RoleRegistry,
        workspace: &Path,
    ) -> Result<()> {
        // Register the implicit main agent (role "main").
        let role_ctx = role_registry.get("main").with_context(|| {
            "main agent requires role 'main' (add ~/.synbot/roles/main/ with AGENTS.md, SOUL.md, TOOLS.md, USER.md, IDENTITY.md)"
        })?;
        let system_prompt = role_ctx.system_prompt.clone();
        let params = ResolvedAgentParams::from_main_defaults(main_agent);
        let ctx = AgentContext {
            name: "main".to_string(),
            role_name: "main".to_string(),
            system_prompt,
            skills: Vec::new(),
            tools: Vec::new(),
            params,
            workspace_dir: workspace.to_path_buf(),
        };
        self.agents.insert("main".to_string(), ctx);

        // Register additional agents (config forbids name "main").
        for agent in &main_agent.agents {
            if agent.name == "main" {
                anyhow::bail!("agent name 'main' is reserved (main agent is implicit from mainAgent)");
            }
            let role_ctx = role_registry.get(&agent.role).with_context(|| {
                format!("agent '{}' references unknown role '{}' (add a subdir under ~/.synbot/roles/ for this role)", agent.name, agent.role)
            })?;
            let system_prompt = role_ctx.system_prompt.clone();
            let params = ResolvedAgentParams::from_config(agent, main_agent);
            let ctx = AgentContext {
                name: agent.name.clone(),
                role_name: agent.role.clone(),
                system_prompt,
                skills: agent.skills.clone(),
                tools: agent.tools.clone(),
                params,
                workspace_dir: workspace.to_path_buf(),
            };
            self.agents.insert(agent.name.clone(), ctx);
        }
        Ok(())
    }

    /// Get an agent context by name.
    pub fn get(&self, name: &str) -> Option<&AgentContext> {
        self.agents.get(name)
    }

    /// List all registered agent names.
    pub fn list_names(&self) -> Vec<&str> {
        self.agents.keys().map(|s| s.as_str()).collect()
    }

    /// Check whether an agent with the given name is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.agents.contains_key(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_agent(name: &str, role: &str) -> AgentConfig {
        AgentConfig {
            name: name.into(),
            role: role.into(),
            provider: None,
            model: None,
            max_tokens: None,
            temperature: None,
            max_iterations: None,
            skills: Vec::new(),
            tools: Vec::new(),
        }
    }

    fn test_defaults() -> MainAgent {
        MainAgent {
            workspace: "/tmp/test".into(),
            provider: "default_provider".into(),
            model: "default_model".into(),
            max_tokens: 4096,
            temperature: 0.5,
            max_tool_iterations: 10,
            max_chat_history_messages: 20,
            max_concurrent_subagents: 3,
            agents: Vec::new(),
        }
    }

    #[test]
    fn main_agent_uses_workspace_root() {
        let tmp = TempDir::new().unwrap();
        let roles_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(roles_dir.path().join("main")).unwrap();
        std::fs::write(roles_dir.path().join("main").join("AGENTS.md"), "# Main").unwrap();
        std::fs::write(roles_dir.path().join("main").join("SOUL.md"), "").unwrap();
        std::fs::write(roles_dir.path().join("main").join("TOOLS.md"), "").unwrap();

        let mut role_reg = RoleRegistry::new();
        role_reg.load_from_dirs(roles_dir.path()).unwrap();

        let main_agent = test_defaults();
        let mut agent_reg = AgentRegistry::new();
        agent_reg
            .load_from_config(&main_agent, &role_reg, tmp.path())
            .unwrap();

        let ctx = agent_reg.get("main").unwrap();
        assert_eq!(ctx.workspace_dir, tmp.path());
        assert_eq!(ctx.role_name, "main");
    }

    #[test]
    fn non_main_agent_shares_workspace_root() {
        let tmp = TempDir::new().unwrap();
        let roles_dir = TempDir::new().unwrap();
        for (ref_name, content) in &[("main", ""), ("dev", "# Dev")] {
            let dir = roles_dir.path().join(ref_name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("AGENTS.md"), *content).unwrap();
            std::fs::write(dir.join("SOUL.md"), "").unwrap();
            std::fs::write(dir.join("TOOLS.md"), "").unwrap();
        }

        let mut role_reg = RoleRegistry::new();
        role_reg.load_from_dirs(roles_dir.path()).unwrap();

        let mut main_agent = test_defaults();
        main_agent.agents = vec![make_agent("dev", "dev")];
        let mut agent_reg = AgentRegistry::new();
        agent_reg
            .load_from_config(&main_agent, &role_reg, tmp.path())
            .unwrap();

        let ctx = agent_reg.get("dev").unwrap();
        // All agents share the same workspace root (no agents/dev, memory, or skills under workspace).
        assert_eq!(ctx.workspace_dir, tmp.path());
    }
}
