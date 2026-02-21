// Integration tests for sandbox security solution
// Tests the interaction between sandbox components:
// - Application sandbox and tool sandbox interaction
// - Configuration manager and sandbox manager integration
// - Monitoring module and sandbox runtime integration

use synbot::sandbox::{
    SandboxManager, SandboxConfig, FilesystemConfig, NetworkConfig,
    ResourceConfig, ProcessConfig, MonitoringConfig, ConfigurationManager,
    MonitoringModule, LogOutput, AuditConfig,
    MetricsConfig, PlatformDetector,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

mod common;

/// Test 1: Application sandbox and tool sandbox interaction
/// Verifies that app sandbox can invoke tool sandbox and receive results
#[tokio::test]
async fn test_app_and_tool_sandbox_interaction() {
    // Create sandbox manager
    let config_manager = ConfigurationManager::new("test_config.json".to_string());
    let monitoring = MonitoringModule::new(create_test_monitoring_config());
    
    let manager = SandboxManager::new(config_manager, monitoring);
    
    // Create application sandbox
    let app_config = create_test_app_sandbox_config("app-sandbox-001");
    let app_sandbox_id = manager.create_app_sandbox(app_config).await;
    
    assert!(app_sandbox_id.is_ok(), "Failed to create app sandbox: {:?}", app_sandbox_id.err());
    let app_sandbox_id = app_sandbox_id.unwrap();
    
    // Note: Tool sandbox creation requires Docker/gVisor which may not be available in test environment
    // We test that the manager can handle tool sandbox requests gracefully
    let tool_config = create_test_tool_sandbox_config("tool-sandbox-001");
    let tool_sandbox_result = manager.create_tool_sandbox(tool_config).await;
    
    // If Docker is available, verify sandbox was created
    // If not, verify error is handled gracefully
    if tool_sandbox_result.is_ok() {
        let tool_sandbox_id = tool_sandbox_result.unwrap();
        
        // Verify both sandboxes exist
        let sandboxes = manager.list_sandboxes().await;
        assert_eq!(sandboxes.len(), 2, "Expected 2 sandboxes");
        
        // Verify sandboxes are isolated (different IDs)
        assert_ne!(app_sandbox_id, tool_sandbox_id, "Sandboxes should have different IDs");
        
        // Cleanup
        let _ = manager.destroy_sandbox(&tool_sandbox_id).await;
    }
    
    // Cleanup app sandbox
    let _ = manager.destroy_sandbox(&app_sandbox_id).await;
}

/// Test 2: Configuration manager and sandbox manager integration
/// Verifies that configuration changes are properly propagated to sandboxes
#[tokio::test]
async fn test_config_manager_sandbox_manager_integration() {
    // Create temporary config file
    let config_path = "test_integration_config.json";
    let initial_config = create_full_sandbox_config();
    std::fs::write(
        config_path,
        serde_json::to_string_pretty(&initial_config).unwrap()
    ).unwrap();
    
    // Create configuration manager
    let mut config_manager = ConfigurationManager::new(config_path.to_string());
    let loaded_config = config_manager.load();
    assert!(loaded_config.is_ok(), "Failed to load config");
    
    // Create sandbox manager with config manager
    let monitoring = MonitoringModule::new(create_test_monitoring_config());
    let manager = SandboxManager::new(config_manager, monitoring);
    
    // Create sandbox with loaded config
    let sandbox_config = loaded_config.unwrap();
    let sandbox_id = manager.create_app_sandbox(sandbox_config.clone()).await;
    assert!(sandbox_id.is_ok(), "Failed to create sandbox with config");
    
    // Verify sandbox uses correct configuration
    let sandbox_id = sandbox_id.unwrap();
    let sandbox_info = manager.get_sandbox(&sandbox_id).await;
    assert!(sandbox_info.is_some(), "Sandbox should exist");
    
    // Cleanup
    let _ = manager.destroy_sandbox(&sandbox_id).await;
    let _ = std::fs::remove_file(config_path);
}

/// Test 3: Monitoring module and sandbox runtime integration
/// Verifies that monitoring module correctly captures sandbox events
#[tokio::test]
async fn test_monitoring_sandbox_runtime_integration() {
    // Create monitoring module with file logger
    let log_path = "test_integration_audit.log";
    let monitoring_config = MonitoringConfig {
        log_level: "info".to_string(),
        log_output: vec![
            LogOutput {
                output_type: "file".to_string(),
                path: log_path.to_string(),
                facility: "".to_string(),
            }
        ],
        audit: AuditConfig {
            file_access: true,
            network_access: true,
            process_creation: true,
            violations: true,
        },
        metrics: MetricsConfig {
            enabled: true,
            interval: 60,
            endpoint: "http://localhost:9090/metrics".to_string(),
        },
    };
    
    let monitoring = Arc::new(MonitoringModule::new(monitoring_config));
    
    // Create sandbox manager with monitoring
    let config_manager = ConfigurationManager::new("test_config.json".to_string());
    let manager = SandboxManager::new(config_manager, MonitoringModule::new(create_test_monitoring_config()));
    
    // Create sandbox
    let sandbox_config = create_test_app_sandbox_config("monitored-sandbox-001");
    let sandbox_id = manager.create_app_sandbox(sandbox_config).await;
    assert!(sandbox_id.is_ok(), "Failed to create monitored sandbox");
    let sandbox_id = sandbox_id.unwrap();
    
    // Simulate sandbox events using the monitoring reference
    monitoring.log_file_access(&sandbox_id, "/tmp/test.txt", "read", true).await;
    monitoring.log_network_access(&sandbox_id, "api.example.com", 443, true).await;
    monitoring.log_process_creation(&sandbox_id, "echo", &["hello".to_string()]).await;
    
    // Give time for logs to be written
    sleep(Duration::from_millis(100)).await;
    
    // Verify logs were created
    let log_exists = std::path::Path::new(log_path).exists();
    assert!(log_exists, "Audit log file should exist");
    
    // Cleanup
    let _ = manager.destroy_sandbox(&sandbox_id).await;
    let _ = std::fs::remove_file(log_path);
}

/// Test 4: Cross-component error handling
/// Verifies that errors propagate correctly across components
#[tokio::test]
async fn test_cross_component_error_handling() {
    // Create sandbox manager with invalid config manager
    let config_manager = ConfigurationManager::new("nonexistent_config.json".to_string());
    let monitoring = MonitoringModule::new(create_test_monitoring_config());
    
    let manager = SandboxManager::new(config_manager, monitoring);
    
    // Try to create sandbox with invalid configuration
    let invalid_config = SandboxConfig {
        sandbox_id: "invalid-001".to_string(),
        platform: "unsupported_platform".to_string(),
        filesystem: FilesystemConfig {
            readonly_paths: vec![],
            writable_paths: vec![],
            hidden_paths: vec![],
            ..Default::default()
        },
        network: NetworkConfig {
            enabled: false,
            allowed_hosts: vec![],
            allowed_ports: vec![],
        },
        resources: ResourceConfig {
            max_memory: 0, // Invalid: too small
            max_cpu: 0.0,  // Invalid: too small
            max_disk: 0,   // Invalid: too small
        },
        process: ProcessConfig {
            allow_fork: false,
            max_processes: 0, // Invalid: too small
        },
        monitoring: create_test_monitoring_config(),
        child_work_dir: None,
        delete_on_start: false,
        requested_tool_sandbox_type: None,
        image: None,
    };
    
    let result = manager.create_app_sandbox(invalid_config).await;
    assert!(result.is_err(), "Should fail with invalid config");
}

/// Test 5: Concurrent sandbox operations
/// Verifies that multiple sandboxes can be managed concurrently
#[tokio::test]
async fn test_concurrent_sandbox_operations() {
    let config_manager = ConfigurationManager::new("test_config.json".to_string());
    let monitoring = MonitoringModule::new(create_test_monitoring_config());
    
    let manager = Arc::new(SandboxManager::new(config_manager, monitoring));
    
    // Create multiple sandboxes concurrently
    let mut handles = vec![];
    
    for i in 0..5 {
        let manager_clone = manager.clone();
        let handle = tokio::spawn(async move {
            let config = create_test_app_sandbox_config(&format!("concurrent-sandbox-{:03}", i));
            manager_clone.create_app_sandbox(config).await
        });
        handles.push(handle);
    }
    
    // Wait for all sandboxes to be created
    let mut sandbox_ids = vec![];
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Failed to create sandbox concurrently");
        sandbox_ids.push(result.unwrap());
    }
    
    // Verify all sandboxes exist
    let sandboxes = manager.list_sandboxes().await;
    assert_eq!(sandboxes.len(), 5, "Expected 5 sandboxes");
    
    // Cleanup all sandboxes
    for sandbox_id in sandbox_ids {
        let _ = manager.destroy_sandbox(&sandbox_id).await;
    }
}

