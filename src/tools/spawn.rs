//! Spawn tool — placeholder for subagent spawning.

use anyhow::Result;
use serde_json::{json, Value};

use crate::tools::DynTool;

pub struct SpawnTool;

#[async_trait::async_trait]
impl DynTool for SpawnTool {
    fn name(&self) -> &str { "spawn" }
    fn description(&self) -> &str {
        "Spawn a subagent to handle a task in the background."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "Task description" },
                "label": { "type": "string", "description": "Human-readable label" }
            },
            "required": ["task"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let task = args["task"].as_str().unwrap_or("");
        let label = args["label"].as_str().unwrap_or(&task[..task.len().min(30)]);
        // TODO: integrate with SubagentManager
        Ok(format!("Subagent [{}] queued (not yet implemented in Rust rewrite).", label))
    }
}
