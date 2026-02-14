# Synbot Sandbox Examples

This directory contains example programs demonstrating the Synbot sandbox security solution.

## Examples

### 1. Basic Sandbox (`basic_sandbox.rs`)

Demonstrates the fundamental operations:
- Creating a sandbox manager
- Creating and starting an application sandbox
- Listing active sandboxes
- Stopping and destroying sandboxes

**Run:**
```bash
cargo run --example basic_sandbox
```

**What you'll learn:**
- Basic sandbox lifecycle management
- How to initialize the sandbox manager
- How to create and configure sandboxes
- Proper cleanup procedures

### 2. Tool Execution (`tool_execution.rs`)

Shows how to execute tools in isolated sandboxes:
- Creating tool sandboxes
- Executing commands with timeout control
- Handling execution results
- Transferring results safely between sandboxes
- Security filtering of malicious content

**Run:**
```bash
cargo run --example tool_execution
```

**What you'll learn:**
- Tool sandbox configuration
- Command execution patterns
- Result handling and error management
- Security filtering mechanisms

### 3. Dual-Layer Isolation (`dual_layer_isolation.rs`)

Demonstrates the dual-layer sandbox architecture:
- Creating both application and tool sandboxes
- Verifying isolation between layers
- Safe communication between sandboxes
- Security validation

**Run:**
```bash
cargo run --example dual_layer_isolation
```

**What you'll learn:**
- Defense-in-depth architecture
- Isolation verification
- Cross-sandbox communication
- Security best practices

### 4. Advanced Configuration (`advanced_configuration.rs`)

Shows advanced configuration features:
- Fine-grained filesystem controls
- Network access whitelisting
- Resource limit enforcement
- Command validation
- Audit logging and metrics collection

**Run:**
```bash
cargo run --example advanced_configuration
```

**What you'll learn:**
- Advanced configuration options
- Security hardening techniques
- Monitoring and auditing
- Resource management

## Prerequisites

Before running the examples, ensure you have:

1. **Rust** (1.70 or higher)
   ```bash
   rustup update
   ```

2. **Docker** (for tool sandboxes)
   ```bash
   docker --version
   ```

3. **gVisor** (for enhanced security)
   ```bash
   # Linux
   sudo apt-get install runsc
   
   # macOS
   brew install gvisor
   ```

4. **Platform-specific requirements**:
   - **Windows**: WSL2 enabled
   - **Linux**: User namespaces enabled
   - **macOS**: Docker Desktop installed

## Configuration

Most examples use default configuration. For production use, create a `config.json` file:

```json
{
  "version": "1.0",
  "app_sandbox": {
    "platform": "auto",
    "filesystem": {
      "readonly_paths": ["/usr", "/lib"],
      "writable_paths": ["/tmp"],
      "hidden_paths": ["/etc/shadow"]
    },
    "network": {
      "enabled": true,
      "allowed_hosts": ["api.example.com"],
      "allowed_ports": [80, 443]
    },
    "resources": {
      "max_memory": "2G",
      "max_cpu": 2.0,
      "max_disk": "10G"
    }
  },
  "tool_sandbox": {
    "image": "ubuntu:22.04",
    "network": {
      "enabled": false
    },
    "resources": {
      "max_memory": "1G",
      "max_cpu": 1.0,
      "max_disk": "5G"
    }
  }
}
```

## Running Examples

### Run a specific example:
```bash
cargo run --example basic_sandbox
cargo run --example tool_execution
cargo run --example dual_layer_isolation
cargo run --example advanced_configuration
```

### Run with custom configuration:
```bash
SYNBOT_CONFIG=my-config.json cargo run --example basic_sandbox
```

### Run with debug logging:
```bash
RUST_LOG=debug cargo run --example basic_sandbox
```

### Build all examples:
```bash
cargo build --examples
```

## Troubleshooting

### Docker not running
```bash
# Linux
sudo systemctl start docker

# macOS
open -a Docker

# Windows
Start-Service docker
```

### Permission denied
```bash
# Add user to docker group (Linux)
sudo usermod -aG docker $USER
newgrp docker
```

### gVisor not found
```bash
# Verify installation
which runsc
docker run --runtime=runsc hello-world
```

### WSL2 not available (Windows)
```powershell
# Enable WSL2
wsl --install
wsl --set-default-version 2
```

## Example Output

### Basic Sandbox Example
```
=== Basic Sandbox Example ===

1. Initializing sandbox manager...
   ✓ Manager initialized

2. Creating application sandbox...
   ✓ Sandbox created: example-app-sandbox

3. Starting sandbox...
   ✓ Sandbox started

4. Getting sandbox information...
   Sandbox ID: example-app-sandbox
   Platform: linux
   Type: nono

5. Listing all sandboxes...
   Active sandboxes: 1
   - example-app-sandbox (nono)

6. Stopping sandbox...
   ✓ Sandbox stopped

7. Destroying sandbox...
   ✓ Sandbox destroyed

=== Example completed successfully ===
```

## Next Steps

After running the examples:

1. **Read the documentation**:
   - [API Reference](../docs/api-reference/sandbox-api.md)
   - [Deployment Guide](../docs/getting-started/sandbox-deployment.md)
   - [Configuration Guide](../docs/getting-started/configuration.md)

2. **Explore the source code**:
   - `src/sandbox/manager.rs` - Sandbox manager implementation
   - `src/sandbox/sandbox_trait.rs` - Sandbox interface
   - `src/sandbox/monitoring.rs` - Monitoring and audit logging

3. **Try modifying the examples**:
   - Change resource limits
   - Add custom filesystem paths
   - Implement custom security validators
   - Add your own monitoring logic

4. **Build your own application**:
   - Use the examples as templates
   - Integrate with your existing code
   - Customize for your use case

## Support

- **Documentation**: https://docs.synbot.dev
- **GitHub Issues**: https://github.com/your-org/synbot/issues
- **Community**: https://discord.gg/synbot

## License

These examples are part of the Synbot project and are licensed under the same terms.
See [LICENSE](../LICENSE) for details.
