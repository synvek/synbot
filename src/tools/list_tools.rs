//! list_tools — returns the current set of available tools (name + description).
//!
//! Registered after all other tools (including MCP and plugin tools) so the agent
//! can answer "what tools do you have?" / "列举可用工具" with the real list.

use anyhow::Result;
use serde_json::{json, Value};

use crate::tools::{DynTool, ToolInfo};

/// Tool that returns a formatted list of all registered tools (name and description).
/// Use when the user asks what tools are available, to list tools, or 列举可用工具.
pub struct ListToolsTool {
    /// Snapshot of tool names and descriptions at registration time.
    tools: Vec<ToolInfo>,
}

impl ListToolsTool {
    /// Build from a snapshot of the registry (e.g. from `registry.list_tools()`).
    /// Include this tool itself in the output so the list is complete.
    pub fn new(mut tools: Vec<ToolInfo>) -> Self {
        tools.push(ToolInfo {
            name: "list_tools".to_string(),
            description: "List all available tools (name and short description). Use when the user asks what tools you have or to list/enumerate tools.".to_string(),
            parameters_schema: json!({"type": "object", "properties": {}, "required": []}),
        });
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        Self { tools }
    }
}

#[async_trait::async_trait]
impl DynTool for ListToolsTool {
    fn name(&self) -> &str {
        "list_tools"
    }

    fn description(&self) -> &str {
        "List all available tools (name and short description). Call this when the user asks what tools you have, what you can do, or to list/enumerate available tools (e.g. 列举可用工具)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn call(&self, _args: Value) -> Result<String> {
        let mut lines: Vec<String> = Vec::with_capacity(self.tools.len());
        for t in &self.tools {
            let desc = t.description.lines().next().unwrap_or(&t.description).trim();
            lines.push(format!("- **{}**: {}", t.name, desc));
        }
        Ok(lines.join("\n"))
    }
}
