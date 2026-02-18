//! # Synbot - Personal AI Assistant
//!
//! Synbot is a personal AI assistant written in Rust, providing secure execution
//! environments through multi-layer sandbox isolation.
//!
//! ## Features
//!
//! - **Multi-layer Sandbox Security**: Application and tool sandboxes for defense in depth
//! - **Cross-platform Support**: Windows, Linux, and macOS
//! - **Flexible Configuration**: JSON-based configuration with hot reload
//! - **Comprehensive Monitoring**: Audit logging and metrics collection
//! - **Multiple Channels**: Discord, Telegram, Feishu integration
//! - **Cron Scheduling**: Automated task execution
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use synbot::sandbox::{SandboxManager, SandboxConfig};
//! use synbot::config::ConfigurationManager;
//! use synbot::sandbox::MonitoringModule;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create configuration and monitoring
//!     let config_manager = ConfigurationManager::new("config.json".to_string());
//!     let monitoring = MonitoringModule::new(Default::default());
//!     
//!     // Create sandbox manager
//!     let manager = SandboxManager::new(config_manager, monitoring);
//!     
//!     // Create an application sandbox
//!     let config = SandboxConfig::default();
//!     let sandbox_id = manager.create_app_sandbox(config).await?;
//!     
//!     println!("Created sandbox: {}", sandbox_id);
//!     Ok(())
//! }
//! ```
//!
//! ## Modules
//!
//! - [`sandbox`] - Multi-layer sandbox security solution
//! - [`agent`] - AI agent management and execution
//! - [`channels`] - Communication channel integrations
//! - [`config`] - Configuration management
//! - [`tools`] - Tool execution and management
//! - [`web`] - Web server and API
//! - [`cron`] - Scheduled task execution
//!
//! ## Sandbox Architecture
//!
//! The sandbox module provides two layers of isolation:
//!
//! 1. **Application Sandbox**: Isolates the main application
//!    - Windows: AppContainer or Sandboxie-Plus
//!    - Linux/macOS: nono.sh with namespaces
//!
//! 2. **Tool Sandbox**: Isolates tool execution
//!    - Windows: WSL2 + gVisor Docker
//!    - Linux/macOS: gVisor Docker
//!
//! ## Configuration
//!
//! Configuration is managed through JSON files. See the `config` module for details.
//!
//! Example configuration:
//!
//! ```json
//! {
//!   "version": "1.0",
//!   "app_sandbox": {
//!     "platform": "auto",
//!     "filesystem": {
//!       "readonly_paths": ["/usr/lib"],
//!       "writable_paths": ["/tmp"]
//!     },
//!     "network": {
//!       "enabled": true,
//!       "allowed_hosts": ["api.example.com"]
//!     }
//!   }
//! }
//! ```
//!
//! ## Security
//!
//! The sandbox system implements multiple security layers:
//!
//! - Filesystem access control
//! - Network isolation
//! - Resource limits (CPU, memory, disk)
//! - Process isolation
//! - Privilege escalation prevention
//! - Sandbox escape prevention
//!
//! ## Monitoring
//!
//! All sandbox activities are logged and monitored:
//!
//! - File access auditing
//! - Network connection tracking
//! - Process creation logging
//! - Security violation detection
//! - Resource usage metrics

pub mod agent;
#[cfg(target_os = "windows")]
pub mod appcontainer_dns;
pub mod bus;
pub mod channels;
pub mod cli;
pub mod config;
pub mod cron;
pub mod heartbeat;
pub mod logging;
pub mod sandbox;
pub mod tools;
pub mod url_utils;
pub mod web;
