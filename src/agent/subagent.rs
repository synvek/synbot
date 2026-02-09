//! Subagent manager — background task execution (placeholder).

use std::collections::HashMap;
use tokio::task::JoinHandle;

pub struct SubagentManager {
    running: HashMap<String, JoinHandle<()>>,
}

impl SubagentManager {
    pub fn new() -> Self {
        Self {
            running: HashMap::new(),
        }
    }

    /// Number of currently running subagents.
    pub fn active_count(&mut self) -> usize {
        self.running.retain(|_, h| !h.is_finished());
        self.running.len()
    }
}
