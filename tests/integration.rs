//! Integration tests: multi-module collaboration, no real network.
//!
//! Run with: `cargo test --test integration`

#[path = "integration/sandbox.rs"]
mod sandbox;

#[path = "integration/channel_factory.rs"]
mod channel_factory;
