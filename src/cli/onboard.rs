//! Onboard command - Initialize configuration and workspace.

use anyhow::Result;
use crate::config;

pub async fn cmd_onboard() -> Result<()> {
    let cfg_path = config::config_path();
    if cfg_path.exists() {
        println!("Config already exists at {}", cfg_path.display());
        println!("Delete it first if you want to re-initialize.");
        return Ok(());
    }

    let cfg = config::Config::default();
    config::save_config(&cfg, None)?;
    println!("✓ Created config at {}", cfg_path.display());

    let ws = config::workspace_path(&cfg);
    std::fs::create_dir_all(&ws)?;
    create_workspace_templates(&ws)?;
    println!("✓ Created workspace at {}", ws.display());

    println!("\n🐈 synbot is ready!");
    println!("\nNext steps:");
    println!("  1. Add your API key to {}", cfg_path.display());
    println!("  2. Chat: synbot agent -m \"Hello!\"");
    Ok(())
}

fn create_workspace_templates(ws: &std::path::Path) -> Result<()> {
    let templates = [
        ("AGENTS.md", "# Agent Instructions\n\nYou are a helpful AI assistant. Be concise, accurate, and friendly.\n"),
        ("SOUL.md", "# Soul\n\nI am synbot, a personal AI assistant.\n\n## Personality\n\n- Helpful and friendly\n- Concise and to the point\n"),
        ("USER.md", "# User Profile\n\n(Add information about yourself here.)\n"),
        ("TOOLS.md", "# Available Tools\n\nSee tool definitions provided by the agent runtime.\n"),
        ("HEARTBEAT.md", "# Heartbeat Tasks\n\n<!-- Add periodic tasks below -->\n"),
    ];
    for (name, content) in templates {
        let path = ws.join(name);
        if !path.exists() {
            std::fs::write(&path, content)?;
        }
    }
    std::fs::create_dir_all(ws.join("memory"))?;
    std::fs::create_dir_all(ws.join("skills"))?;
    Ok(())
}
