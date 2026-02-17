use crate::tools::approval::{ApprovalRequest, ApprovalResponse, ApprovalStatus};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};

/// Approval history entry combining request and response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalHistoryEntry {
    pub request: ApprovalRequest,
    pub response: Option<ApprovalResponse>,
    pub status: ApprovalStatusRecord,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Serializable approval status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApprovalStatusRecord {
    Pending,
    Approved { responder: String, timestamp: DateTime<Utc> },
    Rejected { responder: String, timestamp: DateTime<Utc> },
    Timeout,
}

impl From<&ApprovalStatus> for ApprovalStatusRecord {
    fn from(status: &ApprovalStatus) -> Self {
        match status {
            ApprovalStatus::Pending => ApprovalStatusRecord::Pending,
            ApprovalStatus::Approved(response) => ApprovalStatusRecord::Approved {
                responder: response.responder.clone(),
                timestamp: response.timestamp,
            },
            ApprovalStatus::Rejected(response) => ApprovalStatusRecord::Rejected {
                responder: response.responder.clone(),
                timestamp: response.timestamp,
            },
            ApprovalStatus::Timeout => ApprovalStatusRecord::Timeout,
        }
    }
}

/// Approval history store for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalStore {
    /// Map of request_id to history entry
    entries: HashMap<String, ApprovalHistoryEntry>,
    /// Path to the storage file
    #[serde(skip)]
    file_path: PathBuf,
}

impl ApprovalStore {
    /// Create a new approval store with the given file path
    pub fn new<P: AsRef<Path>>(file_path: P) -> Self {
        Self {
            entries: HashMap::new(),
            file_path: file_path.as_ref().to_path_buf(),
        }
    }

    /// Add or update an approval history entry
    pub fn add_entry(&mut self, entry: ApprovalHistoryEntry) {
        self.entries.insert(entry.request.id.clone(), entry);
    }

    /// Get an approval history entry by request ID
    pub fn get_entry(&self, request_id: &str) -> Option<&ApprovalHistoryEntry> {
        self.entries.get(request_id)
    }

    /// Get all approval history entries
    pub fn get_all_entries(&self) -> Vec<&ApprovalHistoryEntry> {
        self.entries.values().collect()
    }

