//! JSON-over-length-prefixed frames on a Windows named pipe for remote tool sandbox exec.
//! Used when the daemon runs inside the app AppContainer and the tool AppContainer runs in
//! `synbot tool-sandbox serve` on the host.

/// Environment: named pipe path (e.g. `\\.\pipe\synbot-tool-...`) for the daemon to connect to the helper.
pub const ENV_TOOL_SANDBOX_PIPE: &str = "SYNBOT_TOOL_SANDBOX_PIPE";
/// Environment: shared secret for IPC requests (launcher-generated).
pub const ENV_TOOL_SANDBOX_AUTH: &str = "SYNBOT_TOOL_SANDBOX_AUTH";
/// Environment: set to `1` in the helper process to avoid recursive sandbox setup.
pub const ENV_TOOL_SANDBOX_HELPER: &str = "SYNBOT_TOOL_SANDBOX_HELPER";

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};

use super::error::{Result, SandboxError};
use super::manager::SandboxManager;
use super::types::ExecutionResult;

const IPC_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolSandboxIpcRequest {
    pub version: u32,
    pub auth: String,
    pub sandbox_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub timeout_ms: u64,
    pub working_dir: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolSandboxIpcResponse {
    pub ok: bool,
    #[serde(default)]
    pub exit_code: i32,
    #[serde(default)]
    pub stdout_b64: String,
    #[serde(default)]
    pub stderr_b64: String,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub error: Option<String>,
}

pub(crate) async fn write_frame<W: AsyncWriteExt + Unpin>(
    w: &mut W,
    payload: &[u8],
) -> std::io::Result<()> {
    let len = u32::try_from(payload.len()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "IPC payload too large")
    })?;
    w.write_u32_le(IPC_VERSION).await?;
    w.write_u32_le(len).await?;
    w.write_all(payload).await?;
    w.flush().await?;
    Ok(())
}

pub(crate) async fn read_frame<R: AsyncReadExt + Unpin>(
    r: &mut R,
) -> std::io::Result<(u32, Vec<u8>)> {
    let ver = r.read_u32_le().await?;
    let len = r.read_u32_le().await? as usize;
    if len > 64 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "IPC frame too large",
        ));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await?;
    Ok((ver, buf))
}

/// Client used by the daemon (inside app sandbox) to send exec requests to the helper.
pub struct ToolSandboxIpcClient {
    pipe_path: String,
    auth: String,
}

impl ToolSandboxIpcClient {
    pub fn new(pipe_path: String, auth: String) -> Self {
        Self { pipe_path, auth }
    }

    pub async fn execute(
        &self,
        sandbox_id: &str,
        command: &str,
        args: &[String],
        timeout: Duration,
        working_dir: Option<&str>,
    ) -> Result<ExecutionResult> {
        let req = ToolSandboxIpcRequest {
            version: IPC_VERSION,
            auth: self.auth.clone(),
            sandbox_id: sandbox_id.to_string(),
            command: command.to_string(),
            args: args.to_vec(),
            timeout_ms: timeout.as_millis() as u64,
            working_dir: working_dir.map(|s| s.to_string()),
        };
        let json = serde_json::to_vec(&req).map_err(|e| {
            SandboxError::ExecutionFailed(format!("IPC request serialize: {}", e))
        })?;

        let mut client = ClientOptions::new()
            .open(&self.pipe_path)
            .map_err(|e| SandboxError::ExecutionFailed(format!("IPC connect to pipe: {}", e)))?;

        write_frame(&mut client, &json)
            .await
            .map_err(|e| SandboxError::ExecutionFailed(format!("IPC write: {}", e)))?;

        let (_ver, body) = read_frame(&mut client)
            .await
            .map_err(|e| SandboxError::ExecutionFailed(format!("IPC read: {}", e)))?;

        let resp: ToolSandboxIpcResponse = serde_json::from_slice(&body).map_err(|e| {
            SandboxError::ExecutionFailed(format!("IPC response parse: {}", e))
        })?;

        if !resp.ok {
            return Err(SandboxError::ExecutionFailed(
                resp.error.unwrap_or_else(|| "unknown IPC error".to_string()),
            ));
        }

        let stdout = B64
            .decode(resp.stdout_b64.as_bytes())
            .map_err(|e| SandboxError::ExecutionFailed(format!("IPC stdout base64: {}", e)))?;
        let stderr = B64
            .decode(resp.stderr_b64.as_bytes())
            .map_err(|e| SandboxError::ExecutionFailed(format!("IPC stderr base64: {}", e)))?;

        Ok(ExecutionResult {
            exit_code: resp.exit_code,
            stdout,
            stderr,
            duration: Duration::from_millis(resp.duration_ms),
            error: None,
        })
    }
}

