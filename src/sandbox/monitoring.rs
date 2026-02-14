// Monitoring and auditing module for the sandbox security solution

use super::types::{AuditEvent, MonitoringConfig, SandboxMetrics};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Logger trait for different log outputs
pub trait Logger: Send + Sync {
    fn log(&self, event: &AuditEvent);
}

/// File logger implementation
pub struct FileLogger {
    path: String,
}

impl FileLogger {
    pub fn new(path: &str) -> Self {
        Self {
            path: path.to_string(),
        }
    }
}

impl Logger for FileLogger {
    fn log(&self, event: &AuditEvent) {
        use std::fs::OpenOptions;
        use std::io::Write;
        
        let json = event.to_json();
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(file, "{}", json);
        }
    }
}

/// Syslog logger implementation
pub struct SyslogLogger {
    facility: String,
}

impl SyslogLogger {
    pub fn new(facility: &str) -> Self {
        Self {
            facility: facility.to_string(),
        }
    }
}

impl Logger for SyslogLogger {
    fn log(&self, event: &AuditEvent) {
        let syslog_msg = event.to_syslog();
        // In a real implementation, this would use a syslog library
        // For now, we'll just log to stderr as a placeholder
        eprintln!("SYSLOG[{}]: {}", self.facility, syslog_msg);
    }
}

/// Monitoring module
pub struct MonitoringModule {
    config: MonitoringConfig,
    loggers: Vec<Arc<dyn Logger>>,
    metrics_collector: MetricsCollector,
    audit_log: Arc<RwLock<Vec<AuditEvent>>>,
}

