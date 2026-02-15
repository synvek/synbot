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

    create_roles_templates()?;
    println!("✓ Created role templates at {}", config::roles_dir().display());

    println!("\n🐈 synbot is ready!");
    println!("\nNext steps:");
    println!("  1. Add your API key to {}", cfg_path.display());
    println!("  2. Chat: synbot agent -m \"Hello!\"");
    Ok(())
}

/// 编译时从 crate 根目录的 templates/ 读取模板内容
fn create_workspace_templates(ws: &std::path::Path) -> Result<()> {
    let templates: &[(&str, &str)] = &[
        ("AGENTS.md", include_str!("../../templates/agent/AGENTS.md")),
        ("SOUL.md", include_str!("../../templates/agent/SOUL.md")),
        ("USER.md", include_str!("../../templates/agent/USER.md")),
        ("TOOLS.md", include_str!("../../templates/agent/TOOLS.md")),
        ("HEARTBEAT.md", include_str!("../../templates/agent/HEARTBEAT.md")),
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

/// 将 templates/roles 下的 role 模板写入 ~/.synbot/roles（固定路径）
fn create_roles_templates() -> Result<()> {
    let roles_root = config::roles_dir();
    let role_files: &[(&str, &str, &str)] = &[
        (
            "dev",
            "AGENTS.md",
            include_str!("../../templates/roles/dev/AGENTS.md"),
        ),
        (
            "dev",
            "SOUL.md",
            include_str!("../../templates/roles/dev/SOUL.md"),
        ),
        (
            "dev",
            "TOOLS.md",
            include_str!("../../templates/roles/dev/TOOLS.md"),
        ),
    ];
    for (role_name, file_name, content) in role_files {
        let dir = roles_root.join(role_name);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(file_name);
        if !path.exists() {
            std::fs::write(&path, content)?;
        }
    }
    Ok(())
}
