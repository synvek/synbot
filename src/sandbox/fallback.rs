// Sandbox Fallback Strategy Module
//
// This module provides fallback mechanisms when primary sandbox implementations
// are not available or fail to initialize.

use super::error::{Result, SandboxError};
use super::sandbox_trait::Sandbox;
use super::types::SandboxConfig;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Fallback strategy configuration
#[derive(Debug, Clone)]
pub struct FallbackConfig {
    /// Enable fallback to alternative implementations
    pub enable_fallback: bool,
    
    /// Log warnings when falling back
    pub log_fallback_warnings: bool,
    
    /// Allow fallback to less secure implementations
    pub allow_insecure_fallback: bool,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            enable_fallback: true,
            log_fallback_warnings: true,
            allow_insecure_fallback: false,
        }
    }
}

/// Fallback result
#[derive(Debug, Clone, PartialEq)]
pub enum FallbackResult {
    /// Primary implementation succeeded
    Primary,
    
    /// Fell back to alternative implementation
    Fallback(String),
    
    /// All implementations failed
    Failed(String),
}

/// Sandbox fallback manager
pub struct FallbackManager {
    config: FallbackConfig,
    fallback_history: Arc<RwLock<Vec<FallbackEvent>>>,
}

/// Fallback event for tracking
#[derive(Debug, Clone)]
pub struct FallbackEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub sandbox_id: String,
    pub primary_impl: String,
    pub fallback_impl: String,
    pub reason: String,
}

