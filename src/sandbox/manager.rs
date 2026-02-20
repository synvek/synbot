// Sandbox Manager - manages the lifecycle of sandbox instances

use super::error::{Result, SandboxError};
use super::sandbox_trait::Sandbox;
use super::types::{SandboxConfig, SandboxInfo, ExecutionResult};
use std::time::Duration;
use super::config::ConfigurationManager;
use super::monitoring::MonitoringModule;
use super::security::{SecurityValidator, EscapePrevention, PrivilegeEscalationPrevention, ResourceExhaustionPrevention};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Sandbox Manager
/// 
/// The SandboxManager is responsible for:
/// - Creating application and tool sandboxes
/// - Managing sandbox lifecycle (start, stop, destroy)
/// - Tracking active sandbox instances
/// - Coordinating between configuration and monitoring modules
/// - Enforcing security policies and validating configurations
pub struct SandboxManager {
    /// Active sandbox instances indexed by sandbox_id
    sandboxes: Arc<RwLock<HashMap<String, Box<dyn Sandbox>>>>,
    
    /// Configuration manager for loading and validating configs
    config_manager: Arc<RwLock<ConfigurationManager>>,
    
    /// Monitoring module for audit logging and metrics
    monitoring: Arc<MonitoringModule>,
    
    /// Security validator for configuration and runtime checks
    security_validator: Arc<SecurityValidator>,
}

impl SandboxManager {
    /// Create a new SandboxManager
    /// 
    /// # Arguments
    /// 
    /// * `config_manager` - Configuration manager instance
    /// * `monitoring` - Monitoring module instance
    pub fn new(
        config_manager: ConfigurationManager,
        monitoring: MonitoringModule,
    ) -> Self {
        Self {
            sandboxes: Arc::new(RwLock::new(HashMap::new())),
            config_manager: Arc::new(RwLock::new(config_manager)),
            monitoring: Arc::new(monitoring),
            security_validator: Arc::new(SecurityValidator::new()),
        }
    }
    
    /// Create a new SandboxManager with custom security validator
    /// 
    /// # Arguments
    /// 
    /// * `config_manager` - Configuration manager instance
    /// * `monitoring` - Monitoring module instance
    /// * `security_validator` - Custom security validator
    pub fn with_security_validator(
        config_manager: ConfigurationManager,
        monitoring: MonitoringModule,
        security_validator: SecurityValidator,
    ) -> Self {
        Self {
            sandboxes: Arc::new(RwLock::new(HashMap::new())),
            config_manager: Arc::new(RwLock::new(config_manager)),
            monitoring: Arc::new(monitoring),
            security_validator: Arc::new(security_validator),
        }
    }
    
    /// Create a new SandboxManager with default configuration
    /// 
    /// This is a convenience constructor for testing and simple use cases.
    pub fn with_defaults() -> Self {
        let config_manager = ConfigurationManager::new("sandbox_config.json".to_string());
        let monitoring = MonitoringModule::new(Default::default());
        Self::new(config_manager, monitoring)
    }
    
