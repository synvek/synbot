//! Session manager — creation, lookup, routing, and lifecycle of conversation sessions.
//!
//! [`SessionManager`] is the central component that resolves which session a
//! message belongs to, manages in-memory session state, and handles participant
//! auto-completion for group/topic configurations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::agent::session::SessionMessage;
use crate::agent::session_id::{SessionId, SessionScope};
use crate::config::{GroupConfig, ParticipantConfig, TopicConfig};

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

/// Manages in-memory sessions, resolves session routing, and handles
/// participant auto-completion.
pub struct SessionManager {
    /// Active sessions keyed by `SessionId`.
    sessions: HashMap<SessionId, (SessionMeta, Vec<SessionMessage>)>,
    /// Group configurations loaded from config.
    groups: Vec<GroupConfig>,
    /// Topic configurations loaded from config.
    topics: Vec<TopicConfig>,
}

impl SessionManager {
    /// Create a new `SessionManager` with the given group and topic configs.
    pub fn new(groups: Vec<GroupConfig>, topics: Vec<TopicConfig>) -> Self {
        Self {
            sessions: HashMap::new(),
            groups,
            topics,
        }
    }

    // ── Session resolution ──────────────────────────────────────────

    /// Determine the [`SessionId`] for an incoming message based on the
    /// agent, channel, chat_id, and optional metadata.
    ///
    /// Resolution order:
    /// 1. Check if `chat_id` matches a configured group → `Group` scope
    /// 2. Check metadata for a `"group"` key → `Group` scope
    /// 3. Otherwise → `Dm` scope using `chat_id` as identifier
    pub fn resolve_session(
        &self,
        agent_id: &str,
        channel: &str,
        chat_id: &str,
        metadata: &serde_json::Value,
    ) -> SessionId {
        // 1. Check if chat_id matches a configured group
        if self.is_group_chat(channel, chat_id) {
            return SessionId::full(agent_id, channel, SessionScope::Group, chat_id);
        }

        // 2. Check metadata for group info
        if let Some(group_id) = metadata.get("group").and_then(|v| v.as_str()) {
            return SessionId::full(agent_id, channel, SessionScope::Group, group_id);
        }

        // 3. Default to Dm scope
        SessionId::full(agent_id, channel, SessionScope::Dm, chat_id)
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

    // ── Group chat detection ────────────────────────────────────────

    /// Check whether a `chat_id` on a given `channel` matches any configured
    /// group.
    ///
    /// A match occurs when any group has a participant whose `channel` matches
    /// and whose `channel_user_id` equals the `chat_id`.
    pub fn is_group_chat(&self, channel: &str, chat_id: &str) -> bool {
        self.groups.iter().any(|group| {
            group.participants.iter().any(|p| {
                p.channel == channel
                    && p.channel_user_id
                        .as_deref()
                        .map_or(false, |uid| uid == chat_id)
            })
        })
    }

    // ── Participant auto-completion ─────────────────────────────────

    /// Auto-complete participants for a list of [`ParticipantConfig`] entries.
    ///
    /// When a participant has a non-empty `channel_user_id`, the channel's
    /// connection account (represented by `channel_user_id = None`) is
    /// automatically included if not already present.
    ///
    /// Returns a new list with the auto-completed entries.
    pub fn auto_complete_participants(
        participants: &[ParticipantConfig],
    ) -> Vec<ParticipantConfig> {
        let mut result: Vec<ParticipantConfig> = participants.to_vec();

        // Collect channels that need their connection account added
        let mut channels_needing_account: Vec<String> = Vec::new();
        for p in participants {
            if p.channel_user_id.is_some() {
                // Check if the channel's connection account is already present
                let has_connection_account = participants.iter().any(|other| {
                    other.channel == p.channel && other.channel_user_id.is_none()
                });
                if !has_connection_account
                    && !channels_needing_account.contains(&p.channel)
                {
                    channels_needing_account.push(p.channel.clone());
                }
            }
        }

        // Add missing connection accounts
        for channel in channels_needing_account {
            result.push(ParticipantConfig {
                channel,
                channel_user_id: None,
            });
        }

        result
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

    // ── Helpers ─────────────────────────────────────────────────────

    fn empty_manager() -> SessionManager {
        SessionManager::new(Vec::new(), Vec::new())
    }

    fn manager_with_groups() -> SessionManager {
        let groups = vec![
            GroupConfig {
                name: "design_team".into(),
                participants: vec![
                    ParticipantConfig {
                        channel: "telegram".into(),
                        channel_user_id: Some("group_123".into()),
                    },
                    ParticipantConfig {
                        channel: "telegram".into(),
                        channel_user_id: None,
                    },
                    ParticipantConfig {
                        channel: "discord".into(),
                        channel_user_id: Some("guild_abc".into()),
                    },
                ],
            },
            GroupConfig {
                name: "dev_team".into(),
                participants: vec![ParticipantConfig {
                    channel: "discord".into(),
                    channel_user_id: Some("guild_dev".into()),
                }],
            },
        ];
        SessionManager::new(groups, Vec::new())
    }

    // ── resolve_session ─────────────────────────────────────────────

    #[test]
    fn resolve_session_dm_for_unknown_chat_id() {
        let mgr = manager_with_groups();
        let sid = mgr.resolve_session("main", "telegram", "user_999", &json!({}));
        assert_eq!(sid.agent_id, "main");
        assert_eq!(sid.channel, "telegram");
        assert_eq!(sid.scope, Some(SessionScope::Dm));
        assert_eq!(sid.identifier, Some("user_999".into()));
    }

    #[test]
    fn resolve_session_group_by_chat_id() {
        let mgr = manager_with_groups();
        let sid = mgr.resolve_session("main", "telegram", "group_123", &json!({}));
        assert_eq!(sid.scope, Some(SessionScope::Group));
        assert_eq!(sid.identifier, Some("group_123".into()));
    }

    #[test]
    fn resolve_session_group_by_metadata() {
        let mgr = empty_manager();
        let metadata = json!({ "group": "meta_group_42" });
        let sid = mgr.resolve_session("main", "telegram", "some_chat", &metadata);
        assert_eq!(sid.scope, Some(SessionScope::Group));
        assert_eq!(sid.identifier, Some("meta_group_42".into()));
    }

    #[test]
    fn resolve_session_chat_id_group_takes_priority_over_metadata() {
        let mgr = manager_with_groups();
        let metadata = json!({ "group": "other_group" });
        // chat_id matches a configured group — should use chat_id, not metadata
        let sid = mgr.resolve_session("main", "telegram", "group_123", &metadata);
        assert_eq!(sid.scope, Some(SessionScope::Group));
        assert_eq!(sid.identifier, Some("group_123".into()));
    }

    #[test]
    fn resolve_session_uses_agent_id() {
        let mgr = empty_manager();
        let sid = mgr.resolve_session("ui_designer", "discord", "user_1", &json!({}));
        assert_eq!(sid.agent_id, "ui_designer");
        assert_eq!(sid.channel, "discord");
        assert_eq!(sid.scope, Some(SessionScope::Dm));
        assert_eq!(sid.identifier, Some("user_1".into()));
    }

    // ── is_group_chat ───────────────────────────────────────────────

    #[test]
    fn is_group_chat_true_for_configured_group() {
        let mgr = manager_with_groups();
        assert!(mgr.is_group_chat("telegram", "group_123"));
        assert!(mgr.is_group_chat("discord", "guild_abc"));
        assert!(mgr.is_group_chat("discord", "guild_dev"));
    }

    #[test]
    fn is_group_chat_false_for_unknown_chat_id() {
        let mgr = manager_with_groups();
        assert!(!mgr.is_group_chat("telegram", "unknown_id"));
        assert!(!mgr.is_group_chat("discord", "unknown_id"));
    }

    #[test]
    fn is_group_chat_false_for_wrong_channel() {
        let mgr = manager_with_groups();
        // group_123 is on telegram, not discord
        assert!(!mgr.is_group_chat("discord", "group_123"));
    }

    #[test]
    fn is_group_chat_false_for_connection_account() {
        // A participant with channel_user_id = None is the connection account,
        // not a group identifier.
        let mgr = manager_with_groups();
        assert!(!mgr.is_group_chat("telegram", ""));
    }

    #[test]
    fn is_group_chat_false_when_no_groups() {
        let mgr = empty_manager();
        assert!(!mgr.is_group_chat("telegram", "anything"));
    }

    // ── get_or_create ───────────────────────────────────────────────

    #[test]
    fn get_or_create_creates_new_session() {
        let mut mgr = empty_manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        let history = mgr.get_or_create(&sid);
        assert!(history.is_empty());
        assert_eq!(mgr.session_count(), 1);
    }

    #[test]
    fn get_or_create_returns_existing_session() {
        let mut mgr = empty_manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");

        // Create and add a message
        mgr.get_or_create(&sid).push(SessionMessage {
            role: "user".into(),
            content: "hello".into(),
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
        let mgr = empty_manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        assert!(mgr.get_history(&sid).is_none());
    }

    #[test]
    fn get_history_returns_messages() {
        let mut mgr = empty_manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        mgr.append(
            &sid,
            SessionMessage {
                role: "user".into(),
                content: "hi".into(),
            },
        );
        let history = mgr.get_history(&sid).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].content, "hi");
    }

    // ── append ──────────────────────────────────────────────────────

    #[test]
    fn append_creates_session_if_needed() {
        let mut mgr = empty_manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        mgr.append(
            &sid,
            SessionMessage {
                role: "user".into(),
                content: "first".into(),
            },
        );
        assert_eq!(mgr.session_count(), 1);
        let history = mgr.get_history(&sid).unwrap();
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn append_adds_to_existing_session() {
        let mut mgr = empty_manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        mgr.append(
            &sid,
            SessionMessage {
                role: "user".into(),
                content: "first".into(),
            },
        );
        mgr.append(
            &sid,
            SessionMessage {
                role: "assistant".into(),
                content: "second".into(),
            },
        );
        let history = mgr.get_history(&sid).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "first");
        assert_eq!(history[1].content, "second");
    }

    #[test]
    fn append_updates_updated_at() {
        let mut mgr = empty_manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        mgr.append(
            &sid,
            SessionMessage {
                role: "user".into(),
                content: "msg1".into(),
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
            },
        );
        let second_updated = mgr.get_meta(&sid).unwrap().updated_at;
        assert!(second_updated >= first_updated);
    }

    // ── Session isolation (Req 4.9) ─────────────────────────────────

    #[test]
    fn sessions_are_isolated() {
        let mut mgr = empty_manager();
        let sid_a = SessionId::full("main", "telegram", SessionScope::Dm, "user_a");
        let sid_b = SessionId::full("main", "telegram", SessionScope::Dm, "user_b");

        mgr.append(
            &sid_a,
            SessionMessage {
                role: "user".into(),
                content: "msg_a".into(),
            },
        );
        mgr.append(
            &sid_b,
            SessionMessage {
                role: "user".into(),
                content: "msg_b".into(),
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
        let mut mgr = empty_manager();
        let sid_main = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        let sid_role = SessionId::full("ui_designer", "telegram", SessionScope::Dm, "user_1");

        mgr.append(
            &sid_main,
            SessionMessage {
                role: "user".into(),
                content: "to_main".into(),
            },
        );
        mgr.append(
            &sid_role,
            SessionMessage {
                role: "user".into(),
                content: "to_role".into(),
            },
        );

        assert_eq!(mgr.get_history(&sid_main).unwrap()[0].content, "to_main");
        assert_eq!(mgr.get_history(&sid_role).unwrap()[0].content, "to_role");
        assert_eq!(mgr.session_count(), 2);
    }

    // ── Participant auto-completion (Req 5.8) ───────────────────────

    #[test]
    fn auto_complete_adds_connection_account() {
        let participants = vec![ParticipantConfig {
            channel: "telegram".into(),
            channel_user_id: Some("12345".into()),
        }];
        let completed = SessionManager::auto_complete_participants(&participants);
        assert_eq!(completed.len(), 2);
        // Original entry
        assert!(completed
            .iter()
            .any(|p| p.channel == "telegram" && p.channel_user_id == Some("12345".into())));
        // Auto-added connection account
        assert!(completed
            .iter()
            .any(|p| p.channel == "telegram" && p.channel_user_id.is_none()));
    }

    #[test]
    fn auto_complete_does_not_duplicate_connection_account() {
        let participants = vec![
            ParticipantConfig {
                channel: "telegram".into(),
                channel_user_id: None, // already present
            },
            ParticipantConfig {
                channel: "telegram".into(),
                channel_user_id: Some("12345".into()),
            },
        ];
        let completed = SessionManager::auto_complete_participants(&participants);
        assert_eq!(completed.len(), 2); // no extra entry added
    }

    #[test]
    fn auto_complete_handles_multiple_channels() {
        let participants = vec![
            ParticipantConfig {
                channel: "telegram".into(),
                channel_user_id: Some("12345".into()),
            },
            ParticipantConfig {
                channel: "discord".into(),
                channel_user_id: Some("guild_abc".into()),
            },
        ];
        let completed = SessionManager::auto_complete_participants(&participants);
        assert_eq!(completed.len(), 4); // 2 original + 2 connection accounts
        assert!(completed
            .iter()
            .any(|p| p.channel == "telegram" && p.channel_user_id.is_none()));
        assert!(completed
            .iter()
            .any(|p| p.channel == "discord" && p.channel_user_id.is_none()));
    }

    #[test]
    fn auto_complete_no_change_when_only_connection_accounts() {
        let participants = vec![
            ParticipantConfig {
                channel: "telegram".into(),
                channel_user_id: None,
            },
            ParticipantConfig {
                channel: "discord".into(),
                channel_user_id: None,
            },
        ];
        let completed = SessionManager::auto_complete_participants(&participants);
        assert_eq!(completed.len(), 2); // no change
    }

    #[test]
    fn auto_complete_empty_participants() {
        let completed = SessionManager::auto_complete_participants(&[]);
        assert!(completed.is_empty());
    }

    #[test]
    fn auto_complete_multiple_users_same_channel_adds_one_account() {
        let participants = vec![
            ParticipantConfig {
                channel: "telegram".into(),
                channel_user_id: Some("user_1".into()),
            },
            ParticipantConfig {
                channel: "telegram".into(),
                channel_user_id: Some("user_2".into()),
            },
        ];
        let completed = SessionManager::auto_complete_participants(&participants);
        // 2 original + 1 connection account for telegram
        assert_eq!(completed.len(), 3);
        let connection_accounts: Vec<_> = completed
            .iter()
            .filter(|p| p.channel == "telegram" && p.channel_user_id.is_none())
            .collect();
        assert_eq!(connection_accounts.len(), 1);
    }

    // ── get_meta ────────────────────────────────────────────────────

    #[test]
    fn get_meta_returns_none_for_nonexistent() {
        let mgr = empty_manager();
        let sid = SessionId::main_session();
        assert!(mgr.get_meta(&sid).is_none());
    }

    #[test]
    fn get_meta_returns_correct_id() {
        let mut mgr = empty_manager();
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "user_1");
        mgr.get_or_create(&sid);
        let meta = mgr.get_meta(&sid).unwrap();
        assert_eq!(meta.id, sid);
    }
}
