//! Dual-Layer Isolation Example
//!
//! This example demonstrates the dual-layer sandbox architecture:
//! - Application sandbox (first layer)
//! - Tool sandbox (second layer)
//! - Isolation verification between layers
//! - Safe communication between sandboxes

use synbot::sandbox::*;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Dual-Layer Isolation Example ===\n");

    // Initialize manager
    println!("1. Initializing sandbox manager...");
    let manager = SandboxManager::with_defaults();
    println!("   ✓ Manager initialized\n");

    // Create application sandbox (Layer 1)
    println!("2. Creating application sandbox (Layer 1)...");
    let app_config = SandboxConfig {
        sandbox_id: "app-layer".to_string(),
        platform: "auto".to_string(),
        filesystem: FilesystemConfig {
            readonly_paths: vec!["/usr".to_string(), "/lib".to_string()],
            writable_paths: vec!["/tmp".to_string(), "/app/data".to_string()],
            hidden_paths: vec!["/etc/shadow".to_string()],
        },
        network: NetworkConfig {
            enabled: true,
            allowed_hosts: vec!["api.example.com".to_string()],
            allowed_ports: vec![443],
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
    };

    let app_sandbox_id = manager.create_app_sandbox(app_config).await?;
    manager.start_sandbox(&app_sandbox_id).await?;
    println!("   ✓ Application sandbox created and started: {}\n", app_sandbox_id);

    // Create tool sandbox (Layer 2)
    println!("3. Creating tool sandbox (Layer 2)...");
    let tool_config = SandboxConfig {
        sandbox_id: "tool-layer".to_string(),
        platform: "auto".to_string(),
        filesystem: FilesystemConfig {
            readonly_paths: vec![],
            writable_paths: vec!["/workspace".to_string()],
            hidden_paths: vec![],
        },
        network: NetworkConfig {
            enabled: false, // More restrictive than app sandbox
            allowed_hosts: vec![],
            allowed_ports: vec![],
        },
        resources: ResourceConfig {
            max_memory: 1024 * 1024 * 1024, // 1GB - less than app
            max_cpu: 1.0, // Less CPU than app
            max_disk: 5 * 1024 * 1024 * 1024, // 5GB
        },
        process: ProcessConfig {
            allow_fork: false,
            max_processes: 5,
        },
        monitoring: MonitoringConfig::default(),
    };

    let tool_sandbox_id = manager.create_tool_sandbox(tool_config).await?;
    manager.start_sandbox(&tool_sandbox_id).await?;
    println!("   ✓ Tool sandbox created and started: {}\n", tool_sandbox_id);

    // Verify isolation between layers
    println!("4. Verifying isolation between layers...");
    let verification = manager.verify_isolation(&app_sandbox_id, &tool_sandbox_id).await?;
    
    println!("   Isolation Status: {}", if verification.isolated { "✓ ISOLATED" } else { "✗ NOT ISOLATED" });
    println!("   Checks performed: {}", verification.checks.len());
    println!("   Isolation score: {:.2}", verification.isolation_score);
    
    for check in &verification.checks {
        let status = if check.passed { "✓" } else { "✗" };
        println!("   {} {}: {}", status, check.name, check.description);
    }
    
    if !verification.violations.is_empty() {
        println!("\n   ⚠ Violations detected:");
        for violation in &verification.violations {
            println!("     - {}", violation);
        }
    }
    println!();

    // Demonstrate safe communication
    println!("5. Demonstrating safe communication...");
    
    // Simulate tool execution result
    let tool_result = ExecutionResult {
        exit_code: 0,
        stdout: b"Tool execution completed successfully".to_vec(),
        stderr: vec![],
        duration: std::time::Duration::from_secs(2),
        error: None,
    };
    
    println!("   Tool result (raw):");
    println!("     Exit code: {}", tool_result.exit_code);
    println!("     Output: {}", String::from_utf8_lossy(&tool_result.stdout));
    
    // Transfer through secure channel
    let safe_result = manager.transfer_result(tool_result)?;
    
    println!("\n   Tool result (after filtering):");
    println!("     Exit code: {}", safe_result.exit_code);
    println!("     Output: {}", String::from_utf8_lossy(&safe_result.stdout));
    println!("     ✓ Result safely transferred to app sandbox\n");

    // Test malicious payload filtering
    println!("6. Testing malicious payload filtering...");
    
    let malicious_payloads = vec![
        ("ELF executable", vec![0x7F, 0x45, 0x4C, 0x46, 0x01, 0x02]),
        ("PE executable", vec![0x4D, 0x5A, 0x90, 0x00]),
        ("Shell script", b"#!/bin/bash\nrm -rf /".to_vec()),
    ];
    
    for (name, payload) in malicious_payloads {
        let malicious_result = ExecutionResult {
            exit_code: 0,
            stdout: payload,
            stderr: vec![],
            duration: std::time::Duration::from_secs(1),
            error: None,
        };
        
        match manager.transfer_result(malicious_result) {
            Ok(_) => println!("   ✗ {} was NOT filtered!", name),
            Err(_) => println!("   ✓ {} was blocked", name),
        }
    }
    println!();

    // Get sandbox information
    println!("7. Sandbox information...");
    let sandboxes = manager.list_sandboxes().await;
    println!("   Total active sandboxes: {}", sandboxes.len());
    for sandbox in &sandboxes {
        println!("\n   Sandbox: {}", sandbox.sandbox_id);
        println!("     Platform: {}", sandbox.platform);
        println!("     Type: {}", sandbox.sandbox_type);
    }
    println!();

    // Security validation
    println!("8. Performing security validation...");
    
    // Verify isolation integrity
    match manager.verify_sandbox_isolation(&app_sandbox_id).await {
        Ok(_) => println!("   ✓ App sandbox isolation verified"),
        Err(e) => println!("   ✗ App sandbox isolation check failed: {:?}", e),
    }
    
    match manager.verify_sandbox_isolation(&tool_sandbox_id).await {
        Ok(_) => println!("   ✓ Tool sandbox isolation verified"),
        Err(e) => println!("   ✗ Tool sandbox isolation check failed: {:?}", e),
    }
    
    // Verify no-new-privileges
    match manager.verify_no_new_privileges() {
        Ok(_) => println!("   ✓ No-new-privileges flag verified"),
        Err(e) => println!("   ✗ No-new-privileges check failed: {:?}", e),
    }
    
    // Enforce resource limits
    match manager.enforce_resource_limits(&app_sandbox_id).await {
        Ok(_) => println!("   ✓ App sandbox resource limits enforced"),
        Err(e) => println!("   ✗ Resource limit enforcement failed: {:?}", e),
    }
    
    match manager.enforce_resource_limits(&tool_sandbox_id).await {
        Ok(_) => println!("   ✓ Tool sandbox resource limits enforced"),
        Err(e) => println!("   ✗ Resource limit enforcement failed: {:?}", e),
    }
    println!();

    // Cleanup
    println!("9. Cleaning up...");
    manager.stop_sandbox(&app_sandbox_id).await?;
    manager.stop_sandbox(&tool_sandbox_id).await?;
    manager.destroy_sandbox(&app_sandbox_id).await?;
    manager.destroy_sandbox(&tool_sandbox_id).await?;
    println!("   ✓ All sandboxes cleaned up\n");

    println!("=== Example completed successfully ===");
    println!("\nKey Takeaways:");
    println!("  1. Application sandbox provides first layer of defense");
    println!("  2. Tool sandbox provides second layer with stricter limits");
    println!("  3. Isolation verification ensures layers are properly separated");
    println!("  4. Safe communication channel filters malicious content");
    println!("  5. Security validation ensures ongoing protection");
    
    Ok(())
}
