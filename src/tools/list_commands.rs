//! list_commands — returns the set of user-facing slash commands.

use anyhow::Result;
use serde_json::{json, Value};

use crate::agent::control_commands::slash_commands_help_text;
use crate::tools::DynTool;

pub struct ListCommandsTool;

impl ListCommandsTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl DynTool for ListCommandsTool {
    fn name(&self) -> &str {
        "list_commands"
    }

    fn description(&self) -> &str {
        "List user-facing slash commands (e.g. /workflow, /stop, /status, /clear, /resume, /commands). Use when the user asks what chat commands are available."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(&self, _args: Value) -> Result<String> {
        Ok(slash_commands_help_text().to_string())
    }
}

