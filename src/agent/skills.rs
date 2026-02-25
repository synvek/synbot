//! Skills loader — markdown-based agent capabilities.
//! Loads from the global skills directory `~/.synbot/skills/` or from plugin [SkillProvider]s.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// SkillProvider trait (for plugins and composite)
// ---------------------------------------------------------------------------

/// A source of skills (filesystem, plugin, or composite). Plugins can implement this trait
/// and register with a [CompositeSkillProvider].
pub trait SkillProvider: Send + Sync {
    /// List available skill names from this provider.
    fn list_skills(&self) -> Vec<String>;

    /// Load a skill's content by name. Returns None if this provider does not have the skill.
    fn load_skill(&self, name: &str) -> Option<String>;

    /// Build a summary string for the system prompt (e.g. "Available skills: - a - b").
    fn build_skills_summary(&self) -> String {
        let skills = self.list_skills();
        if skills.is_empty() {
            return String::new();
        }
        let mut lines = vec!["Available skills (use read_file to load):".to_string()];
        for name in &skills {
            lines.push(format!("- {}", name));
        }
        lines.join("\n")
    }
}

// ---------------------------------------------------------------------------
// Filesystem provider (default)
// ---------------------------------------------------------------------------

/// Loads skills from a directory: each subdir with SKILL.md is a skill.
pub struct SkillsLoader {
    /// Global skills root: ~/.synbot/skills/
    skills_root: PathBuf,
}

impl SkillsLoader {
    /// Create a loader for the given skills root (e.g. `config::skills_dir()`).
    pub fn new(skills_root: &Path) -> Self {
        Self {
            skills_root: skills_root.to_path_buf(),
        }
    }
}

impl SkillProvider for SkillsLoader {
    fn list_skills(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.skills_root) {
            for entry in entries.flatten() {
                if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                    let skill_file = entry.path().join("SKILL.md");
                    if skill_file.exists() {
                        if let Some(name) = entry.file_name().to_str() {
                            names.push(name.to_string());
                        }
                    }
                }
            }
        }
        names
    }

    fn load_skill(&self, name: &str) -> Option<String> {
        let path = self.skills_root.join(name).join("SKILL.md");
        std::fs::read_to_string(&path).ok()
    }
}

// ---------------------------------------------------------------------------
// Composite provider (merges multiple providers)
// ---------------------------------------------------------------------------

/// Aggregates multiple [SkillProvider]s: merged deduplicated list_skills, first non-None load_skill.
pub struct CompositeSkillProvider {
    providers: Vec<Box<dyn SkillProvider>>,
}

impl CompositeSkillProvider {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn add(&mut self, provider: Box<dyn SkillProvider>) {
        self.providers.push(provider);
    }

    /// Build a default composite with only the filesystem provider for the given skills dir.
    pub fn default_with_fs(skills_root: &Path) -> Self {
        let mut c = Self::new();
        c.add(Box::new(SkillsLoader::new(skills_root)));
        c
    }
}

impl Default for CompositeSkillProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillProvider for CompositeSkillProvider {
    fn list_skills(&self) -> Vec<String> {
        let mut set = HashSet::new();
        for p in &self.providers {
            for name in p.list_skills() {
                set.insert(name);
            }
        }
        let mut names: Vec<String> = set.into_iter().collect();
        names.sort();
        names
    }

    fn load_skill(&self, name: &str) -> Option<String> {
        for p in &self.providers {
            if let Some(content) = p.load_skill(name) {
                return Some(content);
            }
        }
        None
    }

    fn build_skills_summary(&self) -> String {
        let skills = self.list_skills();
        if skills.is_empty() {
            return String::new();
        }
        let mut lines = vec!["Available skills (use read_file to load):".to_string()];
        for name in &skills {
            lines.push(format!("- {}", name));
        }
        lines.join("\n")
    }
}
