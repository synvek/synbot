//! Session persistence — atomic save/load of conversation history.
//!
//! Supports the new `SessionData` format (with `SessionMeta` + messages) as well
//! as backward-compatible loading of the legacy format (plain `Vec<SessionMessage>`).

use anyhow::{Context, Result};
use chrono::Utc;
use rig::message::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use tracing::{debug, warn};

use crate::agent::session_id::SessionId;
use crate::agent::session_manager::SessionMeta;

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

// ---------------------------------------------------------------------------
// SessionData — new persistence format
// ---------------------------------------------------------------------------

/// The new persistence format that bundles session metadata with messages.
///
/// When serialized to JSON it looks like:
/// ```json
/// {
///   "meta": { "id": "agent:main:telegram:dm:12345", ... },
///   "messages": [ { "role": "user", "content": "hello" }, ... ]
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionData {
    pub meta: SessionMeta,
    pub messages: Vec<SessionMessage>,
}

/// Persistent session store backed by JSON files on disk.
///
/// Sessions are stored in `{workspace}/sessions/` with one JSON file per
/// session key.  Writes are atomic (write to `.tmp`, then rename).
///
/// Sub-Role sessions are stored under `{workspace}/roles/{role_name}/sessions/`.
pub struct SessionStore {
    session_dir: PathBuf,
    workspace: PathBuf,
}

impl SessionStore {
    /// Create a new `SessionStore` rooted at `workspace/sessions/`.
    pub fn new(workspace: &Path) -> Self {
        Self {
            session_dir: workspace.join("sessions"),
            workspace: workspace.to_path_buf(),
        }
    }

    /// Get the workspace root path.
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    // ── helpers ──────────────────────────────────────────────────────

    /// Turn a session key into a safe filename (replace `:` with `_`).
    fn safe_filename(key: &str) -> String {
        key.replace(':', "_")
    }

    /// Determine the sessions directory for a given session key.
    ///
    /// If the key corresponds to a Sub-Role (i.e. the agent_id extracted from
    /// the SessionId is neither `"main"` nor unparseable), the directory is
    /// `workspace/roles/{role_name}/sessions/`. Otherwise it falls back to the
    /// default `workspace/sessions/`.
    fn sessions_dir_for_key(&self, key: &str) -> PathBuf {
        if let Ok(sid) = SessionId::parse(key) {
            if sid.agent_id != "main" {
                return self
                    .workspace
                    .join("roles")
                    .join(&sid.agent_id)
                    .join("sessions");
            }
        }
        self.session_dir.clone()
    }

    /// Full path for a session file.
    fn session_path(&self, key: &str) -> PathBuf {
        self.sessions_dir_for_key(key)
            .join(format!("{}.json", Self::safe_filename(key)))
    }

    /// Full path for the temporary file used during atomic writes.
    fn tmp_path(&self, key: &str) -> PathBuf {
        self.sessions_dir_for_key(key)
            .join(format!("{}.json.tmp", Self::safe_filename(key)))
    }

    /// Path to the `archived/` subdirectory.
    fn archive_dir(&self) -> PathBuf {
        self.session_dir.join("archived")
    }

    // ── public API ──────────────────────────────────────────────────

