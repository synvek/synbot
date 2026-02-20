// Sandbox Recovery Module
//
// This module provides automatic recovery mechanisms for sandboxes that enter
// error states, including exponential backoff retry and health checking.

use super::error::{Result, SandboxError};
use super::sandbox_trait::Sandbox;
use super::types::{HealthStatus, SandboxState};
use std::time::Duration;
use tokio::time::sleep;

/// Recovery configuration
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Maximum number of recovery attempts
    pub max_retries: u32,
    
    /// Initial backoff duration in seconds
    pub initial_backoff_secs: u64,
    
    /// Maximum backoff duration in seconds
    pub max_backoff_secs: u64,
    
    /// Health check timeout in seconds
    pub health_check_timeout_secs: u64,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_secs: 2,
            max_backoff_secs: 60,
            health_check_timeout_secs: 10,
        }
    }
}

/// Recovery result
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryResult {
    /// Recovery succeeded
    Success,
    
    /// Recovery failed after all retries
    Failed(String),
    
    /// Recovery not attempted (error not recoverable)
    NotRecoverable,
}

/// Sandbox recovery manager
pub struct RecoveryManager {
    config: RecoveryConfig,
}

impl RecoveryManager {
    /// Create a new recovery manager with default configuration
    pub fn new() -> Self {
        Self {
            config: RecoveryConfig::default(),
        }
    }
    
    /// Create a new recovery manager with custom configuration
    pub fn with_config(config: RecoveryConfig) -> Self {
        Self { config }
    }
    
