//! Skills loader — markdown-based agent capabilities.
//! Loads from the global skills directory `~/.synbot/skills/`, not from workspace.

use std::path::{Path, PathBuf};

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

    /// List available skill names.
    pub fn list_skills(&self) -> Vec<String> {
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

    /// Load a skill's content by name.
    pub fn load_skill(&self, name: &str) -> Option<String> {
        let path = self.skills_root.join(name).join("SKILL.md");
        std::fs::read_to_string(&path).ok()
    }

    /// Build a summary of available skills for the system prompt.
    pub fn build_skills_summary(&self) -> String {
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
