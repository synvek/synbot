use crate::agent::agent_registry::AgentRegistry;
use crate::agent::session_manager::SessionManager;
use crate::agent::skills::SkillProvider;
use crate::bus::{InboundMessage, OutboundMessage};
use crate::config::Config;
use crate::cron::service::CronService;
use crate::web::log_buffer::SharedLogBuffer;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};

/// Shared application state for the web server
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<RwLock<Config>>,
    /// Path used for `save_config` from the web UI (same file as the running daemon).
    pub config_path: PathBuf,
    pub session_manager: Arc<RwLock<SessionManager>>,
    pub cron_service: Arc<RwLock<CronService>>,
    pub agent_registry: Arc<AgentRegistry>,
    pub skills_loader: Arc<dyn SkillProvider>,
    pub inbound_tx: mpsc::Sender<InboundMessage>,
    pub outbound_tx: broadcast::Sender<OutboundMessage>,
    pub log_buffer: SharedLogBuffer,
    pub approval_manager: Arc<crate::tools::approval::ApprovalManager>,
    pub permission_policy: Option<Arc<crate::tools::permission::CommandPermissionPolicy>>,
}

impl AppState {
    pub fn new(
        config: Arc<RwLock<Config>>,
        config_path: PathBuf,
        session_manager: Arc<RwLock<SessionManager>>,
        cron_service: Arc<RwLock<CronService>>,
        agent_registry: Arc<AgentRegistry>,
        skills_loader: Arc<dyn SkillProvider>,
        inbound_tx: mpsc::Sender<InboundMessage>,
        outbound_tx: broadcast::Sender<OutboundMessage>,
        log_buffer: SharedLogBuffer,
        approval_manager: Arc<crate::tools::approval::ApprovalManager>,
        permission_policy: Option<Arc<crate::tools::permission::CommandPermissionPolicy>>,
    ) -> Self {
        Self {
            config,
            config_path,
            session_manager,
            cron_service,
            agent_registry,
            skills_loader,
            inbound_tx,
            outbound_tx,
            log_buffer,
            approval_manager,
            permission_policy,
        }
    }

    /// Read `show_toolCalls` flags synchronously (e.g. actix WebSocket actor).
    pub fn show_tool_progress_for_ws(&self) -> bool {
        match tokio::runtime::Handle::try_current() {
            Ok(h) => h.block_on(async {
                let c = self.config.read().await;
                c.show_tool_calls && c.web.show_tool_calls
            }),
            Err(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::skills::SkillsLoader;
    use std::path::PathBuf;

    #[test]
    fn app_state_new_and_clone() {
        let (inbound_tx, _) = mpsc::channel(10);
        let (outbound_tx, _) = broadcast::channel(10);
        let state = AppState::new(
            Arc::new(RwLock::new(Config::default())),
            PathBuf::from("/tmp/synbot_app_state_test_config.json"),
            Arc::new(RwLock::new(SessionManager::new())),
            Arc::new(RwLock::new(crate::cron::service::CronService::new(
                PathBuf::from("test_cron.json"),
            ))),
            Arc::new(AgentRegistry::new()),
            Arc::new(SkillsLoader::new(&PathBuf::from("."))),
            inbound_tx,
            outbound_tx,
            Arc::new(RwLock::new(crate::web::log_buffer::LogBuffer::new(100))),
            Arc::new(crate::tools::approval::ApprovalManager::new()),
            None,
        );
        let cloned = state.clone();
        assert!(Arc::ptr_eq(&state.config, &cloned.config));
    }
}
