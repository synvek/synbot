// Error types for the sandbox security solution

use thiserror::Error;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Main sandbox error type
#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    #[error("Sandbox creation failed: {0}")]
    CreationFailed(String),
    
    #[error("Sandbox execution error: {0}")]
    ExecutionFailed(String),
    
    #[error("Security violation: {0}")]
    SecurityViolation(String),
    
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),
    
    #[error("Timeout")]
    Timeout,
    
    #[error("Unsupported platform")]
    UnsupportedPlatform,
    
    #[error("Sandbox not found")]
    NotFound,
    
    #[error("Sandbox not started")]
    NotStarted,
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl SandboxError {
    /// Convert error to detailed error report in JSON format
    /// 
    /// This provides a structured error report that includes:
    /// - Error code
    /// - Error message
    /// - Timestamp
    /// - Context information
    /// - Suggestions for resolution
    pub fn to_error_report(&self, sandbox_id: Option<&str>, context: Option<HashMap<String, String>>) -> ErrorReport {
        let (error_code, suggestion) = match self {
            SandboxError::Configuration(_) => (
                "SANDBOX_CONFIGURATION_ERROR",
                "Please check the configuration file for missing or invalid fields"
            ),
            SandboxError::CreationFailed(_) => (
                "SANDBOX_CREATION_FAILED",
                "Ensure all dependencies are installed and you have sufficient permissions"
            ),
            SandboxError::ExecutionFailed(_) => (
                "SANDBOX_EXECUTION_FAILED",
                "Check the command syntax and ensure the sandbox is running"
            ),
            SandboxError::SecurityViolation(_) => (
                "SECURITY_VIOLATION",
                "This operation violates the sandbox security policy"
            ),
            SandboxError::ResourceExhausted(_) => (
                "RESOURCE_EXHAUSTED",
                "Increase resource limits in the configuration or reduce resource usage"
            ),
            SandboxError::Timeout => (
                "EXECUTION_TIMEOUT",
                "Increase the timeout value or optimize the command execution"
            ),
            SandboxError::UnsupportedPlatform => (
                "UNSUPPORTED_PLATFORM",
                "This platform is not supported. Supported platforms: Windows, Linux, macOS"
            ),
            SandboxError::NotFound => (
                "SANDBOX_NOT_FOUND",
                "Verify the sandbox ID and ensure the sandbox has been created"
            ),
            SandboxError::NotStarted => (
                "SANDBOX_NOT_STARTED",
                "Start the sandbox before executing commands"
            ),
            SandboxError::Io(_) => (
                "IO_ERROR",
                "Check file permissions and disk space"
            ),
            SandboxError::Json(_) => (
                "JSON_PARSE_ERROR",
                "Ensure the JSON data is properly formatted"
            ),
        };
        
        let mut error_context = context.unwrap_or_default();
        error_context.insert("platform".to_string(), std::env::consts::OS.to_string());
        
        if let Some(id) = sandbox_id {
            error_context.insert("sandbox_id".to_string(), id.to_string());
        }
        
        ErrorReport {
            error_code: error_code.to_string(),
            error_message: self.to_string(),
            timestamp: Utc::now(),
            context: error_context,
            suggestion: suggestion.to_string(),
        }
    }
    
    /// Get error severity level
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            SandboxError::SecurityViolation(_) => ErrorSeverity::Critical,
            SandboxError::ResourceExhausted(_) => ErrorSeverity::Critical,
            SandboxError::CreationFailed(_) => ErrorSeverity::Error,
            SandboxError::ExecutionFailed(_) => ErrorSeverity::Error,
            SandboxError::Configuration(_) => ErrorSeverity::Error,
            SandboxError::Timeout => ErrorSeverity::Warning,
            SandboxError::NotFound => ErrorSeverity::Warning,
            SandboxError::NotStarted => ErrorSeverity::Warning,
            SandboxError::UnsupportedPlatform => ErrorSeverity::Error,
            SandboxError::Io(_) => ErrorSeverity::Error,
            SandboxError::Json(_) => ErrorSeverity::Warning,
        }
    }
    
    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            SandboxError::Timeout => true,
            SandboxError::ExecutionFailed(_) => true,
            SandboxError::ResourceExhausted(_) => true,
            SandboxError::NotStarted => true,
            SandboxError::Io(_) => true,
            _ => false,
        }
    }
}

