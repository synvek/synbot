//! Config-based cron runner: reads config.cron.tasks and fires due jobs, sending InboundMessage to the bus.

use anyhow::Result;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

use crate::bus::InboundMessage;
use crate::config::{Config, CronTaskConfig};

/// Tracks last run time per task key to avoid duplicate fires.
fn task_key(task: &CronTaskConfig) -> String {
    format!(
        "{}|{}|{}|{}",
        task.schedule, task.channel, task.user_id, task.command
    )
}

/// Normalize cron expression: 5-field (min hour dom month dow) -> 7-field (sec min hour dom month dow year).
fn normalize_cron_expr(expr: &str) -> String {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() == 5 {
        format!("0 {} *", expr) // sec=0, then min hour dom month dow, year=*
    } else {
        expr.to_string()
    }
}

/// Returns the next run time (epoch ms) for a cron expression after the given timestamp, or None if invalid.
fn next_run_ms_after(expr: &str, after_ms: i64) -> Option<i64> {
    use chrono::TimeZone;
    let normalized = normalize_cron_expr(expr);
    let schedule = cron::Schedule::from_str(&normalized).ok()?;
    let after_secs = after_ms / 1000;
    let rem = ((after_ms % 1000) + 1000) % 1000;
    let after_nsecs = (rem * 1_000_000) as u32;
    let after_dt = chrono::Utc
        .timestamp_opt(after_secs, after_nsecs)
        .single()
        .unwrap_or_else(chrono::Utc::now);
    let next = schedule.after(&after_dt).next()?;
    Some(next.timestamp_millis())
}

pub struct ConfigCronRunner {
    config: Arc<RwLock<Config>>,
    /// task_key -> last_fired_at_ms
    last_fired: RwLock<HashMap<String, i64>>,
}

impl ConfigCronRunner {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self {
            config,
            last_fired: RwLock::new(HashMap::new()),
        }
    }

    /// Run loop: every 60s read config.cron.tasks, for each enabled task check if due, then send InboundMessage.
    pub async fn run(&self, inbound_tx: mpsc::Sender<InboundMessage>) -> Result<()> {
        info!("Config cron runner started");
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.tick().await; // first tick fires immediately, skip it

        loop {
            interval.tick().await;

            let tasks = {
                let cfg = self.config.read().await;
                cfg.cron.tasks.clone()
            };

            let now_ms = chrono::Utc::now().timestamp_millis();
            let mut last_fired = self.last_fired.write().await;

            for task in tasks {
                if !task.enabled || task.command.is_empty() {
                    continue;
                }
                let key = task_key(&task);
                let after_ms = last_fired.get(&key).copied().unwrap_or(0);
                let next_ms = match next_run_ms_after(&task.schedule, after_ms) {
                    Some(ms) => ms,
                    None => {
                        warn!(
                            schedule = %task.schedule,
                            "Config cron: invalid schedule, skipping"
                        );
                        continue;
                    }
                };
                if now_ms < next_ms {
                    continue;
                }

                let chat_id = task
                    .chat_id
                    .as_deref()
                    .unwrap_or(&task.user_id)
                    .to_string();
                let msg = InboundMessage {
                    channel: task.channel.clone(),
                    sender_id: task.user_id.clone(),
                    chat_id,
                    content: task.command.clone(),
                    timestamp: chrono::Utc::now(),
                    media: vec![],
                    metadata: serde_json::json!({ "source": "cron", "description": task.description }),
                };
                if let Err(e) = inbound_tx.send(msg).await {
                    warn!("Config cron failed to send task: {e}");
                } else {
                    info!(
                        schedule = %task.schedule,
                        channel = %task.channel,
                        "Config cron task sent"
                    );
                }
                last_fired.insert(key, now_ms);
            }
        }
    }
}
