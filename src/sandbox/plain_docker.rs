// Plain Docker sandbox (default runc runtime) for tool execution fallback on Linux/macOS.
// Used when gVisor (runsc) is not available; provides container isolation without gVisor.

use super::error::{Result, SandboxError};
use super::sandbox_trait::Sandbox;
use super::types::{
    ExecutionResult, HealthStatus, SandboxConfig, SandboxInfo, SandboxState, SandboxStatus,
};
use bollard::container::{
    Config, CreateContainerOptions, RemoveContainerOptions, StartContainerOptions,
    StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::HostConfig;
use bollard::{API_DEFAULT_VERSION, Docker};
use chrono::Utc;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout as tokio_timeout;

/// Plain Docker sandbox using default runtime (runc).
/// Fallback when gVisor is not available; exec runs inside a normal Docker container.
pub struct PlainDockerSandbox {
    config: SandboxConfig,
    docker: Docker,
    container_id: Option<String>,
    status: SandboxStatus,
}

/// Connect to the local Docker daemon. Used by both PlainDockerSandbox and GVisorDockerSandbox.
/// - macOS: Docker Desktop uses ~/.docker/run/docker.sock; try that first, then /var/run/docker.sock.
/// - Linux: uses /var/run/docker.sock (connect_with_local_defaults).
/// - Windows: named pipe, then DOCKER_HOST, then tcp://localhost:2375.
pub(crate) fn connect_docker() -> Result<Docker> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let socket = format!("{}/.docker/run/docker.sock", home);
            if std::path::Path::new(&socket).exists() {
                if let Ok(d) = Docker::connect_with_socket(&socket, 120, API_DEFAULT_VERSION) {
                    return Ok(d);
                }
            }
        }
        Docker::connect_with_local_defaults()
            .map_err(|e| SandboxError::CreationFailed(format!("Failed to connect to Docker: {}", e)))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Docker::connect_with_local_defaults()
            .map_err(|e| SandboxError::CreationFailed(format!("Failed to connect to Docker: {}", e)))
    }

    #[cfg(target_os = "windows")]
    {
        let mut last_err = None;

        // 1. Named pipe (default for Docker Desktop)
        match Docker::connect_with_local_defaults() {
            Ok(d) => return Ok(d),
            Err(e) => last_err = Some(e.to_string()),
        }

        // 2. DOCKER_HOST (e.g. tcp://localhost:2375 if user exposed daemon)
        if let Ok(d) = Docker::connect_with_defaults() {
            return Ok(d);
        }

        // 3. Explicit TCP fallback (Docker Desktop: General -> "Expose daemon on tcp://localhost:2375")
        if let Ok(d) = Docker::connect_with_http("http://localhost:2375", 120, API_DEFAULT_VERSION) {
            return Ok(d);
        }

        Err(SandboxError::CreationFailed(format!(
            "Failed to connect to Docker on Windows. {} \
             Ensure Docker Desktop is running. If the named pipe fails, either set DOCKER_HOST=tcp://localhost:2375 \
             or in Docker Desktop enable General -> 'Expose daemon on tcp://localhost:2375 without TLS', then restart.",
            last_err.as_deref().unwrap_or("")
        )))
    }
}

impl PlainDockerSandbox {
    /// Create a new plain Docker sandbox instance.
    pub fn new(config: SandboxConfig) -> Result<Self> {
        let docker = connect_docker()?;

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
            docker,
            container_id: None,
            status,
        })
    }

    fn get_network_mode(&self) -> String {
        if self.config.network.enabled {
            "bridge".to_string()
        } else {
            "none".to_string()
        }
    }

    fn get_volumes(&self) -> Vec<String> {
        let mut binds: Vec<String> = self
            .config
            .filesystem
            .writable_paths
            .iter()
            .map(|p| format!("{}:{}", p, p))
            .collect();
        if let Some((ref host, ref guest)) = self.config.filesystem.workspace_mount {
            binds.push(format!("{}:{}", host, guest));
        }
        binds
    }
}

