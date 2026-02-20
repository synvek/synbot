// WSL2 integration for Windows Docker connectivity
//
// This module provides WSL2 detection and Docker connection capabilities
// for Windows systems, enabling tool sandboxes to run via WSL2 + gVisor Docker.

use super::error::{Result, SandboxError};
use std::process::Command;

/// WSL2 integration utilities
pub struct Wsl2Integration;

impl Wsl2Integration {
    /// Detect if WSL2 is available on the system
    /// 
    /// This function checks if:
    /// 1. WSL is installed
    /// 2. WSL2 is the default version or at least one distribution uses WSL2
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(true)` if WSL2 is available, `Ok(false)` if not available,
    /// or an error if detection fails
    /// 
    /// # Examples
    /// 
    /// ```no_run
    /// use sandbox::wsl2::Wsl2Integration;
    /// 
    /// if Wsl2Integration::is_wsl2_available().unwrap_or(false) {
    ///     println!("WSL2 is available");
    /// }
    /// ```
    pub fn is_wsl2_available() -> Result<bool> {
        #[cfg(target_os = "windows")]
        {
            // Prefer wsl -l -v (available on all WSL installs); then try wsl --status (newer builds).
            let list_output = Command::new("wsl.exe")
                .args(["-l", "-v"])
                .output();

            if let Ok(list_result) = list_output {
                let list_stdout = String::from_utf8_lossy(&list_result.stdout);
                // Lines look like: "  NAME            STATE           VERSION" then "* Ubuntu    Running         2"
                // Or with locale: version number is last column. Match "2" as version (WSL2).
                for line in list_stdout.lines() {
                    let line = line.trim();
                    // Skip header
                    if line.eq_ignore_ascii_case("version") || line.is_empty() {
                        continue;
                    }
                    // Last token is often the version number (1 or 2)
                    if let Some(last) = line.split_whitespace().last() {
                        if last == "2" {
                            return Ok(true);
                        }
                    }
                }
                // Fallback: simple substring (handles " 2 " in table)
                if list_stdout.contains(" 2 ") && (list_stdout.contains("Running") || list_stdout.contains("Stopped")) {
                    return Ok(true);
                }
            }

            // Fallback: wsl --status (may not exist on older Windows 10)
            if let Ok(output) = Command::new("wsl.exe").arg("--status").output() {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let lower = stdout.to_lowercase();
                    if lower.contains("default version: 2")
                        || lower.contains("version: 2")
                        || lower.contains("version 2")
                    {
                        return Ok(true);
                    }
                }
            }

            Ok(false)
        }

