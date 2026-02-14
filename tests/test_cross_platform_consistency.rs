// Cross-platform consistency tests for error and log formats
//
// These tests verify that error reports and audit logs maintain consistent
// formats across all supported platforms (Windows, Linux, macOS).

use synbot::sandbox::{
    SandboxError, ConfigError, ErrorReport, ErrorSeverity,
    MonitoringModule, MonitoringConfig, AuditConfig, MetricsConfig, AuditEvent,
};
use std::collections::HashMap;
use chrono::Utc;

/// Test that error reports have consistent structure across platforms
#[test]
fn test_error_report_structure_consistency() {
    let errors = vec![
        SandboxError::Configuration("Invalid config".to_string()),
        SandboxError::CreationFailed("Failed to create".to_string()),
        SandboxError::ExecutionFailed("Execution error".to_string()),
        SandboxError::SecurityViolation("Security breach".to_string()),
        SandboxError::ResourceExhausted("Out of memory".to_string()),
        SandboxError::Timeout,
        SandboxError::UnsupportedPlatform,
        SandboxError::NotFound,
        SandboxError::NotStarted,
    ];
    
    for error in errors {
        let report = error.to_error_report(Some("test-sandbox"), None);
        
        // Verify all required fields are present
        assert!(!report.error_code.is_empty(), "error_code must not be empty");
        assert!(!report.error_message.is_empty(), "error_message must not be empty");
        assert!(!report.suggestion.is_empty(), "suggestion must not be empty");
        
        // Verify context contains platform information
        assert!(report.context.contains_key("platform"), "context must contain platform");
        assert!(report.context.contains_key("sandbox_id"), "context must contain sandbox_id");
        
        // Verify timestamp is set
        assert!(report.timestamp <= Utc::now(), "timestamp must be valid");
        
        // Verify error_code follows naming convention (UPPERCASE_WITH_UNDERSCORES)
        assert!(
            report.error_code.chars().all(|c| c.is_uppercase() || c == '_'),
            "error_code must be UPPERCASE_WITH_UNDERSCORES: {}",
            report.error_code
        );
    }
}

/// Test that error reports can be serialized to JSON consistently
#[test]
fn test_error_report_json_serialization() {
    let error = SandboxError::CreationFailed("Docker not available".to_string());
    let report = error.to_error_report(Some("test-sandbox"), None);
    
    // Serialize to JSON
    let json = report.to_json().expect("Should serialize to JSON");
    
    // Verify JSON contains all required fields
    assert!(json.contains("error_code"), "JSON must contain error_code");
    assert!(json.contains("error_message"), "JSON must contain error_message");
    assert!(json.contains("timestamp"), "JSON must contain timestamp");
    assert!(json.contains("context"), "JSON must contain context");
    assert!(json.contains("suggestion"), "JSON must contain suggestion");
    
    // Deserialize back
    let parsed = ErrorReport::from_json(&json).expect("Should deserialize from JSON");
    
    // Verify round-trip consistency
    assert_eq!(parsed.error_code, report.error_code);
    assert_eq!(parsed.error_message, report.error_message);
    assert_eq!(parsed.suggestion, report.suggestion);
    assert_eq!(parsed.context.get("sandbox_id"), report.context.get("sandbox_id"));
}

/// Test that error reports with custom context maintain consistency
#[test]
fn test_error_report_with_custom_context() {
    let mut context = HashMap::new();
    context.insert("user".to_string(), "test_user".to_string());
    context.insert("operation".to_string(), "create_sandbox".to_string());
    context.insert("custom_field".to_string(), "custom_value".to_string());
    
    let error = SandboxError::CreationFailed("Test error".to_string());
    let report = error.to_error_report(Some("test-sandbox"), Some(context.clone()));
    
    // Verify custom context is preserved
    assert_eq!(report.context.get("user"), Some(&"test_user".to_string()));
    assert_eq!(report.context.get("operation"), Some(&"create_sandbox".to_string()));
    assert_eq!(report.context.get("custom_field"), Some(&"custom_value".to_string()));
    
    // Verify platform is still added
    assert!(report.context.contains_key("platform"));
    assert!(report.context.contains_key("sandbox_id"));
}

/// Test that config errors have consistent format
#[test]
fn test_config_error_report_consistency() {
    let errors = vec![
        ConfigError::MissingField("max_memory".to_string()),
        ConfigError::InvalidValue("CPU must be positive".to_string()),
        ConfigError::ReloadFailed("Parse error".to_string()),
    ];
    
    for error in errors {
        let report = error.to_error_report(None);
        
        // Verify all required fields are present
        assert!(!report.error_code.is_empty());
        assert!(!report.error_message.is_empty());
        assert!(!report.suggestion.is_empty());
        assert!(report.context.contains_key("platform"));
        
        // Verify error_code starts with CONFIG_
        assert!(
            report.error_code.starts_with("CONFIG_"),
            "Config error codes must start with CONFIG_: {}",
            report.error_code
        );
    }
}

