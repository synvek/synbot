//! Skills tools: list_skills (from system prompt), list_system_skills (from config dir), read_system_skill (load SKILL.md), install_system_skill.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

use crate::config;
use crate::tools::DynTool;

/// Tool that does not read the filesystem. The available skills (name + description) are already
/// in your system prompt under the "# Skills" section. Use that section to answer the user:
/// summarize, categorize, or list skills as appropriate.
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
        "When the user asks what skills you have, which skills are available, or to list skills: your available skills and their descriptions are already in your system prompt under the '# Skills' section. Use that section to answer—summarize, categorize, or list them. Do not read the filesystem; answer from your system context."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(&self, _args: Value) -> Result<String> {
        Ok("Your available skills and their descriptions are in your system prompt under '# Skills'. Use that section to answer the user: summarize, categorize, or list the skills as appropriate.".to_string())
    }
}

// ---------------------------------------------------------------------------
// System skills (config dir: ~/.synbot/skills/) — not under workspace
// ---------------------------------------------------------------------------

/// Lists system-installed skills from the config skills directory (~/.synbot/skills/).
/// Use this when you need to discover which skills are available on disk (e.g. before using read_system_skill).
pub struct ListSystemSkillsTool;

impl ListSystemSkillsTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl DynTool for ListSystemSkillsTool {
    fn name(&self) -> &str {
        "list_system_skills"
    }

    fn description(&self) -> &str {
        "List system-installed skills from the config skills directory (e.g. ~/.synbot/skills/). Returns skill names and the directory path. Use read_system_skill(name) to load a skill's full SKILL.md content. Do not use list_dir('skills') under workspace—that path is for user skills and may be empty."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(&self, _args: Value) -> Result<String> {
        let root = config::skills_dir();
        let mut names: Vec<String> = Vec::new();
        if root.exists() {
            if let Ok(entries) = std::fs::read_dir(&root) {
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
        }
        names.sort();
        let path_display = root.display().to_string();
        if names.is_empty() {
            return Ok(format!(
                "System skills directory: {} (empty or missing). Run `synbot onboard` to install default skill templates.",
                path_display
            ));
        }
        Ok(format!(
            "System skills directory: {}\nSkill names: {}",
            path_display,
            names.join(", ")
        ))
    }
}

/// Reads the full SKILL.md content for a system skill by name (from ~/.synbot/skills/{name}/SKILL.md).
pub struct ReadSystemSkillTool;

impl ReadSystemSkillTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl DynTool for ReadSystemSkillTool {
    fn name(&self) -> &str {
        "read_system_skill"
    }

    fn description(&self) -> &str {
        "Load the full SKILL.md content for a system-installed skill by name. The skill must exist under the config skills directory (see list_system_skills). Use this when you need to follow a skill's instructions (e.g. code-dev, skill-creator)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Skill directory name (e.g. code-dev, skill-creator)" }
            },
            "required": ["name"]
        })
    }

    async fn call(&self, args: Value) -> Result<String> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if name.is_empty() {
            return Ok("Error: name is required. Use list_system_skills to see available names.".to_string());
        }
        let path = config::skills_dir().join(name).join("SKILL.md");
        if !path.exists() {
            return Ok(format!(
                "Error: skill '{}' not found at {}. Use list_system_skills to see available skills.",
                name,
                path.display()
            ));
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("read_system_skill: failed to read {}", path.display()))?;
        Ok(content)
    }
}

// ---------------------------------------------------------------------------
// Install system skill (npx skills add → copy to ~/.synbot/skills)
// ---------------------------------------------------------------------------

/// Installs a skill from the ecosystem (e.g. GitHub) into Synbot's system skills directory
/// (~/.synbot/skills/). Runs `npx skills add <package_spec> -g -y` then copies from
/// ~/.agents/skills/ into ~/.synbot/skills/ so the skill is discoverable by list_system_skills.
pub struct InstallSystemSkillTool;

impl InstallSystemSkillTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl DynTool for InstallSystemSkillTool {
    fn name(&self) -> &str {
        "install_system_skill"
    }

    fn description(&self) -> &str {
        "Install a skill from the ecosystem (e.g. owner/repo@skill) into Synbot's system skills directory (~/.synbot/skills/). Use this instead of exec('npx skills add ...') so the skill is installed where Synbot can find it. Argument: package_spec (e.g. 'othmanadi/planning-with-files@planning-with-files'). Runs with a long timeout (600s) and copies from the CLI's global install path into ~/.synbot/skills/."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "package_spec": { "type": "string", "description": "Package spec from skills.sh (e.g. owner/repo@skill or owner/repo)" }
            },
            "required": ["package_spec"]
        })
    }

    async fn call(&self, args: Value) -> Result<String> {
        let package_spec = args
            .get("package_spec")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if package_spec.is_empty() {
            return Ok("Error: package_spec is required (e.g. othmanadi/planning-with-files@planning-with-files).".to_string());
        }

        let dest_root = config::skills_dir();
        if let Err(e) = std::fs::create_dir_all(&dest_root) {
            return Ok(format!(
                "Error: could not create skills directory {}: {}",
                dest_root.display(),
                e
            ));
        }

        let cmd = format!("npx skills add {} -g -y", package_spec);
        let timeout_secs = 600u64;
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "sh" })
                .args(if cfg!(windows) {
                    vec!["/C", &cmd]
                } else {
                    vec!["-c", &cmd]
                })
                .output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("npx skills add timed out after {}s", timeout_secs))?
        .map_err(|e| anyhow::anyhow!("npx skills add failed: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            return Ok(format!(
                "npx skills add failed (exit code {:?}). stdout: {} stderr: {}",
                output.status.code(),
                stdout,
                stderr
            ));
        }

        let agents_skills = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("no home dir"))?
            .join(".agents")
            .join("skills");
        if !agents_skills.exists() {
            return Ok(format!(
                "Install command succeeded but {} not found. The CLI may have installed elsewhere. stdout: {} stderr: {}",
                agents_skills.display(),
                stdout,
                stderr
            ));
        }

        let mut copied: Vec<String> = Vec::new();
        let entries = std::fs::read_dir(&agents_skills).with_context(|| {
            format!("read_dir {}", agents_skills.display())
        })?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name: String = match entry.file_name().to_str() {
                Some(n) => n.to_string(),
                None => continue,
            };
            if !path.join("SKILL.md").exists() {
                continue;
            }
            let dest = dest_root.join(&name);
            if let Err(e) = copy_dir_all(&path, &dest).await {
                return Ok(format!(
                    "Copied {} but failed to copy {} to {}: {}",
                    copied.join(", "),
                    name,
                    dest.display(),
                    e
                ));
            }
            copied.push(name.to_string());
        }

        if copied.is_empty() {
            return Ok(format!(
                "npx skills add succeeded but no skill with SKILL.md found under {}. stdout: {} stderr: {}",
                agents_skills.display(),
                stdout,
                stderr
            ));
        }

        Ok(format!(
            "Installed and copied to {}: {}. Use list_system_skills and read_system_skill(name) to use them.",
            dest_root.display(),
            copied.join(", ")
        ))
    }
}

async fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    tokio::fs::create_dir_all(dst).await?;
    let mut entries = tokio::fs::read_dir(src).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let name = entry.file_name();
        let dest_path = dst.join(&name);
        if entry.file_type().await?.is_dir() {
            let path = path.to_path_buf();
            let dest_path = dest_path.to_path_buf();
            Box::pin(copy_dir_all(&path, &dest_path)).await?;
        } else {
            tokio::fs::copy(&path, &dest_path).await?;
        }
    }
    Ok(())
}
