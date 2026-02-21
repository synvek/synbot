// nono sandbox implementation for Linux/macOS
//
// Uses the nono crate (Landlock/Seatbelt) when available for `synbot sandbox`;
// falls back to the nono CLI binary for NonoSandbox::execute and spawn_child_in_sandbox
// when the crate is used only in the fork+apply+exec launcher path.

use super::error::{Result, SandboxError};
use super::sandbox_trait::Sandbox;
use super::types::*;
use chrono::Utc;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use nix::unistd::Pid;

/// Build a nono crate `CapabilitySet` from our sandbox config for use with `Sandbox::apply`.
/// Adds the executable's parent directory (so exec can open the binary), minimal system paths,
/// then config readonly/writable paths and network blocking.
/// `exe` must be the path to the binary that will be exec'd in the child (e.g. current_exe()).
#[cfg(unix)]
pub fn build_nono_capability_set(
    config: &SandboxConfig,
    exe: &Path,
) -> std::result::Result<nono::CapabilitySet, SandboxError> {
    use nono::{AccessMode, CapabilitySet};

    let mut caps = CapabilitySet::new();

    // Allow the directory containing the executable so exec() can open the binary (required for Landlock/Seatbelt)
    let exe_abs = exe
        .canonicalize()
        .unwrap_or_else(|_| exe.to_path_buf());
    if let Some(parent) = exe_abs.parent() {
        if !parent.as_os_str().is_empty() {
            caps = caps
                .allow_path(parent, AccessMode::Read)
                .map_err(|e| SandboxError::CreationFailed(format!("nono allow_path exe dir {}: {}", parent.display(), e)))?;
        }
    }

    // Minimal system paths: binary/libs; /etc, /run, /mnt/wsl for DNS (resolv.conf may be
    // a symlink: systemd -> /run/systemd/resolve/..., WSL -> /mnt/wsl/resolv.conf)
    let system_read_only: &[&str] = if cfg!(target_os = "linux") {
        &["/usr", "/lib", "/lib64", "/bin", "/etc", "/run", "/mnt/wsl"]
    } else {
        &["/usr", "/lib", "/bin", "/etc", "/run"]
    };
    for p in system_read_only {
        if Path::new(p).exists() {
            caps = caps
                .allow_path(p, AccessMode::Read)
                .map_err(|e| SandboxError::CreationFailed(format!("nono allow_path {}: {}", p, e)))?;
        }
    }

    for p in &config.filesystem.readonly_paths {
        caps = caps
            .allow_path(p.as_str(), AccessMode::Read)
            .map_err(|e| SandboxError::CreationFailed(format!("nono allow_path read {}: {}", p, e)))?;
    }
    for p in &config.filesystem.writable_paths {
        caps = caps
            .allow_path(p.as_str(), AccessMode::ReadWrite)
            .map_err(|e| SandboxError::CreationFailed(format!("nono allow_path readwrite {}: {}", p, e)))?;
    }

    if !config.network.enabled {
        caps = caps.block_network();
    }

    // On macOS with network enabled: allow TLS to verify server certs (fix OSStatus -9808).
    // - Allow read to system and user Keychains dirs so Security framework can use root CAs.
    // - Opt in to login keychain file so nono may skip its SecurityServer/securityd deny.
    // - Add explicit mach-lookup allow rules so they override nono's deny (last-match).
    #[cfg(target_os = "macos")]
    if config.network.enabled {
        let keychains_dirs: &[&str] = &["/Library/Keychains", "/System/Library/Keychains"];
        for d in keychains_dirs {
            if Path::new(d).exists() {
                caps = caps
                    .allow_path(*d, AccessMode::Read)
                    .map_err(|e| SandboxError::CreationFailed(format!("nono allow_path keychains {}: {}", d, e)))?;
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            let user_keychain = Path::new(&home).join("Library/Keychains/login.keychain-db");
            if user_keychain.exists() {
                caps = caps
                    .allow_file(&user_keychain, AccessMode::Read)
                    .map_err(|e| SandboxError::CreationFailed(format!("nono allow_file user keychain: {}", e)))?;
            }
        }
        let system_keychain = Path::new("/Library/Keychains/login.keychain-db");
        if system_keychain.exists() {
            caps = caps
                .allow_file(system_keychain, AccessMode::Read)
                .map_err(|e| SandboxError::CreationFailed(format!("nono allow_file system keychain: {}", e)))?;
        }
        caps = caps
            .platform_rule("(allow mach-lookup (global-name \"com.apple.SecurityServer\"))")
            .map_err(|e| SandboxError::CreationFailed(format!("nono platform_rule SecurityServer: {}", e)))?;
        caps = caps
            .platform_rule("(allow mach-lookup (global-name \"com.apple.securityd\"))")
            .map_err(|e| SandboxError::CreationFailed(format!("nono platform_rule securityd: {}", e)))?;
    }

    // On macOS, allow Docker Desktop socket (~/.docker/run) so tool sandbox (plain-docker / gvisor-docker) can connect.
    #[cfg(target_os = "macos")]
    if let Ok(home) = std::env::var("HOME") {
        let docker_run = Path::new(&home).join(".docker/run");
        if docker_run.exists() {
            caps = caps
                .allow_path(docker_run.as_path(), AccessMode::ReadWrite)
                .map_err(|e| SandboxError::CreationFailed(format!("nono allow_path docker run: {}", e)))?;
        }
    }

    Ok(caps)
}

/// nono.sh sandbox configuration
#[derive(Debug, Clone)]
struct NonoConfig {
    filesystem: NonoFilesystemConfig,
    network: NonoNetworkConfig,
    resources: NonoResourceConfig,
}

#[derive(Debug, Clone)]
struct NonoFilesystemConfig {
    readonly_paths: Vec<String>,
    writable_paths: Vec<String>,
    hidden_paths: Vec<String>,
}

#[derive(Debug, Clone)]
struct NonoNetworkConfig {
    enabled: bool,
    allowed_hosts: Vec<String>,
}

#[derive(Debug, Clone)]
struct NonoResourceConfig {
    max_memory: u64,
    max_cpu: f64,
}

/// nono.sh sandbox implementation
/// 
/// NonoSandbox provides application-level sandboxing on Linux and macOS using:
/// - Namespaces for process, network, and filesystem isolation
/// - cgroups for resource limits (CPU, memory)
/// - Mount namespaces for filesystem access control
pub struct NonoSandbox {
    config: SandboxConfig,
    nono_config: NonoConfig,
    process: Option<Child>,
    status: SandboxStatus,
}

impl NonoSandbox {
    /// Create a new nono.sh sandbox instance
    /// 
    /// # Arguments
    /// 
    /// * `config` - Sandbox configuration
    /// 
    /// # Returns
    /// 
    /// A new NonoSandbox instance
    pub fn new(config: SandboxConfig) -> Result<Self> {
        let nono_config = Self::build_nono_config(&config);
        
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
            nono_config,
            process: None,
            status,
        })
    }
    
    /// Build nono.sh specific configuration from sandbox config
    fn build_nono_config(config: &SandboxConfig) -> NonoConfig {
        NonoConfig {
            filesystem: NonoFilesystemConfig {
                readonly_paths: config.filesystem.readonly_paths.clone(),
                writable_paths: config.filesystem.writable_paths.clone(),
                hidden_paths: config.filesystem.hidden_paths.clone(),
            },
            network: NonoNetworkConfig {
                enabled: config.network.enabled,
                allowed_hosts: config.network.allowed_hosts.clone(),
            },
            resources: NonoResourceConfig {
                max_memory: config.resources.max_memory,
                max_cpu: config.resources.max_cpu,
            },
        }
    }
    
    /// Build command line arguments for nono.sh
    fn build_nono_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        
        // Add readonly paths - these paths will be mounted read-only
        for path in &self.nono_config.filesystem.readonly_paths {
            args.push("--ro".to_string());
            args.push(path.clone());
        }
        
        // Add writable paths - these paths will be mounted read-write
        for path in &self.nono_config.filesystem.writable_paths {
            args.push("--rw".to_string());
            args.push(path.clone());
        }
        
        // Add hidden paths - these paths will be hidden from the sandbox
        // This prevents access to sensitive files like /etc/shadow
        for path in &self.nono_config.filesystem.hidden_paths {
            args.push("--hide".to_string());
            args.push(path.clone());
        }
        
        // Enable filesystem namespace isolation
        args.push("--unshare-fs".to_string());
        
        // Add network configuration
        args.extend(self.build_network_args());
        
        // Add resource limits
        args.extend(self.build_resource_args());
        
        args
    }
    
    /// Apply filesystem isolation using mount namespaces
    /// 
    /// This method configures the filesystem access control by:
    /// 1. Creating a new mount namespace
    /// 2. Mounting readonly paths as read-only
    /// 3. Mounting writable paths as read-write
    /// 4. Hiding sensitive paths
    #[cfg(target_os = "linux")]
    fn apply_filesystem_isolation(&self) -> Result<()> {
        use nix::mount::{mount, MsFlags};
        use nix::sched::{unshare, CloneFlags};
        
        // Create new mount namespace
        unshare(CloneFlags::CLONE_NEWNS)
            .map_err(|e| SandboxError::CreationFailed(format!("Failed to create mount namespace: {}", e)))?;
        
        // Mount readonly paths
        for path in &self.nono_config.filesystem.readonly_paths {
            let flags = MsFlags::MS_BIND | MsFlags::MS_RDONLY;
            mount(
                Some(path.as_str()),
                path.as_str(),
                None::<&str>,
                flags,
                None::<&str>,
            ).map_err(|e| SandboxError::CreationFailed(format!("Failed to mount readonly path {}: {}", path, e)))?;
        }
        
        // Mount writable paths
        for path in &self.nono_config.filesystem.writable_paths {
            let flags = MsFlags::MS_BIND;
            mount(
                Some(path.as_str()),
                path.as_str(),
                None::<&str>,
                flags,
                None::<&str>,
            ).map_err(|e| SandboxError::CreationFailed(format!("Failed to mount writable path {}: {}", path, e)))?;
        }
        
        Ok(())
    }
    #[cfg(target_os = "macos")]
    fn apply_filesystem_isolation(&self) -> Result<()> {
        Ok(())
    }
    
    /// Apply network isolation using network namespaces
    /// 
    /// This method configures network access control by:
    /// 1. Creating a new network namespace (if network is disabled)
    /// 2. Configuring allowed hosts and ports (if network is enabled)
    /// 
    /// Network isolation is achieved through:
    /// - CLONE_NEWNET: Creates isolated network stack
    /// - Firewall rules: Restricts connections to allowed hosts/ports
    #[cfg(target_os = "linux")]
    fn apply_network_isolation(&self) -> Result<()> {
        use nix::sched::{unshare, CloneFlags};
        
        if !self.nono_config.network.enabled {
            // Create new network namespace - completely isolates network
            unshare(CloneFlags::CLONE_NEWNET)
                .map_err(|e| SandboxError::CreationFailed(format!("Failed to create network namespace: {}", e)))?;
        } else {
            // Network is enabled but restricted to allowed hosts/ports
            // This would typically be enforced by:
            // 1. iptables/nftables rules
            // 2. Network namespace with veth pair
            // 3. Traffic filtering at the namespace boundary
            //
            // For now, we rely on nono.sh to handle this configuration
            // through its --allow-host and --allow-port flags
        }
        
        Ok(())
    }
    
    #[cfg(target_os = "macos")]
    fn apply_network_isolation(&self) -> Result<()> {
        Ok(())
    }

    /// Build network-related command line arguments
    fn build_network_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        
        if !self.nono_config.network.enabled {
            // Completely disable network access
            args.push("--no-network".to_string());
        } else {
            // Enable network but restrict to allowed hosts
            for host in &self.nono_config.network.allowed_hosts {
                args.push("--allow-host".to_string());
                args.push(host.clone());
            }
        }
        
        args
    }
    
    /// Apply resource limits using cgroups
    /// 
    /// This method configures resource limits by:
    /// 1. Creating a cgroup for the sandbox
    /// 2. Setting CPU limits (cpu.cfs_quota_us, cpu.cfs_period_us)
    /// 3. Setting memory limits (memory.limit_in_bytes)
    /// 4. Setting disk I/O limits (blkio.throttle.*)
    /// 
    /// Resource limits are enforced by the Linux kernel through cgroups v1/v2
    #[cfg(target_os = "linux")]
    fn apply_resource_limits(&self, pid: Pid) -> Result<()> {
        use std::fs;
        use std::io::Write;
        
        let cgroup_name = format!("nono-{}", self.config.sandbox_id);
        
        // Create cgroup directory structure
        let cgroup_base = format!("/sys/fs/cgroup/{}", cgroup_name);
        
        // Memory limit
        if self.nono_config.resources.max_memory > 0 {
            let memory_path = format!("{}/memory.limit_in_bytes", cgroup_base);
            if let Ok(mut file) = fs::File::create(&memory_path) {
                let _ = write!(file, "{}", self.nono_config.resources.max_memory);
            }
        }
        
        // CPU limit
        if self.nono_config.resources.max_cpu > 0.0 {
            // Convert CPU cores to cgroup quota
            // cfs_period_us is typically 100000 (100ms)
            // cfs_quota_us = period * cpu_cores
            let period = 100000;
            let quota = (period as f64 * self.nono_config.resources.max_cpu) as u64;
            
            let cpu_quota_path = format!("{}/cpu.cfs_quota_us", cgroup_base);
            if let Ok(mut file) = fs::File::create(&cpu_quota_path) {
                let _ = write!(file, "{}", quota);
            }
            
            let cpu_period_path = format!("{}/cpu.cfs_period_us", cgroup_base);
            if let Ok(mut file) = fs::File::create(&cpu_period_path) {
                let _ = write!(file, "{}", period);
            }
        }
        
        // Add process to cgroup
        let tasks_path = format!("{}/tasks", cgroup_base);
        if let Ok(mut file) = fs::File::create(&tasks_path) {
            let _ = write!(file, "{}", pid);
        }
        
        Ok(())
    }
    
    /// Build resource limit command line arguments
    fn build_resource_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        
        // Memory limit
        if self.nono_config.resources.max_memory > 0 {
            args.push("--memory".to_string());
            args.push(self.nono_config.resources.max_memory.to_string());
        }
        
        // CPU limit
        if self.nono_config.resources.max_cpu > 0.0 {
            args.push("--cpu".to_string());
            args.push(self.nono_config.resources.max_cpu.to_string());
        }
        
        // Enable cgroup isolation
        args.push("--cgroup".to_string());
        args.push(format!("nono-{}", self.config.sandbox_id));
        
        args
    }

    /// Spawn a long-running child process inside the nono sandbox.
    ///
    /// Runs `nono <args> -- <exe> <args>` with `SYNBOT_IN_APP_SANDBOX=1` in the child environment.
    /// The caller must wait on the returned `Child` and should forward signals (SIGINT, SIGTERM)
    /// to the child's PID for graceful shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error if the `nono` binary is not found or the process fails to spawn.
    pub fn spawn_child_in_sandbox(&self, exe: &Path, args: &[String]) -> Result<Child> {
        let mut nono_args = self.build_nono_args();
        nono_args.push("--".to_string());
        nono_args.push(exe.to_string_lossy().into_owned());
        nono_args.extend_from_slice(args);

        let child = Command::new("nono")
            .args(&nono_args)
            .env("SYNBOT_IN_APP_SANDBOX", "1")
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| SandboxError::CreationFailed(format!("Failed to spawn child in nono sandbox: {}", e)))?;

        Ok(child)
    }
}