/// Test that error severity levels are consistent
#[test]
fn test_error_severity_consistency() {
    let test_cases = vec![
        (SandboxError::SecurityViolation("test".to_string()), ErrorSeverity::Critical),
        (SandboxError::ResourceExhausted("test".to_string()), ErrorSeverity::Critical),
        (SandboxError::CreationFailed("test".to_string()), ErrorSeverity::Error),
        (SandboxError::ExecutionFailed("test".to_string()), ErrorSeverity::Error),
        (SandboxError::Configuration("test".to_string()), ErrorSeverity::Error),
        (SandboxError::Timeout, ErrorSeverity::Warning),
        (SandboxError::NotFound, ErrorSeverity::Warning),
        (SandboxError::NotStarted, ErrorSeverity::Warning),
    ];
    
    for (error, expected_severity) in test_cases {
        assert_eq!(
            error.severity(),
            expected_severity,
            "Error {:?} should have severity {:?}",
            error,
            expected_severity
        );
    }
}

/// Test that audit events have consistent JSON format
#[test]
fn test_audit_event_json_format() {
    let event = AuditEvent {
        timestamp: Utc::now(),
        sandbox_id: "test-sandbox".to_string(),
        event_type: "file_access".to_string(),
        details: serde_json::json!({
            "path": "/etc/passwd",
            "operation": "read",
            "allowed": false
        }),
    };
    
    let json = event.to_json();
    
    // Verify JSON contains all required fields
    assert!(json.contains("timestamp"));
    assert!(json.contains("sandbox_id"));
    assert!(json.contains("event_type"));
    assert!(json.contains("details"));
    
    // Verify JSON is valid
    let parsed: serde_json::Value = serde_json::from_str(&json)
        .expect("Audit event JSON should be valid");
    
    assert_eq!(parsed["sandbox_id"], "test-sandbox");
    assert_eq!(parsed["event_type"], "file_access");
    assert_eq!(parsed["details"]["path"], "/etc/passwd");
}

/// Test that audit events have consistent syslog format
#[test]
fn test_audit_event_syslog_format() {
    let event = AuditEvent {
        timestamp: Utc::now(),
        sandbox_id: "test-sandbox".to_string(),
        event_type: "network_access".to_string(),
        details: serde_json::json!({
            "host": "example.com",
            "port": 443,
            "allowed": true
        }),
    };
    
    let syslog = event.to_syslog();
    
    // Verify syslog format: <priority>version timestamp hostname app-name procid msgid structured-data msg
    assert!(syslog.starts_with("<"), "Syslog must start with priority");
    assert!(syslog.contains("sandbox"), "Syslog must contain app name");
    assert!(syslog.contains("test-sandbox"), "Syslog must contain sandbox_id");
    
    // Verify timestamp is in RFC3339 format
    assert!(syslog.contains("T"), "Syslog timestamp must be RFC3339");
    assert!(syslog.contains("Z") || syslog.contains("+") || syslog.contains("-"), 
            "Syslog timestamp must have timezone");
}

/// Test that different event types have consistent priority in syslog
#[test]
fn test_audit_event_syslog_priority_consistency() {
    let test_cases = vec![
        ("violation", 3),  // Error
        ("file_access", 6),  // Info
        ("network_access", 6),  // Info
        ("process_creation", 6),  // Info
    ];
    
    for (event_type, expected_priority) in test_cases {
        let event = AuditEvent {
            timestamp: Utc::now(),
            sandbox_id: "test-sandbox".to_string(),
            event_type: event_type.to_string(),
            details: serde_json::json!({}),
        };
        
        let syslog = event.to_syslog();
        
        // Extract priority from syslog format: <priority>...
        let priority_str = syslog.split('>').next()
            .and_then(|s| s.strip_prefix('<'))
            .expect("Should extract priority");
        
        let priority: u8 = priority_str.parse()
            .expect("Priority should be a number");
        
        assert_eq!(
            priority, expected_priority,
            "Event type '{}' should have priority {}",
            event_type, expected_priority
        );
    }
}

