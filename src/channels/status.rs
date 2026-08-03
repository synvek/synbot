use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

/// Runtime state reported by a channel instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeChannelStatus {
    Starting,
    Connected,
    Reconnecting,
    Failed,
    Disabled,
    Stopped,
}

/// Serializable point-in-time state for one configured channel instance.
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeChannelSnapshot {
    pub instance_id: String,
    pub channel_type: String,
    pub name: String,
    pub enabled: bool,
    pub status: RuntimeChannelStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub last_connected_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub reconnect_count: u64,
    pub last_received_at: Option<DateTime<Utc>>,
    pub last_sent_at: Option<DateTime<Utc>>,
    pub last_latency_ms: Option<u64>,
    pub supports_send: bool,
    pub supports_receive: bool,
}

#[derive(Debug, Clone)]
pub struct ChannelStatusHandle {
    registry: Arc<ChannelStatusRegistry>,
    instance_id: String,
}

impl ChannelStatusHandle {
    pub fn detached() -> Self {
        let registry = Arc::new(ChannelStatusRegistry::new());
        registry.register(
            "__detached__".to_string(),
            "unknown".to_string(),
            "unknown".to_string(),
            false,
            false,
            false,
        )
    }

    pub fn mark_starting(&self) {
        self.registry.update(&self.instance_id, |entry| {
            entry.status = RuntimeChannelStatus::Starting;
            entry.started_at.get_or_insert_with(Utc::now);
        });
    }

    pub fn mark_connected(&self) {
        self.registry.update(&self.instance_id, |entry| {
            entry.status = RuntimeChannelStatus::Connected;
            entry.last_connected_at = Some(Utc::now());
            entry.last_error = None;
        });
    }

    pub fn mark_reconnecting(&self, error: Option<&str>) {
        self.registry.update(&self.instance_id, |entry| {
            entry.status = RuntimeChannelStatus::Reconnecting;
            entry.reconnect_count = entry.reconnect_count.saturating_add(1);
            if let Some(error) = error {
                entry.last_error = Some(error.to_string());
            }
        });
    }

    pub fn mark_failed(&self, error: impl Into<String>) {
        self.registry.update(&self.instance_id, |entry| {
            entry.status = RuntimeChannelStatus::Failed;
            entry.last_error = Some(error.into());
        });
    }

    pub fn mark_stopped(&self) {
        self.registry.update(&self.instance_id, |entry| {
            entry.status = RuntimeChannelStatus::Stopped;
        });
    }

    pub fn record_received(&self) {
        self.registry.update(&self.instance_id, |entry| {
            entry.last_received_at = Some(Utc::now());
        });
    }

    pub fn record_sent(&self) {
        self.registry.update(&self.instance_id, |entry| {
            entry.last_sent_at = Some(Utc::now());
        });
    }

    pub fn record_latency(&self, latency_ms: u64) {
        self.registry.update(&self.instance_id, |entry| {
            entry.last_latency_ms = Some(latency_ms);
        });
    }

    pub fn record_sent_with_latency(&self, latency_ms: u64) {
        self.registry.update(&self.instance_id, |entry| {
            entry.last_sent_at = Some(Utc::now());
            entry.last_latency_ms = Some(latency_ms);
        });
    }

    /// Preserve the latest operational error without changing the connection state.
    pub fn record_error(&self, error: impl Into<String>) {
        self.registry.update(&self.instance_id, |entry| {
            entry.last_error = Some(error.into());
        });
    }
}

#[derive(Debug)]
struct RuntimeChannelEntry {
    snapshot: RuntimeChannelSnapshot,
}

/// Process-wide registry shared by channel tasks and the web API.
#[derive(Debug)]
pub struct ChannelStatusRegistry {
    process_started_at: DateTime<Utc>,
    daemon_started_at: RwLock<Option<DateTime<Utc>>>,
    daemon_running: AtomicBool,
    entries: RwLock<HashMap<String, RuntimeChannelEntry>>,
}

