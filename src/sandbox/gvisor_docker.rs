// gVisor Docker sandbox implementation for tool execution

use super::error::{Result, SandboxError};
use super::sandbox_trait::Sandbox;
use super::types::{
    ExecutionResult, HealthStatus, SandboxConfig, SandboxInfo, SandboxState, SandboxStatus,
};
use super::plain_docker::connect_docker;
use bollard::container::{
    Config, CreateContainerOptions, KillContainerOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::models::HostConfig;
use bollard::Docker;
use chrono::Utc;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout as tokio_timeout;

/// gVisor Docker sandbox implementation
/// 
/// This sandbox uses Docker with gVisor (runsc) runtime to provide
/// secure isolation for tool execution. It supports:
/// - Network isolation
/// - Filesystem isolation via volume mounts
/// - Resource limits (CPU, memory, disk)
/// - Execution timeout control
pub struct GVisorDockerSandbox {
    config: SandboxConfig,
    docker: Docker,
    container_id: Option<String>,
    status: SandboxStatus,
}

impl GVisorDockerSandbox {
    /// Create a new gVisor Docker sandbox instance
    /// 
    /// # Arguments
    /// 
    /// * `config` - Sandbox configuration
    /// 
    /// # Returns
    /// 
    /// Returns a new `GVisorDockerSandbox` instance
    /// 
    /// # Errors
    /// 
    /// Returns an error if Docker connection cannot be established
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
    
    /// Get network mode based on configuration
    fn get_network_mode(&self) -> String {
        if self.config.network.enabled {
            "bridge".to_string()
        } else {
            "none".to_string()
        }
    }
    
    /// Get volume bindings from configuration (writable_paths as host:host, plus workspace at /workspace if set)
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
        if let Some((ref host, ref guest)) = self.config.filesystem.skills_mount {
            binds.push(format!("{}:{}:ro", host, guest));
        }
        binds
    }
    
    /// Get required capabilities (minimal set for security)
    fn get_required_capabilities(&self) -> Vec<String> {
        // Return minimal capability set
        // Most capabilities are dropped for security
        vec![]
    }
}

impl Sandbox for GVisorDockerSandbox {
    fn start(&mut self) -> Result<()> {
        self.status.state = SandboxState::Starting;

        let docker = self.docker.clone();
        let sandbox_id = self.config.sandbox_id.clone();
        let delete_on_start = self.config.delete_on_start;
        let network_mode = self.get_network_mode();
        let binds = self.get_volumes();
        let memory = self.config.resources.max_memory as i64;
        let nano_cpus = (self.config.resources.max_cpu * 1_000_000_000.0) as i64;
        let pids_limit = self.config.process.max_processes as i64;
        let cap_add = self.get_required_capabilities();
        let image = self
            .config
            .image
            .as_deref()
            .unwrap_or("ubuntu:22.04")
            .to_string();

        // Run Docker async work in a blocking thread to avoid "runtime within runtime" when
        // start() is called from an async context (e.g. init_sandbox_if_configured).
        // Use async move + cloned Docker only — borrowing self across .await inside block_on
        // can trigger rustc internal errors on some toolchains.
        let container_id = tokio::task::block_in_place(|| {
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|e| SandboxError::CreationFailed(format!("Failed to create runtime: {}", e)))?;

            runtime.block_on(async move {
                if delete_on_start {
                    let _ = docker
                        .remove_container(
                            &sandbox_id,
                            Some(RemoveContainerOptions {
                                force: true,
                                ..Default::default()
                            }),
                        )
                        .await;
                } else if let Ok(existing) = docker.inspect_container(&sandbox_id, None).await {
                    let id = existing
                        .id
                        .as_deref()
                        .unwrap_or(sandbox_id.as_str())
                        .to_string();
                    let running = existing
                        .state
                        .as_ref()
                        .and_then(|s| s.running)
                        .unwrap_or(false);
                    if !running {
                        docker
                            .start_container(&id, None::<StartContainerOptions<String>>)
                            .await
                            .map_err(|e| {
                                SandboxError::CreationFailed(format!(
                                    "Failed to start existing container: {}",
                                    e
                                ))
                            })?;
                    }
                    return Ok::<String, SandboxError>(id);
                }

                let options = CreateContainerOptions {
                    name: sandbox_id.clone(),
                    platform: None,
                };

                let host_config = HostConfig {
                    runtime: Some("runsc".to_string()),
                    network_mode: Some(network_mode),
                    binds: Some(binds),
                    memory: Some(memory),
                    nano_cpus: Some(nano_cpus),
                    // Docker's pids controller limits the whole container tree.
                    pids_limit: Some(pids_limit),
                    security_opt: Some(vec!["no-new-privileges".to_string()]),
                    cap_drop: Some(vec!["ALL".to_string()]),
                    cap_add: Some(cap_add),
                    ..Default::default()
                };

                let config = Config {
                    image: Some(image),
                    host_config: Some(host_config),
                    cmd: Some(vec!["sleep".to_string(), "infinity".to_string()]),
                    ..Default::default()
                };

                let container = docker
                    .create_container(Some(options), config)
                    .await
                    .map_err(|e| SandboxError::CreationFailed(format!("Failed to create container: {}", e)))?;

                let cid = container.id.clone();
                docker
                    .start_container(&cid, None::<StartContainerOptions<String>>)
                    .await
                    .map_err(|e| SandboxError::CreationFailed(format!("Failed to start container: {}", e)))?;

                Ok(cid)
            })
        })?;

