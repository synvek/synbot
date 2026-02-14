//! Tool Execution Example
//!
//! This example demonstrates how to:
//! - Create a tool sandbox for executing commands
//! - Execute commands with timeout control
//! - Handle execution results
//! - Transfer results safely between sandboxes

use synbot::sandbox::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("=== Tool Execution Example ===\n");

    // Initialize manager
    println!("1. Initializing sandbox manager...");
    let manager = SandboxManager::with_defaults();
    println!("   ✓ Manager initialized\n");

    // Create tool sandbox
    println!("2. Creating tool sandbox...");
    let tool_config = SandboxConfig {
        sandbox_id: "example-tool-sandbox".to_string(),
        platform: "auto".to_string(),
        filesystem: FilesystemConfig {
            readonly_paths: vec![],
            writable_paths: vec!["/workspace".to_string()],
            hidden_paths: vec![],
        },
        network: NetworkConfig {
            enabled: false, // Tools typically don't need network
            allowed_hosts: vec![],
            allowed_ports: vec![],
        },
        resources: ResourceConfig {
            max_memory: 1024 * 1024 * 1024, // 1GB
            max_cpu: 1.0,
            max_disk: 5 * 1024 * 1024 * 1024, // 5GB
        },
        process: ProcessConfig {
            allow_fork: false,
            max_processes: 5,
        },
        monitoring: MonitoringConfig::default(),
    };

    let sandbox_id = manager.create_tool_sandbox(tool_config).await?;
    println!("   ✓ Tool sandbox created: {}\n", sandbox_id);

    // Start the sandbox
    println!("3. Starting tool sandbox...");
    manager.start_sandbox(&sandbox_id).await?;
    println!("   ✓ Tool sandbox started\n");

    // Note: In a real implementation, you would execute commands through the sandbox
    // This is a conceptual example showing the API usage
    println!("4. Executing commands (conceptual)...");
    
    // Example 1: Simple command
    println!("   Example 1: echo 'Hello from sandbox'");
    // let result = sandbox.execute("echo", &["Hello from sandbox".to_string()], Duration::from_secs(5))?;
    // println!("   Output: {}", String::from_utf8_lossy(&result.stdout));
    
    // Example 2: Command with timeout
    println!("   Example 2: ls -la /workspace");
    // let result = sandbox.execute("ls", &["-la".to_string(), "/workspace".to_string()], Duration::from_secs(10))?;
    
    // Example 3: Handling execution result
    println!("   Example 3: Creating a mock execution result");
    let mock_result = ExecutionResult {
        exit_code: 0,
        stdout: b"file1.txt\nfile2.txt\n".to_vec(),
        stderr: vec![],
        duration: Duration::from_secs(1),
        error: None,
    };
    
    println!("   Exit code: {}", mock_result.exit_code);
    println!("   Output: {}", String::from_utf8_lossy(&mock_result.stdout));
    println!("   Duration: {:?}\n", mock_result.duration);

    // Transfer result safely
    println!("5. Transferring result safely...");
    let safe_result = manager.transfer_result(mock_result)?;
    println!("   ✓ Result transferred and sanitized");
    println!("   Safe output: {}\n", String::from_utf8_lossy(&safe_result.stdout));

    // Example with malicious content
    println!("6. Testing security filtering...");
    let malicious_result = ExecutionResult {
        exit_code: 0,
        stdout: vec![0x7F, 0x45, 0x4C, 0x46], // ELF header
        stderr: vec![],
        duration: Duration::from_secs(1),
        error: None,
    };
    
    match manager.transfer_result(malicious_result) {
        Ok(_) => println!("   ⚠ Malicious content was not filtered!"),
        Err(e) => println!("   ✓ Malicious content blocked: {:?}\n", e),
    }

    // Cleanup
    println!("7. Cleaning up...");
    manager.stop_sandbox(&sandbox_id).await?;
    manager.destroy_sandbox(&sandbox_id).await?;
    println!("   ✓ Cleanup complete\n");

    println!("=== Example completed successfully ===");
    Ok(())
}
