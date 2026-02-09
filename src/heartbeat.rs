//! Heartbeat service — periodic agent wake-up to check HEARTBEAT.md.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::info;

use crate::bus::InboundMessage;

const DEFAULT_INTERVAL_SECS: u64 = 30 * 60; // 30 minutes

const HEARTBEAT_PROMPT: &str =
    "Read HEARTBEAT.md in your workspace (if it exists). \
     Follow any instructions or tasks listed there. \
     If nothing needs attention, reply with just: HEARTBEAT_OK";

pub struct HeartbeatService {
    workspace: PathBuf,
    interval: Duration,
    enabled: bool,
}

impl HeartbeatService {
    pub fn new(workspace: &Path, enabled: bool) -> Self {
        Self {
            workspace: workspace.to_path_buf(),
            interval: Duration::from_secs(DEFAULT_INTERVAL_SECS),
            enabled,
        }
    }

    fn heartbeat_file(&self) -> PathBuf {
        self.workspace.join("HEARTBEAT.md")
    }

    fn has_actionable_content(&self) -> bool {
        let path = self.heartbeat_file();
        if !path.exists() {
            return false;
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => content
                .lines()
                .any(|l| {
                    let t = l.trim();
                    !t.is_empty() && !t.starts_with('#') && !t.starts_with("<!--")
                }),
            Err(_) => false,
        }
    }

    /// Run the heartbeat loop, injecting synthetic inbound messages.
    pub async fn run(&self, inbound_tx: mpsc::Sender<InboundMessage>) -> Result<()> {
        if !self.enabled {
            info!("Heartbeat disabled");
            return Ok(());
        }
        info!("Heartbeat service started (interval: {}s)", self.interval.as_secs());
        loop {
            tokio::time::sleep(self.interval).await;
            if self.has_actionable_content() {
                info!("Heartbeat: actionable content found, waking agent");
                let _ = inbound_tx
                    .send(InboundMessage {
                        channel: "system".into(),
                        sender_id: "heartbeat".into(),
                        chat_id: "heartbeat".into(),
                        content: HEARTBEAT_PROMPT.into(),
                        timestamp: chrono::Utc::now(),
                        media: vec![],
                        metadata: serde_json::Value::Null,
                    })
                    .await;
            }
        }
    }
}
