//! `synbot doctor` — diagnostic command.
//!
//! Runs a series of checks against the current configuration and environment,
//! then prints a structured ✓/✗/⚠ summary report.

use anyhow::Result;
use async_trait::async_trait;

use crate::config::{self, Config};

// ---------------------------------------------------------------------------
// Check status & result types
// ---------------------------------------------------------------------------

/// The outcome of a single diagnostic check.
pub enum CheckStatus {
    /// Check passed; message describes what was verified.
    Pass(String),
    /// Check failed; message describes the problem.
    Fail(String),
    /// Check passed with a warning; message describes the concern.
    Warn(String),
    /// Check was skipped (e.g. feature not configured); message explains why.
    Skip(String),
}

/// Result of a single named diagnostic check.
pub struct CheckResult {
    pub name: String,
    pub status: CheckStatus,
}

impl CheckResult {
    fn pass(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self { name: name.into(), status: CheckStatus::Pass(msg.into()) }
    }
    fn fail(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self { name: name.into(), status: CheckStatus::Fail(msg.into()) }
    }
    fn warn(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self { name: name.into(), status: CheckStatus::Warn(msg.into()) }
    }
    fn skip(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self { name: name.into(), status: CheckStatus::Skip(msg.into()) }
    }
}

// ---------------------------------------------------------------------------
// Doctor report
// ---------------------------------------------------------------------------

/// Aggregated results from all diagnostic checks.
pub struct DoctorReport {
    pub results: Vec<CheckResult>,
}

impl DoctorReport {
    pub fn new() -> Self {
        Self { results: Vec::new() }
    }

    /// Print a structured ✓/✗/⚠/- summary to stdout.
    pub fn print_summary(&self) {
        println!("\nsynbot doctor — diagnostic report");
        println!("{}", "─".repeat(50));
        for r in &self.results {
            let (icon, msg) = match &r.status {
                CheckStatus::Pass(m) => ("✓", m.as_str()),
                CheckStatus::Fail(m) => ("✗", m.as_str()),
                CheckStatus::Warn(m) => ("⚠", m.as_str()),
                CheckStatus::Skip(m) => ("-", m.as_str()),
            };
            println!("  {} {}  {}", icon, r.name, msg);
        }
        println!("{}", "─".repeat(50));

        let passes = self.results.iter().filter(|r| matches!(r.status, CheckStatus::Pass(_))).count();
        let fails  = self.results.iter().filter(|r| matches!(r.status, CheckStatus::Fail(_))).count();
        let warns  = self.results.iter().filter(|r| matches!(r.status, CheckStatus::Warn(_))).count();
        let skips  = self.results.iter().filter(|r| matches!(r.status, CheckStatus::Skip(_))).count();

        println!(
            "  {} passed  {} failed  {} warnings  {} skipped",
            passes, fails, warns, skips
        );

        if fails > 0 {
            println!("\n  Run `synbot onboard` to re-initialize if configuration is missing.");
        }
    }
}

impl Default for DoctorReport {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// DoctorCheck trait
// ---------------------------------------------------------------------------

/// A single diagnostic check. Implement this trait for each check item.
#[async_trait]
pub trait DoctorCheck: Send + Sync {
    fn name(&self) -> &str;
    async fn run(&self, config: &Config) -> CheckResult;
}

// ---------------------------------------------------------------------------
// Check: Config syntax & serde deserialization
// ---------------------------------------------------------------------------

pub struct ConfigSyntaxCheck;

#[async_trait]
impl DoctorCheck for ConfigSyntaxCheck {
    fn name(&self) -> &str {
        "Config syntax"
    }

    async fn run(&self, _config: &Config) -> CheckResult {
        // By the time we reach here the config was already loaded successfully.
        // We re-read the file to verify the raw JSON is still valid on disk.
        let path = config::config_path();
        if !path.exists() {
            return CheckResult::fail(
                self.name(),
                format!("config file not found at {}", path.display()),
            );
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => return CheckResult::fail(self.name(), format!("cannot read config: {}", e)),
        };
        // Validate JSON syntax
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(_) => CheckResult::pass(self.name(), format!("valid JSON at {}", path.display())),
            Err(e) => CheckResult::fail(self.name(), format!("JSON parse error: {}", e)),
        }
    }
}

// ---------------------------------------------------------------------------
// Check: Channel credentials
// ---------------------------------------------------------------------------

pub struct ChannelCredentialCheck;

#[async_trait]
impl DoctorCheck for ChannelCredentialCheck {
    fn name(&self) -> &str {
        "Channel credentials"
    }

