use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::Level;

/// A log entry stored in the buffer
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub target: String,
    pub message: String,
}

impl LogEntry {
    pub fn new(level: Level, target: String, message: String) -> Self {
        Self {
            timestamp: Utc::now(),
            level: level.to_string(),
            target,
            message,
        }
    }
}

/// Ring buffer for storing recent log entries
pub struct LogBuffer {
    entries: VecDeque<LogEntry>,
    capacity: usize,
    broadcast_tx: broadcast::Sender<LogEntry>,
}

impl LogBuffer {
    /// Create a new LogBuffer with the specified capacity
    pub fn new(capacity: usize) -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
            broadcast_tx,
        }
    }

    /// Add a log entry to the buffer
    /// If the buffer is at capacity, the oldest entry is removed
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry.clone());
        
        // Broadcast to subscribers (ignore errors if no subscribers)
        let _ = self.broadcast_tx.send(entry);
    }

    /// Subscribe to new log entries
    pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.broadcast_tx.subscribe()
    }

    /// Get all log entries in reverse chronological order (newest first)
    pub fn get_all(&self) -> Vec<LogEntry> {
        self.entries.iter().rev().cloned().collect()
    }

    /// Get log entries filtered by level and keyword
    /// Returns entries in reverse chronological order (newest first)
    pub fn get_filtered(&self, level_filter: Option<&str>, keyword: Option<&str>) -> Vec<LogEntry> {
        self.entries
            .iter()
            .rev()
            .filter(|entry| {
                // Filter by level if specified
                if let Some(level) = level_filter {
                    if !Self::level_matches(&entry.level, level) {
                        return false;
                    }
                }
                
                // Filter by keyword if specified
                if let Some(kw) = keyword {
                    if !entry.message.contains(kw) {
                        return false;
                    }
                }
                
                true
            })
            .cloned()
            .collect()
    }

    /// Check if a log level matches or exceeds the filter level
    fn level_matches(entry_level: &str, filter_level: &str) -> bool {
        let entry_severity = Self::level_to_severity(entry_level);
        let filter_severity = Self::level_to_severity(filter_level);
        entry_severity >= filter_severity
    }

    /// Convert log level string to numeric severity (higher = more severe)
    fn level_to_severity(level: &str) -> u8 {
        match level.to_uppercase().as_str() {
            "TRACE" => 0,
            "DEBUG" => 1,
            "INFO" => 2,
            "WARN" => 3,
            "ERROR" => 4,
            _ => 0,
        }
    }

    /// Get the current number of entries in the buffer
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Thread-safe wrapper for LogBuffer
pub type SharedLogBuffer = Arc<RwLock<LogBuffer>>;

/// Create a new shared log buffer
pub fn create_log_buffer(capacity: usize) -> SharedLogBuffer {
    Arc::new(RwLock::new(LogBuffer::new(capacity)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::Level;

    #[test]
    fn test_log_buffer_capacity() {
        let mut buffer = LogBuffer::new(3);
        
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "msg1".to_string()));
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "msg2".to_string()));
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "msg3".to_string()));
        
        assert_eq!(buffer.len(), 3);
        
        // Adding one more should remove the oldest
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "msg4".to_string()));
        
        assert_eq!(buffer.len(), 3);
        let entries = buffer.get_all();
        assert_eq!(entries[0].message, "msg4");
        assert_eq!(entries[2].message, "msg2");
    }

    #[test]
    fn test_log_buffer_reverse_order() {
        let mut buffer = LogBuffer::new(10);
        
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "first".to_string()));
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "second".to_string()));
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "third".to_string()));
        
        let entries = buffer.get_all();
        assert_eq!(entries[0].message, "third");
        assert_eq!(entries[1].message, "second");
        assert_eq!(entries[2].message, "first");
    }

    #[test]
    fn test_log_level_filtering() {
        let mut buffer = LogBuffer::new(10);
        
        buffer.push(LogEntry::new(Level::DEBUG, "test".to_string(), "debug msg".to_string()));
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "info msg".to_string()));
        buffer.push(LogEntry::new(Level::WARN, "test".to_string(), "warn msg".to_string()));
        buffer.push(LogEntry::new(Level::ERROR, "test".to_string(), "error msg".to_string()));
        
        let filtered = buffer.get_filtered(Some("WARN"), None);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|e| e.level == "WARN" || e.level == "ERROR"));
    }

    #[test]
    fn test_keyword_filtering() {
        let mut buffer = LogBuffer::new(10);
        
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "hello world".to_string()));
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "goodbye world".to_string()));
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "hello universe".to_string()));
        
        let filtered = buffer.get_filtered(None, Some("hello"));
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|e| e.message.contains("hello")));
    }

    #[test]
    fn test_combined_filtering() {
        let mut buffer = LogBuffer::new(10);
        
        buffer.push(LogEntry::new(Level::INFO, "test".to_string(), "info hello".to_string()));
        buffer.push(LogEntry::new(Level::WARN, "test".to_string(), "warn hello".to_string()));
        buffer.push(LogEntry::new(Level::ERROR, "test".to_string(), "error goodbye".to_string()));
        
        let filtered = buffer.get_filtered(Some("WARN"), Some("hello"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].message, "warn hello");
    }
}
