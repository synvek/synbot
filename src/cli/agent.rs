//! Agent command - Run the agent (one-shot or interactive).

use anyhow::Result;
use tracing::info;
use crate::config;
use crate::logging;
use super::helpers::{resolve_provider, detect_rig_provider, build_default_tools};

pub async fn cmd_agent(message: Option<String>, provider: Option<String>, model: Option<String>) -> Result<()> {
    let cfg = config::load_config(None)?;
    
    // Initialize logging with config
    logging::init_logging(&cfg, None)?;
    
    let ws = config::workspace_path(&cfg);

    // Resolve provider
    let (api_key, api_base) = resolve_provider(&cfg);
    if api_key.is_empty() {
        anyhow::bail!(
            "No API key configured. Run `synbot onboard` and edit {}",
            config::config_path().display()
        );
    }

    let model_name = model.unwrap_or(cfg.agent.model.clone());
    info!(model = %model_name, "Starting agent");

    // Build rig provider via rig-dyn
    let provider_name = provider.unwrap_or(cfg.agent.provider.clone());
    let provider = detect_rig_provider(&provider_name);
    let client = provider.client(&api_key, api_base.as_deref())?;
    let completion_model = client.completion_model(model_name.as_str()).await;

    // Subagent manager (shared via Arc<Mutex<>>)
    let subagent_mgr = std::sync::Arc::new(
        tokio::sync::Mutex::new(
            crate::agent::subagent::SubagentManager::new(
                cfg.agent.max_concurrent_subagents,
            ),
        ),
    );

    // Create approval manager
    let approval_manager = std::sync::Arc::new(crate::tools::approval::ApprovalManager::new());

    // Load permission policy if enabled
    let permission_policy = if cfg.tools.exec.permissions.enabled {
        Some(std::sync::Arc::new(crate::tools::permission::CommandPermissionPolicy::new(
            cfg.tools.exec.permissions.rules.clone(),
            cfg.tools.exec.permissions.default_level,
        )))
    } else {
        None
    };

    // Build tools (pass subagent manager, approval manager, and permission policy)
    let tools = build_default_tools(&cfg, &ws, std::sync::Arc::clone(&subagent_mgr), approval_manager, permission_policy);
    let tools = std::sync::Arc::new(tools);

    // Set up bus
    let mut bus = crate::bus::MessageBus::new();
    let inbound_tx = bus.inbound_sender();
    let inbound_rx = bus.take_inbound_receiver().unwrap();

    // Create session_manager for CLI mode
    let session_manager = std::sync::Arc::new(tokio::sync::RwLock::new(
        crate::agent::session_manager::SessionManager::new(),
    ));

    // Agent loop
    let mut agent_loop = crate::agent::r#loop::AgentLoop::new(
        completion_model,
        ws,
        tools,
        cfg.agent.max_tool_iterations,
        inbound_rx,
        bus.outbound_tx_clone(),
        &cfg,
        session_manager,
    )
    .await;

    // If one-shot message, inject it and collect response
    if let Some(msg) = message {
        let _ = inbound_tx
            .send(crate::bus::InboundMessage {
                channel: "cli".into(),
                sender_id: "user".into(),
                chat_id: "direct".into(),
                content: msg,
                timestamp: chrono::Utc::now(),
                media: vec![],
                metadata: serde_json::Value::Null,
            })
            .await;

        // Spawn outbound printer
        let mut rx = bus.subscribe_outbound();
        let printer = tokio::spawn(async move {
            if let Ok(out) = rx.recv().await {
                match out.message_type {
                    crate::bus::OutboundMessageType::Chat { content, .. } => {
                        println!("{}", content);
                    }
                    crate::bus::OutboundMessageType::ApprovalRequest { request } => {
                        println!("Approval request: {}", request.command);
                    }
                    crate::bus::OutboundMessageType::ToolProgress {
                        tool_name,
                        status,
                        result_preview,
                    } => {
                        println!("[Tool: {}] {} — {}", tool_name, status, result_preview);
                    }
                }
            }
        });

        // Run agent for one turn then exit
        // We'll use a timeout to avoid hanging
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            agent_loop.run(),
        )
        .await;

        printer.abort();
    } else {
        println!("🐈 synbot interactive mode (type 'exit' to quit)");
        loop {
            print!("> ");
            use std::io::Write;
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim();
            if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
                break;
            }
            if input.is_empty() {
                continue;
            }
            let _ = inbound_tx
                .send(crate::bus::InboundMessage {
                    channel: "cli".into(),
                    sender_id: "user".into(),
                    chat_id: "direct".into(),
                    content: input.to_string(),
                    timestamp: chrono::Utc::now(),
                    media: vec![],
                    metadata: serde_json::Value::Null,
                })
                .await;
        }
    }

    Ok(())
}
