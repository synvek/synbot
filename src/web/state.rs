use crate::agent::session_manager::SessionManager;
use crate::agent::role_registry::RoleRegistry;
use crate::agent::skills::SkillsLoader;
use crate::bus::{InboundMessage, OutboundMessage};
use crate::config::Config;
use crate::cron::service::CronService;
use crate::web::log_buffer::SharedLogBuffer;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

/// Shared application state for the web server
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub session_manager: Arc<RwLock<SessionManager>>,
    pub cron_service: Arc<RwLock<CronService>>,
    pub role_registry: Arc<RoleRegistry>,
    pub skills_loader: Arc<SkillsLoader>,
    pub inbound_tx: mpsc::Sender<InboundMessage>,
    pub outbound_tx: broadcast::Sender<OutboundMessage>,
    pub log_buffer: SharedLogBuffer,
    pub approval_manager: Arc<crate::tools::approval::ApprovalManager>,
    pub permission_policy: Option<Arc<crate::tools::permission::CommandPermissionPolicy>>,
}

impl AppState {
    pub fn new(
        config: Arc<Config>,
        session_manager: Arc<RwLock<SessionManager>>,
        cron_service: Arc<RwLock<CronService>>,
        role_registry: Arc<RoleRegistry>,
        skills_loader: Arc<SkillsLoader>,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_tx: broadcast::Sender<OutboundMessage>,
        log_buffer: SharedLogBuffer,
        approval_manager: Arc<crate::tools::approval::ApprovalManager>,
        permission_policy: Option<Arc<crate::tools::permission::CommandPermissionPolicy>>,
    ) -> Self {
        Self {
            config,
            session_manager,
            cron_service,
            role_registry,
            skills_loader,
            inbound_tx,
            outbound_tx,
            log_buffer,
            approval_manager,
            permission_policy,
        }
    }
}
