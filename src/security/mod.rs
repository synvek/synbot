//! Security layer modules for Synbot.
//!
//! - [`audit`]: bounded in-memory trail of enforcement decisions (blocked commands, rejected
//!   approvals, plugin permission denials, path escapes) surfaced by the Security page.
//! - [`secret_masker`]: Log secret masking via `tracing_subscriber::Layer`

pub mod audit;
pub mod secret_masker;

pub use secret_masker::SecretMaskerLayer;
