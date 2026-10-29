use crate::agent::agent_registry::AgentRegistry;
use crate::agent::events::EventBus;
use crate::agent::session_manager::SessionManager;
use crate::agent::skills::SkillProvider;
use crate::bus::{InboundMessage, OutboundMessage};
use crate::channels::ChannelStatusRegistry;
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
    /// Streaming agent events (deltas, tool lifecycle) for `/ws/agent`.
    pub events: Arc<EventBus>,
    pub log_buffer: SharedLogBuffer,
    pub approval_manager: Arc<crate::tools::approval::ApprovalManager>,
    pub permission_policy: Option<Arc<crate::tools::permission::CommandPermissionPolicy>>,
    pub runtime_status: Arc<ChannelStatusRegistry>,
    /// Opened folders + which chat works inside which folder (shared with the agent loop).
    pub projects: Arc<RwLock<crate::project::ProjectStore>>,
    /// Turn snapshots (write before/after images) shared with the agent loop.
    pub checkpoints: Arc<crate::checkpoint::CheckpointStore>,
    /// Persistent PTY sessions for the workbench terminal (`/ws/terminal`).
    pub terminals: Arc<crate::terminal::TerminalHub>,
    /// Subagents (spawn / run_team / directive routing) shown in the workbench team panel.
    /// Startup replaces this with the pool the tools actually spawn into.
    pub subagents: Arc<tokio::sync::Mutex<crate::agent::subagent::SubagentManager>>,
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
        events: Arc<EventBus>,
        log_buffer: SharedLogBuffer,
        approval_manager: Arc<crate::tools::approval::ApprovalManager>,
        permission_policy: Option<Arc<crate::tools::permission::CommandPermissionPolicy>>,
        runtime_status: Arc<ChannelStatusRegistry>,
        projects: Arc<RwLock<crate::project::ProjectStore>>,
        checkpoints: Arc<crate::checkpoint::CheckpointStore>,
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
            events,
            log_buffer,
            approval_manager,
            permission_policy,
            runtime_status,
            projects,
            checkpoints,
            terminals: Arc::new(crate::terminal::TerminalHub::new()),
            subagents: Arc::new(tokio::sync::Mutex::new(
                crate::agent::subagent::SubagentManager::new(4, None),
            )),
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
            EventBus::new(),
            Arc::new(RwLock::new(crate::web::log_buffer::LogBuffer::new(100))),
            Arc::new(crate::tools::approval::ApprovalManager::new()),
            None,
            Arc::new(ChannelStatusRegistry::new()),
            Arc::new(RwLock::new(crate::project::ProjectStore::load(
                PathBuf::from("/tmp/synbot_app_state_test_projects.json"),
            ))),
            Arc::new(crate::checkpoint::CheckpointStore::new(PathBuf::from(
                "/tmp/synbot_app_state_test_checkpoints",
            ))),
        );
        let cloned = state.clone();
        assert!(Arc::ptr_eq(&state.config, &cloned.config));
    }
}
