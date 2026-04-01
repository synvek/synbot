//! Sandbox launcher command: start app sandbox and run `synbot <child_args>` inside it.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::PathBuf;
use tracing::info;

/// `synbot sandbox …` uses a trailing var-arg for everything after `sandbox`, so `--root-dir`
/// placed after `sandbox` is **not** parsed as the global `Cli.root_dir`. Extract it here and
/// apply [`crate::config::set_root_dir`] before loading config, and strip it from argv forwarded
/// to the child (the child gets `--root-dir` re-prepended from the override when needed).
fn strip_root_dir_from_child_args(args: Vec<String>) -> (Option<PathBuf>, Vec<String>) {
    let mut root: Option<PathBuf> = None;
    let mut out = Vec::with_capacity(args.len());
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--root-dir" {
            if i + 1 < args.len() {
                root = Some(PathBuf::from(&args[i + 1]));
                i += 2;
                continue;
            }
        } else if let Some(rest) = args[i].strip_prefix("--root-dir=") {
            if !rest.is_empty() {
                root = Some(PathBuf::from(rest));
                i += 1;
                continue;
            }
        }
        out.push(args[i].clone());
        i += 1;
    }
    (root, out)
}

fn apply_root_dir_from_sandbox_child_args(child_args: Vec<String>) -> Vec<String> {
    let (inline_root, filtered) = strip_root_dir_from_child_args(child_args);
    if crate::config::get_root_dir_override().is_none() {
        if let Some(root) = inline_root {
            crate::config::set_root_dir(Some(root));
        }
    }
    filtered
}

fn progress(msg: &str) {
    let _ = eprintln!("[synbot sandbox] {}", msg);
    let _ = std::io::stderr().flush();
}

/// Build argv for the child process: prepend --root-dir if this instance was started with one,
/// so the child uses the same workspace.
fn child_argv(child_args: &[String]) -> Vec<String> {
    let base: Vec<String> = if child_args.is_empty() {
        vec!["start".to_string()]
    } else {
        child_args.to_vec()
    };
    if let Some(root) = crate::config::get_root_dir_override() {
        let mut out = vec![
            "--root-dir".to_string(),
            root.to_string_lossy().into_owned(),
        ];
        out.extend(base);
        out
    } else {
        base
    }
}

/// Run the given subcommand and args inside the app sandbox.
/// Example: cmd_sandbox(vec!["start".into()]) → starts sandbox, then runs `synbot start` in it.
/// If child_args is ["setup"], on Windows only: install firewall/WFP rules then exit (no daemon).
pub async fn cmd_sandbox(child_args: Vec<String>) -> Result<()> {
    let child_args = apply_root_dir_from_sandbox_child_args(child_args);
    progress("Loading config...");
    let cfg = crate::config::load_config(None).context("Load config for sandbox")?;

    let app_cfg = cfg
        .app_sandbox
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("app_sandbox is not configured; add it to config to use `synbot sandbox`"))?;

    let monitoring = &cfg.sandbox_monitoring;
    progress("Building sandbox config...");
    let sandbox_config =
        crate::config::build_app_sandbox_config(app_cfg, &cfg, monitoring).context("Build app sandbox config")?;

    // Windows-only: setup adds firewall/WFP rules once (run as Administrator). After that, normal users can start the sandbox.
    if child_args.get(0).map(|s| s.as_str()) == Some("setup") {
        return run_sandbox_setup(&cfg, &sandbox_config).await;
    }

    #[cfg(target_os = "windows")]
    {
        run_sandbox_windows(&sandbox_config, &child_args).await
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        run_sandbox_nono(&sandbox_config, &child_args).await
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        let _ = (sandbox_config, child_args);
        anyhow::bail!("`synbot sandbox` (app sandbox launcher) is not supported on this platform.");
    }
}

/// Run `synbot sandbox setup`: add firewall and WFP rules for AppContainer(s) (Windows only).
/// Installs rules for **app** sandbox always; if `toolSandbox.sandboxType` is `appcontainer`, also installs rules for the tool sandbox profile (separate AppContainer SID).
/// On Windows: requires Administrator. On other platforms: no-op with a message.
async fn run_sandbox_setup(
    cfg: &crate::config::Config,
    app_sandbox_config: &crate::sandbox::SandboxConfig,
) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        progress("Installing firewall and WFP rules for app AppContainer (requires Administrator)...");
        // Only grant parent/ancestor traverse ACLs when the **tool sandbox** also uses AppContainer.
        // If tool sandbox is not AppContainer (or not enabled), keep ACL edits scoped to the target folders.
        let tool_is_appcontainer = cfg
            .tool_sandbox
            .as_ref()
            .and_then(|t| t.sandbox_type.as_deref())
            .map(|s| s.eq_ignore_ascii_case("appcontainer"))
            .unwrap_or(false);
        crate::sandbox::windows_appcontainer::install_windows_sandbox_network_rules(
            app_sandbox_config.clone(),
            tool_is_appcontainer,
        )
            .context("Install Windows app sandbox network rules")?;

        if let Some(ref tool_cfg) = cfg.tool_sandbox {
            let st = tool_cfg.sandbox_type.as_deref().unwrap_or("gvisor-docker");
            if st == "appcontainer" {
                let workspace_path = crate::config::effective_workspace_path(cfg);
                let skills_dir = crate::config::skills_dir();
                match crate::config::build_tool_sandbox_config(
                    tool_cfg,
                    &cfg.sandbox_monitoring,
                    &workspace_path,
                    &skills_dir,
                ) {
                    Ok(tool_sandbox_config) => {
                        progress("Installing firewall and WFP rules for tool AppContainer (tool sandbox)...");
                        // Tool sandbox is AppContainer: it needs parent/ancestor traverse ACLs so the
                        // AppContainer SID can reach configured paths.
                        crate::sandbox::windows_appcontainer::install_windows_sandbox_network_rules(
                            tool_sandbox_config,
                            true,
                        )
                        .context("Install Windows tool sandbox network rules")?;
                    }
                    Err(e) => {
                        progress(&format!(
                            "Skipping tool sandbox setup (invalid tool sandbox config): {}",
                            e
                        ));
                    }
                }
            }
        }

        progress("Rules installed. You can run `synbot sandbox start` and tool sandbox as a normal user.");
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (cfg, app_sandbox_config);
        progress("setup is only needed on Windows (AppContainer). On this platform you can run `synbot sandbox start` directly.");
        Ok(())
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
    let args = child_argv(child_args);
    info!(exe = %exe.display(), args = ?args, "Spawning child in sandbox");

    let code = sandbox.spawn_child_in_container(&exe, &args)?;
    progress(&format!("Child exited with code {}", code));
    std::process::exit(code);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
