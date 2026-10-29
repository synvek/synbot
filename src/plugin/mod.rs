//! Extism-based external plugins: load Wasm and register tools, hooks, skills, background, provider.
//!
//! Packages carry a `plugin.json` manifest that declares version, permissions, digest and optional
//! signature; see [`manifest`]. Installing/uninstalling packages and the trusted publisher keys live
//! in [`install`].

mod abi;
mod host_fns;

pub mod adapters;
pub mod install;
pub mod loader;
pub mod manifest;

pub use loader::load_extism_plugins;
