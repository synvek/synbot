//! Session manager — creation, lookup, routing, and lifecycle of conversation sessions.
//!
//! [`SessionManager`] resolves which session a message belongs to and manages
//! in-memory session state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::agent::session::SessionMessage;
use crate::agent::session_id::{SessionId, SessionScope};

// ---------------------------------------------------------------------------
// Session metadata
// ---------------------------------------------------------------------------

/// Metadata associated with a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMeta {
    /// The unique session identifier.
    pub id: SessionId,
    /// List of participant identifiers (e.g. `"telegram"`, `"telegram:12345"`).
    pub participants: Vec<String>,
    /// When the session was first created.
    pub created_at: DateTime<Utc>,
    /// When the session was last updated (message appended).
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// SessionManager
// ---------------------------------------------------------------------------

/// Manages in-memory sessions and resolves session routing.
pub struct SessionManager {
    /// Active sessions keyed by `SessionId`.
    sessions: HashMap<SessionId, (SessionMeta, Vec<SessionMessage>)>,
}

impl SessionManager {
    /// Create a new `SessionManager`.
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    // ── Session resolution ──────────────────────────────────────────

    /// Determine the [`SessionId`] for an incoming message based on the
    /// agent, channel, chat_id, and optional metadata.
    ///
    /// If metadata contains a truthy `"group"` key (e.g. from channel allowlist),
    /// uses `Group` scope; otherwise `Dm` scope. Identifier is always `chat_id`.
    pub fn resolve_session(
        &self,
        agent_id: &str,
        channel: &str,
        chat_id: &str,
        metadata: &serde_json::Value,
    ) -> SessionId {
        let is_group = metadata
            .get("group")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let scope = if is_group {
            SessionScope::Group
        } else {
            SessionScope::Dm
        };
        SessionId::full(agent_id, channel, scope, chat_id)
    }

    // ── Session CRUD ────────────────────────────────────────────────

    /// Get or create a session, returning a mutable reference to its message
    /// history.
    ///
    /// If the session does not exist yet, a new empty session is created with
    /// the current timestamp.
    pub fn get_or_create(&mut self, id: &SessionId) -> &mut Vec<SessionMessage> {
        let entry = self.sessions.entry(id.clone()).or_insert_with(|| {
            let now = Utc::now();
            let meta = SessionMeta {
                id: id.clone(),
                participants: Vec::new(),
                created_at: now,
                updated_at: now,
            };
            (meta, Vec::new())
        });
        &mut entry.1
    }

    /// Get read-only access to a session's message history.
    ///
    /// Returns `None` if the session does not exist.
    pub fn get_history(&self, id: &SessionId) -> Option<&Vec<SessionMessage>> {
        self.sessions.get(id).map(|(_, msgs)| msgs)
    }

    /// Get all sessions for a given channel and chat/group identifier (e.g. web + user_id).
    /// Used to show a unified channel view: one timeline with all roles (main, dev, …) for that channel.
    /// Returns sessions sorted by agent_id (main first, then alphabetically) so the order is stable.
    pub fn get_sessions_for_channel(
        &self,
        channel: &str,
        scope: SessionScope,
        identifier: &str,
    ) -> Vec<(SessionMeta, Vec<SessionMessage>)> {
        let mut out: Vec<_> = self
            .sessions
            .iter()
            .filter(|(id, _)| {
                id.channel == channel
                    && id.scope.as_ref() == Some(&scope)
                    && id.identifier.as_deref() == Some(identifier)
            })
            .map(|(_, (meta, msgs))| (meta.clone(), msgs.clone()))
            .collect();
        out.sort_by(|a, b| {
            let a_main = a.0.id.agent_id == "main";
            let b_main = b.0.id.agent_id == "main";
            match (a_main, b_main) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.0.id.agent_id.cmp(&b.0.id.agent_id),
            }
        });
        out
    }

    /// Append a message to a session. Creates the session if it does not
    /// exist.
    pub fn append(&mut self, id: &SessionId, message: SessionMessage) {
        let entry = self.sessions.entry(id.clone()).or_insert_with(|| {
            let now = Utc::now();
            let meta = SessionMeta {
                id: id.clone(),
                participants: Vec::new(),
                created_at: now,
                updated_at: now,
            };
            (meta, Vec::new())
        });
        entry.0.updated_at = Utc::now();
        entry.1.push(message);
    }

    // ── Accessors ───────────────────────────────────────────────────

    /// Get a reference to the session metadata, if the session exists.
    pub fn get_meta(&self, id: &SessionId) -> Option<&SessionMeta> {
        self.sessions.get(id).map(|(meta, _)| meta)
    }

    /// Get the number of active sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Get all sessions as a vector of (SessionMeta, message_count) tuples.
    pub fn get_all_sessions(&self) -> Vec<(SessionMeta, usize)> {
        self.sessions
            .values()
            .map(|(meta, messages)| (meta.clone(), messages.len()))
            .collect()
    }
}

// ── tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manager() -> SessionManager {
        SessionManager::new()
    }

    // ── resolve_session ─────────────────────────────────────────────

    #[test]
    fn resolve_session_dm_by_default() {
        let mgr = manager();
        let sid = mgr.resolve_session("main", "telegram", "user_999", &json!({}));
        assert_eq!(sid.agent_id, "main");
        assert_eq!(sid.channel, "telegram");
        assert_eq!(sid.scope, Some(SessionScope::Dm));
        assert_eq!(sid.identifier, Some("user_999".into()));
    }

    #[test]
    fn resolve_session_group_when_metadata_group_true() {
        let mgr = manager();
        let metadata = json!({ "group": true });
        let sid = mgr.resolve_session("main", "telegram", "group_123", &metadata);
        assert_eq!(sid.scope, Some(SessionScope::Group));
        assert_eq!(sid.identifier, Some("group_123".into()));
    }

    #[test]
    fn resolve_session_dm_when_metadata_group_false() {
        let mgr = manager();
        let metadata = json!({ "group": false });
        let sid = mgr.resolve_session("main", "telegram", "user_1", &metadata);
        assert_eq!(sid.scope, Some(SessionScope::Dm));
        assert_eq!(sid.identifier, Some("user_1".into()));
    }

    #[test]
    fn resolve_session_uses_agent_id() {
        let mgr = manager();
        let sid = mgr.resolve_session("ui_designer", "discord", "user_1", &json!({}));
        assert_eq!(sid.agent_id, "ui_designer");
        assert_eq!(sid.channel, "discord");
        assert_eq!(sid.scope, Some(SessionScope::Dm));
        assert_eq!(sid.identifier, Some("user_1".into()));
    }

    // ── get_or_create ───────────────────────────────────────────────

    #[test]
    fn get_or_create_creates_new_session() {
        let mut mgr = manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        let history = mgr.get_or_create(&sid);
        assert!(history.is_empty());
        assert_eq!(mgr.session_count(), 1);
    }

    #[test]
    fn get_or_create_returns_existing_session() {
        let mut mgr = manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");

        // Create and add a message
        mgr.get_or_create(&sid).push(SessionMessage {
            role: "user".into(),
            content: "hello".into(),
            timestamp: Utc::now(),
        });

        // Get again — should return the same session with the message
        let history = mgr.get_or_create(&sid);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "hello");
        assert_eq!(mgr.session_count(), 1);
    }

    // ── get_history ─────────────────────────────────────────────────

    #[test]
    fn get_history_returns_none_for_nonexistent() {
        let mgr = manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        assert!(mgr.get_history(&sid).is_none());
    }

    #[test]
    fn get_history_returns_messages() {
        let mut mgr = manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        mgr.append(
            &sid,
            SessionMessage {
                role: "user".into(),
                content: "hi".into(),
                timestamp: Utc::now(),
            },
        );
        let history = mgr.get_history(&sid).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "hi");
    }

    // ── append ──────────────────────────────────────────────────────

    #[test]
    fn append_creates_session_if_needed() {
        let mut mgr = manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        mgr.append(
            &sid,
            SessionMessage {
                role: "user".into(),
                content: "first".into(),
                timestamp: Utc::now(),
            },
        );
        assert_eq!(mgr.session_count(), 1);
        let history = mgr.get_history(&sid).unwrap();
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn append_adds_to_existing_session() {
        let mut mgr = manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        mgr.append(
            &sid,
            SessionMessage {
                role: "user".into(),
                content: "first".into(),
                timestamp: Utc::now(),
            },
        );
        mgr.append(
            &sid,
            SessionMessage {
                role: "assistant".into(),
                content: "second".into(),
                timestamp: Utc::now(),
            },
        );
        let history = mgr.get_history(&sid).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "first");
        assert_eq!(history[1].content, "second");
    }

    #[test]
    fn append_updates_updated_at() {
        let mut mgr = manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        mgr.append(
            &sid,
            SessionMessage {
                role: "user".into(),
                content: "msg1".into(),
                timestamp: Utc::now(),
            },
        );
        let first_updated = mgr.get_meta(&sid).unwrap().updated_at;

        // Small sleep to ensure time difference (chrono uses microsecond precision)
        std::thread::sleep(std::time::Duration::from_millis(10));

        mgr.append(
            &sid,
            SessionMessage {
                role: "user".into(),
                content: "msg2".into(),
                timestamp: Utc::now(),
            },
        );
        let second_updated = mgr.get_meta(&sid).unwrap().updated_at;
        assert!(second_updated >= first_updated);
    }

    // ── Session isolation (Req 4.9) ─────────────────────────────────

    #[test]
    fn sessions_are_isolated() {
        let mut mgr = manager();
        let sid_a = SessionId::full("main", "telegram", SessionScope::Dm, "user_a");
        let sid_b = SessionId::full("main", "telegram", SessionScope::Dm, "user_b");

        mgr.append(
            &sid_a,
            SessionMessage {
                role: "user".into(),
                content: "msg_a".into(),
                timestamp: Utc::now(),
            },
        );
        mgr.append(
            &sid_b,
            SessionMessage {
                role: "user".into(),
                content: "msg_b".into(),
                timestamp: Utc::now(),
            },
        );

        let history_a = mgr.get_history(&sid_a).unwrap();
        assert_eq!(history_a.len(), 1);
        assert_eq!(history_a[0].content, "msg_a");

        let history_b = mgr.get_history(&sid_b).unwrap();
        assert_eq!(history_b.len(), 1);
        assert_eq!(history_b[0].content, "msg_b");
    }

    #[test]
    fn different_agents_have_separate_sessions() {
        let mut mgr = manager();
        let sid_main = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        let sid_role = SessionId::full("ui_designer", "telegram", SessionScope::Dm, "user_1");

        mgr.append(
            &sid_main,
            SessionMessage {
                role: "user".into(),
                content: "to_main".into(),
                timestamp: Utc::now(),
            },
        );
        mgr.append(
            &sid_role,
            SessionMessage {
                role: "user".into(),
                content: "to_role".into(),
                timestamp: Utc::now(),
            },
        );

        assert_eq!(mgr.get_history(&sid_main).unwrap()[0].content, "to_main");
        assert_eq!(mgr.get_history(&sid_role).unwrap()[0].content, "to_role");
        assert_eq!(mgr.session_count(), 2);
    }

    // ── get_meta ────────────────────────────────────────────────────

    #[test]
    fn get_meta_returns_none_for_nonexistent() {
        let mgr = manager();
        let sid = SessionId::main_session();
        assert!(mgr.get_meta(&sid).is_none());
    }

    #[test]
    fn get_meta_returns_correct_id() {
        let mut mgr = manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        mgr.get_or_create(&sid);
        let meta = mgr.get_meta(&sid).unwrap();
        assert_eq!(meta.id, sid);
    }
}
