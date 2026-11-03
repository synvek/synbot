//! Session manager — creation, lookup, routing, and lifecycle of conversation sessions.
//!
//! [`SessionManager`] resolves which session a message belongs to and manages
//! in-memory session state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::agent::session::SessionMessage;
use crate::agent::session_id::{SessionId, SessionScope};
use crate::agent::timestamp_serde;

// ---------------------------------------------------------------------------
// Session metadata
// ---------------------------------------------------------------------------

/// A per-session model choice made from the console (`provider` + `model`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelOverride {
    pub provider: String,
    pub model: String,
}

/// A plan the agent produced in plan mode and is waiting for the user to approve.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingPlan {
    pub id: String,
    pub summary: String,
    pub steps: Vec<crate::agent::events::PlanStep>,
    #[serde(with = "timestamp_serde::rfc3339_local")]
    pub created_at: DateTime<Utc>,
}

/// Metadata associated with a session.
///
/// Every field added after the first release is `#[serde(default)]`: session files written by an
/// older build must load unchanged, and files written by this build must stay readable there.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionMeta {
    /// The unique session identifier.
    pub id: SessionId,
    /// List of participant identifiers (e.g. `"telegram"`, `"telegram:12345"`).
    pub participants: Vec<String>,
    /// When the session was first created.
    #[serde(with = "timestamp_serde::rfc3339_local")]
    pub created_at: DateTime<Utc>,
    /// When the session was last updated (message appended).
    #[serde(with = "timestamp_serde::rfc3339_local")]
    pub updated_at: DateTime<Utc>,
    /// Display title. Derived from the first user message when absent.
    #[serde(default)]
    pub title: Option<String>,
    /// Hidden from the default session list but still readable.
    #[serde(default)]
    pub archived: bool,
    /// Model chosen for this conversation; `None` means the configured default.
    #[serde(default)]
    pub model_override: Option<ModelOverride>,
    /// Plan-first mode: the next turn may only read, then stops with a plan.
    #[serde(default)]
    pub plan_mode: bool,
    /// Plan produced by the last plan-mode turn, waiting for an approval decision.
    #[serde(default)]
    pub pending_plan: Option<PendingPlan>,
}

impl Default for SessionMeta {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: SessionId::main_session(),
            participants: Vec::new(),
            created_at: now,
            updated_at: now,
            title: None,
            archived: false,
            model_override: None,
            plan_mode: false,
            pending_plan: None,
        }
    }
}