    /// Create an application sandbox instance
    /// 
    /// Creates a sandbox for running applications based on the platform:
    /// - Windows: AppContainer or Sandboxie-Plus
    /// - Linux/macOS: nono.sh
    /// 
    /// Uses automatic platform detection if config.platform is "auto".
    /// 
    /// # Arguments
    /// 
    /// * `config` - Sandbox configuration
    /// 
    /// # Returns
    /// 
    /// Returns the sandbox ID on success
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The configuration fails security validation
    /// - The platform is not supported
    /// - The sandbox cannot be created
    /// - A sandbox with the same ID already exists
    pub async fn create_app_sandbox(&self, config: SandboxConfig) -> Result<String> {
        // Validate configuration for security issues
        self.security_validator.validate_config(&config)?;
        
        // Check if sandbox already exists
        {
            let sandboxes = self.sandboxes.read().await;
            if sandboxes.contains_key(&config.sandbox_id) {
                return Err(SandboxError::CreationFailed(
                    format!("Sandbox with ID '{}' already exists", config.sandbox_id)
                ));
            }
        }
        
        // Verify minimal privileges before creating sandbox
        PrivilegeEscalationPrevention::verify_minimal_privileges()?;
        
        // Use SandboxFactory for platform-appropriate sandbox creation
        let sandbox = super::platform::SandboxFactory::create_app_sandbox(config.clone())?;
        let sandbox_id = config.sandbox_id.clone();
        
        // Store sandbox instance
        {
            let mut sandboxes = self.sandboxes.write().await;
            sandboxes.insert(sandbox_id.clone(), sandbox);
        }
        
        // Log sandbox creation
        self.monitoring.log_sandbox_created(&sandbox_id, "app");
        
        Ok(sandbox_id)
    }
    
    /// Create a tool sandbox instance
    /// 
    /// Creates a sandbox for running tools based on the platform:
    /// - Windows: WSL2 + gVisor Docker
    /// - Linux/macOS: gVisor Docker
    /// 
    /// Uses automatic platform detection if config.platform is "auto".
    /// 
    /// # Arguments
    /// 
    /// * `config` - Sandbox configuration
    /// 
    /// # Returns
    /// 
    /// Returns the sandbox ID on success
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The configuration fails security validation
    /// - Docker is not available
    /// - gVisor runtime is not installed
    /// - On Windows: WSL2 is not available
    /// - The sandbox cannot be created
    /// - A sandbox with the same ID already exists
    pub async fn create_tool_sandbox(&self, config: SandboxConfig) -> Result<String> {
        // Validate configuration for security issues
        self.security_validator.validate_config(&config)?;
        
        // Check if sandbox already exists
        {
            let sandboxes = self.sandboxes.read().await;
            if sandboxes.contains_key(&config.sandbox_id) {
                return Err(SandboxError::CreationFailed(
                    format!("Sandbox with ID '{}' already exists", config.sandbox_id)
                ));
            }
        }
        
        // Verify minimal privileges before creating sandbox
        PrivilegeEscalationPrevention::verify_minimal_privileges()?;

        let sandbox_id = config.sandbox_id.clone();

        // Create only the requested sandbox type (no fallback); if environment doesn't match, fail and user can change config.
        let sandbox = super::platform::SandboxFactory::create_tool_sandbox(config)?;

        // Store sandbox instance
        {
            let mut sandboxes = self.sandboxes.write().await;
            sandboxes.insert(sandbox_id.clone(), sandbox);
        }

        // Log sandbox creation
        self.monitoring.log_sandbox_created(&sandbox_id, "tool");

        Ok(sandbox_id)
    }
    
    /// Get a sandbox instance by ID
    /// 
    /// # Arguments
    /// 
    /// * `sandbox_id` - The sandbox ID to look up
    /// 
    /// # Returns
    /// 
    /// Returns `Some(&dyn Sandbox)` if found, `None` otherwise
    pub async fn get_sandbox(&self, sandbox_id: &str) -> Option<SandboxInfo> {
        let sandboxes = self.sandboxes.read().await;
        sandboxes.get(sandbox_id).map(|s| s.get_info())
    }
    
    /// List all active sandboxes
    /// 
    /// # Returns
    /// 
    /// A vector of `SandboxInfo` for all active sandboxes
    pub async fn list_sandboxes(&self) -> Vec<SandboxInfo> {
        let sandboxes = self.sandboxes.read().await;
        sandboxes.values()
            .map(|s| s.get_info())
            .collect()
    }
    