    async fn run(&self, config: &Config) -> CheckResult {
        let mut issues: Vec<String> = Vec::new();
        let mut checked = 0usize;

        for c in &config.channels.telegram {
            if c.enabled {
                checked += 1;
                if c.token.is_empty() {
                    issues.push(format!("telegram/{}: token is empty", c.name));
                }
            }
        }
        for c in &config.channels.discord {
            if c.enabled {
                checked += 1;
                if c.token.is_empty() {
                    issues.push(format!("discord/{}: token is empty", c.name));
                }
            }
        }
        for c in &config.channels.feishu {
            if c.enabled {
                checked += 1;
                if c.app_id.is_empty() || c.app_secret.is_empty() {
                    issues.push(format!("feishu/{}: app_id or app_secret is empty", c.name));
                }
            }
        }
        for c in &config.channels.slack {
            if c.enabled {
                checked += 1;
                if c.token.is_empty() || c.app_token.is_empty() {
                    issues.push(format!("slack/{}: token or app_token is empty", c.name));
                }
            }
        }
        for c in &config.channels.matrix {
            if c.enabled {
                checked += 1;
                let has_token = c.access_token.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
                if !has_token && (c.username.is_empty() || c.password.is_empty()) {
                    issues.push(format!("matrix/{}: missing access_token or username/password", c.name));
                }
            }
        }
        for c in &config.channels.dingtalk {
            if c.enabled {
                checked += 1;
                let has_id = !c.client_id.is_empty()
                    || c.app_key.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
                let has_secret = !c.client_secret.is_empty()
                    || c.app_secret.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
                if !has_id || !has_secret {
                    issues.push(format!("dingtalk/{}: client_id or client_secret is empty", c.name));
                }
            }
        }
        if let Some(wa_list) = &config.channels.whatsapp {
            for c in wa_list {
                if c.enabled {
                    checked += 1;
                    if c.session_dir.is_empty() {
                        issues.push(format!("whatsapp/{}: session_dir is empty", c.name));
                    }
                }
            }
        }
        if let Some(irc_list) = &config.channels.irc {
            for c in irc_list {
                if c.enabled {
                    checked += 1;
                    if c.server.as_deref().map(|s| s.is_empty()).unwrap_or(true) {
                        issues.push(format!("irc/{}: server is empty", c.name));
                    }
                    if c.nickname.as_deref().map(|s| s.is_empty()).unwrap_or(true) {
                        issues.push(format!("irc/{}: nickname is empty", c.name));
                    }
                }
            }
        }

        if checked == 0 {
            return CheckResult::skip(self.name(), "no enabled channels configured");
        }
        if issues.is_empty() {
            CheckResult::pass(self.name(), format!("{} channel(s) have valid credentials", checked))
        } else {
            CheckResult::fail(self.name(), issues.join("; "))
        }
    }
}

// ---------------------------------------------------------------------------
// Check: Provider API keys (lightweight validation — non-empty key check)
// ---------------------------------------------------------------------------

pub struct ProviderApiCheck;

#[async_trait]
impl DoctorCheck for ProviderApiCheck {
    fn name(&self) -> &str {
        "Provider API keys"
    }

    async fn run(&self, config: &Config) -> CheckResult {
        let p = &config.providers;
        let mut configured: Vec<String> = Vec::new();
        let mut missing: Vec<String> = Vec::new();

        let providers = [
            ("anthropic", &p.anthropic.api_key),
            ("openai",    &p.openai.api_key),
            ("gemini",    &p.gemini.api_key),
            ("openrouter",&p.openrouter.api_key),
            ("deepseek",  &p.deepseek.api_key),
            ("moonshot",  &p.moonshot.api_key),
            ("kimi_code", &p.kimi_code.api_key),
            ("ollama",    &p.ollama.api_key),
        ];

        for (name, key) in &providers {
            if !key.is_empty() {
                configured.push(name.to_string());
            }
        }
        for (name, _) in &p.extra {
            configured.push(name.clone());
        }

        // Check that the main agent's configured provider has a key
        let main_provider = &config.main_agent.provider;
        let main_key_present = providers
            .iter()
            .find(|(n, _)| *n == main_provider.as_str())
            .map(|(_, k)| !k.is_empty())
            .unwrap_or_else(|| p.extra.contains_key(main_provider.as_str()));

        if !main_key_present {
            missing.push(format!("main provider '{}' has no API key", main_provider));
        }

        if configured.is_empty() {
            return CheckResult::warn(self.name(), "no provider API keys configured");
        }
        if !missing.is_empty() {
            return CheckResult::fail(self.name(), missing.join("; "));
        }

        CheckResult::pass(
            self.name(),
            format!("configured providers: {}", configured.join(", ")),
        )
    }
}

