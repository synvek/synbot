//! Process-wide security audit trail.
//!
//! Enforcement decisions (a blocked command, a rejected approval, a plugin asking for something
//! its manifest does not grant) used to exist only as log lines scattered across `tracing`. This
//! module keeps the last [`DEFAULT_CAPACITY`] of them in memory so the Web UI can answer "what has
//! this agent been stopped from doing?" without grepping a log file.
//!
//! It is deliberately **not** persistent: the trail is a debugging and transparency tool, not a
//! compliance archive. Anything that needs to survive a restart belongs in the real log.
//!
//! ## Usage
//!
//! ```ignore
//! use crate::security::audit::{record, SecurityKind, SecurityOutcome};
//!
//! record(
//!     SecurityKind::CommandDenied,
//!     SecurityOutcome::Blocked,
//!     &cmd,
//!     "matched policy rule `rm -rf *`",
//! );
//! ```

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, Utc};
use serde::Serialize;

/// How many events are retained before the oldest ones are dropped.
pub const DEFAULT_CAPACITY: usize = 500;

/// What kind of enforcement decision happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityKind {
    /// `exec` rejected by `denyPatterns` / not matching `allowPatterns`.
    CommandBlocked,
    /// `exec` rejected by the permission policy (`deny` level).
    CommandDenied,
    /// The command needed a human but no approval channel was available.
    ApprovalUnavailable,
    /// The command was escalated to a human and the request is in flight.
    ApprovalRequested,
    /// The user approved the command.
    ApprovalApproved,
    /// The user declined the command.
    ApprovalRejected,
    /// Nobody answered the approval request in time.
    ApprovalTimeout,
    /// The active profile requires a tool sandbox and none is available.
    SandboxRequired,
    /// A file tool was pointed outside the workspace.
    PathEscape,
    /// A plugin hit the boundary declared in its manifest.
    PluginPermission,
}

impl SecurityKind {
    /// All variants, in the order the UI lists them.
    pub const ALL: [SecurityKind; 10] = [
        SecurityKind::CommandBlocked,
        SecurityKind::CommandDenied,
        SecurityKind::ApprovalUnavailable,
        SecurityKind::ApprovalRequested,
        SecurityKind::ApprovalApproved,
        SecurityKind::ApprovalRejected,
        SecurityKind::ApprovalTimeout,
        SecurityKind::SandboxRequired,
        SecurityKind::PathEscape,
        SecurityKind::PluginPermission,
    ];

    /// Serialized name, matching the JSON the API emits.
    pub fn key(self) -> &'static str {
        match self {
            SecurityKind::CommandBlocked => "command_blocked",
            SecurityKind::CommandDenied => "command_denied",
            SecurityKind::ApprovalUnavailable => "approval_unavailable",
            SecurityKind::ApprovalRequested => "approval_requested",
            SecurityKind::ApprovalApproved => "approval_approved",
            SecurityKind::ApprovalRejected => "approval_rejected",
            SecurityKind::ApprovalTimeout => "approval_timeout",
            SecurityKind::SandboxRequired => "sandbox_required",
            SecurityKind::PathEscape => "path_escape",
            SecurityKind::PluginPermission => "plugin_permission",
        }
    }

    /// Parse the query-string form of [`SecurityKind::key`].
    pub fn from_key(key: &str) -> Option<Self> {
        SecurityKind::ALL.into_iter().find(|kind| kind.key() == key)
    }
}

/// How the decision ended for the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityOutcome {
    /// The action did not run.
    Blocked,
    /// The action is waiting on (or was decided by) a human.
    NeedsUser,
    /// The action ran because a human allowed it.
    Allowed,
}

/// One enforcement decision.
#[derive(Debug, Clone, Serialize)]
pub struct SecurityEvent {
    /// Monotonic per-process id; lets the UI de-duplicate a polled snapshot.
    pub id: u64,
    pub at: DateTime<Utc>,
    pub kind: SecurityKind,
    pub outcome: SecurityOutcome,
    /// The thing that was judged: a command, a path, a plugin id.
    pub subject: String,
    /// Why it was judged that way (rule name, boundary, refusing party).
    pub detail: String,
}

