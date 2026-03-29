//! macOS tool sandbox using `/usr/bin/sandbox-exec` and a generated Seatbelt profile (`.sb`).
//!
//! `allowed_hosts` / `allowed_ports` from config are not expressed in the profile; only network
//! on/off is applied. Tighten policy by setting `toolSandbox.network.enabled` to false.

#![cfg(target_os = "macos")]

use super::error::{Result, SandboxError};
use super::sandbox_trait::Sandbox;
use super::types::{
    ExecutionResult, HealthStatus, SandboxConfig, SandboxInfo, SandboxState, SandboxStatus,
};
use chrono::Utc;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn escape_sb_subpath(p: &str) -> String {
    p.replace('\\', "\\\\").replace('"', "\\\"")
}

fn build_seatbelt_profile(config: &SandboxConfig) -> String {
    let mut lines: Vec<String> = vec![
        "(version 1)".to_string(),
        "(deny default)".to_string(),
        "(allow process-exec)".to_string(),
        "(allow process-fork)".to_string(),
        "(allow signal (target self))".to_string(),
        "(allow sysctl-read)".to_string(),
        "(allow file-read-metadata)".to_string(),
    ];

    let system_read: &[&str] = &[
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/System",
        "/private/etc",
        "/private/var/db",
        "/private/var/folders",
        "/private/tmp",
        "/tmp",
        "/var",
        "/dev",
        "/Library",
    ];
    for p in system_read {
        if Path::new(p).exists() {
            lines.push(format!(
                "(allow file-read* (subpath \"{}\"))",
                escape_sb_subpath(p)
            ));
        }
    }

    for p in &config.filesystem.readonly_paths {
        if !p.is_empty() {
            lines.push(format!(
                "(allow file-read* (subpath \"{}\"))",
                escape_sb_subpath(p)
            ));
        }
    }
    for p in &config.filesystem.writable_paths {
        if !p.is_empty() {
            let e = escape_sb_subpath(p);
            lines.push(format!("(allow file-read* (subpath \"{}\"))", e));
            lines.push(format!("(allow file-write* (subpath \"{}\"))", e));
        }
    }

    if config.network.enabled {
        lines.push("(allow network*)".to_string());
    } else {
        lines.push("(deny network*)".to_string());
    }

    lines.join("\n")
}

/// Tool sandbox on macOS via `sandbox-exec(1)`.
pub struct MacosSandboxExecToolSandbox {
    config: SandboxConfig,
    status: SandboxStatus,
    /// Last generated profile path (removed on stop / drop).
    profile_path: Option<PathBuf>,
}

impl MacosSandboxExecToolSandbox {
    pub fn new(config: SandboxConfig) -> Result<Self> {
        let status = SandboxStatus {
            sandbox_id: config.sandbox_id.clone(),
            state: SandboxState::Created,
            created_at: Utc::now(),
            started_at: None,
            stopped_at: None,
            error: None,
        };
        Ok(Self {
            config,
            status,
            profile_path: None,
        })
    }

    fn sandbox_exec_path() -> &'static str {
        "/usr/bin/sandbox-exec"
    }

    fn write_profile(&mut self) -> Result<PathBuf> {
        let dir = std::env::temp_dir();
        let name = format!(
            "synbot-seatbelt-{}.sb",
            self.config.sandbox_id.replace(|c: char| !c.is_alphanumeric(), "_")
        );
        let path = dir.join(name);
        let body = build_seatbelt_profile(&self.config);
        let mut f = fs::File::create(&path).map_err(|e| {
            SandboxError::CreationFailed(format!("seatbelt profile create {}: {}", path.display(), e))
        })?;
        f.write_all(body.as_bytes()).map_err(|e| {
            SandboxError::CreationFailed(format!("seatbelt profile write {}: {}", path.display(), e))
        })?;
        self.profile_path = Some(path.clone());
        Ok(path)
    }

    fn remove_profile(&mut self) {
        if let Some(p) = self.profile_path.take() {
            let _ = fs::remove_file(&p);
        }
    }
}