impl MonitoringModule {
    /// Create a new monitoring module
    pub fn new(config: MonitoringConfig) -> Self {
        let loggers = Self::init_loggers(&config);
        let metrics_collector = MetricsCollector::new();
        
        Self {
            config,
            loggers,
            metrics_collector,
            audit_log: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    /// Initialize loggers based on configuration
    fn init_loggers(config: &MonitoringConfig) -> Vec<Arc<dyn Logger>> {
        let mut loggers: Vec<Arc<dyn Logger>> = Vec::new();
        
        for output in &config.log_output {
            match output.output_type.as_str() {
                "file" => loggers.push(Arc::new(FileLogger::new(&output.path))),
                "syslog" => loggers.push(Arc::new(SyslogLogger::new(&output.facility))),
                _ => {}
            }
        }
        
        loggers
    }
    
    /// Log sandbox creation
    pub fn log_sandbox_created(&self, sandbox_id: &str, sandbox_type: &str) {
        let event = AuditEvent {
            timestamp: Utc::now(),
            sandbox_id: sandbox_id.to_string(),
            event_type: "sandbox_created".to_string(),
            details: serde_json::json!({
                "sandbox_type": sandbox_type
            }),
        };
        
        // Write to loggers synchronously
        for logger in &self.loggers {
            logger.log(&event);
        }
    }
    
    /// Log file access
    pub async fn log_file_access(&self, sandbox_id: &str, path: &str, operation: &str, allowed: bool) {
        if !self.config.audit.file_access {
            return;
        }
        
        let event = AuditEvent {
            timestamp: Utc::now(),
            sandbox_id: sandbox_id.to_string(),
            event_type: "file_access".to_string(),
            details: serde_json::json!({
                "path": path,
                "operation": operation,
                "allowed": allowed
            }),
        };
        
        self.write_audit_log(event).await;
    }
    
    /// Log network access
    pub async fn log_network_access(&self, sandbox_id: &str, host: &str, port: u16, allowed: bool) {
        if !self.config.audit.network_access {
            return;
        }
        
        let event = AuditEvent {
            timestamp: Utc::now(),
            sandbox_id: sandbox_id.to_string(),
            event_type: "network_access".to_string(),
            details: serde_json::json!({
                "host": host,
                "port": port,
                "allowed": allowed
            }),
        };
        
        self.write_audit_log(event).await;
    }
    
    /// Log process creation
    pub async fn log_process_creation(&self, sandbox_id: &str, command: &str, args: &[String]) {
        if !self.config.audit.process_creation {
            return;
        }
        
        let event = AuditEvent {
            timestamp: Utc::now(),
            sandbox_id: sandbox_id.to_string(),
            event_type: "process_creation".to_string(),
            details: serde_json::json!({
                "command": command,
                "args": args
            }),
        };
        
        self.write_audit_log(event).await;
    }
    
    /// Log violation
    pub async fn log_violation(&self, sandbox_id: &str, violation_type: &str, details: serde_json::Value) {
        if !self.config.audit.violations {
            return;
        }
        
        let mut event_details = details.clone();
        if let Some(obj) = event_details.as_object_mut() {
            obj.insert("violation_type".to_string(), serde_json::json!(violation_type));
        }
        
        let event = AuditEvent {
            timestamp: Utc::now(),
            sandbox_id: sandbox_id.to_string(),
            event_type: "violation".to_string(),
            details: event_details,
        };
        
        self.write_audit_log(event.clone()).await;
        
        // Violations also trigger alerts
        self.send_alert(event);
    }
    
    /// Write audit log to all configured loggers
    async fn write_audit_log(&self, event: AuditEvent) {
        // Store in memory for querying
        let mut log = self.audit_log.write().await;
        log.push(event.clone());
        
        // Write to all loggers
        for logger in &self.loggers {
            logger.log(&event);
        }
    }
    
    /// Send alert for critical events
    fn send_alert(&self, event: AuditEvent) {
        // In a real implementation, this would send alerts via email, SMS, etc.
        // For now, we'll just log to stderr
        eprintln!("ALERT: {:?}", event);
    }
    
    /// Collect sandbox metrics
    pub fn collect_metrics(&self, sandbox_id: &str) -> SandboxMetrics {
        self.metrics_collector.collect(sandbox_id)
    }
    
    /// Query audit logs with filters
    /// 
    /// Supported filters:
    /// - "sandbox_id": Filter by sandbox ID
    /// - "event_type": Filter by event type (file_access, network_access, process_creation, violation)
    /// - "start_time": Filter by start timestamp (RFC3339 format)
    /// - "end_time": Filter by end timestamp (RFC3339 format)
    pub async fn query_logs(&self, filters: HashMap<String, String>) -> Vec<AuditEvent> {
        let log = self.audit_log.read().await;
        
        log.iter()
            .filter(|event| self.matches_filters(event, &filters))
            .cloned()
            .collect()
    }
    
    /// Check if an event matches the given filters
    fn matches_filters(&self, event: &AuditEvent, filters: &HashMap<String, String>) -> bool {
        // Filter by sandbox_id
        if let Some(sandbox_id) = filters.get("sandbox_id") {
            if &event.sandbox_id != sandbox_id {
                return false;
            }
        }
        
        // Filter by event_type
        if let Some(event_type) = filters.get("event_type") {
            if &event.event_type != event_type {
                return false;
            }
        }
        
        // Filter by start_time
        if let Some(start_time_str) = filters.get("start_time") {
            if let Ok(start_time) = chrono::DateTime::parse_from_rfc3339(start_time_str) {
                if event.timestamp < start_time.with_timezone(&Utc) {
                    return false;
                }
            }
        }
        
        // Filter by end_time
        if let Some(end_time_str) = filters.get("end_time") {
            if let Ok(end_time) = chrono::DateTime::parse_from_rfc3339(end_time_str) {
                if event.timestamp > end_time.with_timezone(&Utc) {
                    return false;
                }
            }
        }
        
        true
    }
}

/// Metrics collector
pub struct MetricsCollector {
    // Store metrics history for each sandbox
    metrics_history: Arc<RwLock<HashMap<String, Vec<(chrono::DateTime<Utc>, SandboxMetrics)>>>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics_history: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Collect metrics for a sandbox
    pub fn collect(&self, sandbox_id: &str) -> SandboxMetrics {
        // In a real implementation, this would query the actual sandbox runtime
        // For now, we'll collect basic system metrics as placeholders
        
        let cpu_usage = self.get_cpu_usage(sandbox_id);
        let memory_usage = self.get_memory_usage(sandbox_id);
        let disk_usage = self.get_disk_usage(sandbox_id);
        let network_io = self.get_network_io(sandbox_id);
        
        SandboxMetrics {
            cpu_usage,
            memory_usage,
            disk_usage,
            network_io,
        }
    }
    
    /// Store metrics for historical tracking
    pub async fn store_metrics(&self, sandbox_id: &str, metrics: SandboxMetrics) {
        let mut history = self.metrics_history.write().await;
        let entry = history.entry(sandbox_id.to_string()).or_insert_with(Vec::new);
        entry.push((Utc::now(), metrics));
        
        // Keep only last 1000 entries per sandbox
        if entry.len() > 1000 {
            entry.remove(0);
        }
    }
    
    /// Get historical metrics for a sandbox
    pub async fn get_history(&self, sandbox_id: &str) -> Vec<(chrono::DateTime<Utc>, SandboxMetrics)> {
        let history = self.metrics_history.read().await;
        history.get(sandbox_id).cloned().unwrap_or_default()
    }
    
    /// Clear metrics history for a sandbox
    pub async fn clear_history(&self, sandbox_id: &str) {
        let mut history = self.metrics_history.write().await;
        history.remove(sandbox_id);
    }
    
    /// Get CPU usage for a sandbox
    fn get_cpu_usage(&self, _sandbox_id: &str) -> f64 {
        // In a real implementation, this would query cgroup stats or Docker API
        // Placeholder: return 0.0
        0.0
    }
    
    /// Get memory usage for a sandbox
    fn get_memory_usage(&self, _sandbox_id: &str) -> u64 {
        // In a real implementation, this would query cgroup stats or Docker API
        // Placeholder: return 0
        0
    }
    
    /// Get disk usage for a sandbox
    fn get_disk_usage(&self, _sandbox_id: &str) -> u64 {
        // In a real implementation, this would query filesystem stats
        // Placeholder: return 0
        0
    }
    
    /// Get network I/O for a sandbox
    fn get_network_io(&self, _sandbox_id: &str) -> HashMap<String, u64> {
        // In a real implementation, this would query network stats
        // Placeholder: return empty map
        let mut io = HashMap::new();
        io.insert("rx_bytes".to_string(), 0);
        io.insert("tx_bytes".to_string(), 0);
        io
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::types::{AuditConfig, LogOutput, MetricsConfig};
    
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
                endpoint: String::new(),
            },
        }
    }
    
