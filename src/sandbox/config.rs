// Configuration management for the sandbox security solution

use super::error::{ConfigError, ConfigResult};
use super::types::SandboxConfig;
use std::fs;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Configuration change listener trait
pub trait ConfigChangeListener: Send + Sync {
    fn on_config_changed(&self, old_config: &SandboxConfig, new_config: &SandboxConfig);
}

/// Configuration manager
pub struct ConfigurationManager {
    config_path: String,
    config: Option<SandboxConfig>,
    listeners: Arc<RwLock<Vec<Arc<dyn ConfigChangeListener>>>>,
}

impl ConfigurationManager {
    /// Create a new configuration manager
    pub fn new(config_path: String) -> Self {
        Self {
            config_path,
            config: None,
            listeners: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Register a configuration change listener
    pub async fn add_listener(&self, listener: Arc<dyn ConfigChangeListener>) {
        let mut listeners = self.listeners.write().await;
        listeners.push(listener);
    }
    
    /// Remove all listeners
    pub async fn clear_listeners(&self) {
        let mut listeners = self.listeners.write().await;
        listeners.clear();
    }
    
    /// Load configuration from file
    pub fn load(&mut self) -> ConfigResult<SandboxConfig> {
        let content = fs::read_to_string(&self.config_path)?;
        let raw_config: serde_json::Value = serde_json::from_str(&content)?;
        
        // Validate configuration
        self.validate(&raw_config)?;
        
        // Parse configuration
        let config: SandboxConfig = serde_json::from_value(raw_config)?;
        self.config = Some(config.clone());
        
        Ok(config)
    }
    
    /// Validate configuration
    fn validate(&self, config: &serde_json::Value) -> ConfigResult<()> {
        // Validate resource limits if present
        if let Some(resources) = config.get("resources") {
            self.validate_resources(resources)?;
        }
        
        Ok(())
    }
    
    /// Validate resource limits
    fn validate_resources(&self, resources: &serde_json::Value) -> ConfigResult<()> {
        if let Some(max_memory) = resources.get("max_memory") {
            if let Some(memory_str) = max_memory.as_str() {
                let memory = Self::parse_size(memory_str)?;
                if memory < 128 * 1024 * 1024 {  // minimum 128MB
                    return Err(ConfigError::InvalidValue(
                        "max_memory must be at least 128M".to_string()
                    ));
                }
            }
        }
        
        if let Some(max_cpu) = resources.get("max_cpu") {
            if let Some(cpu) = max_cpu.as_f64() {
                let num_cpus = num_cpus::get() as f64;
                if cpu <= 0.0 || cpu > num_cpus {
                    return Err(ConfigError::InvalidValue(
                        format!("max_cpu must be between 0 and {}", num_cpus)
                    ));
                }
            }
        }
        
        Ok(())
    }
    
    /// Parse size string (e.g., "2G", "512M")
    pub fn parse_size(size_str: &str) -> ConfigResult<u64> {
        let size_str = size_str.trim();
        let (num_str, unit) = if size_str.ends_with('G') || size_str.ends_with('g') {
            (&size_str[..size_str.len()-1], 1024 * 1024 * 1024)
        } else if size_str.ends_with('M') || size_str.ends_with('m') {
            (&size_str[..size_str.len()-1], 1024 * 1024)
        } else if size_str.ends_with('K') || size_str.ends_with('k') {
            (&size_str[..size_str.len()-1], 1024)
        } else {
            (size_str, 1)
        };
        
        let num: u64 = num_str.parse()
            .map_err(|_| ConfigError::InvalidValue(format!("Invalid size: {}", size_str)))?;
        
        Ok(num * unit)
    }
    
    /// Reload configuration (hot reload)
    pub async fn reload(&mut self) -> ConfigResult<SandboxConfig> {
        let old_config = self.config.clone();
        match self.load() {
            Ok(new_config) => {
                if let Some(old) = old_config {
                    self.notify_config_change(&old, &new_config).await;
                }
                Ok(new_config)
            }
            Err(e) => {
                // Keep old configuration on failure
                Err(ConfigError::ReloadFailed(format!("Failed to reload config: {}", e)))
            }
        }
    }
    
    /// Notify configuration change to all listeners
    async fn notify_config_change(&self, old_config: &SandboxConfig, new_config: &SandboxConfig) {
        let listeners = self.listeners.read().await;
        for listener in listeners.iter() {
            listener.on_config_changed(old_config, new_config);
        }
    }
    
    /// Get current configuration
    pub fn get_config(&self) -> Option<&SandboxConfig> {
        self.config.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_size_gigabytes() {
        assert_eq!(ConfigurationManager::parse_size("2G").unwrap(), 2 * 1024 * 1024 * 1024);
        assert_eq!(ConfigurationManager::parse_size("1g").unwrap(), 1024 * 1024 * 1024);
    }
    
    #[test]
    fn test_parse_size_megabytes() {
        assert_eq!(ConfigurationManager::parse_size("512M").unwrap(), 512 * 1024 * 1024);
        assert_eq!(ConfigurationManager::parse_size("256m").unwrap(), 256 * 1024 * 1024);
    }
    
    #[test]
    fn test_parse_size_kilobytes() {
        assert_eq!(ConfigurationManager::parse_size("1024K").unwrap(), 1024 * 1024);
        assert_eq!(ConfigurationManager::parse_size("512k").unwrap(), 512 * 1024);
    }
    
    #[test]
    fn test_parse_size_bytes() {
        assert_eq!(ConfigurationManager::parse_size("1024").unwrap(), 1024);
    }
    
    #[test]
    fn test_parse_size_invalid() {
        assert!(ConfigurationManager::parse_size("invalid").is_err());
        assert!(ConfigurationManager::parse_size("").is_err());
    }
}

    #[tokio::test]
    async fn test_config_hot_reload() {
        use std::io::Write;
        use tempfile::NamedTempFile;
        
        // Create a temporary config file
        let mut temp_file = NamedTempFile::new().unwrap();
        let config_v1 = r#"{
            "sandbox_id": "test-sandbox",
            "platform": "linux",
            "filesystem": {
                "readonly_paths": ["/usr"],
                "writable_paths": ["/tmp"],
                "hidden_paths": []
            },
            "network": {
                "enabled": false,
                "allowed_hosts": [],
                "allowed_ports": []
            },
            "resources": {
                "max_memory": 1073741824,
                "max_cpu": 1.0,
                "max_disk": 5368709120
            },
            "process": {
                "allow_fork": false,
                "max_processes": 10
            },
            "monitoring": {
                "log_level": "info",
                "log_output": [],
                "audit": {
                    "file_access": true,
                    "network_access": true,
                    "process_creation": true,
                    "violations": true
                },
                "metrics": {
                    "enabled": false,
                    "interval": 60,
                    "endpoint": ""
                }
            }
        }"#;
        
        write!(temp_file, "{}", config_v1).unwrap();
        temp_file.flush().unwrap();
        
        let config_path = temp_file.path().to_str().unwrap().to_string();
        let mut manager = ConfigurationManager::new(config_path.clone());
        
        // Load initial config
        let config1 = manager.load().unwrap();
        assert_eq!(config1.resources.max_memory, 1024 * 1024 * 1024);
        
        // Update config file
        let config_v2 = r#"{
            "sandbox_id": "test-sandbox",
            "platform": "linux",
            "filesystem": {
                "readonly_paths": ["/usr"],
                "writable_paths": ["/tmp"],
                "hidden_paths": []
            },
            "network": {
                "enabled": false,
                "allowed_hosts": [],
                "allowed_ports": []
            },
            "resources": {
                "max_memory": 2147483648,
                "max_cpu": 2.0,
                "max_disk": 10737418240
            },
            "process": {
                "allow_fork": false,
                "max_processes": 10
            },
            "monitoring": {
                "log_level": "info",
                "log_output": [],
                "audit": {
                    "file_access": true,
                    "network_access": true,
                    "process_creation": true,
                    "violations": true
                },
                "metrics": {
                    "enabled": false,
                    "interval": 60,
                    "endpoint": ""
                }
            }
        }"#;
        
        std::fs::write(&config_path, config_v2).unwrap();
        
        // Reload config
        let config2 = manager.reload().await.unwrap();
        assert_eq!(config2.resources.max_memory, 2 * 1024 * 1024 * 1024);
        assert_eq!(config2.resources.max_cpu, 2.0);
    }
    
