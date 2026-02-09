use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Channel configs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
    pub proxy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiscordConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FeishuConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub app_secret: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChannelsConfig {
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub discord: DiscordConfig,
    #[serde(default)]
    pub feishu: FeishuConfig,
}

// ---------------------------------------------------------------------------
// Provider configs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderEntry {
    #[serde(default)]
    pub api_key: String,
    pub api_base: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersConfig {
    #[serde(default)]
    pub anthropic: ProviderEntry,
    #[serde(default)]
    pub openai: ProviderEntry,
    #[serde(default)]
    pub openrouter: ProviderEntry,
    #[serde(default)]
    pub deepseek: ProviderEntry,
    #[serde(default)]
    pub ollama: ProviderEntry,
}

// ---------------------------------------------------------------------------
// Agent defaults
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefaults {
    #[serde(default = "default_workspace")]
    pub workspace: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_iterations")]
    pub max_tool_iterations: u32,
    #[serde(default = "default_max_concurrent_subagents")]
    pub max_concurrent_subagents: usize,
}

fn default_workspace() -> String {
    "~/.synbot/workspace".into()
}
fn default_model() -> String {
    "anthropic/claude-sonnet-4-5".into()
}
fn default_max_tokens() -> u32 {
    8192
}
fn default_temperature() -> f32 {
    0.7
}
fn default_max_iterations() -> u32 {
    20
}
fn default_max_concurrent_subagents() -> usize {
    5
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self {
            workspace: default_workspace(),
            model: default_model(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            max_tool_iterations: default_max_iterations(),
            max_concurrent_subagents: default_max_concurrent_subagents(),
        }
    }
}

// ---------------------------------------------------------------------------
// Exec tool config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecToolConfig {
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub restrict_to_workspace: bool,
    #[serde(default = "default_deny_patterns")]
    pub deny_patterns: Vec<String>,
    #[serde(default)]
    pub allow_patterns: Option<Vec<String>>,
}

fn default_timeout() -> u64 {
    60
}

fn default_deny_patterns() -> Vec<String> {
    vec![
        "rm -rf /".to_string(),
        "mkfs".to_string(),
        "dd if=".to_string(),
        "format".to_string(),
        "shutdown".to_string(),
        "reboot".to_string(),
        ":(){".to_string(),
        "fork bomb".to_string(),
    ]
}

impl Default for ExecToolConfig {
    fn default() -> Self {
        Self {
            timeout_secs: default_timeout(),
            restrict_to_workspace: false,
            deny_patterns: default_deny_patterns(),
            allow_patterns: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Web tool config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebToolConfig {
    #[serde(default)]
    pub brave_api_key: String,
}

// ---------------------------------------------------------------------------
// Tools config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolsConfig {
    #[serde(default)]
    pub exec: ExecToolConfig,
    #[serde(default)]
    pub web: WebToolConfig,
}

// ---------------------------------------------------------------------------
// Root config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub channels: ChannelsConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub agent: AgentDefaults,
    #[serde(default)]
    pub tools: ToolsConfig,
}

// ---------------------------------------------------------------------------
// Config validation
// ---------------------------------------------------------------------------

/// A structured validation error identifying the field, its invalid value,
/// and the constraint that was violated.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub value: String,
    pub constraint: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "field '{}': value '{}' violates constraint: {}",
            self.field, self.value, self.constraint
        )
    }
}