impl FallbackManager {
    /// Create a new fallback manager with default configuration
    pub fn new() -> Self {
        Self {
            config: FallbackConfig::default(),
            fallback_history: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Create a new fallback manager with custom configuration
    pub fn with_config(config: FallbackConfig) -> Self {
        Self {
            config,
            fallback_history: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Create an application sandbox with fallback support
    /// 
    /// On Windows: Tries AppContainer first, falls back to Sandboxie-Plus if available
    /// On Linux/macOS: Uses nono.sh (no fallback currently)
    /// 
    /// # Arguments
    /// 
    /// * `config` - Sandbox configuration
    /// 
    /// # Returns
    /// 
    /// Returns a tuple of (Sandbox, FallbackResult)
    pub async fn create_app_sandbox_with_fallback(
        &self,
        config: SandboxConfig,
    ) -> Result<(Box<dyn Sandbox>, FallbackResult)> {
        #[cfg(target_os = "windows")]
        {
            use crate::sandbox::WindowsAppContainerSandbox;
            
            // Try primary implementation: AppContainer
            match WindowsAppContainerSandbox::new(config.clone()) {
                Ok(sandbox) => {
                    log::info!("Using primary AppContainer sandbox for: {}", config.sandbox_id);
                    return Ok((Box::new(sandbox), FallbackResult::Primary));
                }
                Err(e) => {
                    if !self.config.enable_fallback {
                        return Err(e);
                    }
                    
                    if self.config.log_fallback_warnings {
                        log::warn!(
                            "AppContainer creation failed for {}: {}. Attempting fallback...",
                            config.sandbox_id,
                            e
                        );
                    }
                    
                    // Try fallback: Sandboxie-Plus (if implemented)
                    // For now, return error as Sandboxie-Plus is not yet implemented
                    self.record_fallback_event(
                        &config.sandbox_id,
                        "AppContainer",
                        "Sandboxie-Plus",
                        &format!("AppContainer failed: {}", e),
                    ).await;
                    
                    return Err(SandboxError::CreationFailed(
                        format!("Primary and fallback implementations failed: {}", e)
                    ));
                }
            }
        }
        
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            use crate::sandbox::NonoSandbox;
            
            // Try nono.sh (no fallback currently)
            match NonoSandbox::new(config.clone()) {
                Ok(sandbox) => {
                    log::info!("Using nono.sh sandbox for: {}", config.sandbox_id);
                    Ok((Box::new(sandbox), FallbackResult::Primary))
                }
                Err(e) => {
                    log::error!("nono.sh creation failed for {}: {}", config.sandbox_id, e);
                    Err(e)
                }
            }
        }
        
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            Err(SandboxError::UnsupportedPlatform)
        }
    }
    
    /// Create a tool sandbox with fallback support
    /// 
    /// Tries gVisor Docker first, falls back to standard Docker if gVisor is not available
    /// 
    /// # Arguments
    /// 
    /// * `config` - Sandbox configuration
    /// 
    /// # Returns
    /// 
    /// Returns a tuple of (Sandbox, FallbackResult)
    pub async fn create_tool_sandbox_with_fallback(
        &self,
        config: SandboxConfig,
    ) -> Result<(Box<dyn Sandbox>, FallbackResult)> {
        #[cfg(target_os = "windows")]
        {
            use crate::sandbox::Wsl2GVisorSandbox;
            
            // Try primary implementation: WSL2 + gVisor Docker
            match Wsl2GVisorSandbox::new(config.clone()) {
                Ok(sandbox) => {
                    log::info!("Using WSL2 + gVisor Docker sandbox for: {}", config.sandbox_id);
                    return Ok((Box::new(sandbox), FallbackResult::Primary));
                }
                Err(e) => {
                    if !self.config.enable_fallback {
                        return Err(e);
                    }
                    
                    if self.config.log_fallback_warnings {
                        log::warn!(
                            "WSL2 + gVisor creation failed for {}: {}. Attempting fallback to standard Docker...",
                            config.sandbox_id,
                            e
                        );
                    }
                    
                    // Try fallback: Standard Docker (less secure)
                    if !self.config.allow_insecure_fallback {
                        return Err(SandboxError::CreationFailed(
                            format!("gVisor not available and insecure fallback is disabled: {}", e)
                        ));
                    }
                    
                    self.record_fallback_event(
                        &config.sandbox_id,
                        "WSL2+gVisor",
                        "Standard Docker",
                        &format!("gVisor failed: {}", e),
                    ).await;
                    
                    // Create standard Docker sandbox (implementation would go here)
                    return Err(SandboxError::CreationFailed(
                        format!("Standard Docker fallback not yet implemented: {}", e)
                    ));
                }
            }
        }
        
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            use crate::sandbox::GVisorDockerSandbox;
            
            // Try primary implementation: gVisor Docker
            match GVisorDockerSandbox::new(config.clone()) {
                Ok(sandbox) => {
                    log::info!("Using gVisor Docker sandbox for: {}", config.sandbox_id);
                    return Ok((Box::new(sandbox), FallbackResult::Primary));
                }
                Err(e) => {
                    if !self.config.enable_fallback {
                        return Err(e);
                    }
                    
                    if self.config.log_fallback_warnings {
                        log::warn!(
                            "gVisor Docker creation failed for {}: {}. Attempting fallback to standard Docker...",
                            config.sandbox_id,
                            e
                        );
                    }
                    
                    // Try fallback: Standard Docker (less secure)
                    if !self.config.allow_insecure_fallback {
                        return Err(SandboxError::CreationFailed(
                            format!("gVisor not available and insecure fallback is disabled: {}", e)
                        ));
                    }
                    
                    self.record_fallback_event(
                        &config.sandbox_id,
                        "gVisor Docker",
                        "Standard Docker",
                        &format!("gVisor failed: {}", e),
                    ).await;
                    
                    // Create standard Docker sandbox (implementation would go here)
                    return Err(SandboxError::CreationFailed(
                        format!("Standard Docker fallback not yet implemented: {}", e)
                    ));
                }
            }
        }
        
        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            Err(SandboxError::UnsupportedPlatform)
        }
    }
    
    /// Record a fallback event
    async fn record_fallback_event(
        &self,
        sandbox_id: &str,
        primary_impl: &str,
        fallback_impl: &str,
        reason: &str,
    ) {
        let event = FallbackEvent {
            timestamp: chrono::Utc::now(),
            sandbox_id: sandbox_id.to_string(),
            primary_impl: primary_impl.to_string(),
            fallback_impl: fallback_impl.to_string(),
            reason: reason.to_string(),
        };
        
        let mut history = self.fallback_history.write().await;
        history.push(event);
        
        // Keep only last 100 events
        if history.len() > 100 {
            history.remove(0);
        }
    }
    
    /// Get fallback history
    pub async fn get_fallback_history(&self) -> Vec<FallbackEvent> {
        self.fallback_history.read().await.clone()
    }
    
