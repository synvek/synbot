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
// Role, Participant, Group, Topic configs
// ---------------------------------------------------------------------------

/// 参与者配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticipantConfig {
    pub channel: String,
    #[serde(default)]
    pub channel_user_id: Option<String>,
}

/// 群组配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupConfig {
    pub name: String,
    pub participants: Vec<ParticipantConfig>,
}

/// 话题配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopicConfig {
    pub name: String,
    pub participants: Vec<ParticipantConfig>,
}

/// 单个角色的配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleConfig {
    pub name: String,
    pub system_prompt: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    /// 以下字段可选，未设置时继承 AgentDefaults
    pub provider: Option<String>,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub max_iterations: Option<u32>,
}

// ---------------------------------------------------------------------------
// Agent defaults
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefaults {
    #[serde(default = "default_workspace")]
    pub workspace: String,
    #[serde(default = "default_provider")]
    pub provider: String,
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
    #[serde(default)]
    pub roles: Vec<RoleConfig>,
}

fn default_workspace() -> String {
    "~/.synbot/workspace".into()
}
fn default_provider() -> String {
    "anthropic".into()
}

fn default_model() -> String {
    "claude-sonnet-4-5".into()
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
            provider: default_model(),
            model: default_model(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            max_tool_iterations: default_max_iterations(),
            max_concurrent_subagents: default_max_concurrent_subagents(),
            roles: Vec::new(),
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
    #[serde(default)]
    pub main_channel: String,
    #[serde(default)]
    pub groups: Vec<GroupConfig>,
    #[serde(default)]
    pub topics: Vec<TopicConfig>,
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

    // --- Collect enabled channel names for reference validation ---
    let enabled_channels: Vec<&str> = {
        let mut ch = Vec::new();
        if config.channels.telegram.enabled {
            ch.push("telegram");
        }
        if config.channels.discord.enabled {
            ch.push("discord");
        }
        if config.channels.feishu.enabled {
            ch.push("feishu");
        }
        ch
    };

    // --- main_channel validation ---
    // Only validate when multi-agent features are in use
    let has_multi_agent_features = !config.agent.roles.is_empty()
        || !config.groups.is_empty()
        || !config.topics.is_empty();

    if has_multi_agent_features {
        if config.main_channel.is_empty() {
            errors.push(ValidationError {
                field: "main_channel".into(),
                value: String::new(),
                constraint: "must be non-empty when roles, groups, or topics are configured".into(),
            });
        } else if !enabled_channels.contains(&config.main_channel.as_str()) {
            errors.push(ValidationError {
                field: "main_channel".into(),
                value: config.main_channel.clone(),
                constraint: format!(
                    "must reference an enabled channel (available: {})",
                    enabled_channels.join(", ")
                ),
            });
        }
    }

    // --- Role validation ---
    let mut seen_role_names = std::collections::HashSet::new();
    for (i, role) in config.agent.roles.iter().enumerate() {
        let role_label = if role.name.is_empty() {
            format!("agent.roles[{}]", i)
        } else {
            format!("agent.roles[{}] ({})", i, role.name)
        };

        // Required fields
        if role.name.is_empty() {
            errors.push(ValidationError {
                field: format!("{}.name", role_label),
                value: String::new(),
                constraint: "role name must be non-empty".into(),
            });
        }
        if role.system_prompt.is_empty() {
            errors.push(ValidationError {
                field: format!("{}.system_prompt", role_label),
                value: String::new(),
                constraint: "role system_prompt must be non-empty".into(),
            });
        }

        // Name format: only [a-zA-Z0-9_]
        if !role.name.is_empty()
            && !role
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            errors.push(ValidationError {
                field: format!("{}.name", role_label),
                value: role.name.clone(),
                constraint: "role name must contain only letters, digits, and underscores"
                    .into(),
            });
        }

        // Duplicate names
        if !role.name.is_empty() && !seen_role_names.insert(role.name.clone()) {
            errors.push(ValidationError {
                field: format!("{}.name", role_label),
                value: role.name.clone(),
                constraint: "duplicate role name".into(),
            });
        }
    }

    // --- Groups participant channel validation ---
    for (i, group) in config.groups.iter().enumerate() {
        for (j, participant) in group.participants.iter().enumerate() {
            if !enabled_channels.contains(&participant.channel.as_str()) {
                errors.push(ValidationError {
                    field: format!("groups[{}].participants[{}].channel", i, j),
                    value: participant.channel.clone(),
                    constraint: format!(
                        "must reference an enabled channel (available: {})",
                        enabled_channels.join(", ")
                    ),
                });
            }
        }
    }

    // --- Topics participant channel validation ---
    for (i, topic) in config.topics.iter().enumerate() {
        for (j, participant) in topic.participants.iter().enumerate() {
            if !enabled_channels.contains(&participant.channel.as_str()) {
                errors.push(ValidationError {
                    field: format!("topics[{}].participants[{}].channel", i, j),
                    value: participant.channel.clone(),
                    constraint: format!(
                        "must reference an enabled channel (available: {})",
                        enabled_channels.join(", ")
                    ),
                });
            }
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

    // --- Helper: config with multi-agent features enabled ---

    fn config_with_telegram() -> Config {
        let mut cfg = valid_config();
        cfg.channels.telegram.enabled = true;
        cfg.channels.telegram.token = "bot123:abc".into();
        cfg.main_channel = "telegram".into();
        cfg
    }

    fn make_role(name: &str, system_prompt: &str) -> RoleConfig {
        RoleConfig {
            name: name.into(),
            system_prompt: system_prompt.into(),
            skills: Vec::new(),
            tools: Vec::new(),
            provider: None,
            model: None,
            max_tokens: None,
            temperature: None,
            max_iterations: None,
        }
    }

    // --- main_channel validation ---

    #[test]
    fn main_channel_empty_with_roles_is_rejected() {
        let mut cfg = config_with_telegram();
        cfg.main_channel = String::new();
        cfg.agent.roles = vec![make_role("helper", "You help")];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "main_channel"));
    }

    #[test]
    fn main_channel_referencing_disabled_channel_is_rejected() {
        let mut cfg = valid_config();
        cfg.main_channel = "telegram".into();
        cfg.agent.roles = vec![make_role("helper", "You help")];
        // telegram is not enabled
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "main_channel"));
    }

    #[test]
    fn main_channel_referencing_enabled_channel_is_accepted() {
        let mut cfg = config_with_telegram();
        cfg.agent.roles = vec![make_role("helper", "You help")];
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn main_channel_not_required_without_multi_agent_features() {
        let mut cfg = valid_config();
        cfg.main_channel = String::new();
        // No roles, groups, or topics
        assert!(validate_config(&cfg).is_ok());
    }

    // --- Role name format validation ---

    #[test]
    fn role_name_with_special_chars_is_rejected() {
        let mut cfg = config_with_telegram();
        cfg.agent.roles = vec![make_role("bad-name!", "prompt")];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.constraint.contains("letters, digits, and underscores")));
    }

    #[test]
    fn role_name_alphanumeric_underscore_is_accepted() {
        let mut cfg = config_with_telegram();
        cfg.agent.roles = vec![make_role("good_Role_123", "prompt")];
        assert!(validate_config(&cfg).is_ok());
    }

    // --- Role duplicate names ---

    #[test]
    fn duplicate_role_names_are_rejected() {
        let mut cfg = config_with_telegram();
        cfg.agent.roles = vec![
            make_role("helper", "prompt1"),
            make_role("helper", "prompt2"),
        ];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.constraint.contains("duplicate")));
    }

    // --- Role required fields ---

    #[test]
    fn role_empty_name_is_rejected() {
        let mut cfg = config_with_telegram();
        cfg.agent.roles = vec![make_role("", "prompt")];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.constraint.contains("name must be non-empty")));
    }

    #[test]
    fn role_empty_system_prompt_is_rejected() {
        let mut cfg = config_with_telegram();
        cfg.agent.roles = vec![make_role("helper", "")];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.constraint.contains("system_prompt must be non-empty")));
    }

    // --- Groups/topics participant channel validation ---

    #[test]
    fn group_participant_invalid_channel_is_rejected() {
        let mut cfg = config_with_telegram();
        cfg.groups = vec![GroupConfig {
            name: "team".into(),
            participants: vec![ParticipantConfig {
                channel: "slack".into(),
                channel_user_id: None,
            }],
        }];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.field.contains("groups[0].participants[0].channel")));
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