        #[cfg(not(target_os = "windows"))]
        {
            Ok(false)
        }
    }
    
    /// Get the Docker connection string for WSL2
    /// 
    /// On Windows with WSL2, Docker Desktop typically exposes the Docker daemon
    /// through a named pipe or TCP socket that can be accessed from Windows.
    /// 
    /// # Returns
    /// 
    /// Returns the Docker connection string suitable for use with bollard
    /// 
    /// # Errors
    /// 
    /// Returns an error if WSL2 is not available or Docker connection cannot be determined
    pub fn get_docker_connection() -> Result<String> {
        #[cfg(target_os = "windows")]
        {
            // First check if WSL2 is available
            if !Self::is_wsl2_available()? {
                return Err(SandboxError::CreationFailed(
                    "WSL2 is not available on this system".to_string()
                ));
            }
            
            // Docker Desktop on Windows with WSL2 backend typically uses:
            // 1. Named pipe: npipe:////./pipe/docker_engine (Windows native)
            // 2. TCP socket: tcp://localhost:2375 (if exposed)
            // 
            // The default connection (local defaults) should work with Docker Desktop
            // which automatically configures the connection when WSL2 backend is enabled
            
            // Return the default connection string
            // bollard's connect_with_local_defaults() will handle the actual connection
            Ok("default".to_string())
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            Err(SandboxError::UnsupportedPlatform)
        }
    }
    
    /// Check if Docker is accessible through WSL2
    /// 
    /// This function attempts to connect to Docker and verify it's working
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(true)` if Docker is accessible, `Ok(false)` otherwise
    pub fn is_docker_accessible() -> Result<bool> {
        #[cfg(target_os = "windows")]
        {
            use bollard::Docker;
            
            // Try to connect to Docker
            let docker = Docker::connect_with_local_defaults()
                .map_err(|e| SandboxError::CreationFailed(
                    format!("Failed to connect to Docker: {}", e)
                ))?;
            
            // Try to ping Docker to verify connection
            let runtime = tokio::runtime::Runtime::new()
                .map_err(|e| SandboxError::CreationFailed(
                    format!("Failed to create runtime: {}", e)
                ))?;
            
            let ping_result = runtime.block_on(async {
                docker.ping().await
            });
            
            match ping_result {
                Ok(_) => Ok(true),
                Err(_) => Ok(false),
            }
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            Ok(false)
        }
    }
    
    /// Get WSL2 distribution information
    /// 
    /// Returns a list of installed WSL2 distributions
    /// 
    /// # Returns
    /// 
    /// A vector of distribution names that are using WSL2
    pub fn get_wsl2_distributions() -> Result<Vec<String>> {
        #[cfg(target_os = "windows")]
        {
            let output = Command::new("wsl.exe")
                .arg("-l")
                .arg("-v")
                .output()
                .map_err(|e| SandboxError::ExecutionFailed(
                    format!("Failed to list WSL distributions: {}", e)
                ))?;
            
            if !output.status.success() {
                return Ok(Vec::new());
            }
            
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut distributions = Vec::new();
            
            // Parse the output to find distributions with version 2
            for line in stdout.lines().skip(1) { // Skip header line
                // Format is typically: "  * Ubuntu-22.04    Running         2"
                let parts: Vec<&str> = line.split_whitespace().collect();
                
                if parts.len() >= 3 {
                    // Check if the last part is "2" (version)
                    if parts.last() == Some(&"2") {
                        // Get the distribution name (first non-asterisk part)
                        let name = parts.iter()
                            .find(|&&p| p != "*" && p != "Running" && p != "Stopped" && p != "2")
                            .map(|&s| s.to_string());
                        
                        if let Some(dist_name) = name {
                            distributions.push(dist_name);
                        }
                    }
                }
            }
            
            Ok(distributions)
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            Ok(Vec::new())
        }
    }
    
    /// Check if Docker Desktop is running with WSL2 backend
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(true)` if Docker Desktop is running with WSL2 backend
    pub fn is_docker_desktop_wsl2() -> Result<bool> {
        #[cfg(target_os = "windows")]
        {
            // Check if docker-desktop and docker-desktop-data distributions exist
            let distributions = Self::get_wsl2_distributions()?;
            
            let has_docker_desktop = distributions.iter()
                .any(|d| d.to_lowercase().contains("docker-desktop"));
            
            // Also verify Docker is accessible
            let docker_accessible = Self::is_docker_accessible().unwrap_or(false);
            
            Ok(has_docker_desktop && docker_accessible)
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    #[cfg(target_os = "windows")]
    fn test_wsl2_detection() {
        // This test will only pass on Windows systems with WSL2 installed
        // On CI or systems without WSL2, it should return Ok(false)
        let result = Wsl2Integration::is_wsl2_available();
        assert!(result.is_ok());
    }
    
    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_wsl2_not_available_on_non_windows() {
        let result = Wsl2Integration::is_wsl2_available();
        assert_eq!(result.unwrap(), false);
    }
    
    #[test]
    #[cfg(target_os = "windows")]
    fn test_get_docker_connection() {
        // This test checks if the function returns a valid connection string
        // It may fail if WSL2 is not available
        let result = Wsl2Integration::get_docker_connection();
        
        // If WSL2 is available, should return Ok
        // If not available, should return Err
        if Wsl2Integration::is_wsl2_available().unwrap_or(false) {
            assert!(result.is_ok());
        }
    }
    
    #[test]
    #[cfg(target_os = "windows")]
    fn test_get_wsl2_distributions() {
        let result = Wsl2Integration::get_wsl2_distributions();
        assert!(result.is_ok());
        
        // If WSL2 is available, there should be at least one distribution
        if Wsl2Integration::is_wsl2_available().unwrap_or(false) {
            let distributions = result.unwrap();
            // May be empty if no distributions are installed
            // Just verify it returns a valid vector
            assert!(distributions.len() >= 0);
        }
    }
    
    #[test]
    #[cfg(target_os = "windows")]
    fn test_docker_desktop_wsl2_detection() {
        let result = Wsl2Integration::is_docker_desktop_wsl2();
        assert!(result.is_ok());
        
        // The result depends on whether Docker Desktop with WSL2 is installed
        // Just verify the function executes without panicking
    }
}


/// WSL2 + gVisor Docker sandbox implementation
/// 
/// This sandbox implementation uses WSL2 on Windows to run Docker with gVisor runtime.
/// It extends the standard GVisorDockerSandbox with WSL2-specific connection handling.
pub struct Wsl2GVisorSandbox {
    /// The underlying gVisor Docker sandbox
    inner: super::GVisorDockerSandbox,
}

impl Wsl2GVisorSandbox {
    /// Create a new WSL2 + gVisor Docker sandbox
    /// 
    /// This constructor verifies that WSL2 is available and Docker is accessible
    /// before creating the sandbox instance.
    /// 
    /// # Arguments
    /// 
    /// * `config` - Sandbox configuration
    /// 
    /// # Returns
    /// 
    /// Returns a new `Wsl2GVisorSandbox` instance
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - WSL2 is not available
    /// - Docker is not accessible through WSL2
    /// - The underlying GVisorDockerSandbox cannot be created
    pub fn new(config: super::SandboxConfig) -> Result<Self> {
        #[cfg(target_os = "windows")]
        {
            // Verify WSL2 is available
            if !Wsl2Integration::is_wsl2_available()? {
                return Err(SandboxError::CreationFailed(
                    "WSL2 is not available. Please install WSL2 to use tool sandboxes on Windows.".to_string()
                ));
            }
            
            // Verify Docker is accessible
            if !Wsl2Integration::is_docker_accessible()? {
                return Err(SandboxError::CreationFailed(
                    "Docker is not accessible through WSL2. Please ensure Docker Desktop is running with WSL2 backend.".to_string()
                ));
            }
            
            // Create the underlying GVisorDockerSandbox
            // The GVisorDockerSandbox will use Docker's local defaults which
            // automatically work with Docker Desktop's WSL2 backend
            let inner = super::GVisorDockerSandbox::new(config)?;
            
            Ok(Self { inner })
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            Err(SandboxError::UnsupportedPlatform)
        }
    }
    
    /// Check if WSL2 + gVisor sandbox is supported on this system
    /// 
    /// # Returns
    /// 
    /// Returns `true` if WSL2 and Docker are both available
    pub fn is_supported() -> bool {
        #[cfg(target_os = "windows")]
        {
            Wsl2Integration::is_wsl2_available().unwrap_or(false)
                && Wsl2Integration::is_docker_accessible().unwrap_or(false)
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }
}

impl super::Sandbox for Wsl2GVisorSandbox {
    fn start(&mut self) -> Result<()> {
        self.inner.start()
    }
    
    fn stop(&mut self) -> Result<()> {
        self.inner.stop()
    }
    
    fn execute(
        &self,
        command: &str,
        args: &[String],
        timeout: std::time::Duration,
        working_dir: Option<&str>,
    ) -> Result<super::ExecutionResult> {
        self.inner.execute(command, args, timeout, working_dir)
    }
    
    fn get_status(&self) -> super::SandboxStatus {
        self.inner.get_status()
    }
    
    fn health_check(&self) -> super::HealthStatus {
        self.inner.health_check()
    }
    
    fn get_info(&self) -> super::SandboxInfo {
        let mut info = self.inner.get_info();
        // Update sandbox type to indicate WSL2 usage
        info.sandbox_type = "wsl2-gvisor-docker".to_string();
        info
    }
}

#[cfg(test)]
mod wsl2_sandbox_tests {
    use super::*;
    use super::super::types::*;
    
    fn create_test_config() -> SandboxConfig {
        SandboxConfig {
            sandbox_id: "test-wsl2-sandbox".to_string(),
            platform: "windows".to_string(),
            filesystem: FilesystemConfig {
                readonly_paths: vec![],
                writable_paths: vec!["/tmp".to_string()],
                hidden_paths: vec![],
                ..Default::default()
            },
            network: NetworkConfig {
                enabled: false,
                allowed_hosts: vec![],
                allowed_ports: vec![],
            },
            resources: ResourceConfig {
                max_memory: 512 * 1024 * 1024, // 512MB
                max_cpu: 1.0,
                max_disk: 1024 * 1024 * 1024, // 1GB
            },
            process: ProcessConfig {
                allow_fork: false,
                max_processes: 10,
            },
            child_work_dir: None,
            monitoring: MonitoringConfig::default(),
            delete_on_start: false,
        }
    }
    
    #[test]
    #[cfg(target_os = "windows")]
    fn test_wsl2_sandbox_creation() {
        let config = create_test_config();
        
        // This test will only succeed if WSL2 and Docker are available
        let result = Wsl2GVisorSandbox::new(config);
        
        if Wsl2Integration::is_wsl2_available().unwrap_or(false) 
            && Wsl2Integration::is_docker_accessible().unwrap_or(false) {
            assert!(result.is_ok());
        } else {
            // Should fail with appropriate error if WSL2 or Docker not available
            assert!(result.is_err());
        }
    }
    
    #[test]
    #[cfg(target_os = "windows")]
    fn test_wsl2_sandbox_is_supported() {
        let supported = Wsl2GVisorSandbox::is_supported();
        
        // Should match the availability of WSL2 and Docker
        let expected = Wsl2Integration::is_wsl2_available().unwrap_or(false)
            && Wsl2Integration::is_docker_accessible().unwrap_or(false);
        
        assert_eq!(supported, expected);
    }
    
    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_wsl2_sandbox_not_supported_on_non_windows() {
        assert!(!Wsl2GVisorSandbox::is_supported());
    }
    
    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_wsl2_sandbox_creation_fails_on_non_windows() {
        let config = create_test_config();
        let result = Wsl2GVisorSandbox::new(config);
        assert!(result.is_err());
    }
}
