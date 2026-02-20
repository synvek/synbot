// Sandbox Security Solution Module
//
// This module provides a multi-layer sandbox security solution for safe execution
// of applications and tools in isolated environments.

pub mod types;
pub mod error;
pub mod config;
pub mod monitoring;
pub mod sandbox_trait;
pub mod manager;
pub mod gvisor_docker;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod plain_docker;
pub mod isolation;
pub mod recovery;
pub mod fallback;
pub mod platform;
pub mod security;
pub mod performance;

// Platform-specific implementations
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod nono;

#[cfg(target_os = "windows")]
pub mod windows_appcontainer;

#[cfg(target_os = "windows")]
pub mod wsl2;

// Re-export commonly used types
pub use error::{SandboxError, ConfigError, Result, ErrorReport, ErrorSeverity};
pub use types::*;
pub use config::*;
pub use monitoring::*;
pub use sandbox_trait::Sandbox;
pub use manager::SandboxManager;
pub use gvisor_docker::GVisorDockerSandbox;
pub use isolation::{
    IsolationVerifier, IsolationVerification, IsolationCheck,
    CrossSandboxChannel, PayloadFilter, ChannelConfig, IsolationVerifierConfig,
};
pub use recovery::{RecoveryManager, RecoveryConfig, RecoveryResult, recover_sandbox, recover_sandbox_with_retries};
pub use fallback::{FallbackManager, FallbackConfig, FallbackResult, FallbackEvent};
pub use platform::{PlatformDetector, PlatformInfo, SandboxFactory};
pub use security::{
    SecurityValidator, EscapePrevention, PrivilegeEscalationPrevention,
    ResourceExhaustionPrevention, ResourceLimits,
};
pub use performance::{
    DockerConnectionPool, LazyInit, ConfigCache, BatchExecutor,
    ResourcePool, PoolStats, StringInterner, parallel_init,
};

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use nono::NonoSandbox;

#[cfg(target_os = "windows")]
pub use windows_appcontainer::WindowsAppContainerSandbox;

#[cfg(target_os = "windows")]
pub use wsl2::{Wsl2Integration, Wsl2GVisorSandbox};
