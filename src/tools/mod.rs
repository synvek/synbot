pub mod approval;
pub mod approval_store;
pub mod context;
pub mod filesystem;
pub mod memory_tool;
pub mod message;
pub mod permission;
pub mod shell;
pub mod spawn;
pub mod truncation;
pub mod web;

use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, debug};

pub use context::{scope, ToolContext};

/// Build a short string for logging tool args (path, command, etc.) without leaking full content.
fn sanitize_args_for_log(tool_name: &str, args: &Value) -> String {
    let obj = match args.as_object() {
        Some(o) => o,
        None => return "args=?".to_string(),
    };
    let part = match tool_name {
        "read_file" | "write_file" | "edit_file" | "list_dir" => obj
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| format!("path={}", truncate_for_log(s, 120))),
        "exec" => obj
            .get("command")
            .and_then(|v| v.as_str())
            .map(|s| format!("command={}", truncate_for_log(s, 80))),
        "remember" => obj
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| format!("content_len={}", s.len())),
        "list_memory" => obj
            .get("agent_id")
            .and_then(|v| v.as_str())
            .map(|s| format!("agent_id={}", s)),
        _ => None,
    };
    part.unwrap_or_else(|| "args=...".to_string())
}

fn truncate_for_log(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut end = max_len.min(s.len());
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}... ({} chars)", &s[..end], s.len())
    }
}

/// A type-erased tool that can be stored in the registry.
#[async_trait::async_trait]
pub trait DynTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    async fn call(&self, args: Value) -> Result<String>;
}

/// Metadata about a registered tool, returned by `list_tools`.
#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters_schema: Value,
}

/// Registry that holds all available tools.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn DynTool>>,
}


impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Returns an error if a tool with the same name already exists.
    pub fn register(&mut self, tool: Arc<dyn DynTool>) -> Result<()> {
        let name = tool.name().to_string();
        if self.tools.contains_key(&name) {
            anyhow::bail!("Tool '{}' is already registered", name);
        }
        self.tools.insert(name, tool);
        Ok(())
    }

    /// Remove a tool by name. Returns `Ok(true)` if the tool was found and removed,
    /// `Ok(false)` if no tool with that name was registered.
    pub fn deregister(&mut self, name: &str) -> Result<bool> {
        Ok(self.tools.remove(name).is_some())
    }

    /// Return metadata about all registered tools.
    pub fn list_tools(&self) -> Vec<ToolInfo> {
        self.tools
            .values()
            .map(|t| ToolInfo {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters_schema: t.parameters_schema(),
            })
            .collect()
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn DynTool>> {
        self.tools.get(name)
    }

    pub async fn execute(&self, name: &str, args: Value) -> Result<String> {
        let args_for_log = sanitize_args_for_log(name, &args);
        let span = tracing::info_span!(
            "tool_execution",
            tool_name = %name,
            args = %args_for_log,
        );
        let _guard = span.enter();

        debug!(tool_name = %name, args = ?args, "Tool call started");
        let start = std::time::Instant::now();
        let result = match self.tools.get(name) {
            Some(tool) => tool.call(args).await,
            None => anyhow::bail!("Tool '{}' not found", name),
        };
        let duration_ms = start.elapsed().as_millis() as u64;

        match &result {
            Ok(s) => {
                let result_preview = truncate_for_log(s, 200);
                info!(
                    tool_name = %name,
                    duration_ms = duration_ms,
                    status = "success",
                    result_preview = %result_preview,
                    "Tool execution completed"
                );
            }
            Err(e) => {
                info!(
                    tool_name = %name,
                    duration_ms = duration_ms,
                    status = "failure",
                    error = %e,
                    "Tool execution failed"
                );
            }
        }

        result
    }

    /// Return rig-compatible ToolDefinition list for the LLM.
    pub fn rig_definitions(&self) -> Vec<rig::completion::ToolDefinition> {
        self.tools
            .values()
            .map(|t| rig::completion::ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect()
    }

    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A minimal DynTool implementation for testing purposes.
    struct FakeTool {
        tool_name: String,
        tool_desc: String,
    }

    impl FakeTool {
        fn new(name: &str, desc: &str) -> Self {
            Self {
                tool_name: name.to_string(),
                tool_desc: desc.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl DynTool for FakeTool {
        fn name(&self) -> &str {
            &self.tool_name
        }
        fn description(&self) -> &str {
            &self.tool_desc
        }
        fn parameters_schema(&self) -> Value {
            json!({"type": "object", "properties": {}})
        }
        async fn call(&self, _args: Value) -> Result<String> {
            Ok(format!("{} called", self.tool_name))
        }
    }

    fn fake_tool(name: &str) -> Arc<dyn DynTool> {
        Arc::new(FakeTool::new(name, &format!("Description for {}", name)))
    }

    #[test]
    fn register_succeeds_for_new_tool() {
        let mut reg = ToolRegistry::new();
        assert!(reg.register(fake_tool("alpha")).is_ok());
        assert!(reg.get("alpha").is_some());
    }

    #[test]
    fn register_returns_error_on_duplicate_name() {
        let mut reg = ToolRegistry::new();
        reg.register(fake_tool("dup")).unwrap();
        let err = reg.register(fake_tool("dup"));
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("already registered"), "error message was: {msg}");
    }

    #[test]
    fn duplicate_register_preserves_original_tool() {
        let mut reg = ToolRegistry::new();
        let original = Arc::new(FakeTool::new("tool", "original"));
        reg.register(original).unwrap();

        let duplicate = Arc::new(FakeTool::new("tool", "duplicate"));
        assert!(reg.register(duplicate).is_err());

        // Original tool should still be there with its original description
        let t = reg.get("tool").unwrap();
        assert_eq!(t.description(), "original");
    }

    #[test]
    fn deregister_returns_true_when_tool_exists() {
        let mut reg = ToolRegistry::new();
        reg.register(fake_tool("removeme")).unwrap();
        assert_eq!(reg.deregister("removeme").unwrap(), true);
        assert!(reg.get("removeme").is_none());
    }

    #[test]
    fn deregister_returns_false_when_tool_not_found() {
        let mut reg = ToolRegistry::new();
        assert_eq!(reg.deregister("nonexistent").unwrap(), false);
    }

    #[test]
    fn deregister_allows_re_registration() {
        let mut reg = ToolRegistry::new();
        reg.register(fake_tool("reuse")).unwrap();
        reg.deregister("reuse").unwrap();
        // Should be able to register again after deregistration
        assert!(reg.register(fake_tool("reuse")).is_ok());
        assert!(reg.get("reuse").is_some());
    }

    #[test]
    fn list_tools_returns_empty_for_new_registry() {
        let reg = ToolRegistry::new();
        assert!(reg.list_tools().is_empty());
    }

    #[test]
    fn list_tools_returns_all_registered_tools() {
        let mut reg = ToolRegistry::new();
        reg.register(fake_tool("aaa")).unwrap();
        reg.register(fake_tool("bbb")).unwrap();
        reg.register(fake_tool("ccc")).unwrap();

        let infos = reg.list_tools();
        assert_eq!(infos.len(), 3);

        let mut names: Vec<String> = infos.iter().map(|i| i.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["aaa", "bbb", "ccc"]);
    }

    #[test]
    fn list_tools_reflects_deregistration() {
        let mut reg = ToolRegistry::new();
        reg.register(fake_tool("keep")).unwrap();
        reg.register(fake_tool("remove")).unwrap();
        reg.deregister("remove").unwrap();

        let infos = reg.list_tools();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].name, "keep");
    }

    #[test]
    fn list_tools_contains_correct_metadata() {
        let mut reg = ToolRegistry::new();
        let tool = Arc::new(FakeTool::new("mytool", "My tool description"));
        reg.register(tool).unwrap();

        let infos = reg.list_tools();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].name, "mytool");
        assert_eq!(infos[0].description, "My tool description");
        assert_eq!(infos[0].parameters_schema, json!({"type": "object", "properties": {}}));
    }

    #[tokio::test]
    async fn execute_works_after_register() {
        let mut reg = ToolRegistry::new();
        reg.register(fake_tool("exec_test")).unwrap();
        let result = reg.execute("exec_test", json!({})).await.unwrap();
        assert_eq!(result, "exec_test called");
    }

    #[tokio::test]
    async fn execute_fails_after_deregister() {
        let mut reg = ToolRegistry::new();
        reg.register(fake_tool("gone")).unwrap();
        reg.deregister("gone").unwrap();
        let result = reg.execute("gone", json!({})).await;
        assert!(result.is_err());
    }
}
