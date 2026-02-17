//! Shell execution tool.

use anyhow::Result;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tracing::{info, warn};

use crate::tools::truncation::smart_truncate_streams;
use crate::tools::DynTool;

const MAX_OUTPUT: usize = 10_000;

/// Normalize tool-provided command strings before execution.
///
/// Tool call JSON sometimes contains literal backslash-quote (e.g. `print(\"x\")`)
/// which then gets written into files by echo. Strip every backslash that precedes
/// a double-quote so the command runs with real quotes. Applied on all platforms.
fn normalize_command_input(cmd: &str) -> String {
    let mut s = cmd.to_string();
    while s.contains("\\\"") {
        s = s.replace("\\\"", "\"");
    }
    s
}

/// Decode process output bytes to a UTF-8 String. On Windows, cmd.exe often outputs
/// in the system OEM code page (e.g. GBK/CP936 on Chinese systems); decoding as UTF-8
/// produces mojibake. We try UTF-8 first, then GBK when on Windows.
fn decode_output_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    #[cfg(windows)]
    {
        if let Ok(s) = std::str::from_utf8(bytes) {
            if !s.contains('\u{FFFD}') {
                return s.to_string();
            }
        }
        let (decoded, _, had_errors) = encoding_rs::GBK.decode(bytes);
        let s = decoded.into_owned();
        if had_errors {
            String::from_utf8_lossy(bytes).into_owned()
        } else {
            s
        }
    }
    #[cfg(not(windows))]
    {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

// ---------------------------------------------------------------------------
// CommandPolicy – configurable deny/allow pattern matching
// ---------------------------------------------------------------------------

/// Command security policy that validates commands against configurable
/// deny and allow pattern lists before execution.
#[derive(Debug, Clone)]
pub struct CommandPolicy {
    pub deny_patterns: Vec<String>,
    pub allow_patterns: Option<Vec<String>>,
}

impl CommandPolicy {
    /// Create a new `CommandPolicy` from the given deny and allow patterns.
    pub fn new(deny_patterns: Vec<String>, allow_patterns: Option<Vec<String>>) -> Self {
        Self {
            deny_patterns,
            allow_patterns,
        }
    }

    /// Validate whether a command is allowed to execute.
    ///
    /// Checks deny patterns first – if any deny pattern matches (case-insensitive
    /// substring), the command is rejected.  Then, if an allow list is configured,
    /// the command must match at least one allow pattern to be accepted.
    pub fn validate(&self, command: &str) -> std::result::Result<(), String> {
        let lower = command.to_lowercase();

        // 1. Check deny patterns – reject if any match
        for pat in &self.deny_patterns {
            if lower.contains(&pat.to_lowercase()) {
                return Err(format!(
                    "Command rejected: matches deny pattern '{}'. Command: {}",
                    pat, command
                ));
            }
        }

        // 2. Check allow patterns – if set, command must match at least one
        if let Some(ref allow) = self.allow_patterns {
            let allowed = allow
                .iter()
                .any(|pat| lower.contains(&pat.to_lowercase()));
            if !allowed {
                return Err(format!(
                    "Command rejected: does not match any allow pattern. Command: {}",
                    command
                ));
            }
        }

        Ok(())
    }
}

impl Default for CommandPolicy {
    fn default() -> Self {
        Self {
            deny_patterns: vec![
                "rm -rf /".to_string(),
                "mkfs".to_string(),
                "dd if=".to_string(),
                "format".to_string(),
                "shutdown".to_string(),
                "reboot".to_string(),
                ":(){".to_string(),
                "fork bomb".to_string(),
            ],
            allow_patterns: None,
        }
    }
}

// ---------------------------------------------------------------------------
// ExecResult – enhanced execution result with full context
// ---------------------------------------------------------------------------

/// Enhanced execution result containing full execution context.
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub working_dir: String,
    pub truncated: bool,
    pub original_size: Option<usize>,
}

