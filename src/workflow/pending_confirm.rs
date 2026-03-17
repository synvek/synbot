//! Pending user confirmation for a workflow definition (when user provided JSON).
//! Uses the same intent-based approval classifier as permission/approval (no hardcoded keywords).

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::channels::approval_classifier;
use crate::rig_provider::SynbotCompletionModel;
use crate::workflow::types::WorkflowDef;

#[derive(Clone)]
pub struct PendingConfirmStore {
    inner: Arc<RwLock<HashMap<String, WorkflowDef>>>,
}

impl PendingConfirmStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn set(&self, session_key: &str, def: WorkflowDef) {
        self.inner.write().await.insert(session_key.to_string(), def);
    }

    /// Take the pending definition if the user's message is classified as approval (agree/confirm).
    /// Uses the same LLM-based classifier as command approval so any language works.
    pub async fn take_if_confirm(
        &self,
        session_key: &str,
        content: &str,
        model: &dyn SynbotCompletionModel,
    ) -> Option<WorkflowDef> {
        match approval_classifier::classify_approval_response(model, content).await {
            Some(true) => self.inner.write().await.remove(session_key),
            _ => None,
        }
    }

    pub async fn remove(&self, session_key: &str) -> Option<WorkflowDef> {
        self.inner.write().await.remove(session_key)
    }
}

impl Default for PendingConfirmStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::types::{WorkflowDef, WorkflowStepDef};

    fn sample_def() -> WorkflowDef {
        WorkflowDef {
            id: "wf-1".to_string(),
            name: "Test".to_string(),
            description: String::new(),
            inputs: vec![],
            steps: vec![WorkflowStepDef {
                id: "s1".to_string(),
                step_type: "llm".to_string(),
                description: "".to_string(),
                input_key: None,
            }],
        }
    }

    #[tokio::test]
    async fn set_and_remove() {
        let store = PendingConfirmStore::new();
        let def = sample_def();
        store.set("session1", def.clone()).await;
        let taken = store.remove("session1").await;
        assert!(taken.is_some());
        assert_eq!(taken.unwrap().id, def.id);
        assert!(store.remove("session1").await.is_none());
    }

    #[tokio::test]
    async fn remove_nonexistent_returns_none() {
        let store = PendingConfirmStore::new();
        assert!(store.remove("nobody").await.is_none());
    }
}
