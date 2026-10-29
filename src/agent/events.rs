//! Structured agent events for streaming UIs (web workbench / Tauri desktop).
//!
//! Two contracts exist on purpose:
//!
//! - [`crate::bus::OutboundMessage`] is the **channel-facing** contract: final text, approval
//!   requests and coarse tool progress. Every channel (Telegram, Discord, ...) consumes it.
//! - [`AgentEvent`] is the **UI-facing** contract: turn lifecycle, token deltas, tool call
//!   lifecycle, plan updates, usage. The web workbench subscribes through [`EventBus`].
//!
//! Both are emitted by the agent loop, so channels keep working unchanged while the UI gains
//! streaming without a second execution path.
//!
//! [`EventBus`] keeps a bounded ring buffer so a client that reconnects can replay the events
//! it missed (`seq` based), which is what makes "stream survives a page refresh" possible.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use crate::tools::approval::ApprovalRequest;

/// Ring buffer / broadcast capacity. Deltas are token-level, so keep this generous:
/// a single long answer can produce several hundred events.
pub const DEFAULT_EVENT_CAPACITY: usize = 2048;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeKind {
    Created,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TurnFinishReason {
    /// The model produced a final answer (no further tool calls).
    Completed,
    /// Stopped by the user (`/stop`, `cancel`).
    Cancelled,
    /// Hit `mainAgent.maxToolIterations` (productive model rounds).
    MaxIterations,
    /// Hit `mainAgent.maxIdleIterations`: the turn kept repeating calls without making progress.
    NoProgress,
    /// Hit `mainAgent.maxConsecutiveToolErrors`.
    MaxToolErrors,
    /// Provider/completion error surfaced to the user.
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanStepStatus {
    Pending,
    InProgress,
    Completed,
    /// Dropped on purpose (the goal changed), as opposed to never started.
    Cancelled,
}

/// One step of a plan/todo list. Emitted by `PlanUpdated` so the UI can render a task checklist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    pub id: String,
    pub title: String,
    pub status: PlanStepStatus,
}

/// Lifecycle of a subagent (`spawn` / `run_team` / directive routing).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubagentState {
    Running,
    Completed,
    Failed,
    /// Stopped by `cancel_subagent`, `/stop`, or a parent turn that was cancelled.
    Cancelled,
}

/// One subagent as the UI sees it: enough to render the team panel without polling the manager.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubagentInfo {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub state: SubagentState,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    /// Latest tool the subagent used, or the final result/error preview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<String>,
}

/// UI-facing agent event. Serialized as `{"type": "...", ...}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// A new turn started (one user message → one final answer, possibly with tool calls).
    TurnStarted { turn_id: String, agent_id: String },
    /// Incremental assistant text. Concatenate in order to get the message body.
    MessageDelta { turn_id: String, delta: String },
    /// Incremental reasoning/thinking text (providers that expose it).
    ReasoningDelta { turn_id: String, delta: String },
    /// A tool call is about to execute. `args_preview` is already truncated for display.
    ToolCallStarted {
        turn_id: String,
        call_id: String,
        tool_name: String,
        args_preview: String,
    },
    /// A tool call finished. `result_preview` is truncated to `tool_result_preview_chars`.
    ToolCallFinished {
        turn_id: String,
        call_id: String,
        tool_name: String,
        status: ToolCallStatus,
        result_preview: String,
        duration_ms: u64,
    },
    /// The agent (or a `plan` tool) republished its plan.
    PlanUpdated {
        turn_id: String,
        steps: Vec<PlanStep>,
    },
    /// A tool call needs human approval before it can proceed.
    ApprovalRequested {
        turn_id: String,
        request: ApprovalRequest,
    },
    /// A workspace file was created/modified/deleted by a tool.
    FileChanged {
        turn_id: String,
        path: String,
        change_kind: FileChangeKind,
    },
    /// Token usage for the turn so far (only emitted when the provider reports it).
    Usage {
        turn_id: String,
        prompt_tokens: u64,
        completion_tokens: u64,
        total_tokens: u64,
    },
    /// A subagent was spawned, made progress, or finished. Carries the whole snapshot so the
    /// team panel needs no extra round-trip.
    SubagentUpdated { subagent: SubagentInfo },
    /// The turn ended. `reason` tells the UI whether to show a retry affordance.
    TurnFinished {
        turn_id: String,
        reason: TurnFinishReason,
        iterations: u32,
    },
    /// A recoverable or terminal error for this turn.
    Error { turn_id: String, message: String },
}

