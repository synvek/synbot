use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Channel configs
// ---------------------------------------------------------------------------

/// Single allowlist entry: one chat (DM or group) allowed to talk to the bot.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AllowlistEntry {
    /// Chat ID (user id for DM, group/channel id for group).
    pub chat_id: String,
    /// Human-readable alias for this chat (for logs and UI).
    pub chat_alias: String,
    /// Bot's name in the group (optional). When set, only messages starting with @my_name are processed.
    #[serde(default)]
    pub my_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TelegramConfig {
    /// Unique channel name (used to associate channel; default "telegram").
    #[serde(default = "default_telegram_name")]
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub allowlist: Vec<AllowlistEntry>,
    /// When true (default), only chats in allowlist are accepted; when false, allowlist is not checked.
    #[serde(default = "default_true")]
    pub enable_allowlist: bool,
    /// When enable_allowlist is false, bot name used for group @ check (optional).
    #[serde(default)]
    pub group_my_name: Option<String>,
    pub proxy: Option<String>,
    /// When true (default), push tool execution progress to this channel.
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiscordConfig {
    /// Unique channel name (default "discord").
    #[serde(default = "default_discord_name")]
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub allowlist: Vec<AllowlistEntry>,
    /// When true (default), only chats in allowlist are accepted; when false, allowlist is not checked.
    #[serde(default = "default_true")]
    pub enable_allowlist: bool,
    /// When enable_allowlist is false, bot name/id used for group @ check (optional).
    #[serde(default)]
    pub group_my_name: Option<String>,
    /// When true (default), push tool execution progress to this channel.
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FeishuConfig {
    /// Unique channel name (default "feishu").
    #[serde(default = "default_feishu_name")]
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub app_secret: String,
    #[serde(default)]
    pub allowlist: Vec<AllowlistEntry>,
    /// When true (default), only chats in allowlist are accepted; when false, allowlist is not checked.
    #[serde(default = "default_true")]
    pub enable_allowlist: bool,
    /// When enable_allowlist is false, bot name used for group @ check (optional).
    #[serde(default)]
    pub group_my_name: Option<String>,
    /// When true (default), push tool execution progress to this channel.
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
}

fn default_telegram_name() -> String {
    "telegram".into()
}
fn default_discord_name() -> String {
    "discord".into()
}
fn default_feishu_name() -> String {
    "feishu".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChannelsConfig {
    #[serde(default)]
    pub telegram: Vec<TelegramConfig>,
    #[serde(default)]
    pub discord: Vec<DiscordConfig>,
    #[serde(default)]
    pub feishu: Vec<FeishuConfig>,
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
    pub moonshot: ProviderEntry,
    #[serde(default)]
    pub ollama: ProviderEntry,
}

// ---------------------------------------------------------------------------
// Role configs
// ---------------------------------------------------------------------------

/// Configuration for a single role.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleConfig {
    pub name: String,
    /// Deprecated: use `reference` instead; system prompt is generated from templates/roles/{reference} md files.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub system_prompt: Option<String>,
    /// Subdirectory name under templates/roles; used to build system prompt from AGENTS.md, SOUL.md, TOOLS.md.
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    /// Optional; when unset, values are inherited from AgentDefaults.
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

/// Permission configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionConfig {
    /// Whether permission control is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Default permission level when no rule matches.
    #[serde(default = "default_permission_level")]
    pub default_level: PermissionLevel,
    /// Approval request timeout (seconds)
    #[serde(default = "default_approval_timeout")]
    pub approval_timeout_secs: u64,
    /// List of permission rules (matched in order).
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

fn default_search_count() -> u32 {
    5
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

/// Which search backend to use for web_search.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum WebSearchBackend {
    /// DuckDuckGo HTML scraping — no API key required (default).
    #[default]
    DuckDuckGo,
    /// SearxNG self-hosted instance — requires `searxng_url`.
    SearxNG,
    /// Brave Search API — requires `brave_api_key`.
    Brave,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebToolConfig {
    /// Legacy Brave API key (kept for backwards compatibility; sets backend=brave when non-empty).
    #[serde(default)]
    pub brave_api_key: String,

    /// Active search backend. Defaults to duckDuckGo when not set.
    #[serde(default)]
    pub search_backend: WebSearchBackend,

    /// SearxNG instance base URL, e.g. "https://searx.example.com".
    #[serde(default)]
    pub searxng_url: String,

    /// Maximum results to return (default 5).
    #[serde(default = "default_search_count")]
    pub search_count: u32,
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
// Heartbeat config
// ---------------------------------------------------------------------------

/// A single heartbeat task: run periodically and send result to the given channel/chat.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatTask {
    /// Channel name (e.g. "feishu", "telegram").
    pub channel: String,
    /// Conversation id (group id or user id for DM) where to send the result.
    pub chat_id: String,
    /// User id of the task creator (for display / reply_to).
    pub user_id: String,
    /// Task content to execute (e.g. "list files in current directory").
    pub target: String,
}

/// Heartbeat: periodic execution of tasks from config, results sent to configured channel/user.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeartbeatConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Interval in seconds (default 300 = 5 minutes).
    #[serde(default = "default_heartbeat_interval")]
    pub interval: u64,
    #[serde(default)]
    pub tasks: Vec<HeartbeatTask>,
}

fn default_heartbeat_interval() -> u64 {
    300
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: default_heartbeat_interval(),
            tasks: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Cron config (config-file cron tasks; similar to heartbeat but with cron schedule)
// ---------------------------------------------------------------------------

/// A single cron task from config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronTaskConfig {
    /// Cron expression (e.g. "0 9 * * 1-5").
    pub schedule: String,
    /// Human-readable description (e.g. "weekdays at 9:00").
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Task content to execute.
    #[serde(default)]
    pub command: String,
    /// Channel name for result (e.g. "feishu").
    pub channel: String,
    /// User id to receive result.
    pub user_id: String,
    /// Conversation id where to send result (defaults to user_id for DM).
    #[serde(default)]
    pub chat_id: Option<String>,
}

/// Cron config: array of cron tasks (schedule, command, channel, userId).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CronConfig {
    #[serde(default)]
    pub tasks: Vec<CronTaskConfig>,
}

// ---------------------------------------------------------------------------
// Sandbox config (app_sandbox / tool_sandbox / monitoring)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SandboxFilesystemConfig {
    #[serde(default)]
    pub readonly_paths: Vec<String>,
    #[serde(default)]
    pub writable_paths: Vec<String>,
    #[serde(default)]
    pub hidden_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SandboxNetworkConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
    #[serde(default)]
    pub allowed_ports: Vec<u16>,
}

/// Resource limits; max_memory and max_disk can be "2G", "512M", or bytes number.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SandboxResourceConfig {
    #[serde(default)]
    pub max_memory: Option<SandboxSize>,
    #[serde(default)]
    pub max_cpu: Option<f64>,
    #[serde(default)]
    pub max_disk: Option<SandboxSize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SandboxSize {
    String(String),
    Number(u64),
}

impl Default for SandboxSize {
    fn default() -> Self {
        SandboxSize::String("1G".to_string())
    }
}

/// Parse size string ("2G", "512M", "1K") or number to bytes.
pub fn parse_sandbox_size_bytes(v: &SandboxSize) -> anyhow::Result<u64> {
    match v {
        SandboxSize::String(s) => parse_size_str(s),
        SandboxSize::Number(n) => Ok(*n),
    }
}

fn parse_size_str(s: &str) -> anyhow::Result<u64> {
    let s = s.trim();
    let (num_str, unit) = if s.ends_with('G') || s.ends_with('g') {
        (&s[..s.len() - 1], 1024 * 1024 * 1024u64)
    } else if s.ends_with('M') || s.ends_with('m') {
        (&s[..s.len() - 1], 1024 * 1024)
    } else if s.ends_with('K') || s.ends_with('k') {
        (&s[..s.len() - 1], 1024)
    } else {
        (s, 1u64)
    };
    let num: u64 = num_str
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid sandbox size: {}", s))?;
    Ok(num * unit)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SandboxProcessConfig {
    #[serde(default)]
    pub allow_fork: Option<bool>,
    #[serde(default)]
    pub max_processes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WindowsSandboxConfig {
    #[serde(default)]
    pub r#type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppSandboxConfig {
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub windows: Option<WindowsSandboxConfig>,
    #[serde(default)]
    pub filesystem: Option<SandboxFilesystemConfig>,
    #[serde(default)]
    pub network: Option<SandboxNetworkConfig>,
    #[serde(default)]
    pub resources: Option<SandboxResourceConfig>,
    #[serde(default)]
    pub process: Option<SandboxProcessConfig>,
    /// Working directory for the child process in app sandbox. Defaults to `"~"` (home).
    /// config_dir() uses home.join(".synbot") or, if home fails, ".".join(".synbot"); so cwd must be home, not ~/.synbot.
    #[serde(default)]
    pub work_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolSandboxConfig {
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub filesystem: Option<SandboxFilesystemConfig>,
    #[serde(default)]
    pub network: Option<SandboxNetworkConfig>,
    #[serde(default)]
    pub resources: Option<SandboxResourceConfig>,
    #[serde(default)]
    pub process: Option<SandboxProcessConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SandboxLogOutputConfig {
    #[serde(rename = "type")]
    pub output_type: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub rotation: Option<String>,
    #[serde(default)]
    pub max_size: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SandboxMonitoringConfig {
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default)]
    pub log_output: Vec<SandboxLogOutputConfig>,
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
    /// Heartbeat: periodic tasks (enabled, interval, tasks with channel/userId/target).
    #[serde(default)]
    pub heartbeat: HeartbeatConfig,
    /// Cron: scheduled tasks from config (schedule, description, enabled, command, channel, userId).
    #[serde(default)]
    pub cron: CronConfig,
    /// Optional app sandbox (isolates main process; platform-specific).
    #[serde(default)]
    pub app_sandbox: Option<AppSandboxConfig>,
    /// Optional tool sandbox (exec and other tools run inside this).
    #[serde(default)]
    pub tool_sandbox: Option<ToolSandboxConfig>,
    /// Optional sandbox monitoring (log_level, log_output for sandbox audit).
    #[serde(default)]
    pub sandbox_monitoring: Option<SandboxMonitoringConfig>,
}

/// Expand paths that start with "~" to the user's home directory.
fn expand_sandbox_paths(paths: &[String]) -> Vec<String> {
    let home = dirs::home_dir();
    paths
        .iter()
        .map(|p| {
            let s = p.trim();
            if s.starts_with("~/") || s == "~" {
                home.as_ref()
                    .map(|h| h.join(s.trim_start_matches('~').trim_start_matches('/')).display().to_string())
                    .unwrap_or_else(|| p.to_string())
            } else if s.starts_with("~\\") || (cfg!(windows) && s.starts_with('~')) {
                home.as_ref()
                    .map(|h| {
                        let rest = s.trim_start_matches('~').trim_start_matches('\\');
                        h.join(rest).display().to_string()
                    })
                    .unwrap_or_else(|| p.to_string())
            } else {
                p.to_string()
            }
        })
        .collect()
}

fn build_sandbox_monitoring(mon: &Option<SandboxMonitoringConfig>) -> crate::sandbox::types::MonitoringConfig {
    match mon {
        None => crate::sandbox::types::MonitoringConfig::default(),
        Some(m) => crate::sandbox::types::MonitoringConfig {
            log_level: m.log_level.clone().unwrap_or_else(|| "info".to_string()),
            log_output: m
                .log_output
                .iter()
                .map(|o| crate::sandbox::types::LogOutput {
                    output_type: o.output_type.clone(),
                    path: o.path.clone(),
                    facility: String::new(),
                })
                .collect(),
            audit: crate::sandbox::types::AuditConfig::default(),
            metrics: crate::sandbox::types::MetricsConfig::default(),
        },
    }
}

/// Build SandboxConfig for app sandbox from Config.
pub fn build_app_sandbox_config(
    cfg: &AppSandboxConfig,
    monitoring: &Option<SandboxMonitoringConfig>,
) -> anyhow::Result<crate::sandbox::types::SandboxConfig> {
    let platform = cfg
        .platform
        .as_deref()
        .unwrap_or("auto")
        .to_string();
    let fs = cfg.filesystem.as_ref().map(|f| SandboxFilesystemConfig {
        readonly_paths: f.readonly_paths.clone(),
        writable_paths: f.writable_paths.clone(),
        hidden_paths: f.hidden_paths.clone(),
    }).unwrap_or_default();
    let net = cfg
        .network
        .as_ref()
        .cloned()
        .unwrap_or_default();
    let res = cfg.resources.as_ref();
    let max_memory = res
        .and_then(|r| r.max_memory.as_ref())
        .map(|v| parse_sandbox_size_bytes(v))
        .transpose()?
        .unwrap_or(2 * 1024 * 1024 * 1024);
    let max_cpu = res
        .and_then(|r| r.max_cpu)
        .unwrap_or(2.0);
    let max_disk = res
        .and_then(|r| r.max_disk.as_ref())
        .map(|v| parse_sandbox_size_bytes(v))
        .transpose()?
        .unwrap_or(10 * 1024 * 1024 * 1024);
    let process = cfg.process.as_ref();
    // Default workDir to "~" so child cwd is home and config_dir() resolves to ~/.synbot (see comment in sandbox spawn).
    let work_dir_raw = cfg.work_dir.as_deref().unwrap_or("~");
    let child_work_dir = expand_sandbox_paths(&[work_dir_raw.to_string()]).into_iter().next();
    Ok(crate::sandbox::types::SandboxConfig {
        sandbox_id: "synbot-app".to_string(),
        platform,
        filesystem: crate::sandbox::types::FilesystemConfig {
            readonly_paths: expand_sandbox_paths(&fs.readonly_paths),
            writable_paths: expand_sandbox_paths(&fs.writable_paths),
            hidden_paths: expand_sandbox_paths(&fs.hidden_paths),
        },
        network: crate::sandbox::types::NetworkConfig {
            enabled: net.enabled,
            allowed_hosts: net.allowed_hosts,
            allowed_ports: net.allowed_ports,
        },
        resources: crate::sandbox::types::ResourceConfig {
            max_memory,
            max_cpu,
            max_disk,
        },
        process: crate::sandbox::types::ProcessConfig {
            allow_fork: process.and_then(|p| p.allow_fork).unwrap_or(false),
            max_processes: process.and_then(|p| p.max_processes).unwrap_or(10),
        },
        child_work_dir,
        monitoring: build_sandbox_monitoring(monitoring),
    })
}

/// Build SandboxConfig for tool sandbox from Config.
pub fn build_tool_sandbox_config(
    cfg: &ToolSandboxConfig,
    monitoring: &Option<SandboxMonitoringConfig>,
) -> anyhow::Result<crate::sandbox::types::SandboxConfig> {
    let platform = "auto".to_string();
    let fs = cfg.filesystem.as_ref().map(|f| SandboxFilesystemConfig {
        readonly_paths: f.readonly_paths.clone(),
        writable_paths: f.writable_paths.clone(),
        hidden_paths: f.hidden_paths.clone(),
    }).unwrap_or_default();
    let net = cfg
        .network
        .as_ref()
        .cloned()
        .unwrap_or_default();
    let res = cfg.resources.as_ref();
    let max_memory = res
        .and_then(|r| r.max_memory.as_ref())
        .map(|v| parse_sandbox_size_bytes(v))
        .transpose()?
        .unwrap_or(1024 * 1024 * 1024);
    let max_cpu = res.and_then(|r| r.max_cpu).unwrap_or(1.0);
    let max_disk = res
        .and_then(|r| r.max_disk.as_ref())
        .map(|v| parse_sandbox_size_bytes(v))
        .transpose()?
        .unwrap_or(5 * 1024 * 1024 * 1024);
    let process = cfg.process.as_ref();
    Ok(crate::sandbox::types::SandboxConfig {
        sandbox_id: "synbot-tool".to_string(),
        platform,
        filesystem: crate::sandbox::types::FilesystemConfig {
            readonly_paths: expand_sandbox_paths(&fs.readonly_paths),
            writable_paths: expand_sandbox_paths(&fs.writable_paths),
            hidden_paths: expand_sandbox_paths(&fs.hidden_paths),
        },
        network: crate::sandbox::types::NetworkConfig {
            enabled: net.enabled,
            allowed_hosts: net.allowed_hosts,
            allowed_ports: net.allowed_ports,
        },
        resources: crate::sandbox::types::ResourceConfig {
            max_memory,
            max_cpu,
            max_disk,
        },
        process: crate::sandbox::types::ProcessConfig {
            allow_fork: process.and_then(|p| p.allow_fork).unwrap_or(false),
            max_processes: process.and_then(|p| p.max_processes).unwrap_or(5),
        },
        child_work_dir: None,
        monitoring: build_sandbox_monitoring(monitoring),
    })
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

    // --- Channel credentials and unique names ---
    let mut all_channel_names = std::collections::HashSet::new();
    for (i, c) in config.channels.telegram.iter().enumerate() {
        if c.enabled && c.token.is_empty() {
            errors.push(ValidationError {
                field: format!("channels.telegram[{}].token", i),
                value: String::new(),
                constraint: "must be non-empty when enabled".into(),
            });
        }
        if !c.name.is_empty() && !all_channel_names.insert(c.name.clone()) {
            errors.push(ValidationError {
                field: format!("channels.telegram[{}].name", i),
                value: c.name.clone(),
                constraint: "channel name must be globally unique".into(),
            });
        }
    }
    for (i, c) in config.channels.discord.iter().enumerate() {
        if c.enabled && c.token.is_empty() {
            errors.push(ValidationError {
                field: format!("channels.discord[{}].token", i),
                value: String::new(),
                constraint: "must be non-empty when enabled".into(),
            });
        }
        if !c.name.is_empty() && !all_channel_names.insert(c.name.clone()) {
            errors.push(ValidationError {
                field: format!("channels.discord[{}].name", i),
                value: c.name.clone(),
                constraint: "channel name must be globally unique".into(),
            });
        }
    }
    for (i, c) in config.channels.feishu.iter().enumerate() {
        if c.enabled {
            if c.app_id.is_empty() {
                errors.push(ValidationError {
                    field: format!("channels.feishu[{}].app_id", i),
                    value: String::new(),
                    constraint: "must be non-empty when enabled".into(),
                });
            }
            if c.app_secret.is_empty() {
                errors.push(ValidationError {
                    field: format!("channels.feishu[{}].app_secret", i),
                    value: String::new(),
                    constraint: "must be non-empty when enabled".into(),
                });
            }
        }
        if !c.name.is_empty() && !all_channel_names.insert(c.name.clone()) {
            errors.push(ValidationError {
                field: format!("channels.feishu[{}].name", i),
                value: c.name.clone(),
                constraint: "channel name must be globally unique".into(),
            });
        }
    }

    // --- Collect enabled channel names for main_channel validation ---
    let enabled_channels: Vec<String> = {
        let mut ch = Vec::new();
        for c in &config.channels.telegram {
            if c.enabled && !c.name.is_empty() {
                ch.push(c.name.clone());
            }
        }
        for c in &config.channels.discord {
            if c.enabled && !c.name.is_empty() {
                ch.push(c.name.clone());
            }
        }
        for c in &config.channels.feishu {
            if c.enabled && !c.name.is_empty() {
                ch.push(c.name.clone());
            }
        }
        ch
    };

    // --- main_channel validation ---
    let has_multi_agent_features = !config.agent.roles.is_empty();

    if has_multi_agent_features {
        if config.main_channel.is_empty() {
            errors.push(ValidationError {
                field: "main_channel".into(),
                value: String::new(),
                constraint: "must be non-empty when roles are configured".into(),
            });
        } else if !enabled_channels.contains(&config.main_channel) {
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

/// Fixed path: role template directory; onboard writes from templates/roles here.
pub fn roles_dir() -> PathBuf {
    config_dir().join("roles")
}

/// Application skills directory: `~/.synbot/skills/`. Each skill is a subdirectory containing SKILL.md.
pub fn skills_dir() -> PathBuf {
    config_dir().join("skills")
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

/// Number of config backup slots: config.json.bak.1 .. config.json.bak.5.
const CONFIG_BACKUP_COUNT: u32 = 5;

/// Before overwriting config, backup the current file to one of config.json.bak.1 .. .bak.5.
/// Uses the first slot that doesn't exist; if all exist, overwrites the oldest by mtime.
fn backup_config_before_save(config_path: &Path) -> Result<()> {
    if !config_path.exists() {
        return Ok(());
    }
    let bak_path_for = |i: u32| {
        let name = config_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "config.json".to_string());
        config_path.parent().map_or_else(
            || PathBuf::from(format!("{}.bak.{}", name, i)),
            |parent| parent.join(format!("{}.bak.{}", name, i)),
        )
    };
    let mut slot_to_use: Option<u32> = None;
    for i in 1..=CONFIG_BACKUP_COUNT {
        if !bak_path_for(i).exists() {
            slot_to_use = Some(i);
            break;
        }
    }
    let slot = match slot_to_use {
        Some(s) => s,
        None => {
            let mut oldest: Option<(u32, std::time::SystemTime)> = None;
            for i in 1..=CONFIG_BACKUP_COUNT {
                let bak = bak_path_for(i);
                if let Ok(meta) = std::fs::metadata(&bak) {
                    if let Ok(mtime) = meta.modified() {
                        let use_this = oldest
                            .map(|(_, t)| mtime < t)
                            .unwrap_or(true);
                        if use_this {
                            oldest = Some((i, mtime));
                        }
                    }
                }
            }
            oldest.map(|(i, _)| i).unwrap_or(1)
        }
    };
    let bak_path = bak_path_for(slot);
    std::fs::copy(config_path, &bak_path)?;
    Ok(())
}

pub fn save_config(cfg: &Config, path: Option<&Path>) -> Result<()> {
    let p = path
        .map(PathBuf::from)
        .unwrap_or_else(config_path);

    backup_config_before_save(&p)?;

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
        cfg.channels.telegram = vec![TelegramConfig {
            name: "telegram".into(),
            enabled: true,
            token: String::new(),
            ..Default::default()
        }];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "channels.telegram[0].token"));
    }

    #[test]
    fn telegram_enabled_with_token_is_accepted() {
        let mut cfg = valid_config();
        cfg.channels.telegram = vec![TelegramConfig {
            name: "telegram".into(),
            enabled: true,
            token: "bot123:abc".into(),
            ..Default::default()
        }];
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn telegram_disabled_with_empty_token_is_accepted() {
        let mut cfg = valid_config();
        cfg.channels.telegram = vec![TelegramConfig {
            enabled: false,
            ..Default::default()
        }];
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn discord_enabled_with_empty_token_is_rejected() {
        let mut cfg = valid_config();
        cfg.channels.discord = vec![DiscordConfig {
            name: "discord".into(),
            enabled: true,
            token: String::new(),
            ..Default::default()
        }];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "channels.discord[0].token"));
    }

    #[test]
    fn discord_enabled_with_token_is_accepted() {
        let mut cfg = valid_config();
        cfg.channels.discord = vec![DiscordConfig {
            name: "discord".into(),
            enabled: true,
            token: "discord-token".into(),
            ..Default::default()
        }];
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn feishu_enabled_with_empty_credentials_is_rejected() {
        let mut cfg = valid_config();
        cfg.channels.feishu = vec![FeishuConfig {
            name: "feishu".into(),
            enabled: true,
            app_id: String::new(),
            app_secret: String::new(),
            ..Default::default()
        }];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "channels.feishu[0].app_id"));
        assert!(errors.iter().any(|e| e.field == "channels.feishu[0].app_secret"));
    }

    #[test]
    fn feishu_enabled_with_credentials_is_accepted() {
        let mut cfg = valid_config();
        cfg.channels.feishu = vec![FeishuConfig {
            name: "feishu".into(),
            enabled: true,
            app_id: "app-id".into(),
            app_secret: "app-secret".into(),
            ..Default::default()
        }];
        assert!(validate_config(&cfg).is_ok());
    }

    // --- Multiple errors collected at once ---

    #[test]
    fn multiple_errors_are_collected() {
        let mut cfg = valid_config();
        cfg.agent.max_tokens = 0;
        cfg.agent.temperature = 5.0;
        cfg.tools.exec.timeout_secs = 0;
        cfg.channels.telegram = vec![TelegramConfig {
            name: "telegram".into(),
            enabled: true,
            token: String::new(),
            ..Default::default()
        }];

        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.len() >= 4, "expected at least 4 errors, got {}", errors.len());
        assert!(find_error(&errors, "agent.max_tokens").is_some());
        assert!(find_error(&errors, "agent.temperature").is_some());
        assert!(find_error(&errors, "tools.exec.timeout_secs").is_some());
        assert!(errors.iter().any(|e| e.field == "channels.telegram[0].token"));
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
        cfg.channels.telegram = vec![TelegramConfig {
            name: "telegram".into(),
            enabled: true,
            token: "bot123:abc".into(),
            ..Default::default()
        }];
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

