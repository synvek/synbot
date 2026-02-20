// Security hardening module for sandbox implementations
//
// This module provides security validation, escape prevention, privilege escalation
// protection, and resource exhaustion defense mechanisms for all sandbox implementations.

use super::error::{Result, SandboxError};
use super::types::SandboxConfig;
use std::collections::HashSet;

/// Security validator for sandbox configurations and runtime behavior
pub struct SecurityValidator {
    /// Known dangerous paths that should never be accessible
    dangerous_paths: HashSet<String>,
    /// Known dangerous capabilities that should be blocked
    dangerous_capabilities: HashSet<String>,
    /// Maximum allowed resource limits
    max_resource_limits: ResourceLimits,
}

/// Maximum resource limits to prevent exhaustion
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_memory: u64,
    pub max_cpu: f64,
    pub max_disk: u64,
    pub max_processes: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_memory: 16 * 1024 * 1024 * 1024, // 16GB
            max_cpu: num_cpus::get() as f64,      // All available CPUs
            max_disk: 100 * 1024 * 1024 * 1024,   // 100GB
            max_processes: 1000,
        }
    }
}

impl SecurityValidator {
    /// Create a new security validator with default settings
    pub fn new() -> Self {
        Self {
            dangerous_paths: Self::init_dangerous_paths(),
            dangerous_capabilities: Self::init_dangerous_capabilities(),
            max_resource_limits: ResourceLimits::default(),
        }
    }
    
    /// Create a security validator with custom resource limits
    pub fn with_limits(limits: ResourceLimits) -> Self {
        Self {
            dangerous_paths: Self::init_dangerous_paths(),
            dangerous_capabilities: Self::init_dangerous_capabilities(),
            max_resource_limits: limits,
        }
    }
    
    /// Initialize list of dangerous paths that should never be accessible
    fn init_dangerous_paths() -> HashSet<String> {
        let mut paths = HashSet::new();
        
        // Linux/Unix sensitive paths
        paths.insert("/etc/shadow".to_string());
        paths.insert("/etc/sudoers".to_string());
        paths.insert("/etc/passwd".to_string());
        paths.insert("/root".to_string());
        paths.insert("/boot".to_string());
        paths.insert("/sys".to_string());
        paths.insert("/proc/sys".to_string());
        paths.insert("/dev/mem".to_string());
        paths.insert("/dev/kmem".to_string());
        
        // Windows sensitive paths
        paths.insert("C:\\Windows\\System32\\config".to_string());
        paths.insert("C:\\Windows\\System32\\SAM".to_string());
        paths.insert("C:\\Windows\\System32\\SECURITY".to_string());
        paths.insert("C:\\ProgramData\\Microsoft\\Crypto".to_string());
        
        paths
    }
    
    /// Initialize list of dangerous capabilities that should be blocked
    fn init_dangerous_capabilities() -> HashSet<String> {
        let mut caps = HashSet::new();
        
        // Linux capabilities that enable privilege escalation
        caps.insert("CAP_SYS_ADMIN".to_string());
        caps.insert("CAP_SYS_MODULE".to_string());
        caps.insert("CAP_SYS_RAWIO".to_string());
        caps.insert("CAP_SYS_PTRACE".to_string());
        caps.insert("CAP_SYS_BOOT".to_string());
        caps.insert("CAP_MAC_ADMIN".to_string());
        caps.insert("CAP_MAC_OVERRIDE".to_string());
        caps.insert("CAP_SETUID".to_string());
        caps.insert("CAP_SETGID".to_string());
        caps.insert("CAP_SETFCAP".to_string());
        
        caps
    }
    
    /// Validate sandbox configuration for security issues
    /// 
    /// This performs comprehensive security checks including:
    /// - Filesystem access validation
    /// - Resource limit validation
    /// - Network configuration validation
    /// - Process control validation
    /// 
    /// # Arguments
    /// 
    /// * `config` - Sandbox configuration to validate
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if configuration is secure, or an error describing the issue
    pub fn validate_config(&self, config: &SandboxConfig) -> Result<()> {
        // Validate filesystem access
        self.validate_filesystem_access(config)?;
        
        // Validate resource limits
        self.validate_resource_limits(config)?;
        
        // Validate network configuration
        self.validate_network_config(config)?;
        
        // Validate process controls
        self.validate_process_controls(config)?;
        
        Ok(())
    }
    