        self.container_id = Some(container_id);
        self.status.state = SandboxState::Running;
        self.status.started_at = Some(Utc::now());
        self.status.error = None;

        Ok(())
    }
    
    fn stop(&mut self) -> Result<()> {
        let Some(container_id) = self.container_id.clone() else {
            return Ok(());
        };

        self.status.state = SandboxState::Stopping;
        let docker = self.docker.clone();

        let exit_reason = tokio::task::block_in_place(|| {
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to create runtime: {}", e)))?;
            runtime.block_on(async move {
                docker.stop_container(&container_id, None::<StopContainerOptions>).await.ok();
                let reason = docker.inspect_container(&container_id, None).await.ok().and_then(|info| {
                    info.state.and_then(|state| state.error.filter(|reason| !reason.is_empty()).or_else(|| {
                        if state.oom_killed == Some(true) {
                            Some("container was killed by the OOM killer".to_string())
                        } else if let Some(exit_code) = state.exit_code {
                            Some(format!("container exited with code {}", exit_code))
                        } else {
                            state.status.map(|status| format!("container exited with status {}", status))
                        }
                    }))
                });
                docker.remove_container(&container_id, Some(RemoveContainerOptions {
                    force: true, ..Default::default()
                })).await.ok();
                Ok::<Option<String>, SandboxError>(reason)
            })
        })?;

        // The container is removed by stop(), so never retain its stale ID.
        self.container_id = None;
        self.status.error = exit_reason;
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
        let container_id = self.container_id.as_ref().ok_or(SandboxError::NotStarted)?.clone();
        let docker = self.docker.clone();
        let cmd: Vec<String> = std::iter::once(command.to_string()).chain(args.iter().cloned()).collect();
        let working_dir = working_dir.map(str::to_string);

        tokio::task::block_in_place(|| {
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to create runtime: {}", e)))?;
            runtime.block_on(async move {
                let exec_config = CreateExecOptions {
                    cmd: Some(cmd), attach_stdout: Some(true), attach_stderr: Some(true),
                    working_dir, ..Default::default()
                };
                let exec = docker.create_exec(&container_id, exec_config).await
                    .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to create exec: {}", e)))?;
                let started_at = std::time::Instant::now();
                let result = tokio_timeout(timeout, async {
                    match docker.start_exec(&exec.id, None).await {
                        Ok(StartExecResults::Attached { output, .. }) => {
                            let mut stdout = Vec::new();
                            let mut stderr = Vec::new();
                            use futures_util::stream::StreamExt;
                            tokio::pin!(output);
                            while let Some(chunk) = output.next().await {
                                match chunk {
                                    Ok(bollard::container::LogOutput::StdOut { message }) => stdout.extend_from_slice(&message),
                                    Ok(bollard::container::LogOutput::StdErr { message }) => stderr.extend_from_slice(&message),
                                    _ => {}
                                }
                            }
                            let inspect = docker.inspect_exec(&exec.id).await
                                .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to inspect exec: {}", e)))?;
                            Ok::<ExecutionResult, SandboxError>(ExecutionResult {
                                exit_code: inspect.exit_code.unwrap_or(-1) as i32, stdout, stderr,
                                duration: started_at.elapsed(), error: None,
                            })
                        }
                        Ok(StartExecResults::Detached) => Err(SandboxError::ExecutionFailed("Docker exec unexpectedly detached".to_string())),
                        Err(e) => Err(SandboxError::ExecutionFailed(format!("Exec failed: {}", e))),
                    }
                }).await;
                match result {
                    Ok(result) => result,
                    Err(_) => {
                        let _ = docker.kill_container(&container_id, None::<KillContainerOptions<String>>).await;
                        let _ = docker.start_container(&container_id, None::<StartContainerOptions<String>>).await;
                        Err(SandboxError::Timeout)
                    }
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
            let cid = container_id.clone();
            let docker = self.docker.clone();
            let container_running = tokio::task::block_in_place(|| {
                let runtime = tokio::runtime::Runtime::new().unwrap();
                runtime.block_on(async move {
                    docker
                        .inspect_container(&cid, None)
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
            sandbox_type: "gvisor-docker".to_string(),
        }
    }
}