/// An [`AgentEvent`] stamped with a monotonic sequence number and its conversation key.
///
/// `channel` + `chat_id` let a subscriber filter to one conversation, mirroring
/// [`crate::bus::InboundMessage::session_key`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequencedEvent {
    pub seq: u64,
    pub channel: String,
    pub chat_id: String,
    pub event: AgentEvent,
}

/// Broadcast + replayable event bus for agent events.
///
/// Cheap to clone via `Arc`; one instance is created at startup and shared by the agent loop
/// and the web layer.
#[derive(Debug)]
pub struct EventBus {
    tx: broadcast::Sender<SequencedEvent>,
    seq: AtomicU64,
    history: Mutex<VecDeque<SequencedEvent>>,
    capacity: usize,
}

impl EventBus {
    /// Create a bus wrapped in `Arc` with [`DEFAULT_EVENT_CAPACITY`].
    pub fn new() -> Arc<Self> {
        Self::with_capacity(DEFAULT_EVENT_CAPACITY)
    }

    /// Create a bus wrapped in `Arc` with an explicit ring-buffer capacity.
    pub fn with_capacity(capacity: usize) -> Arc<Self> {
        let capacity = capacity.max(1);
        let (tx, _) = broadcast::channel(capacity);
        Arc::new(Self {
            tx,
            seq: AtomicU64::new(0),
            history: Mutex::new(VecDeque::with_capacity(capacity.min(256))),
            capacity,
        })
    }

    /// Publish an event and return its sequence number.
    ///
    /// Publishing is fire-and-forget: a lagging subscriber does not block the agent loop.
    pub fn publish(&self, channel: &str, chat_id: &str, event: AgentEvent) -> u64 {
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let sequenced = SequencedEvent {
            seq,
            channel: channel.to_string(),
            chat_id: chat_id.to_string(),
            event,
        };

        if let Ok(mut history) = self.history.lock() {
            while history.len() >= self.capacity {
                history.pop_front();
            }
            history.push_back(sequenced.clone());
        }

        let _ = self.tx.send(sequenced);
        seq
    }

    /// Subscribe to live events.
    pub fn subscribe(&self) -> broadcast::Receiver<SequencedEvent> {
        self.tx.subscribe()
    }

    /// Last published sequence number (`0` when nothing was published yet).
    pub fn last_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }

    /// Replay buffered events for one conversation with `seq > after_seq`.
    ///
    /// Returns an empty vector when `after_seq` already covers the buffer (or the buffer has
    /// evicted the requested range — callers should then fall back to history from the session).
    pub fn replay_from(&self, channel: &str, chat_id: &str, after_seq: u64) -> Vec<SequencedEvent> {
        let history = match self.history.lock() {
            Ok(h) => h,
            Err(_) => return Vec::new(),
        };
        history
            .iter()
            .filter(|e| e.seq > after_seq && e.channel == channel && e.chat_id == chat_id)
            .cloned()
            .collect()
    }

    /// Number of buffered events (for diagnostics/tests).
    pub fn buffered(&self) -> usize {
        self.history.lock().map(|h| h.len()).unwrap_or(0)
    }
}