/// Validate all fields of a [`Config`] against defined constraints.
///
/// Collects **all** validation errors and returns them at once so the user
/// can fix every problem in a single pass.
///
/// # Validation rules
/// - `agent.max_tokens > 0`
/// - `agent.temperature` in `[0.0, 2.0]`
/// - `agent.max_tool_iterations > 0`
/// - `tools.exec.timeout_secs > 0`
/// - Enabled channels must have non-empty credentials
pub fn validate_config(config: &Config) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    // --- Agent defaults ---
    if config.agent.max_tokens == 0 {
        errors.push(ValidationError {
            field: "agent.max_tokens".into(),
            value: config.agent.max_tokens.to_string(),
            constraint: "must be greater than 0".into(),
        });
    }

    if config.agent.temperature < 0.0 || config.agent.temperature > 2.0 {
        errors.push(ValidationError {
            field: "agent.temperature".into(),
            value: config.agent.temperature.to_string(),
            constraint: "must be between 0.0 and 2.0".into(),
        });
    }

    if config.agent.max_tool_iterations == 0 {
        errors.push(ValidationError {
            field: "agent.max_tool_iterations".into(),
            value: config.agent.max_tool_iterations.to_string(),
            constraint: "must be greater than 0".into(),
        });
    }

    // --- Tools ---
    if config.tools.exec.timeout_secs == 0 {
        errors.push(ValidationError {
            field: "tools.exec.timeout_secs".into(),
            value: config.tools.exec.timeout_secs.to_string(),
            constraint: "must be greater than 0".into(),
        });
    }

    // --- Channel credentials ---
    if config.channels.telegram.enabled && config.channels.telegram.token.is_empty() {
        errors.push(ValidationError {
            field: "channels.telegram.token".into(),
            value: String::new(),
            constraint: "must be non-empty when telegram is enabled".into(),
        });
    }

    if config.channels.discord.enabled && config.channels.discord.token.is_empty() {
        errors.push(ValidationError {
            field: "channels.discord.token".into(),
            value: String::new(),
            constraint: "must be non-empty when discord is enabled".into(),
        });
    }

    if config.channels.feishu.enabled {
        if config.channels.feishu.app_id.is_empty() {
            errors.push(ValidationError {
                field: "channels.feishu.app_id".into(),
                value: String::new(),
                constraint: "must be non-empty when feishu is enabled".into(),
            });
        }
        if config.channels.feishu.app_secret.is_empty() {
            errors.push(ValidationError {
                field: "channels.feishu.app_secret".into(),
                value: String::new(),
                constraint: "must be non-empty when feishu is enabled".into(),
            });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ---------------------------------------------------------------------------
// Paths & loading
// ---------------------------------------------------------------------------

pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".synbot")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn workspace_path(cfg: &Config) -> PathBuf {
    let raw = &cfg.agent.workspace;
    if raw.starts_with('~') {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(raw.trim_start_matches("~/"))
    } else {
        PathBuf::from(raw)
    }
}

pub fn load_config(path: Option<&Path>) -> Result<Config> {
    let p = path
        .map(PathBuf::from)
        .unwrap_or_else(config_path);

    let cfg = if p.exists() {
        let text = std::fs::read_to_string(&p)
            .with_context(|| format!("reading config from {}", p.display()))?;
        serde_json::from_str(&text)
            .with_context(|| format!("parsing config from {}", p.display()))?
    } else {
        Config::default()
    };

    // Validate after successful parsing
    if let Err(errors) = validate_config(&cfg) {
        let msg = errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        anyhow::bail!("config validation failed: {}", msg);
    }

    Ok(cfg)
}

pub fn save_config(cfg: &Config, path: Option<&Path>) -> Result<()> {
    let p = path
        .map(PathBuf::from)
        .unwrap_or_else(config_path);

    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cfg)?;
    std::fs::write(&p, json)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a valid default config (all defaults pass validation).
    fn valid_config() -> Config {
        Config::default()
    }

    /// Helper: find a ValidationError by field name in an error list.
    fn find_error<'a>(errors: &'a [ValidationError], field: &str) -> Option<&'a ValidationError> {
        errors.iter().find(|e| e.field == field)
    }

    // --- Default config should be valid ---

    #[test]
    fn default_config_passes_validation() {
        let cfg = valid_config();
        assert!(validate_config(&cfg).is_ok());
    }

    // --- agent.max_tokens ---

    #[test]
    fn max_tokens_zero_is_rejected() {
        let mut cfg = valid_config();
        cfg.agent.max_tokens = 0;
        let errors = validate_config(&cfg).unwrap_err();
        let err = find_error(&errors, "agent.max_tokens").expect("expected error for max_tokens");
        assert_eq!(err.value, "0");
        assert!(err.constraint.contains("greater than 0"));
    }

    #[test]
    fn max_tokens_positive_is_accepted() {
        let mut cfg = valid_config();
        cfg.agent.max_tokens = 1;
        assert!(validate_config(&cfg).is_ok());
    }

    // --- agent.temperature ---

    #[test]
    fn temperature_below_zero_is_rejected() {
        let mut cfg = valid_config();
        cfg.agent.temperature = -0.1;
        let errors = validate_config(&cfg).unwrap_err();
        assert!(find_error(&errors, "agent.temperature").is_some());
    }

    #[test]
    fn temperature_above_two_is_rejected() {
        let mut cfg = valid_config();
        cfg.agent.temperature = 2.1;
        let errors = validate_config(&cfg).unwrap_err();
        assert!(find_error(&errors, "agent.temperature").is_some());
    }

    #[test]
    fn temperature_at_boundaries_is_accepted() {
        let mut cfg = valid_config();
        cfg.agent.temperature = 0.0;
        assert!(validate_config(&cfg).is_ok());

        cfg.agent.temperature = 2.0;
        assert!(validate_config(&cfg).is_ok());
    }

    // --- agent.max_tool_iterations ---

    #[test]
    fn max_tool_iterations_zero_is_rejected() {
        let mut cfg = valid_config();
        cfg.agent.max_tool_iterations = 0;
        let errors = validate_config(&cfg).unwrap_err();
        assert!(find_error(&errors, "agent.max_tool_iterations").is_some());
    }

    // --- tools.exec.timeout_secs ---

    #[test]
    fn timeout_secs_zero_is_rejected() {
        let mut cfg = valid_config();
        cfg.tools.exec.timeout_secs = 0;
        let errors = validate_config(&cfg).unwrap_err();
        assert!(find_error(&errors, "tools.exec.timeout_secs").is_some());
    }

    // --- Channel credential validation ---

    #[test]
    fn telegram_enabled_with_empty_token_is_rejected() {
        let mut cfg = valid_config();
        cfg.channels.telegram.enabled = true;
        cfg.channels.telegram.token = String::new();
        let errors = validate_config(&cfg).unwrap_err();
        assert!(find_error(&errors, "channels.telegram.token").is_some());
    }

    #[test]
    fn telegram_enabled_with_token_is_accepted() {
        let mut cfg = valid_config();
        cfg.channels.telegram.enabled = true;
        cfg.channels.telegram.token = "bot123:abc".into();
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn telegram_disabled_with_empty_token_is_accepted() {
        let mut cfg = valid_config();
        cfg.channels.telegram.enabled = false;
        cfg.channels.telegram.token = String::new();
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn discord_enabled_with_empty_token_is_rejected() {
        let mut cfg = valid_config();
        cfg.channels.discord.enabled = true;
        cfg.channels.discord.token = String::new();
        let errors = validate_config(&cfg).unwrap_err();
        assert!(find_error(&errors, "channels.discord.token").is_some());
    }

    #[test]
    fn discord_enabled_with_token_is_accepted() {
        let mut cfg = valid_config();
        cfg.channels.discord.enabled = true;
        cfg.channels.discord.token = "discord-token".into();
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn feishu_enabled_with_empty_credentials_is_rejected() {
        let mut cfg = valid_config();
        cfg.channels.feishu.enabled = true;
        cfg.channels.feishu.app_id = String::new();
        cfg.channels.feishu.app_secret = String::new();
        let errors = validate_config(&cfg).unwrap_err();
        assert!(find_error(&errors, "channels.feishu.app_id").is_some());
        assert!(find_error(&errors, "channels.feishu.app_secret").is_some());
    }

    #[test]
    fn feishu_enabled_with_credentials_is_accepted() {
        let mut cfg = valid_config();
        cfg.channels.feishu.enabled = true;
        cfg.channels.feishu.app_id = "app-id".into();
        cfg.channels.feishu.app_secret = "app-secret".into();
        assert!(validate_config(&cfg).is_ok());
    }

    // --- Multiple errors collected at once ---

    #[test]
    fn multiple_errors_are_collected() {
        let mut cfg = valid_config();
        cfg.agent.max_tokens = 0;
        cfg.agent.temperature = 5.0;
        cfg.tools.exec.timeout_secs = 0;
        cfg.channels.telegram.enabled = true;
        cfg.channels.telegram.token = String::new();

        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.len() >= 4, "expected at least 4 errors, got {}", errors.len());
        assert!(find_error(&errors, "agent.max_tokens").is_some());
        assert!(find_error(&errors, "agent.temperature").is_some());
        assert!(find_error(&errors, "tools.exec.timeout_secs").is_some());
        assert!(find_error(&errors, "channels.telegram.token").is_some());
    }

    // --- load_config integration ---

    #[test]
    fn load_config_with_invalid_values_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad_config.json");
        std::fs::write(
            &path,
            r#"{"agent":{"maxTokens":0,"temperature":3.0}}"#,
        )
        .unwrap();

        let result = load_config(Some(&path));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("config validation failed"));
        assert!(msg.contains("agent.max_tokens"));
        assert!(msg.contains("agent.temperature"));
    }

    #[test]
    fn load_config_with_valid_file_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("good_config.json");
        std::fs::write(
            &path,
            r#"{"agent":{"maxTokens":4096,"temperature":0.5,"maxToolIterations":10},"tools":{"exec":{"timeoutSecs":30}}}"#,
        )
        .unwrap();

        let result = load_config(Some(&path));
        assert!(result.is_ok());
        let cfg = result.unwrap();
        assert_eq!(cfg.agent.max_tokens, 4096);
    }

    // --- ValidationError Display ---

    #[test]
    fn validation_error_display_format() {
        let err = ValidationError {
            field: "agent.temperature".into(),
            value: "3.0".into(),
            constraint: "must be between 0.0 and 2.0".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("agent.temperature"));
        assert!(msg.contains("3.0"));
        assert!(msg.contains("must be between 0.0 and 2.0"));
    }
}
