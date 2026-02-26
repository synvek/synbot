//! Extism-based external plugins: load Wasm and register tools, hooks, skills, background, provider.

mod abi;
mod host_fns;

pub mod adapters;
pub mod loader;

pub use loader::load_extism_plugins;
