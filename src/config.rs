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
    /// When true (default), push tool execution progress to this channel.
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
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
    /// When true (default), push tool execution progress to this channel.
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
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
    /// When true (default), push tool execution progress to this channel.
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
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
    /// 已废弃：请改用 reference，从 templates/roles/{reference} 的 md 文件生成 prompt
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub system_prompt: Option<String>,
    /// 引用 templates/roles 下的子目录名，用于从 AGENTS.md、SOUL.md、TOOLS.md 生成 system prompt
    #[serde(default)]
    pub reference: Option<String>,
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
// Permission config
// ---------------------------------------------------------------------------

use crate::tools::permission::{PermissionLevel, PermissionRule};

/// 权限配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionConfig {
    /// 是否启用权限控制
    #[serde(default)]
    pub enabled: bool,
    /// 默认权限级别（未匹配任何规则时使用）
    #[serde(default = "default_permission_level")]
    pub default_level: PermissionLevel,
    /// 审批请求超时时间（秒）
    #[serde(default = "default_approval_timeout")]
    pub approval_timeout_secs: u64,
    /// 权限规则列表（按顺序匹配）
    #[serde(default)]
    pub rules: Vec<PermissionRule>,
}

fn default_permission_level() -> PermissionLevel {
    PermissionLevel::RequireApproval
}

fn default_approval_timeout() -> u64 {
    60 // 5 minutes
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_level: default_permission_level(),
            approval_timeout_secs: default_approval_timeout(),
            rules: Vec::new(),
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
    #[serde(default)]
    pub permissions: PermissionConfig,
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
            permissions: PermissionConfig::default(),
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
// Log config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogConfig {
    /// Log level: trace, debug, info, warn, error
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Log format: json, text, compact, pretty
    #[serde(default = "default_log_format")]
    pub format: String,
    /// Log directory path
    #[serde(default = "default_log_dir")]
    pub dir: String,
    /// Maximum number of log files to keep
    #[serde(default = "default_max_log_files")]
    pub max_files: u32,
    /// Maximum size of each log file in MB
    #[serde(default = "default_max_log_file_size")]
    pub max_file_size_mb: u32,
    /// Show timestamps in logs
    #[serde(default = "default_show_timestamp")]
    pub show_timestamp: bool,
    /// Show log level in logs
    #[serde(default = "default_show_level")]
    pub show_level: bool,
    /// Show target module in logs
    #[serde(default = "default_show_target")]
    pub show_target: bool,
    /// Show thread names in logs
    #[serde(default = "default_show_thread_names")]
    pub show_thread_names: bool,
    /// Show thread IDs in logs
    #[serde(default = "default_show_thread_ids")]
    pub show_thread_ids: bool,
    /// Show file and line number in logs
    #[serde(default = "default_show_file")]
    pub show_file: bool,
    /// Timestamp format: rfc3339, local, utc, custom
    #[serde(default = "default_timestamp_format")]
    pub timestamp_format: String,
    /// Custom timestamp format string (e.g., "%Y-%m-%d %H:%M:%S")
    #[serde(default)]
    pub custom_timestamp_format: Option<String>,
    /// Module-specific log levels (e.g., {"synbot": "debug", "open_lark": "info"})
    #[serde(default)]
    pub module_levels: std::collections::HashMap<String, String>,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "text".to_string()
}

fn default_log_dir() -> String {
    "logs".to_string()
}

fn default_max_log_files() -> u32 {
    20
}

fn default_max_log_file_size() -> u32 {
    50
}

fn default_show_timestamp() -> bool {
    true
}

fn default_show_level() -> bool {
    true
}

fn default_show_target() -> bool {
    true
}

fn default_show_thread_names() -> bool {
    false
}

fn default_show_thread_ids() -> bool {
    false
}

fn default_show_file() -> bool {
    false
}