impl ExecResult {
    /// Format the result as a human-readable string for the LLM.
    pub fn to_display_string(&self) -> String {
        let mut result = format!(
            "exit code: {}\nworking_dir: {}\nduration: {}ms\n",
            self.exit_code, self.working_dir, self.duration_ms
        );
        if !self.stdout.is_empty() {
            result.push_str(&self.stdout);
        }
        if !self.stderr.is_empty() {
            result.push_str("\n[stderr]\n");
            result.push_str(&self.stderr);
        }
        if self.truncated {
            if let Some(orig) = self.original_size {
                result.push_str(&format!(
                    "\n...[truncated, original size: {} bytes]",
                    orig
                ));
            } else {
                result.push_str("\n...[truncated]");
            }
        }
        result
    }
}

// ---------------------------------------------------------------------------
// Workspace path validation
// ---------------------------------------------------------------------------

/// Verify that `target` resolves to a path within `workspace`.
/// Returns `Ok(resolved)` on success, or an error message on failure.
fn validate_workspace_path(
    workspace: &Path,
    target: &Path,
) -> std::result::Result<PathBuf, String> {
    // Resolve both paths to canonicalize symlinks / ".." etc.
    // If the target doesn't exist yet we fall back to lexical resolution.
    let ws_canon = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());

    let target_canon = target
        .canonicalize()
        .unwrap_or_else(|_| {
            // For non-existent paths, do a best-effort absolute resolution
            if target.is_absolute() {
                target.to_path_buf()
            } else {
                ws_canon.join(target)
            }
        });

    if target_canon.starts_with(&ws_canon) {
        Ok(target_canon)
    } else {
        Err(format!(
            "Working directory '{}' is outside the workspace '{}'",
            target.display(),
            workspace.display()
        ))
    }
}

// ---------------------------------------------------------------------------
// ExecTool
// ---------------------------------------------------------------------------

pub struct ExecTool {
    pub workspace: PathBuf,
    pub timeout_secs: u64,
    /// 审批超时秒数（仅当需要审批时使用）
    pub approval_timeout_secs: u64,
    pub restrict_to_workspace: bool,
    pub policy: CommandPolicy,
    pub permission_policy: Option<Arc<crate::tools::permission::CommandPermissionPolicy>>,
    pub approval_manager: Option<Arc<crate::tools::approval::ApprovalManager>>,
    pub session_id: Option<String>,
    pub channel: Option<String>,
    pub chat_id: Option<String>,
}

#[async_trait::async_trait]
impl DynTool for ExecTool {
    fn name(&self) -> &str {
        "exec"
    }
    fn description(&self) -> &str {
        "Execute a shell command and return output. Dangerous commands are blocked. Prefer read_file/write_file/edit_file/list_dir for filesystem operations."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to run" },
                "working_dir": { "type": "string", "description": "Optional working directory" },
                "approval_message": { "type": "string", "description": "Optional. When the command requires approval, this message is shown to the user. Use the same language as the user (e.g. if the user writes in Japanese, write the approval request in Japanese). Include: command, working dir, context, and how to approve/reject (e.g. yes/no, approve/reject). If omitted, a default format is used." }
            },
            "required": ["command"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let cmd_str = normalize_command_input(args["command"].as_str().unwrap_or(""));

        // Validate command against policy
        self.policy
            .validate(&cmd_str)
            .map_err(|e| anyhow::anyhow!(e))?;

        // Check permission level if permission policy is enabled
        if let Some(permission_policy) = &self.permission_policy {
            let permission = permission_policy.check_permission(&cmd_str);
            
            match permission {
                crate::tools::permission::PermissionLevel::Deny => {
                    return Err(anyhow::anyhow!(
                        "Command denied by policy: {}",
                        cmd_str
                    ));
                }
                crate::tools::permission::PermissionLevel::RequireApproval => {
                    // Request approval if approval manager is available
                    if let (Some(approval_manager), Some(session_id), Some(channel), Some(chat_id)) = 
                        (&self.approval_manager, &self.session_id, &self.channel, &self.chat_id) 
                    {
                        let cwd = args["working_dir"]
                            .as_str()
                            .unwrap_or_else(|| self.workspace.to_str().unwrap_or("."));
                        
                        let context = format!(
                            "session: {} channel: {}",
                            session_id, channel
                        );
                        
                        let approval_message = args["approval_message"]
                            .as_str()
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty());
                        
                        let approved = approval_manager
                            .request_approval(
                                session_id.clone(),
                                channel.clone(),
                                chat_id.clone(),
                                cmd_str.to_string(),
                                cwd.to_string(),
                                context,
                                self.approval_timeout_secs,
                                approval_message,
                            )
                            .await?;
                        
                        if !approved {
                            return Err(anyhow::anyhow!(
                                "Execution rejected: {} (user did not approve or request timed out)",
                                cmd_str
                            ));
                        }
                    } else {
                        // If approval manager or session info is not available, deny by default
                        return Err(anyhow::anyhow!(
                            "Approval required but approval system not configured: {}",
                            cmd_str
                        ));
                    }
                }
                crate::tools::permission::PermissionLevel::Allow => {
                    // Allow execution, continue
                }
            }
        }

