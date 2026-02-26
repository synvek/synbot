//! Hooks API — lifecycle events for external observers (plugins, logging, metrics).
//!
//! Hooks are notified of message received/sent, tool runs, and agent runs. They do not replace
//! the MessageBus; they are observation-only.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::bus::{InboundMessage, OutboundMessage};
use crate::tools::approval::{ApprovalRequest, ApprovalResponse};

// ---------------------------------------------------------------------------
// Hook events
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HookEvent {
    /// User message received (before agent processing).
    MessageReceived(InboundMessage),

    /// Chat/approval/tool_progress message sent to channels.
    MessageSent(OutboundMessage),

    /// Tool execution started.
    ToolRunStart {
        tool_name: String,
        args_preview: String,
        channel: String,
        chat_id: String,
        session_id: String,
    },

    /// Tool execution finished.
    ToolRunEnd {
        tool_name: String,
        result_preview: String,
        success: bool,
    },

    /// Agent run started (before completion loop).
    AgentRunStart {
        agent_id: String,
        directive_preview: String,
    },

    /// Agent run finished.
    AgentRunEnd {
        agent_id: String,
        iteration_count: u32,
        duration_ms: u64,
    },

    /// Approval requested (e.g. from exec tool).
    ApprovalRequested(ApprovalRequest),

    /// User responded to an approval request.
    ApprovalResponded(ApprovalResponse),
}

// ---------------------------------------------------------------------------
// Hook trait and registry
// ---------------------------------------------------------------------------

/// A hook that receives lifecycle events. Implementations should be non-blocking and fast;
/// heavy work should be spawned off.
#[async_trait]
pub trait Hook: Send + Sync {
    async fn on_event(&self, event: HookEvent);
}

/// Registry that holds all registered hooks and dispatches events to them.
#[derive(Clone, Default)]
pub struct HookRegistry {
    hooks: Arc<tokio::sync::RwLock<Vec<Arc<dyn Hook>>>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            hooks: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        }
    }

    /// Register a hook. Hooks are invoked in registration order.
    pub async fn register(&self, hook: Arc<dyn Hook>) {
        let mut guard = self.hooks.write().await;
        guard.push(hook);
    }

    /// Dispatch an event to all registered hooks. Returns immediately after spawning a
    /// background task so the agent loop is not blocked. Hooks run sequentially in the
    /// background.
    pub async fn dispatch(&self, event: HookEvent) {
        let hooks = {
            let guard = self.hooks.read().await;
            guard.clone()
        };
        if hooks.is_empty() {
            return;
        }
        tokio::spawn(async move {
            for hook in hooks {
                hook.on_event(event.clone()).await;
            }
        });
    }

    /// Return true if any hooks are registered (avoids allocating event payloads when no one listens).
    pub async fn is_empty(&self) -> bool {
        let guard = self.hooks.read().await;
        guard.is_empty()
    }
}
