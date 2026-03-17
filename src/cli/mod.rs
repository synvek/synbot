//! CLI commands module.

mod onboard;
mod agent;
mod start;
mod cron;
mod sandbox_cmd;
mod service;
mod helpers;

use std::path::PathBuf;
use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};

pub use onboard::cmd_onboard;
pub use agent::cmd_agent;
pub use start::cmd_start;
pub use cron::{cmd_cron, CronAction};
pub use sandbox_cmd::cmd_sandbox;
pub use service::{cmd_service, ServiceAction};

#[derive(Parser)]
#[command(name = "synbot", about = "synbot — Personal AI Assistant")]
struct Cli {
    /// Root directory for this instance (config, roles, memory, sessions). Default: ~/.synbot.
    /// Use different --root-dir to run multiple synbot instances with separate workspaces.
    #[arg(long, value_name = "DIR", global = true)]
    root_dir: Option<PathBuf>,

    /// Print version, OS, architecture, and git tag.
    #[arg(short = 'v', long = "version", global = true)]
    version: bool,

    #[command(subcommand)]
    command: Option<Commands>,
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

        /// Provider override (e.g. "anthropic").
        #[arg(short, long)]
        provider: Option<String>,

        /// Model override (e.g. "claude-sonnet-4-5").
        #[arg(long)]
        model: Option<String>,
    },

    /// Start the full daemon (channels + heartbeat + cron).
    Start,

    /// Run a subcommand inside the app sandbox. Starts the sandbox, then launches `synbot <args..>` as a child process in the container.
    /// Example: `synbot sandbox start` runs `synbot start` inside the sandbox.
    Sandbox {
        /// Subcommand and arguments passed to the child process (e.g. `start`, or `agent --message hello`).
        #[arg(trailing_var_arg = true)]
        child_args: Vec<String>,
    },

    /// Manage cron jobs.
    Cron {
        #[command(subcommand)]
        action: CronAction,
    },

    /// Install, uninstall, start, stop, restart, or show status of the Synbot daemon as a system service (Linux: systemd user, macOS: launchd, Windows: scheduled task).
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();

    if cli.version {
        return cmd_version();
    }

    let Some(command) = cli.command else {
        Cli::command().print_help()?;
        return Ok(());
    };

    crate::config::set_root_dir(cli.root_dir.clone());

    match command {
        Commands::Onboard => cmd_onboard().await,
        Commands::Agent { message, provider, model } => cmd_agent(message, provider, model).await,
        Commands::Start => cmd_start().await,
        Commands::Sandbox { child_args } => cmd_sandbox(child_args).await,
        Commands::Cron { action } => cmd_cron(action).await,
        Commands::Service { action } => cmd_service(action).await,
    }
}

fn cmd_version() -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let git_commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok().map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("synbot {}", version);
    println!("  os:    {}", os);
    println!("  arch:  {}", arch);
    println!("  git:   {}", git_commit);
    Ok(())
}
