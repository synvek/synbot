//! Service command — Install, uninstall, start, stop, restart, status for Synbot daemon.
//! Supports Linux (systemd user), macOS (launchd), and Windows (scheduled task).

use anyhow::{Context, Result};
use clap::Subcommand;
use std::path::PathBuf;

use crate::config;

#[derive(Subcommand)]
pub enum ServiceAction {
    /// Install the service (systemd user unit, launchd agent, or Windows scheduled task).
    Install,

    /// Uninstall the service.
    Uninstall,

    /// Start the service (or run the daemon under the service manager).
    Start,

    /// Stop the running service.
    Stop,

    /// Restart the service (stop then start).
    Restart,

    /// Show service status (active/inactive).
    Status,
}

pub async fn cmd_service(action: ServiceAction) -> Result<()> {
    let _ = action;
    #[cfg(target_os = "linux")]
    return run_linux(action).await;
    #[cfg(target_os = "macos")]
    return run_macos(action).await;
    #[cfg(target_os = "windows")]
    return run_windows(action).await;
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    anyhow::bail!("service command is not supported on this platform (Linux, macOS, Windows only)");
}

fn exe_and_args() -> Result<(PathBuf, Vec<String>)> {
    let exe = std::env::current_exe().context("Current executable path")?;
    let root = config::get_root_dir_override();
    let mut args = Vec::new();
    if let Some(ref r) = root {
        args.push("--root-dir".to_string());
        args.push(r.to_string_lossy().to_string());
    }
    args.push("start".to_string());
    Ok((exe, args))
}