async fn run_sandbox_nono(
    sandbox_config: &crate::sandbox::SandboxConfig,
    child_args: &[String],
) -> Result<()> {
    use crate::sandbox::nono::build_nono_capability_set;
    use nix::sys::signal::{self, Signal};
    use nix::sys::wait::WaitStatus;
    use nix::unistd::{execv, fork, ForkResult};
    use std::ffi::CString;
    use tokio::signal::unix::{signal, SignalKind};

    let exe = std::env::current_exe().context("Current executable path")?;
    progress("Building nono capability set...");
    let caps = build_nono_capability_set(sandbox_config, &exe)?;
    if !nono::Sandbox::is_supported() {
        anyhow::bail!("nono sandbox is not supported on this platform (need Landlock on Linux or Seatbelt on macOS)");
    }
    progress("Starting nono sandbox (fork+apply+exec)...");
    info!(sandbox_id = %sandbox_config.sandbox_id, "App sandbox started");
    let args = child_argv(child_args);
    info!(exe = %exe.display(), args = ?args, "Spawning child in nono sandbox");

    let pid = match unsafe { fork() } {
        Ok(ForkResult::Parent { child, .. }) => child,
        Ok(ForkResult::Child) => {
            // Child: apply sandbox, set env, exec synbot (never returns on success)
            if let Err(e) = nono::Sandbox::apply(&caps) {
                eprintln!("[synbot sandbox] nono Sandbox::apply failed: {}", e);
                std::process::exit(1);
            }
            std::env::set_var("SYNBOT_IN_APP_SANDBOX", "1");
            let exe_c = match CString::new(exe.to_string_lossy().as_bytes().to_vec()) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[synbot sandbox] exe CString: {}", e);
                    std::process::exit(1);
                }
            };
            let argv: Vec<CString> = std::iter::once(exe_c.clone())
                .chain(args.iter().map(|s| CString::new(s.as_bytes().to_vec()).unwrap()))
                .collect();
            let argv_ref: Vec<&std::ffi::CStr> = argv.iter().map(|c| c.as_c_str()).collect();
            let _ = execv(exe_c.as_c_str(), &argv_ref);
            eprintln!("[synbot sandbox] exec failed: {}", std::io::Error::last_os_error());
            std::process::exit(1);
        }
        Err(e) => anyhow::bail!("fork failed: {}", e),
    };

    fn exit_code_from_wait(status: std::result::Result<WaitStatus, nix::errno::Errno>) -> i32 {
        match status {
            Ok(WaitStatus::Exited(_, code)) => code,
            Ok(WaitStatus::Signaled(_, sig, _)) => 128 + sig as i32,
            Ok(_) => 1,
            Err(_) => 130,
        }
    }

    let mut join = tokio::task::spawn_blocking(move || nix::sys::wait::waitpid(pid, None));

    let mut sigterm = signal(SignalKind::terminate())
        .context("Register SIGTERM handler")?;

    let code = tokio::select! {
        res = &mut join => exit_code_from_wait(res.unwrap_or(Err(nix::errno::Errno::ECHILD))),
        _ = tokio::signal::ctrl_c() => {
            let _ = signal::kill(pid, Signal::SIGINT);
            exit_code_from_wait(join.await.unwrap_or(Err(nix::errno::Errno::ECHILD)))
        }
        _ = sigterm.recv() => {
            let _ = signal::kill(pid, Signal::SIGTERM);
            exit_code_from_wait(join.await.unwrap_or(Err(nix::errno::Errno::ECHILD)))
        }
    };

    progress(&format!("Child exited with code {}", code));
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::strip_root_dir_from_child_args;
    use std::path::Path;

    #[test]
    fn strip_root_dir_removes_flag_and_value() {
        let (root, rest) = strip_root_dir_from_child_args(vec![
            "setup".into(),
            "--root-dir".into(),
            r"c:\synbot".into(),
        ]);
        assert_eq!(root.as_ref().map(|p| p.as_path()), Some(Path::new(r"c:\synbot")));
        assert_eq!(rest, vec!["setup".to_string()]);
    }

    #[test]
    fn strip_root_dir_equals_form() {
        let (root, rest) =
            strip_root_dir_from_child_args(vec![r"--root-dir=d:\inst".into(), "start".into()]);
        assert_eq!(root.as_ref().map(|p| p.as_path()), Some(Path::new(r"d:\inst")));
        assert_eq!(rest, vec!["start".to_string()]);
    }
}