    #[tokio::test]
    async fn test_config_change_listener() {
        use std::io::Write;
        use std::sync::atomic::{AtomicBool, Ordering};
        use tempfile::NamedTempFile;
        
        struct TestListener {
            called: Arc<AtomicBool>,
        }
        
        impl ConfigChangeListener for TestListener {
            fn on_config_changed(&self, _old_config: &SandboxConfig, _new_config: &SandboxConfig) {
                self.called.store(true, Ordering::Relaxed);
            }
        }
        
        // Create a temporary config file
        let mut temp_file = NamedTempFile::new().unwrap();
        let config_content = r#"{
            "sandbox_id": "test-sandbox",
            "platform": "linux",
            "filesystem": {
                "readonly_paths": ["/usr"],
                "writable_paths": ["/tmp"],
                "hidden_paths": []
            },
            "network": {
                "enabled": false,
                "allowed_hosts": [],
                "allowed_ports": []
            },
            "resources": {
                "max_memory": 1073741824,
                "max_cpu": 1.0,
                "max_disk": 5368709120
            },
            "process": {
                "allow_fork": false,
                "max_processes": 10
            },
            "monitoring": {
                "log_level": "info",
                "log_output": [],
                "audit": {
                    "file_access": true,
                    "network_access": true,
                    "process_creation": true,
                    "violations": true
                },
                "metrics": {
                    "enabled": false,
                    "interval": 60,
                    "endpoint": ""
                }
            }
        }"#;
        
        write!(temp_file, "{}", config_content).unwrap();
        temp_file.flush().unwrap();
        
        let config_path = temp_file.path().to_str().unwrap().to_string();
        let mut manager = ConfigurationManager::new(config_path.clone());
        
        // Load initial config
        manager.load().unwrap();
        
        // Add listener
        let called = Arc::new(AtomicBool::new(false));
        let listener = Arc::new(TestListener {
            called: Arc::clone(&called),
        });
        manager.add_listener(listener).await;
        
        // Reload config (should trigger listener)
        manager.reload().await.unwrap();
        
        // Verify listener was called
        assert!(called.load(Ordering::Relaxed));
    }
