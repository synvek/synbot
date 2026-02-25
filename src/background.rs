//! Background services — abstraction for runnable tasks (heartbeat, cron, and plugin services).

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use crate::bus::InboundMessage;
use crate::config::Config;

/// Context passed to each background service when it runs (inbound sender, shared config).
#[derive(Clone)]
pub struct BackgroundContext {
    pub inbound_tx: mpsc::Sender<InboundMessage>,
    pub config: Arc<RwLock<Config>>,
}

/// A service that runs in the background (e.g. heartbeat, cron, or a plugin).
#[async_trait::async_trait]
pub trait BackgroundService: Send + Sync {
    fn name(&self) -> &str;

    /// Run the service until it exits or errors. The service receives the shared context.
    async fn run(&self, ctx: BackgroundContext) -> Result<()>;
}

/// Registry of background services. Built-in and plugin services are registered and then
/// started together from [crate::cli::start::cmd_start].
pub struct BackgroundServiceRegistry {
    services: Vec<Arc<dyn BackgroundService>>,
}

impl BackgroundServiceRegistry {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
        }
    }

    pub fn register(&mut self, service: Arc<dyn BackgroundService>) {
        self.services.push(service);
    }

    /// Return all registered services (for spawning).
    pub fn services(&self) -> &[Arc<dyn BackgroundService>] {
        &self.services
    }
}

impl Default for BackgroundServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Built-in service adapters
// ---------------------------------------------------------------------------

/// Wraps [crate::heartbeat::HeartbeatService] as a [BackgroundService].
pub struct HeartbeatBackgroundService {
    inner: crate::heartbeat::HeartbeatService,
}

impl HeartbeatBackgroundService {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self {
            inner: crate::heartbeat::HeartbeatService::new(config),
        }
    }
}

#[async_trait::async_trait]
impl BackgroundService for HeartbeatBackgroundService {
    fn name(&self) -> &str {
        "heartbeat"
    }

    async fn run(&self, ctx: BackgroundContext) -> Result<()> {
        self.inner.run(ctx.inbound_tx).await
    }
}

/// Wraps [crate::cron::config_runner::ConfigCronRunner] as a [BackgroundService].
pub struct CronBackgroundService {
    inner: crate::cron::config_runner::ConfigCronRunner,
}

impl CronBackgroundService {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self {
            inner: crate::cron::config_runner::ConfigCronRunner::new(config),
        }
    }
}

#[async_trait::async_trait]
impl BackgroundService for CronBackgroundService {
    fn name(&self) -> &str {
        "config_cron"
    }

    async fn run(&self, ctx: BackgroundContext) -> Result<()> {
        self.inner.run(ctx.inbound_tx).await
    }
}