    /// Clear fallback history
    pub async fn clear_fallback_history(&self) {
        self.fallback_history.write().await.clear();
    }
    
    /// Check if a specific sandbox implementation is available
    pub fn is_implementation_available(&self, implementation: &str) -> bool {
        match implementation {
            "appcontainer" => {
                #[cfg(target_os = "windows")]
                {
                    // Check if AppContainer is available
                    // This would involve checking Windows version and capabilities
                    true // Simplified for now
                }
                #[cfg(not(target_os = "windows"))]
                {
                    false
                }
            }
            "nono" => {
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                {
                    // Check if nono.sh is available
                    true // Simplified for now
                }
                #[cfg(not(any(target_os = "linux", target_os = "macos")))]
                {
                    false
                }
            }
            "gvisor" => {
                // Check if gVisor is available
                // This would involve checking Docker and gVisor runtime
                true // Simplified for now
            }
            _ => false,
        }
    }
}

impl Default for FallbackManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::types::*;
    
    fn create_test_config(sandbox_id: &str) -> SandboxConfig {
        SandboxConfig {
            sandbox_id: sandbox_id.to_string(),
            platform: "auto".to_string(),
            filesystem: FilesystemConfig {
                readonly_paths: vec![],
                writable_paths: vec![],
                hidden_paths: vec![],
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
        }
    }
    
    #[tokio::test]
    async fn test_fallback_manager_creation() {
        let manager = FallbackManager::new();
        assert!(manager.config.enable_fallback);
        assert!(manager.config.log_fallback_warnings);
        assert!(!manager.config.allow_insecure_fallback);
    }
    
    #[tokio::test]
    async fn test_fallback_manager_with_custom_config() {
        let config = FallbackConfig {
            enable_fallback: false,
            log_fallback_warnings: false,
            allow_insecure_fallback: true,
        };
        let manager = FallbackManager::with_config(config);
        assert!(!manager.config.enable_fallback);
        assert!(!manager.config.log_fallback_warnings);
        assert!(manager.config.allow_insecure_fallback);
    }
    
    #[tokio::test]
    async fn test_fallback_history() {
        let manager = FallbackManager::new();
        
        manager.record_fallback_event(
            "test-sandbox",
            "AppContainer",
            "Sandboxie-Plus",
            "Test reason",
        ).await;
        
        let history = manager.get_fallback_history().await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].sandbox_id, "test-sandbox");
        assert_eq!(history[0].primary_impl, "AppContainer");
        assert_eq!(history[0].fallback_impl, "Sandboxie-Plus");
    }
    
    #[tokio::test]
    async fn test_clear_fallback_history() {
        let manager = FallbackManager::new();
        
        manager.record_fallback_event(
            "test-sandbox",
            "gVisor",
            "Docker",
            "Test reason",
        ).await;
        
        assert_eq!(manager.get_fallback_history().await.len(), 1);
        
        manager.clear_fallback_history().await;
        assert_eq!(manager.get_fallback_history().await.len(), 0);
    }
    
    #[tokio::test]
    async fn test_fallback_history_limit() {
        let manager = FallbackManager::new();
        
        // Add 150 events
        for i in 0..150 {
            manager.record_fallback_event(
                &format!("sandbox-{}", i),
                "Primary",
                "Fallback",
                "Test",
            ).await;
        }
        
        // Should keep only last 100
        let history = manager.get_fallback_history().await;
        assert_eq!(history.len(), 100);
        assert_eq!(history[0].sandbox_id, "sandbox-50");
        assert_eq!(history[99].sandbox_id, "sandbox-149");
    }
    
    #[test]
    fn test_implementation_availability() {
        let manager = FallbackManager::new();
        
        #[cfg(target_os = "windows")]
        {
            assert!(manager.is_implementation_available("appcontainer"));
            assert!(!manager.is_implementation_available("nono"));
        }
        
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            assert!(!manager.is_implementation_available("appcontainer"));
            assert!(manager.is_implementation_available("nono"));
        }
        
        // gVisor should be available on all platforms (via Docker)
        assert!(manager.is_implementation_available("gvisor"));
        
        // Unknown implementation
        assert!(!manager.is_implementation_available("unknown"));
    }
}
