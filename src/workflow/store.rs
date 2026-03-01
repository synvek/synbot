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

    fn safe_filename(key: &str) -> String {
        key.replace(':', "_")
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
