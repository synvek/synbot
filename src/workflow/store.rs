//! Workflow state persistence: one JSON file per session under workflows_root.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;

use crate::workflow::types::WorkflowState;

#[derive(Clone)]
pub struct WorkflowStore {
    workflows_root: PathBuf,
}

impl WorkflowStore {
    pub fn new(workflows_root: &Path) -> Self {
        Self {
            workflows_root: workflows_root.to_path_buf(),
        }
    }

    /// Same rules as session file names: one flat path segment (`:` / `/` / `\` etc. → `_`).
    fn safe_filename(key: &str) -> String {
        key.chars()
            .map(|c| match c {
                ':' => '_',
                '/' | '\\' | '<' | '>' | '"' | '|' | '?' | '*' => '_',
                c if c.is_control() => '_',
                _ => c,
            })
            .collect()
    }

    fn state_path(&self, session_key: &str) -> PathBuf {
        self.workflows_root
            .join(format!("{}.json", Self::safe_filename(session_key)))
    }

    fn tmp_path(&self, session_key: &str) -> PathBuf {
        self.workflows_root
            .join(format!("{}.json.tmp", Self::safe_filename(session_key)))
    }

    /// Persist workflow state for this session (atomic write).
    pub async fn save_state(&self, session_key: &str, state: &WorkflowState) -> Result<()> {
        fs::create_dir_all(&self.workflows_root)
            .await
            .context("create workflows root")?;
        let json = serde_json::to_string_pretty(state).context("serialize workflow state")?;
        let tmp = self.tmp_path(session_key);
        let target = self.state_path(session_key);
        fs::write(&tmp, &json).await.context("write tmp workflow file")?;
        #[cfg(target_os = "windows")]
        if target.exists() {
            let _ = fs::remove_file(&target).await;
        }
        fs::rename(&tmp, &target)
            .await
            .context("rename tmp to target workflow file")?;
        Ok(())
    }

    /// Load workflow state for this session. Returns None if no file.
    pub async fn load_state(&self, session_key: &str) -> Result<Option<WorkflowState>> {
        let path = self.state_path(session_key);
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(&path)
            .await
            .context("read workflow state file")?;
        let state =
            serde_json::from_str(&data).context("deserialize workflow state")?;
        Ok(Some(state))
    }

    /// Remove persisted state for this session (e.g. after workflow completed and user cleared).
    pub async fn delete_state(&self, session_key: &str) -> Result<()> {
        let path = self.state_path(session_key);
        if path.exists() {
            fs::remove_file(&path)
                .await
                .context("remove workflow state file")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::types::{WorkflowDef, WorkflowState, WorkflowStepDef};
    use std::collections::HashMap;

    fn sample_state(session_key: &str) -> WorkflowState {
        let def = WorkflowDef {
            id: "wf-1".to_string(),
            name: "Test".to_string(),
            description: String::new(),
            inputs: vec![],
            steps: vec![WorkflowStepDef {
                id: "step1".to_string(),
                step_type: "llm".to_string(),
                description: "Do task".to_string(),
                input_key: None,
            }],
        };
        WorkflowState::new(
            session_key.to_string(),
            def,
            HashMap::new(),
            60,
        )
    }

    #[tokio::test]
    async fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = WorkflowStore::new(dir.path());

        let key = "agent:main:telegram";
        let state = sample_state(key);
        store.save_state(key, &state).await.unwrap();

        let loaded = store.load_state(key).await.unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.session_key, state.session_key);
        assert_eq!(loaded.workflow_id, state.workflow_id);
        assert_eq!(loaded.definition.steps.len(), state.definition.steps.len());
    }

    #[tokio::test]
    async fn load_nonexistent_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = WorkflowStore::new(dir.path());
        let loaded = store.load_state("nonexistent").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn delete_state_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = WorkflowStore::new(dir.path());
        let key = "session:key";
        store.save_state(key, &sample_state(key)).await.unwrap();
        assert!(store.load_state(key).await.unwrap().is_some());

        store.delete_state(key).await.unwrap();
        assert!(store.load_state(key).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn safe_filename_colon_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let store = WorkflowStore::new(dir.path());
        let key = "agent:main:chat_123";
        store.save_state(key, &sample_state(key)).await.unwrap();
        let path = dir.path().join("agent_main_chat_123.json");
        assert!(path.exists(), "session key with colons should become underscores in filename");
    }

    #[tokio::test]
    async fn safe_filename_slash_does_not_create_subdir() {
        let dir = tempfile::tempdir().unwrap();
        let store = WorkflowStore::new(dir.path());
        let key = "agent:main:dingtalk:dm:a/b+c=";
        store.save_state(key, &sample_state(key)).await.unwrap();
        let flat = dir.path().join("agent_main_dingtalk_dm_a_b+c=.json");
        assert!(
            flat.exists(),
            "expected single file in workflows root, not nested dirs"
        );
        assert_eq!(dir.path().read_dir().unwrap().count(), 1);
    }

    #[tokio::test]
    async fn delete_nonexistent_ok() {
        let dir = tempfile::tempdir().unwrap();
        let store = WorkflowStore::new(dir.path());
        store.delete_state("nonexistent").await.unwrap();
    }
}
