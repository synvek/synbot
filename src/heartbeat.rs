//! Heartbeat service — periodic execution of config.heartbeat tasks; results sent to channel/userId.

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::info;

use crate::bus::InboundMessage;
use crate::config::Config;

pub struct HeartbeatService {
    config: Arc<RwLock<Config>>,
}

impl HeartbeatService {
    /// Create a heartbeat service that reads `config.heartbeat` each interval.
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self { config }
    }

    /// Run the heartbeat loop: every `heartbeat.interval` seconds, read tasks from config
    /// and send each task as an InboundMessage so the agent runs it and replies to the task's channel/chat_id.
    pub async fn run(&self, inbound_tx: mpsc::Sender<InboundMessage>) -> Result<()> {
        loop {
            let (enabled, interval_secs, tasks) = {
                let cfg = self.config.read().await;
                let hb = &cfg.heartbeat;
                (
                    hb.enabled,
                    hb.interval,
                    hb.tasks.clone(),
                )
            };

            if !enabled {
                info!("Heartbeat disabled");
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }

            let interval = Duration::from_secs(interval_secs);
            info!(
                "Heartbeat service running (interval: {}s, tasks: {})",
                interval_secs,
                tasks.len()
            );

            tokio::time::sleep(interval).await;

            let (enabled2, tasks2) = {
                let cfg = self.config.read().await;
                (cfg.heartbeat.enabled, cfg.heartbeat.tasks.clone())
            };
            if !enabled2 || tasks2.is_empty() {
                continue;
            }

            for task in &tasks2 {
                let msg = InboundMessage {
                    channel: task.channel.clone(),
                    sender_id: task.user_id.clone(),
                    chat_id: task.chat_id.clone(),
                    content: task.target.clone(),
                    timestamp: chrono::Utc::now(),
                    media: vec![],
                    metadata: serde_json::json!({ "source": "heartbeat" }),
                };
                if let Err(e) = inbound_tx.send(msg).await {
                    tracing::error!("Heartbeat failed to send task to bus: {e}");
                } else {
                    info!(
                        channel = %task.channel,
                        chat_id = %task.chat_id,
                        target = %task.target,
                        "Heartbeat task sent"
                    );
                }
            }
        }
    }
}