    /// Destroy a sandbox
    /// 
    /// Stops and removes a sandbox instance, cleaning up all resources.
    /// 
    /// # Arguments
    /// 
    /// * `sandbox_id` - The sandbox ID to destroy
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The sandbox is not found
    /// - The sandbox cannot be stopped
    pub async fn destroy_sandbox(&self, sandbox_id: &str) -> Result<()> {
        let mut sandboxes = self.sandboxes.write().await;
        
        if let Some(mut sandbox) = sandboxes.remove(sandbox_id) {
            // Stop the sandbox before destroying
            sandbox.stop()?;
            Ok(())
        } else {
            Err(SandboxError::NotFound)
        }
    }
    
    /// Start a sandbox by ID
    /// 
    /// # Arguments
    /// 
    /// * `sandbox_id` - The sandbox ID to start
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The sandbox is not found
    /// - The sandbox cannot be started
    pub async fn start_sandbox(&self, sandbox_id: &str) -> Result<()> {
        let mut sandboxes = self.sandboxes.write().await;
        
        if let Some(sandbox) = sandboxes.get_mut(sandbox_id) {
            sandbox.start()?;
            Ok(())
        } else {
            Err(SandboxError::NotFound)
        }
    }
    
    /// Stop a sandbox by ID
    /// 
    /// # Arguments
    /// 
    /// * `sandbox_id` - The sandbox ID to stop
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The sandbox is not found
    /// - The sandbox cannot be stopped
    pub async fn stop_sandbox(&self, sandbox_id: &str) -> Result<()> {
        let mut sandboxes = self.sandboxes.write().await;
        
        if let Some(sandbox) = sandboxes.get_mut(sandbox_id) {
            sandbox.stop()?;
            Ok(())
        } else {
            Err(SandboxError::NotFound)
        }
    }
    
    /// Get the number of active sandboxes
    pub async fn sandbox_count(&self) -> usize {
        let sandboxes = self.sandboxes.read().await;
        sandboxes.len()
    }
    
    /// Check if a sandbox exists
    /// 
    /// # Arguments
    /// 
    /// * `sandbox_id` - The sandbox ID to check
    /// 
    /// # Returns
    /// 
    /// `true` if the sandbox exists, `false` otherwise
    pub async fn sandbox_exists(&self, sandbox_id: &str) -> bool {
        let sandboxes = self.sandboxes.read().await;
        sandboxes.contains_key(sandbox_id)
    }
    
    /// Execute a command inside a sandbox.
    /// 
    /// # Arguments
    /// 
    /// * `sandbox_id` - The sandbox ID (e.g. "synbot-tool")
    /// * `command` - The command to run (e.g. "sh" or "cmd")
    /// * `args` - Command arguments (e.g. ["-c", "echo hello"])
    /// * `timeout` - Maximum execution time
    /// * `working_dir` - Optional working directory inside the sandbox (e.g. `/workspace`)
    /// 
    /// # Errors
    /// 
    /// Returns an error if the sandbox is not found or execution fails.
    pub async fn execute_in_sandbox(
        &self,
        sandbox_id: &str,
        command: &str,
        args: &[String],
        timeout: Duration,
        working_dir: Option<&str>,
    ) -> Result<ExecutionResult> {
        let sandboxes = self.sandboxes.read().await;
        let sandbox = sandboxes
            .get(sandbox_id)
            .ok_or(SandboxError::NotFound)?;
        sandbox.execute(command, args, timeout, working_dir)
    }
    
    /// Get reference to the monitoring module
    pub fn monitoring(&self) -> &Arc<MonitoringModule> {
        &self.monitoring
    }
    
    /// Get reference to the configuration manager
    pub fn config_manager(&self) -> &Arc<RwLock<ConfigurationManager>> {
        &self.config_manager
    }
    
    /// Get reference to the security validator
    pub fn security_validator(&self) -> &Arc<SecurityValidator> {
        &self.security_validator
    }
    