/// After [`NamedPipeServer::connect`], read one request, execute in sandbox, write response.
pub async fn dispatch_connected_request(
    pipe: &mut NamedPipeServer,
    auth_expected: &str,
    expected_sandbox_id: &str,
    manager: &SandboxManager,
) -> std::io::Result<()> {
    let (_ver, body) = read_frame(pipe).await?;
    let req: ToolSandboxIpcRequest = serde_json::from_slice(&body)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    let resp = if req.version != IPC_VERSION {
        ToolSandboxIpcResponse {
            ok: false,
            exit_code: -1,
            stdout_b64: String::new(),
            stderr_b64: String::new(),
            duration_ms: 0,
            error: Some(format!("unsupported IPC version {}", req.version)),
        }
    } else if req.auth != auth_expected {
        ToolSandboxIpcResponse {
            ok: false,
            exit_code: -1,
            stdout_b64: String::new(),
            stderr_b64: String::new(),
            duration_ms: 0,
            error: Some("unauthorized".to_string()),
        }
    } else if req.sandbox_id != expected_sandbox_id {
        ToolSandboxIpcResponse {
            ok: false,
            exit_code: -1,
            stdout_b64: String::new(),
            stderr_b64: String::new(),
            duration_ms: 0,
            error: Some(format!(
                "sandbox_id mismatch (expected {})",
                expected_sandbox_id
            )),
        }
    } else {
        let timeout = Duration::from_millis(req.timeout_ms.max(1));
        match manager
            .execute_in_sandbox(
                &req.sandbox_id,
                &req.command,
                &req.args,
                timeout,
                req.working_dir.as_deref(),
            )
            .await
        {
            Ok(er) => ToolSandboxIpcResponse {
                ok: true,
                exit_code: er.exit_code,
                stdout_b64: B64.encode(&er.stdout),
                stderr_b64: B64.encode(&er.stderr),
                duration_ms: er.duration.as_millis() as u64,
                error: None,
            },
            Err(e) => ToolSandboxIpcResponse {
                ok: false,
                exit_code: -1,
                stdout_b64: String::new(),
                stderr_b64: String::new(),
                duration_ms: 0,
                error: Some(e.to_string()),
            },
        }
    };

    let json = serde_json::to_vec(&resp)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    write_frame(pipe, &json).await?;
    Ok(())
}

/// SDDL for the pipe: allow same-user clients including AppContainer (`CreateFile` from lowbox
/// often fails with ERROR_ACCESS_DENIED on the default named-pipe DACL).
/// Pipe name is unguessable (UUID) and requests still require [`ToolSandboxIpcRequest::auth`].
const TOOL_SANDBOX_PIPE_SDDL: &str = concat!(
    "D:",
    "(A;;GA;;;WD)",           // Everyone — broad connect (name + auth bound exposure)
    "(A;;GA;;;AC)",          // ALL APPLICATION PACKAGES (SDDL_ALL_APP_PACKAGES)
    "(A;;GA;;;S-1-15-2-2)",  // ALL RESTRICTED APPLICATION PACKAGES
);

/// Create a named pipe instance for tool-sandbox IPC.
///
/// `first_instance=true` sets `FILE_FLAG_FIRST_PIPE_INSTANCE` to ensure we don't accidentally
/// connect to a stale/other server when starting up.
pub fn create_tool_sandbox_pipe(pipe_path: &str, first_instance: bool) -> std::io::Result<NamedPipeServer> {
    // Do **not** call `max_instances(255)`: tokio's builder panics for `>= 255` (`usize`).
    // Default is `PIPE_UNLIMITED_INSTANCES` (255) passed straight to `CreateNamedPipeW`, which is valid.
    unsafe {
        let sddl: Vec<u16> = TOOL_SANDBOX_PIPE_SDDL
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut sd = PSECURITY_DESCRIPTOR::default();
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut sd,
            None,
        )
        .map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("ConvertStringSecurityDescriptorToSecurityDescriptorW: {}", e),
            )
        })?;

        let mut sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0,
            bInheritHandle: false.into(),
        };

        let mut opts = ServerOptions::new();
        if first_instance {
            opts.first_pipe_instance(true);
        }
        let out = opts.create_with_security_attributes_raw(
            pipe_path,
            &mut sa as *mut SECURITY_ATTRIBUTES as *mut std::ffi::c_void,
        );

        if !sd.0.is_null() {
            let _ = LocalFree(HLOCAL(sd.0));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::ToolSandboxIpcResponse;

    #[test]
    fn ipc_response_json_roundtrip() {
        let r = ToolSandboxIpcResponse {
            ok: true,
            exit_code: 0,
            stdout_b64: "aGk=".into(),
            stderr_b64: String::new(),
            duration_ms: 42,
            error: None,
        };
        let v = serde_json::to_vec(&r).unwrap();
        let r2: ToolSandboxIpcResponse = serde_json::from_slice(&v).unwrap();
        assert!(r2.ok && r2.duration_ms == 42);
    }
}