    #[tokio::test]
    async fn test_log_file_access() {
        let config = create_test_monitoring_config();
        let monitoring = MonitoringModule::new(config);
        
        monitoring.log_file_access("test-sandbox", "/etc/passwd", "read", false).await;
        
        let log = monitoring.audit_log.read().await;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].event_type, "file_access");
        assert_eq!(log[0].sandbox_id, "test-sandbox");
    }
    
    #[tokio::test]
    async fn test_log_network_access() {
        let config = create_test_monitoring_config();
        let monitoring = MonitoringModule::new(config);
        
        monitoring.log_network_access("test-sandbox", "example.com", 443, true).await;
        
        let log = monitoring.audit_log.read().await;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].event_type, "network_access");
    }
    
    #[tokio::test]
    async fn test_log_process_creation() {
        let config = create_test_monitoring_config();
        let monitoring = MonitoringModule::new(config);
        
        monitoring.log_process_creation("test-sandbox", "bash", &["-c".to_string(), "ls".to_string()]).await;
        
        let log = monitoring.audit_log.read().await;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].event_type, "process_creation");
    }
    
    #[tokio::test]
    async fn test_log_violation() {
        let config = create_test_monitoring_config();
        let monitoring = MonitoringModule::new(config);
        
        let details = serde_json::json!({
            "attempted_path": "/root/.ssh/id_rsa",
            "action": "read"
        });
        
        monitoring.log_violation("test-sandbox", "unauthorized_file_access", details).await;
        
        let log = monitoring.audit_log.read().await;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].event_type, "violation");
    }
    
    #[tokio::test]
    async fn test_audit_disabled() {
        let mut config = create_test_monitoring_config();
        config.audit.file_access = false;
        
        let monitoring = MonitoringModule::new(config);
        
        monitoring.log_file_access("test-sandbox", "/etc/passwd", "read", false).await;
        
        let log = monitoring.audit_log.read().await;
        assert_eq!(log.len(), 0);
    }
    
    #[tokio::test]
    async fn test_query_logs_by_sandbox_id() {
        let config = create_test_monitoring_config();
        let monitoring = MonitoringModule::new(config);
        
        // Log events for different sandboxes
        monitoring.log_file_access("sandbox-1", "/etc/passwd", "read", false).await;
        monitoring.log_file_access("sandbox-2", "/tmp/file", "write", true).await;
        monitoring.log_network_access("sandbox-1", "example.com", 443, true).await;
        
        // Query for sandbox-1 only
        let mut filters = HashMap::new();
        filters.insert("sandbox_id".to_string(), "sandbox-1".to_string());
        
        let results = monitoring.query_logs(filters).await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.sandbox_id == "sandbox-1"));
    }
    
    #[tokio::test]
    async fn test_query_logs_by_event_type() {
        let config = create_test_monitoring_config();
        let monitoring = MonitoringModule::new(config);
        
        // Log different event types
        monitoring.log_file_access("test-sandbox", "/etc/passwd", "read", false).await;
        monitoring.log_network_access("test-sandbox", "example.com", 443, true).await;
        monitoring.log_process_creation("test-sandbox", "bash", &[]).await;
        
        // Query for file_access only
        let mut filters = HashMap::new();
        filters.insert("event_type".to_string(), "file_access".to_string());
        
        let results = monitoring.query_logs(filters).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].event_type, "file_access");
    }
    
    #[tokio::test]
    async fn test_query_logs_by_time_range() {
        use std::time::Duration;
        use tokio::time::sleep;
        
        let config = create_test_monitoring_config();
        let monitoring = MonitoringModule::new(config);
        
        // Log first event
        monitoring.log_file_access("test-sandbox", "/file1", "read", true).await;
        
        let start_time = Utc::now();
        sleep(Duration::from_millis(100)).await;
        
        // Log second event
        monitoring.log_file_access("test-sandbox", "/file2", "read", true).await;
        
        sleep(Duration::from_millis(100)).await;
        let end_time = Utc::now();
        
        // Log third event
        monitoring.log_file_access("test-sandbox", "/file3", "read", true).await;
        
        // Query for events in time range
        let mut filters = HashMap::new();
        filters.insert("start_time".to_string(), start_time.to_rfc3339());
        filters.insert("end_time".to_string(), end_time.to_rfc3339());
        
        let results = monitoring.query_logs(filters).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].details.get("path").and_then(|v| v.as_str()), Some("/file2"));
    }
    
    #[tokio::test]
    async fn test_query_logs_multiple_filters() {
        let config = create_test_monitoring_config();
        let monitoring = MonitoringModule::new(config);
        
        // Log events
        monitoring.log_file_access("sandbox-1", "/file1", "read", true).await;
        monitoring.log_file_access("sandbox-2", "/file2", "read", true).await;
        monitoring.log_network_access("sandbox-1", "example.com", 443, true).await;
        
        // Query with multiple filters
        let mut filters = HashMap::new();
        filters.insert("sandbox_id".to_string(), "sandbox-1".to_string());
        filters.insert("event_type".to_string(), "file_access".to_string());
        
        let results = monitoring.query_logs(filters).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].sandbox_id, "sandbox-1");
        assert_eq!(results[0].event_type, "file_access");
    }
    
    #[tokio::test]
    async fn test_query_logs_no_filters() {
        let config = create_test_monitoring_config();
        let monitoring = MonitoringModule::new(config);
        
        // Log multiple events
        monitoring.log_file_access("sandbox-1", "/file1", "read", true).await;
        monitoring.log_network_access("sandbox-2", "example.com", 443, true).await;
        monitoring.log_process_creation("sandbox-3", "bash", &[]).await;
        
        // Query without filters (should return all)
        let filters = HashMap::new();
        let results = monitoring.query_logs(filters).await;
        assert_eq!(results.len(), 3);
    }
    
    #[test]
    fn test_metrics_collector_collect() {
        let collector = MetricsCollector::new();
        let metrics = collector.collect("test-sandbox");
        
        // Verify metrics structure
        assert_eq!(metrics.cpu_usage, 0.0);
        assert_eq!(metrics.memory_usage, 0);
        assert_eq!(metrics.disk_usage, 0);
        assert!(metrics.network_io.contains_key("rx_bytes"));
        assert!(metrics.network_io.contains_key("tx_bytes"));
    }
    
    #[tokio::test]
    async fn test_metrics_collector_store_and_retrieve() {
        let collector = MetricsCollector::new();
        
        let metrics1 = SandboxMetrics {
            cpu_usage: 25.5,
            memory_usage: 1024 * 1024 * 512,
            disk_usage: 1024 * 1024 * 1024,
            network_io: {
                let mut io = HashMap::new();
                io.insert("rx_bytes".to_string(), 1000);
                io.insert("tx_bytes".to_string(), 2000);
                io
            },
        };
        
        collector.store_metrics("test-sandbox", metrics1).await;
        
        let history = collector.get_history("test-sandbox").await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].1.cpu_usage, 25.5);
    }
    
    #[tokio::test]
    async fn test_metrics_collector_history_limit() {
        let collector = MetricsCollector::new();
        
        // Store more than 1000 metrics
        for i in 0..1100 {
            let metrics = SandboxMetrics {
                cpu_usage: i as f64,
                memory_usage: 0,
                disk_usage: 0,
                network_io: HashMap::new(),
            };
            collector.store_metrics("test-sandbox", metrics).await;
        }
        
        let history = collector.get_history("test-sandbox").await;
        // Should keep only last 1000
        assert_eq!(history.len(), 1000);
        // First entry should be from iteration 100 (0-99 were removed)
        assert_eq!(history[0].1.cpu_usage, 100.0);
    }
    
    #[tokio::test]
    async fn test_metrics_collector_clear_history() {
        let collector = MetricsCollector::new();
        
        let metrics = SandboxMetrics {
            cpu_usage: 50.0,
            memory_usage: 0,
            disk_usage: 0,
            network_io: HashMap::new(),
        };
        
        collector.store_metrics("test-sandbox", metrics).await;
        
        let history = collector.get_history("test-sandbox").await;
        assert_eq!(history.len(), 1);
        
        collector.clear_history("test-sandbox").await;
        
        let history = collector.get_history("test-sandbox").await;
        assert_eq!(history.len(), 0);
    }
    
    #[test]
    fn test_monitoring_module_collect_metrics() {
        let config = create_test_monitoring_config();
        let monitoring = MonitoringModule::new(config);
        
        let metrics = monitoring.collect_metrics("test-sandbox");
        
        // Verify metrics are returned
        assert_eq!(metrics.cpu_usage, 0.0);
        assert_eq!(metrics.memory_usage, 0);
    }
}