    /// Validate a command before execution
    /// 
    /// Checks commands for security issues before allowing execution.
    /// 
    /// # Arguments
    /// 
    /// * `command` - Command to validate
    /// * `args` - Command arguments
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if command is safe, or an error if security issue detected
    pub fn validate_command(&self, command: &str, args: &[String]) -> Result<()> {
        self.security_validator.validate_command(command, args)
    }
    
    /// Verify sandbox isolation integrity
    /// 
    /// Performs runtime checks to ensure sandbox hasn't been compromised.
    /// 
    /// # Arguments
    /// 
    /// * `sandbox_id` - ID of sandbox to check
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if isolation is intact, or an error if compromise detected
    pub async fn verify_sandbox_isolation(&self, sandbox_id: &str) -> Result<()> {
        // Check if sandbox exists
        if !self.sandbox_exists(sandbox_id).await {
            return Err(SandboxError::NotFound);
        }
        
        // Verify isolation integrity
        EscapePrevention::verify_isolation(sandbox_id)?;
        
        // Check for escape attempts
        EscapePrevention::check_escape_attempts()?;
        
        Ok(())
    }
    
    /// Enforce resource limits for a sandbox
    /// 
    /// Actively monitors and enforces resource limits.
    /// 
    /// # Arguments
    /// 
    /// * `sandbox_id` - ID of sandbox to check
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if resources are within limits, or an error if exceeded
    pub async fn enforce_resource_limits(&self, sandbox_id: &str) -> Result<()> {
        let sandboxes = self.sandboxes.read().await;
        
        if let Some(_sandbox) = sandboxes.get(sandbox_id) {
            // In a real implementation, we would fetch the actual config for this sandbox
            // For now, we perform basic resource checks without config
            ResourceExhaustionPrevention::check_file_descriptors()?;
            
            Ok(())
        } else {
            Err(SandboxError::NotFound)
        }
    }
    
    /// Verify no-new-privileges flag is set
    /// 
    /// Ensures the sandbox process cannot gain new privileges.
    /// 
    /// # Returns
    /// 
    /// Returns `Ok(())` if flag is set, or an error if not
    pub fn verify_no_new_privileges(&self) -> Result<()> {
        PrivilegeEscalationPrevention::ensure_no_new_privileges()
    }
    
    /// Verify isolation between two sandboxes
    /// 
    /// Checks that two sandboxes are properly isolated from each other.
    /// This is particularly important for dual-layer isolation (app sandbox vs tool sandbox).
    /// 
    /// # Arguments
    /// 
    /// * `sandbox_a_id` - ID of the first sandbox
    /// * `sandbox_b_id` - ID of the second sandbox
    /// 
    /// # Returns
    /// 
    /// An `IsolationVerification` result
    /// 
    /// # Errors
    /// 
    /// Returns an error if either sandbox is not found
    pub async fn verify_isolation(
        &self,
        sandbox_a_id: &str,
        sandbox_b_id: &str,
    ) -> Result<super::isolation::IsolationVerification> {
        use super::isolation::IsolationVerifier;
        
        let sandboxes = self.sandboxes.read().await;
        
        let sandbox_a = sandboxes.get(sandbox_a_id)
            .ok_or(SandboxError::NotFound)?;
        let sandbox_b = sandboxes.get(sandbox_b_id)
            .ok_or(SandboxError::NotFound)?;
        
        let info_a = sandbox_a.get_info();
        let info_b = sandbox_b.get_info();
        
        let verifier = IsolationVerifier::new();
        Ok(verifier.verify_isolation(&info_a, &info_b))
    }
    
