//! Structured session identifier for the multi-agent session system.
//!
//! A [`SessionId`] uniquely identifies a conversation session and comes in two forms:
//! - **Simple**: `agent:<agentId>:<channel>`
//! - **Full**: `agent:<agentId>:<channel>:<scope>:<identifier>`

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Session scope type indicating the kind of conversation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionScope {
    /// Direct message / peer-to-peer chat.
    Dm,
    /// Group chat with multiple participants.
    Group,
    /// Topic-based conversation.
    Topic,
}

impl SessionScope {
    /// Parse a scope string into a `SessionScope`.
    fn parse(s: &str) -> Result<Self> {
        match s {
            "dm" => Ok(SessionScope::Dm),
            "group" => Ok(SessionScope::Group),
            "topic" => Ok(SessionScope::Topic),
            other => bail!("invalid session scope: '{}'", other),
        }
    }

    /// Return the string representation of this scope.
    fn as_str(&self) -> &'static str {
        match self {
            SessionScope::Dm => "dm",
            SessionScope::Group => "group",
            SessionScope::Topic => "topic",
        }
    }
}

/// Structured session identifier.
///
/// Supports two forms:
/// - Simple: `agent:<agentId>:<channel>` (scope and identifier are `None`)
/// - Full: `agent:<agentId>:<channel>:<scope>:<identifier>`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId {
    pub agent_id: String,
    pub channel: String,
    /// Optional scope (present only in the full form).
    pub scope: Option<SessionScope>,
    /// Optional identifier within the scope (present only in the full form).
    pub identifier: Option<String>,
}

impl SessionId {
    /// Create a simple-form session id: `agent:<agentId>:<channel>`.
    pub fn simple(agent_id: &str, channel: &str) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            channel: channel.to_string(),
            scope: None,
            identifier: None,
        }
    }

    /// Create a full-form session id: `agent:<agentId>:<channel>:<scope>:<identifier>`.
    pub fn full(agent_id: &str, channel: &str, scope: SessionScope, identifier: &str) -> Self {
        Self {
            agent_id: agent_id.to_string(),
            channel: channel.to_string(),
            scope: Some(scope),
            identifier: Some(identifier.to_string()),
        }
    }

    /// Create the default main session: `agent:main:main`.
    pub fn main_session() -> Self {
        Self::simple("main", "main")
    }

    /// Parse a session id string.
    ///
    /// Accepts both forms:
    /// - `agent:<agentId>:<channel>` (3 parts after prefix)
    /// - `agent:<agentId>:<channel>:<scope>:<identifier>` (5 parts after prefix)
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.split(':').collect();

        if parts.is_empty() || parts[0] != "agent" {
            bail!("session id must start with 'agent:': '{}'", s);
        }

        match parts.len() {
            3 => {
                // Simple form: agent:<agentId>:<channel>
                let agent_id = parts[1];
                let channel = parts[2];
                if agent_id.is_empty() {
                    bail!("agent_id must not be empty in session id: '{}'", s);
                }
                if channel.is_empty() {
                    bail!("channel must not be empty in session id: '{}'", s);
                }
                Ok(Self::simple(agent_id, channel))
            }
            5 => {
                // Full form: agent:<agentId>:<channel>:<scope>:<identifier>
                let agent_id = parts[1];
                let channel = parts[2];
                let scope_str = parts[3];
                let identifier = parts[4];
                if agent_id.is_empty() {
                    bail!("agent_id must not be empty in session id: '{}'", s);
                }
                if channel.is_empty() {
                    bail!("channel must not be empty in session id: '{}'", s);
                }
                if identifier.is_empty() {
                    bail!("identifier must not be empty in session id: '{}'", s);
                }
                let scope = SessionScope::parse(scope_str)?;
                Ok(Self::full(agent_id, channel, scope, identifier))
            }
            _ => {
                bail!(
                    "invalid session id format (expected 3 or 5 colon-separated parts): '{}'",
                    s
                );
            }
        }
    }

    /// Format this session id as a string.
    pub fn format(&self) -> String {
        match (&self.scope, &self.identifier) {
            (Some(scope), Some(identifier)) => {
                format!(
                    "agent:{}:{}:{}:{}",
                    self.agent_id,
                    self.channel,
                    scope.as_str(),
                    identifier
                )
            }
            _ => {
                format!("agent:{}:{}", self.agent_id, self.channel)
            }
        }
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format())
    }
}

// ── tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // --- Constructors ---

    #[test]
    fn simple_creates_correct_fields() {
        let sid = SessionId::simple("main", "telegram");
        assert_eq!(sid.agent_id, "main");
        assert_eq!(sid.channel, "telegram");
        assert_eq!(sid.scope, None);
        assert_eq!(sid.identifier, None);
    }

    #[test]
    fn full_creates_correct_fields() {
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "12345");
        assert_eq!(sid.agent_id, "main");
        assert_eq!(sid.channel, "telegram");
        assert_eq!(sid.scope, Some(SessionScope::Dm));
        assert_eq!(sid.identifier, Some("12345".to_string()));
    }

    #[test]
    fn main_session_is_agent_main_main() {
        let sid = SessionId::main_session();
        assert_eq!(sid.agent_id, "main");
        assert_eq!(sid.channel, "main");
        assert_eq!(sid.scope, None);
        assert_eq!(sid.identifier, None);
        assert_eq!(sid.format(), "agent:main:main");
    }

    // --- Format ---

    #[test]
    fn format_simple_form() {
        let sid = SessionId::simple("main", "telegram");
        assert_eq!(sid.format(), "agent:main:telegram");
    }

    #[test]
    fn format_full_form_dm() {
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "12345");
        assert_eq!(sid.format(), "agent:main:telegram:dm:12345");
    }

    #[test]
    fn format_full_form_group() {
        let sid = SessionId::full("main", "discord", SessionScope::Group, "guild_123");
        assert_eq!(sid.format(), "agent:main:discord:group:guild_123");
    }

    #[test]
    fn format_full_form_topic() {
        let sid = SessionId::full("ui_designer", "feishu", SessionScope::Topic, "sprint_1");
        assert_eq!(sid.format(), "agent:ui_designer:feishu:topic:sprint_1");
    }

    // --- Parse ---

    #[test]
    fn parse_simple_form() {
        let sid = SessionId::parse("agent:main:telegram").unwrap();
        assert_eq!(sid.agent_id, "main");
        assert_eq!(sid.channel, "telegram");
        assert_eq!(sid.scope, None);
        assert_eq!(sid.identifier, None);
    }

    #[test]
    fn parse_full_form_dm() {
        let sid = SessionId::parse("agent:main:telegram:dm:12345").unwrap();
        assert_eq!(sid.agent_id, "main");
        assert_eq!(sid.channel, "telegram");
        assert_eq!(sid.scope, Some(SessionScope::Dm));
        assert_eq!(sid.identifier, Some("12345".to_string()));
    }

    #[test]
    fn parse_full_form_group() {
        let sid = SessionId::parse("agent:main:discord:group:guild_123").unwrap();
        assert_eq!(sid.scope, Some(SessionScope::Group));
        assert_eq!(sid.identifier, Some("guild_123".to_string()));
    }

    #[test]
    fn parse_full_form_topic() {
        let sid = SessionId::parse("agent:ui_designer:feishu:topic:sprint_1").unwrap();
        assert_eq!(sid.agent_id, "ui_designer");
        assert_eq!(sid.scope, Some(SessionScope::Topic));
        assert_eq!(sid.identifier, Some("sprint_1".to_string()));
    }

    #[test]
    fn parse_main_session() {
        let sid = SessionId::parse("agent:main:main").unwrap();
        assert_eq!(sid, SessionId::main_session());
    }

    // --- Parse errors ---

    #[test]
    fn parse_missing_prefix_fails() {
        assert!(SessionId::parse("session:main:main").is_err());
    }

    #[test]
    fn parse_wrong_part_count_fails() {
        assert!(SessionId::parse("agent:main").is_err());
        assert!(SessionId::parse("agent:main:telegram:dm").is_err());
        assert!(SessionId::parse("agent:a:b:c:d:e").is_err());
    }

    #[test]
    fn parse_empty_agent_id_fails() {
        assert!(SessionId::parse("agent::telegram").is_err());
    }

    #[test]
    fn parse_empty_channel_fails() {
        assert!(SessionId::parse("agent:main:").is_err());
    }

    #[test]
    fn parse_invalid_scope_fails() {
        assert!(SessionId::parse("agent:main:telegram:invalid:123").is_err());
    }

    #[test]
    fn parse_empty_identifier_fails() {
        assert!(SessionId::parse("agent:main:telegram:dm:").is_err());
    }

    // --- Roundtrip ---

    #[test]
    fn roundtrip_simple() {
        let original = "agent:main:telegram";
        let sid = SessionId::parse(original).unwrap();
        assert_eq!(sid.format(), original);
    }

    #[test]
    fn roundtrip_full() {
        let original = "agent:ui_designer:telegram:dm:12345";
        let sid = SessionId::parse(original).unwrap();
        assert_eq!(sid.format(), original);
    }

    // --- Display ---

    #[test]
    fn display_matches_format() {
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "12345");
        assert_eq!(format!("{}", sid), sid.format());

        let sid_simple = SessionId::simple("main", "main");
        assert_eq!(format!("{}", sid_simple), sid_simple.format());
    }

    // --- Hash and Eq ---

    #[test]
    fn equal_session_ids_have_same_hash() {
        use std::collections::HashSet;

        let a = SessionId::full("main", "telegram", SessionScope::Dm, "12345");
        let b = SessionId::full("main", "telegram", SessionScope::Dm, "12345");
        assert_eq!(a, b);

        let mut set = HashSet::new();
        set.insert(a);
        assert!(set.contains(&b));
    }

    #[test]
    fn different_session_ids_are_not_equal() {
        let a = SessionId::simple("main", "telegram");
        let b = SessionId::full("main", "telegram", SessionScope::Dm, "12345");
        assert_ne!(a, b);
    }

    // --- Serialize / Deserialize ---

    #[test]
    fn serde_roundtrip_simple() {
        let sid = SessionId::simple("main", "telegram");
        let json = serde_json::to_string(&sid).unwrap();
        let deserialized: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(sid, deserialized);
    }

    #[test]
    fn serde_roundtrip_full() {
        let sid = SessionId::full("main", "telegram", SessionScope::Dm, "12345");
        let json = serde_json::to_string(&sid).unwrap();
        let deserialized: SessionId = serde_json::from_str(&json).unwrap();
        assert_eq!(sid, deserialized);
    }

    #[test]
    fn serde_scope_serializes_lowercase() {
        let json = serde_json::to_string(&SessionScope::Dm).unwrap();
        assert_eq!(json, "\"dm\"");

        let json = serde_json::to_string(&SessionScope::Group).unwrap();
        assert_eq!(json, "\"group\"");

        let json = serde_json::to_string(&SessionScope::Topic).unwrap();
        assert_eq!(json, "\"topic\"");
    }
}
