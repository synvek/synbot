//! Session persistence — atomic save/load of conversation history.

use anyhow::{Context, Result};
use rig::message::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use tracing::{debug, warn};

/// A serializable representation of a chat message.
///
/// We use this wrapper instead of `rig::message::Message` directly because
/// the upstream type may not implement Serialize/Deserialize.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
}

impl SessionMessage {
    /// Convert a `rig::message::Message` into a `SessionMessage`.
    pub fn from_message(msg: &Message) -> Self {
        match msg {
            Message::User { content } => {
                let text = content
                    .iter()
                    .filter_map(|c| {
                        if let rig::message::UserContent::Text(t) = c {
                            Some(t.text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");
                SessionMessage {
                    role: "user".to_string(),
                    content: text,
                }
            }
            Message::Assistant { content } => {
                let text = content
                    .iter()
                    .filter_map(|c| {
                        if let rig::message::AssistantContent::Text(t) = c {
                            Some(t.text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");
                SessionMessage {
                    role: "assistant".to_string(),
                    content: text,
                }
            }
        }
    }

    /// Convert back to a `rig::message::Message`.
    pub fn to_message(&self) -> Message {
        match self.role.as_str() {
            "assistant" => Message::assistant(&self.content),
            _ => Message::user(&self.content),
        }
    }
}

/// Persistent session store backed by JSON files on disk.
///
/// Sessions are stored in `{workspace}/sessions/` with one JSON file per
/// session key.  Writes are atomic (write to `.tmp`, then rename).
pub struct SessionStore {
    session_dir: PathBuf,
}

impl SessionStore {
    /// Create a new `SessionStore` rooted at `workspace/sessions/`.
    pub fn new(workspace: &Path) -> Self {
        Self {
            session_dir: workspace.join("sessions"),
        }
    }

    // ── helpers ──────────────────────────────────────────────────────

    /// Turn a session key into a safe filename (replace `:` with `_`).
    fn safe_filename(key: &str) -> String {
        key.replace(':', "_")
    }

    /// Full path for a session file.
    fn session_path(&self, key: &str) -> PathBuf {
        self.session_dir
            .join(format!("{}.json", Self::safe_filename(key)))
    }

    /// Full path for the temporary file used during atomic writes.
    fn tmp_path(&self, key: &str) -> PathBuf {
        self.session_dir
            .join(format!("{}.json.tmp", Self::safe_filename(key)))
    }

    /// Path to the `archived/` subdirectory.
    fn archive_dir(&self) -> PathBuf {
        self.session_dir.join("archived")
    }

    // ── public API ──────────────────────────────────────────────────

    /// Persist a single session to disk using atomic write.
    ///
    /// The data is first written to a `.tmp` file and then renamed to the
    /// target path so that a crash mid-write never leaves a corrupt file.
    pub async fn save_session(&self, key: &str, messages: &[Message]) -> Result<()> {
        fs::create_dir_all(&self.session_dir)
            .await
            .context("failed to create sessions directory")?;

        let session_messages: Vec<SessionMessage> =
            messages.iter().map(SessionMessage::from_message).collect();

        let json = serde_json::to_string_pretty(&session_messages)
            .context("failed to serialize session")?;

        let tmp = self.tmp_path(key);
        let target = self.session_path(key);

        fs::write(&tmp, &json)
            .await
            .context("failed to write tmp session file")?;

        fs::rename(&tmp, &target)
            .await
            .context("failed to rename tmp to target")?;

        debug!(session_key = %key, "session saved");
        Ok(())
    }

    /// Load a single session from disk.
    ///
    /// Returns `Ok(None)` if the file does not exist.
    pub async fn load_session(&self, key: &str) -> Result<Option<Vec<Message>>> {
        let path = self.session_path(key);
        if !path.exists() {
            return Ok(None);
        }

        let data = fs::read_to_string(&path)
            .await
            .context("failed to read session file")?;

        let session_messages: Vec<SessionMessage> =
            serde_json::from_str(&data).context("failed to deserialize session")?;

        let messages = session_messages.iter().map(SessionMessage::to_message).collect();
        Ok(Some(messages))
    }

    /// Load every persisted session from the `sessions/` directory.
    pub async fn load_all_sessions(&self) -> Result<HashMap<String, Vec<Message>>> {
        let mut sessions = HashMap::new();

        if !self.session_dir.exists() {
            return Ok(sessions);
        }

        let mut entries = fs::read_dir(&self.session_dir)
            .await
            .context("failed to read sessions directory")?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Only process .json files (skip .tmp, archived/, etc.)
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            // Skip directories
            if path.is_dir() {
                continue;
            }

            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };

            // Reverse the safe-filename encoding: `_` back to `:`
            let session_key = stem.replacen('_', ":", 1);

            match fs::read_to_string(&path).await {
                Ok(data) => match serde_json::from_str::<Vec<SessionMessage>>(&data) {
                    Ok(msgs) => {
                        let messages = msgs.iter().map(SessionMessage::to_message).collect();
                        sessions.insert(session_key, messages);
                    }
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "skipping corrupt session file");
                    }
                },
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "failed to read session file");
                }
            }
        }

        Ok(sessions)
    }

    /// Archive sessions whose file has not been modified for longer than
    /// `max_inactive`.
    ///
    /// Archived files are moved into the `sessions/archived/` subdirectory.
    /// Returns the number of sessions archived.
    pub async fn archive_inactive(&self, max_inactive: Duration) -> Result<u32> {
        if !self.session_dir.exists() {
            return Ok(0);
        }

        let archive = self.archive_dir();
        let mut archived_count = 0u32;

        let mut entries = fs::read_dir(&self.session_dir)
            .await
            .context("failed to read sessions directory")?;

        let now = std::time::SystemTime::now();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Only process .json files
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if path.is_dir() {
                continue;
            }

            let metadata = match fs::metadata(&path).await {
                Ok(m) => m,
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "failed to read metadata");
                    continue;
                }
            };

            let modified = match metadata.modified() {
                Ok(t) => t,
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "failed to get modified time");
                    continue;
                }
            };

            let inactive_duration = match now.duration_since(modified) {
                Ok(d) => d,
                Err(_) => continue, // modified time in the future — skip
            };

            if inactive_duration > max_inactive {
                // Ensure archive directory exists
                fs::create_dir_all(&archive)
                    .await
                    .context("failed to create archive directory")?;

                let filename = match path.file_name() {
                    Some(f) => f.to_owned(),
                    None => continue,
                };
                let dest = archive.join(filename);

                fs::rename(&path, &dest)
                    .await
                    .with_context(|| format!("failed to archive {}", path.display()))?;

                archived_count += 1;
                debug!(path = %path.display(), "session archived");
            }
        }

        Ok(archived_count)
    }
}

// ── tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create a SessionStore backed by a temporary directory.
    fn temp_store() -> (TempDir, SessionStore) {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::new(dir.path());
        (dir, store)
    }

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let (_dir, store) = temp_store();

        let messages = vec![
            Message::user("hello"),
            Message::assistant("hi there!"),
        ];

        store.save_session("telegram:123", &messages).await.unwrap();

        let loaded = store.load_session("telegram:123").await.unwrap();
        assert!(loaded.is_some());

        let loaded = loaded.unwrap();
        assert_eq!(loaded.len(), 2);

        // Verify content via SessionMessage round-trip
        let original: Vec<SessionMessage> = messages.iter().map(SessionMessage::from_message).collect();
        let restored: Vec<SessionMessage> = loaded.iter().map(SessionMessage::from_message).collect();
        assert_eq!(original, restored);
    }

    #[tokio::test]
    async fn load_nonexistent_returns_none() {
        let (_dir, store) = temp_store();
        let result = store.load_session("no:such:key").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn load_all_sessions_returns_all() {
        let (_dir, store) = temp_store();

        let msgs_a = vec![Message::user("a")];
        let msgs_b = vec![Message::user("b"), Message::assistant("reply")];

        store.save_session("ch:1", &msgs_a).await.unwrap();
        store.save_session("ch:2", &msgs_b).await.unwrap();

        let all = store.load_all_sessions().await.unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("ch:1"));
        assert!(all.contains_key("ch:2"));

        // Verify content
        let a_msgs: Vec<SessionMessage> = all["ch:1"].iter().map(SessionMessage::from_message).collect();
        assert_eq!(a_msgs.len(), 1);
        assert_eq!(a_msgs[0].content, "a");

        let b_msgs: Vec<SessionMessage> = all["ch:2"].iter().map(SessionMessage::from_message).collect();
        assert_eq!(b_msgs.len(), 2);
    }

    #[tokio::test]
    async fn load_all_sessions_empty_dir() {
        let (_dir, store) = temp_store();
        let all = store.load_all_sessions().await.unwrap();
        assert!(all.is_empty());
    }

    #[tokio::test]
    async fn archive_inactive_moves_old_sessions() {
        let (_dir, store) = temp_store();

        // Save a session
        let messages = vec![Message::user("old message")];
        store.save_session("old:session", &messages).await.unwrap();

        // Set the file's modified time to the past by using filetime
        // Since we don't have the filetime crate, we'll use a zero-duration
        // threshold to archive everything.
        let archived = store
            .archive_inactive(Duration::from_secs(0))
            .await
            .unwrap();

        assert_eq!(archived, 1);

        // The session should no longer be loadable from the active directory
        let loaded = store.load_session("old:session").await.unwrap();
        assert!(loaded.is_none());

        // The file should exist in the archived directory
        let archive_path = store.archive_dir().join("old_session.json");
        assert!(archive_path.exists());
    }

    #[tokio::test]
    async fn archive_inactive_keeps_recent_sessions() {
        let (_dir, store) = temp_store();

        let messages = vec![Message::user("recent")];
        store.save_session("recent:session", &messages).await.unwrap();

        // Use a very large threshold — nothing should be archived
        let archived = store
            .archive_inactive(Duration::from_secs(86400 * 365))
            .await
            .unwrap();

        assert_eq!(archived, 0);

        // Session should still be loadable
        let loaded = store.load_session("recent:session").await.unwrap();
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn atomic_write_no_tmp_left_behind() {
        let (_dir, store) = temp_store();

        let messages = vec![Message::user("test")];
        store.save_session("key:1", &messages).await.unwrap();

        // The .tmp file should not exist after a successful save
        let tmp = store.tmp_path("key:1");
        assert!(!tmp.exists());

        // The target file should exist
        let target = store.session_path("key:1");
        assert!(target.exists());
    }

    #[tokio::test]
    async fn session_message_conversion_roundtrip() {
        let user_msg = Message::user("hello world");
        let sm = SessionMessage::from_message(&user_msg);
        assert_eq!(sm.role, "user");
        assert_eq!(sm.content, "hello world");

        let back = sm.to_message();
        let sm2 = SessionMessage::from_message(&back);
        assert_eq!(sm, sm2);

        let asst_msg = Message::assistant("I can help");
        let sm = SessionMessage::from_message(&asst_msg);
        assert_eq!(sm.role, "assistant");
        assert_eq!(sm.content, "I can help");

        let back = sm.to_message();
        let sm2 = SessionMessage::from_message(&back);
        assert_eq!(sm, sm2);
    }

    #[tokio::test]
    async fn save_overwrites_existing_session() {
        let (_dir, store) = temp_store();

        let v1 = vec![Message::user("version 1")];
        store.save_session("key:x", &v1).await.unwrap();

        let v2 = vec![Message::user("version 2"), Message::assistant("updated")];
        store.save_session("key:x", &v2).await.unwrap();

        let loaded = store.load_session("key:x").await.unwrap().unwrap();
        let msgs: Vec<SessionMessage> = loaded.iter().map(SessionMessage::from_message).collect();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].content, "version 2");
        assert_eq!(msgs[1].content, "updated");
    }
}