impl ChannelStatusRegistry {
    pub fn new() -> Self {
        Self {
            process_started_at: Utc::now(),
            daemon_started_at: RwLock::new(None),
            daemon_running: AtomicBool::new(false),
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Mark the daemon as actively running. This is separate from registering channels so
    /// startup failures cannot be reported as a healthy daemon.
    pub fn mark_running(&self) {
        let mut started_at = self
            .daemon_started_at
            .write()
            .expect("channel status registry poisoned");
        if started_at.is_none() {
            *started_at = Some(Utc::now());
        }
        self.daemon_running.store(true, Ordering::Release);
    }

    /// Mark the daemon as stopped and transition active channel instances to stopped.
    pub fn mark_shutdown(&self) {
        self.daemon_running.store(false, Ordering::Release);
        let mut entries = self
            .entries
            .write()
            .expect("channel status registry poisoned");
        for entry in entries.values_mut() {
            if entry.snapshot.enabled && entry.snapshot.status != RuntimeChannelStatus::Disabled {
                entry.snapshot.status = RuntimeChannelStatus::Stopped;
            }
        }
    }

    pub fn register(
        self: &Arc<Self>,
        instance_id: String,
        channel_type: String,
        name: String,
        enabled: bool,
        supports_send: bool,
        supports_receive: bool,
    ) -> ChannelStatusHandle {
        let initial_status = if enabled {
            RuntimeChannelStatus::Starting
        } else {
            RuntimeChannelStatus::Disabled
        };
        let mut entries = self.entries.write().expect("channel status registry poisoned");
        entries.insert(
            instance_id.clone(),
            RuntimeChannelEntry {
                snapshot: RuntimeChannelSnapshot {
                    instance_id: instance_id.clone(),
                    channel_type,
                    name,
                    enabled,
                    status: initial_status,
                    started_at: None,
                    last_connected_at: None,
                    last_error: None,
                    reconnect_count: 0,
                    last_received_at: None,
                    last_sent_at: None,
                    last_latency_ms: None,
                    supports_send,
                    supports_receive,
                },
            },
        );
        ChannelStatusHandle {
            registry: Arc::clone(self),
            instance_id,
        }
    }

    fn update<F>(&self, instance_id: &str, update: F)
    where
        F: FnOnce(&mut RuntimeChannelSnapshot),
    {
        if let Some(entry) = self
            .entries
            .write()
            .expect("channel status registry poisoned")
            .get_mut(instance_id)
        {
            update(&mut entry.snapshot);
        }
    }

    pub fn snapshots(&self) -> Vec<RuntimeChannelSnapshot> {
        let mut snapshots: Vec<_> = self
            .entries
            .read()
            .expect("channel status registry poisoned")
            .values()
            .map(|entry| entry.snapshot.clone())
            .collect();
        snapshots.sort_by(|a, b| a.instance_id.cmp(&b.instance_id));
        snapshots
    }

    pub fn uptime_secs(&self) -> u64 {
        let started_at = self
            .daemon_started_at
            .read()
            .expect("channel status registry poisoned")
            .as_ref()
            .copied()
            .unwrap_or(self.process_started_at);
        Utc::now()
            .signed_duration_since(started_at)
            .num_seconds()
            .max(0) as u64
    }

    pub fn is_running(&self) -> bool {
        self.daemon_running.load(Ordering::Acquire)
    }
}

impl Default for ChannelStatusRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_lifecycle_is_not_always_running() {
        let registry = Arc::new(ChannelStatusRegistry::new());
        assert!(!registry.is_running());
        registry.mark_running();
        assert!(registry.is_running());
        registry.mark_shutdown();
        assert!(!registry.is_running());
    }

    #[test]
    fn snapshots_report_runtime_events_and_capabilities() {
        let registry = Arc::new(ChannelStatusRegistry::new());
        let handle = registry.register(
            "telegram:0:alerts".into(),
            "telegram".into(),
            "alerts".into(),
            true,
            true,
            true,
        );
        handle.mark_starting();
        handle.mark_reconnecting(Some("temporary failure"));
        handle.record_received();
        handle.record_sent();
        handle.record_latency(42);
        let snapshot = registry.snapshots().pop().expect("snapshot");
        assert_eq!(snapshot.status, RuntimeChannelStatus::Reconnecting);
        assert_eq!(snapshot.reconnect_count, 1);
        assert_eq!(snapshot.last_error.as_deref(), Some("temporary failure"));
        assert!(snapshot.started_at.is_some());
        assert!(snapshot.last_received_at.is_some());
        assert!(snapshot.last_sent_at.is_some());
        assert_eq!(snapshot.last_latency_ms, Some(42));
        assert!(snapshot.supports_send);
        assert!(snapshot.supports_receive);
    }

    #[test]
    fn shutdown_stops_active_channels_but_keeps_disabled_channels_disabled() {
        let registry = Arc::new(ChannelStatusRegistry::new());
        let active = registry.register("active".into(), "x".into(), "active".into(), true, true, true);
        let _disabled = registry.register("disabled".into(), "x".into(), "disabled".into(), false, true, true);
        active.mark_connected();
        registry.mark_shutdown();
        let snapshots = registry.snapshots();
        assert_eq!(snapshots[0].status, RuntimeChannelStatus::Stopped);
        assert_eq!(snapshots[1].status, RuntimeChannelStatus::Disabled);
    }
}
