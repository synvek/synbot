//! Role registry — manages Sub-Role definitions and their resolved configurations.
//!
//! Each registered role gets a workspace directory under `workspace/roles/{role_name}/`
//! with `memory`, `sessions`, and `skills` subdirectories.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::{AgentDefaults, RoleConfig};

// ---------------------------------------------------------------------------
// Resolved role parameters
// ---------------------------------------------------------------------------

/// Resolved role parameters after applying defaults from `AgentDefaults`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRoleParams {
    pub provider: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub max_iterations: u32,
}

impl ResolvedRoleParams {
    /// Resolve optional fields in `RoleConfig`, falling back to `AgentDefaults`.
    pub fn from_config(role: &RoleConfig, defaults: &AgentDefaults) -> Self {
        Self {
            provider: role
                .provider
                .clone()
                .unwrap_or_else(|| defaults.provider.clone()),
            model: role
                .model
                .clone()
                .unwrap_or_else(|| defaults.model.clone()),
            max_tokens: role.max_tokens.unwrap_or(defaults.max_tokens),
            temperature: role.temperature.unwrap_or(defaults.temperature),
            max_iterations: role.max_iterations.unwrap_or(defaults.max_tool_iterations),
        }
    }
}

// ---------------------------------------------------------------------------
// Role context
// ---------------------------------------------------------------------------

/// Runtime role context generated after registration.
#[derive(Debug, Clone)]
pub struct RoleContext {
    pub name: String,
    pub system_prompt: String,
    pub skills: Vec<String>,
    pub tools: Vec<String>,
    pub params: ResolvedRoleParams,
    pub workspace_dir: PathBuf,
}

// ---------------------------------------------------------------------------
// Role registry
// ---------------------------------------------------------------------------

/// Registry that manages all configured Sub-Roles.
pub struct RoleRegistry {
    roles: HashMap<String, RoleContext>,
}

impl RoleRegistry {
    pub fn new() -> Self {
        Self {
            roles: HashMap::new(),
        }
    }

    /// Load and register all roles from config, creating workspace directories.
    ///
    /// For each role the optional fields (`provider`, `model`, `max_tokens`,
    /// `temperature`, `max_iterations`) are resolved against `defaults`.
    /// Workspace directories are created under `workspace/roles/{role_name}/`.
    pub fn load_from_config(
        &mut self,
        roles: &[RoleConfig],
        defaults: &AgentDefaults,
        workspace: &Path,
    ) -> Result<()> {
        for role in roles {
            let params = ResolvedRoleParams::from_config(role, defaults);

            let role_dir = workspace.join("roles").join(&role.name);
            let subdirs = ["memory", "sessions", "skills"];
            for sub in &subdirs {
                let dir = role_dir.join(sub);
                std::fs::create_dir_all(&dir).with_context(|| {
                    format!(
                        "failed to create workspace directory '{}' for role '{}'",
                        dir.display(),
                        role.name
                    )
                })?;
            }

            let ctx = RoleContext {
                name: role.name.clone(),
                system_prompt: role.system_prompt.clone(),
                skills: role.skills.clone(),
                tools: role.tools.clone(),
                params,
                workspace_dir: role_dir,
            };

            self.roles.insert(role.name.clone(), ctx);
        }
        Ok(())
    }

    /// Get a role context by name.
    pub fn get(&self, name: &str) -> Option<&RoleContext> {
        self.roles.get(name)
    }

    /// List all registered role names.
    pub fn list_names(&self) -> Vec<&str> {
        self.roles.keys().map(|s| s.as_str()).collect()
    }

    /// Check whether a role with the given name is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.roles.contains_key(name)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create an `AgentDefaults` with known values.
    fn test_defaults() -> AgentDefaults {
        AgentDefaults {
            workspace: "/tmp/test".into(),
            provider: "default_provider".into(),
            model: "default_model".into(),
            max_tokens: 4096,
            temperature: 0.5,
            max_tool_iterations: 10,
            max_concurrent_subagents: 3,
            roles: Vec::new(),
        }
    }

    /// Helper: create a minimal `RoleConfig`.
    fn make_role(name: &str, prompt: &str) -> RoleConfig {
        RoleConfig {
            name: name.into(),
            system_prompt: prompt.into(),
            skills: vec![],
            tools: vec![],
            provider: None,
            model: None,
            max_tokens: None,
            temperature: None,
            max_iterations: None,
        }
    }

    // -- ResolvedRoleParams --------------------------------------------------

    #[test]
    fn resolved_params_uses_defaults_when_none() {
        let defaults = test_defaults();
        let role = make_role("r", "prompt");
        let params = ResolvedRoleParams::from_config(&role, &defaults);

        assert_eq!(params.provider, "default_provider");
        assert_eq!(params.model, "default_model");
        assert_eq!(params.max_tokens, 4096);
        assert_eq!(params.temperature, 0.5);
        assert_eq!(params.max_iterations, 10);
    }

    #[test]
    fn resolved_params_uses_role_values_when_some() {
        let defaults = test_defaults();
        let role = RoleConfig {
            name: "r".into(),
            system_prompt: "prompt".into(),
            skills: vec![],
            tools: vec![],
            provider: Some("openai".into()),
            model: Some("gpt-4o".into()),
            max_tokens: Some(2048),
            temperature: Some(0.9),
            max_iterations: Some(5),
        };
        let params = ResolvedRoleParams::from_config(&role, &defaults);

        assert_eq!(params.provider, "openai");
        assert_eq!(params.model, "gpt-4o");
        assert_eq!(params.max_tokens, 2048);
        assert_eq!(params.temperature, 0.9);
        assert_eq!(params.max_iterations, 5);
    }

