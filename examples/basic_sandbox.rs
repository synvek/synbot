//! Basic Sandbox Example
//!
//! This example demonstrates the basic usage of the Synbot sandbox security solution.
//! It shows how to:
//! - Create a sandbox manager
//! - Create an application sandbox
//! - Start and stop sandboxes
//! - List active sandboxes
//! - Clean up resources

use synbot::sandbox::*;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Basic Sandbox Example ===\n");

    // Step 1: Create configuration and monitoring
    println!("1. Initializing sandbox manager...");
    let config_manager = ConfigurationManager::new("config.json".to_string());
    let monitoring = MonitoringModule::new(Default::default());
    let manager = SandboxManager::new(config_manager, monitoring);
    println!("   ✓ Manager initialized\n");

    // Step 2: Create an application sandbox
    println!("2. Creating application sandbox...");
    let app_config = SandboxConfig {
        sandbox_id: "example-app-sandbox".to_string(),
        platform: "auto".to_string(),
        filesystem: FilesystemConfig {
            readonly_paths: vec!["/usr".to_string()],
            writable_paths: vec!["/tmp".to_string()],
            hidden_paths: vec![],
            ..Default::default()
        },
        network: NetworkConfig {
            enabled: true,
            allowed_hosts: vec!["api.example.com".to_string()],
            allowed_ports: vec![80, 443],
        },
        resources: ResourceConfig {
            max_memory: 2 * 1024 * 1024 * 1024, // 2GB
            max_cpu: 2.0,
            max_disk: 10 * 1024 * 1024 * 1024, // 10GB
        },
        process: ProcessConfig {
            allow_fork: false,
            max_processes: 10,
        },
        monitoring: MonitoringConfig::default(),
        delete_on_start: false,
        requested_tool_sandbox_type: None,
    };

    let sandbox_id = manager.create_app_sandbox(app_config).await?;
    println!("   ✓ Sandbox created: {}\n", sandbox_id);

    // Step 3: Start the sandbox
    println!("3. Starting sandbox...");
    manager.start_sandbox(&sandbox_id).await?;
    println!("   ✓ Sandbox started\n");

    // Step 4: Get sandbox information
    println!("4. Getting sandbox information...");
    if let Some(info) = manager.get_sandbox(&sandbox_id).await {
        println!("   Sandbox ID: {}", info.sandbox_id);
        println!("   Platform: {}", info.platform);
        println!("   Type: {}\n", info.sandbox_type);
    }

    // Step 5: List all sandboxes
    println!("5. Listing all sandboxes...");
    let sandboxes = manager.list_sandboxes().await;
    println!("   Active sandboxes: {}", sandboxes.len());
    for sandbox in &sandboxes {
        println!("   - {} ({})", sandbox.sandbox_id, sandbox.sandbox_type);
    }
    println!();

    // Step 6: Stop the sandbox
    println!("6. Stopping sandbox...");
    manager.stop_sandbox(&sandbox_id).await?;
    println!("   ✓ Sandbox stopped\n");

    // Step 7: Destroy the sandbox
    println!("7. Destroying sandbox...");
    manager.destroy_sandbox(&sandbox_id).await?;
    println!("   ✓ Sandbox destroyed\n");

    println!("=== Example completed successfully ===");
    Ok(())
}
