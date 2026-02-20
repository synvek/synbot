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

/// Build a rig completion model using rig-core (no rig-dyn). Returns Arc<dyn SynbotCompletionModel>.
pub fn build_rig_completion_model(
    provider_name: &str,
    model_name: &str,
    api_key: &str,
    api_base: Option<&str>,
) -> anyhow::Result<std::sync::Arc<dyn crate::rig_provider::SynbotCompletionModel>> {
    crate::rig_provider::build_completion_model(
        provider_name,
        model_name,
        api_key,
        api_base,
    )
}

/// Optional context for heartbeat/cron tools (shared config + path). When provided, list/add/delete heartbeat and cron tools are registered.
pub type HeartbeatCronContext = Option<(
    std::sync::Arc<tokio::sync::RwLock<config::Config>>,
    Option<std::path::PathBuf>,
)>;

/// When sandbox is configured: (manager, tool_sandbox_id).
/// If tool_sandbox_id is Some("synbot-tool"), exec runs inside it; if None, manager is kept alive (e.g. for app sandbox only).
/// Pass by reference so the caller can keep the manager alive for the process lifetime when only app_sandbox is running.
pub type SandboxContext = Option<(
    std::sync::Arc<crate::sandbox::SandboxManager>,
    Option<String>, // tool_sandbox_id when tool sandbox started, None when only app sandbox or tool failed
)>;

/// Build default tool registry.
pub fn build_default_tools(
    cfg: &config::Config,
    ws: &std::path::Path,
    subagent_mgr: std::sync::Arc<tokio::sync::Mutex<crate::agent::subagent::SubagentManager>>,
    approval_manager: std::sync::Arc<crate::tools::approval::ApprovalManager>,
    permission_policy: Option<std::sync::Arc<crate::tools::permission::CommandPermissionPolicy>>,
    heartbeat_cron: HeartbeatCronContext,
    sandbox_context: &SandboxContext,
) -> crate::tools::ToolRegistry {
    use crate::tools::*;
    let restrict = cfg.tools.exec.restrict_to_workspace;
    let ws = ws.to_path_buf();

    let tool_sandbox_enabled = sandbox_context
        .as_ref()
        .and_then(|(_, id)| id.as_ref())
        .is_some();

    let mut reg = ToolRegistry::new();
    if !tool_sandbox_enabled {
        reg.register(std::sync::Arc::new(filesystem::ReadFileTool { workspace: ws.clone(), restrict })).expect("register ReadFileTool");
        reg.register(std::sync::Arc::new(filesystem::WriteFileTool { workspace: ws.clone(), restrict })).expect("register WriteFileTool");
        reg.register(std::sync::Arc::new(filesystem::EditFileTool { workspace: ws.clone(), restrict })).expect("register EditFileTool");
        reg.register(std::sync::Arc::new(filesystem::ListDirTool { workspace: ws.clone(), restrict })).expect("register ListDirTool");
    }
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
        sandbox_context: sandbox_context.as_ref().and_then(|(m, id)| id.as_ref().map(|sid| (m.clone(), sid.clone()))),
    })).expect("register ExecTool");
    reg.register(std::sync::Arc::new(web::WebSearchTool::from_config(&cfg.tools.web)))
        .expect("register WebSearchTool");
    reg.register(std::sync::Arc::new(web::WebFetchTool)).expect("register WebFetchTool");
    if cfg.tools.browser.enabled {
        reg.register(std::sync::Arc::new(browser::BrowserTool::from_config(&cfg.tools.browser)))
            .expect("register BrowserTool");
    }
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