    #[test]
    fn resolved_params_partial_override() {
        let defaults = test_defaults();
        let role = RoleConfig {
            name: "r".into(),
            system_prompt: "prompt".into(),
            skills: vec![],
            tools: vec![],
            provider: Some("openai".into()),
            model: None,
            max_tokens: Some(1024),
            temperature: None,
            max_iterations: None,
        };
        let params = ResolvedRoleParams::from_config(&role, &defaults);

        assert_eq!(params.provider, "openai");
        assert_eq!(params.model, "default_model"); // from defaults
        assert_eq!(params.max_tokens, 1024);
        assert_eq!(params.temperature, 0.5); // from defaults
        assert_eq!(params.max_iterations, 10); // from defaults
    }

    // -- RoleRegistry --------------------------------------------------------

    #[test]
    fn new_registry_is_empty() {
        let reg = RoleRegistry::new();
        assert!(reg.list_names().is_empty());
        assert!(!reg.contains("anything"));
        assert!(reg.get("anything").is_none());
    }

    #[test]
    fn load_from_config_registers_roles() {
        let tmp = TempDir::new().unwrap();
        let defaults = test_defaults();
        let roles = vec![
            make_role("ui_designer", "You are a UI designer"),
            make_role("product_manager", "You are a PM"),
        ];

        let mut reg = RoleRegistry::new();
        reg.load_from_config(&roles, &defaults, tmp.path()).unwrap();

        assert!(reg.contains("ui_designer"));
        assert!(reg.contains("product_manager"));
        assert!(!reg.contains("unknown"));

        let mut names = reg.list_names();
        names.sort();
        assert_eq!(names, vec!["product_manager", "ui_designer"]);
    }

    #[test]
    fn load_from_config_creates_workspace_directories() {
        let tmp = TempDir::new().unwrap();
        let defaults = test_defaults();
        let roles = vec![make_role("test_role", "prompt")];

        let mut reg = RoleRegistry::new();
        reg.load_from_config(&roles, &defaults, tmp.path()).unwrap();

        let role_dir = tmp.path().join("roles").join("test_role");
        assert!(role_dir.join("memory").is_dir());
        assert!(role_dir.join("sessions").is_dir());
        assert!(role_dir.join("skills").is_dir());
    }

    #[test]
    fn get_returns_correct_role_context() {
        let tmp = TempDir::new().unwrap();
        let defaults = test_defaults();
        let roles = vec![RoleConfig {
            name: "designer".into(),
            system_prompt: "You design things".into(),
            skills: vec!["figma".into(), "css".into()],
            tools: vec!["web".into()],
            provider: Some("openai".into()),
            model: Some("gpt-4o".into()),
            max_tokens: None,
            temperature: Some(0.8),
            max_iterations: None,
        }];

        let mut reg = RoleRegistry::new();
        reg.load_from_config(&roles, &defaults, tmp.path()).unwrap();

        let ctx = reg.get("designer").unwrap();
        assert_eq!(ctx.name, "designer");
        assert_eq!(ctx.system_prompt, "You design things");
        assert_eq!(ctx.skills, vec!["figma", "css"]);
        assert_eq!(ctx.tools, vec!["web"]);
        assert_eq!(ctx.params.provider, "openai");
        assert_eq!(ctx.params.model, "gpt-4o");
        assert_eq!(ctx.params.max_tokens, 4096); // from defaults
        assert_eq!(ctx.params.temperature, 0.8);
        assert_eq!(ctx.params.max_iterations, 10); // from defaults
        assert_eq!(
            ctx.workspace_dir,
            tmp.path().join("roles").join("designer")
        );
    }

    #[test]
    fn load_from_config_with_empty_roles_succeeds() {
        let tmp = TempDir::new().unwrap();
        let defaults = test_defaults();

        let mut reg = RoleRegistry::new();
        reg.load_from_config(&[], &defaults, tmp.path()).unwrap();

        assert!(reg.list_names().is_empty());
    }

    #[test]
    fn load_from_config_overwrites_duplicate_role() {
        let tmp = TempDir::new().unwrap();
        let defaults = test_defaults();
        let roles = vec![
            RoleConfig {
                name: "dup".into(),
                system_prompt: "first".into(),
                skills: vec![],
                tools: vec![],
                provider: None,
                model: None,
                max_tokens: None,
                temperature: None,
                max_iterations: None,
            },
            RoleConfig {
                name: "dup".into(),
                system_prompt: "second".into(),
                skills: vec![],
                tools: vec![],
                provider: None,
                model: None,
                max_tokens: None,
                temperature: None,
                max_iterations: None,
            },
        ];

        let mut reg = RoleRegistry::new();
        reg.load_from_config(&roles, &defaults, tmp.path()).unwrap();

        // The last one wins
        let ctx = reg.get("dup").unwrap();
        assert_eq!(ctx.system_prompt, "second");
        assert_eq!(reg.list_names().len(), 1);
    }

    #[test]
    fn workspace_dir_is_set_correctly_for_each_role() {
        let tmp = TempDir::new().unwrap();
        let defaults = test_defaults();
        let roles = vec![
            make_role("alpha", "prompt a"),
            make_role("beta", "prompt b"),
        ];

        let mut reg = RoleRegistry::new();
        reg.load_from_config(&roles, &defaults, tmp.path()).unwrap();

        assert_eq!(
            reg.get("alpha").unwrap().workspace_dir,
            tmp.path().join("roles").join("alpha")
        );
        assert_eq!(
            reg.get("beta").unwrap().workspace_dir,
            tmp.path().join("roles").join("beta")
        );
    }
}
