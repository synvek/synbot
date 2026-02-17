/// Common test utilities and helpers for integration tests.
///
/// This module provides shared test infrastructure including:
/// - proptest configuration presets
/// - common test data generators
/// - helper functions for test setup/teardown

use proptest::prelude::*;

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

/// Create test AppState with approval manager for testing
pub async fn create_test_app_state_with_approval(
    inbound_tx: tokio::sync::mpsc::Sender<synbot::bus::InboundMessage>,
    outbound_tx: tokio::sync::broadcast::Sender<synbot::bus::OutboundMessage>,
    approval_manager: std::sync::Arc<synbot::tools::approval::ApprovalManager>,
) -> synbot::web::state::AppState {
    use synbot::agent::role_registry::RoleRegistry;
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
    let role_registry = Arc::new(RoleRegistry::new());
    let skills_loader = Arc::new(SkillsLoader::new(&PathBuf::from(".")));
    let log_buffer = Arc::new(RwLock::new(LogBuffer::new(1000)));
    
    synbot::web::state::AppState::new(
        config,
        session_manager,
        cron_service,
        role_registry,
        skills_loader,
        inbound_tx,
        outbound_tx,
        log_buffer,
        approval_manager,
        None,
    )
}
