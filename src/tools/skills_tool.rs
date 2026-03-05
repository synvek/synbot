//! list_skills tool — list available skills from the config skills directory (~/.synbot/skills).

use anyhow::Result;
use serde_json::{json, Value};

use crate::agent::skills::{SkillProvider, SkillsLoader};
use crate::config;
use crate::tools::DynTool;

/// Tool to list available skills from the global skills directory (e.g. ~/.synbot/skills/).
/// Each subdirectory that contains SKILL.md is reported as a skill. Use when the user asks what
/// skills are available or how to check skills.
pub struct ListSkillsTool;

impl ListSkillsTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl DynTool for ListSkillsTool {
    fn name(&self) -> &str {
        "list_skills"
    }

    fn description(&self) -> &str {
        "List available skills from the global skills directory (~/.synbot/skills or config root). Returns skill names (one per line). Use when the user asks what skills are available, which skills exist, or to check available skills."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(&self, _args: Value) -> Result<String> {
        let skills_dir = config::skills_dir();
        let loader = SkillsLoader::new(skills_dir.as_path());
        let names = loader.list_skills();
        if names.is_empty() {
            return Ok(format!(
                "No skills found. Skills are loaded from {} (each subdirectory with SKILL.md is a skill).",
                skills_dir.display()
            ));
        }
        let mut lines = vec![format!("Available skills (from {}):", skills_dir.display())];
        for name in &names {
            lines.push(format!("- {}", name));
        }
        Ok(lines.join("\n"))
    }
}
