use synbot::web::{create_log_buffer, LogEntry};
use tracing::Level;

#[tokio::test]
async fn test_log_buffer_filtering() {
    let log_buffer = create_log_buffer(100);
    
    {
        let mut buffer = log_buffer.write().await;
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "info message".to_string()));
        buffer.push(LogEntry::new(Level::WARN, "test".to_string(), "warn message".to_string()));
        buffer.push(LogEntry::new(Level::ERROR, "test".to_string(), "error message".to_string()));
        buffer.push(LogEntry::new(Level::DEBUG, "test".to_string(), "debug message".to_string()));
    }
    
    // Test level filtering
    {
        let buffer = log_buffer.read().await;
        let filtered = buffer.get_filtered(Some("WARN"), None);
        assert_eq!(filtered.len(), 2); // WARN and ERROR
        assert!(filtered.iter().all(|e| e.level == "WARN" || e.level == "ERROR"));
    }
    
    // Test keyword filtering
    {
        let buffer = log_buffer.read().await;
        let filtered = buffer.get_filtered(None, Some("error"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].level, "ERROR");
    }
    
    // Test combined filtering
    {
        let buffer = log_buffer.read().await;
        let filtered = buffer.get_filtered(Some("INFO"), Some("message"));
        assert_eq!(filtered.len(), 3); // INFO, WARN, ERROR (all contain "message")
    }
}

#[tokio::test]
async fn test_log_buffer_capacity() {
    let log_buffer = create_log_buffer(3);
    
    {
        let mut buffer = log_buffer.write().await;
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "msg1".to_string()));
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "msg2".to_string()));
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "msg3".to_string()));
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "msg4".to_string()));
    }
    
    let buffer = log_buffer.read().await;
    assert_eq!(buffer.len(), 3);
    
    let all_logs = buffer.get_all();
    // Newest first
    assert_eq!(all_logs[0].message, "msg4");
    assert_eq!(all_logs[2].message, "msg2");
}

#[tokio::test]
async fn test_log_buffer_reverse_chronological_order() {
    let log_buffer = create_log_buffer(10);
    
    {
        let mut buffer = log_buffer.write().await;
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "first".to_string()));
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "second".to_string()));
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "third".to_string()));
    }
    
    let buffer = log_buffer.read().await;
    let logs = buffer.get_all();
    
    // Should be in reverse chronological order (newest first)
    assert_eq!(logs[0].message, "third");
    assert_eq!(logs[1].message, "second");
    assert_eq!(logs[2].message, "first");
}
