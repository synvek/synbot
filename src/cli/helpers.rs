//! Helper functions for CLI commands.

use tracing::warn;
use crate::config;

/// Resolve provider API key and base URL from config.
pub fn resolve_provider(cfg: &config::Config) -> (String, Option<String>) {
    // Helper: trim empty to None, then normalize URL (IDN -> punycode) so reqwest does not fail
    let normalize_base = |base: &Option<String>| -> Option<String> {
        base.as_ref().and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return None;
            }
            match crate::url_utils::normalize_http_url(trimmed) {
                Ok(normalized) => Some(normalized),
                Err(e) => {
                    warn!(url = %trimmed, error = %e, "Invalid api_base URL, using as-is (may cause request to fail)");
                    Some(trimmed.to_string())
                }
            }
        })
    };

    // Priority: openrouter > anthropic > openai > deepseek > ollama
    if !cfg.providers.openrouter.api_key.is_empty() {
        return (cfg.providers.openrouter.api_key.clone(), normalize_base(&cfg.providers.openrouter.api_base));
    }
    if !cfg.providers.anthropic.api_key.is_empty() {
        return (cfg.providers.anthropic.api_key.clone(), normalize_base(&cfg.providers.anthropic.api_base));
    }
    if !cfg.providers.openai.api_key.is_empty() {
        return (cfg.providers.openai.api_key.clone(), normalize_base(&cfg.providers.openai.api_base));
    }
    if !cfg.providers.deepseek.api_key.is_empty() {
        return (cfg.providers.deepseek.api_key.clone(), normalize_base(&cfg.providers.deepseek.api_base));
    }
    if !cfg.providers.moonshot.api_key.is_empty() {
        return (cfg.providers.moonshot.api_key.clone(), normalize_base(&cfg.providers.moonshot.api_base));
    }
    if !cfg.providers.ollama.api_key.is_empty() {
        return (cfg.providers.ollama.api_key.clone(), normalize_base(&cfg.providers.ollama.api_base));
    }
    (String::new(), None)
}

/// Detect rig provider from model name.
pub fn detect_rig_provider(model: &str) -> rig_dyn::Provider {
    let lower = model.to_lowercase();
    if lower.contains("anthropic") || lower.contains("claude") {
        rig_dyn::Provider::Anthropic
    } else if lower.contains("openai") || lower.contains("gpt") {
        rig_dyn::Provider::OpenAI
    } else if lower.contains("deepseek") {
        rig_dyn::Provider::DeepSeek
    } else if lower.contains("moonshot") {
        rig_dyn::Provider::Moonshot
    } else if lower.contains("ollama") {
        rig_dyn::Provider::Ollama
    } else {
        // Default to OpenAI-compatible (works with OpenRouter etc.)
        rig_dyn::Provider::OpenAI
    }
}

/// Optional context for heartbeat/cron tools (shared config + path). When provided, list/add/delete heartbeat and cron tools are registered.
pub type HeartbeatCronContext = Option<(
    std::sync::Arc<tokio::sync::RwLock<config::Config>>,
    Option<std::path::PathBuf>,
)>;

/// Build default tool registry.
pub fn build_default_tools(
    cfg: &config::Config,
    ws: &std::path::Path,
    subagent_mgr: std::sync::Arc<tokio::sync::Mutex<crate::agent::subagent::SubagentManager>>,
    approval_manager: std::sync::Arc<crate::tools::approval::ApprovalManager>,
    permission_policy: Option<std::sync::Arc<crate::tools::permission::CommandPermissionPolicy>>,
    heartbeat_cron: HeartbeatCronContext,
) -> crate::tools::ToolRegistry {
    use crate::tools::*;
    let restrict = cfg.tools.exec.restrict_to_workspace;
    let ws = ws.to_path_buf();

    let mut reg = ToolRegistry::new();
    reg.register(std::sync::Arc::new(filesystem::ReadFileTool { workspace: ws.clone(), restrict })).expect("register ReadFileTool");
    reg.register(std::sync::Arc::new(filesystem::WriteFileTool { workspace: ws.clone(), restrict })).expect("register WriteFileTool");
    reg.register(std::sync::Arc::new(filesystem::EditFileTool { workspace: ws.clone(), restrict })).expect("register EditFileTool");
    reg.register(std::sync::Arc::new(filesystem::ListDirTool { workspace: ws.clone(), restrict })).expect("register ListDirTool");
    reg.register(std::sync::Arc::new(approval_tool::SubmitApprovalResponseTool {
        approval_manager: approval_manager.clone(),
    })).expect("register SubmitApprovalResponseTool");
    reg.register(std::sync::Arc::new(shell::ExecTool {
        workspace: ws.clone(),
        timeout_secs: cfg.tools.exec.timeout_secs,
        approval_timeout_secs: cfg.tools.exec.permissions.approval_timeout_secs,
        restrict_to_workspace: restrict,
        policy: shell::CommandPolicy::new(
            cfg.tools.exec.deny_patterns.clone(),
            cfg.tools.exec.allow_patterns.clone(),
        ),
        permission_policy,
        approval_manager: Some(approval_manager),
        session_id: None,
        channel: None,
        chat_id: None,
    })).expect("register ExecTool");
    if !cfg.tools.web.brave_api_key.is_empty() {
        reg.register(std::sync::Arc::new(web::WebSearchTool {
            api_key: cfg.tools.web.brave_api_key.clone(),
        })).expect("register WebSearchTool");
    }
    reg.register(std::sync::Arc::new(web::WebFetchTool)).expect("register WebFetchTool");
    reg.register(std::sync::Arc::new(spawn::SpawnTool {
        manager: subagent_mgr,
    })).expect("register SpawnTool");
    reg.register(std::sync::Arc::new(memory_tool::RememberTool::new("main"))).expect("register RememberTool");
    reg.register(std::sync::Arc::new(memory_tool::ListMemoryTool::new("main"))).expect("register ListMemoryTool");

    if let Some((config, config_path)) = heartbeat_cron {
        let inner = heartbeat_cron::HeartbeatCronTools {
            config,
            config_path,
        };
        reg.register(std::sync::Arc::new(heartbeat_cron::ListHeartbeatTasksTool {
            inner: inner.clone(),
        })).expect("register ListHeartbeatTasksTool");
        reg.register(std::sync::Arc::new(heartbeat_cron::AddHeartbeatTaskTool {
            inner: inner.clone(),
        })).expect("register AddHeartbeatTaskTool");
        reg.register(std::sync::Arc::new(heartbeat_cron::DeleteHeartbeatTaskTool {
            inner: inner.clone(),
        })).expect("register DeleteHeartbeatTaskTool");
        reg.register(std::sync::Arc::new(heartbeat_cron::ListCronTasksTool {
            inner: inner.clone(),
        })).expect("register ListCronTasksTool");
        reg.register(std::sync::Arc::new(heartbeat_cron::AddCronTaskTool {
            inner: inner.clone(),
        })).expect("register AddCronTaskTool");
        reg.register(std::sync::Arc::new(heartbeat_cron::DeleteCronTaskTool {
            inner: inner.clone(),
        })).expect("register DeleteCronTaskTool");
    }

    reg
}
