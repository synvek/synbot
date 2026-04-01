//! How tool sandbox execution is delegated: in-process [`SandboxManager`] or (Windows) remote helper.

use std::sync::Arc;

use super::manager::SandboxManager;
use super::types::ToolSandboxExecKind;

#[cfg(windows)]
use super::tool_sandbox_ipc::ToolSandboxIpcClient;

/// Where `exec` runs when a tool sandbox is active.
#[derive(Clone)]
pub enum ToolSandboxDelegate {
    /// Default: sandbox lives in this process.
    Local {
        manager: Arc<SandboxManager>,
        sandbox_id: String,
        kind: ToolSandboxExecKind,
    },
    /// Windows: tool AppContainer runs in `synbot tool-sandbox serve` on the host; daemon uses IPC.
    #[cfg(windows)]
    Remote {
        client: Arc<ToolSandboxIpcClient>,
        sandbox_id: String,
        kind: ToolSandboxExecKind,
    },
}

impl ToolSandboxDelegate {
    pub fn exec_kind(&self) -> ToolSandboxExecKind {
        match self {
            ToolSandboxDelegate::Local { kind, .. } => *kind,
            #[cfg(windows)]
            ToolSandboxDelegate::Remote { kind, .. } => *kind,
        }
    }

    pub fn sandbox_id(&self) -> &str {
        match self {
            ToolSandboxDelegate::Local { sandbox_id, .. } => sandbox_id.as_str(),
            #[cfg(windows)]
            ToolSandboxDelegate::Remote { sandbox_id, .. } => sandbox_id.as_str(),
        }
    }
}

/// Optional tool sandbox context for CLI / tools. Replaces the old `(Manager, Option<id>, kind)` tuple.
pub type SandboxContext = Option<ToolSandboxDelegate>;
