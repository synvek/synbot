//! Skills loader — markdown-based agent capabilities.

use std::path::{Path, PathBuf};

pub struct SkillsLoader {
    workspace_skills: PathBuf,
}

impl SkillsLoader {
    pub fn new(workspace: &Path) -> Self {
        Self {
            workspace_skills: workspace.join("skills"),
        }
    }

    /// List available skill names.
    pub fn list_skills(&self) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.workspace_skills) {
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
        let path = self.workspace_skills.join(name).join("SKILL.md");
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
