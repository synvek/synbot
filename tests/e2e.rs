//! E2E tests: config reload, timeouts, concurrency, etc.
//! Run with: `cargo test --test e2e`

mod common;

#[path = "e2e/config_reload.rs"]
mod config_reload;

#[path = "e2e/agent_loop.rs"]
mod agent_loop;
