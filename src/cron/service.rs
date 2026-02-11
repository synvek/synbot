//! Cron service — scheduling and executing timed jobs.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::cron::types::{CronJob, CronStore, Schedule};

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

    pub fn job_count(&self) -> usize {
        self.store.jobs.len()
    }

    /// Update the enabled status of a job by ID.
    pub fn update_job_enabled(&mut self, id: &str, enabled: bool) -> Result<bool> {
        if let Some(job) = self.store.jobs.iter_mut().find(|j| j.id == id) {
            job.enabled = enabled;
            job.updated_at_ms = chrono::Utc::now().timestamp_millis();
            self.save_store()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Compute the next run time (in epoch milliseconds) for a given job.
    ///
    /// - `Schedule::Cron { expr, .. }`: parse the cron expression and find the
    ///   next occurrence after now.
    /// - `Schedule::Every { every_ms }`: return `now_ms + every_ms`.
    /// - `Schedule::At { at_ms }`: return the fixed timestamp.
    pub fn compute_next_run(job: &CronJob) -> Option<i64> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        match &job.schedule {
            Schedule::Cron { expr, .. } => {
                let schedule = cron::Schedule::from_str(expr).ok()?;
                let next = schedule.upcoming(chrono::Utc).next()?;
                Some(next.timestamp_millis())
            }
            Schedule::Every { every_ms } => Some(now_ms + every_ms),
            Schedule::At { at_ms } => Some(*at_ms),
        }
    }

    /// Update job state after execution.
    ///
    /// - Sets `last_run_at_ms` to the current time.
    /// - Sets `last_status` to the provided status string.
    /// - For `Every` schedules: recomputes `next_run_at_ms = now + every_ms`.
    /// - For `Cron` schedules: recomputes `next_run_at_ms` from the expression.
    /// - For `At` schedules with `delete_after_run = true`: removes the job.
    /// - Persists the updated store to disk.
    pub fn mark_job_executed(&mut self, job_id: &str, status: &str) -> Result<()> {
        let now_ms = chrono::Utc::now().timestamp_millis();

        // Check if this is an At + delete_after_run job first
        let should_delete = self
            .store
            .jobs
            .iter()
            .find(|j| j.id == job_id)
            .map(|j| matches!(j.schedule, Schedule::At { .. }) && j.delete_after_run)
            .unwrap_or(false);

        if should_delete {
            self.store.jobs.retain(|j| j.id != job_id);
            info!(job_id = job_id, "Removed one-shot job after execution");
            return self
                .save_store()
                .context("Failed to save store after deleting one-shot job");
        }

        // Find the job and update its state
        let job = self
            .store
            .jobs
            .iter_mut()
            .find(|j| j.id == job_id)
            .context(format!("Job not found: {job_id}"))?;

        job.state.last_run_at_ms = Some(now_ms);
        job.state.last_status = Some(status.to_string());

        // Recompute next_run_at_ms based on schedule type
        match &job.schedule {
            Schedule::Every { every_ms } => {
                job.state.next_run_at_ms = Some(now_ms + every_ms);
            }
            Schedule::Cron { expr, .. } => {
                if let Ok(schedule) = cron::Schedule::from_str(expr) {
                    if let Some(next) = schedule.upcoming(chrono::Utc).next() {
                        job.state.next_run_at_ms = Some(next.timestamp_millis());
                    }
                }
            }
            Schedule::At { .. } => {
                // Non-delete At jobs: clear next_run_at_ms so they don't fire again
                job.state.next_run_at_ms = None;
            }
        }

        self.save_store()
            .context("Failed to save store after marking job executed")
    }

    /// Initialize `next_run_at_ms` for any job that doesn't have it set yet.
    fn initialize_new_jobs(&mut self) {
        let now_ms = chrono::Utc::now().timestamp_millis();
        for job in &mut self.store.jobs {
            if job.enabled && job.state.next_run_at_ms.is_none() {
                let next = match &job.schedule {
                    Schedule::Cron { expr, .. } => {
                        cron::Schedule::from_str(expr)
                            .ok()
                            .and_then(|s| s.upcoming(chrono::Utc).next())
                            .map(|dt| dt.timestamp_millis())
                    }
                    Schedule::Every { every_ms } => Some(now_ms + every_ms),
                    Schedule::At { at_ms } => Some(*at_ms),
                };
                if let Some(next_ms) = next {
                    info!(
                        job_id = %job.id,
                        job_name = %job.name,
                        next_run_at_ms = next_ms,
                        "Initialized next_run_at_ms for new job"
                    );
                    job.state.next_run_at_ms = Some(next_ms);
                } else {
                    warn!(
                        job_id = %job.id,
                        job_name = %job.name,
                        "Could not compute next_run_at_ms for job"
                    );
                }
            }
        }
    }

    /// Run the cron tick loop. Checks every 10 seconds for due jobs.
    pub async fn run(&mut self, job_tx: mpsc::Sender<CronJob>) -> Result<()> {
        self.running = true;
        info!("Cron service started");

        // Initialize next_run_at_ms for any jobs that don't have it set
        self.initialize_new_jobs();
        if let Err(e) = self.save_store() {
            warn!("Failed to persist initialized job state: {e}");
        }

        while self.running {
            let now_ms = chrono::Utc::now().timestamp_millis();
            let mut fired_ids = Vec::new();

            // Collect jobs that are due to fire
            for job in &self.store.jobs {
                if !job.enabled {
                    continue;
                }
                if let Some(next) = job.state.next_run_at_ms {
                    if now_ms >= next {
                        fired_ids.push((job.id.clone(), job.clone()));
                    }
                }
            }

            // Dispatch fired jobs and update their state
            for (id, job) in fired_ids {
                let schedule_type = match &job.schedule {
                    Schedule::Cron { .. } => "cron",
                    Schedule::Every { .. } => "every",
                    Schedule::At { .. } => "at",
                };
                info!(
                    job_id = %id,
                    job_name = %job.name,
                    schedule_type = %schedule_type,
                    "Dispatching cron job"
                );
                if let Err(e) = job_tx.send(job).await {
                    error!(
                        job_id = %id,
                        schedule_type = %schedule_type,
                        error = %e,
                        "Failed to dispatch cron job"
                    );
                    if let Err(e) = self.mark_job_executed(&id, "dispatch_error") {
                        error!(
                            job_id = %id,
                            error = %e,
                            "Failed to mark job as dispatch_error"
                        );
                    }
                    continue;
                }
                info!(
                    job_id = %id,
                    schedule_type = %schedule_type,
                    execution_result = "ok",
                    "Cron job dispatched successfully"
                );
                if let Err(e) = self.mark_job_executed(&id, "ok") {
                    error!(
                        job_id = %id,
                        error = %e,
                        "Failed to mark job as executed"
                    );
                }
            }

            // Initialize any newly added jobs
            self.initialize_new_jobs();

            tokio::time::sleep(Duration::from_secs(10)).await;
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cron::types::*;
    use tempfile::TempDir;

    /// Helper: create a CronService backed by a temp directory.
    fn temp_service() -> (CronService, TempDir) {
        let dir = TempDir::new().unwrap();
        let store_path = dir.path().join("jobs.json");
        let svc = CronService::new(store_path);
        (svc, dir)
    }

    /// Helper: build a CronJob with the given schedule.
    fn make_job(id: &str, schedule: Schedule, delete_after_run: bool) -> CronJob {
        CronJob {
            id: id.to_string(),
            name: format!("test-{id}"),
            enabled: true,
            schedule,
            payload: CronPayload::default(),
            state: CronJobState::default(),
            created_at_ms: 0,
            updated_at_ms: 0,
            delete_after_run,
        }
    }

    // ---------------------------------------------------------------
    // compute_next_run tests
    // ---------------------------------------------------------------

    #[test]
    fn compute_next_run_every_returns_future_timestamp() {
        let job = make_job("e1", Schedule::Every { every_ms: 5000 }, false);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let next = CronService::compute_next_run(&job).unwrap();
        // next should be approximately now + 5000ms (allow small delta)
        assert!(next >= now_ms + 4900, "next={next}, now={now_ms}");
        assert!(next <= now_ms + 6000, "next={next}, now={now_ms}");
    }

    #[test]
    fn compute_next_run_at_returns_fixed_timestamp() {
        let fixed_ts = 1_700_000_000_000i64;
        let job = make_job("a1", Schedule::At { at_ms: fixed_ts }, false);
        let next = CronService::compute_next_run(&job).unwrap();
        assert_eq!(next, fixed_ts);
    }

    #[test]
    fn compute_next_run_cron_returns_future() {
        // "every second" expression — next occurrence must be in the future
        let job = make_job(
            "c1",
            Schedule::Cron {
                expr: "* * * * * * *".to_string(),
                tz: None,
            },
            false,
        );
        let now_ms = chrono::Utc::now().timestamp_millis();
        let next = CronService::compute_next_run(&job).unwrap();
        assert!(next > now_ms, "next={next} should be > now={now_ms}");
    }

    #[test]
    fn compute_next_run_invalid_cron_returns_none() {
        let job = make_job(
            "bad",
            Schedule::Cron {
                expr: "not a cron".to_string(),
                tz: None,
            },
            false,
        );
        assert!(CronService::compute_next_run(&job).is_none());
    }

    // ---------------------------------------------------------------
    // mark_job_executed tests
    // ---------------------------------------------------------------

    #[test]
    fn mark_job_executed_updates_state_for_every() {
        let (mut svc, _dir) = temp_service();
        let job = make_job("e1", Schedule::Every { every_ms: 60_000 }, false);
        svc.add_job(job).unwrap();

        svc.mark_job_executed("e1", "ok").unwrap();

        let j = svc.store.jobs.iter().find(|j| j.id == "e1").unwrap();
        assert_eq!(j.state.last_status.as_deref(), Some("ok"));
        assert!(j.state.last_run_at_ms.is_some());
        // next_run should be roughly now + 60_000
        let next = j.state.next_run_at_ms.unwrap();
        let now_ms = chrono::Utc::now().timestamp_millis();
        assert!(next >= now_ms + 59_000, "next={next}, now={now_ms}");
        assert!(next <= now_ms + 61_000, "next={next}, now={now_ms}");
    }

    #[test]
    fn mark_job_executed_updates_state_for_cron() {
        let (mut svc, _dir) = temp_service();
        let job = make_job(
            "c1",
            Schedule::Cron {
                expr: "* * * * * * *".to_string(),
                tz: None,
            },
            false,
        );
        svc.add_job(job).unwrap();

        svc.mark_job_executed("c1", "ok").unwrap();

        let j = svc.store.jobs.iter().find(|j| j.id == "c1").unwrap();
        assert_eq!(j.state.last_status.as_deref(), Some("ok"));
        assert!(j.state.last_run_at_ms.is_some());
        // next_run should be in the future
        let next = j.state.next_run_at_ms.unwrap();
        let now_ms = chrono::Utc::now().timestamp_millis();
        assert!(next > now_ms, "next={next} should be > now={now_ms}");
    }

    #[test]
    fn mark_job_executed_deletes_at_job_with_delete_after_run() {
        let (mut svc, _dir) = temp_service();
        let job = make_job("a1", Schedule::At { at_ms: 1_000 }, true);
        svc.add_job(job).unwrap();
        assert_eq!(svc.store.jobs.len(), 1);

        svc.mark_job_executed("a1", "ok").unwrap();

        // Job should be removed
        assert!(svc.store.jobs.is_empty());
    }

    #[test]
    fn mark_job_executed_keeps_at_job_without_delete_after_run() {
        let (mut svc, _dir) = temp_service();
        let job = make_job("a2", Schedule::At { at_ms: 1_000 }, false);
        svc.add_job(job).unwrap();

        svc.mark_job_executed("a2", "ok").unwrap();

        let j = svc.store.jobs.iter().find(|j| j.id == "a2").unwrap();
        assert_eq!(j.state.last_status.as_deref(), Some("ok"));
        assert!(j.state.last_run_at_ms.is_some());
        // next_run should be cleared so it doesn't fire again
        assert!(j.state.next_run_at_ms.is_none());
    }

    #[test]
    fn mark_job_executed_returns_error_for_missing_job() {
        let (mut svc, _dir) = temp_service();
        let result = svc.mark_job_executed("nonexistent", "ok");
        assert!(result.is_err());
    }

    #[test]
    fn mark_job_executed_persists_to_disk() {
        let (mut svc, dir) = temp_service();
        let job = make_job("p1", Schedule::Every { every_ms: 1000 }, false);
        svc.add_job(job).unwrap();

        svc.mark_job_executed("p1", "ok").unwrap();

        // Reload from disk and verify
        let store_path = dir.path().join("jobs.json");
        let reloaded = CronService::load_store(&store_path).unwrap();
        let j = reloaded.jobs.iter().find(|j| j.id == "p1").unwrap();
        assert_eq!(j.state.last_status.as_deref(), Some("ok"));
        assert!(j.state.last_run_at_ms.is_some());
    }

    // ---------------------------------------------------------------
    // initialize_new_jobs tests
    // ---------------------------------------------------------------

    #[test]
    fn initialize_new_jobs_sets_next_run_for_uninitialized() {
        let (mut svc, _dir) = temp_service();
        let job = make_job("i1", Schedule::Every { every_ms: 30_000 }, false);
        svc.add_job(job).unwrap();

        // next_run_at_ms should be None initially
        assert!(svc.store.jobs[0].state.next_run_at_ms.is_none());

        svc.initialize_new_jobs();

        let next = svc.store.jobs[0].state.next_run_at_ms.unwrap();
        let now_ms = chrono::Utc::now().timestamp_millis();
        assert!(next >= now_ms + 29_000);
        assert!(next <= now_ms + 31_000);
    }

    #[test]
    fn initialize_new_jobs_skips_disabled() {
        let (mut svc, _dir) = temp_service();
        let mut job = make_job("d1", Schedule::Every { every_ms: 1000 }, false);
        job.enabled = false;
        svc.add_job(job).unwrap();

        svc.initialize_new_jobs();

        assert!(svc.store.jobs[0].state.next_run_at_ms.is_none());
    }

    #[test]
    fn initialize_new_jobs_skips_already_initialized() {
        let (mut svc, _dir) = temp_service();
        let mut job = make_job("s1", Schedule::Every { every_ms: 1000 }, false);
        job.state.next_run_at_ms = Some(999);
        svc.add_job(job).unwrap();

        svc.initialize_new_jobs();

        // Should not be overwritten
        assert_eq!(svc.store.jobs[0].state.next_run_at_ms, Some(999));
    }
}