/// Aggregated event count for one kind.
#[derive(Debug, Clone, Serialize)]
pub struct SecurityCount {
    pub kind: SecurityKind,
    pub count: u64,
}

/// Bounded, thread-safe log of [`SecurityEvent`]s.
#[derive(Debug)]
pub struct SecurityAudit {
    capacity: usize,
    events: Mutex<VecDeque<SecurityEvent>>,
    next_id: AtomicU64,
    /// Events evicted by the capacity limit since the last [`SecurityAudit::clear`].
    dropped: AtomicU64,
}

impl Default for SecurityAudit {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

impl SecurityAudit {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            events: Mutex::new(VecDeque::with_capacity(capacity.min(1024))),
            next_id: AtomicU64::new(1),
            dropped: AtomicU64::new(0),
        }
    }

    /// Append an event, evicting the oldest one when the buffer is full.
    pub fn record(
        &self,
        kind: SecurityKind,
        outcome: SecurityOutcome,
        subject: impl Into<String>,
        detail: impl Into<String>,
    ) -> SecurityEvent {
        let event = SecurityEvent {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            at: Utc::now(),
            kind,
            outcome,
            subject: truncate(subject.into(), 512),
            detail: truncate(detail.into(), 1024),
        };
        // Poisoning only means a previous holder panicked mid-push; the deque is still consistent
        // enough to use, and losing the audit trail on a panic would be the worse outcome.
        let mut events = self.events.lock().unwrap_or_else(|e| e.into_inner());
        if events.len() >= self.capacity {
            events.pop_front();
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        events.push_back(event.clone());
        event
    }

    /// Newest-first snapshot, optionally filtered by kind.
    pub fn snapshot(&self, kind: Option<SecurityKind>, limit: usize) -> Vec<SecurityEvent> {
        let events = self.events.lock().unwrap_or_else(|e| e.into_inner());
        events
            .iter()
            .rev()
            .filter(|event| kind.map(|k| k == event.kind).unwrap_or(true))
            .take(limit)
            .cloned()
            .collect()
    }

    /// Per-kind totals over the whole buffer, biggest first.
    pub fn counts(&self) -> Vec<SecurityCount> {
        let events = self.events.lock().unwrap_or_else(|e| e.into_inner());
        let mut counts: Vec<SecurityCount> = SecurityKind::ALL
            .iter()
            .map(|kind| SecurityCount {
                kind: *kind,
                count: events.iter().filter(|e| e.kind == *kind).count() as u64,
            })
            .filter(|count| count.count > 0)
            .collect();
        counts.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| format!("{:?}", a.kind).cmp(&format!("{:?}", b.kind)))
        });
        counts
    }

    pub fn len(&self) -> usize {
        self.events.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Events evicted since the last [`SecurityAudit::clear`].
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn clear(&self) {
        let mut events = self.events.lock().unwrap_or_else(|e| e.into_inner());
        events.clear();
        self.dropped.store(0, Ordering::Relaxed);
    }
}

/// Keep one subject/detail bounded so a huge command cannot pin memory.
fn truncate(text: String, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text;
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}

static GLOBAL: OnceLock<SecurityAudit> = OnceLock::new();

/// The process-wide audit trail shared by tools, plugins and the web layer.
pub fn audit() -> &'static SecurityAudit {
    GLOBAL.get_or_init(SecurityAudit::default)
}