impl Sandbox for PlainDockerSandbox {
    fn start(&mut self) -> Result<()> {
        self.status.state = SandboxState::Starting;

        tokio::task::block_in_place(|| {
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|e| SandboxError::CreationFailed(format!("Failed to create runtime: {}", e)))?;

            runtime.block_on(async {
                if self.config.delete_on_start {
                    let _ = self
                        .docker
                        .remove_container(
                            &self.config.sandbox_id,
                            Some(RemoveContainerOptions {
                                force: true,
                                ..Default::default()
                            }),
                        )
                        .await;
                } else {
                    if let Ok(existing) = self
                        .docker
                        .inspect_container(&self.config.sandbox_id, None)
                        .await
                    {
                        let id = existing
                            .id
                            .as_deref()
                            .unwrap_or(self.config.sandbox_id.as_str());
                        let running = existing
                            .state
                            .as_ref()
                            .and_then(|s| s.running)
                            .unwrap_or(false);
                        if !running {
                            self.docker
                                .start_container(id, None::<StartContainerOptions<String>>)
                                .await
                                .map_err(|e| SandboxError::CreationFailed(format!("Failed to start existing container: {}", e)))?;
                        }
                        self.container_id = Some(id.to_string());
                        self.status.state = SandboxState::Running;
                        self.status.started_at = Some(Utc::now());
                        return Ok(());
                    }
                }

                let options = CreateContainerOptions {
                    name: self.config.sandbox_id.clone(),
                    platform: None,
                };

                let host_config = HostConfig {
                    runtime: None,
                    network_mode: Some(self.get_network_mode()),
                    binds: Some(self.get_volumes()),
                    memory: Some(self.config.resources.max_memory as i64),
                    nano_cpus: Some((self.config.resources.max_cpu * 1_000_000_000.0) as i64),
                    security_opt: Some(vec!["no-new-privileges".to_string()]),
                    cap_drop: Some(vec!["ALL".to_string()]),
                    cap_add: Some(vec![]),
                    ..Default::default()
                };

                let image = "ubuntu:22.04".to_string();
                let config = Config {
                    image: Some(image),
                    host_config: Some(host_config),
                    cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
                    ..Default::default()
                };

                let container = self.docker
                    .create_container(Some(options), config)
                    .await
                    .map_err(|e| SandboxError::CreationFailed(format!("Failed to create container: {}", e)))?;

                self.container_id = Some(container.id.clone());

                self.docker
                    .start_container(&container.id, None::<StartContainerOptions<String>>)
                    .await
                    .map_err(|e| SandboxError::CreationFailed(format!("Failed to start container: {}", e)))?;

                self.status.state = SandboxState::Running;
                self.status.started_at = Some(Utc::now());

                Ok(())
            })
        })
    }

    fn stop(&mut self) -> Result<()> {
        if self.container_id.is_none() {
            return Ok(());
        }

        self.status.state = SandboxState::Stopping;

        tokio::task::block_in_place(|| {
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to create runtime: {}", e)))?;

            runtime.block_on(async {
                if let Some(container_id) = &self.container_id {
                    self.docker.stop_container(container_id, None::<StopContainerOptions>).await.ok();
                    self.docker
                        .remove_container(
                            container_id,
                            Some(RemoveContainerOptions {
                                force: true,
                                ..Default::default()
                            }),
                        )
                        .await
                        .ok();
                }

                self.status.state = SandboxState::Stopped;
                self.status.stopped_at = Some(Utc::now());

                Ok(())
            })
        })
    }

    fn execute(
        &self,
        command: &str,
        args: &[String],
        timeout: Duration,
        working_dir: Option<&str>,
    ) -> Result<ExecutionResult> {
        let container_id = self
            .container_id
            .as_ref()
            .ok_or(SandboxError::NotStarted)?;

        tokio::task::block_in_place(|| {
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to create runtime: {}", e)))?;

            runtime.block_on(async {
                let mut cmd = vec![command];
                cmd.extend(args.iter().map(|s| s.as_str()));

                let exec_config = CreateExecOptions {
                    cmd: Some(cmd),
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    working_dir,
                    ..Default::default()
                };

                let exec = self.docker
                    .create_exec(container_id, exec_config)
                    .await
                    .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to create exec: {}", e)))?;

                let start = std::time::Instant::now();

                let result = tokio_timeout(
                    timeout,
                    self.docker.start_exec(&exec.id, None),
                )
                .await;

                match result {
                    Ok(Ok(StartExecResults::Attached { output, .. })) => {
                        let mut stdout = Vec::new();
                        let mut stderr = Vec::new();

                        use futures_util::stream::StreamExt;
                        let mut output_stream = output;

                        while let Some(chunk) = output_stream.next().await {
                            match chunk {
                                Ok(bollard::container::LogOutput::StdOut { message }) => {
                                    stdout.extend_from_slice(&message);
                                }
                                Ok(bollard::container::LogOutput::StdErr { message }) => {
                                    stderr.extend_from_slice(&message);
                                }
                                _ => {}
                            }
                        }

                        let inspect = self.docker.inspect_exec(&exec.id).await
                            .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to inspect exec: {}", e)))?;

                        let exit_code = inspect.exit_code.unwrap_or(-1) as i32;

                        Ok(ExecutionResult {
                            exit_code,
                            stdout,
                            stderr,
                            duration: start.elapsed(),
                            error: None,
                        })
                    }
                    Err(_) => Err(SandboxError::Timeout),
                    Ok(Err(e)) => Err(SandboxError::ExecutionFailed(format!("Exec failed: {}", e))),
                    _ => Err(SandboxError::ExecutionFailed("Unexpected exec result".to_string())),
                }
            })
        })
    }

    fn get_status(&self) -> SandboxStatus {
        self.status.clone()
    }

    fn health_check(&self) -> HealthStatus {
        let mut checks = HashMap::new();

        if let Some(container_id) = &self.container_id {
            let container_running = tokio::task::block_in_place(|| {
                let runtime = tokio::runtime::Runtime::new().unwrap();
                runtime.block_on(async {
                    self.docker
                        .inspect_container(container_id, None)
                        .await
                        .map(|info| info.state.and_then(|s| s.running).unwrap_or(false))
                        .unwrap_or(false)
                })
            });

            checks.insert("container_running".to_string(), container_running);

            HealthStatus {
                healthy: container_running,
                checks,
                message: if container_running {
                    "Container is running".to_string()
                } else {
                    "Container is not running".to_string()
                },
            }
        } else {
            checks.insert("container_exists".to_string(), false);

            HealthStatus {
                healthy: false,
                checks,
                message: "Container not created".to_string(),
            }
        }
    }

    fn get_info(&self) -> SandboxInfo {
        SandboxInfo {
            sandbox_id: self.config.sandbox_id.clone(),
            platform: std::env::consts::OS.to_string(),
            sandbox_type: "plain-docker".to_string(),
        }
    }
}
