//! Start command - Start the full daemon (channels + heartbeat + cron).

use anyhow::Result;
use tracing::{info, warn};
use crate::config;
use crate::logging;
use crate::channels::Channel;
use super::helpers::{resolve_provider, detect_rig_provider, build_default_tools};

pub async fn cmd_start() -> Result<()> {
    let cfg = config::load_config(None)?;
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
    
    let ws = config::workspace_path(&cfg);

    let (api_key, api_base) = resolve_provider(&cfg);
    if api_key.is_empty() {
        anyhow::bail!("No API key configured.");
    }

    let model = cfg.agent.model.clone();
    let provider_name = cfg.agent.provider.clone();
    let provider = detect_rig_provider(&provider_name);
    let client = provider.client(&api_key, api_base.as_deref())?;
    let completion_model = client.completion_model(&model).await;
    let completion_model = std::sync::Arc::from(completion_model);

    // Subagent manager (shared via Arc<Mutex<>>)
    let subagent_mgr = std::sync::Arc::new(
        tokio::sync::Mutex::new(
            crate::agent::subagent::SubagentManager::new(
                cfg.agent.max_concurrent_subagents,
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

    let tools = std::sync::Arc::new(build_default_tools(
        &cfg,
        &ws,
        std::sync::Arc::clone(&subagent_mgr),
        std::sync::Arc::clone(&approval_manager),
        permission_policy.clone(),
        heartbeat_cron_ctx,
        &sandbox_context,
    ));

    // Initialize components for web server
    let session_manager = std::sync::Arc::new(tokio::sync::RwLock::new(
        crate::agent::session_manager::SessionManager::new(),
    ));

    let mut role_registry = crate::agent::role_registry::RoleRegistry::new();
    let roles_dir = config::roles_dir();
    role_registry.load_from_config(&cfg.agent.roles, &cfg.agent, &ws, &roles_dir)?;
    let role_registry = std::sync::Arc::new(role_registry);

    // Ensure memory dirs and MEMORY.md exist under ~/.synbot/memory/{agentId} (main + each role)
    crate::agent::memory::ensure_memory_dirs(&cfg);

    // Create main agent's SQLite index file so it exists and can be populated by reindex later
    #[cfg(feature = "memory-index")]
    {
        let _ = crate::agent::memory_index::open_index("main");
    }

    let skills_dir = config::skills_dir();
    if let Err(e) = std::fs::create_dir_all(&skills_dir) {
        tracing::warn!(path = %skills_dir.display(), error = %e, "Could not create skills dir");
    }
    let skills_loader = std::sync::Arc::new(crate::agent::skills::SkillsLoader::new(&skills_dir));

    let cron_store_path = config::config_dir().join("cron").join("jobs.json");
    let cron_service = std::sync::Arc::new(tokio::sync::RwLock::new(
        crate::cron::service::CronService::new(cron_store_path),
    ));

    // Start agent loop
    let mut agent_loop = crate::agent::r#loop::AgentLoop::new(
        std::sync::Arc::clone(&completion_model),
        ws.clone(),
        tools,
        cfg.agent.max_tool_iterations,
        inbound_rx,
        bus.outbound_tx_clone(),
        &cfg,
        std::sync::Arc::clone(&session_manager),
    )
    .await;
    tokio::spawn(async move {
        if let Err(e) = agent_loop.run().await {
            tracing::error!("Agent loop error: {e:#}");
        }
    });

    // Start heartbeat (reads config.heartbeat.tasks each interval)
    let hb = crate::heartbeat::HeartbeatService::new(std::sync::Arc::clone(&shared_config));
    let hb_tx = inbound_tx.clone();
    tokio::spawn(async move {
        if let Err(e) = hb.run(hb_tx).await {
            tracing::error!("Heartbeat error: {e:#}");
        }
    });

    // Start config-based cron runner (reads config.cron.tasks, fires due jobs)
    let cron_runner = crate::cron::config_runner::ConfigCronRunner::new(std::sync::Arc::clone(&shared_config));
    let cron_inbound = inbound_tx.clone();
    tokio::spawn(async move {
        if let Err(e) = cron_runner.run(cron_inbound).await {
            tracing::error!("Config cron runner error: {e:#}");
        }
    });

    for tg_cfg in &cfg.channels.telegram {
        if !tg_cfg.enabled {
            continue;
        }
        let tg_cfg = tg_cfg.clone();
        let tg_inbound = inbound_tx.clone();
        let tg_outbound = bus.subscribe_outbound();
        let show_tool_calls = cfg.show_tool_calls && tg_cfg.show_tool_calls;
        tokio::spawn(async move {
            let mut ch = crate::channels::telegram::TelegramChannel::new(
                tg_cfg, tg_inbound, tg_outbound, show_tool_calls,
            );
            if let Err(e) = ch.start().await {
                tracing::error!("Telegram channel error: {e:#}");
            }
        });
    }

    for feishu_cfg in &cfg.channels.feishu {
        if !feishu_cfg.enabled {
            continue;
        }
        let feishu_cfg = feishu_cfg.clone();
        let feishu_inbound = inbound_tx.clone();
        let feishu_outbound = bus.subscribe_outbound();
        let show_tool_calls = cfg.show_tool_calls && feishu_cfg.show_tool_calls;
        let feishu_approval = std::sync::Arc::clone(&approval_manager);
        let feishu_classifier = std::sync::Arc::clone(&completion_model);
        tokio::spawn(async move {
            let mut ch = crate::channels::feishu::FeishuChannel::new(
                feishu_cfg, feishu_inbound, feishu_outbound, show_tool_calls,
            )
            .with_approval_manager(feishu_approval)
            .with_approval_classifier(feishu_classifier);
            if let Err(e) = ch.start().await {
                tracing::error!("Feishu channel error: {e:#}");
            }
        });
    }

    for dc_cfg in &cfg.channels.discord {
        if !dc_cfg.enabled {
            continue;
        }
        let dc_cfg = dc_cfg.clone();
        let dc_inbound = inbound_tx.clone();
        let dc_outbound = bus.subscribe_outbound();
        let show_tool_calls = cfg.show_tool_calls && dc_cfg.show_tool_calls;
        tokio::spawn(async move {
            let mut ch = crate::channels::discord::DiscordChannel::new(
                dc_cfg, dc_inbound, dc_outbound, show_tool_calls,
            );
            if let Err(e) = ch.start().await {
                tracing::error!("Discord channel error: {e:#}");
            }
        });
    }

    // Start web server if enabled
    if cfg.web.enabled {
        let web_config = cfg.web.clone();
        let web_state = crate::web::AppState::new(
            std::sync::Arc::new(cfg.clone()),
            session_manager,
            cron_service,
            role_registry,
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
async fn init_sandbox_if_configured(
    cfg: &config::Config,
) -> Option<(std::sync::Arc<crate::sandbox::SandboxManager>, Option<String>)> {
    let has_app = cfg.app_sandbox.is_some();
    let has_tool = cfg.tool_sandbox.is_some();
    if !has_app && !has_tool {
        return None;
    }

    let manager = std::sync::Arc::new(crate::sandbox::SandboxManager::with_defaults());
    let monitoring = &cfg.sandbox_monitoring;
    let mut app_started = false;

    if let Some(ref app_cfg) = cfg.app_sandbox {
        match config::build_app_sandbox_config(app_cfg, monitoring) {
            Ok(sandbox_config) => {
                match manager.create_app_sandbox(sandbox_config).await {
                    Ok(id) => {
                        if let Err(e) = manager.start_sandbox(&id).await {
                            warn!(sandbox_id = %id, error = %e, "App sandbox start failed");
                        } else {
                            app_started = true;
                            info!(sandbox_id = %id, "App sandbox started");
                        }
                    }
                    Err(e) => warn!(error = %e, "App sandbox creation failed (daemon runs without app sandbox)"),
                }
            }
            Err(e) => warn!(error = %e, "App sandbox config invalid"),
        }
    }

    let tool_sandbox_id = "synbot-tool".to_string();
    if let Some(ref tool_cfg) = cfg.tool_sandbox {
        match config::build_tool_sandbox_config(tool_cfg, monitoring) {
            Ok(sandbox_config) => {
                match manager.create_tool_sandbox(sandbox_config).await {
                    Ok(id) => {
                        if let Err(e) = manager.start_sandbox(&id).await {
                            warn!(sandbox_id = %id, error = %e, "Tool sandbox start failed (exec will run on host)");
                        } else {
                            info!(sandbox_id = %id, "Tool sandbox started (exec runs in sandbox)");
                            return Some((manager, Some(tool_sandbox_id)));
                        }
                    }
                    Err(e) => warn!(error = %e, "Tool sandbox creation failed (exec will run on host)"),
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
