//! CLI commands.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::info;

use crate::channels::Channel;
use crate::config;

#[derive(Parser)]
#[command(name = "synbot", about = "synbot — Personal AI Assistant")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize configuration and workspace.
    Onboard,

    /// Run the agent (one-shot or interactive).
    Agent {
        /// Single message to process (non-interactive).
        #[arg(short, long)]
        message: Option<String>,

        /// Model override (e.g. "anthropic/claude-sonnet-4-5").
        #[arg(long)]
        model: Option<String>,
    },

    /// Start the full daemon (channels + heartbeat + cron).
    Start,

    /// Manage cron jobs.
    Cron {
        #[command(subcommand)]
        action: CronAction,
    },
}

#[derive(Subcommand)]
enum CronAction {
    /// List all scheduled jobs.
    List,
    /// Add a new job.
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        message: String,
        #[arg(long)]
        at: Option<String>,
        #[arg(long)]
        every: Option<u64>,
        #[arg(long)]
        cron: Option<String>,
    },
    /// Remove a job by ID.
    Remove {
        id: String,
    },
}

pub async fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "synbot=debug,open_lark=debug".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Onboard => cmd_onboard().await,
        Commands::Agent { message, model } => cmd_agent(message, model).await,
        Commands::Start => cmd_start().await,
        Commands::Cron { action } => cmd_cron(action).await,
    }
}

// ---------------------------------------------------------------------------
// onboard
// ---------------------------------------------------------------------------

async fn cmd_onboard() -> Result<()> {
    let cfg_path = config::config_path();
    if cfg_path.exists() {
        println!("Config already exists at {}", cfg_path.display());
        println!("Delete it first if you want to re-initialize.");
        return Ok(());
    }

    let cfg = config::Config::default();
    config::save_config(&cfg, None)?;
    println!("✓ Created config at {}", cfg_path.display());

    let ws = config::workspace_path(&cfg);
    std::fs::create_dir_all(&ws)?;
    create_workspace_templates(&ws)?;
    println!("✓ Created workspace at {}", ws.display());

    println!("\n🐈 synbot is ready!");
    println!("\nNext steps:");
    println!("  1. Add your API key to {}", cfg_path.display());
    println!("  2. Chat: synbot agent -m \"Hello!\"");
    Ok(())
}

