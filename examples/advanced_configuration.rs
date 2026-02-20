//! Advanced Configuration Example
//!
//! This example demonstrates advanced configuration features:
//! - Custom security validators
//! - Fine-grained resource limits
//! - Network access control
//! - Filesystem isolation
//! - Monitoring and audit logging
//! - Configuration hot reload

use synbot::sandbox::*;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Advanced Configuration Example ===\n");

    // Step 1: Create custom monitoring configuration
    println!("1. Setting up advanced monitoring...");
    let monitoring_config = MonitoringConfig {
        log_level: "debug".to_string(),
        log_output: vec![
            LogOutput {
                output_type: "file".to_string(),
                path: "/var/log/synbot/sandbox.log".to_string(),
                facility: "".to_string(),
            },
            LogOutput {
                output_type: "syslog".to_string(),
                path: "".to_string(),
                facility: "local0".to_string(),
            },
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
    
    let monitoring = MonitoringModule::new(monitoring_config);
    println!("   ✓ Monitoring configured with file and syslog output\n");

    // Step 2: Create configuration manager
    println!("2. Setting up configuration manager...");
    let config_manager = ConfigurationManager::new("config.json".to_string());
    println!("   ✓ Configuration manager initialized\n");

    // Step 3: Create sandbox manager with custom security validator
    println!("3. Creating sandbox manager with security validator...");
    let security_validator = SecurityValidator::new();
    let manager = SandboxManager::with_security_validator(
        config_manager,
        monitoring,
        security_validator,
    );
    println!("   ✓ Manager created with enhanced security\n");

    // Step 4: Create sandbox with fine-grained filesystem controls
    println!("4. Creating sandbox with fine-grained filesystem controls...");
    let fs_config = FilesystemConfig {
        readonly_paths: vec![
            "/usr".to_string(),
            "/lib".to_string(),
            "/lib64".to_string(),
            "/bin".to_string(),
            "/sbin".to_string(),
        ],
        writable_paths: vec![
            "/tmp".to_string(),
            "/var/tmp".to_string(),
            "/app/data".to_string(),
            "/app/cache".to_string(),
        ],
        hidden_paths: vec![
            "/etc/shadow".to_string(),
            "/etc/gshadow".to_string(),
            "/root".to_string(),
            "/home".to_string(),
            "/proc/kcore".to_string(),
        ],
        ..Default::default()
    };
    
    println!("   Readonly paths: {}", fs_config.readonly_paths.len());
    println!("   Writable paths: {}", fs_config.writable_paths.len());
    println!("   Hidden paths: {}\n", fs_config.hidden_paths.len());

    // Step 5: Create sandbox with network access control
    println!("5. Configuring network access control...");
    let network_config = NetworkConfig {
        enabled: true,
        allowed_hosts: vec![
            "api.example.com".to_string(),
            "*.trusted-domain.com".to_string(),
            "cdn.example.net".to_string(),
        ],
        allowed_ports: vec![80, 443, 8080],
    };
    
    println!("   Network enabled: {}", network_config.enabled);
    println!("   Allowed hosts: {:?}", network_config.allowed_hosts);
    println!("   Allowed ports: {:?}\n", network_config.allowed_ports);

    // Step 6: Configure resource limits
    println!("6. Setting resource limits...");
    let resource_config = ResourceConfig {
        max_memory: 4 * 1024 * 1024 * 1024, // 4GB
        max_cpu: 4.0, // 4 cores
        max_disk: 20 * 1024 * 1024 * 1024, // 20GB
    };
    
    println!("   Max memory: {} GB", resource_config.max_memory / (1024 * 1024 * 1024));
    println!("   Max CPU: {} cores", resource_config.max_cpu);
    println!("   Max disk: {} GB\n", resource_config.max_disk / (1024 * 1024 * 1024));

    // Step 7: Configure process controls
    println!("7. Configuring process controls...");
    let process_config = ProcessConfig {
        allow_fork: false,
        max_processes: 20,
    };
    
    println!("   Allow fork: {}", process_config.allow_fork);
    println!("   Max processes: {}\n", process_config.max_processes);

    // Step 8: Create the sandbox with all configurations
    println!("8. Creating sandbox with advanced configuration...");
    let sandbox_config = SandboxConfig {
        sandbox_id: "advanced-sandbox".to_string(),
        platform: "auto".to_string(),
        filesystem: fs_config,
        network: network_config,
        resources: resource_config,
        process: process_config,
        monitoring: MonitoringConfig::default(),
        delete_on_start: false,
    };

    let sandbox_id = manager.create_app_sandbox(sandbox_config).await?;
    manager.start_sandbox(&sandbox_id).await?;
    println!("   ✓ Advanced sandbox created: {}\n", sandbox_id);

    // Step 9: Demonstrate command validation
    println!("9. Testing command validation...");
    
    let safe_commands = vec![
        ("ls", vec!["-la".to_string()]),
        ("cat", vec!["file.txt".to_string()]),
        ("echo", vec!["Hello".to_string()]),
    ];
    
    let dangerous_commands = vec![
        ("rm", vec!["-rf".to_string(), "/".to_string()]),
        ("dd", vec!["if=/dev/zero".to_string(), "of=/dev/sda".to_string()]),
        ("chmod", vec!["777".to_string(), "/etc/shadow".to_string()]),
    ];
    
    println!("   Safe commands:");
    for (cmd, args) in safe_commands {
        match manager.validate_command(cmd, &args) {
            Ok(_) => println!("     ✓ {} {:?} - allowed", cmd, args),
            Err(e) => println!("     ✗ {} {:?} - blocked: {:?}", cmd, args, e),
        }
    }
    
    println!("\n   Dangerous commands:");
    for (cmd, args) in dangerous_commands {
        match manager.validate_command(cmd, &args) {
            Ok(_) => println!("     ⚠ {} {:?} - allowed (should be blocked!)", cmd, args),
            Err(_) => println!("     ✓ {} {:?} - blocked", cmd, args),
        }
    }
    println!();

    // Step 10: Demonstrate monitoring
    println!("10. Demonstrating audit logging...");
    let monitoring_ref = manager.monitoring();
    
    // Log various events
    monitoring_ref.log_file_access(&sandbox_id, "/etc/passwd", "read", true);
    monitoring_ref.log_file_access(&sandbox_id, "/etc/shadow", "read", false);
    monitoring_ref.log_network_access(&sandbox_id, "api.example.com", 443, true);
    monitoring_ref.log_network_access(&sandbox_id, "malicious.com", 80, false);
    monitoring_ref.log_process_creation(&sandbox_id, "bash", &["-c".to_string(), "ls".to_string()]);
    
    println!("   ✓ Logged 5 audit events");
    println!("     - 2 file access events");
    println!("     - 2 network access events");
    println!("     - 1 process creation event\n");

    // Step 11: Collect metrics
    println!("11. Collecting sandbox metrics...");
    let metrics = monitoring_ref.collect_metrics(&sandbox_id);
    println!("   CPU usage: {:.2}%", metrics.cpu_usage);
    println!("   Memory usage: {} MB", metrics.memory_usage / (1024 * 1024));
    println!("   Disk usage: {} MB", metrics.disk_usage / (1024 * 1024));
    println!("   Network I/O:");
    for (key, value) in &metrics.network_io {
        println!("     {}: {} bytes", key, value);
    }
    println!();

    // Step 12: Security validation
    println!("12. Running security validation...");
    
    // Verify isolation
    match manager.verify_sandbox_isolation(&sandbox_id).await {
        Ok(_) => println!("   ✓ Isolation integrity verified"),
        Err(e) => println!("   ✗ Isolation check failed: {:?}", e),
    }
    
    // Enforce resource limits
    match manager.enforce_resource_limits(&sandbox_id).await {
        Ok(_) => println!("   ✓ Resource limits enforced"),
        Err(e) => println!("   ✗ Resource enforcement failed: {:?}", e),
    }
    
    // Verify no-new-privileges
    match manager.verify_no_new_privileges() {
        Ok(_) => println!("   ✓ No-new-privileges verified"),
        Err(e) => println!("   ✗ Privilege check failed: {:?}", e),
    }
    println!();

    // Step 13: Configuration hot reload (conceptual)
    println!("13. Configuration hot reload (conceptual)...");
    println!("   In production, you would:");
    println!("   1. Modify config.json");
    println!("   2. Call config_manager.reload()");
    println!("   3. Apply new settings to existing sandboxes");
    println!("   4. Or create new sandboxes with updated config\n");

    // Step 14: Cleanup
    println!("14. Cleaning up...");
    manager.stop_sandbox(&sandbox_id).await?;
    manager.destroy_sandbox(&sandbox_id).await?;
    println!("   ✓ Sandbox destroyed\n");

    println!("=== Example completed successfully ===");
    println!("\nAdvanced Features Demonstrated:");
    println!("  ✓ Fine-grained filesystem controls");
    println!("  ✓ Network access whitelisting");
    println!("  ✓ Resource limit enforcement");
    println!("  ✓ Process controls");
    println!("  ✓ Command validation");
    println!("  ✓ Comprehensive audit logging");
    println!("  ✓ Metrics collection");
    println!("  ✓ Security validation");
    
    Ok(())
}
