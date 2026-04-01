//! Helper functions for CLI commands.

use tracing::warn;
use crate::config;

/// Resolve API key and base URL for the given provider name.
/// Returns the credentials for that provider only, so model and key stay consistent when multiple providers are configured.
pub fn resolve_provider(cfg: &config::Config, provider_name: &str) -> (String, Option<String>) {
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

    let name = provider_name.trim().to_lowercase();
    let (key, base) = if let Some(entry) = cfg
        .providers
        .extra
        .get(provider_name.trim())
        .or_else(|| cfg.providers.extra.get(&name))
    {
        (entry.api_key.clone(), normalize_base(&entry.api_base))
    } else if name.contains("openrouter") {
        (cfg.providers.openrouter.api_key.clone(), normalize_base(&cfg.providers.openrouter.api_base))
    } else if name.contains("anthropic") || name.contains("claude") {
        (cfg.providers.anthropic.api_key.clone(), normalize_base(&cfg.providers.anthropic.api_base))
    } else if name.contains("openai") {
        (cfg.providers.openai.api_key.clone(), normalize_base(&cfg.providers.openai.api_base))
    } else if name.contains("gemini") {
        (cfg.providers.gemini.api_key.clone(), normalize_base(&cfg.providers.gemini.api_base))
    } else if name.contains("deepseek") {
        (cfg.providers.deepseek.api_key.clone(), normalize_base(&cfg.providers.deepseek.api_base))
    } else if name.contains("moonshot") {
        (cfg.providers.moonshot.api_key.clone(), normalize_base(&cfg.providers.moonshot.api_base))
    } else if name.contains("kimi") {
        (cfg.providers.kimi_code.api_key.clone(), normalize_base(&cfg.providers.kimi_code.api_base))
    } else if name.contains("ollama") {
        (cfg.providers.ollama.api_key.clone(), normalize_base(&cfg.providers.ollama.api_base))
    } else {
        (String::new(), normalize_base(&None))
    };
    (key, base)
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

/// When tool sandbox is active: [`crate::sandbox::ToolSandboxDelegate`] (local manager or Windows remote IPC).
pub use crate::sandbox::SandboxContext;

/// Build default tool registry. Returns the registry and a shared spawn context;
/// set the context (model, workspace, tools, agent_id) after you have them so the
/// spawn tool runs real subagents instead of no-ops.
pub fn build_default_tools(
    cfg: &config::Config,
    ws: &std::path::Path,
    subagent_mgr: std::sync::Arc<tokio::sync::Mutex<crate::agent::subagent::SubagentManager>>,
    approval_manager: std::sync::Arc<crate::tools::approval::ApprovalManager>,
    permission_policy: Option<std::sync::Arc<crate::tools::permission::CommandPermissionPolicy>>,
    heartbeat_cron: HeartbeatCronContext,
    sandbox_context: &SandboxContext,
    shared_session_state: crate::agent::session_state::SharedSessionState,
    outbound_tx: tokio::sync::broadcast::Sender<crate::bus::OutboundMessage>,
) -> (crate::tools::ToolRegistry, std::sync::Arc<tokio::sync::RwLock<Option<crate::tools::spawn::SpawnContext>>>) {
    use crate::tools::*;
    // Same flag as ExecTool: paths must stay under workspace when true (see filesystem::resolve_path).
    // File tools run in the main process; tool sandbox only wraps exec — workspace scope still applies here.
    let restrict = cfg.tools.exec.restrict_to_workspace;
    let ws = ws.to_path_buf();

    let spawn_context = std::sync::Arc::new(tokio::sync::RwLock::new(None));
    let mut reg = ToolRegistry::new();
    reg.register(std::sync::Arc::new(filesystem::ReadFileTool { workspace: ws.clone(), restrict })).expect("register ReadFileTool");
    reg.register(std::sync::Arc::new(filesystem::WriteFileTool { workspace: ws.clone(), restrict })).expect("register WriteFileTool");
    reg.register(std::sync::Arc::new(filesystem::EditFileTool { workspace: ws.clone(), restrict })).expect("register EditFileTool");
    reg.register(std::sync::Arc::new(filesystem::ListDirTool { workspace: ws.clone(), restrict })).expect("register ListDirTool");
    reg.register(std::sync::Arc::new(filesystem::ReadMultipleFilesTool { workspace: ws.clone(), restrict })).expect("register ReadMultipleFilesTool");
    reg.register(std::sync::Arc::new(filesystem::CreateDirTool { workspace: ws.clone(), restrict })).expect("register CreateDirTool");
    reg.register(std::sync::Arc::new(filesystem::MoveFileTool { workspace: ws.clone(), restrict })).expect("register MoveFileTool");
    reg.register(std::sync::Arc::new(filesystem::SearchFilesTool { workspace: ws.clone(), restrict })).expect("register SearchFilesTool");
    reg.register(std::sync::Arc::new(filesystem::SearchTextTool { workspace: ws.clone(), restrict })).expect("register SearchTextTool");
    reg.register(std::sync::Arc::new(filesystem::GetFileInfoTool { workspace: ws.clone(), restrict })).expect("register GetFileInfoTool");
    reg.register(std::sync::Arc::new(code_analyzer::CodeAnalyzerTool {
        workspace: ws.clone(),
        restrict,
        max_file_size: 1_048_576,  // 1MB
        max_scan_files: 1000,
    })).expect("register CodeAnalyzerTool");
    reg.register(std::sync::Arc::new(diff_viewer::DiffViewerTool {
        workspace: ws.clone(),
        restrict,
        max_diff_lines: 500,
    })).expect("register DiffViewerTool");
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
        sandbox_context: sandbox_context.clone(),
    })).expect("register ExecTool");
    reg.register(std::sync::Arc::new(web::WebSearchTool::from_config(&cfg.tools.web)))
        .expect("register WebSearchTool");
    reg.register(std::sync::Arc::new(web::WebFetchTool)).expect("register WebFetchTool");
    if cfg.tools.browser.enabled {
        reg.register(std::sync::Arc::new(browser::BrowserTool::from_config(&cfg.tools.browser)))
            .expect("register BrowserTool");
        tracing::info!("Registered tool: browser (BrowserTool)");
    }
    reg.register(std::sync::Arc::new(spawn::SpawnTool {
        manager: subagent_mgr,
        context: spawn_context.clone(),
    })).expect("register SpawnTool");
    reg.register(std::sync::Arc::new(memory_tool::RememberTool::new("main"))).expect("register RememberTool");
    reg.register(std::sync::Arc::new(memory_tool::ListMemoryTool::new("main"))).expect("register ListMemoryTool");
    reg.register(std::sync::Arc::new(session_tools::ListSessionsTool::new(
        shared_session_state.clone(),
    )))
    .expect("register ListSessionsTool");
    reg.register(std::sync::Arc::new(session_tools::ResetSessionTool::new(
        shared_session_state.clone(),
    )))
    .expect("register ResetSessionTool");
    reg.register(std::sync::Arc::new(skills_tool::ListSkillsTool::new()))
        .expect("register ListSkillsTool");
    reg.register(std::sync::Arc::new(skills_tool::ListSystemSkillsTool::new()))
        .expect("register ListSystemSkillsTool");
    reg.register(std::sync::Arc::new(skills_tool::ReadSystemSkillTool::new()))
        .expect("register ReadSystemSkillTool");
    reg.register(std::sync::Arc::new(skills_tool::InstallSystemSkillTool::new()))
        .expect("register InstallSystemSkillTool");

    reg.register(std::sync::Arc::new(message::MessageTool {
        outbound_tx: outbound_tx.clone(),
        default_channel: String::new(),
        default_chat_id: String::new(),
    }))
    .expect("register MessageTool");

    if cfg.tools.generation.image.enabled && !cfg.tools.generation.image.provider.is_empty() {
        let (api_key, api_base) = resolve_provider(cfg, &cfg.tools.generation.image.provider);
        reg.register(std::sync::Arc::new(crate::tools::generation::GenerateImageTool {
            workspace: ws.clone(),
            outbound_tx: outbound_tx.clone(),
            default_channel: String::new(),
            default_chat_id: String::new(),
            api_key,
            api_base,
            output_dir: cfg.tools.generation.image.output_dir.clone(),
            model: cfg.tools.generation.image.model.clone(),
            size: cfg.tools.generation.image.size.clone(),
            quality: cfg.tools.generation.image.quality.clone(),
        }))
        .expect("register GenerateImageTool");
    }
    if cfg.tools.generation.speech.enabled && !cfg.tools.generation.speech.provider.is_empty() {
        let (api_key, api_base) = resolve_provider(cfg, &cfg.tools.generation.speech.provider);
        reg.register(std::sync::Arc::new(crate::tools::generation::GenerateSpeechTool {
            workspace: ws.clone(),
            outbound_tx: outbound_tx.clone(),
            default_channel: String::new(),
            default_chat_id: String::new(),
            api_key,
            api_base,
            output_dir: cfg.tools.generation.speech.output_dir.clone(),
            model: cfg.tools.generation.speech.model.clone(),
            voice: cfg.tools.generation.speech.voice.clone(),
            format: cfg.tools.generation.speech.format.clone(),
        }))
        .expect("register GenerateSpeechTool");
    }
    if cfg.tools.generation.video.enabled && !cfg.tools.generation.video.provider.is_empty() {
        let (api_key, api_base) = resolve_provider(cfg, &cfg.tools.generation.video.provider);
        reg.register(std::sync::Arc::new(crate::tools::generation::GenerateVideoTool {
            workspace: ws.clone(),
            outbound_tx: outbound_tx.clone(),
            default_channel: String::new(),
            default_chat_id: String::new(),
            api_key,
            api_base,
            output_dir: cfg.tools.generation.video.output_dir.clone(),
            model: cfg.tools.generation.video.model.clone(),
        }))
        .expect("register GenerateVideoTool");
    }

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

    (reg, spawn_context)
}