/// Shorthand for [`SecurityAudit::record`] on the global trail.
pub fn record(
    kind: SecurityKind,
    outcome: SecurityOutcome,
    subject: impl Into<String>,
    detail: impl Into<String>,
) {
    audit().record(kind, outcome, subject, detail);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(audit: &SecurityAudit) -> Vec<SecurityKind> {
        audit.snapshot(None, 100).iter().map(|e| e.kind).collect()
    }

    #[test]
    fn snapshot_is_newest_first_and_bounded() {
        let audit = SecurityAudit::new(10);
        for i in 0..3 {
            audit.record(
                SecurityKind::PathEscape,
                SecurityOutcome::Blocked,
                format!("file-{i}"),
                "outside workspace",
            );
        }
        let snapshot = audit.snapshot(None, 2);
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].subject, "file-2");
        assert_eq!(snapshot[1].subject, "file-1");
        assert!(snapshot[0].id > snapshot[1].id);
    }

    #[test]
    fn capacity_evicts_the_oldest_and_counts_drops() {
        let audit = SecurityAudit::new(2);
        for i in 0..4 {
            audit.record(
                SecurityKind::CommandDenied,
                SecurityOutcome::Blocked,
                format!("cmd-{i}"),
                "policy",
            );
        }
        assert_eq!(audit.len(), 2);
        assert_eq!(audit.dropped(), 2);
        assert_eq!(
            kinds(&audit),
            vec![SecurityKind::CommandDenied, SecurityKind::CommandDenied]
        );
        assert_eq!(audit.snapshot(None, 5)[0].subject, "cmd-3");
    }

    #[test]
    fn filtering_and_counts() {
        let audit = SecurityAudit::new(50);
        audit.record(
            SecurityKind::CommandDenied,
            SecurityOutcome::Blocked,
            "rm -rf /",
            "deny pattern",
        );
        audit.record(
            SecurityKind::PluginPermission,
            SecurityOutcome::Blocked,
            "demo",
            "permission denied: network host not in the manifest allowlist",
        );
        audit.record(
            SecurityKind::CommandDenied,
            SecurityOutcome::Blocked,
            "curl evil",
            "permission policy",
        );

        let denied = audit.snapshot(Some(SecurityKind::CommandDenied), 10);
        assert_eq!(denied.len(), 2);
        assert!(denied.iter().all(|e| e.kind == SecurityKind::CommandDenied));

        let counts = audit.counts();
        assert_eq!(counts.len(), 2);
        assert_eq!(counts[0].kind, SecurityKind::CommandDenied);
        assert_eq!(counts[0].count, 2);
    }

    #[test]
    fn clear_resets_events_and_dropped() {
        let audit = SecurityAudit::new(1);
        audit.record(
            SecurityKind::SandboxRequired,
            SecurityOutcome::Blocked,
            "exec",
            "no backend",
        );
        audit.record(
            SecurityKind::SandboxRequired,
            SecurityOutcome::Blocked,
            "exec",
            "no backend",
        );
        assert_eq!(audit.dropped(), 1);
        audit.clear();
        assert!(audit.is_empty());
        assert_eq!(audit.dropped(), 0);
        assert!(audit.counts().is_empty());
        // Ids keep increasing so the UI never mistakes a new event for an old one.
        let next = audit.record(SecurityKind::PathEscape, SecurityOutcome::Blocked, "p", "d");
        assert_eq!(next.id, 3);
    }

    #[test]
    fn kind_keys_round_trip() {
        for kind in SecurityKind::ALL {
            assert_eq!(SecurityKind::from_key(kind.key()), Some(kind));
            // The serde name must stay in sync with `key()`.
            let json = serde_json::to_value(kind).unwrap();
            assert_eq!(json.as_str().unwrap(), kind.key());
        }
        assert!(SecurityKind::from_key("nope").is_none());
    }

    #[test]
    fn subjects_are_truncated() {
        let audit = SecurityAudit::new(4);
        let event = audit.record(
            SecurityKind::CommandDenied,
            SecurityOutcome::Blocked,
            "x".repeat(1000),
            "y".repeat(2000),
        );
        assert_eq!(event.subject.chars().count(), 513);
        assert_eq!(event.detail.chars().count(), 1025);
        assert!(event.subject.ends_with('…'));
    }
}
