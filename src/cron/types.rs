//! Cron data types.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    /// One-shot at a specific timestamp (ms).
    At { at_ms: i64 },
    /// Recurring every N milliseconds.
    Every { every_ms: i64 },
    /// Cron expression (e.g. "0 9 * * *").
    Cron { expr: String, tz: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronPayload {
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub deliver: bool,
    pub channel: Option<String>,
    pub to: Option<String>,
}

fn default_kind() -> String { "agent_turn".into() }

impl Default for CronPayload {
    fn default() -> Self {
        Self {
            kind: default_kind(),
            message: String::new(),
            deliver: false,
            channel: None,
            to: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CronJobState {
    pub next_run_at_ms: Option<i64>,
    pub last_run_at_ms: Option<i64>,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJob {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub schedule: Schedule,
    #[serde(default)]
    pub payload: CronPayload,
    #[serde(default)]
    pub state: CronJobState,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
    #[serde(default)]
    pub delete_after_run: bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CronStore {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub jobs: Vec<CronJob>,
}

fn default_version() -> u32 { 1 }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cron_payload_default() {
        let p = CronPayload::default();
        assert_eq!(p.kind, "agent_turn");
        assert!(p.message.is_empty());
        assert!(!p.deliver);
        assert!(p.channel.is_none());
        assert!(p.to.is_none());
    }

    #[test]
    fn cron_job_state_default() {
        let s = CronJobState::default();
        assert!(s.next_run_at_ms.is_none());
        assert!(s.last_run_at_ms.is_none());
        assert!(s.last_status.is_none());
        assert!(s.last_error.is_none());
    }

    #[test]
    fn schedule_serialization_cron() {
        let s = Schedule::Cron {
            expr: "0 9 * * *".to_string(),
            tz: Some("UTC".to_string()),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("cron"));
        assert!(json.contains("0 9 * * *"));
        let parsed: Schedule = serde_json::from_str(&json).unwrap();
        match &parsed {
            Schedule::Cron { expr, tz } => {
                assert_eq!(expr, "0 9 * * *");
                assert_eq!(tz.as_deref(), Some("UTC"));
            }
            _ => panic!("expected Cron variant"),
        }
    }

    #[test]
    fn cron_store_default() {
        let store = CronStore::default();
        assert_eq!(store.version, 0); // Default derive gives 0; serde uses default_version() for deserialization
        assert!(store.jobs.is_empty());
    }
}
