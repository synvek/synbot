//! Cron service — scheduling and executing timed jobs.

use anyhow::Result;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::cron::types::{CronJob, CronStore};

pub struct CronService {
    store_path: PathBuf,
    store: CronStore,
    running: bool,
}

impl CronService {
    pub fn new(store_path: PathBuf) -> Self {
        let store = Self::load_store(&store_path).unwrap_or_default();
        Self {
            store_path,
            store,
            running: false,
        }
    }

    fn load_store(path: &PathBuf) -> Result<CronStore> {
        if path.exists() {
            let text = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&text)?)
        } else {
            Ok(CronStore::default())
        }
    }

    fn save_store(&self) -> Result<()> {
        if let Some(parent) = self.store_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.store)?;
        std::fs::write(&self.store_path, json)?;
        Ok(())
    }

    pub fn add_job(&mut self, job: CronJob) -> Result<()> {
        self.store.jobs.push(job);
        self.save_store()
    }

    pub fn remove_job(&mut self, id: &str) -> Result<bool> {
        let before = self.store.jobs.len();
        self.store.jobs.retain(|j| j.id != id);
        let removed = self.store.jobs.len() < before;
        if removed {
            self.save_store()?;
        }
        Ok(removed)
    }

    pub fn list_jobs(&self) -> &[CronJob] {
        &self.store.jobs
    }

    /// Run the cron tick loop. Checks every 10 seconds for due jobs.
    pub async fn run(&mut self, job_tx: mpsc::Sender<CronJob>) -> Result<()> {
        self.running = true;
        info!("Cron service started");
        while self.running {
            let now_ms = chrono::Utc::now().timestamp_millis();
            let mut to_fire = Vec::new();

            for job in &self.store.jobs {
                if !job.enabled {
                    continue;
                }
                if let Some(next) = job.state.next_run_at_ms {
                    if now_ms >= next {
                        to_fire.push(job.clone());
                    }
                }
            }

            for job in to_fire {
                if let Err(e) = job_tx.send(job).await {
                    error!("Failed to dispatch cron job: {e}");
                }
            }

            tokio::time::sleep(Duration::from_secs(10)).await;
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running = false;
    }
}
