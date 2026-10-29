// Sandbox Security Solution Module
//
// This module provides a multi-layer sandbox security solution for safe execution
// of applications and tools in isolated environments.

pub mod config;
pub mod error;
pub mod gvisor_docker;
pub mod manager;
pub mod monitoring;
pub mod sandbox_trait;
pub mod types;
//#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod fallback;
pub mod isolation;
pub mod performance;
pub mod plain_docker;
pub mod platform;
pub mod recovery;
pub mod security;

// Platform-specific implementations
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod nono;

pub mod tool_sandbox_delegate;
#[cfg(target_os = "windows")]
pub mod tool_sandbox_ipc;

#[cfg(target_os = "windows")]
pub mod windows_appcontainer;

#[cfg(target_os = "macos")]
pub mod macos_sandbox_exec;

#[cfg(target_os = "windows")]
pub mod wsl2;

// Re-export commonly used types
pub use config::*;
pub use error::{ConfigError, ErrorReport, ErrorSeverity, Result, SandboxError};
pub use fallback::{FallbackConfig, FallbackEvent, FallbackManager, FallbackResult};
pub use gvisor_docker::GVisorDockerSandbox;
pub use isolation::{
    ChannelConfig, CrossSandboxChannel, IsolationCheck, IsolationVerification, IsolationVerifier,
    IsolationVerifierConfig, PayloadFilter,
};
pub use manager::SandboxManager;
pub use monitoring::*;
pub use performance::{
    parallel_init, BatchExecutor, ConfigCache, DockerConnectionPool, LazyInit, PoolStats,
    ResourcePool, StringInterner,
};
pub use platform::{PlatformDetector, PlatformInfo, SandboxFactory};
pub use recovery::{
    recover_sandbox, recover_sandbox_with_retries, RecoveryConfig, RecoveryManager, RecoveryResult,
};
pub use sandbox_trait::Sandbox;
pub use security::{
    EscapePrevention, PrivilegeEscalationPrevention, ResourceExhaustionPrevention, ResourceLimits,
    SecurityValidator,
};
pub use tool_sandbox_delegate::{SandboxContext, ToolSandboxDelegate};
pub use types::*;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use nono::NonoSandbox;

#[cfg(target_os = "windows")]
pub use windows_appcontainer::WindowsAppContainerSandbox;

#[cfg(target_os = "windows")]
pub use wsl2::{Wsl2GVisorSandbox, Wsl2Integration};