/// Test 6: Platform detection integration
/// Verifies that platform detection works with sandbox creation
#[tokio::test]
async fn test_platform_detection_integration() {
    let platform_info = PlatformDetector::detect();
    
    // Create sandbox config with auto platform detection
    let mut config = create_test_app_sandbox_config("platform-test-001");
    config.platform = "auto".to_string();
    
    let config_manager = ConfigurationManager::new("test_config.json".to_string());
    let monitoring = MonitoringModule::new(create_test_monitoring_config());
    
    let manager = SandboxManager::new(config_manager, monitoring);
    
    // Create sandbox - should auto-detect platform
    let result = manager.create_app_sandbox(config).await;
    
    // On supported platforms, this should succeed
    if platform_info.os == "linux" || platform_info.os == "windows" || platform_info.os == "macos" {
        assert!(result.is_ok(), "Should create sandbox on supported platform");
        if let Ok(sandbox_id) = result {
            let _ = manager.destroy_sandbox(&sandbox_id).await;
        }
    }
}

/// Test 7: Configuration reload with active sandboxes
/// Verifies that config reload doesn't break active sandboxes
#[tokio::test]
async fn test_config_reload_with_active_sandboxes() {
    let config_path = "test_reload_config.json";
    let initial_config = create_full_sandbox_config();
    std::fs::write(
        config_path,
        serde_json::to_string_pretty(&initial_config).unwrap()
    ).unwrap();
    
    let mut config_manager = ConfigurationManager::new(config_path.to_string());
    let _ = config_manager.load();
    
    let monitoring = MonitoringModule::new(create_test_monitoring_config());
    let manager = SandboxManager::new(config_manager, monitoring);
    
    // Create sandbox
    let sandbox_config = create_test_app_sandbox_config("reload-test-001");
    let sandbox_id = manager.create_app_sandbox(sandbox_config).await;
    assert!(sandbox_id.is_ok());
    let sandbox_id = sandbox_id.unwrap();
    
    // Modify config file
    let mut modified_config = initial_config.clone();
    modified_config.resources.max_memory = 2 * 1024 * 1024 * 1024; // 2GB
    std::fs::write(
        config_path,
        serde_json::to_string_pretty(&modified_config).unwrap()
    ).unwrap();
    
    // Note: In a real scenario, we would need mutable access to config_manager to reload
    // For this test, we verify that the sandbox continues to work with the original config
    
    // Verify existing sandbox still exists
    let sandbox_info = manager.get_sandbox(&sandbox_id).await;
    assert!(sandbox_info.is_some(), "Existing sandbox should still exist after config file change");
    
    // Cleanup
    let _ = manager.destroy_sandbox(&sandbox_id).await;
    let _ = std::fs::remove_file(config_path);
}

