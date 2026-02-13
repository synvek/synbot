//! CLI commands module.

mod onboard;
mod agent;
mod start;
mod cron;
mod helpers;

use anyhow::Result;
use clap::{Parser, Subcommand};

pub use onboard::cmd_onboard;
pub use agent::cmd_agent;
pub use start::cmd_start;
pub use cron::{cmd_cron, CronAction};

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
        Commands::Cron { action } => cmd_cron(action).await,
    }
}
