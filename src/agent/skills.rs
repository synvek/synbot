//! Skills loader — markdown-based agent capabilities.
//! Loads from the global skills directory `~/.synbot/skills/` or from plugin [SkillProvider]s.
//!
//! SKILL.md format: YAML frontmatter between `---` with required `name` and `description`.
//! Example:
//! ```yaml
//! ---
//! name: skill-creator
//! description: Create new skills, modify and improve existing skills...
//! ---
//! ```

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Parsed frontmatter from SKILL.md (name and description only).
#[derive(Debug, Default)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
}

/// Parse SKILL.md content: extract YAML frontmatter between first `---` and second `---`,
/// then parse `name:` and `description:` (description supports inline or multiline after `|`/`>`).
pub fn parse_skill_frontmatter(content: &str) -> Option<SkillFrontmatter> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        return None;
    }
    let mut end_idx = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            end_idx = Some(i);
            break;
        }
    }
    let end_idx = end_idx?;
    let frontmatter_lines = &lines[1..end_idx];

    let mut name = String::new();
    let mut description = String::new();
    let mut i = 0;
    while i < frontmatter_lines.len() {
        let line = frontmatter_lines[i];
        if line.starts_with("name:") {
            name = line["name:".len()..].trim().trim_matches('"').trim_matches('\'').to_string();
        } else if line.starts_with("description:") {
            let value = line["description:".len()..].trim();
            if value == "|" || value == ">" || value == ">-" || value == "|-" {
                let mut continuation = Vec::new();
                i += 1;
                while i < frontmatter_lines.len() {
                    let next = frontmatter_lines[i];
                    if next.starts_with("  ") || next.starts_with('\t') {
                        continuation.push(next.trim());
                        i += 1;
                    } else {
                        break;
                    }
                }
                description = continuation.join(" ");
                continue;
            } else {
                description = value.trim_matches('"').trim_matches('\'').to_string();
            }
        }
        i += 1;
    }
    if name.is_empty() {
        return None;
    }
    Some(SkillFrontmatter { name, description })
}

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

    /// Build skills summary for system prompt: each skill uses name and description from SKILL.md frontmatter.
    fn build_skills_summary(&self) -> String {
        let skills = self.list_skills();
        if skills.is_empty() {
            return String::new();
        }
        let mut lines = vec!["Available skills (use read_file to load SKILL.md for full content):".to_string()];
        for dir_name in &skills {
            let (display_name, description) = self
                .load_skill(dir_name)
                .and_then(|c| parse_skill_frontmatter(&c).map(|fm| (fm.name, fm.description)))
                .unwrap_or_else(|| (dir_name.clone(), String::new()));
            if description.is_empty() {
                lines.push(format!("- **{}**", display_name));
            } else {
                lines.push(format!("- **{}**: {}", display_name, description));
            }
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_skill_frontmatter_inline_description() {
        let content = r#"---
name: skill-creator
description: Create new skills, modify and improve existing skills. Use when users want to create a skill from scratch.
---

# Skill Creator
"#;
        let fm = parse_skill_frontmatter(content).unwrap();
        assert_eq!(fm.name, "skill-creator");
        assert!(fm.description.starts_with("Create new skills"));
    }

    #[test]
    fn parse_skill_frontmatter_multiline_description() {
        let content = r#"---
name: my-skill
description: |
  First line of description.
  Second line.
---

# Body
"#;
        let fm = parse_skill_frontmatter(content).unwrap();
        assert_eq!(fm.name, "my-skill");
        assert_eq!(fm.description, "First line of description. Second line.");
    }

    #[test]
    fn parse_skill_frontmatter_no_opening_delimiter() {
        assert!(parse_skill_frontmatter("name: x\ndescription: y").is_none());
    }

    #[test]
    fn parse_skill_frontmatter_no_name() {
        let content = "---\ndescription: only\n---";
        assert!(parse_skill_frontmatter(content).is_none());
    }
}