/// Derive a display title from the first user message.
///
/// Counts `char`s (not bytes) so CJK and emoji are never cut mid-codepoint, and collapses
/// newlines/tabs so a pasted block does not turn into a multi-line sidebar row.
pub fn derive_title(content: &str) -> String {
    let flat: String = content
        .chars()
        .map(|c| match c {
            '\n' | '\r' | '\t' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let trimmed = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    let title: String = trimmed.chars().take(40).collect();
    if title.trim().is_empty() {
        "New conversation".to_string()
    } else {
        title.trim().to_string()
    }
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
    /// If metadata contains a truthy `"group"` key (e.g. Telegram/Discord/Feishu, or
    /// DingTalk when `conversationType` is group), uses `Group` scope; otherwise `Dm`.
    /// Identifier is always `chat_id` (for DingTalk, the `conversationId`).
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
                ..Default::default()
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
                ..Default::default()
            };
            (meta, Vec::new())
        });
        entry.0.updated_at = Utc::now();
        // First user message names the conversation; a manual rename always wins.
        if entry.0.title.is_none() && message.role == "user" {
            entry.0.title = Some(derive_title(&message.content));
        }
        entry.1.push(message);
    }

    /// Borrow a session's metadata, creating the session if needed.
    pub fn meta_mut(&mut self, id: &SessionId) -> &mut SessionMeta {
        if !self.sessions.contains_key(id) {
            self.get_or_create(id);
        }
        &mut self
            .sessions
            .get_mut(id)
            .expect("session was just inserted")
            .0
    }

    /// Rename a session. Returns `false` when the session does not exist.
    pub fn set_title(&mut self, id: &SessionId, title: Option<String>) -> bool {
        match self.sessions.get_mut(id) {
            Some((meta, _)) => {
                meta.title = title.map(|t| t.trim().chars().take(80).collect::<String>());
                meta.updated_at = Utc::now();
                true
            }
            None => false,
        }
    }

    /// Hide/show a session in the default list. Returns `false` when it does not exist.
    pub fn set_archived(&mut self, id: &SessionId, archived: bool) -> bool {
        match self.sessions.get_mut(id) {
            Some((meta, _)) => {
                meta.archived = archived;
                meta.updated_at = Utc::now();
                true
            }
            None => false,
        }
    }

    /// Persist a per-session model choice (`None` restores the configured default).
    pub fn set_model_override(&mut self, id: &SessionId, model: Option<ModelOverride>) -> bool {
        match self.sessions.get_mut(id) {
            Some((meta, _)) => {
                meta.model_override = model;
                true
            }
            None => false,
        }
    }

    /// Toggle plan-first mode for the next turn.
    pub fn set_plan_mode(&mut self, id: &SessionId, enabled: bool) -> bool {
        match self.sessions.get_mut(id) {
            Some((meta, _)) => {
                meta.plan_mode = enabled;
                true
            }
            None => false,
        }
    }

    /// Store the plan awaiting an approval decision.
    pub fn set_pending_plan(&mut self, id: &SessionId, plan: Option<PendingPlan>) -> bool {
        match self.sessions.get_mut(id) {
            Some((meta, _)) => {
                meta.pending_plan = plan;
                true
            }
            None => false,
        }
    }

    /// Take the pending plan out of the session (the decision consumes it).
    pub fn take_pending_plan(&mut self, id: &SessionId) -> Option<PendingPlan> {
        self.sessions
            .get_mut(id)
            .and_then(|(meta, _)| meta.pending_plan.take())
    }

    /// Remove a session by id. Used when resetting a conversation so it appears fresh.
    /// No-op if the session does not exist.
    pub fn remove_session(&mut self, id: &SessionId) {
        self.sessions.remove(id);
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

    // ── multi-session metadata ──────────────────────────────────────

    #[test]
    fn derive_title_collapses_whitespace_and_truncates_by_char() {
        assert_eq!(
            derive_title("  fix   the\nbuild\tplease "),
            "fix the build please"
        );
        assert_eq!(derive_title("   "), "New conversation");
        // 41 CJK characters: the cut must land on a char boundary, not a byte one.
        let long: String = "改".repeat(41);
        let title = derive_title(&long);
        assert_eq!(title.chars().count(), 40);
        assert!(long.starts_with(&title));
    }

    #[test]
    fn append_derives_title_once_and_manual_rename_wins() {
        let mut mgr = manager();
        let sid = SessionId::full("main", "web", SessionScope::Dm, "w-0123456789ab");
        mgr.append(
            &sid,
            SessionMessage {
                role: "user".into(),
                content: "first question".into(),
                timestamp: Utc::now(),
            },
        );
        assert_eq!(
            mgr.get_meta(&sid).unwrap().title.as_deref(),
            Some("first question")
        );

        // A later user message must not rewrite the derived title.
        mgr.append(
            &sid,
            SessionMessage {
                role: "user".into(),
                content: "second question".into(),
                timestamp: Utc::now(),
            },
        );
        assert_eq!(
            mgr.get_meta(&sid).unwrap().title.as_deref(),
            Some("first question")
        );

        assert!(mgr.set_title(&sid, Some("  renamed  ".into())));
        assert_eq!(
            mgr.get_meta(&sid).unwrap().title.as_deref(),
            Some("renamed")
        );
    }

    #[test]
    fn setters_cover_archive_model_and_plan() {
        let mut mgr = manager();
        let sid = SessionId::full("main", "web", SessionScope::Dm, "w-0123456789ab");
        mgr.get_or_create(&sid);

        assert!(mgr.set_archived(&sid, true));
        assert!(mgr.get_meta(&sid).unwrap().archived);

        assert!(mgr.set_model_override(
            &sid,
            Some(ModelOverride {
                provider: "openai".into(),
                model: "gpt-4o-mini".into(),
            })
        ));
        assert_eq!(
            mgr.get_meta(&sid)
                .unwrap()
                .model_override
                .as_ref()
                .map(|m| m.model.as_str()),
            Some("gpt-4o-mini")
        );
        assert!(mgr.set_model_override(&sid, None));
        assert!(mgr.get_meta(&sid).unwrap().model_override.is_none());

        assert!(mgr.set_plan_mode(&sid, true));
        assert!(mgr.get_meta(&sid).unwrap().plan_mode);

        let missing = SessionId::full("main", "web", SessionScope::Dm, "w-ffffffffffff");
        assert!(!mgr.set_archived(&missing, true));
        assert!(!mgr.set_title(&missing, Some("x".into())));
    }

    #[test]
    fn pending_plan_is_consumed_once() {
        let mut mgr = manager();
        let sid = SessionId::full("main", "web", SessionScope::Dm, "w-0123456789ab");
        mgr.get_or_create(&sid);
        let plan = PendingPlan {
            id: "plan-1".into(),
            summary: "do the thing".into(),
            steps: Vec::new(),
            created_at: Utc::now(),
        };
        assert!(mgr.set_pending_plan(&sid, Some(plan)));
        assert_eq!(
            mgr.get_meta(&sid)
                .unwrap()
                .pending_plan
                .as_ref()
                .map(|p| p.id.as_str()),
            Some("plan-1")
        );
        let taken = mgr.take_pending_plan(&sid).expect("plan must be pending");
        assert_eq!(taken.id, "plan-1");
        assert!(mgr.get_meta(&sid).unwrap().pending_plan.is_none());
        assert!(mgr.take_pending_plan(&sid).is_none());
    }
}