        // Resolve working directory
        let cwd = args["working_dir"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.workspace.clone());

        // Validate working directory if restrict_to_workspace is enabled
        if self.restrict_to_workspace {
            validate_workspace_path(&self.workspace, &cwd)
                .map_err(|e| anyhow::anyhow!(e))?;
        }

        let start = Instant::now();

        let output = tokio::time::timeout(
            Duration::from_secs(self.timeout_secs),
            Command::new(if cfg!(windows) { "cmd" } else { "sh" })
                .args(if cfg!(windows) {
                    vec!["/C", &cmd_str]
                } else {
                    vec!["-c", &cmd_str]
                })
                .current_dir(&cwd)
                .output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Command timed out after {}s", self.timeout_secs))??;

        let duration_ms = start.elapsed().as_millis() as u64;

        let stdout = decode_output_bytes(&output.stdout);
        let stderr = decode_output_bytes(&output.stderr);
        let working_dir = cwd.display().to_string();

        let total_size = stdout.len() + stderr.len();
        let needs_truncation = total_size > MAX_OUTPUT;

        // Apply smart truncation if the combined output exceeds the limit.
        // smart_truncate_streams handles proportional budget allocation and
        // preserves the head + tail of each stream.
        let (truncated_stdout, truncated_stderr, original_size) = if needs_truncation {
            let (out_r, err_r) =
                smart_truncate_streams(&stdout, &stderr, MAX_OUTPUT);
            (out_r.content, err_r.content, Some(total_size))
        } else {
            (stdout, stderr, None)
        };

        let exec_result = ExecResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: truncated_stdout.clone(),
            stderr: truncated_stderr.clone(),
            duration_ms,
            working_dir: working_dir.clone(),
            truncated: needs_truncation,
            original_size,
        };

        // Log command execution result
        if exec_result.exit_code == 0 {
            info!(
                command = %cmd_str,
                exit_code = exec_result.exit_code,
                duration_ms = exec_result.duration_ms,
                working_dir = %working_dir,
                stdout_len = truncated_stdout.len(),
                stderr_len = truncated_stderr.len(),
                truncated = needs_truncation,
                session_id = ?self.session_id,
                channel = ?self.channel,
                "Command executed successfully"
            );
        } else {
            // On Windows, "dir /s /b" returns exit code 1 when no files match (not an error)
            let is_windows_dir_no_match = cfg!(windows)
                && exec_result.exit_code == 1
                && cmd_str.trim().to_uppercase().starts_with("DIR");
            if is_windows_dir_no_match {
                tracing::debug!(
                    command = %cmd_str,
                    exit_code = exec_result.exit_code,
                    working_dir = %working_dir,
                    "dir returned 1 (no matches), not treating as failure"
                );
            } else {
                warn!(
                    command = %cmd_str,
                    exit_code = exec_result.exit_code,
                    duration_ms = exec_result.duration_ms,
                    working_dir = %working_dir,
                    stderr = %mask_sensitive_info(&truncated_stderr),
                    session_id = ?self.session_id,
                    channel = ?self.channel,
                    "Command execution failed"
                );
            }
        }