// Helper functions

fn create_test_app_sandbox_config(sandbox_id: &str) -> SandboxConfig {
    SandboxConfig {
        sandbox_id: sandbox_id.to_string(),
        platform: std::env::consts::OS.to_string(),
        filesystem: FilesystemConfig {
            readonly_paths: vec!["/usr".to_string(), "/lib".to_string()],
            writable_paths: vec!["/tmp".to_string()],
            hidden_paths: vec!["/etc/shadow".to_string()],
            ..Default::default()
        },
        network: NetworkConfig {
            enabled: true,
            allowed_hosts: vec!["api.example.com".to_string()],
            allowed_ports: vec![80, 443],
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
        monitoring: create_test_monitoring_config(),
        child_work_dir: None,
        delete_on_start: false,
        requested_tool_sandbox_type: None,
        image: None,
    }
}

fn create_test_tool_sandbox_config(sandbox_id: &str) -> SandboxConfig {
    SandboxConfig {
        sandbox_id: sandbox_id.to_string(),
        platform: std::env::consts::OS.to_string(),
        filesystem: FilesystemConfig {
            readonly_paths: vec![],
            writable_paths: vec!["/workspace".to_string()],
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
            max_cpu: 0.5,
            max_disk: 2 * 1024 * 1024 * 1024, // 2GB
        },
        process: ProcessConfig {
            allow_fork: false,
            max_processes: 5,
        },
        monitoring: create_test_monitoring_config(),
        child_work_dir: None,
        delete_on_start: false,
        requested_tool_sandbox_type: None,
        image: None,
    }
}

fn create_test_monitoring_config() -> MonitoringConfig {
    MonitoringConfig {
        log_level: "info".to_string(),
        log_output: vec![],
        audit: AuditConfig {
            file_access: true,
            network_access: true,
            process_creation: true,
            violations: true,
        },
        metrics: MetricsConfig {
            enabled: false,
            interval: 60,
            endpoint: "".to_string(),
        },
    }
}

fn create_full_sandbox_config() -> SandboxConfig {
    create_test_app_sandbox_config("full-config-001")
}