/// Configuration error type
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Missing field: {0}")]
    MissingField(String),
    
    #[error("Invalid value: {0}")]
    InvalidValue(String),
    
    #[error("Reload failed: {0}")]
    ReloadFailed(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl ConfigError {
    /// Convert error to detailed error report
    pub fn to_error_report(&self, context: Option<HashMap<String, String>>) -> ErrorReport {
        let (error_code, suggestion) = match self {
            ConfigError::MissingField(_) => (
                "CONFIG_MISSING_FIELD",
                "Add the required field to the configuration file"
            ),
            ConfigError::InvalidValue(_) => (
                "CONFIG_INVALID_VALUE",
                "Check the configuration value against the allowed range or format"
            ),
            ConfigError::ReloadFailed(_) => (
                "CONFIG_RELOAD_FAILED",
                "Fix the configuration errors and try reloading again"
            ),
            ConfigError::Io(_) => (
                "CONFIG_IO_ERROR",
                "Check file permissions and ensure the configuration file exists"
            ),
            ConfigError::Json(_) => (
                "CONFIG_JSON_ERROR",
                "Validate the JSON syntax in the configuration file"
            ),
        };
        
        let mut error_context = context.unwrap_or_default();
        error_context.insert("platform".to_string(), std::env::consts::OS.to_string());
        
        ErrorReport {
            error_code: error_code.to_string(),
            error_message: self.to_string(),
            timestamp: Utc::now(),
            context: error_context,
            suggestion: suggestion.to_string(),
        }
    }
}

/// Error severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    /// Critical error requiring immediate attention
    Critical,
    /// Error that prevents normal operation
    Error,
    /// Warning that may lead to errors
    Warning,
    /// Informational message
    Info,
}

/// Detailed error report structure
/// 
/// This structure provides comprehensive error information in a format
/// suitable for logging, monitoring, and user feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorReport {
    /// Error code (e.g., "SANDBOX_CREATION_FAILED")
    pub error_code: String,
    
    /// Human-readable error message
    pub error_message: String,
    
    /// Timestamp when the error occurred
    pub timestamp: DateTime<Utc>,
    
    /// Additional context information
    pub context: HashMap<String, String>,
    
    /// Suggestion for resolving the error
    pub suggestion: String,
}

impl ErrorReport {
    /// Convert error report to JSON string
    pub fn to_json(&self) -> std::result::Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    
    /// Create error report from JSON string
    pub fn from_json(json: &str) -> std::result::Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

/// Result type alias for sandbox operations
pub type Result<T> = std::result::Result<T, SandboxError>;

/// Result type alias for configuration operations
pub type ConfigResult<T> = std::result::Result<T, ConfigError>;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_report_creation() {
        let error = SandboxError::NotFound;
        let report = error.to_error_report(Some("test-sandbox"), None);
        
        assert_eq!(report.error_code, "SANDBOX_NOT_FOUND");
        assert!(report.context.contains_key("sandbox_id"));
        assert_eq!(report.context.get("sandbox_id").unwrap(), "test-sandbox");
    }
    
    #[test]
    fn test_error_report_json_serialization() {
        let error = SandboxError::Configuration("Invalid memory limit".to_string());
        let report = error.to_error_report(Some("test-sandbox"), None);
        
        let json = report.to_json().unwrap();
        assert!(json.contains("SANDBOX_CONFIGURATION_ERROR"));
        assert!(json.contains("test-sandbox"));
        
        let parsed = ErrorReport::from_json(&json).unwrap();
        assert_eq!(parsed.error_code, report.error_code);
    }
    
    #[test]
    fn test_error_severity() {
        assert_eq!(SandboxError::SecurityViolation("test".to_string()).severity(), ErrorSeverity::Critical);
        assert_eq!(SandboxError::Timeout.severity(), ErrorSeverity::Warning);
        assert_eq!(SandboxError::CreationFailed("test".to_string()).severity(), ErrorSeverity::Error);
    }
    
    #[test]
    fn test_error_recoverability() {
        assert!(SandboxError::Timeout.is_recoverable());
        assert!(SandboxError::ExecutionFailed("test".to_string()).is_recoverable());
        assert!(!SandboxError::SecurityViolation("test".to_string()).is_recoverable());
        assert!(!SandboxError::UnsupportedPlatform.is_recoverable());
    }
    
    #[test]
    fn test_config_error_report() {
        let error = ConfigError::MissingField("max_memory".to_string());
        let report = error.to_error_report(None);
        
        assert_eq!(report.error_code, "CONFIG_MISSING_FIELD");
        assert!(report.suggestion.contains("required field"));
    }
    
    #[test]
    fn test_error_report_with_context() {
        let mut context = HashMap::new();
        context.insert("user".to_string(), "test_user".to_string());
        context.insert("operation".to_string(), "create_sandbox".to_string());
        
        let error = SandboxError::CreationFailed("Docker not available".to_string());
        let report = error.to_error_report(Some("test-sandbox"), Some(context));
        
        assert!(report.context.contains_key("user"));
        assert!(report.context.contains_key("operation"));
        assert_eq!(report.context.get("user").unwrap(), "test_user");
    }
}
