//! Role registry — manages role definitions (behavior only).
//!
//! Each role is a name + reference; system prompt is built from
//! `~/.synbot/roles/{reference}/` (AGENTS.md, SOUL.md, TOOLS.md).
//! No workspace or params; those belong to agents.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

/// Read AGENTS.md, SOUL.md, TOOLS.md from `roles_dir/reference/` and concatenate into system prompt.
/// Missing files or directories are skipped; returns read content or an empty string.
pub fn build_system_prompt_from_role_dir(roles_dir: &Path, reference: &str) -> String {
    let role_dir = roles_dir.join(reference);
    if !role_dir.is_dir() {
        return String::new();
    }
    let files = ["AGENTS.md", "SOUL.md", "TOOLS.md", "USER.md"];
    let mut parts = Vec::new();
    for name in &files {
        let path = role_dir.join(name);
        if let Ok(content) = std::fs::read_to_string(&path) {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
        }
    }
    parts.join("\n\n")
}

// ---------------------------------------------------------------------------
// Role definition context (runtime)
// ---------------------------------------------------------------------------

/// Runtime role definition context: name and system prompt only.
#[derive(Debug, Clone)]
pub struct RoleDefinitionContext {
    pub name: String,
    pub system_prompt: String,
}

// ---------------------------------------------------------------------------
// Role registry
// ---------------------------------------------------------------------------

/// Registry that manages role definitions (name -> system prompt).
/// Roles are discovered by scanning roles_dir for subdirectories; each subdir name is a role (reference).
pub struct RoleRegistry {
    roles: HashMap<String, RoleDefinitionContext>,
}

impl RoleRegistry {
    pub fn new() -> Self {
        Self {
            roles: HashMap::new(),
        }
    }

    /// Discover and load all roles from the roles directory.
    /// Each subdirectory of roles_dir is a role; name = reference = subdir name.
    /// System prompt is built from roles_dir/{name}/ (AGENTS.md, SOUL.md, TOOLS.md).
    pub fn load_from_dirs(&mut self, roles_dir: &Path) -> Result<()> {
        if !roles_dir.is_dir() {
            return Ok(());
        }
        for entry in std::fs::read_dir(roles_dir).with_context(|| format!("reading roles dir {}", roles_dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if !name.starts_with('.') {
                        let system_prompt = build_system_prompt_from_role_dir(roles_dir, name);
                        let ctx = RoleDefinitionContext {
                            name: name.to_string(),
                            system_prompt,
                        };
                        self.roles.insert(name.to_string(), ctx);
                    }
                }
            }
        }
        Ok(())
    }

    /// Get a role definition context by name.
    pub fn get(&self, name: &str) -> Option<&RoleDefinitionContext> {
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

    #[test]
    fn new_registry_is_empty() {
        let reg = RoleRegistry::new();
        assert!(reg.list_names().is_empty());
        assert!(!reg.contains("anything"));
        assert!(reg.get("anything").is_none());
    }

    #[test]
    fn load_from_dirs_registers_subdirs_as_roles() {
        let roles_dir = TempDir::new().unwrap();
        let d1 = roles_dir.path().join("ui_designer");
        let d2 = roles_dir.path().join("product_manager");
        std::fs::create_dir_all(&d1).unwrap();
        std::fs::create_dir_all(&d2).unwrap();

        let mut reg = RoleRegistry::new();
        reg.load_from_dirs(roles_dir.path()).unwrap();

        assert!(reg.contains("ui_designer"));
        assert!(reg.contains("product_manager"));
        assert!(!reg.contains("unknown"));

        let mut names = reg.list_names();
        names.sort();
        assert_eq!(names, vec!["product_manager", "ui_designer"]);
    }

    #[test]
    fn get_returns_correct_role_context() {
        let roles_dir = TempDir::new().unwrap();
        let role_tpl = roles_dir.path().join("designer");
        std::fs::create_dir_all(&role_tpl).unwrap();
        std::fs::write(role_tpl.join("AGENTS.md"), "You design things").unwrap();
        std::fs::write(role_tpl.join("SOUL.md"), "").unwrap();
        std::fs::write(role_tpl.join("TOOLS.md"), "").unwrap();

        let mut reg = RoleRegistry::new();
        reg.load_from_dirs(roles_dir.path()).unwrap();

        let ctx = reg.get("designer").unwrap();
        assert_eq!(ctx.name, "designer");
        assert_eq!(ctx.system_prompt, "You design things");
    }

    #[test]
    fn load_from_dirs_with_empty_or_missing_dir_succeeds() {
        let roles_dir = TempDir::new().unwrap();
        let mut reg = RoleRegistry::new();
        reg.load_from_dirs(roles_dir.path()).unwrap();
        assert!(reg.list_names().is_empty());
    }

    #[test]
    fn load_from_dirs_skips_dot_dirs() {
        let roles_dir = TempDir::new().unwrap();
        std::fs::create_dir_all(roles_dir.path().join(".hidden")).unwrap();
        std::fs::create_dir_all(roles_dir.path().join("visible")).unwrap();
        let mut reg = RoleRegistry::new();
        reg.load_from_dirs(roles_dir.path()).unwrap();
        assert!(!reg.contains(".hidden"));
        assert!(reg.contains("visible"));
    }
}