fn default_timestamp_format() -> String {
    "local".to_string()
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
            dir: default_log_dir(),
            max_files: default_max_log_files(),
            max_file_size_mb: default_max_log_file_size(),
            show_timestamp: default_show_timestamp(),
            show_level: default_show_level(),
            show_target: default_show_target(),
            show_thread_names: default_show_thread_names(),
            show_thread_ids: default_show_thread_ids(),
            show_file: default_show_file(),
            timestamp_format: default_timestamp_format(),
            custom_timestamp_format: None,
            module_levels: std::collections::HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Web server config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebAuthConfig {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_web_port")]
    pub port: u16,
    #[serde(default = "default_web_host")]
    pub host: String,
    #[serde(default)]
    pub auth: Option<WebAuthConfig>,
    #[serde(default)]
    pub cors_origins: Vec<String>,
    /// When true (default), push tool execution progress to web clients.
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
}

fn default_web_port() -> u16 {
    18888
}

fn default_web_host() -> String {
    "127.0.0.1".to_string()
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: default_web_port(),
            host: default_web_host(),
            auth: None,
            cors_origins: Vec::new(),
            show_tool_calls: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Memory config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCompressionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_compression_max_turns")]
    pub max_conversation_turns: u32,
    #[serde(default = "default_summary_write_to_memory")]
    pub summary_write_to_memory: bool,
}

fn default_compression_max_turns() -> u32 {
    50
}
fn default_summary_write_to_memory() -> bool {
    true
}

impl Default for MemoryCompressionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_conversation_turns: default_compression_max_turns(),
            summary_write_to_memory: default_summary_write_to_memory(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryConfig {
    #[serde(default)]
    pub backend: String,
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    #[serde(default = "default_vector_weight")]
    pub vector_weight: f32,
    #[serde(default = "default_text_weight")]
    pub text_weight: f32,
    #[serde(default = "default_auto_index")]
    pub auto_index: bool,
    #[serde(default)]
    pub compression: MemoryCompressionConfig,
}

fn default_embedding_model() -> String {
    "local/default".to_string()
}
fn default_vector_weight() -> f32 {
    0.7
}
fn default_text_weight() -> f32 {
    0.3
}
fn default_auto_index() -> bool {
    true
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            backend: String::new(),
            embedding_model: default_embedding_model(),
            vector_weight: default_vector_weight(),
            text_weight: default_text_weight(),
            auto_index: default_auto_index(),
            compression: MemoryCompressionConfig::default(),
        }
    }
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

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// When true (default), all channels may receive tool execution progress; each channel can override via channels.*.showToolCalls.
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
    #[serde(default)]
    pub channels: ChannelsConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub agent: AgentDefaults,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub web: WebConfig,
    #[serde(default)]
    pub log: LogConfig,
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

    // --- Memory ---
    if config.memory.vector_weight < 0.0 || config.memory.vector_weight > 1.0 {
        errors.push(ValidationError {
            field: "memory.vectorWeight".into(),
            value: config.memory.vector_weight.to_string(),
            constraint: "must be between 0.0 and 1.0".into(),
        });
    }
    if config.memory.text_weight < 0.0 || config.memory.text_weight > 1.0 {
        errors.push(ValidationError {
            field: "memory.textWeight".into(),
            value: config.memory.text_weight.to_string(),
            constraint: "must be between 0.0 and 1.0".into(),
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

    // --- Log config validation ---
    let valid_log_levels = ["trace", "debug", "info", "warn", "error"];
    if !valid_log_levels.contains(&config.log.level.to_lowercase().as_str()) {
        errors.push(ValidationError {
            field: "log.level".into(),
            value: config.log.level.clone(),
            constraint: format!("must be one of: {}", valid_log_levels.join(", ")),
        });
    }

    let valid_log_formats = ["json", "text", "compact", "pretty"];
    if !valid_log_formats.contains(&config.log.format.to_lowercase().as_str()) {
        errors.push(ValidationError {
            field: "log.format".into(),
            value: config.log.format.clone(),
            constraint: format!("must be one of: {}", valid_log_formats.join(", ")),
        });
    }

    if config.log.max_files == 0 {
        errors.push(ValidationError {
            field: "log.max_files".into(),
            value: config.log.max_files.to_string(),
            constraint: "must be greater than 0".into(),
        });
    }

    if config.log.max_file_size_mb == 0 {
        errors.push(ValidationError {
            field: "log.max_file_size_mb".into(),
            value: config.log.max_file_size_mb.to_string(),
            constraint: "must be greater than 0".into(),
        });
    }

    // Validate module-specific log levels
    for (module, level) in &config.log.module_levels {
        if !valid_log_levels.contains(&level.to_lowercase().as_str()) {
            errors.push(ValidationError {
                field: format!("log.module_levels.{}", module),
                value: level.clone(),
                constraint: format!("must be one of: {}", valid_log_levels.join(", ")),
            });
        }
    }

    // --- Permission config validation ---
    if config.tools.exec.permissions.enabled {
        if config.tools.exec.permissions.approval_timeout_secs == 0 {
            errors.push(ValidationError {
                field: "tools.exec.permissions.approval_timeout_secs".into(),
                value: config.tools.exec.permissions.approval_timeout_secs.to_string(),
                constraint: "must be greater than 0".into(),
            });
        }

        // Validate permission rules
        for (i, rule) in config.tools.exec.permissions.rules.iter().enumerate() {
            if rule.pattern.is_empty() {
                errors.push(ValidationError {
                    field: format!("tools.exec.permissions.rules[{}].pattern", i),
                    value: String::new(),
                    constraint: "pattern must be non-empty".into(),
                });
            }
        }
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

/// Memory root directory: `~/.synbot/memory/`.
pub fn memory_root() -> PathBuf {
    config_dir().join("memory")
}

/// Sessions root directory: `~/.synbot/sessions/`.
/// Main agent sessions live in `sessions_root()/main/`, role sessions in `sessions_root()/{role}/`.
pub fn sessions_root() -> PathBuf {
    config_dir().join("sessions")
}

/// Memory directory for an agent: `~/.synbot/memory/{agentId}`.
/// Empty or "main" both resolve to the default "main" agent directory.
pub fn memory_dir(agent_id: &str) -> PathBuf {
    let id = if agent_id.trim().is_empty() {
        "main"
    } else {
        agent_id
    };
    memory_root().join(id)
}

/// 固定路径：role 模板目录，onboard 时从 templates/roles 写入此处。
pub fn roles_dir() -> PathBuf {
    config_dir().join("roles")
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

pub fn log_dir_path(cfg: &Config) -> PathBuf {
    let raw = &cfg.log.dir;
    if raw.starts_with('~') {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(raw.trim_start_matches("~/"))
    } else if raw.starts_with('/') || raw.contains(':') {
        // Absolute path
        PathBuf::from(raw)
    } else {
        // Relative path - relative to config_dir
        config_dir().join(raw)
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

    fn make_role(name: &str, reference: Option<&str>) -> RoleConfig {
        RoleConfig {
            name: name.into(),
            system_prompt: None,
            reference: reference.map(String::from),
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
        cfg.agent.roles = vec![make_role("helper", Some("dev"))];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "main_channel"));
    }

    #[test]
    fn main_channel_referencing_disabled_channel_is_rejected() {
        let mut cfg = valid_config();
        cfg.main_channel = "telegram".into();
        cfg.agent.roles = vec![make_role("helper", Some("dev"))];
        // telegram is not enabled
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "main_channel"));
    }

    #[test]
    fn main_channel_referencing_enabled_channel_is_accepted() {
        let mut cfg = config_with_telegram();
        cfg.agent.roles = vec![make_role("helper", Some("dev"))];
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
        cfg.agent.roles = vec![make_role("bad-name!", Some("dev"))];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.constraint.contains("letters, digits, and underscores")));
    }

    #[test]
    fn role_name_alphanumeric_underscore_is_accepted() {
        let mut cfg = config_with_telegram();
        cfg.agent.roles = vec![make_role("good_Role_123", Some("dev"))];
        assert!(validate_config(&cfg).is_ok());
    }

    // --- Role duplicate names ---

    #[test]
    fn duplicate_role_names_are_rejected() {
        let mut cfg = config_with_telegram();
        cfg.agent.roles = vec![
            make_role("helper", Some("dev")),
            make_role("helper", Some("dev")),
        ];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.constraint.contains("duplicate")));
    }

    // --- Role required fields ---

    #[test]
    fn role_empty_name_is_rejected() {
        let mut cfg = config_with_telegram();
        cfg.agent.roles = vec![make_role("", Some("dev"))];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.constraint.contains("name must be non-empty")));
    }

    #[test]
    fn role_without_reference_is_accepted() {
        let mut cfg = config_with_telegram();
        cfg.agent.roles = vec![make_role("helper", None)];
        assert!(validate_config(&cfg).is_ok());
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

    // --- Permission config loading tests ---

    #[test]
    fn permission_config_default_is_disabled() {
        let cfg = valid_config();
        assert!(!cfg.tools.exec.permissions.enabled);
        assert_eq!(cfg.tools.exec.permissions.default_level, PermissionLevel::RequireApproval);
        assert_eq!(cfg.tools.exec.permissions.approval_timeout_secs, 300);
        assert!(cfg.tools.exec.permissions.rules.is_empty());
    }

    #[test]
    fn permission_config_loads_from_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config_with_permissions.json");
        let json = r#"{
            "tools": {
                "exec": {
                    "permissions": {
                        "enabled": true,
                        "defaultLevel": "require_approval",
                        "approvalTimeoutSecs": 600,
                        "rules": [
                            {
                                "pattern": "ls*",
                                "level": "allow",
                                "description": "Allow ls commands"
                            },
                            {
                                "pattern": "rm*",
                                "level": "deny"
                            }
                        ]
                    }
                }
            }
        }"#;
        std::fs::write(&path, json).unwrap();

        let cfg = load_config(Some(&path)).unwrap();
        assert!(cfg.tools.exec.permissions.enabled);
        assert_eq!(cfg.tools.exec.permissions.default_level, PermissionLevel::RequireApproval);
        assert_eq!(cfg.tools.exec.permissions.approval_timeout_secs, 600);
        assert_eq!(cfg.tools.exec.permissions.rules.len(), 2);
        assert_eq!(cfg.tools.exec.permissions.rules[0].pattern, "ls*");
        assert_eq!(cfg.tools.exec.permissions.rules[0].level, PermissionLevel::Allow);
        assert_eq!(cfg.tools.exec.permissions.rules[1].pattern, "rm*");
        assert_eq!(cfg.tools.exec.permissions.rules[1].level, PermissionLevel::Deny);
    }

    #[test]
    fn permission_config_with_minimal_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config_minimal_permissions.json");
        let json = r#"{
            "tools": {
                "exec": {
                    "permissions": {
                        "enabled": true
                    }
                }
            }
        }"#;
        std::fs::write(&path, json).unwrap();

        let cfg = load_config(Some(&path)).unwrap();
        assert!(cfg.tools.exec.permissions.enabled);
        assert_eq!(cfg.tools.exec.permissions.default_level, PermissionLevel::RequireApproval);
        assert_eq!(cfg.tools.exec.permissions.approval_timeout_secs, 300);
        assert!(cfg.tools.exec.permissions.rules.is_empty());
    }

    #[test]
    fn permission_config_serialization_roundtrip() {
        let mut cfg = valid_config();
        cfg.tools.exec.permissions.enabled = true;
        cfg.tools.exec.permissions.default_level = PermissionLevel::Deny;
        cfg.tools.exec.permissions.approval_timeout_secs = 600;
        cfg.tools.exec.permissions.rules = vec![
            PermissionRule {
                pattern: "git*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("Allow git commands".to_string()),
            },
            PermissionRule {
                pattern: "sudo*".to_string(),
                level: PermissionLevel::Deny,
                description: None,
            },
        ];

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roundtrip.json");
        save_config(&cfg, Some(&path)).unwrap();
        let loaded = load_config(Some(&path)).unwrap();

        assert_eq!(loaded.tools.exec.permissions.enabled, cfg.tools.exec.permissions.enabled);
        assert_eq!(loaded.tools.exec.permissions.default_level, cfg.tools.exec.permissions.default_level);
        assert_eq!(loaded.tools.exec.permissions.approval_timeout_secs, cfg.tools.exec.permissions.approval_timeout_secs);
        assert_eq!(loaded.tools.exec.permissions.rules.len(), cfg.tools.exec.permissions.rules.len());
    }

    // --- Permission config validation tests ---

    #[test]
    fn permission_approval_timeout_zero_is_rejected() {
        let mut cfg = valid_config();
        cfg.tools.exec.permissions.enabled = true;
        cfg.tools.exec.permissions.approval_timeout_secs = 0;
        let errors = validate_config(&cfg).unwrap_err();
        let err = find_error(&errors, "tools.exec.permissions.approval_timeout_secs")
            .expect("expected error for approval_timeout_secs");
        assert_eq!(err.value, "0");
        assert!(err.constraint.contains("greater than 0"));
    }

    #[test]
    fn permission_approval_timeout_positive_is_accepted() {
        let mut cfg = valid_config();
        cfg.tools.exec.permissions.enabled = true;
        cfg.tools.exec.permissions.approval_timeout_secs = 300;
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn permission_disabled_with_zero_timeout_is_accepted() {
        let mut cfg = valid_config();
        cfg.tools.exec.permissions.enabled = false;
        cfg.tools.exec.permissions.approval_timeout_secs = 0;
        // Validation only checks when enabled
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn permission_rule_empty_pattern_is_rejected() {
        let mut cfg = valid_config();
        cfg.tools.exec.permissions.enabled = true;
        cfg.tools.exec.permissions.rules = vec![
            PermissionRule {
                pattern: String::new(),
                level: PermissionLevel::Allow,
                description: None,
            },
        ];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.field.contains("tools.exec.permissions.rules[0].pattern")));
        assert!(errors.iter().any(|e| e.constraint.contains("pattern must be non-empty")));
    }

    #[test]
    fn permission_rule_valid_pattern_is_accepted() {
        let mut cfg = valid_config();
        cfg.tools.exec.permissions.enabled = true;
        cfg.tools.exec.permissions.rules = vec![
            PermissionRule {
                pattern: "ls*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("Allow ls commands".to_string()),
            },
            PermissionRule {
                pattern: "rm -rf*".to_string(),
                level: PermissionLevel::Deny,
                description: None,
            },
        ];
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn permission_multiple_validation_errors_collected() {
        let mut cfg = valid_config();
        cfg.tools.exec.permissions.enabled = true;
        cfg.tools.exec.permissions.approval_timeout_secs = 0;
        cfg.tools.exec.permissions.rules = vec![
            PermissionRule {
                pattern: String::new(),
                level: PermissionLevel::Allow,
                description: None,
            },
            PermissionRule {
                pattern: String::new(),
                level: PermissionLevel::Deny,
                description: None,
            },
        ];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.len() >= 3, "expected at least 3 errors, got {}", errors.len());
        assert!(find_error(&errors, "tools.exec.permissions.approval_timeout_secs").is_some());
        assert!(errors.iter().any(|e| e.field.contains("tools.exec.permissions.rules[0].pattern")));
        assert!(errors.iter().any(|e| e.field.contains("tools.exec.permissions.rules[1].pattern")));
    }
}

