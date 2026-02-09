//! Shell execution tool.

use anyhow::Result;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;

use crate::tools::DynTool;

const MAX_OUTPUT: usize = 10_000;

const BLOCKED_PATTERNS: &[&str] = &[
    "rm -rf /", "mkfs", "dd if=", "format", "shutdown", "reboot",
    ":(){", "fork bomb",
];

pub struct ExecTool {
    pub workspace: PathBuf,
    pub timeout_secs: u64,
    pub restrict_to_workspace: bool,
}

#[async_trait::async_trait]
impl DynTool for ExecTool {
    fn name(&self) -> &str { "exec" }
    fn description(&self) -> &str {
        "Execute a shell command and return output. Dangerous commands are blocked."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to run" },
                "working_dir": { "type": "string", "description": "Optional working directory" }
            },
            "required": ["command"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let cmd_str = args["command"].as_str().unwrap_or("");
        let lower = cmd_str.to_lowercase();
        for pat in BLOCKED_PATTERNS {
            if lower.contains(pat) {
                anyhow::bail!("Blocked dangerous command: {}", cmd_str);
            }
        }

        let cwd = args["working_dir"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.workspace.clone());

        let output = tokio::time::timeout(
            Duration::from_secs(self.timeout_secs),
            Command::new(if cfg!(windows) { "cmd" } else { "sh" })
                .args(if cfg!(windows) { vec!["/C", cmd_str] } else { vec!["-c", cmd_str] })
                .current_dir(&cwd)
                .output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Command timed out after {}s", self.timeout_secs))??;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut result = format!("exit code: {}\n", output.status.code().unwrap_or(-1));
        if !stdout.is_empty() {
            result.push_str(&stdout);
        }
        if !stderr.is_empty() {
            result.push_str("\n[stderr]\n");
            result.push_str(&stderr);
        }
        if result.len() > MAX_OUTPUT {
            result.truncate(MAX_OUTPUT);
            result.push_str("\n...[truncated]");
        }
        Ok(result)
    }
}
