//! CLI commands module.

mod onboard;
mod agent;
mod start;
mod cron;
mod sandbox_cmd;
mod helpers;

use anyhow::Result;
use clap::{Parser, Subcommand};

pub use onboard::cmd_onboard;
pub use agent::cmd_agent;
pub use start::cmd_start;
pub use cron::{cmd_cron, CronAction};
pub use sandbox_cmd::cmd_sandbox;

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
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Onboard => cmd_onboard().await,
        Commands::Agent { message, provider, model } => cmd_agent(message, provider, model).await,
        Commands::Start => cmd_start().await,
        Commands::Sandbox { child_args } => cmd_sandbox(child_args).await,
        Commands::Cron { action } => cmd_cron(action).await,
    }
}
