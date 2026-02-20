// Sandbox trait definition and related interfaces

use super::error::Result;
use super::types::{ExecutionResult, HealthStatus, SandboxInfo, SandboxStatus};
use std::time::Duration;

/// Sandbox abstraction trait
/// 
/// This trait defines the core interface that all sandbox implementations must provide.
/// It supports both application sandboxes (AppContainer, nono.sh) and tool sandboxes (gVisor Docker).
pub trait Sandbox: Send + Sync {
    /// Start the sandbox
    /// 
    /// Initializes and starts the sandbox environment. This may involve:
    /// - Creating namespaces (Linux/macOS)
    /// - Setting up AppContainer (Windows)
    /// - Starting Docker containers (tool sandboxes)
    /// - Applying resource limits
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The sandbox cannot be created (insufficient permissions, missing dependencies)
    /// - Resource limits cannot be applied
    /// - The platform is not supported
    fn start(&mut self) -> Result<()>;
    
    /// Stop the sandbox
    /// 
    /// Gracefully stops the sandbox and cleans up resources. This includes:
    /// - Terminating running processes
    /// - Unmounting filesystems
    /// - Releasing network resources
    /// - Cleaning up temporary files
    /// 
    /// # Errors
    /// 
    /// Returns an error if the sandbox cannot be stopped cleanly.
    fn stop(&mut self) -> Result<()>;
    
    /// Execute a command in the sandbox
    /// 
    /// Runs a command with the given arguments inside the sandbox environment.
    /// The execution is subject to:
    /// - Filesystem access restrictions
    /// - Network access restrictions
    /// - Resource limits
    /// - Timeout control
    /// 
    /// # Arguments
    /// 
    /// * `command` - The command to execute
    /// * `args` - Command arguments
    /// * `timeout` - Maximum execution time
    /// * `working_dir` - Optional working directory inside the sandbox (e.g. `/workspace` for tool sandbox)
    /// 
    /// # Returns
    /// 
    /// Returns an `ExecutionResult` containing:
    /// - Exit code
    /// - Standard output
    /// - Standard error
    /// - Execution duration
    /// - Error message (if any)
    /// 
    /// # Errors
    /// 
    /// Returns an error if:
    /// - The command cannot be executed
    /// - The execution times out
    /// - A security violation occurs
    fn execute(
        &self,
        command: &str,
        args: &[String],
        timeout: Duration,
        working_dir: Option<&str>,
    ) -> Result<ExecutionResult>;
    
    /// Get the current status of the sandbox
    /// 
    /// Returns detailed status information including:
    /// - Current state (Created, Starting, Running, Stopping, Stopped, Error)
    /// - Timestamps (created, started, stopped)
    /// - Error information (if any)
    /// 
    /// # Returns
    /// 
    /// A `SandboxStatus` object with current state information
    fn get_status(&self) -> SandboxStatus;
    
    /// Perform a health check on the sandbox
    /// 
    /// Verifies that the sandbox is functioning correctly by checking:
    /// - Process is running
    /// - Resources are within limits
    /// - No security violations detected
    /// - Communication channels are working
    /// 
    /// # Returns
    /// 
    /// A `HealthStatus` object indicating whether the sandbox is healthy
    fn health_check(&self) -> HealthStatus;
    
    /// Get sandbox information
    /// 
    /// Returns metadata about the sandbox including:
    /// - Sandbox ID
    /// - Platform (windows, linux, macos)
    /// - Sandbox type (appcontainer, nono, gvisor-docker, etc.)
    /// 
    /// # Returns
    /// 
    /// A `SandboxInfo` object with sandbox metadata
    fn get_info(&self) -> SandboxInfo;
}
