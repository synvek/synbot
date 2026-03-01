//! Shared session state for per-session parallelism and session tools.
//!
//! Holds the in-memory sessions map (per-session locked), SessionManager, and
//! SessionStore. Provides `clear_session` for the reset_session tool and is used
//! by the agent loop for all session reads/writes.

use anyhow::Result;
use rig::message::Message;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

use crate::agent::session::{SessionMessage, SessionStore};
use crate::agent::session_id::SessionId;
use crate::agent::session_manager::{SessionManager, SessionMeta};

/// Shared state for session storage: per-session message history, manager, and persistence.
/// Used by the agent loop (dispatcher + workers) and by the reset_session tool.
#[derive(Clone)]
pub struct SharedSessionState {
    /// In-memory conversation history per session_key. Each entry is independently locked.
    pub sessions: Arc<RwLock<HashMap<String, Arc<Mutex<Vec<Message>>>>>>,
    pub session_manager: Arc<RwLock<SessionManager>>,
    pub session_store: Arc<SessionStore>,
    /// Session keys currently being processed -> human-readable activity (e.g. "processing", "tool: exec").
    active_tasks: Arc<RwLock<HashMap<String, String>>>,
}

impl SharedSessionState {
    pub fn new(session_store: SessionStore) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_manager: Arc::new(RwLock::new(SessionManager::new())),
            session_store: Arc::new(session_store),
            active_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Mark a session as currently running with the given activity description.
    pub async fn set_active(&self, session_key: &str, activity: &str) {
        self.active_tasks
            .write()
            .await
            .insert(session_key.to_string(), activity.to_string());
    }

    /// Clear the running state for a session.
    pub async fn clear_active(&self, session_key: &str) {
        self.active_tasks.write().await.remove(session_key);
    }

    /// Snapshot of which sessions are running and their activity. Used by list_sessions.
    pub async fn get_active_snapshot(&self) -> HashMap<String, String> {
        self.active_tasks.read().await.clone()
    }

    /// Get or create the in-memory message list for a session. Returns a clone of the
    /// Arc<Mutex<Vec<Message>>> so the caller can lock it and use it.
    pub async fn get_or_create_session_messages(
        &self,
        session_key: &str,
    ) -> Arc<Mutex<Vec<Message>>> {
        let mut guard = self.sessions.write().await;
        guard
            .entry(session_key.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(Vec::new())))
            .clone()
    }

    /// Load all persisted sessions from disk into the in-memory map and SessionManager.
    /// Called once at startup (e.g. from AgentLoop::new).
    pub async fn load_persisted_sessions(&self) -> Result<()> {
        let session_data = match self.session_store.load_all_sessions().await {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "Failed to load persisted sessions, starting fresh");
                return Ok(());
            }
        };
        if session_data.is_empty() {
            return Ok(());
        }
        info!(count = session_data.len(), "Restored persisted sessions");
        let mut sessions_guard = self.sessions.write().await;
        let mut sm = self.session_manager.write().await;
        for (key, data) in session_data {
            let messages: Vec<Message> = data.messages.iter().map(SessionMessage::to_message).collect();
            sessions_guard
                .entry(key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(Vec::new())))
                .lock()
                .await
                .extend(messages);
            if let Ok(session_id) = SessionId::parse(&key) {
                let history = sm.get_or_create(&session_id);
                history.clear();
                history.extend(data.messages);
            }
        }
        info!(count = sm.session_count(), "Loaded sessions into session_manager");
        Ok(())
    }

    /// Append a user message to a session and persist. Used when handling workflow triggers
    /// so the user's "twfw ..." or "continue workflow" appears in conversation history.
    pub async fn append_user_message_and_save(
        &self,
        session_key: &str,
        content: &str,
    ) -> Result<()> {
        let session_messages = self.get_or_create_session_messages(session_key).await;
        {
            let mut history = session_messages.lock().await;
            history.push(Message::user(content));
        }
        let messages = session_messages.lock().await.clone();
        let now = chrono::Utc::now();
        let meta = if let Ok(sid) = SessionId::parse(session_key) {
            {
                let mut sm = self.session_manager.write().await;
                let session_messages_sm: Vec<SessionMessage> =
                    messages.iter().map(SessionMessage::from_message).collect();
                let history = sm.get_or_create(&sid);
                history.clear();
                history.extend(session_messages_sm);
            }
            Some(SessionMeta {
                id: sid,
                participants: vec![],
                created_at: now,
                updated_at: now,
            })
        } else {
            None
        };
        if let Err(e) = self
            .session_store
            .save_session(session_key, &messages, meta.as_ref())
            .await
        {
            warn!(
                session_key = %session_key,
                error = %e,
                "Failed to persist session (append_user_message)"
            );
        }
        Ok(())
    }

    /// Append an assistant message to a session and persist. Used by workflow runner
    /// so that workflow output appears in conversation history.
    pub async fn append_assistant_message_and_save(
        &self,
        session_key: &str,
        content: &str,
    ) -> Result<()> {
        let session_messages = self.get_or_create_session_messages(session_key).await;
        {
            let mut history = session_messages.lock().await;
            history.push(Message::assistant(content));
        }
        let messages = session_messages.lock().await.clone();
        let now = chrono::Utc::now();
        let meta = if let Ok(sid) = SessionId::parse(session_key) {
            {
                let mut sm = self.session_manager.write().await;
                let session_messages_sm: Vec<SessionMessage> =
                    messages.iter().map(SessionMessage::from_message).collect();
                let history = sm.get_or_create(&sid);
                history.clear();
                history.extend(session_messages_sm);
            }
            Some(SessionMeta {
                id: sid,
                participants: vec![],
                created_at: now,
                updated_at: now,
            })
        } else {
            None
        };
        if let Err(e) = self
            .session_store
            .save_session(session_key, &messages, meta.as_ref())
            .await
        {
            warn!(
                session_key = %session_key,
                error = %e,
                "Failed to persist session (append_assistant_message)"
            );
        }
        Ok(())
    }

    /// Clear a session: remove from in-memory map, from SessionManager, and delete
    /// the persisted file. Used by the reset_session tool. No-op if the session
    /// does not exist.
    pub async fn clear_session(&self, session_key: &str) -> Result<()> {
        self.sessions.write().await.remove(session_key);
        if let Ok(sid) = SessionId::parse(session_key) {
            self.session_manager.write().await.remove_session(&sid);
        }
        self.session_store.delete_session(session_key).await?;
        debug!(session_key = %session_key, "session cleared");
        Ok(())
    }
}
