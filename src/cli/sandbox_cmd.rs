//! Sandbox launcher command: start app sandbox and run `synbot <child_args>` inside it.

use anyhow::{Context, Result};
use std::io::Write;
use tracing::info;

fn progress(msg: &str) {
    let _ = eprintln!("[synbot sandbox] {}", msg);
    let _ = std::io::stderr().flush();
}

/// Run the given subcommand and args inside the app sandbox.
/// Example: cmd_sandbox(vec!["start".into()]) → starts sandbox, then runs `synbot start` in it.
pub async fn cmd_sandbox(child_args: Vec<String>) -> Result<()> {
    progress("Loading config...");
    let cfg = crate::config::load_config(None).context("Load config for sandbox")?;

    let app_cfg = cfg
        .app_sandbox
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("app_sandbox is not configured; add it to config to use `synbot sandbox`"))?;

    let monitoring = &cfg.sandbox_monitoring;
    progress("Building sandbox config...");
    let sandbox_config =
        crate::config::build_app_sandbox_config(app_cfg, monitoring).context("Build app sandbox config")?;

    #[cfg(target_os = "windows")]
    {
        run_sandbox_windows(&sandbox_config, &child_args).await
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (sandbox_config, child_args);
        anyhow::bail!("`synbot sandbox` (app sandbox launcher) is currently only supported on Windows. On Linux/macOS use tool_sandbox or run without sandbox.");
    }
}

#[cfg(target_os = "windows")]
async fn run_sandbox_windows(
    sandbox_config: &crate::sandbox::SandboxConfig,
    child_args: &[String],
) -> Result<()> {
    use crate::sandbox::sandbox_trait::Sandbox;
    use crate::sandbox::WindowsAppContainerSandbox;

    progress("Starting AppContainer...");
    let mut sandbox = WindowsAppContainerSandbox::new(sandbox_config.clone())?;
    sandbox.start()?;
    progress("AppContainer started, spawning child process...");
    info!(sandbox_id = %sandbox_config.sandbox_id, "App sandbox started");

    let exe = std::env::current_exe().context("Current executable path")?;
    let args: Vec<String> = if child_args.is_empty() {
        vec!["start".to_string()]
    } else {
        child_args.to_vec()
    };
    info!(exe = %exe.display(), args = ?args, "Spawning child in sandbox");

    let code = sandbox.spawn_child_in_container(&exe, &args)?;
    progress(&format!("Child exited with code {}", code));
    std::process::exit(code);
}