    /// Attempt to recover a sandbox
    /// 
    /// This function tries to recover a sandbox that has entered an error state
    /// by stopping it, cleaning up resources, and restarting it. It uses
    /// exponential backoff between retry attempts.
    /// 
    /// # Arguments
    /// 
    /// * `sandbox` - The sandbox to recover
    /// 
    /// # Returns
    /// 
    /// Returns `RecoveryResult::Success` if recovery succeeded,
    /// `RecoveryResult::Failed` if all retries were exhausted,
    /// or `RecoveryResult::NotRecoverable` if the error is not recoverable.
    pub async fn recover_sandbox(&self, sandbox: &mut Box<dyn Sandbox>) -> RecoveryResult {
        let sandbox_id = sandbox.get_info().sandbox_id.clone();
        
        log::info!("Starting recovery for sandbox: {}", sandbox_id);
        
        for attempt in 0..self.config.max_retries {
            log::info!(
                "Recovery attempt {}/{} for sandbox: {}",
                attempt + 1,
                self.config.max_retries,
                sandbox_id
            );
            
            match self.try_recover(sandbox).await {
                Ok(()) => {
                    // Perform health check
                    log::info!("Recovery attempt succeeded, performing health check...");
                    
                    match self.health_check_with_timeout(sandbox).await {
                        Ok(health) if health.healthy => {
                            log::info!("Sandbox {} recovered successfully", sandbox_id);
                            return RecoveryResult::Success;
                        }
                        Ok(health) => {
                            log::warn!(
                                "Sandbox {} health check failed: {}",
                                sandbox_id,
                                health.message
                            );
                        }
                        Err(e) => {
                            log::warn!(
                                "Sandbox {} health check error: {}",
                                sandbox_id,
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    log::warn!(
                        "Recovery attempt {} failed for sandbox {}: {}",
                        attempt + 1,
                        sandbox_id,
                        e
                    );
                    
                    // Check if error is recoverable
                    if !e.is_recoverable() {
                        log::error!(
                            "Error is not recoverable for sandbox {}: {}",
                            sandbox_id,
                            e
                        );
                        return RecoveryResult::NotRecoverable;
                    }
                }
            }
            
            // Exponential backoff before next retry
            if attempt < self.config.max_retries - 1 {
                let backoff = self.calculate_backoff(attempt);
                log::info!(
                    "Waiting {} seconds before next recovery attempt...",
                    backoff.as_secs()
                );
                sleep(backoff).await;
            }
        }
        
        log::error!(
            "Failed to recover sandbox {} after {} attempts",
            sandbox_id,
            self.config.max_retries
        );
        
        RecoveryResult::Failed(format!(
            "Failed to recover after {} attempts",
            self.config.max_retries
        ))
    }
    
    /// Try to recover a sandbox once
    async fn try_recover(&self, sandbox: &mut Box<dyn Sandbox>) -> Result<()> {
        let sandbox_id = sandbox.get_info().sandbox_id.clone();
        
        // Step 1: Stop the sandbox if it's running
        log::debug!("Stopping sandbox: {}", sandbox_id);
        if let Err(e) = sandbox.stop() {
            log::warn!("Failed to stop sandbox {}: {}", sandbox_id, e);
            // Continue anyway - sandbox might already be stopped
        }
        
        // Step 2: Clean up resources (implementation depends on sandbox type)
        log::debug!("Cleaning up resources for sandbox: {}", sandbox_id);
        // Resource cleanup is handled by the stop() method in each implementation
        
        // Step 3: Restart the sandbox
        log::debug!("Restarting sandbox: {}", sandbox_id);
        sandbox.start()?;
        
        Ok(())
    }
    
    /// Perform health check with timeout
    async fn health_check_with_timeout(&self, sandbox: &Box<dyn Sandbox>) -> Result<HealthStatus> {
        let timeout = Duration::from_secs(self.config.health_check_timeout_secs);
        
        match tokio::time::timeout(timeout, async {
            Ok(sandbox.health_check())
        }).await {
            Ok(result) => result,
            Err(_) => Err(SandboxError::Timeout),
        }
    }
    
    /// Calculate exponential backoff duration
    fn calculate_backoff(&self, attempt: u32) -> Duration {
        let backoff_secs = self.config.initial_backoff_secs * 2_u64.pow(attempt);
        let backoff_secs = backoff_secs.min(self.config.max_backoff_secs);
        Duration::from_secs(backoff_secs)
    }
}

impl Default for RecoveryManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to recover a sandbox with default configuration
/// 
/// # Arguments
/// 
/// * `sandbox` - The sandbox to recover
/// 
/// # Returns
/// 
/// Returns `true` if recovery succeeded, `false` otherwise
pub async fn recover_sandbox(sandbox: &mut Box<dyn Sandbox>) -> bool {
    let manager = RecoveryManager::new();
    matches!(manager.recover_sandbox(sandbox).await, RecoveryResult::Success)
}

/// Convenience function to recover a sandbox with custom retry count
/// 
/// # Arguments
/// 
/// * `sandbox` - The sandbox to recover
/// * `max_retries` - Maximum number of recovery attempts
/// 
/// # Returns
/// 
/// Returns `true` if recovery succeeded, `false` otherwise
pub async fn recover_sandbox_with_retries(
    sandbox: &mut Box<dyn Sandbox>,
    max_retries: u32,
) -> bool {
    let config = RecoveryConfig {
        max_retries,
        ..Default::default()
    };
    let manager = RecoveryManager::with_config(config);
    matches!(manager.recover_sandbox(sandbox).await, RecoveryResult::Success)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::types::*;
    use std::sync::{Arc, Mutex};
    
    // Mock sandbox for testing
    struct MockSandbox {
        info: SandboxInfo,
        state: Arc<Mutex<SandboxState>>,
        start_count: Arc<Mutex<u32>>,
        stop_count: Arc<Mutex<u32>>,
        should_fail_start: bool,
        should_fail_health: bool,
    }
    
    impl MockSandbox {
        fn new(sandbox_id: &str) -> Self {
            Self {
                info: SandboxInfo {
                    sandbox_id: sandbox_id.to_string(),
                    platform: "test".to_string(),
                    sandbox_type: "mock".to_string(),
                },
                state: Arc::new(Mutex::new(SandboxState::Created)),
                start_count: Arc::new(Mutex::new(0)),
                stop_count: Arc::new(Mutex::new(0)),
                should_fail_start: false,
                should_fail_health: false,
            }
        }
        
        fn new_with_failures(sandbox_id: &str, fail_start: bool, fail_health: bool) -> Self {
            Self {
                info: SandboxInfo {
                    sandbox_id: sandbox_id.to_string(),
                    platform: "test".to_string(),
                    sandbox_type: "mock".to_string(),
                },
                state: Arc::new(Mutex::new(SandboxState::Created)),
                start_count: Arc::new(Mutex::new(0)),
                stop_count: Arc::new(Mutex::new(0)),
                should_fail_start: fail_start,
                should_fail_health: fail_health,
            }
        }
        
        fn get_start_count(&self) -> u32 {
            *self.start_count.lock().unwrap()
        }
    }
    
    impl Sandbox for MockSandbox {
        fn start(&mut self) -> Result<()> {
            *self.start_count.lock().unwrap() += 1;
            
            if self.should_fail_start {
                return Err(SandboxError::ExecutionFailed("Mock failure".to_string()));
            }
            
            *self.state.lock().unwrap() = SandboxState::Running;
            Ok(())
        }
        
        fn stop(&mut self) -> Result<()> {
            *self.stop_count.lock().unwrap() += 1;
            *self.state.lock().unwrap() = SandboxState::Stopped;
            Ok(())
        }
        
        fn execute(
            &self,
            _command: &str,
            _args: &[String],
            _timeout: Duration,
            _working_dir: Option<&str>,
        ) -> Result<ExecutionResult> {
            unimplemented!()
        }
        
        fn get_status(&self) -> SandboxStatus {
            SandboxStatus {
                sandbox_id: self.info.sandbox_id.clone(),
                state: self.state.lock().unwrap().clone(),
                created_at: chrono::Utc::now(),
                started_at: None,
                stopped_at: None,
                error: None,
            }
        }
        
        fn health_check(&self) -> HealthStatus {
            let healthy = !self.should_fail_health;
            HealthStatus {
                healthy,
                checks: std::collections::HashMap::new(),
                message: if healthy {
                    "Healthy".to_string()
                } else {
                    "Unhealthy".to_string()
                },
            }
        }
        
        fn get_info(&self) -> SandboxInfo {
            self.info.clone()
        }
    }
    
    #[tokio::test]
    async fn test_recovery_success() {
        let mut sandbox: Box<dyn Sandbox> = Box::new(MockSandbox::new("test-sandbox"));
        
        let manager = RecoveryManager::new();
        let result = manager.recover_sandbox(&mut sandbox).await;
        
        assert_eq!(result, RecoveryResult::Success);
    }
    
    #[tokio::test]
    async fn test_recovery_with_retry() {
        let mock = MockSandbox::new_with_failures("test-sandbox", true, false);
        let start_count_ref = mock.start_count.clone();
        let mut sandbox: Box<dyn Sandbox> = Box::new(mock);
        
        let config = RecoveryConfig {
            max_retries: 3,
            initial_backoff_secs: 0, // Use 0 for faster tests
            ..Default::default()
        };
        let manager = RecoveryManager::with_config(config);
        let result = manager.recover_sandbox(&mut sandbox).await;
        
        // Should fail because start always fails
        assert!(matches!(result, RecoveryResult::Failed(_)));
        
        // Verify that we attempted to start multiple times
        assert_eq!(*start_count_ref.lock().unwrap(), 3);
    }
    
    #[tokio::test]
    async fn test_recovery_health_check_failure() {
        let mock = MockSandbox::new_with_failures("test-sandbox", false, true);
        let mut sandbox: Box<dyn Sandbox> = Box::new(mock);
        
        let config = RecoveryConfig {
            max_retries: 2,
            initial_backoff_secs: 0, // Use 0 for faster tests
            ..Default::default()
        };
        let manager = RecoveryManager::with_config(config);
        let result = manager.recover_sandbox(&mut sandbox).await;
        
        // Should fail because health check always fails
        assert!(matches!(result, RecoveryResult::Failed(_)));
    }
    
    #[tokio::test]
    async fn test_backoff_calculation() {
        let manager = RecoveryManager::new();
        
        assert_eq!(manager.calculate_backoff(0), Duration::from_secs(2));
        assert_eq!(manager.calculate_backoff(1), Duration::from_secs(4));
        assert_eq!(manager.calculate_backoff(2), Duration::from_secs(8));
        assert_eq!(manager.calculate_backoff(3), Duration::from_secs(16));
    }
    
    #[tokio::test]
    async fn test_backoff_max_limit() {
        let config = RecoveryConfig {
            max_backoff_secs: 10,
            ..Default::default()
        };
        let manager = RecoveryManager::with_config(config);
        
        // Should be capped at max_backoff_secs
        assert_eq!(manager.calculate_backoff(10), Duration::from_secs(10));
    }
    
    #[tokio::test]
    async fn test_convenience_function() {
        let mut sandbox: Box<dyn Sandbox> = Box::new(MockSandbox::new("test-sandbox"));
        
        let result = recover_sandbox(&mut sandbox).await;
        assert!(result);
    }
    
    #[tokio::test]
    async fn test_convenience_function_with_retries() {
        let mut sandbox: Box<dyn Sandbox> = Box::new(MockSandbox::new("test-sandbox"));
        
        let result = recover_sandbox_with_retries(&mut sandbox, 5).await;
        assert!(result);
    }
}