impl Drop for MacosSandboxExecToolSandbox {
    fn drop(&mut self) {
        self.remove_profile();
    }
}

impl Sandbox for MacosSandboxExecToolSandbox {
    fn start(&mut self) -> Result<()> {
        if self.status.state == SandboxState::Running {
            return Ok(());
        }
        if !Path::new(Self::sandbox_exec_path()).exists() {
            return Err(SandboxError::CreationFailed(format!(
                "{} not found; seatbelt tool sandbox requires macOS sandbox-exec",
                Self::sandbox_exec_path()
            )));
        }
        self.status.state = SandboxState::Starting;
        let _path = self.write_profile()?;
        self.status.state = SandboxState::Running;
        self.status.started_at = Some(Utc::now());
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.status.state = SandboxState::Stopping;
        self.remove_profile();
        self.status.state = SandboxState::Stopped;
        self.status.stopped_at = Some(Utc::now());
        Ok(())
    }

    fn execute(
        &self,
        command: &str,
        args: &[String],
        timeout: Duration,
        working_dir: Option<&str>,
    ) -> Result<ExecutionResult> {
        if self.status.state != SandboxState::Running {
            return Err(SandboxError::NotStarted);
        }
        let profile = self
            .profile_path
            .as_ref()
            .ok_or_else(|| SandboxError::ExecutionFailed("seatbelt profile missing; call start()".to_string()))?;

        let mut cmd = Command::new(Self::sandbox_exec_path());
        cmd.arg("-f").arg(profile).arg("--").arg(command);
        cmd.args(args);
        if let Some(wd) = working_dir {
            if !wd.is_empty() {
                cmd.current_dir(wd);
            }
        }
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let start = Instant::now();
        let mut child = cmd.spawn().map_err(|e| {
            SandboxError::ExecutionFailed(format!("sandbox-exec spawn failed: {}", e))
        })?;

        let stdout_handle = child.stdout.take();
        let stderr_handle = child.stderr.take();
        let stdout_j = std::thread::spawn(move || {
            let mut v = Vec::new();
            if let Some(mut out) = stdout_handle {
                let _ = out.read_to_end(&mut v);
            }
            v
        });
        let stderr_j = std::thread::spawn(move || {
            let mut v = Vec::new();
            if let Some(mut err) = stderr_handle {
                let _ = err.read_to_end(&mut v);
            }
            v
        });

        let exit_code = loop {
            if start.elapsed() > timeout {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_j.join();
                let _ = stderr_j.join();
                return Err(SandboxError::Timeout);
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    break status.code().unwrap_or(-1);
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                Err(e) => {
                    let _ = stdout_j.join();
                    let _ = stderr_j.join();
                    return Err(SandboxError::ExecutionFailed(format!("wait failed: {}", e)));
                }
            }
        };

        let stdout = stdout_j.join().unwrap_or_default();
        let stderr = stderr_j.join().unwrap_or_default();
        let duration = start.elapsed();

        Ok(ExecutionResult {
            exit_code,
            stdout,
            stderr,
            duration,
            error: None,
        })
    }

    fn get_status(&self) -> SandboxStatus {
        self.status.clone()
    }

    fn health_check(&self) -> HealthStatus {
        let mut checks = HashMap::new();
        let exe_ok = Path::new(Self::sandbox_exec_path()).exists();
        checks.insert("sandbox_exec".to_string(), exe_ok);
        let running = self.status.state == SandboxState::Running;
        checks.insert("running".to_string(), running);
        HealthStatus {
            healthy: exe_ok && running,
            checks,
            message: if exe_ok && running {
                "seatbelt tool sandbox ok".to_string()
            } else {
                "seatbelt tool sandbox not healthy".to_string()
            },
        }
    }

    fn get_info(&self) -> SandboxInfo {
        SandboxInfo {
            sandbox_id: self.config.sandbox_id.clone(),
            platform: "macos".to_string(),
            sandbox_type: "seatbelt".to_string(),
        }
    }
}
