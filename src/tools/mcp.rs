//! MCP (Model Context Protocol) tools: connect to MCP servers and expose their tools as [DynTool].

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use mcp_client::client::{
    ClientCapabilities, ClientInfo, McpClient, McpClientTrait,
};
use mcp_client::service::McpService;
use mcp_client::transport::{Transport, SseTransport, StdioTransport};
use mcp_spec::content::Content;
use serde_json::Value;
use tracing::{info, warn};

use crate::config::{McpConfig, McpServerConfig, McpTransport};
use crate::tools::{DynTool, ToolRegistry};

/// Default timeout for MCP client operations (initialize, list_tools, call_tool).
const MCP_TIMEOUT_SECS: u64 = 30;

/// Load MCP servers from config and register their tools into the registry.
/// On per-server failure (connect/init/list), log and skip that server; do not fail startup.
pub async fn load_mcp_tools(cfg: &McpConfig, tools: &mut ToolRegistry) {
    if cfg.servers.is_empty() {
        return;
    }
    for server in &cfg.servers {
        if let Err(e) = load_one_mcp_server(server, tools).await {
            warn!(
                server_id = %server.id,
                error = %e,
                "MCP server connect/init failed, skipping"
            );
        }
    }
}

async fn load_one_mcp_server(server: &McpServerConfig, tools: &mut ToolRegistry) -> Result<()> {
    let timeout = Duration::from_secs(MCP_TIMEOUT_SECS);
    match server.transport {
        McpTransport::Stdio => {
            if server.command.is_empty() {
                anyhow::bail!("MCP stdio server '{}': command is required", server.id);
            }
            let transport = StdioTransport::new(
                server.command.clone(),
                server.args.clone(),
                HashMap::new(),
            );
            let handle = transport.start().await.map_err(|e| {
                anyhow::anyhow!("MCP stdio transport start failed for '{}': {}", server.id, e)
            })?;
            let service = McpService::with_timeout(handle, timeout);
            let mut client = McpClient::new(service);
            client
                .initialize(
                    ClientInfo {
                        name: "synbot".into(),
                        version: env!("CARGO_PKG_VERSION").into(),
                    },
                    ClientCapabilities::default(),
                )
                .await
                .map_err(|e| anyhow::anyhow!("MCP initialize failed for '{}': {}", server.id, e))?;
            let list = client
                .list_tools(None)
                .await
                .map_err(|e| anyhow::anyhow!("MCP list_tools failed for '{}': {}", server.id, e))?;
            let client: Arc<dyn McpClientTrait + Send + Sync> = Arc::new(client);
            register_mcp_tools(tools, server, list.tools, client);
        }
        McpTransport::Sse => {
            if server.url.is_empty() {
                anyhow::bail!("MCP SSE server '{}': url is required", server.id);
            }
            let transport = SseTransport::new(server.url.clone(), HashMap::new());
            let handle = transport.start().await.map_err(|e| {
                anyhow::anyhow!("MCP SSE transport start failed for '{}': {}", server.id, e)
            })?;
            let service = McpService::with_timeout(handle, timeout);
            let mut client = McpClient::new(service);
            client
                .initialize(
                    ClientInfo {
                        name: "synbot".into(),
                        version: env!("CARGO_PKG_VERSION").into(),
                    },
                    ClientCapabilities::default(),
                )
                .await
                .map_err(|e| anyhow::anyhow!("MCP initialize failed for '{}': {}", server.id, e))?;
            let list = client
                .list_tools(None)
                .await
                .map_err(|e| anyhow::anyhow!("MCP list_tools failed for '{}': {}", server.id, e))?;
            let client: Arc<dyn McpClientTrait + Send + Sync> = Arc::new(client);
            register_mcp_tools(tools, server, list.tools, client);
        }
    }
    Ok(())
}

fn register_mcp_tools(
    tools: &mut ToolRegistry,
    server: &McpServerConfig,
    mcp_tools: Vec<mcp_spec::Tool>,
    client: Arc<dyn McpClientTrait + Send + Sync>,
) {
    let prefix = server
        .tool_name_prefix
        .as_deref()
        .unwrap_or("");
    for t in mcp_tools {
        let name = format!("{}{}", prefix, t.name);
        if tools.get(&name).is_some() {
            warn!(
                server_id = %server.id,
                tool_name = %name,
                "MCP tool name already registered, skipping"
            );
            continue;
        }
        let parameters_schema = normalize_parameters_schema(t.input_schema);
        let adapter = McpToolAdapter {
            client: Arc::clone(&client),
            server_id: server.id.clone(),
            tool_name: t.name,
            description: t.description,
            parameters_schema,
            display_name: name.clone(),
        };
        if let Err(e) = tools.register(Arc::new(adapter)) {
            warn!(
                server_id = %server.id,
                tool_name = %name,
                error = %e,
                "failed to register MCP tool"
            );
        } else {
            info!(
                server_id = %server.id,
                tool_name = %name,
                "registered MCP tool"
            );
        }
    }
}

/// Adapts a single MCP tool to [DynTool].
struct McpToolAdapter {
    client: Arc<dyn McpClientTrait + Send + Sync>,
    server_id: String,
    tool_name: String,
    description: String,
    parameters_schema: Value,
    /// Registered name (may include prefix).
    display_name: String,
}

#[async_trait]
impl DynTool for McpToolAdapter {
    fn name(&self) -> &str {
        &self.display_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Value {
        self.parameters_schema.clone()
    }

    async fn call(&self, args: Value) -> Result<String> {
        let result = self
            .client
            .call_tool(&self.tool_name, args)
            .await
            .map_err(|e| anyhow::anyhow!("MCP call_tool '{}' (server {}): {}", self.tool_name, self.server_id, e))?;
        if result.is_error == Some(true) {
            let text = content_to_string(&result.content);
            anyhow::bail!("MCP tool error: {}", if text.is_empty() { "unknown" } else { &text });
        }
        Ok(content_to_string(&result.content))
    }
}

fn content_to_string(content: &[Content]) -> String {
    content
        .iter()
        .filter_map(Content::as_text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Ensure parameters_schema is a valid JSON Schema object for LLM providers (e.g. DeepSeek
/// requires `type: "object"`). MCP servers may return null or incomplete schema.
fn normalize_parameters_schema(schema: Value) -> Value {
    let mut obj = match schema {
        Value::Object(m) => m,
        _ => return default_object_schema(),
    };
    if obj.is_empty() {
        return default_object_schema();
    }
    // Require type: "object" for provider compatibility
    obj.insert("type".into(), Value::String("object".into()));
    Value::Object(obj)
}

fn default_object_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": true
    })
}