impl Default for EventBus {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(DEFAULT_EVENT_CAPACITY);
        Self {
            tx,
            seq: AtomicU64::new(0),
            history: Mutex::new(VecDeque::new()),
            capacity: DEFAULT_EVENT_CAPACITY,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(turn: &str, text: &str) -> AgentEvent {
        AgentEvent::MessageDelta {
            turn_id: turn.to_string(),
            delta: text.to_string(),
        }
    }

    #[test]
    fn seq_is_monotonic_and_starts_at_one() {
        let bus = EventBus::with_capacity(8);
        assert_eq!(bus.last_seq(), 0);
        assert_eq!(bus.publish("web", "u1", delta("t1", "a")), 1);
        assert_eq!(bus.publish("web", "u1", delta("t1", "b")), 2);
        assert_eq!(bus.last_seq(), 2);
    }

    #[test]
    fn ring_buffer_evicts_oldest() {
        let bus = EventBus::with_capacity(2);
        bus.publish("web", "u1", delta("t1", "a"));
        bus.publish("web", "u1", delta("t1", "b"));
        bus.publish("web", "u1", delta("t1", "c"));
        assert_eq!(bus.buffered(), 2);
        let replayed = bus.replay_from("web", "u1", 0);
        assert_eq!(replayed.len(), 2);
        assert_eq!(replayed[0].seq, 2);
    }

    #[test]
    fn replay_filters_by_conversation_and_seq() {
        let bus = EventBus::with_capacity(16);
        bus.publish("web", "u1", delta("t1", "a"));
        bus.publish("web", "u2", delta("t2", "b"));
        bus.publish("web", "u1", delta("t1", "c"));
        bus.publish("telegram", "u1", delta("t3", "d"));

        let replayed = bus.replay_from("web", "u1", 0);
        assert_eq!(replayed.len(), 2);
        assert_eq!(replayed[0].seq, 1);
        assert_eq!(replayed[1].seq, 3);

        let tail = bus.replay_from("web", "u1", 1);
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].seq, 3);
        assert!(bus.replay_from("web", "u1", 3).is_empty());
    }

    #[test]
    fn serializes_with_type_tag() {
        let json = serde_json::to_string(&delta("t1", "hi")).unwrap();
        assert!(
            json.contains("\"type\":\"message_delta\""),
            "json was {json}"
        );
        assert!(json.contains("\"delta\":\"hi\""));

        let plan = AgentEvent::PlanUpdated {
            turn_id: "t1".to_string(),
            steps: vec![PlanStep {
                id: "1".to_string(),
                title: "read code".to_string(),
                status: PlanStepStatus::InProgress,
            }],
        };
        let json = serde_json::to_string(&plan).unwrap();
        assert!(json.contains("\"type\":\"plan_updated\""));
        assert!(json.contains("\"status\":\"in_progress\""));

        let subagent = AgentEvent::SubagentUpdated {
            subagent: SubagentInfo {
                id: "ab12cd34".to_string(),
                label: "scan auth".to_string(),
                role: Some("reviewer".to_string()),
                state: SubagentState::Failed,
                created_at: Utc::now(),
                finished_at: None,
                activity: Some("tool: search_text".to_string()),
            },
        };
        let json = serde_json::to_string(&subagent).unwrap();
        assert!(json.contains("\"type\":\"subagent_updated\""));
        assert!(json.contains("\"state\":\"failed\""));
        // A running subagent has no `finished_at`, so the field is omitted rather than null.
        assert!(!json.contains("finished_at"), "json was {json}");

        // Every variant must round-trip, so the UI contract stays in sync.
        let envelope = SequencedEvent {
            seq: 7,
            channel: "web".to_string(),
            chat_id: "web_admin".to_string(),
            event: AgentEvent::TurnFinished {
                turn_id: "t1".to_string(),
                reason: TurnFinishReason::Cancelled,
                iterations: 2,
            },
        };
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("\"seq\":7"));
        assert!(json.contains("\"reason\":\"cancelled\""));
        let parsed: SequencedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.seq, 7);
    }

    #[tokio::test]
    async fn subscribers_receive_published_events() {
        let bus = EventBus::with_capacity(8);
        let mut rx = bus.subscribe();
        bus.publish("web", "u1", delta("t1", "hello"));
        let received = rx.recv().await.expect("event");
        assert_eq!(received.seq, 1);
        assert_eq!(received.channel, "web");
        match received.event {
            AgentEvent::MessageDelta { delta, .. } => assert_eq!(delta, "hello"),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