    /// Persist a single session to disk using atomic write.
    ///
    /// Uses the new `SessionData` format that includes metadata alongside
    /// messages.  The data is first written to a `.tmp` file and then renamed
    /// to the target path so that a crash mid-write never leaves a corrupt
    /// file.
    pub async fn save_session(
        &self,
        key: &str,
        messages: &[Message],
        meta: Option<&SessionMeta>,
    ) -> Result<()> {
        let dir = self.sessions_dir_for_key(key);
        fs::create_dir_all(&dir)
            .await
            .context("failed to create sessions directory")?;

        let session_messages: Vec<SessionMessage> =
            messages.iter().map(SessionMessage::from_message).collect();

        let json = if let Some(meta) = meta {
            let data = SessionData {
                meta: meta.clone(),
                messages: session_messages,
            };
            serde_json::to_string_pretty(&data)
                .context("failed to serialize session data")?
        } else {
            // Fallback: save as plain messages array (legacy format) when no
            // meta is provided.
            serde_json::to_string_pretty(&session_messages)
                .context("failed to serialize session")?
        };

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
    /// Supports both the new `SessionData` format and the legacy plain
    /// `Vec<SessionMessage>` format for backward compatibility.
    ///
    /// Returns `Ok(None)` if the file does not exist.
    pub async fn load_session(&self, key: &str) -> Result<Option<SessionData>> {
        let path = self.session_path(key);
        if !path.exists() {
            return Ok(None);
        }

        let data = fs::read_to_string(&path)
            .await
            .context("failed to read session file")?;

        let session_data = Self::parse_session_json(&data, key)
            .context("failed to deserialize session")?;

        Ok(Some(session_data))
    }

    /// Parse a JSON string into `SessionData`, supporting both the new format
    /// (object with `meta` + `messages`) and the legacy format (plain array of
    /// messages).
    fn parse_session_json(json_str: &str, key: &str) -> Result<SessionData> {
        // Try new format first (object with "meta" field)
        if let Ok(data) = serde_json::from_str::<SessionData>(json_str) {
            return Ok(data);
        }

        // Fall back to legacy format: plain Vec<SessionMessage>
        let messages: Vec<SessionMessage> = serde_json::from_str(json_str)
            .context("failed to parse as legacy session format")?;

        // Build a default SessionMeta from the key
        let id = SessionId::parse(key).unwrap_or_else(|_| {
            // If the key doesn't parse as a SessionId, create a simple one
            SessionId::simple("main", key)
        });

        let now = Utc::now();
        let meta = SessionMeta {
            id,
            participants: Vec::new(),
            created_at: now,
            updated_at: now,
        };

        Ok(SessionData { meta, messages })
    }

    /// Load every persisted session from the `sessions/` directory.
    ///
    /// Supports both the new `SessionData` format and the legacy format.
    pub async fn load_all_sessions(&self) -> Result<HashMap<String, SessionData>> {
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

            match fs::read_to_string(&path).await {
                Ok(data) => {
                    // Try new format first — it contains the authoritative key
                    // inside `meta.id`.
                    if let Ok(session_data) = serde_json::from_str::<SessionData>(&data) {
                        let key = session_data.meta.id.format();
                        sessions.insert(key, session_data);
                        continue;
                    }

                    // Legacy format: reverse the safe-filename encoding
                    // (`_` → `:`). Replace all underscores since
                    // `safe_filename` replaced all colons.
                    let session_key = stem.replace('_', ":");
                    match Self::parse_session_json(&data, &session_key) {
                        Ok(session_data) => {
                            sessions.insert(session_key, session_data);
                        }
                        Err(e) => {
                            warn!(path = %path.display(), error = %e, "skipping corrupt session file");
                        }
                    }
                }
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
    use crate::agent::session_id::SessionScope;
    use tempfile::TempDir;

    /// Helper: create a SessionStore backed by a temporary directory.
    fn temp_store() -> (TempDir, SessionStore) {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::new(dir.path());
        (dir, store)
    }

    /// Helper: build a SessionMeta for testing.
    fn test_meta(key: &str) -> SessionMeta {
        let id = SessionId::parse(key)
            .unwrap_or_else(|_| SessionId::simple("main", key));
        let now = Utc::now();
        SessionMeta {
            id,
            participants: vec!["user:12345".into(), "agent:main".into()],
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn save_and_load_roundtrip() {
        let (_dir, store) = temp_store();

        let messages = vec![
            Message::user("hello"),
            Message::assistant("hi there!"),
        ];
        let meta = test_meta("agent:main:telegram");

        store
            .save_session("agent:main:telegram", &messages, Some(&meta))
            .await
            .unwrap();

        let loaded = store.load_session("agent:main:telegram").await.unwrap();
        assert!(loaded.is_some());

        let loaded = loaded.unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.meta.id, meta.id);
        assert_eq!(loaded.meta.participants, meta.participants);

        // Verify content via SessionMessage round-trip
        let original: Vec<SessionMessage> =
            messages.iter().map(SessionMessage::from_message).collect();
        assert_eq!(original, loaded.messages);
    }

    #[tokio::test]
    async fn load_nonexistent_returns_none() {
        let (_dir, store) = temp_store();
        let result = store.load_session("agent:main:nokey").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn load_all_sessions_returns_all() {
        let (_dir, store) = temp_store();

        let msgs_a = vec![Message::user("a")];
        let msgs_b = vec![Message::user("b"), Message::assistant("reply")];
        let meta_a = test_meta("agent:main:ch1");
        let meta_b = test_meta("agent:main:ch2");

        store
            .save_session("agent:main:ch1", &msgs_a, Some(&meta_a))
            .await
            .unwrap();
        store
            .save_session("agent:main:ch2", &msgs_b, Some(&meta_b))
            .await
            .unwrap();

        let all = store.load_all_sessions().await.unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.contains_key("agent:main:ch1"));
        assert!(all.contains_key("agent:main:ch2"));

        // Verify content
        assert_eq!(all["agent:main:ch1"].messages.len(), 1);
        assert_eq!(all["agent:main:ch1"].messages[0].content, "a");

        assert_eq!(all["agent:main:ch2"].messages.len(), 2);
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

        let messages = vec![Message::user("old message")];
        store
            .save_session("agent:main:old", &messages, None)
            .await
            .unwrap();

        let archived = store
            .archive_inactive(Duration::from_secs(0))
            .await
            .unwrap();

        assert_eq!(archived, 1);

        // The session should no longer be loadable from the active directory
        let loaded = store.load_session("agent:main:old").await.unwrap();
        assert!(loaded.is_none());

        // The file should exist in the archived directory
        let archive_path = store
            .archive_dir()
            .join("agent_main_old.json");
        assert!(archive_path.exists());
    }

    #[tokio::test]
    async fn archive_inactive_keeps_recent_sessions() {
        let (_dir, store) = temp_store();

        let messages = vec![Message::user("recent")];
        store
            .save_session("agent:main:recent", &messages, None)
            .await
            .unwrap();

        // Use a very large threshold — nothing should be archived
        let archived = store
            .archive_inactive(Duration::from_secs(86400 * 365))
            .await
            .unwrap();

        assert_eq!(archived, 0);

        let loaded = store.load_session("agent:main:recent").await.unwrap();
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn atomic_write_no_tmp_left_behind() {
        let (_dir, store) = temp_store();

        let messages = vec![Message::user("test")];
        store
            .save_session("agent:main:key1", &messages, None)
            .await
            .unwrap();

        // The .tmp file should not exist after a successful save
        let tmp = store.tmp_path("agent:main:key1");
        assert!(!tmp.exists());

        // The target file should exist
        let target = store.session_path("agent:main:key1");
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
        store
            .save_session("agent:main:keyx", &v1, None)
            .await
            .unwrap();

        let v2 = vec![Message::user("version 2"), Message::assistant("updated")];
        store
            .save_session("agent:main:keyx", &v2, None)
            .await
            .unwrap();

        let loaded = store.load_session("agent:main:keyx").await.unwrap().unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].content, "version 2");
        assert_eq!(loaded.messages[1].content, "updated");
    }

    // ── New format tests ────────────────────────────────────────────

    #[tokio::test]
    async fn save_with_meta_and_load_preserves_metadata() {
        let (_dir, store) = temp_store();

        let messages = vec![Message::user("hello"), Message::assistant("hi")];
        let meta = SessionMeta {
            id: SessionId::full("main", "telegram", SessionScope::Dm, "12345"),
            participants: vec!["user:12345".into(), "agent:main".into()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        store
            .save_session(
                "agent:main:telegram:dm:12345",
                &messages,
                Some(&meta),
            )
            .await
            .unwrap();

        let loaded = store
            .load_session("agent:main:telegram:dm:12345")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(loaded.meta.id, meta.id);
        assert_eq!(loaded.meta.participants, meta.participants);
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].role, "user");
        assert_eq!(loaded.messages[0].content, "hello");
    }

    #[tokio::test]
    async fn backward_compat_loads_legacy_format() {
        let (_dir, store) = temp_store();

        // Manually write a legacy-format file (plain array of messages)
        let legacy_messages = vec![
            SessionMessage {
                role: "user".into(),
                content: "old hello".into(),
            },
            SessionMessage {
                role: "assistant".into(),
                content: "old reply".into(),
            },
        ];
        let json = serde_json::to_string_pretty(&legacy_messages).unwrap();

        let dir = store.sessions_dir_for_key("agent:main:legacy");
        fs::create_dir_all(&dir).await.unwrap();
        let path = store.session_path("agent:main:legacy");
        fs::write(&path, &json).await.unwrap();

        // Load should succeed and wrap in SessionData with default meta
        let loaded = store
            .load_session("agent:main:legacy")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].content, "old hello");
        assert_eq!(loaded.messages[1].content, "old reply");
        // Meta should have a default SessionId derived from the key
        assert_eq!(loaded.meta.id.agent_id, "main");
        assert_eq!(loaded.meta.id.channel, "legacy");
        assert!(loaded.meta.participants.is_empty());
    }

    #[tokio::test]
    async fn subrole_session_stored_in_role_directory() {
        let (_dir, store) = temp_store();

        let messages = vec![Message::user("design request")];
        let meta = SessionMeta {
            id: SessionId::full(
                "ui_designer",
                "telegram",
                SessionScope::Dm,
                "user_1",
            ),
            participants: vec!["user:user_1".into(), "agent:ui_designer".into()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };

        let key = "agent:ui_designer:telegram:dm:user_1";
        store
            .save_session(key, &messages, Some(&meta))
            .await
            .unwrap();

        // Verify the file is stored under roles/ui_designer/sessions/
        let expected_dir = store
            .workspace
            .join("roles")
            .join("ui_designer")
            .join("sessions");
        assert!(expected_dir.exists());

        let expected_file = expected_dir.join(format!(
            "{}.json",
            SessionStore::safe_filename(key)
        ));
        assert!(expected_file.exists());

        // Load should still work
        let loaded = store.load_session(key).await.unwrap().unwrap();
        assert_eq!(loaded.meta.id.agent_id, "ui_designer");
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0].content, "design request");
    }

