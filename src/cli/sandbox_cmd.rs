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
    let args: Vec<String> = if child_args.is_empty() {
        vec!["start".to_string()]
    } else {
        child_args.to_vec()
    };
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