    /// Transfer execution result from tool sandbox to app sandbox
    /// 
    /// Safely transfers an execution result through a filtered channel,
    /// removing any executable code or malicious payloads.
    /// 
    /// # Arguments
    /// 
    /// * `result` - The execution result to transfer
    /// 
    /// # Returns
    /// 
    /// A filtered and sanitized execution result
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The result contains malicious content that cannot be filtered
    /// - The result size exceeds limits
    pub fn transfer_result(
        &self,
        result: super::types::ExecutionResult,
    ) -> Result<super::types::ExecutionResult> {
        use super::isolation::CrossSandboxChannel;
        
        let channel = CrossSandboxChannel::new();
        channel.transfer_result(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::types::*;
    use std::time::Duration;
    
    fn create_test_config(sandbox_id: &str, platform: &str) -> SandboxConfig {
        SandboxConfig {
            sandbox_id: sandbox_id.to_string(),
            platform: platform.to_string(),
            filesystem: FilesystemConfig {
                readonly_paths: vec!["/usr".to_string()],
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
                max_memory: 1024 * 1024 * 1024,
                max_cpu: 1.0,
                max_disk: 5 * 1024 * 1024 * 1024,
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
    
    #[tokio::test]
    async fn test_sandbox_manager_creation() {
        let manager = SandboxManager::with_defaults();
        assert_eq!(manager.sandbox_count().await, 0);
    }
    
    #[tokio::test]
    async fn test_sandbox_exists() {
        let manager = SandboxManager::with_defaults();
        assert!(!manager.sandbox_exists("test-sandbox").await);
    }
    
    #[tokio::test]
    async fn test_list_empty_sandboxes() {
        let manager = SandboxManager::with_defaults();
        let sandboxes = manager.list_sandboxes().await;
        assert_eq!(sandboxes.len(), 0);
    }
    
    #[tokio::test]
    async fn test_get_nonexistent_sandbox() {
        let manager = SandboxManager::with_defaults();
        let result = manager.get_sandbox("nonexistent").await;
        assert!(result.is_none());
    }
    
    #[tokio::test]
    async fn test_destroy_nonexistent_sandbox() {
        let manager = SandboxManager::with_defaults();
        let result = manager.destroy_sandbox("nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SandboxError::NotFound));
    }
    
    #[tokio::test]
    async fn test_start_nonexistent_sandbox() {
        let manager = SandboxManager::with_defaults();
        let result = manager.start_sandbox("nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SandboxError::NotFound));
    }
    
    #[tokio::test]
    async fn test_stop_nonexistent_sandbox() {
        let manager = SandboxManager::with_defaults();
        let result = manager.stop_sandbox("nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SandboxError::NotFound));
    }
    
    // Note: Tests for actual sandbox creation will be added once
    // platform-specific implementations are available (tasks 6, 7, 9)
    
    #[tokio::test]
    async fn test_verify_isolation_nonexistent_sandboxes() {
        let manager = SandboxManager::with_defaults();
        
        let result = manager.verify_isolation("sandbox-a", "sandbox-b").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SandboxError::NotFound));
    }
    
    #[tokio::test]
    async fn test_transfer_safe_result() {
        let manager = SandboxManager::with_defaults();
        
        let result = ExecutionResult {
            exit_code: 0,
            stdout: b"Hello, World!".to_vec(),
            stderr: vec![],
            duration: Duration::from_secs(1),
            error: None,
        };
        
        let transferred = manager.transfer_result(result);
        assert!(transferred.is_ok());
        
        let transferred_result = transferred.unwrap();
        assert_eq!(transferred_result.stdout, b"Hello, World!");
        assert_eq!(transferred_result.exit_code, 0);
    }
    
    #[tokio::test]
    async fn test_transfer_malicious_result() {
        let manager = SandboxManager::with_defaults();
        
        // Result with ELF executable
        let result = ExecutionResult {
            exit_code: 0,
            stdout: vec![0x7F, 0x45, 0x4C, 0x46, 0x01, 0x02],
            stderr: vec![],
            duration: Duration::from_secs(1),
            error: None,
        };
        
        let transferred = manager.transfer_result(result);
        assert!(transferred.is_err());
        assert!(matches!(
            transferred.unwrap_err(),
            SandboxError::SecurityViolation(_)
        ));
    }
}
