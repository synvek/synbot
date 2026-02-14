// Core data structures for the sandbox security solution

use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::time::Duration;
use std::collections::HashMap;

/// Sandbox configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SandboxConfig {
    pub sandbox_id: String,
    pub platform: String,
    pub filesystem: FilesystemConfig,
    pub network: NetworkConfig,
    pub resources: ResourceConfig,
    pub process: ProcessConfig,
    pub monitoring: MonitoringConfig,
}

/// Filesystem configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilesystemConfig {
    pub readonly_paths: Vec<String>,
    pub writable_paths: Vec<String>,
    pub hidden_paths: Vec<String>,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkConfig {
    pub enabled: bool,
    pub allowed_hosts: Vec<String>,
    pub allowed_ports: Vec<u16>,
}

/// Resource limits configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceConfig {
    pub max_memory: u64,  // bytes
    pub max_cpu: f64,     // CPU cores
    pub max_disk: u64,    // bytes
}

/// Process control configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessConfig {
    pub allow_fork: bool,
    pub max_processes: u32,
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitoringConfig {
    pub log_level: String,
    pub log_output: Vec<LogOutput>,
    pub audit: AuditConfig,
    pub metrics: MetricsConfig,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            log_level: "info".to_string(),
            log_output: vec![],
            audit: AuditConfig::default(),
            metrics: MetricsConfig::default(),
        }
    }
}

/// Log output configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogOutput {
    #[serde(rename = "type")]
    pub output_type: String,
    pub path: String,
    #[serde(default)]
    pub facility: String,
}

/// Audit configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditConfig {
    pub file_access: bool,
    pub network_access: bool,
    pub process_creation: bool,
    pub violations: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            file_access: true,
            network_access: true,
            process_creation: true,
            violations: true,
        }
    }
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub interval: u64,
    pub endpoint: String,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: 60,
            endpoint: String::new(),
        }
    }
}

/// Sandbox status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxStatus {
    pub sandbox_id: String,
    pub state: SandboxState,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

/// Sandbox state enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SandboxState {
    Created,
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
}

/// Execution result
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration: Duration,
    pub error: Option<String>,
}

/// Health status
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub healthy: bool,
    pub checks: HashMap<String, bool>,
    pub message: String,
}

/// Sandbox metrics
#[derive(Debug, Clone)]
pub struct SandboxMetrics {
    pub cpu_usage: f64,      // percentage
    pub memory_usage: u64,   // bytes
    pub disk_usage: u64,     // bytes
    pub network_io: HashMap<String, u64>,  // {'rx_bytes': ..., 'tx_bytes': ...}
}

/// Sandbox information
#[derive(Debug, Clone)]
pub struct SandboxInfo {
    pub sandbox_id: String,
    pub platform: String,
    pub sandbox_type: String,
}

/// Audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub sandbox_id: String,
    pub event_type: String,  // file_access | network_access | process_creation | violation
    pub details: serde_json::Value,
}

impl AuditEvent {
    /// Convert to JSON format
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self).unwrap_or_default()
    }
    
    /// Convert to syslog format
    pub fn to_syslog(&self) -> String {
        format!(
            "<{}>1 {} sandbox {} - - - {}",
            self.get_priority(),
            self.timestamp.to_rfc3339(),
            self.sandbox_id,
            serde_json::to_string(&self.details).unwrap_or_default()
        )
    }
    
    fn get_priority(&self) -> u8 {
        // Return syslog priority based on event type
        match self.event_type.as_str() {
            "violation" => 3,  // Error
            "file_access" | "network_access" | "process_creation" => 6,  // Info
            _ => 7,  // Debug
        }
    }
}