    #[tokio::test]
    async fn main_agent_session_stored_in_default_directory() {
        let (_dir, store) = temp_store();

        let messages = vec![Message::user("hello")];
        let key = "agent:main:telegram:dm:user_1";
        store.save_session(key, &messages, None).await.unwrap();

        // Verify the file is stored under sessions/ (not roles/)
        let expected_file = store.session_dir.join(format!(
            "{}.json",
            SessionStore::safe_filename(key)
        ));
        assert!(expected_file.exists());

        // roles/ directory should NOT exist
        let roles_dir = store.workspace.join("roles");
        assert!(!roles_dir.exists());
    }

    #[tokio::test]
    async fn load_all_sessions_handles_mixed_formats() {
        let (_dir, store) = temp_store();

        // Save one session in new format
        let messages_new = vec![Message::user("new format")];
        let meta = test_meta("agent:main:new");
        store
            .save_session("agent:main:new", &messages_new, Some(&meta))
            .await
            .unwrap();

        // Manually write one in legacy format
        let legacy = vec![SessionMessage {
            role: "user".into(),
            content: "legacy format".into(),
        }];
        let json = serde_json::to_string_pretty(&legacy).unwrap();
        let path = store.session_path("agent:main:old");
        fs::write(&path, &json).await.unwrap();

        let all = store.load_all_sessions().await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all["agent:main:new"].messages[0].content, "new format");
        assert_eq!(all["agent:main:old"].messages[0].content, "legacy format");
    }

    #[tokio::test]
    async fn session_data_serde_roundtrip() {
        let meta = SessionMeta {
            id: SessionId::full("main", "telegram", SessionScope::Dm, "12345"),
            participants: vec!["user:12345".into(), "agent:main".into()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let data = SessionData {
            meta,
            messages: vec![
                SessionMessage {
                    role: "user".into(),
                    content: "hello".into(),
                },
                SessionMessage {
                    role: "assistant".into(),
                    content: "hi there!".into(),
                },
            ],
        };

        let json = serde_json::to_string_pretty(&data).unwrap();
        let deserialized: SessionData = serde_json::from_str(&json).unwrap();
        assert_eq!(data, deserialized);
    }
}
