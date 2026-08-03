//! ACP command - serve the Agent Client Protocol (JSON-RPC over stdio).
//!
//! Editors like Zed spawn `synbot acp` as a subprocess. stdout carries the
//! protocol stream, so logging goes to stderr and the log file only.

use anyhow::Result;
use tracing::info;

use super::helpers::{build_default_tools, build_rig_completion_model, resolve_provider};
use crate::config;
use crate::logging;

pub async fn cmd_acp(provider: Option<String>, model: Option<String>) -> Result<()> {
    let cfg = config::load_config(None)?;

    // stdout is reserved for JSON-RPC: log to stderr + file only.
    logging::init_stderr_logging(&cfg)?;

    let ws = config::effective_workspace_path(&cfg);

    let model_name = model.unwrap_or(cfg.main_agent.model.clone());
    let provider_name = provider.unwrap_or(cfg.main_agent.provider.clone());
    info!(model = %model_name, provider = %provider_name, "Starting ACP agent");

    let (api_key, api_base) = resolve_provider(&cfg, &provider_name);
    if api_key.is_empty() {
        anyhow::bail!(
            "No API key configured for provider '{}'. Run `synbot onboard` and set [providers.*] in {}",
            provider_name,
            config::config_path().display()
        );
    }

    let completion_model = build_rig_completion_model(
        &provider_name,
        &model_name,
        &api_key,
        api_base.as_deref(),
    )?;

    let subagent_mgr = std::sync::Arc::new(tokio::sync::Mutex::new(
        crate::agent::subagent::SubagentManager::new(
            cfg.main_agent.max_concurrent_subagents,
            Some(cfg.main_agent.subagent_task_timeout_secs),
        ),
    ));

    // Bus first: the approval manager publishes requests on the outbound bus so
    // the ACP bridge can turn them into session/request_permission round-trips.
    let mut bus = crate::bus::MessageBus::new();

    let approval_manager = std::sync::Arc::new(
        crate::tools::approval::ApprovalManager::with_outbound(bus.outbound_tx_clone()),
    );

    let permission_policy = if cfg.tools.exec.permissions.enabled {
        Some(std::sync::Arc::new(
            crate::tools::permission::CommandPermissionPolicy::new(
                cfg.tools.exec.permissions.rules.clone(),
                cfg.tools.exec.permissions.default_level,
            ),
        ))
    } else {
        None
    };

    let session_store =
        crate::agent::session::SessionStore::new(crate::config::sessions_root().as_path());
    let shared_session_state = crate::agent::session_state::SharedSessionState::new(session_store);
    if let Err(e) = shared_session_state.load_persisted_sessions().await {
        tracing::warn!(error = %e, "Failed to load persisted sessions");
    }

    let shared_config = std::sync::Arc::new(tokio::sync::RwLock::new(cfg.clone()));

    let sandbox_context = super::start::init_sandbox_if_configured(&cfg).await;
    let tool_sandbox_delegate =
        super::start::tool_sandbox_delegate_from_startup(&sandbox_context);

    let (mut tool_reg, spawn_context) = build_default_tools(
        &cfg,
        std::sync::Arc::clone(&shared_config),
        &ws,
        std::sync::Arc::clone(&subagent_mgr),
        std::sync::Arc::clone(&approval_manager),
        permission_policy,
        None, // no heartbeat/cron tools in ACP mode
        &tool_sandbox_delegate, // use the configured sandbox; Safe mode fails closed if unavailable
        shared_session_state.clone(),
        bus.outbound_tx_clone(),
    );

    // Extism plugins (tools, hooks, background, skills) work in ACP mode too.
    let hook_registry = crate::hooks::HookRegistry::new();
    let mut background_registry = crate::background::BackgroundServiceRegistry::new();
    let skills_dir = config::skills_dir();
    if let Err(e) = std::fs::create_dir_all(&skills_dir) {
        tracing::warn!(path = %skills_dir.display(), error = %e, "Could not create skills dir");
    }
    let mut skills_composite =
        crate::agent::skills::CompositeSkillProvider::default_with_fs(&skills_dir);
    crate::plugin::load_extism_plugins(
        &cfg,
        &mut tool_reg,
        &hook_registry,
        &mut background_registry,
        &mut skills_composite,
    )
    .await;

    if let Err(e) = tool_reg.register_list_tools_tool() {
        tracing::warn!(error = %e, "Failed to register list_tools tool");
    }

    let tools = std::sync::Arc::new(tool_reg);
    let skills_loader = std::sync::Arc::new(skills_composite);

    {
        let mut ctx = spawn_context.write().await;
        *ctx = Some(crate::tools::spawn::SpawnContext {
            model: std::sync::Arc::clone(&completion_model),
            workspace: ws.clone(),
            tools: std::sync::Arc::clone(&tools),
            agent_id: "main".to_string(),
            max_tokens: cfg.main_agent.max_tokens,
            temperature: cfg.main_agent.temperature,
            outbound_tx: bus.outbound_tx_clone(),
        });
    }

    let inbound_tx = bus.inbound_sender();
    let inbound_rx = bus.take_inbound_receiver().unwrap();

    // ACP bridge: translates JSON-RPC <-> bus messages. Its turn hook resolves
    // session/prompt requests when the agent run for that session finishes.
    let bridge = crate::acp::AcpBridge::new(
        inbound_tx.clone(),
        bus.outbound_tx_clone(),
        std::sync::Arc::clone(&approval_manager),
    );
    hook_registry.register(bridge.turn_hook()).await;

    let roles_dir = config::roles_dir();
    let mut role_registry = crate::agent::role_registry::RoleRegistry::new();
    if let Err(e) = role_registry.load_from_dirs(&roles_dir) {
        tracing::warn!(error = %e, "Failed to load role registry");
    }
    let role_registry = std::sync::Arc::new(role_registry);
    let mut agent_registry = crate::agent::agent_registry::AgentRegistry::new();
    if let Err(e) = agent_registry.load_from_config(
        &cfg.main_agent,
        &cfg.providers,
        &role_registry,
        &ws,
    ) {
        tracing::warn!(error = %e, "Failed to load agent registry");
    }
    let agent_registry = std::sync::Arc::new(agent_registry);

    let agent_loop = crate::agent::r#loop::AgentLoop::new(
        completion_model,
        ws,
        skills_loader,
        tools,
        cfg.main_agent.max_tool_iterations,
        bus.outbound_tx_clone(),
        &cfg,
        shared_session_state,
        agent_registry,
        None,
        Some(std::sync::Arc::new(hook_registry)),
        std::sync::Arc::clone(&shared_config),
    )
    .await;
    let loop_ref = std::sync::Arc::new(tokio::sync::Mutex::new(agent_loop));

    // Agent loop in the background; ACP connection in the foreground.
    let agent_handle = tokio::spawn(async move {
        let _ = crate::agent::r#loop::AgentLoop::run(loop_ref, inbound_rx).await;
    });

    let serve_result = crate::acp::serve_stdio(bridge).await;

    // Client disconnected (stdin EOF) or connection error: close the inbound
    // channel so the agent loop exits, then wait for it.
    bus.close_inbound();
    drop(inbound_tx);
    let _ = agent_handle.await;

    serve_result
}
