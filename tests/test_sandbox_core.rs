// Integration tests for sandbox core data structures

use synbot::sandbox::{
    SandboxConfig, FilesystemConfig, NetworkConfig, ResourceConfig,
    ProcessConfig, MonitoringConfig, ConfigurationManager, SandboxError,
};

#[test]
fn test_sandbox_config_serialization() {
    let config = SandboxConfig {
        sandbox_id: "test-001".to_string(),
        platform: "linux".to_string(),
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
        monitoring: MonitoringConfig::default(),
        delete_on_start: false,
        requested_tool_sandbox_type: None,
    };
    
    // Serialize to JSON
    let json = serde_json::to_string(&config).unwrap();
    
    // Deserialize back
    let deserialized: SandboxConfig = serde_json::from_str(&json).unwrap();
    
    // Verify equality
    assert_eq!(config, deserialized);
}

#[test]
fn test_parse_size_various_units() {
    assert_eq!(
        ConfigurationManager::parse_size("2G").unwrap(),
        2 * 1024 * 1024 * 1024
    );
    assert_eq!(
        ConfigurationManager::parse_size("512M").unwrap(),
        512 * 1024 * 1024
    );
    assert_eq!(
        ConfigurationManager::parse_size("1024K").unwrap(),
        1024 * 1024
    );
    assert_eq!(
        ConfigurationManager::parse_size("2048").unwrap(),
        2048
    );
}

#[test]
fn test_sandbox_error_display() {
    let err = SandboxError::Timeout;
    assert_eq!(err.to_string(), "Timeout");
    
    let err = SandboxError::NotFound;
    assert_eq!(err.to_string(), "Sandbox not found");
    
    let err = SandboxError::Configuration("invalid field".to_string());
    assert_eq!(err.to_string(), "Configuration error: invalid field");
}
