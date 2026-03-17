//! Common test utilities and helpers for integration tests.
//!
//! Provides:
//! - **Proptest**: `proptest_config()`, `non_empty_string()`, `positive_u32()`, `f64_in_range()`
//! - **AppState**: `create_test_app_state_with_approval()` for API and approval tests
//! - **Temp dirs**: `temp_workflow_root()`, `temp_workspace()` for workflow/session/store tests
//! - **Config**: `default_test_config()` for a valid minimal `Config`
//!
//! Example (approval API test):
//! ```ignore
//! let (inbound_tx, _) = tokio::sync::mpsc::channel(100);
//! let (outbound_tx, _) = tokio::sync::broadcast::channel(100);
//! let approval_manager = Arc::new(ApprovalManager::new());
//! let state = common::create_test_app_state_with_approval(inbound_tx, outbound_tx, approval_manager).await;
//! ```

use proptest::prelude::*;
use std::path::PathBuf;

/// Standard proptest configuration with minimum 100 iterations
/// as specified in the design document.
pub fn proptest_config() -> ProptestConfig {
    ProptestConfig {
        cases: 100,
        ..ProptestConfig::default()
    }
}

/// Generate a non-empty arbitrary string (useful for names, keys, etc.)
pub fn non_empty_string() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_-]{1,64}".prop_map(|s| s)
}

/// Generate an arbitrary positive u32 (> 0)
pub fn positive_u32() -> impl Strategy<Value = u32> {
    1..=u32::MAX
}

/// Generate an arbitrary f64 in a given range
pub fn f64_in_range(min: f64, max: f64) -> impl Strategy<Value = f64> {
    (0..=1000u32).prop_map(move |v| min + (max - min) * (v as f64 / 1000.0))
}

/// Returns a valid default `Config` for tests (all validation passes).
pub fn default_test_config() -> synbot::config::Config {
    synbot::config::Config::default()
}

/// Create a temporary directory for workflow state (e.g. `WorkflowStore::new(path)`).
/// The returned `TempDir` must be held for the duration of the test so the directory is not removed.
pub fn temp_workflow_root() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp workflow root");
    let path = dir.path().to_path_buf();
    (dir, path)
}

/// Create a temporary directory for workspace/session storage.
/// The returned `TempDir` must be held for the duration of the test.
pub fn temp_workspace() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp workspace");
    let path = dir.path().to_path_buf();
    (dir, path)
}

/// Create test AppState with approval manager for testing.
/// Use for API tests (approval history, pending, respond) and any test that needs a full web AppState.
pub async fn create_test_app_state_with_approval(
    inbound_tx: tokio::sync::mpsc::Sender<synbot::bus::InboundMessage>,
    outbound_tx: tokio::sync::broadcast::Sender<synbot::bus::OutboundMessage>,
    approval_manager: std::sync::Arc<synbot::tools::approval::ApprovalManager>,
) -> synbot::web::state::AppState {
    use synbot::agent::agent_registry::AgentRegistry;
    use synbot::agent::session_manager::SessionManager;
    use synbot::agent::skills::SkillsLoader;
    use synbot::config::Config;
    use synbot::cron::service::CronService;
    use synbot::web::log_buffer::LogBuffer;
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use std::path::PathBuf;

    let config = Arc::new(Config::default());
    let session_manager = Arc::new(RwLock::new(SessionManager::new()));
    let cron_service = Arc::new(RwLock::new(CronService::new(PathBuf::from("test_cron.json"))));
    let agent_registry = Arc::new(AgentRegistry::new());
    let skills_loader = Arc::new(SkillsLoader::new(&PathBuf::from(".")));
    let log_buffer = Arc::new(RwLock::new(LogBuffer::new(1000)));

    synbot::web::state::AppState::new(
        config,
        session_manager,
        cron_service,
        agent_registry,
        skills_loader,
        inbound_tx,
        outbound_tx,
        log_buffer,
        approval_manager,
        None,
    )
}