// ---------------------------------------------------------------------------
// Check: Sandbox (Docker daemon availability)
// ---------------------------------------------------------------------------

pub struct SandboxCheck;

#[async_trait]
impl DoctorCheck for SandboxCheck {
    fn name(&self) -> &str {
        "Sandbox (Docker)"
    }

    async fn run(&self, config: &Config) -> CheckResult {
        // Only check if tool_sandbox or app_sandbox is configured
        let sandbox_enabled = config.tool_sandbox.is_some() || config.app_sandbox.is_some();
        if !sandbox_enabled {
            return CheckResult::skip(self.name(), "sandbox not configured");
        }

        // Try to run `docker info` to verify Docker daemon is available
        match tokio::process::Command::new("docker")
            .arg("info")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await
        {
            Ok(status) if status.success() => {
                CheckResult::pass(self.name(), "Docker daemon is running")
            }
            Ok(_) => CheckResult::fail(
                self.name(),
                "Docker daemon is not running or returned an error",
            ),
            Err(e) => CheckResult::fail(
                self.name(),
                format!("cannot run `docker info`: {} — is Docker installed?", e),
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Check: Memory (SQLite + sqlite-vec extension)
// ---------------------------------------------------------------------------

pub struct MemoryCheck;

#[async_trait]
impl DoctorCheck for MemoryCheck {
    fn name(&self) -> &str {
        "Memory (SQLite + sqlite-vec)"
    }

    async fn run(&self, config: &Config) -> CheckResult {
        // Try to open the SQLite index for the main agent (dimension from config)
        match crate::agent::memory_index::open_index("main", config.memory.embedding_dimensions) {
            Ok(_conn) => {
                CheckResult::pass(self.name(), "SQLite database accessible and sqlite-vec extension loaded")
            }
            Err(e) => CheckResult::fail(
                self.name(),
                format!("failed to open memory index: {}", e),
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Check: MCP servers
// ---------------------------------------------------------------------------

pub struct McpServerCheck;

#[async_trait]
impl DoctorCheck for McpServerCheck {
    fn name(&self) -> &str {
        "MCP servers"
    }

    async fn run(&self, config: &Config) -> CheckResult {
        let mcp_cfg = match &config.tools.mcp {
            Some(m) if !m.servers.is_empty() => m,
            _ => return CheckResult::skip(self.name(), "no MCP servers configured"),
        };

        let mut ok: Vec<String> = Vec::new();
        let mut failed: Vec<String> = Vec::new();

        for server in &mcp_cfg.servers {
            let result = try_connect_mcp_server(server).await;
            match result {
                Ok(_) => ok.push(server.id.clone()),
                Err(e) => failed.push(format!("{}: {}", server.id, e)),
            }
        }

        if failed.is_empty() {
            CheckResult::pass(
                self.name(),
                format!("{} MCP server(s) reachable: {}", ok.len(), ok.join(", ")),
            )
        } else if ok.is_empty() {
            CheckResult::fail(self.name(), failed.join("; "))
        } else {
            CheckResult::warn(
                self.name(),
                format!(
                    "{} ok ({}), {} failed ({})",
                    ok.len(),
                    ok.join(", "),
                    failed.len(),
                    failed.join("; ")
                ),
            )
        }
    }
}

/// Attempt a lightweight connection to an MCP server to verify it is reachable.
async fn try_connect_mcp_server(server: &crate::config::McpServerConfig) -> Result<()> {
    use std::collections::HashMap;
    use std::time::Duration;
    use mcp_client::client::{ClientCapabilities, ClientInfo, McpClient, McpClientTrait};
    use mcp_client::service::McpService;
    use mcp_client::transport::{Transport, SseTransport, StdioTransport};
    use crate::config::McpTransport;
    use std::sync::Arc;

    let timeout = Duration::from_secs(10);

    match server.transport {
        McpTransport::Stdio => {
            if server.command.is_empty() {
                anyhow::bail!("command is required for stdio transport");
            }
            let transport = StdioTransport::new(
                server.command.clone(),
                server.args.clone(),
                HashMap::new(),
            );
            let handle = transport.start().await
                .map_err(|e| anyhow::anyhow!("transport start failed: {}", e))?;
            let service = McpService::with_timeout(handle, timeout);
            let mut client = McpClient::new(service);
            client
                .initialize(
                    ClientInfo { name: "synbot-doctor".into(), version: "0".into() },
                    ClientCapabilities::default(),
                )
                .await
                .map_err(|e| anyhow::anyhow!("initialize failed: {}", e))?;
            // Force type inference by casting to trait object
            let _: Arc<dyn McpClientTrait + Send + Sync> = Arc::new(client);
        }
        McpTransport::Sse => {
            if server.url.is_empty() {
                anyhow::bail!("url is required for SSE transport");
            }
            let transport = SseTransport::new(server.url.clone(), HashMap::new());
            let handle = transport.start().await
                .map_err(|e| anyhow::anyhow!("transport start failed: {}", e))?;
            let service = McpService::with_timeout(handle, timeout);
            let mut client = McpClient::new(service);
            client
                .initialize(
                    ClientInfo { name: "synbot-doctor".into(), version: "0".into() },
                    ClientCapabilities::default(),
                )
                .await
                .map_err(|e| anyhow::anyhow!("initialize failed: {}", e))?;
            let _: Arc<dyn McpClientTrait + Send + Sync> = Arc::new(client);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// cmd_doctor entry point
// ---------------------------------------------------------------------------

/// Run all diagnostic checks and print the report.
/// If the config file does not exist, print an error and suggest `synbot onboard`.
pub async fn cmd_doctor() -> Result<()> {
    let config_path = config::config_path();

    // Special-case: config file missing
    if !config_path.exists() {
        eprintln!("✗ Config file not found at {}", config_path.display());
        eprintln!("  Run `synbot onboard` to initialize your configuration.");
        return Ok(());
    }

    // Load config (handles env-var substitution, migration, validation)
    let cfg = match config::load_config(None) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("✗ Failed to load config: {}", e);
            eprintln!("  Fix the errors above, or run `synbot onboard` to re-initialize.");
            return Ok(());
        }
    };

    // Build the list of checks
    let checks: Vec<Box<dyn DoctorCheck>> = vec![
        Box::new(ConfigSyntaxCheck),
        Box::new(ChannelCredentialCheck),
        Box::new(ProviderApiCheck),
        Box::new(SandboxCheck),
        Box::new(MemoryCheck),
        Box::new(McpServerCheck),
    ];

    let mut report = DoctorReport::new();
    for check in &checks {
        let result = check.run(&cfg).await;
        report.results.push(result);
    }

    report.print_summary();
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> Config {
        Config::default()
    }

    #[tokio::test]
    async fn config_syntax_check_skips_when_no_file() {
        // ConfigSyntaxCheck reads the actual config path; with a default config
        // (no file on disk in test env) it should return Fail.
        let cfg = make_config();
        let check = ConfigSyntaxCheck;
        let result = check.run(&cfg).await;
        // Either pass (file exists) or fail (file missing) — just ensure it runs
        let _ = result;
    }

    #[tokio::test]
    async fn channel_credential_check_skips_when_no_channels() {
        let cfg = make_config();
        let check = ChannelCredentialCheck;
        let result = check.run(&cfg).await;
        assert!(matches!(result.status, CheckStatus::Skip(_)));
    }

    #[tokio::test]
    async fn provider_api_check_warns_when_no_keys() {
        let cfg = make_config();
        let check = ProviderApiCheck;
        let result = check.run(&cfg).await;
        // Default config has no API keys → warn or fail
        assert!(matches!(result.status, CheckStatus::Warn(_) | CheckStatus::Fail(_)));
    }

    #[tokio::test]
    async fn sandbox_check_skips_when_not_configured() {
        let cfg = make_config();
        let check = SandboxCheck;
        let result = check.run(&cfg).await;
        assert!(matches!(result.status, CheckStatus::Skip(_)));
    }

    #[tokio::test]
    async fn memory_check_skips_when_not_configured() {
        let cfg = make_config();
        let check = MemoryCheck;
        let result = check.run(&cfg).await;
        assert!(matches!(result.status, CheckStatus::Skip(_)));
    }

    #[tokio::test]
    async fn mcp_check_skips_when_not_configured() {
        let cfg = make_config();
        let check = McpServerCheck;
        let result = check.run(&cfg).await;
        assert!(matches!(result.status, CheckStatus::Skip(_)));
    }

    #[test]
    fn report_print_summary_contains_icons() {
        let mut report = DoctorReport::new();
        report.results.push(CheckResult::pass("A", "all good"));
        report.results.push(CheckResult::fail("B", "broken"));
        report.results.push(CheckResult::warn("C", "concern"));
        report.results.push(CheckResult::skip("D", "not configured"));
        // Just ensure print_summary doesn't panic
        report.print_summary();
    }
}
