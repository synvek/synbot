//! Cron command - Manage cron jobs.

use anyhow::{Context, Result};
use clap::Subcommand;
use crate::config;

#[derive(Subcommand)]
pub enum CronAction {
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

pub async fn cmd_cron(action: CronAction) -> Result<()> {
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