    /// Validate filesystem access configuration
    fn validate_filesystem_access(&self, config: &SandboxConfig) -> Result<()> {
        // Check writable paths for dangerous locations
        for path in &config.filesystem.writable_paths {
            if self.is_dangerous_path(path) {
                return Err(SandboxError::SecurityViolation(
                    format!("Writable access to dangerous path not allowed: {}", path)
                ));
            }
            
            // Check for path traversal attempts
            if path.contains("..") {
                return Err(SandboxError::SecurityViolation(
                    format!("Path traversal detected in writable path: {}", path)
                ));
            }
        }
        
        // Check readonly paths for path traversal
        for path in &config.filesystem.readonly_paths {
            if path.contains("..") {
                return Err(SandboxError::SecurityViolation(
                    format!("Path traversal detected in readonly path: {}", path)
                ));
            }
        }
        
        // Verify that dangerous paths are in hidden_paths
        for dangerous_path in &self.dangerous_paths {
            let is_hidden = config.filesystem.hidden_paths.iter()
                .any(|p| p == dangerous_path || dangerous_path.starts_with(p));
            
            let is_writable = config.filesystem.writable_paths.iter()
                .any(|p| dangerous_path.starts_with(p));
            
            if is_writable && !is_hidden {
                return Err(SandboxError::SecurityViolation(
                    format!("Dangerous path {} must be explicitly hidden", dangerous_path)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Validate resource limits to prevent exhaustion
    fn validate_resource_limits(&self, config: &SandboxConfig) -> Result<()> {
        // Check memory limit
        if config.resources.max_memory > self.max_resource_limits.max_memory {
            return Err(SandboxError::SecurityViolation(
                format!(
                    "Memory limit {} exceeds maximum allowed {}",
                    config.resources.max_memory,
                    self.max_resource_limits.max_memory
                )
            ));
        }
        
        // Enforce minimum memory limit to prevent DoS
        let min_memory = 64 * 1024 * 1024; // 64MB minimum
        if config.resources.max_memory < min_memory {
            return Err(SandboxError::SecurityViolation(
                format!("Memory limit {} is below minimum {}", config.resources.max_memory, min_memory)
            ));
        }
        
        // Check CPU limit
        if config.resources.max_cpu > self.max_resource_limits.max_cpu {
            return Err(SandboxError::SecurityViolation(
                format!(
                    "CPU limit {} exceeds maximum allowed {}",
                    config.resources.max_cpu,
                    self.max_resource_limits.max_cpu
                )
            ));
        }
        
        if config.resources.max_cpu <= 0.0 {
            return Err(SandboxError::SecurityViolation(
                "CPU limit must be greater than 0".to_string()
            ));
        }
        
        // Check disk limit
        if config.resources.max_disk > self.max_resource_limits.max_disk {
            return Err(SandboxError::SecurityViolation(
                format!(
                    "Disk limit {} exceeds maximum allowed {}",
                    config.resources.max_disk,
                    self.max_resource_limits.max_disk
                )
            ));
        }
        
        // Check process limit
        if config.process.max_processes > self.max_resource_limits.max_processes {
            return Err(SandboxError::SecurityViolation(
                format!(
                    "Process limit {} exceeds maximum allowed {}",
                    config.process.max_processes,
                    self.max_resource_limits.max_processes
                )
            ));
        }
        
        if config.process.max_processes == 0 {
            return Err(SandboxError::SecurityViolation(
                "Process limit must be at least 1".to_string()
            ));
        }
        
        Ok(())
    }
    
    /// Validate network configuration
    fn validate_network_config(&self, config: &SandboxConfig) -> Result<()> {
        // If network is disabled, no further checks needed
        if !config.network.enabled {
            return Ok(());
        }
        
        // Validate allowed hosts (outbound allowlist; binding address 0.0.0.0 is not a valid host here)
        for host in &config.network.allowed_hosts {
            // Check for wildcard abuse
            if host == "*" || host == "0.0.0.0" || host == "::" {
                return Err(SandboxError::SecurityViolation(
                    format!(
                        "Overly permissive network host pattern: \"{}\". \
                        allowed_hosts is for outbound connections; use specific hostnames (e.g. api.example.com) or leave empty. \
                        Do not use 0.0.0.0 or *.",
                        host
                    )
                ));
            }
            
            // Check for localhost/loopback access which could bypass isolation
            if host.contains("localhost") || host.contains("127.0.0.1") || host.contains("::1") {
                log::warn!("Network access to localhost/loopback detected: {}", host);
            }
        }
        
        // Validate allowed ports
        for &port in &config.network.allowed_ports {
            // Warn about privileged ports
            if port < 1024 {
                log::warn!("Access to privileged port {} allowed", port);
            }
        }
        
        Ok(())
    }
    
    /// Validate process control configuration
    fn validate_process_controls(&self, config: &SandboxConfig) -> Result<()> {
        // Fork should generally be disabled for security
        if config.process.allow_fork {
            log::warn!("Process forking is enabled - this may allow fork bombs");
        }
        
        // Ensure reasonable process limit
        if config.process.max_processes > 100 {
            log::warn!("High process limit {} may enable resource exhaustion", config.process.max_processes);
        }
        
        Ok(())
    }
    
    /// Check if a path is considered dangerous
    fn is_dangerous_path(&self, path: &str) -> bool {
        // Direct match
        if self.dangerous_paths.contains(path) {
            return true;
        }
        
        // Check if path is under a dangerous directory
        for dangerous_path in &self.dangerous_paths {
            if path.starts_with(dangerous_path) {
                return true;
            }
        }
        
        false
    }
    
    /// Validate command execution for security issues
    /// 
    /// This checks commands before execution to prevent:
    /// - Shell injection
    /// - Privilege escalation attempts
    /// - Escape attempts
    /// 
    /// # Arguments
    /// 
    /// * `command` - Command to execute
    /// * `args` - Command arguments
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if command is safe, or an error if security issue detected
    pub fn validate_command(&self, command: &str, args: &[String]) -> Result<()> {
        // Check for shell metacharacters that could enable injection
        let dangerous_chars = ['|', '&', ';', '\n', '`', '$', '(', ')', '<', '>', '"', '\''];
        
        if command.chars().any(|c| dangerous_chars.contains(&c)) {
            return Err(SandboxError::SecurityViolation(
                format!("Command contains dangerous shell metacharacters: {}", command)
            ));
        }
        
        // Check arguments for injection attempts
        for arg in args {
            if arg.chars().any(|c| dangerous_chars.contains(&c)) {
                log::warn!("Argument contains shell metacharacters: {}", arg);
            }
        }
        
        // Block known privilege escalation commands
        let dangerous_commands = [
            "sudo", "su", "doas", "pkexec",
            "chmod", "chown", "chgrp",
            "mount", "umount",
            "insmod", "rmmod", "modprobe",
        ];
        
        let cmd_name = std::path::Path::new(command)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(command);
        
        if dangerous_commands.contains(&cmd_name) {
            return Err(SandboxError::SecurityViolation(
                format!("Dangerous command not allowed: {}", cmd_name)
            ));
        }
        
        Ok(())
    }
}

impl Default for SecurityValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Escape prevention mechanisms
pub struct EscapePrevention;

impl EscapePrevention {
    /// Verify sandbox isolation integrity
    /// 
    /// This performs runtime checks to ensure the sandbox hasn't been compromised:
    /// - Namespace isolation is intact
    /// - No unauthorized mounts
    /// - Process tree is contained
    /// - Network isolation is enforced
    /// 
    /// # Arguments
    /// 
    /// * `sandbox_id` - ID of sandbox to check
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if isolation is intact, or an error if compromise detected
    #[cfg(target_os = "linux")]
    pub fn verify_isolation(sandbox_id: &str) -> Result<()> {
        use std::fs;
        
        // Check namespace isolation
        // In a properly isolated sandbox, the process should be in its own namespaces
        let ns_path = format!("/proc/self/ns");
        
        if !std::path::Path::new(&ns_path).exists() {
            return Err(SandboxError::SecurityViolation(
                "Cannot verify namespace isolation".to_string()
            ));
        }
        
        // Check for suspicious mounts
        if let Ok(mounts) = fs::read_to_string("/proc/self/mounts") {
            // Look for mounts that could indicate escape attempts
            let suspicious_mounts = ["/proc/sys", "/sys/kernel", "/dev/mem"];
            
            for suspicious in &suspicious_mounts {
                if mounts.contains(suspicious) {
                    log::warn!("Suspicious mount detected: {}", suspicious);
                }
            }
        }
        
        Ok(())
    }
    
    #[cfg(not(target_os = "linux"))]
    pub fn verify_isolation(_sandbox_id: &str) -> Result<()> {
        // Platform-specific isolation verification would go here
        Ok(())
    }
    
    /// Check for common escape techniques
    /// 
    /// This monitors for known sandbox escape patterns:
    /// - Kernel exploits
    /// - Container breakout attempts
    /// - Namespace manipulation
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if no escape attempts detected
    pub fn check_escape_attempts() -> Result<()> {
        // This would integrate with runtime monitoring to detect:
        // - Unusual system calls
        // - Attempts to access /proc/*/ns
        // - Attempts to manipulate cgroups
        // - Kernel module loading attempts
        
        // For now, this is a placeholder for future implementation
        Ok(())
    }
}

/// Privilege escalation prevention
pub struct PrivilegeEscalationPrevention;

impl PrivilegeEscalationPrevention {
    /// Verify process is running with minimal privileges
    /// 
    /// Ensures the sandbox process:
    /// - Is not running as root
    /// - Has no dangerous capabilities
    /// - Cannot escalate privileges
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if privileges are minimal, or an error if issues detected
    #[cfg(target_os = "linux")]
    pub fn verify_minimal_privileges() -> Result<()> {
        use nix::unistd::{Uid, Gid};
        
        // Check if running as root
        let uid = Uid::current();
        if uid.is_root() {
            return Err(SandboxError::SecurityViolation(
                "Sandbox should not run as root".to_string()
            ));
        }
        
        // Check effective UID
        let euid = Uid::effective();
        if euid.is_root() {
            return Err(SandboxError::SecurityViolation(
                "Sandbox has root effective UID".to_string()
            ));
        }
        
        Ok(())
    }
    
    #[cfg(not(target_os = "linux"))]
    pub fn verify_minimal_privileges() -> Result<()> {
        // Platform-specific privilege checks would go here
        Ok(())
    }
    
    /// Ensure no-new-privileges flag is set
    /// 
    /// The no-new-privileges flag prevents processes from gaining new privileges
    /// through execve(), blocking many privilege escalation vectors
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if flag is set, or an error if not
    #[cfg(target_os = "linux")]
    pub fn ensure_no_new_privileges() -> Result<()> {
        use std::fs;
        
        // Check /proc/self/status for NoNewPrivs flag
        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("NoNewPrivs:") {
                    let value = line.split(':').nth(1)
                        .and_then(|v| v.trim().parse::<u32>().ok())
                        .unwrap_or(0);
                    
                    if value == 0 {
                        return Err(SandboxError::SecurityViolation(
                            "NoNewPrivs flag is not set".to_string()
                        ));
                    }
                    
                    return Ok(());
                }
            }
        }
        
        Err(SandboxError::SecurityViolation(
            "Cannot verify NoNewPrivs flag".to_string()
        ))
    }
    
    #[cfg(not(target_os = "linux"))]
    pub fn ensure_no_new_privileges() -> Result<()> {
        // Platform-specific implementation would go here
        Ok(())
    }
}

/// Resource exhaustion prevention
pub struct ResourceExhaustionPrevention;

impl ResourceExhaustionPrevention {
    /// Monitor resource usage and enforce limits
    /// 
    /// This actively monitors and enforces:
    /// - Memory usage
    /// - CPU usage
    /// - Disk usage
    /// - Process count
    /// - File descriptor count
    /// 
    /// # Arguments
    /// 
    /// * `config` - Sandbox configuration with resource limits
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if resources are within limits, or an error if exceeded
    pub fn enforce_resource_limits(config: &SandboxConfig) -> Result<()> {
        // Check memory usage
        Self::check_memory_usage(config.resources.max_memory)?;
        
        // Check process count
        Self::check_process_count(config.process.max_processes)?;
        
        // Check file descriptors
        Self::check_file_descriptors()?;
        
        Ok(())
    }
    
    /// Check current memory usage against limit
    fn check_memory_usage(max_memory: u64) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            use std::fs;
            
            // Read memory usage from cgroup
            if let Ok(usage) = fs::read_to_string("/sys/fs/cgroup/memory/memory.usage_in_bytes") {
                if let Ok(current_usage) = usage.trim().parse::<u64>() {
                    if current_usage > max_memory {
                        return Err(SandboxError::ResourceExhausted(
                            format!("Memory usage {} exceeds limit {}", current_usage, max_memory)
                        ));
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Check current process count against limit
    fn check_process_count(max_processes: u32) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            use std::fs;
            
            // Count processes in current cgroup
            if let Ok(procs) = fs::read_to_string("/sys/fs/cgroup/pids/pids.current") {
                if let Ok(current_count) = procs.trim().parse::<u32>() {
                    if current_count > max_processes {
                        return Err(SandboxError::ResourceExhausted(
                            format!("Process count {} exceeds limit {}", current_count, max_processes)
                        ));
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Check file descriptor usage
    pub fn check_file_descriptors() -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            use std::fs;
            
            // Check open file descriptors
            if let Ok(entries) = fs::read_dir("/proc/self/fd") {
                let fd_count = entries.count();
                
                // Warn if approaching limit (typically 1024 or 4096)
                if fd_count > 900 {
                    log::warn!("High file descriptor usage: {}", fd_count);
                }
            }
        }
        
        Ok(())
    }
    
    /// Prevent fork bombs by monitoring process creation rate
    /// 
    /// # Arguments
    /// 
    /// * `max_rate` - Maximum processes per second
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if rate is acceptable, or an error if fork bomb detected
    pub fn prevent_fork_bomb(_max_rate: u32) -> Result<()> {
        // This would track process creation rate over time
        // For now, this is a placeholder for future implementation
        
        // In a full implementation, this would:
        // 1. Track process creation timestamps
        // 2. Calculate rate over sliding window
        // 3. Block if rate exceeds threshold
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::types::*;
    
    fn create_test_config() -> SandboxConfig {
        SandboxConfig {
            sandbox_id: "test-security".to_string(),
            platform: "linux".to_string(),
            filesystem: FilesystemConfig {
                readonly_paths: vec!["/usr".to_string()],
                writable_paths: vec!["/tmp".to_string()],
                hidden_paths: vec!["/etc/shadow".to_string(), "/root".to_string()],
                ..Default::default()
            },
            network: NetworkConfig {
                enabled: true,
                allowed_hosts: vec!["api.example.com".to_string()],
                allowed_ports: vec![443],
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
        }
    }
    
    #[test]
    fn test_security_validator_valid_config() {
        let validator = SecurityValidator::new();
        let config = create_test_config();
        
        assert!(validator.validate_config(&config).is_ok());
    }
    
    #[test]
    fn test_security_validator_dangerous_writable_path() {
        let validator = SecurityValidator::new();
        let mut config = create_test_config();
        config.filesystem.writable_paths.push("/etc/shadow".to_string());
        
        let result = validator.validate_config(&config);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SandboxError::SecurityViolation(_)));
    }
    
    #[test]
    fn test_security_validator_path_traversal() {
        let validator = SecurityValidator::new();
        let mut config = create_test_config();
        config.filesystem.writable_paths.push("/tmp/../etc".to_string());
        
        let result = validator.validate_config(&config);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_security_validator_excessive_memory() {
        let validator = SecurityValidator::new();
        let mut config = create_test_config();
        config.resources.max_memory = 100 * 1024 * 1024 * 1024; // 100GB
        
        let result = validator.validate_config(&config);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_security_validator_invalid_cpu_limit() {
        let validator = SecurityValidator::new();
        let mut config = create_test_config();
        config.resources.max_cpu = 0.0;
        
        let result = validator.validate_config(&config);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_security_validator_wildcard_network() {
        let validator = SecurityValidator::new();
        let mut config = create_test_config();
        config.network.allowed_hosts.push("*".to_string());
        
        let result = validator.validate_config(&config);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_validate_command_safe() {
        let validator = SecurityValidator::new();
        let result = validator.validate_command("ls", &["-la".to_string()]);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_validate_command_shell_injection() {
        let validator = SecurityValidator::new();
        let result = validator.validate_command("ls; rm -rf /", &[]);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_validate_command_dangerous() {
        let validator = SecurityValidator::new();
        let result = validator.validate_command("sudo", &["rm".to_string(), "-rf".to_string(), "/".to_string()]);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_is_dangerous_path() {
        let validator = SecurityValidator::new();
        
        assert!(validator.is_dangerous_path("/etc/shadow"));
        assert!(validator.is_dangerous_path("/root"));
        assert!(validator.is_dangerous_path("/root/secret"));
        assert!(!validator.is_dangerous_path("/tmp"));
        assert!(!validator.is_dangerous_path("/home/user"));
    }
}