/// Test that audit events maintain consistent structure across different event types
#[tokio::test]
async fn test_audit_event_structure_consistency() {
    let config = MonitoringConfig {
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
    };
    
    let monitoring = MonitoringModule::new(config);
    
    // Log different event types
    monitoring.log_file_access("test-sandbox", "/etc/passwd", "read", false).await;
    monitoring.log_network_access("test-sandbox", "example.com", 443, true).await;
    monitoring.log_process_creation("test-sandbox", "bash", &["-c".to_string()]).await;
    monitoring.log_violation("test-sandbox", "unauthorized_access", serde_json::json!({"test": "data"})).await;
    
    // Query all logs
    let logs = monitoring.query_logs(HashMap::new()).await;
    
    // Verify all events have consistent structure
    for event in logs {
        // All events must have these fields
        assert!(!event.sandbox_id.is_empty(), "sandbox_id must not be empty");
        assert!(!event.event_type.is_empty(), "event_type must not be empty");
        assert!(event.timestamp <= Utc::now(), "timestamp must be valid");
        
        // Verify JSON serialization works
        let json = event.to_json();
        assert!(!json.is_empty(), "JSON serialization must work");
        
        // Verify syslog serialization works
        let syslog = event.to_syslog();
        assert!(!syslog.is_empty(), "Syslog serialization must work");
        assert!(syslog.starts_with("<"), "Syslog must have valid format");
    }
}

/// Test that error reports are consistent regardless of platform
#[test]
fn test_error_report_platform_independence() {
    let error = SandboxError::CreationFailed("Test error".to_string());
    
    // Create multiple reports
    let report1 = error.to_error_report(Some("sandbox-1"), None);
    let report2 = error.to_error_report(Some("sandbox-2"), None);
    
    // Verify error_code is the same
    assert_eq!(report1.error_code, report2.error_code);
    
    // Verify suggestion is the same
    assert_eq!(report1.suggestion, report2.suggestion);
    
    // Verify both have platform in context
    assert!(report1.context.contains_key("platform"));
    assert!(report2.context.contains_key("platform"));
    
    // Platform value should be the same (since we're running on the same platform)
    assert_eq!(
        report1.context.get("platform"),
        report2.context.get("platform")
    );
}

/// Test that JSON serialization is deterministic for the same error
#[test]
fn test_error_report_json_determinism() {
    let error = SandboxError::Timeout;
    let report = error.to_error_report(Some("test-sandbox"), None);
    
    // Serialize multiple times
    let json1 = report.to_json().expect("Should serialize");
    let json2 = report.to_json().expect("Should serialize");
    
    // Parse both
    let parsed1: serde_json::Value = serde_json::from_str(&json1).expect("Should parse");
    let parsed2: serde_json::Value = serde_json::from_str(&json2).expect("Should parse");
    
    // Verify key fields are identical
    assert_eq!(parsed1["error_code"], parsed2["error_code"]);
    assert_eq!(parsed1["error_message"], parsed2["error_message"]);
    assert_eq!(parsed1["suggestion"], parsed2["suggestion"]);
}

/// Test that audit log format is consistent across query results
#[tokio::test]
async fn test_audit_log_query_format_consistency() {
    let config = MonitoringConfig {
        log_level: "info".to_string(),
        log_output: vec![],
        audit: AuditConfig {
            file_access: true,
            network_access: true,
            process_creation: true,
            violations: true,
        },
        metrics: MetricsConfig::default(),
    };
    
    let monitoring = MonitoringModule::new(config);
    
    // Log multiple events
    for i in 0..10 {
        monitoring.log_file_access(
            &format!("sandbox-{}", i),
            &format!("/path/{}", i),
            "read",
            true
        ).await;
    }
    
    // Query all logs
    let logs = monitoring.query_logs(HashMap::new()).await;
    
    // Verify all logs have consistent format
    let first_json = logs[0].to_json();
    let first_parsed: serde_json::Value = serde_json::from_str(&first_json)
        .expect("Should parse");
    
    for log in &logs {
        let json = log.to_json();
        let parsed: serde_json::Value = serde_json::from_str(&json)
            .expect("Should parse");
        
        // Verify all have the same structure (same keys)
        assert_eq!(
            first_parsed.as_object().unwrap().keys().collect::<Vec<_>>(),
            parsed.as_object().unwrap().keys().collect::<Vec<_>>(),
            "All audit logs must have the same JSON structure"
        );
    }
}

/// Test that error recoverability is consistent
#[test]
fn test_error_recoverability_consistency() {
    let recoverable_errors = vec![
        SandboxError::Timeout,
        SandboxError::ExecutionFailed("test".to_string()),
        SandboxError::ResourceExhausted("test".to_string()),
        SandboxError::NotStarted,
    ];
    
    let non_recoverable_errors = vec![
        SandboxError::SecurityViolation("test".to_string()),
        SandboxError::UnsupportedPlatform,
        SandboxError::Configuration("test".to_string()),
    ];
    
    for error in recoverable_errors {
        assert!(
            error.is_recoverable(),
            "Error {:?} should be recoverable",
            error
        );
    }
    
    for error in non_recoverable_errors {
        assert!(
            !error.is_recoverable(),
            "Error {:?} should not be recoverable",
            error
        );
    }
}
