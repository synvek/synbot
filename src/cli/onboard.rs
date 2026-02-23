//! Onboard command - Initialize configuration and workspace.
//!
//! Role templates are embedded at compile time from `templates/roles/`. Adding a new role
//! only requires a new subdirectory under `templates/roles/` and a rebuild.

use std::path::Path;

use anyhow::{Context, Result};
use include_dir::{include_dir, Dir};

use crate::config;

/// Role templates embedded at compile time (templates/roles/).
static TEMPLATES_ROLES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates/roles");

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

    println!("\n🐈 synbot is ready!");
    println!("\nNext steps:");
    println!("  1. Add your API key to {}", cfg_path.display());
    println!("  2. Chat: synbot agent -m \"Hello!\"");
    Ok(())
}

/// Extract an embedded Dir to the filesystem. Creates dirs and overwrites existing files.
fn extract_embedded_dir(embed: &Dir, dest: &Path) -> Result<()> {
    for subdir in embed.dirs() {
        let d = dest.join(subdir.path());
        std::fs::create_dir_all(&d)?;
        extract_embedded_dir(subdir, &d)?;
    }
    for file in embed.files() {
        let fpath = dest.join(file.path());
        if let Some(parent) = fpath.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&fpath, file.contents())
            .with_context(|| format!("write {}", fpath.display()))?;
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