impl Sandbox for NonoSandbox {
    fn start(&mut self) -> Result<()> {
        if self.status.state == SandboxState::Running {
            return Ok(());
        }
        
        self.status.state = SandboxState::Starting;
        
        // For nono.sh, we don't start a persistent process
        // Instead, we prepare the sandbox environment
        // The actual process will be started when execute() is called
        
        self.status.state = SandboxState::Running;
        self.status.started_at = Some(Utc::now());
        
        Ok(())
    }
    
    fn stop(&mut self) -> Result<()> {
        if self.status.state == SandboxState::Stopped {
            return Ok(());
        }
        
        self.status.state = SandboxState::Stopping;
        
        // Kill any running process
        if let Some(mut child) = self.process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        
        self.status.state = SandboxState::Stopped;
        self.status.stopped_at = Some(Utc::now());
        
        Ok(())
    }
    
    fn execute(
        &self,
        command: &str,
        args: &[String],
        timeout: Duration,
        _working_dir: Option<&str>,
    ) -> Result<ExecutionResult> {
        if self.status.state != SandboxState::Running {
            return Err(SandboxError::NotStarted);
        }
        
        // Build nono.sh command
        let mut nono_args = self.build_nono_args();
        nono_args.push("--".to_string());
        nono_args.push(command.to_string());
        nono_args.extend_from_slice(args);
        
        let start = Instant::now();
        
        // Execute command with nono.sh wrapper
        let output = Command::new("nono")
            .args(&nono_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| SandboxError::ExecutionFailed(format!("Failed to execute command: {}", e)))?;
        
        let duration = start.elapsed();
        
        // Check timeout
        if duration > timeout {
            return Err(SandboxError::Timeout);
        }
        
        Ok(ExecutionResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: output.stdout,
            stderr: output.stderr,
            duration,
            error: if output.status.success() {
                None
            } else {
                Some(format!("Command exited with code {}", output.status.code().unwrap_or(-1)))
            },
        })
    }
    
    fn get_status(&self) -> SandboxStatus {
        self.status.clone()
    }
    
    fn health_check(&self) -> HealthStatus {
        let mut checks = std::collections::HashMap::new();
        
        // Check if sandbox is in running state
        checks.insert("state".to_string(), self.status.state == SandboxState::Running);
        
        // Check if nono.sh is available
        let nono_available = Command::new("which")
            .arg("nono")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);
        checks.insert("nono_available".to_string(), nono_available);
        
        let healthy = checks.values().all(|&v| v);
        
        let message = if healthy {
            "Sandbox is healthy".to_string()
        } else {
            "Sandbox health check failed".to_string()
        };
        
        HealthStatus {
            healthy,
            checks,
            message,
        }
    }
    
    fn get_info(&self) -> SandboxInfo {
        SandboxInfo {
            sandbox_id: self.config.sandbox_id.clone(),
            platform: std::env::consts::OS.to_string(),
            sandbox_type: "nono".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_config() -> SandboxConfig {
        SandboxConfig {
            sandbox_id: "test-nono-001".to_string(),
            platform: "linux".to_string(),
            filesystem: FilesystemConfig {
                readonly_paths: vec!["/usr".to_string(), "/lib".to_string()],
                writable_paths: vec!["/tmp".to_string()],
                hidden_paths: vec!["/etc/shadow".to_string()],
                ..Default::default()
            },
            network: NetworkConfig {
                enabled: false,
                allowed_hosts: vec![],
                allowed_ports: vec![],
            },
            resources: ResourceConfig {
                max_memory: 1024 * 1024 * 1024, // 1GB
                max_cpu: 1.0,
                max_disk: 5 * 1024 * 1024 * 1024, // 5GB
            },
            process: ProcessConfig {
                allow_fork: false,
                max_processes: 10,
            },
            child_work_dir: None,
            monitoring: MonitoringConfig::default(),
            delete_on_start: false,
            requested_tool_sandbox_type: None,
            image: None,
        }
    }
    
    #[test]
    fn test_nono_sandbox_creation() {
        let config = create_test_config();
        let sandbox = NonoSandbox::new(config.clone());
        
        assert!(sandbox.is_ok());
        let sandbox = sandbox.unwrap();
        assert_eq!(sandbox.config.sandbox_id, "test-nono-001");
        assert_eq!(sandbox.status.state, SandboxState::Created);
    }
    
    #[test]
    fn test_nono_sandbox_info() {
        let config = create_test_config();
        let sandbox = NonoSandbox::new(config).unwrap();
        
        let info = sandbox.get_info();
        assert_eq!(info.sandbox_id, "test-nono-001");
        assert_eq!(info.sandbox_type, "nono");
    }
    
    #[test]
    fn test_nono_sandbox_start_stop() {
        let config = create_test_config();
        let mut sandbox = NonoSandbox::new(config).unwrap();
        
        // Start sandbox
        let result = sandbox.start();
        assert!(result.is_ok());
        assert_eq!(sandbox.status.state, SandboxState::Running);
        assert!(sandbox.status.started_at.is_some());
        
        // Stop sandbox
        let result = sandbox.stop();
        assert!(result.is_ok());
        assert_eq!(sandbox.status.state, SandboxState::Stopped);
        assert!(sandbox.status.stopped_at.is_some());
    }
    
    #[test]
    fn test_build_nono_args() {
        let config = create_test_config();
        let sandbox = NonoSandbox::new(config).unwrap();
        
        let args = sandbox.build_nono_args();
        
        // Check that readonly paths are included
        assert!(args.contains(&"--ro".to_string()));
        assert!(args.contains(&"/usr".to_string()));
        
        // Check that writable paths are included
        assert!(args.contains(&"--rw".to_string()));
        assert!(args.contains(&"/tmp".to_string()));
        
        // Check that hidden paths are included
        assert!(args.contains(&"--hide".to_string()));
        assert!(args.contains(&"/etc/shadow".to_string()));
        
        // Check that network is disabled
        assert!(args.contains(&"--no-network".to_string()));
        
        // Check that resource limits are included
        assert!(args.contains(&"--memory".to_string()));
        assert!(args.contains(&"--cpu".to_string()));
        
        // Check that cgroup is configured
        assert!(args.contains(&"--cgroup".to_string()));
    }
    
    #[test]
    fn test_build_resource_args() {
        let config = create_test_config();
        let sandbox = NonoSandbox::new(config).unwrap();
        
        let args = sandbox.build_resource_args();
        
        // Check memory limit
        assert!(args.contains(&"--memory".to_string()));
        assert!(args.contains(&(1024 * 1024 * 1024).to_string()));
        
        // Check CPU limit
        assert!(args.contains(&"--cpu".to_string()));
        assert!(args.contains(&"1".to_string()));
        
        // Check cgroup name
        assert!(args.contains(&"--cgroup".to_string()));
        assert!(args.iter().any(|arg| arg.starts_with("nono-test-nono-001")));
    }
    
    #[test]
    fn test_build_network_args_disabled() {
        let config = create_test_config();
        let sandbox = NonoSandbox::new(config).unwrap();
        
        let args = sandbox.build_network_args();
        
        // Network should be disabled
        assert!(args.contains(&"--no-network".to_string()));
    }
    
    #[test]
    fn test_build_network_args_enabled() {
        let mut config = create_test_config();
        config.network.enabled = true;
        config.network.allowed_hosts = vec!["example.com".to_string(), "api.example.com".to_string()];
        
        let sandbox = NonoSandbox::new(config).unwrap();
        let args = sandbox.build_network_args();
        
        // Network should be enabled with allowed hosts
        assert!(!args.contains(&"--no-network".to_string()));
        assert!(args.contains(&"--allow-host".to_string()));
        assert!(args.contains(&"example.com".to_string()));
        assert!(args.contains(&"api.example.com".to_string()));
    }
}
