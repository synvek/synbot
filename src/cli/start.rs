//! Start command - Start the full daemon (channels + heartbeat + cron).

use anyhow::Result;
use std::io::Write;
use std::path::PathBuf;
use tracing::{info, warn};
use crate::config;
use crate::logging;
use super::helpers::{resolve_provider, build_rig_completion_model, build_default_tools};

/// Removes the PID file when the daemon process exits (e.g. Ctrl+C).
struct PidFileGuard(PathBuf);
impl Drop for PidFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn write_pid_file() -> Result<PidFileGuard> {
    let pid_path = config::config_dir().join("synbot.pid");
    std::fs::write(&pid_path, std::process::id().to_string())?;
    Ok(PidFileGuard(pid_path))
}

pub async fn cmd_start() -> Result<()> {
    // Immediate stderr so sandbox parent sees child has started (before any logging init)
    let _ = writeln!(std::io::stderr(), "[synbot] daemon starting...");
    let _ = std::io::stderr().flush();

    // When running inside Windows AppContainer (child of `synbot sandbox`), log token diagnostic for WFP troubleshooting.
    #[cfg(target_os = "windows")]
    if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
        crate::sandbox::windows_appcontainer::log_process_token_appcontainer_diagnostic();
    }

    let cfg = config::load_config(None)?;
    let _pid_guard = write_pid_file()?;
    let shared_config = std::sync::Arc::new(tokio::sync::RwLock::new(cfg.clone()));

    // Create log buffer and channel for web UI before logging init
    let log_buffer = crate::web::create_log_buffer(1000);
    let (log_tx, mut log_rx) = tokio::sync::mpsc::channel(256);
    let log_buffer_clone = std::sync::Arc::clone(&log_buffer);
    tokio::spawn(async move {
        while let Some(entry) = log_rx.recv().await {
            let mut guard = log_buffer_clone.write().await;
            guard.push(entry);
        }
    });
    
    // Initialize logging with config (and feed events to log buffer for web UI)
    logging::init_logging(&cfg, Some(std::sync::Arc::new(log_tx)))?;

    // When running inside app sandbox (Windows AppContainer or macOS nono): one-shot network
    // diagnostic to capture underlying error (DNS, TLS, or connect).
    if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
        let client = crate::appcontainer_dns::build_reqwest_client();
        let url = if cfg!(target_os = "windows") {
            "https://www.microsoft.com"
        } else {
            "https://open.feishu.cn"
        };
        // Optional: log DNS resolution when on macOS to see if 8.8.8.8 is reachable
        #[cfg(target_os = "macos")]
        {
            match tokio::time::timeout(
                std::time::Duration::from_secs(3),
                crate::appcontainer_dns::global_resolver().lookup_ip("open.feishu.cn"),
            )
            .await
            {
                Ok(Ok(lookup)) => {
                    let addrs: Vec<_> = lookup.iter().take(3).collect();
                    info!("App sandbox (macOS) DNS diagnostic: open.feishu.cn -> {:?}", addrs);
                }
                Ok(Err(e)) => {
                    warn!("App sandbox (macOS) DNS diagnostic: open.feishu.cn resolve failed: {}", e);
                }
                Err(_) => {
                    warn!("App sandbox (macOS) DNS diagnostic: open.feishu.cn resolve timed out (3s)");
                }
            }
        }
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            client.get(url).send(),
        )
        .await;
        match result {
            Ok(Ok(resp)) => {
                info!("App sandbox network diagnostic: GET {} -> {}", url, resp.status());
            }
            Ok(Err(e)) => {
                let ae = anyhow::Error::from(e);
                if let Some(io) = ae.downcast_ref::<std::io::Error>() {
                    warn!(
                        "App sandbox network diagnostic: request failed io_error kind={:?} raw_os_error={:?}",
                        io.kind(),
                        io.raw_os_error()
                    );
                }
                warn!("App sandbox network diagnostic: request failed: {:#}", ae);
            }
            Err(_) => {
                warn!("App sandbox network diagnostic: request timed out after 10s");
            }
        }
    }

    let ws = config::workspace_path(&cfg);

    // Subagent manager (shared via Arc<Mutex<>>)
    let subagent_mgr = std::sync::Arc::new(
        tokio::sync::Mutex::new(
            crate::agent::subagent::SubagentManager::new(
                cfg.main_agent.max_concurrent_subagents,
                Some(cfg.main_agent.subagent_task_timeout_secs),
            ),
        ),
    );

    // Message bus (create early so approval manager can broadcast to channels)
    let mut bus = crate::bus::MessageBus::new();
    let inbound_tx = bus.inbound_sender();
    let inbound_rx = bus.take_inbound_receiver().unwrap();

    // Create approval manager with outbound sender so approval requests reach Web/other channels
    let approval_manager = std::sync::Arc::new(
        crate::tools::approval::ApprovalManager::with_outbound(bus.outbound_tx_clone()),
    );

    // Load permission policy if enabled
    let permission_policy = if cfg.tools.exec.permissions.enabled {
        Some(std::sync::Arc::new(crate::tools::permission::CommandPermissionPolicy::new(
            cfg.tools.exec.permissions.rules.clone(),
            cfg.tools.exec.permissions.default_level,
        )))
    } else {
        None
    };

    let heartbeat_cron_ctx: super::helpers::HeartbeatCronContext =
        Some((std::sync::Arc::clone(&shared_config), Some(config::config_path())));

    // Optional sandbox: create and start app/tool sandboxes when configured.
    let sandbox_context = init_sandbox_if_configured(&cfg).await;

    // Shared session state (in-memory + persistence); load before agent loop and tools
    let session_store = crate::agent::session::SessionStore::new(crate::config::sessions_root().as_path());
    let shared_session_state = crate::agent::session_state::SharedSessionState::new(session_store);
    if let Err(e) = shared_session_state.load_persisted_sessions().await {
        tracing::warn!(error = %e, "Failed to load persisted sessions");
    }

    let (mut tool_reg, spawn_context) = build_default_tools(
        &cfg,
        &ws,
        std::sync::Arc::clone(&subagent_mgr),
        std::sync::Arc::clone(&approval_manager),
        permission_policy.clone(),
        heartbeat_cron_ctx,
        &sandbox_context,
        shared_session_state.clone(),
        bus.outbound_tx_clone(),
    );

    let hook_registry = crate::hooks::HookRegistry::new();
    let mut background_registry = crate::background::BackgroundServiceRegistry::new();
    background_registry.register(std::sync::Arc::new(
        crate::background::HeartbeatBackgroundService::new(std::sync::Arc::clone(&shared_config)),
    ));
    background_registry.register(std::sync::Arc::new(
        crate::background::CronBackgroundService::new(std::sync::Arc::clone(&shared_config)),
    ));
    let skills_dir = config::skills_dir();
    if let Err(e) = std::fs::create_dir_all(&skills_dir) {
        tracing::warn!(path = %skills_dir.display(), error = %e, "Could not create skills dir");
    }
    let mut skills_composite = crate::agent::skills::CompositeSkillProvider::default_with_fs(&skills_dir);

    crate::plugin::load_extism_plugins(
        &cfg,
        &mut tool_reg,
        &hook_registry,
        &mut background_registry,
        &mut skills_composite,
    )
    .await;

    #[cfg(feature = "mcp")]
    if let Some(mcp) = &cfg.tools.mcp {
        crate::tools::mcp::load_mcp_tools(mcp, &mut tool_reg).await;
    }

    if let Err(e) = tool_reg.register_list_tools_tool() {
        tracing::warn!(error = %e, "Failed to register list_tools tool");
    }

    let model = cfg.main_agent.model.clone();
    let provider_name = cfg.main_agent.provider.clone();
    let (api_key, api_base) = resolve_provider(&cfg, &provider_name);
    if api_key.is_empty() {
        anyhow::bail!(
            "No API key configured for provider '{}'. Set the corresponding [providers] entry in config.",
            provider_name,
        );
    }
    let completion_model = build_rig_completion_model(
        &provider_name,
        &model,
        &api_key,
        api_base.as_deref(),
    )?;

    let tools = std::sync::Arc::new(tool_reg);

    // Wire spawn tool to run real subagents (model + tools) and to send completion to user
    {
        let mut ctx = spawn_context.write().await;
        *ctx = Some(crate::tools::spawn::SpawnContext {
            model: std::sync::Arc::clone(&completion_model),
            workspace: ws.clone(),
            tools: std::sync::Arc::clone(&tools),
            agent_id: "main".to_string(),
            outbound_tx: bus.outbound_tx_clone(),
        });
    }

    let roles_dir = config::roles_dir();
    let mut role_registry = crate::agent::role_registry::RoleRegistry::new();
    if let Err(e) = role_registry.load_from_dirs(&roles_dir) {
        tracing::warn!(error = %e, "Failed to load role registry from config");
    }
    let role_registry = std::sync::Arc::new(role_registry);

    let mut agent_registry = crate::agent::agent_registry::AgentRegistry::new();
    if let Err(e) = agent_registry.load_from_config(
        &cfg.main_agent,
        &role_registry,
        &ws,
    ) {
        tracing::warn!(error = %e, "Failed to load agent registry from config");
    }
    let agent_registry = std::sync::Arc::new(agent_registry);

    // Ensure memory dirs and MEMORY.md exist under ~/.synbot/memory/{agentId} (main + each agent)
    crate::agent::memory::ensure_memory_dirs(&cfg);

    // Create main agent's SQLite index file so it exists and can be populated by reindex later
    #[cfg(feature = "memory-index")]
    {
        let _ = crate::agent::memory_index::open_index("main");
    }

    let skills_loader = std::sync::Arc::new(skills_composite);

    let cron_store_path = config::config_dir().join("cron").join("jobs.json");
    let cron_service = std::sync::Arc::new(tokio::sync::RwLock::new(
        crate::cron::service::CronService::new(cron_store_path),
    ));

    let tool_sandbox_enabled = sandbox_context
        .as_ref()
        .and_then(|(_, id)| id.as_ref())
        .is_some();

    // Start agent loop (Arc<Mutex<>> so /stop or /cancel can cancel a running agent task)
    let agent_loop = crate::agent::r#loop::AgentLoop::new(
        std::sync::Arc::clone(&completion_model),
        ws.clone(),
        tools,
        cfg.main_agent.max_tool_iterations,
        bus.outbound_tx_clone(),
        &cfg,
        shared_session_state.clone(),
        std::sync::Arc::clone(&agent_registry),
        tool_sandbox_enabled,
        Some(std::sync::Arc::new(hook_registry)),
    )
    .await;
    let loop_ref = std::sync::Arc::new(tokio::sync::Mutex::new(agent_loop));
    tokio::spawn(async move {
        if let Err(e) = crate::agent::r#loop::AgentLoop::run(loop_ref, inbound_rx).await {
            tracing::error!("Agent loop error: {e:#}");
        }
    });

    // Start background services (heartbeat, cron, and any Extism plugin-registered services)
    let bg_ctx = crate::background::BackgroundContext {
        inbound_tx: inbound_tx.clone(),
        config: std::sync::Arc::clone(&shared_config),
    };
    for service in background_registry.services() {
        let ctx = bg_ctx.clone();
        let service = std::sync::Arc::clone(service);
        tokio::spawn(async move {
            if let Err(e) = service.run(ctx).await {
                tracing::error!(service = service.name(), "Background service error: {e:#}");
            }
        });
    }

    // Start channels via registry (built-in + any plugin-registered channel types)
    let mut channel_registry = crate::channels::ChannelRegistry::new();
    crate::channels::factory::register_builtin_channels(&mut channel_registry);
    for (type_name, configs) in cfg.channels.channel_entries() {
        let factory = match channel_registry.get(&type_name) {
            Some(f) => f,
            None => continue,
        };
        for config_value in configs {
            let enabled = config_value
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !enabled {
                continue;
            }
            let channel_show_tool_calls = config_value
                .get("showToolCalls")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let show_tool_calls = cfg.show_tool_calls && channel_show_tool_calls;
            let inbound_tx_ch = inbound_tx.clone();
            let outbound_rx = bus.subscribe_outbound();
            let ctx = crate::channels::ChannelStartContext {
                inbound_tx: inbound_tx_ch,
                outbound_rx,
                show_tool_calls,
                tool_result_preview_chars: cfg.tool_result_preview_chars as usize,
                workspace: Some(ws.clone()),
                approval_manager: Some(std::sync::Arc::clone(&approval_manager)),
                completion_model: Some(std::sync::Arc::clone(&completion_model)),
                outbound_tx: Some(bus.outbound_tx_clone()),
            };
            let factory = std::sync::Arc::clone(&factory);
            let type_name = type_name.clone();
            match factory.create(config_value, ctx) {
                Ok(mut ch) => {
                    tokio::spawn(async move {
                        if let Err(e) = ch.start().await {
                            tracing::error!(channel = %type_name, "Channel error: {e:#}");
                        }
                    });
                }
                Err(e) => tracing::error!(channel = %type_name, "Channel init error: {e:#}"),
            }
        }
    }

    // Start web server if enabled
    if cfg.web.enabled {
        let mut web_config = cfg.web.clone();
        // Inside AppContainer the process is network-isolated; bind to 0.0.0.0 so
        // LAN clients can reach the web UI (inbound firewall rule is added by the
        // parent sandbox process for the configured ports).
        #[cfg(target_os = "windows")]
        if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() && web_config.host == "127.0.0.1" {
            web_config.host = "0.0.0.0".to_string();
            info!("AppContainer: web server binding overridden to 0.0.0.0:{}", web_config.port);
        }
        let web_state = crate::web::AppState::new(
            std::sync::Arc::new(cfg.clone()),
            shared_session_state.session_manager.clone(),
            cron_service,
            agent_registry,
            skills_loader,
            inbound_tx.clone(),
            bus.outbound_tx_clone(),
            log_buffer,
            approval_manager,
            permission_policy,
        );

        // Run web server in the main task (it will block until Ctrl+C)
        tokio::select! {
            result = crate::web::start_web_server(web_config, web_state) => {
                if let Err(e) = result {
                    tracing::error!("Web server error: {e:#}");
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Shutting down...");
            }
        }
    } else {
        info!("🐈 synbot daemon running. Press Ctrl+C to stop.");
        tokio::signal::ctrl_c().await?;
        info!("Shutting down...");
    }
    Ok(())
}