        Ok(exec_result.to_display_string())
    }
}

/// Mask sensitive information in log output
fn mask_sensitive_info(text: &str) -> String {
    let mut masked = text.to_string();
    
    // Simple pattern matching for common sensitive data
    // Replace common patterns with masked versions
    let keywords = ["api_key", "apikey", "api-key", "token", "password", "secret", "Bearer", "Basic"];
    
    for keyword in &keywords {
        if let Some(pos) = masked.to_lowercase().find(&keyword.to_lowercase()) {
            // Find the end of the value (next space or end of string)
            let start = pos + keyword.len();
            if let Some(rest) = masked.get(start..) {
                if let Some(space_pos) = rest.find(|c: char| c.is_whitespace()) {
                    let end = start + space_pos;
                    if let Some(prefix) = masked.get(..start) {
                        if let Some(suffix) = masked.get(end..) {
                            masked = format!("{}***{}", prefix, suffix);
                        }
                    }
                } else {
                    // No space found, mask to end
                    if let Some(prefix) = masked.get(..start) {
                        masked = format!("{}***", prefix);
                    }
                }
            }
        }
    }
    
    masked
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- CommandPolicy tests ----

    #[test]
    fn policy_default_blocks_dangerous_commands() {
        let policy = CommandPolicy::default();
        assert!(policy.validate("rm -rf /").is_err());
        assert!(policy.validate("sudo mkfs /dev/sda").is_err());
        assert!(policy.validate("dd if=/dev/zero of=/dev/sda").is_err());
        assert!(policy.validate("shutdown -h now").is_err());
        assert!(policy.validate("reboot").is_err());
    }

    #[test]
    fn policy_default_allows_safe_commands() {
        let policy = CommandPolicy::default();
        assert!(policy.validate("ls -la").is_ok());
        assert!(policy.validate("echo hello").is_ok());
        assert!(policy.validate("cargo build").is_ok());
        assert!(policy.validate("git status").is_ok());
    }

    #[test]
    fn policy_deny_is_case_insensitive() {
        let policy = CommandPolicy::new(vec!["dangerous".to_string()], None);
        assert!(policy.validate("DANGEROUS command").is_err());
        assert!(policy.validate("Dangerous Command").is_err());
        assert!(policy.validate("dangerous").is_err());
    }

    #[test]
    fn policy_deny_pattern_match_returns_descriptive_error() {
        let policy = CommandPolicy::new(vec!["rm -rf /".to_string()], None);
        let err = policy.validate("rm -rf /").unwrap_err();
        assert!(err.contains("deny pattern"));
        assert!(err.contains("rm -rf /"));
    }

    #[test]
    fn policy_allow_patterns_restrict_commands() {
        let policy = CommandPolicy::new(
            vec![],
            Some(vec!["cargo".to_string(), "git".to_string()]),
        );
        assert!(policy.validate("cargo build").is_ok());
        assert!(policy.validate("git status").is_ok());
        assert!(policy.validate("rm -rf /tmp").is_err());
        assert!(policy.validate("ls -la").is_err());
    }

    #[test]
    fn policy_allow_patterns_none_allows_all_non_denied() {
        let policy = CommandPolicy::new(vec!["bad".to_string()], None);
        assert!(policy.validate("ls").is_ok());
        assert!(policy.validate("echo hello").is_ok());
        assert!(policy.validate("bad command").is_err());
    }

    #[test]
    fn policy_deny_checked_before_allow() {
        // Even if "cargo" is in allow list, deny takes precedence
        let policy = CommandPolicy::new(
            vec!["cargo test --dangerous".to_string()],
            Some(vec!["cargo".to_string()]),
        );
        assert!(policy.validate("cargo build").is_ok());
        assert!(policy.validate("cargo test --dangerous").is_err());
    }

    #[test]
    fn policy_empty_deny_and_no_allow_allows_everything() {
        let policy = CommandPolicy::new(vec![], None);
        assert!(policy.validate("anything").is_ok());
        assert!(policy.validate("rm -rf /").is_ok());
    }

    #[test]
    fn policy_empty_deny_with_empty_allow_rejects_everything() {
        let policy = CommandPolicy::new(vec![], Some(vec![]));
        assert!(policy.validate("ls").is_err());
        assert!(policy.validate("echo hello").is_err());
    }

    #[test]
    fn policy_allow_not_matched_returns_descriptive_error() {
        let policy = CommandPolicy::new(vec![], Some(vec!["cargo".to_string()]));
        let err = policy.validate("ls -la").unwrap_err();
        assert!(err.contains("does not match any allow pattern"));
    }

    // ---- ExecResult tests ----

    #[test]
    fn exec_result_display_basic() {
        let result = ExecResult {
            exit_code: 0,
            stdout: "hello\n".to_string(),
            stderr: String::new(),
            duration_ms: 42,
            working_dir: "/tmp".to_string(),
            truncated: false,
            original_size: None,
        };
        let display = result.to_display_string();
        assert!(display.contains("exit code: 0"));
        assert!(display.contains("working_dir: /tmp"));
        assert!(display.contains("duration: 42ms"));
        assert!(display.contains("hello"));
        assert!(!display.contains("[truncated"));
    }

    #[test]
    fn exec_result_display_with_stderr() {
        let result = ExecResult {
            exit_code: 1,
            stdout: "out\n".to_string(),
            stderr: "err\n".to_string(),
            duration_ms: 100,
            working_dir: "/home".to_string(),
            truncated: false,
            original_size: None,
        };
        let display = result.to_display_string();
        assert!(display.contains("exit code: 1"));
        assert!(display.contains("out\n"));
        assert!(display.contains("[stderr]"));
        assert!(display.contains("err\n"));
    }

    #[test]
    fn exec_result_display_truncated_with_original_size() {
        let result = ExecResult {
            exit_code: 0,
            stdout: "data".to_string(),
            stderr: String::new(),
            duration_ms: 10,
            working_dir: "/tmp".to_string(),
            truncated: true,
            original_size: Some(50000),
        };
        let display = result.to_display_string();
        assert!(display.contains("[truncated, original size: 50000 bytes]"));
    }

    #[test]
    fn exec_result_display_truncated_without_original_size() {
        let result = ExecResult {
            exit_code: 0,
            stdout: "data".to_string(),
            stderr: String::new(),
            duration_ms: 10,
            working_dir: "/tmp".to_string(),
            truncated: true,
            original_size: None,
        };
        let display = result.to_display_string();
        assert!(display.contains("[truncated]"));
        assert!(!display.contains("original size"));
    }

    // ---- Workspace path validation tests ----

    #[test]
    fn validate_workspace_path_inside() {
        let ws = PathBuf::from("/tmp/workspace");
        let target = PathBuf::from("/tmp/workspace/subdir");
        // This may fail on systems where /tmp/workspace doesn't exist,
        // but the fallback logic handles it
        let result = validate_workspace_path(&ws, &target);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_workspace_path_outside() {
        let ws = PathBuf::from("/tmp/workspace");
        let target = PathBuf::from("/etc/passwd");
        let result = validate_workspace_path(&ws, &target);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside the workspace"));
    }

    #[test]
    fn validate_workspace_path_relative_stays_inside() {
        let ws = PathBuf::from("/tmp/workspace");
        let target = PathBuf::from("subdir");
        let result = validate_workspace_path(&ws, &target);
        assert!(result.is_ok());
    }

    // ---- Permission Integration tests ----

    #[tokio::test]
    async fn permission_deny_blocks_command() {
        use crate::tools::permission::{CommandPermissionPolicy, PermissionLevel, PermissionRule};
        
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "rm*".to_string(),
                    level: PermissionLevel::Deny,
                    description: None,
                },
            ],
            PermissionLevel::Allow,
        );

        let tool = ExecTool {
            workspace: PathBuf::from("."),
            timeout_secs: 10,
            approval_timeout_secs: 300,
            restrict_to_workspace: false,
            policy: CommandPolicy::default(),
            permission_policy: Some(Arc::new(policy)),
            approval_manager: None,
            session_id: None,
            channel: None,
            chat_id: None,
        };

        let args = json!({
            "command": "rm -rf test.txt"
        });

        let result = tool.call(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Command denied by policy"));
    }

    #[tokio::test]
    async fn permission_allow_executes_command() {
        use crate::tools::permission::{CommandPermissionPolicy, PermissionLevel, PermissionRule};
        
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "echo*".to_string(),
                    level: PermissionLevel::Allow,
                    description: None,
                },
            ],
            PermissionLevel::Deny,
        );

        let tool = ExecTool {
            workspace: PathBuf::from("."),
            timeout_secs: 10,
            approval_timeout_secs: 300,
            restrict_to_workspace: false,
            policy: CommandPolicy::default(),
            permission_policy: Some(Arc::new(policy)),
            approval_manager: None,
            session_id: None,
            channel: None,
            chat_id: None,
        };

        let args = json!({
            "command": "echo test"
        });

        let result = tool.call(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn permission_require_approval_without_manager_fails() {
        use crate::tools::permission::{CommandPermissionPolicy, PermissionLevel, PermissionRule};
        
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "git push*".to_string(),
                    level: PermissionLevel::RequireApproval,
                    description: None,
                },
            ],
            PermissionLevel::Allow,
        );

        let tool = ExecTool {
            workspace: PathBuf::from("."),
            timeout_secs: 10,
            approval_timeout_secs: 300,
            restrict_to_workspace: false,
            policy: CommandPolicy::default(),
            permission_policy: Some(Arc::new(policy)),
            approval_manager: None,
            session_id: None,
            channel: None,
            chat_id: None,
        };

        let args = json!({
            "command": "git push origin main"
        });

        let result = tool.call(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("approval system not configured"));
    }

    #[tokio::test]
    async fn permission_require_approval_with_manager_configured() {
        use crate::tools::approval::ApprovalManager;
        use crate::tools::permission::{CommandPermissionPolicy, PermissionLevel, PermissionRule};
        
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "curl*".to_string(),
                    level: PermissionLevel::RequireApproval,
                    description: None,
                },
            ],
            PermissionLevel::Allow,
        );

        let approval_manager = Arc::new(ApprovalManager::new());
        
        let tool = ExecTool {
            workspace: PathBuf::from("."),
            timeout_secs: 10,
            approval_timeout_secs: 300,
            restrict_to_workspace: false,
            policy: CommandPolicy::default(),
            permission_policy: Some(Arc::new(policy)),
            approval_manager: Some(approval_manager.clone()),
            session_id: Some("test-session".to_string()),
            channel: Some("test".to_string()),
            chat_id: Some("test-chat".to_string()),
        };

        // Verify that the tool has approval manager configured
        assert!(tool.approval_manager.is_some());
        assert!(tool.session_id.is_some());
    }

    #[tokio::test]
    async fn permission_no_policy_allows_execution() {
        let tool = ExecTool {
            workspace: PathBuf::from("."),
            timeout_secs: 10,
            approval_timeout_secs: 300,
            restrict_to_workspace: false,
            policy: CommandPolicy::default(),
            permission_policy: None,
            approval_manager: None,
            session_id: None,
            channel: None,
            chat_id: None,
        };

        let args = json!({
            "command": "echo test"
        });

        let result = tool.call(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn permission_default_level_applied() {
        use crate::tools::permission::{CommandPermissionPolicy, PermissionLevel};
        
        let policy = CommandPermissionPolicy::new(
            vec![],
            PermissionLevel::Deny,
        );

        let tool = ExecTool {
            workspace: PathBuf::from("."),
            timeout_secs: 10,
            approval_timeout_secs: 300,
            restrict_to_workspace: false,
            policy: CommandPolicy::default(),
            permission_policy: Some(Arc::new(policy)),
            approval_manager: None,
            session_id: None,
            channel: None,
            chat_id: None,
        };

        let args = json!({
            "command": "echo test"
        });

        let result = tool.call(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Command denied by policy"));
    }
}
