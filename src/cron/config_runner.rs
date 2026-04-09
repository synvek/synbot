//! Config-based cron runner: reads config.cron.tasks and fires due jobs, sending InboundMessage to the bus.

use anyhow::Result;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

use chrono::{DateTime, Local, Utc};

use crate::bus::InboundMessage;
use crate::config::{Config, CronTaskConfig};

/// When a task has never fired, use this lookback for the `after` instant when computing the next
/// schedule match. Otherwise `after = now` can fall a few seconds *past* the scheduled minute (the
/// runner ticks every 60s), and `cron::Schedule::after` then returns *tomorrow*'s occurrence — so
/// today's run is skipped entirely.
const NEVER_FIRED_AFTER_LOOKBACK_MS: i64 = 180_000;

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
///
/// Uses the system local timezone for the schedule fields (hour/minute). The `cron` crate interprets
/// expression fields as wall-clock time in the timezone of the "after" datetime; we pass
/// [`DateTime<Local>`] so e.g. `"0 21 * * *"` means 21:00 local, not UTC.
fn next_run_ms_after(expr: &str, after_ms: i64) -> Option<i64> {
    let normalized = normalize_cron_expr(expr);
    let schedule = cron::Schedule::from_str(&normalized).ok()?;
    let after_dt: DateTime<Local> = DateTime::from_timestamp_millis(after_ms)
        .map(|utc| utc.with_timezone(&Local))
        .unwrap_or_else(Local::now);
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

            // Local wall clock: cron fields match machine local time (see `next_run_ms_after`).
            let now_ms = Local::now().timestamp_millis();
            let mut last_fired = self.last_fired.write().await;

            for task in tasks {
                if !task.enabled || task.command.is_empty() {
                    continue;
                }
                let key = task_key(&task);
                let after_ms = match last_fired.get(&key).copied() {
                    Some(ts) => ts,
                    None => now_ms.saturating_sub(NEVER_FIRED_AFTER_LOOKBACK_MS),
                };
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
                    timestamp: Utc::now(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};

    #[test]
    fn config_cron_runner_new() {
        let config = Arc::new(RwLock::new(Config::default()));
        let _runner = ConfigCronRunner::new(config);
    }

    /// If `after` is already a few seconds into the scheduled minute, the next match must not jump to tomorrow.
    #[test]
    fn next_run_after_same_minute_without_lookback_would_skip_today() {
        let during = Local
            .with_ymd_and_hms(2030, 6, 15, 22, 35, 8)
            .unwrap()
            .timestamp_millis();
        let next_late = next_run_ms_after("35 22 * * *", during).expect("valid cron");
        let next_late_dt = DateTime::from_timestamp_millis(next_late)
            .unwrap()
            .with_timezone(&Local);
        assert_eq!(next_late_dt.day(), 16, "next after 22:35:08 should be next day");

        let lookback = during.saturating_sub(NEVER_FIRED_AFTER_LOOKBACK_MS);
        let next_ok = next_run_ms_after("35 22 * * *", lookback).expect("valid cron");
        let next_ok_dt = DateTime::from_timestamp_millis(next_ok)
            .unwrap()
            .with_timezone(&Local);
        assert_eq!(next_ok_dt.day(), 15);
        assert_eq!((next_ok_dt.hour(), next_ok_dt.minute()), (22, 35));
        assert!(next_ok <= during, "today's 22:35:00 should be <= now for firing");
    }
}
