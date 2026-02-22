//! Session tools: list_sessions and reset_session.

use anyhow::Result;
use serde_json::{json, Value};

use crate::agent::session_state::SharedSessionState;
use crate::tools::DynTool;

/// Tool to list active conversation sessions (channel, scope, identifier, agent_id, message count, running status).
pub struct ListSessionsTool {
    session_state: SharedSessionState,
}

impl ListSessionsTool {
    pub fn new(session_state: SharedSessionState) -> Self {
        Self { session_state }
    }
}

#[async_trait::async_trait]
impl DynTool for ListSessionsTool {
    fn name(&self) -> &str {
        "list_sessions"
    }

    fn description(&self) -> &str {
        "List active conversation sessions. Returns channel, scope, identifier, agent_id, message count, and whether the session is currently running (processing a message or tool). Optional args: channel (filter by channel), agent_id (filter by agent). Use when the user asks who is being tracked or what conversations exist."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "channel": {
                    "type": "string",
                    "description": "Optional filter: only list sessions for this channel (e.g. web, telegram)."
                },
                "agent_id": {
                    "type": "string",
                    "description": "Optional filter: only list sessions for this agent (e.g. main, ui_designer)."
                }
            },
            "required": []
        })
    }

    async fn call(&self, args: Value) -> Result<String> {
        let sm = self.session_state.session_manager.read().await;
        let all = sm.get_all_sessions();
        let channel_filter = args.get("channel").and_then(|v| v.as_str());
        let agent_filter = args.get("agent_id").and_then(|v| v.as_str());
        let filtered: Vec<_> = all
            .into_iter()
            .filter(|(meta, _)| {
                if let Some(c) = channel_filter {
                    if meta.id.channel != c {
                        return false;
                    }
                }
                if let Some(a) = agent_filter {
                    if meta.id.agent_id != a {
                        return false;
                    }
                }
                true
            })
            .collect();
        drop(sm);
        let active = self.session_state.get_active_snapshot().await;
        if filtered.is_empty() {
            return Ok("No sessions match.".to_string());
        }
        let lines: Vec<String> = filtered
            .iter()
            .map(|(meta, count)| {
                let scope = meta
                    .id
                    .scope
                    .as_ref()
                    .map(|s| format!("{:?}", s))
                    .unwrap_or_else(|| "—".to_string());
                let id = meta
                    .id
                    .identifier
                    .as_deref()
                    .unwrap_or("—");
                let session_key = meta.id.format();
                let (running, activity) = match active.get(&session_key) {
                    Some(a) => ("running=true", format!("  activity={}", a)),
                    None => ("running=false", String::new()),
                };
                format!(
                    "{}  channel={}  scope={}  identifier={}  messages={}  {}",
                    session_key,
                    meta.id.channel,
                    scope,
                    id,
                    count,
                    format!("{}{}", running, activity)
                )
            })
            .collect();
        Ok(lines.join("\n"))
    }
}

/// Tool to reset the current conversation session so it appears as a new chat.
pub struct ResetSessionTool {
    session_state: SharedSessionState,
}

impl ResetSessionTool {
    pub fn new(session_state: SharedSessionState) -> Self {
        Self { session_state }
    }
}

#[async_trait::async_trait]
impl DynTool for ResetSessionTool {
    fn name(&self) -> &str {
        "reset_session"
    }

    fn description(&self) -> &str {
        "Clear the current conversation history for this chat so the conversation starts fresh. Use when the user asks to start over, forget context, or when the thread is too long and affecting responses. The current session is inferred from context; no arguments required."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "confirm": {
                    "type": "boolean",
                    "description": "Optional. If true, confirms the reset. Omit or false to skip."
                }
            },
            "required": []
        })
    }

    async fn call(&self, args: Value) -> Result<String> {
        let session_key = match args.get("_session_id").and_then(|v| v.as_str()) {
            Some(k) => k.to_string(),
            None => {
                return Ok("Cannot reset: current session id not available (reset_session must be called from a conversation).".to_string());
            }
        };
        if let Err(e) = self.session_state.clear_session(&session_key).await {
            return Ok(format!("Failed to clear session: {}.", e));
        }
        Ok("Session cleared. The conversation will continue as a fresh chat.".to_string())
    }
}