#[cfg(target_os = "linux")]
async fn run_linux(action: ServiceAction) -> Result<()> {
    use std::io::Write;
    use std::process::Command;

    let (exe, args) = exe_and_args()?;
    let exe_str = exe.to_string_lossy();
    let args_str = args.join(" ");
    let unit_dir = dirs::config_dir()
        .context("No config dir")?
        .join("systemd/user");
    let unit_path = unit_dir.join("synbot.service");

    match action {
        ServiceAction::Install => {
            std::fs::create_dir_all(&unit_dir).context("Create systemd user dir")?;
            let exec_start = format!("{} {}", exe_str, args_str);
            let unit = format!(
                r#"[Unit]
Description=Synbot daemon
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#,
                exec_start
            );
            std::fs::write(&unit_path, unit).context("Write synbot.service")?;
            let out = Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .output()
                .context("systemctl daemon-reload")?;
            if !out.status.success() {
                anyhow::bail!(
                    "systemctl daemon-reload failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            let out = Command::new("systemctl")
                .args(["--user", "enable", "synbot.service"])
                .output()
                .context("systemctl enable")?;
            if !out.status.success() {
                anyhow::bail!(
                    "systemctl enable failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            println!("✓ Service installed (systemd user): {}", unit_path.display());
        }
        ServiceAction::Uninstall => {
            let _ = Command::new("systemctl")
                .args(["--user", "stop", "synbot.service"])
                .output();
            let _ = Command::new("systemctl")
                .args(["--user", "disable", "synbot.service"])
                .output();
            if unit_path.exists() {
                std::fs::remove_file(&unit_path).context("Remove unit file")?;
            }
            let out = Command::new("systemctl")
                .args(["--user", "daemon-reload"])
                .output()
                .context("systemctl daemon-reload")?;
            if !out.status.success() {
                let _ = std::io::stderr().write_all(&out.stderr);
            }
            println!("✓ Service uninstalled.");
        }
        ServiceAction::Start => {
            let out = Command::new("systemctl")
                .args(["--user", "start", "synbot.service"])
                .output()
                .context("systemctl start")?;
            if !out.status.success() {
                anyhow::bail!(
                    "systemctl start failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            println!("✓ Service started.");
        }
        ServiceAction::Stop => {
            let out = Command::new("systemctl")
                .args(["--user", "stop", "synbot.service"])
                .output()
                .context("systemctl stop")?;
            if !out.status.success() {
                anyhow::bail!(
                    "systemctl stop failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            println!("✓ Service stopped.");
        }
        ServiceAction::Restart => {
            let out = Command::new("systemctl")
                .args(["--user", "restart", "synbot.service"])
                .output()
                .context("systemctl restart")?;
            if !out.status.success() {
                anyhow::bail!(
                    "systemctl restart failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            println!("✓ Service restarted.");
        }
        ServiceAction::Status => {
            let out = Command::new("systemctl")
                .args(["--user", "is-active", "synbot.service"])
                .output()
                .context("systemctl is-active")?;
            let active = out.status.success() && out.stdout.as_slice() == b"active\n";
            if active {
                println!("active");
            } else {
                println!("inactive");
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn run_macos(action: ServiceAction) -> Result<()> {
    use std::process::Command;

    let (exe, args) = exe_and_args()?;
    let plist_dir = dirs::home_dir()
        .context("No home dir")?
        .join("Library/LaunchAgents");
    let plist_path = plist_dir.join("com.synbot.plist");
    let label = "com.synbot";

    match action {
        ServiceAction::Install => {
            std::fs::create_dir_all(&plist_dir).context("Create LaunchAgents dir")?;
            let mut args_xml = String::new();
            args_xml.push_str(&format!("    <string>{}</string>\n", exe.to_string_lossy().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")));
            for a in &args {
                args_xml.push_str(&format!("    <string>{}</string>\n", a.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")));
            }
            let plist = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{}</string>
  <key>ProgramArguments</key>
  <array>
{}
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
</dict>
</plist>
"#,
                label, args_xml
            );
            std::fs::write(&plist_path, plist).context("Write plist")?;
            let out = Command::new("launchctl")
                .args(["load", plist_path.to_string_lossy().as_ref()])
                .output()
                .context("launchctl load")?;
            if !out.status.success() {
                anyhow::bail!(
                    "launchctl load failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            println!("✓ Service installed (launchd): {}", plist_path.display());
        }
        ServiceAction::Uninstall => {
            let _ = Command::new("launchctl")
                .args(["unload", plist_path.to_string_lossy().as_ref()])
                .output();
            if plist_path.exists() {
                std::fs::remove_file(&plist_path).context("Remove plist")?;
            }
            println!("✓ Service uninstalled.");
        }
        ServiceAction::Start => {
            if plist_path.exists() {
                let out = Command::new("launchctl")
                    .args(["load", plist_path.to_string_lossy().as_ref()])
                    .output()
                    .context("launchctl load")?;
                if !out.status.success() {
                    anyhow::bail!(
                        "launchctl load failed: {}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                }
                println!("✓ Service started.");
            } else {
                anyhow::bail!("Service not installed. Run: synbot service install");
            }
        }
        ServiceAction::Stop => {
            let out = Command::new("launchctl")
                .args(["unload", plist_path.to_string_lossy().as_ref()])
                .output()
                .context("launchctl unload")?;
            if !out.status.success() {
                anyhow::bail!(
                    "launchctl unload failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            println!("✓ Service stopped.");
        }
        ServiceAction::Restart => {
            if plist_path.exists() {
                let _ = Command::new("launchctl")
                    .args(["unload", plist_path.to_string_lossy().as_ref()])
                    .output();
                let out = Command::new("launchctl")
                    .args(["load", plist_path.to_string_lossy().as_ref()])
                    .output()
                    .context("launchctl load")?;
                if !out.status.success() {
                    anyhow::bail!(
                        "launchctl load failed: {}",
                        String::from_utf8_lossy(&out.stderr)
                    );
                }
                println!("✓ Service restarted.");
            } else {
                anyhow::bail!("Service not installed. Run: synbot service install");
            }
        }
        ServiceAction::Status => {
            let out2 = Command::new("launchctl")
                .args(["list", label])
                .output()
                .ok();
            let running = out2
                .as_ref()
                .map(|o| o.status.success() && !o.stdout.is_empty())
                .unwrap_or(false);
            // When loaded, first column is PID (digit) or "-" if not running
            let has_pid = out2
                .as_ref()
                .and_then(|o| std::str::from_utf8(o.stdout.as_slice()).ok())
                .map(|s| s.trim_start().chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false))
                .unwrap_or(false);
            if running && has_pid {
                println!("active");
            } else {
                println!("inactive");
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
async fn run_windows(action: ServiceAction) -> Result<()> {
    use std::process::Command;

    let (exe, args) = exe_and_args()?;
    let exe_quoted = if exe.as_os_str().to_string_lossy().contains(' ') {
        format!("\"{}\"", exe.display())
    } else {
        exe.to_string_lossy().to_string()
    };
    let task_cmd = {
        let mut c = exe_quoted.clone();
        for a in &args {
            c.push(' ');
            if a.contains(' ') {
                c.push('"');
                c.push_str(a);
                c.push('"');
            } else {
                c.push_str(a);
            }
        }
        c
    };
    const TASK_NAME: &str = "Synbot";

    match action {
        ServiceAction::Install => {
            let out = Command::new("schtasks")
                .args([
                    "/create",
                    "/tn", TASK_NAME,
                    "/tr", &task_cmd,
                    "/sc", "onlogon",
                    "/rl", "highest",
                    "/f",
                ])
                .output()
                .context("schtasks create")?;
            if !out.status.success() {
                anyhow::bail!(
                    "schtasks create failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            println!("✓ Service installed (scheduled task): {}", TASK_NAME);
        }
        ServiceAction::Uninstall => {
            let _ = run_windows_stop().await;
            let out = Command::new("schtasks")
                .args(["/delete", "/tn", TASK_NAME, "/f"])
                .output()
                .context("schtasks delete")?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("cannot find") || stderr.contains("does not exist") {
                    println!("✓ Service uninstalled (task was not present).");
                    return Ok(());
                }
                anyhow::bail!("schtasks delete failed: {}", stderr);
            }
            println!("✓ Service uninstalled.");
        }
        ServiceAction::Start => {
            let out = Command::new("schtasks")
                .args(["/run", "/tn", TASK_NAME])
                .output()
                .context("schtasks run")?;
            if !out.status.success() {
                anyhow::bail!(
                    "schtasks run failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            println!("✓ Service started.");
        }
        ServiceAction::Stop => {
            run_windows_stop().await?;
            println!("✓ Service stopped.");
        }
        ServiceAction::Restart => {
            run_windows_stop().await?;
            let out = Command::new("schtasks")
                .args(["/run", "/tn", TASK_NAME])
                .output()
                .context("schtasks run")?;
            if !out.status.success() {
                anyhow::bail!(
                    "schtasks run failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
            println!("✓ Service restarted.");
        }
        ServiceAction::Status => {
            let pid_path = config::config_dir().join("synbot.pid");
            if !pid_path.exists() {
                println!("inactive");
                return Ok(());
            }
            let pid_str = std::fs::read_to_string(&pid_path).context("Read PID file")?;
            let pid: u32 = pid_str.trim().parse().context("Parse PID")?;
            // Check if process exists (Windows: OpenProcess with PROCESS_QUERY_LIMITED_INFORMATION)
            let running = is_process_running(pid);
            if running {
                println!("active");
            } else {
                println!("inactive");
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
async fn run_windows_stop() -> Result<()> {
    let pid_path = config::config_dir().join("synbot.pid");
    if !pid_path.exists() {
        return Ok(());
    }
    let pid_str = std::fs::read_to_string(&pid_path).context("Read PID file")?;
    let pid: u32 = pid_str.trim().parse().context("Parse PID")?;
    if !is_process_running(pid) {
        let _ = std::fs::remove_file(&pid_path);
        return Ok(());
    }
    let out = std::process::Command::new("taskkill")
        .args(["/pid", &pid.to_string(), "/f"])
        .output()
        .context("taskkill")?;
    let _ = std::fs::remove_file(&pid_path);
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !stderr.contains("not found") {
            anyhow::bail!("taskkill failed: {}", stderr);
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn is_process_running(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::OpenProcess;
    use windows::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION;

    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid);
        if let Ok(h) = h {
            let _ = CloseHandle(h);
            true
        } else {
            false
        }
    }
}
