//! Pending user input for workflow steps: session_key -> channel to send the next user message.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, RwLock};

/// Shared store: when a workflow step is waiting for user input, we register a oneshot sender
/// here. The agent loop, when it receives a message for that session, sends the content and removes.
#[derive(Clone)]
pub struct PendingWorkflowInputStore {
    inner: Arc<RwLock<HashMap<String, oneshot::Sender<String>>>>,
}

impl PendingWorkflowInputStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register that this session is waiting for user input. Returns the receiver.
    /// Caller must await the receiver with a timeout.
    pub async fn register(&self, session_key: &str) -> oneshot::Receiver<String> {
        let (tx, rx) = oneshot::channel();
        self.inner.write().await.insert(session_key.to_string(), tx);
        rx
    }

    /// If this session is waiting, send the user content and remove. Returns true if delivered.
    pub async fn deliver(&self, session_key: &str, content: String) -> bool {
        let mut guard = self.inner.write().await;
        if let Some(tx) = guard.remove(session_key) {
            let _ = tx.send(content);
            true
        } else {
            false
        }
    }

    /// Remove pending for this session (e.g. on timeout or cancel). Returns true if was present.
    pub async fn remove(&self, session_key: &str) -> bool {
        self.inner.write().await.remove(session_key).is_some()
    }

    pub async fn is_waiting(&self, session_key: &str) -> bool {
        self.inner.read().await.contains_key(session_key)
    }
}

impl Default for PendingWorkflowInputStore {
    fn default() -> Self {
        Self::new()
    }
}