fn create_workspace_templates(ws: &std::path::Path) -> Result<()> {
    let templates = [
        ("AGENTS.md", "# Agent Instructions\n\nYou are a helpful AI assistant. Be concise, accurate, and friendly.\n"),
        ("SOUL.md", "# Soul\n\nI am synbot, a lightweight AI assistant.\n\n## Personality\n\n- Helpful and friendly\n- Concise and to the point\n"),
        ("USER.md", "# User Profile\n\n(Add information about yourself here.)\n"),
        ("TOOLS.md", "# Available Tools\n\nSee tool definitions provided by the agent runtime.\n"),
        ("HEARTBEAT.md", "# Heartbeat Tasks\n\n<!-- Add periodic tasks below -->\n"),
    ];
    for (name, content) in templates {
        let path = ws.join(name);
        if !path.exists() {
            std::fs::write(&path, content)?;
        }
    }
    std::fs::create_dir_all(ws.join("memory"))?;
    std::fs::create_dir_all(ws.join("skills"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// agent (one-shot)
// ---------------------------------------------------------------------------

async fn cmd_agent(message: Option<String>, model: Option<String>) -> Result<()> {
    let cfg = config::load_config(None)?;
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
    let provider = detect_rig_provider(&model_name);
    let client = provider.client(&api_key, api_base.as_deref())?;
    let model = extract_model(&model_name);
    let completion_model = client.completion_model(model.as_str()).await;

    // Subagent manager (shared via Arc<Mutex<>>)
    let subagent_mgr = std::sync::Arc::new(
        tokio::sync::Mutex::new(
            crate::agent::subagent::SubagentManager::new(
                cfg.agent.max_concurrent_subagents,
            ),
        ),
    );

    // Build tools (pass subagent manager to SpawnTool)
    let tools = build_default_tools(&cfg, &ws, std::sync::Arc::clone(&subagent_mgr));
    let tools = std::sync::Arc::new(tools);

    // Set up bus
    let mut bus = crate::bus::MessageBus::new();
    let inbound_tx = bus.inbound_sender();
    let inbound_rx = bus.take_inbound_receiver().unwrap();

    // Agent loop
    let mut agent_loop = crate::agent::r#loop::AgentLoop::new(
        completion_model,
        ws,
        tools,
        cfg.agent.max_tool_iterations,
        inbound_rx,
        bus.outbound_tx_clone(),
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
                println!("{}", out.content);
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

// ---------------------------------------------------------------------------
// start (daemon)
// ---------------------------------------------------------------------------

async fn cmd_start() -> Result<()> {
    let cfg = config::load_config(None)?;
    let ws = config::workspace_path(&cfg);

    let (api_key, api_base) = resolve_provider(&cfg);
    if api_key.is_empty() {
        anyhow::bail!("No API key configured.");
    }

    let model_name = cfg.agent.model.clone();
    let provider = detect_rig_provider(&model_name);
    let client = provider.client(&api_key, api_base.as_deref())?;
    let model = extract_model(&model_name);
    let completion_model = client.completion_model(&model).await;

    // Subagent manager (shared via Arc<Mutex<>>)
    let subagent_mgr = std::sync::Arc::new(
        tokio::sync::Mutex::new(
            crate::agent::subagent::SubagentManager::new(
                cfg.agent.max_concurrent_subagents,
            ),
        ),
    );

    let tools = std::sync::Arc::new(build_default_tools(&cfg, &ws, std::sync::Arc::clone(&subagent_mgr)));

    let mut bus = crate::bus::MessageBus::new();
    let inbound_tx = bus.inbound_sender();
    let inbound_rx = bus.take_inbound_receiver().unwrap();

    // Start agent loop
    let mut agent_loop = crate::agent::r#loop::AgentLoop::new(
        completion_model,
        ws.clone(),
        tools,
        cfg.agent.max_tool_iterations,
        inbound_rx,
        bus.outbound_tx_clone(),
    )
    .await;
    tokio::spawn(async move {
        if let Err(e) = agent_loop.run().await {
            tracing::error!("Agent loop error: {e:#}");
        }
    });

    // Start heartbeat
    let hb = crate::heartbeat::HeartbeatService::new(&ws, true);
    let hb_tx = inbound_tx.clone();
    tokio::spawn(async move {
        if let Err(e) = hb.run(hb_tx).await {
            tracing::error!("Heartbeat error: {e:#}");
        }
    });

    // Start Feishu channel if enabled
    if cfg.channels.feishu.enabled {
        let tg_inbound = inbound_tx.clone();
        let tg_outbound = bus.subscribe_outbound();
        let tg_cfg = cfg.channels.feishu.clone();
        tokio::spawn(async move {
            let mut ch = crate::channels::feishu::FeishuChannel::new(
                tg_cfg, tg_inbound, tg_outbound,
            );
            if let Err(e) = ch.start().await {
                tracing::error!("Feishu channel error: {e:#}");
            }
        });
    }

    // Start Discord channel if enabled
    if cfg.channels.discord.enabled {
        let dc_inbound = inbound_tx.clone();
        let dc_outbound = bus.subscribe_outbound();
        let dc_cfg = cfg.channels.discord.clone();
        tokio::spawn(async move {
            let mut ch = crate::channels::discord::DiscordChannel::new(
                dc_cfg, dc_inbound, dc_outbound,
            );
            if let Err(e) = ch.start().await {
                tracing::error!("Discord channel error: {e:#}");
            }
        });
    }

    info!("🐈 synbot daemon running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    info!("Shutting down...");
    Ok(())
}

// ---------------------------------------------------------------------------
// cron
// ---------------------------------------------------------------------------

async fn cmd_cron(action: CronAction) -> Result<()> {
    let _cfg = config::load_config(None)?;
    let store_path = config::config_dir().join("cron").join("jobs.json");
    let mut svc = crate::cron::service::CronService::new(store_path);

    match action {
        CronAction::List => {
            let jobs = svc.list_jobs();
            if jobs.is_empty() {
                println!("No scheduled jobs.");
            } else {
                for j in jobs {
                    println!("[{}] {} (enabled: {})", j.id, j.name, j.enabled);
                }
            }
        }
        CronAction::Add { name, message, at, every, cron } => {
            use crate::cron::types::*;
            let schedule = if let Some(at_str) = at {
                let dt = chrono::DateTime::parse_from_rfc3339(&at_str)
                    .or_else(|_| chrono::NaiveDateTime::parse_from_str(&at_str, "%Y-%m-%dT%H:%M:%S")
                        .map(|ndt| ndt.and_utc().fixed_offset()))
                    .context("Invalid --at timestamp")?;
                Schedule::At { at_ms: dt.timestamp_millis() }
            } else if let Some(secs) = every {
                Schedule::Every { every_ms: (secs * 1000) as i64 }
            } else if let Some(expr) = cron {
                Schedule::Cron { expr, tz: None }
            } else {
                anyhow::bail!("Provide --at, --every, or --cron");
            };

            let job = CronJob {
                id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
                name,
                enabled: true,
                schedule,
                payload: CronPayload { message, ..Default::default() },
                state: CronJobState::default(),
                created_at_ms: chrono::Utc::now().timestamp_millis(),
                updated_at_ms: chrono::Utc::now().timestamp_millis(),
                delete_after_run: false,
            };
            let id = job.id.clone();
            svc.add_job(job)?;
            println!("✓ Added job {id}");
        }
        CronAction::Remove { id } => {
            if svc.remove_job(&id)? {
                println!("✓ Removed job {id}");
            } else {
                println!("Job {id} not found.");
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_provider(cfg: &config::Config) -> (String, Option<String>) {
    // Priority: openrouter > anthropic > openai > deepseek
    if !cfg.providers.openrouter.api_key.is_empty() {
        return (cfg.providers.openrouter.api_key.clone(), cfg.providers.openrouter.api_base.clone());
    }
    if !cfg.providers.anthropic.api_key.is_empty() {
        return (cfg.providers.anthropic.api_key.clone(), cfg.providers.anthropic.api_base.clone());
    }
    if !cfg.providers.openai.api_key.is_empty() {
        return (cfg.providers.openai.api_key.clone(), cfg.providers.openai.api_base.clone());
    }
    if !cfg.providers.deepseek.api_key.is_empty() {
        return (cfg.providers.deepseek.api_key.clone(), cfg.providers.deepseek.api_base.clone());
    }
    if !cfg.providers.ollama.api_key.is_empty() {
        return (cfg.providers.ollama.api_key.clone(), cfg.providers.ollama.api_base.clone());
    }
    (String::new(), None)
}

fn detect_rig_provider(model: &str) -> rig_dyn::Provider {
    let lower = model.to_lowercase();
    if lower.contains("anthropic") || lower.contains("claude") {
        rig_dyn::Provider::Anthropic
    } else if lower.contains("openai") || lower.contains("gpt") {
        rig_dyn::Provider::OpenAI
    } else if lower.contains("deepseek") {
        rig_dyn::Provider::DeepSeek
    } else if lower.contains("ollama") {
        rig_dyn::Provider::Ollama
    } else {
        // Default to OpenAI-compatible (works with OpenRouter etc.)
        rig_dyn::Provider::OpenAI
    }
}

fn extract_model(model: &str) -> String {
    let lower = model.to_lowercase();
    if lower.contains("anthropic") || lower.contains("claude") {
        model.to_string()
    } else if lower.contains("openai") || lower.contains("gpt") {
        model.to_string()
    } else if lower.contains("deepseek") {
        model.split_once('/').unwrap().1.to_string()
    } else if lower.contains("ollama") {
        model.split_once('/').unwrap().1.to_string()
    } else {
        // Default to OpenAI-compatible (works with OpenRouter etc.)
        model.to_string()
    }
}
fn build_default_tools(
    cfg: &config::Config,
    ws: &std::path::Path,
    subagent_mgr: std::sync::Arc<tokio::sync::Mutex<crate::agent::subagent::SubagentManager>>,
) -> crate::tools::ToolRegistry {
    use crate::tools::*;
    let restrict = cfg.tools.exec.restrict_to_workspace;
    let ws = ws.to_path_buf();

    let mut reg = ToolRegistry::new();
    reg.register(std::sync::Arc::new(filesystem::ReadFileTool { workspace: ws.clone(), restrict })).expect("register ReadFileTool");
    reg.register(std::sync::Arc::new(filesystem::WriteFileTool { workspace: ws.clone(), restrict })).expect("register WriteFileTool");
    reg.register(std::sync::Arc::new(filesystem::EditFileTool { workspace: ws.clone(), restrict })).expect("register EditFileTool");
    reg.register(std::sync::Arc::new(filesystem::ListDirTool { workspace: ws.clone(), restrict })).expect("register ListDirTool");
    reg.register(std::sync::Arc::new(shell::ExecTool {
        workspace: ws.clone(),
        timeout_secs: cfg.tools.exec.timeout_secs,
        restrict_to_workspace: restrict,
        policy: shell::CommandPolicy::new(
            cfg.tools.exec.deny_patterns.clone(),
            cfg.tools.exec.allow_patterns.clone(),
        ),
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
    reg
}