/// If app_sandbox or tool_sandbox is configured, create SandboxManager, create/start sandboxes.
/// Returns (manager, Some(tool_sandbox_id)) when tool sandbox is running (exec uses it),
/// or (manager, None) when only app sandbox is running (keeps manager alive so app sandbox is not stopped).
/// When we are already inside the app sandbox (child of `synbot sandbox`), we skip creating/starting app sandbox.
async fn init_sandbox_if_configured(
    cfg: &config::Config,
) -> Option<(std::sync::Arc<crate::sandbox::SandboxManager>, Option<String>)> {
    let in_app_sandbox = std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some();
    let has_app = cfg.app_sandbox.is_some() && !in_app_sandbox;
    let has_tool = cfg.tool_sandbox.is_some();
    if !has_app && !has_tool {
        return None;
    }

    let manager = std::sync::Arc::new(crate::sandbox::SandboxManager::with_defaults());
    let monitoring = &cfg.sandbox_monitoring;
    let mut app_started = false;

    if let Some(_) = cfg.app_sandbox {
        if !in_app_sandbox {
            // App sandbox is meant to be used via `synbot sandbox start`; plain `synbot start` does not
            // create/start the app sandbox. Prompt the user and continue without app sandbox.
            info!(
                "app_sandbox is configured; to run inside the app sandbox use `synbot sandbox start`. \
                Continuing without app sandbox."
            );
        }
        // When in_app_sandbox we are already inside the sandbox (started via `synbot sandbox start`).
    }

    let workspace_path = config::workspace_path(cfg);
    let skills_dir = config::skills_dir();
    if let Some(ref tool_cfg) = cfg.tool_sandbox {
        match config::build_tool_sandbox_config(tool_cfg, monitoring, &workspace_path, &skills_dir) {
            Ok(sandbox_config) => {
                match manager.create_tool_sandbox(sandbox_config).await {
                    Ok(id) => {
                        if let Err(e) = manager.start_sandbox(&id).await {
                            warn!(sandbox_id = %id, error = %e, "Tool sandbox start failed (exec will run on host)");
                        } else {
                            info!(sandbox_id = %id, "Tool sandbox started (exec runs in sandbox)");
                            return Some((manager, Some(id)));
                        }
                    }
                    Err(e) => {
                        let requested = tool_cfg.sandbox_type.as_deref().unwrap_or("gvisor-docker");
                        warn!(
                            error = %e,
                            requested_type = %requested,
                            "Tool sandbox creation failed (exec will run on host). \
                             If you accept a less isolated backend, set toolSandbox.sandboxType in config \
                             (e.g. \"plain-docker\" when gVisor is not available) and restart."
                        );
                    }
                }
            }
            Err(e) => warn!(error = %e, "Tool sandbox config invalid"),
        }
    }

    // Keep manager alive when app sandbox was started so it is not dropped and stopped.
    if app_started {
        Some((manager, None))
    } else {
        None
    }
}