    /// Query approval history with filters
    pub fn query(
        &self,
        session_id: Option<&str>,
        channel: Option<&str>,
        status: Option<&ApprovalStatusRecord>,
    ) -> Vec<&ApprovalHistoryEntry> {
        self.entries
            .values()
            .filter(|entry| {
                if let Some(sid) = session_id {
                    if entry.request.session_id != sid {
                        return false;
                    }
                }
                if let Some(ch) = channel {
                    if entry.request.channel != ch {
                        return false;
                    }
                }
                if let Some(st) = status {
                    if !matches_status(&entry.status, st) {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    /// Save approval history to file
    pub fn save(&self) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create approval store directory")?;
        }

        let json = serde_json::to_string_pretty(&self.entries)
            .context("Failed to serialize approval history")?;
        
        fs::write(&self.file_path, json)
            .context("Failed to write approval history to file")?;

        Ok(())
    }

    /// Load approval history from file
    pub fn load<P: AsRef<Path>>(file_path: P) -> Result<Self> {
        let file_path = file_path.as_ref();
        
        if !file_path.exists() {
            return Ok(Self::new(file_path));
        }

        let json = fs::read_to_string(file_path)
            .context("Failed to read approval history file")?;
        
        let entries: HashMap<String, ApprovalHistoryEntry> = serde_json::from_str(&json)
            .context("Failed to deserialize approval history")?;

        Ok(Self {
            entries,
            file_path: file_path.to_path_buf(),
        })
    }

    /// Export approval history to JSON string
    pub fn export(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.entries)
            .context("Failed to export approval history")
    }

    /// Get the number of entries in the store
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries from the store
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Helper function to match approval status
fn matches_status(status: &ApprovalStatusRecord, filter: &ApprovalStatusRecord) -> bool {
    match (status, filter) {
        (ApprovalStatusRecord::Pending, ApprovalStatusRecord::Pending) => true,
        (ApprovalStatusRecord::Approved { .. }, ApprovalStatusRecord::Approved { .. }) => true,
        (ApprovalStatusRecord::Rejected { .. }, ApprovalStatusRecord::Rejected { .. }) => true,
        (ApprovalStatusRecord::Timeout, ApprovalStatusRecord::Timeout) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_request(id: &str, session_id: &str, channel: &str) -> ApprovalRequest {
        ApprovalRequest {
            id: id.to_string(),
            session_id: session_id.to_string(),
            channel: channel.to_string(),
            chat_id: "test-chat".to_string(),
            command: "test command".to_string(),
            working_dir: "/tmp".to_string(),
            context: "test context".to_string(),
            timestamp: Utc::now(),
            timeout_secs: 300,
            display_message: None,
        }
    }

    fn create_test_entry(id: &str, session_id: &str, channel: &str) -> ApprovalHistoryEntry {
        ApprovalHistoryEntry {
            request: create_test_request(id, session_id, channel),
            response: None,
            status: ApprovalStatusRecord::Pending,
            completed_at: None,
        }
    }

    #[test]
    fn test_new_store() {
        let store = ApprovalStore::new("/tmp/test.json");
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_add_and_get_entry() {
        let mut store = ApprovalStore::new("/tmp/test.json");
        let entry = create_test_entry("req-1", "session-1", "telegram");
        
        store.add_entry(entry.clone());
        
        assert_eq!(store.len(), 1);
        assert!(store.get_entry("req-1").is_some());
        assert!(store.get_entry("req-2").is_none());
    }

    #[test]
    fn test_get_all_entries() {
        let mut store = ApprovalStore::new("/tmp/test.json");
        store.add_entry(create_test_entry("req-1", "session-1", "telegram"));
        store.add_entry(create_test_entry("req-2", "session-2", "discord"));
        
        let all = store.get_all_entries();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_query_by_session() {
        let mut store = ApprovalStore::new("/tmp/test.json");
        store.add_entry(create_test_entry("req-1", "session-1", "telegram"));
        store.add_entry(create_test_entry("req-2", "session-2", "discord"));
        store.add_entry(create_test_entry("req-3", "session-1", "feishu"));
        
        let results = store.query(Some("session-1"), None, None);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_by_channel() {
        let mut store = ApprovalStore::new("/tmp/test.json");
        store.add_entry(create_test_entry("req-1", "session-1", "telegram"));
        store.add_entry(create_test_entry("req-2", "session-2", "discord"));
        store.add_entry(create_test_entry("req-3", "session-3", "telegram"));
        
        let results = store.query(None, Some("telegram"), None);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_by_status() {
        let mut store = ApprovalStore::new("/tmp/test.json");
        
        let mut entry1 = create_test_entry("req-1", "session-1", "telegram");
        entry1.status = ApprovalStatusRecord::Approved {
            responder: "user1".to_string(),
            timestamp: Utc::now(),
        };
        
        let entry2 = create_test_entry("req-2", "session-2", "discord");
        
        store.add_entry(entry1);
        store.add_entry(entry2);
        
        let results = store.query(None, None, Some(&ApprovalStatusRecord::Pending));
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("approvals.json");
        
        // Create and save store
        let mut store = ApprovalStore::new(&file_path);
        store.add_entry(create_test_entry("req-1", "session-1", "telegram"));
        store.add_entry(create_test_entry("req-2", "session-2", "discord"));
        
        store.save().unwrap();
        
        // Load store
        let loaded_store = ApprovalStore::load(&file_path).unwrap();
        assert_eq!(loaded_store.len(), 2);
        assert!(loaded_store.get_entry("req-1").is_some());
        assert!(loaded_store.get_entry("req-2").is_some());
    }

    #[test]
    fn test_load_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent.json");
        
        let store = ApprovalStore::load(&file_path).unwrap();
        assert!(store.is_empty());
    }

    #[test]
    fn test_export() {
        let mut store = ApprovalStore::new("/tmp/test.json");
        store.add_entry(create_test_entry("req-1", "session-1", "telegram"));
        
        let json = store.export().unwrap();
        assert!(json.contains("req-1"));
        assert!(json.contains("session-1"));
        assert!(json.contains("telegram"));
    }

    #[test]
    fn test_clear() {
        let mut store = ApprovalStore::new("/tmp/test.json");
        store.add_entry(create_test_entry("req-1", "session-1", "telegram"));
        store.add_entry(create_test_entry("req-2", "session-2", "discord"));
        
        assert_eq!(store.len(), 2);
        
        store.clear();
        assert!(store.is_empty());
    }

    #[test]
    fn test_approval_status_conversion() {
        let response = ApprovalResponse {
            request_id: "req-1".to_string(),
            approved: true,
            responder: "user1".to_string(),
            timestamp: Utc::now(),
        };
        
        let status = ApprovalStatus::Approved(response.clone());
        let record = ApprovalStatusRecord::from(&status);
        
        match record {
            ApprovalStatusRecord::Approved { responder, .. } => {
                assert_eq!(responder, "user1");
            }
            _ => panic!("Expected Approved status"),
        }
    }
}
