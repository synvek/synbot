//! Onboard command - Initialize configuration and workspace.
//!
//! Role templates are embedded at compile time from `templates/roles/`. Adding a new role
//! only requires a new subdirectory under `templates/roles/` and a rebuild.
//! Skill templates are embedded from `templates/skills/` and extracted to the config skills dir
//! (e.g. ~/.synbot/skills/) so each skill subdirectory (e.g. skill-creator) is available there.
//!
//! For security, when creating a new config we enable the web dashboard and Basic auth by default:
//! username = "admin", password = a newly generated UUID. The credentials are printed once so the
//! user can save them.

use std::path::Path;

use anyhow::{Context, Result};
use include_dir::{include_dir, Dir};
use uuid::Uuid;

use crate::config::{self, WebAuthConfig};

/// Role templates embedded at compile time (templates/roles/).
static TEMPLATES_ROLES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/roles");

/// Skill templates embedded at compile time (templates/skills/). Each subdir (e.g. skill-creator) is a skill.
static TEMPLATES_SKILLS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/skills");

/// Config JSON schema embedded at compile time; written to ~/.synbot for editor/IDE validation.
static CONFIG_SCHEMA_JSON: &str = include_str!("../../templates/config.schema.json");

pub async fn cmd_onboard() -> Result<()> {
    let cfg_path = config::config_path();
    if cfg_path.exists() {
        println!("Config already exists at {}", cfg_path.display());
        println!("Delete it first if you want to re-initialize.");
        return Ok(());
    }

    let mut cfg = config::Config::default();
    // Use workspace under current root (important when using --root-dir).
    cfg.main_agent.workspace = config::config_dir().join("workspace").to_string_lossy().into_owned();

    // Enable web dashboard with auth by default for security (username=admin, password=random UUID).
    let web_password = Uuid::new_v4().to_string();
    cfg.web.enabled = true;
    cfg.web.auth = Some(WebAuthConfig {
        username: "admin".to_string(),
        password: web_password.clone(),
    });

    config::save_config(&cfg, None)?;
    println!("✓ Created config at {}", cfg_path.display());

    let dir = config::config_dir();
    std::fs::create_dir_all(&dir)?;
    let schema_path = dir.join("config.schema.json");
    std::fs::write(&schema_path, CONFIG_SCHEMA_JSON)?;
    println!("✓ Created config schema at {}", schema_path.display());

    let ws = config::workspace_path(&cfg);
    std::fs::create_dir_all(&ws)?;
    println!("✓ Created workspace at {}", ws.display());

    create_roles_templates()?;
    println!("✓ Created role templates at {}", config::roles_dir().display());

    create_skills_templates()?;
    println!("✓ Created skill templates at {}", config::skills_dir().display());

    println!("\n🔐 Web dashboard is enabled with authentication. Save these credentials:");
    println!("   ┌─────────────────────────────────────────────────────────────────┐");
    println!("   │  Username:  admin                                                │");
    println!("   │  Password:  {:<36}  │", web_password);
    println!("   └─────────────────────────────────────────────────────────────────┘");
    println!("   The password is stored in {} and will not be shown again.", cfg_path.display());

    println!("\n🐈 synbot is ready!");
    println!("\nNext steps:");
    println!("  1. Add your API key to {}", cfg_path.display());
    println!("  2. Chat: synbot agent -m \"Hello!\"");
    println!("  3. Start with web: synbot start (then open the dashboard and log in with the credentials above)");
    Ok(())
}

/// Extract an embedded Dir to the filesystem. Creates dirs and overwrites existing files.
/// `dest` is the root destination; we always use dest.join(file.path()) because include_dir
/// returns paths relative to the embedded root, so we must not append subdir.path() again when recursing.
fn extract_embedded_dir(embed: &Dir, dest: &Path) -> Result<()> {
    for file in embed.files() {
        let fpath = dest.join(file.path());
        if let Some(parent) = fpath.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&fpath, file.contents())
            .with_context(|| format!("write {}", fpath.display()))?;
    }
    for subdir in embed.dirs() {
        extract_embedded_dir(subdir, dest)?;
    }
    Ok(())
}

/// Copy embedded templates/roles into ~/.synbot/roles/. Existing files are overwritten.
/// All content is compiled into the binary; no templates directory is needed at runtime.
fn create_roles_templates() -> Result<()> {
    let dest = config::roles_dir();
    std::fs::create_dir_all(&dest)?;
    extract_embedded_dir(&TEMPLATES_ROLES, &dest)?;
    Ok(())
}

/// Copy embedded templates/skills into the config skills dir (e.g. ~/.synbot/skills/).
/// Each subdirectory (e.g. skill-creator) is extracted as a skill. Existing files are overwritten.
fn create_skills_templates() -> Result<()> {
    let dest = config::skills_dir();
    std::fs::create_dir_all(&dest)?;
    extract_embedded_dir(&TEMPLATES_SKILLS, &dest)?;
    Ok(())
}
