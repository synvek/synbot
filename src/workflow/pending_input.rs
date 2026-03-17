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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn register_and_deliver() {
        let store = PendingWorkflowInputStore::new();
        let rx = store.register("session1").await;
        assert!(store.is_waiting("session1").await);

        let delivered = store.deliver("session1", "hello".to_string()).await;
        assert!(delivered);
        assert!(!store.is_waiting("session1").await);
        let content = rx.await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn deliver_nonexistent_returns_false() {
        let store = PendingWorkflowInputStore::new();
        let delivered = store.deliver("nobody", "x".to_string()).await;
        assert!(!delivered);
    }

    #[tokio::test]
    async fn remove_cancels_pending() {
        let store = PendingWorkflowInputStore::new();
        let rx = store.register("session1").await;
        let removed = store.remove("session1").await;
        assert!(removed);
        assert!(!store.is_waiting("session1").await);
        assert!(rx.await.is_err());
    }

    #[tokio::test]
    async fn remove_nonexistent_returns_false() {
        let store = PendingWorkflowInputStore::new();
        assert!(!store.remove("nobody").await);
    }

    #[tokio::test]
    async fn is_waiting_false_for_unknown() {
        let store = PendingWorkflowInputStore::new();
        assert!(!store.is_waiting("unknown").await);
    }
}
