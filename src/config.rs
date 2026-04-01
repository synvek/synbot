use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::SystemTime;

// ---------------------------------------------------------------------------
// Channel configs
// ---------------------------------------------------------------------------

/// Single allowlist entry: one chat (DM or group) allowed to talk to the bot.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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

/// Approved pairing: grants access for one chat (identified by pairing code derived from chat id) for a channel provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct PairingEntry {
    /// Channel provider name (registry key): feishu, discord, telegram, …
    pub channel: String,
    /// First 12 hex chars of MD5(chat_id), case-insensitive when matched.
    #[serde(rename = "pairingCode")]
    pub pairing_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// Bot username for group @-mention gate: used when allowlist is off, when a chat is
    /// paired-only (not on allowlist), and as fallback for allowlisted groups without per-entry `myName`.
    #[serde(default)]
    pub group_my_name: Option<String>,
    pub proxy: Option<String>,
    /// When true (default), push tool execution progress to this channel.
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
    /// Agent to use for this channel (e.g. "main", "dev"). Default "main".
    #[serde(default = "default_channel_agent")]
    pub default_agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// Agent to use for this channel (e.g. "main", "dev"). Default "main".
    #[serde(default = "default_channel_agent")]
    pub default_agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// Agent to use for this channel (e.g. "main", "dev"). Default "main".
    #[serde(default = "default_channel_agent")]
    pub default_agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct SlackConfig {
    /// Unique channel name (default "slack").
    #[serde(default = "default_slack_name")]
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    /// Bot token (xoxb-...) for Web API (e.g. chat.postMessage). Required for sending messages.
    #[serde(default)]
    pub token: String,
    /// App-level token (xapp-...) for Socket Mode connection. Required for receiving events.
    #[serde(default)]
    pub app_token: String,
    #[serde(default)]
    pub allowlist: Vec<AllowlistEntry>,
    /// When true (default), only chats in allowlist are accepted; when false, allowlist is not checked.
    #[serde(default = "default_true")]
    pub enable_allowlist: bool,
    /// When enable_allowlist is false, bot user id used for channel @ check (optional).
    #[serde(default)]
    pub group_my_name: Option<String>,
    /// When true (default), push tool execution progress to this channel.
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
    /// Agent to use for this channel (e.g. "main", "dev"). Default "main".
    #[serde(default = "default_channel_agent")]
    pub default_agent: String,
}

fn default_telegram_name() -> String {
    "telegram".into()
}

fn default_channel_agent() -> String {
    "main".into()
}
fn default_discord_name() -> String {
    "discord".into()
}
fn default_feishu_name() -> String {
    "feishu".into()
}
fn default_slack_name() -> String {
    "slack".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct MatrixConfig {
    /// Unique channel name (default "matrix").
    #[serde(default = "default_matrix_name")]
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    /// Matrix homeserver URL (e.g. "https://matrix.example.org"). Required when enabled.
    #[serde(default)]
    pub homeserver_url: String,
    /// Bot user ID (e.g. "@bot:example.org") or localpart for login.
    #[serde(default)]
    pub username: String,
    /// Password for login. Ignored if access_token is set.
    #[serde(default)]
    pub password: String,
    /// Access token (optional). When set, login is skipped and this token is used.
    #[serde(default)]
    pub access_token: Option<String>,
    /// SQLite store path for Matrix sync state and Olm/Megolm keys (E2EE). Tilde `~` is expanded.
    /// When empty, defaults to `{sessions_dir}/matrix/{channelName}/store.sqlite` (see `sessions_root()`).
    /// The store is tied to user + device id; synbot uses a fixed device id `SYNBOT` for password login. Delete this file if you see a crypto store / device mismatch after upgrading.
    #[serde(default)]
    pub store_path: String,
    /// For Matrix, `chatId` must be the internal **room id** (`!xx:server`, from Element → Room settings → Advanced) or a user **MXID** — not a room alias (`#name:server`). Matching ignores surrounding ASCII spaces and letter case.
    #[serde(default)]
    pub allowlist: Vec<AllowlistEntry>,
    /// When true, only `allowlist` entries and CLI pairings are accepted; an empty allowlist denies everyone.
    #[serde(default = "default_true")]
    pub enable_allowlist: bool,
    /// When enable_allowlist is false, bot user id for @mention check in rooms (optional).
    #[serde(default)]
    pub group_my_name: Option<String>,
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
    /// Agent to use for this channel (e.g. "main", "dev"). Default "main".
    #[serde(default = "default_channel_agent")]
    pub default_agent: String,
}

fn default_matrix_name() -> String {
    "matrix".into()
}

/// DingTalk Stream robot — clientId/clientSecret from open platform; receives via Stream CALLBACK, replies via sessionWebhook.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct DingTalkConfig {
    /// Unique channel name (default "dingtalk").
    #[serde(default = "default_dingtalk_name")]
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    /// OAuth ClientID (formerly AppKey). Use this or appKey.
    #[serde(default)]
    pub client_id: String,
    /// OAuth ClientSecret (formerly AppSecret). Use this or appSecret.
    #[serde(default)]
    pub client_secret: String,
    /// Alias for clientId (console may show "AppKey"). Ignored if clientId is set.
    #[serde(default)]
    pub app_key: Option<String>,
    /// Alias for clientSecret (console may show "AppSecret"). Ignored if clientSecret is set.
    #[serde(default)]
    pub app_secret: Option<String>,
    #[serde(default)]
    pub allowlist: Vec<AllowlistEntry>,
    /// When true (default), only allowlist chats are accepted.
    #[serde(default = "default_true")]
    pub enable_allowlist: bool,
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
    /// Agent to use for this channel (e.g. "main", "dev"). Default "main".
    #[serde(default = "default_channel_agent")]
    pub default_agent: String,
    /// Robot code from open platform (机器人 → robotCode). Required for file/image download API if callback omits it.
    #[serde(default)]
    pub robot_code: String,
}

fn default_dingtalk_name() -> String {
    "dingtalk".into()
}

/// Email account and transport settings (IMAP receive, SMTP send).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EmailServerConfig {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    /// Use TLS (e.g. IMAPS 993, SMTPS 465). When false, use STARTTLS on plain port.
    #[serde(default = "default_true")]
    pub use_tls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct EmailConfig {
    /// Unique channel name (default "email").
    #[serde(default = "default_email_name")]
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    /// IMAP receive server (host, port, username, password).
    #[serde(default)]
    pub imap: EmailServerConfig,
    /// SMTP send server (host, port, username, password).
    #[serde(default)]
    pub smtp: EmailServerConfig,
    /// Only treat emails FROM this address as chat (e.g. "user@example.com").
    #[serde(default)]
    pub from_sender: String,
    /// Only process emails received after this time (RFC3339 or date-only "YYYY-MM-DD").
    #[serde(default)]
    pub start_time: String,
    /// Poll interval in seconds (default 120 = 2 minutes).
    #[serde(default = "default_email_poll_interval_secs")]
    pub poll_interval_secs: u64,
    /// When true (default), push tool execution progress to this channel.
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
    /// Agent to use for this channel (e.g. "main", "dev"). Default "main".
    #[serde(default = "default_channel_agent")]
    pub default_agent: String,
}

fn default_email_name() -> String {
    "email".into()
}

fn default_email_poll_interval_secs() -> u64 {
    120
}

/// WhatsApp channel configuration (WhatsApp Web multi-device via wa-rs).
///
/// Link the account with QR or pair code; session is persisted under `session_dir`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct WhatsAppConfig {
    /// Whether this WhatsApp channel is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Unique channel name (default "whatsapp").
    #[serde(default = "default_whatsapp_name")]
    pub name: String,
    /// Directory used to persist the wa-rs session (SQLite DB, creds, etc.).
    /// Example: "~/.synbot/sessions/whatsapp".
    #[serde(default)]
    pub session_dir: String,
    /// Allowlist of sender IDs / phone numbers allowed to interact with the bot.
    /// If empty, all senders are allowed.
    #[serde(default)]
    pub allowlist: Vec<AllowlistEntry>,
    /// Agent to use for this channel (e.g. "main", "dev"). Default "main".
    #[serde(default = "default_channel_agent")]
    pub agent: String,
}

fn default_whatsapp_name() -> String {
    "whatsapp".into()
}

/// IRC channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct IrcConfig {
    /// Whether this IRC channel is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Unique channel name (default "irc").
    #[serde(default = "default_irc_name")]
    pub name: String,
    /// IRC server hostname or IP address.
    pub server: Option<String>,
    /// IRC server port (default 6697 for TLS, 6667 for plain).
    #[serde(default = "default_irc_port")]
    pub port: u16,
    /// Bot nickname on the IRC server.
    pub nickname: Option<String>,
    /// List of IRC channels to join (e.g. ["#general", "#dev"]).
    #[serde(default)]
    pub channels: Vec<String>,
    /// Whether to use TLS (default true).
    #[serde(default = "default_true")]
    pub use_tls: bool,
    /// NickServ password or SASL password for authentication.
    pub password: Option<String>,
    /// Allowlist entries: **channel** messages match `chatId` to the channel name (e.g. `#dev`);
    /// **direct messages** to the bot match `chatId` to the sender's **IRC nick** (e.g. `halloy1905`).
    #[serde(default)]
    pub allowlist: Vec<AllowlistEntry>,
    /// When true (default), only nicks in allowlist are accepted; when false, allowlist is not checked.
    #[serde(default = "default_true")]
    pub enable_allowlist: bool,
    /// Agent to use for this channel (e.g. "main", "dev"). Default "main".
    #[serde(default = "default_channel_agent")]
    pub agent: String,
}

fn default_irc_name() -> String {
    "irc".into()
}

fn default_irc_port() -> u16 {
    6697
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ChannelsConfig {
    #[serde(default)]
    pub telegram: Vec<TelegramConfig>,
    #[serde(default)]
    pub discord: Vec<DiscordConfig>,
    #[serde(default)]
    pub feishu: Vec<FeishuConfig>,
    #[serde(default)]
    pub slack: Vec<SlackConfig>,
    #[serde(default)]
    pub email: Vec<EmailConfig>,
    #[serde(default)]
    pub matrix: Vec<MatrixConfig>,
    #[serde(default)]
    pub dingtalk: Vec<DingTalkConfig>,
    /// WhatsApp channels (WhatsApp Web multi-device via wa-rs).
    /// `whatsappPersonal` is accepted as a legacy config key (same shape).
    #[serde(default, alias = "whatsappPersonal")]
    pub whatsapp: Option<Vec<WhatsAppConfig>>,
    /// IRC channels.
    #[serde(default)]
    pub irc: Option<Vec<IrcConfig>>,
}

impl ChannelsConfig {
    /// Return channel entries for the registry: (type_name, list of config as Value).
    /// Used by [crate::channels::ChannelRegistry] to spawn channels from config.
    pub fn channel_entries(&self) -> Vec<(String, Vec<serde_json::Value>)> {
        let mut out = Vec::new();
        let telegram: Vec<serde_json::Value> = self
            .telegram
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect();
        if !telegram.is_empty() {
            out.push(("telegram".to_string(), telegram));
        }
        let feishu: Vec<serde_json::Value> = self
            .feishu
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect();
        if !feishu.is_empty() {
            out.push(("feishu".to_string(), feishu));
        }
        let discord: Vec<serde_json::Value> = self
            .discord
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect();
        if !discord.is_empty() {
            out.push(("discord".to_string(), discord));
        }
        let slack: Vec<serde_json::Value> = self
            .slack
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect();
        if !slack.is_empty() {
            out.push(("slack".to_string(), slack));
        }
        let email: Vec<serde_json::Value> = self
            .email
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect();
        if !email.is_empty() {
            out.push(("email".to_string(), email));
        }
        let matrix: Vec<serde_json::Value> = self
            .matrix
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect();
        if !matrix.is_empty() {
            out.push(("matrix".to_string(), matrix));
        }
        let dingtalk: Vec<serde_json::Value> = self
            .dingtalk
            .iter()
            .map(|c| serde_json::to_value(c).unwrap_or_default())
            .collect();
        if !dingtalk.is_empty() {
            out.push(("dingtalk".to_string(), dingtalk));
        }
        if let Some(whatsapp_list) = &self.whatsapp {
            let whatsapp: Vec<serde_json::Value> = whatsapp_list
                .iter()
                .map(|c| serde_json::to_value(c).unwrap_or_default())
                .collect();
            if !whatsapp.is_empty() {
                out.push(("whatsapp".to_string(), whatsapp));
            }
        }
        if let Some(irc_list) = &self.irc {
            let irc: Vec<serde_json::Value> = irc_list
                .iter()
                .map(|c| serde_json::to_value(c).unwrap_or_default())
                .collect();
            if !irc.is_empty() {
                out.push(("irc".to_string(), irc));
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Provider configs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ProviderEntry {
    #[serde(default)]
    pub api_key: String,
    pub api_base: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ProvidersConfig {
    #[serde(default)]
    pub anthropic: ProviderEntry,
    #[serde(default)]
    pub openai: ProviderEntry,
    #[serde(default)]
    pub gemini: ProviderEntry,
    #[serde(default)]
    pub openrouter: ProviderEntry,
    #[serde(default)]
    pub deepseek: ProviderEntry,
    #[serde(default)]
    pub moonshot: ProviderEntry,
    /// Kimi Code (Moonshot coding API). Uses OpenAI-compatible chat completions; set api_base if using a custom endpoint.
    #[serde(default)]
    pub kimi_code: ProviderEntry,
    #[serde(default)]
    pub ollama: ProviderEntry,
    /// Extra provider entries for plugin-registered providers. Key = provider name as used in config (e.g. mainAgent.provider).
    #[serde(default)]
    pub extra: std::collections::HashMap<String, ProviderEntry>,
}

// ---------------------------------------------------------------------------
// Agent config (runtime entity that references one role)
// ---------------------------------------------------------------------------

/// Agent configuration: runtime entity that references exactly one role.
/// Role is discovered from ~/.synbot/roles/ (subdir name = role name). All agents use the same workspace from MainAgent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub name: String,
    /// Role name (must match a subdir under ~/.synbot/roles/, e.g. main, dev).
    pub role: String,
    #[serde(default)]
    pub provider: Option<String>,
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub max_iterations: Option<u32>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
}

// ---------------------------------------------------------------------------
// Main agent config (workspace and defaults; agents reference roles from filesystem)
// ---------------------------------------------------------------------------

/// Additional agents only; the main agent is implicit (role "main", from MainAgent settings).
fn default_agents() -> Vec<AgentConfig> {
    vec![AgentConfig {
        name: "dev".to_string(),
        role: "dev".to_string(),
        provider: None,
        model: None,
        max_tokens: None,
        temperature: None,
        max_iterations: None,
        skills: Vec::new(),
        tools: Vec::new(),
    }]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct MainAgent {
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
    /// Maximum consecutive tool failures before stopping. Default 8.
    #[serde(default = "default_max_consecutive_tool_errors")]
    pub max_consecutive_tool_errors: u32,
    /// Maximum number of chat history messages to send to the model (most recent N). Default 20.
    #[serde(default = "default_max_chat_history_messages")]
    pub max_chat_history_messages: u32,
    #[serde(default = "default_max_concurrent_subagents")]
    pub max_concurrent_subagents: usize,
    /// Timeout in seconds for each subagent/directive task. When exceeded, the task is marked failed and the slot is freed. Default 600 (10 min).
    #[serde(default = "default_subagent_task_timeout_secs")]
    pub subagent_task_timeout_secs: u64,
    #[serde(default = "default_agents")]
    pub agents: Vec<AgentConfig>,
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
    99
}
fn default_max_consecutive_tool_errors() -> u32 {
    8
}
fn default_max_chat_history_messages() -> u32 {
    20
}
fn default_max_concurrent_subagents() -> usize {
    5
}
fn default_subagent_task_timeout_secs() -> u64 {
    600
}

impl Default for MainAgent {
    fn default() -> Self {
        Self {
            workspace: default_workspace(),
            provider: default_provider(),
            model: default_model(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            max_tool_iterations: default_max_iterations(),
            max_consecutive_tool_errors: default_max_consecutive_tool_errors(),
            max_chat_history_messages: default_max_chat_history_messages(),
            max_concurrent_subagents: default_max_concurrent_subagents(),
            subagent_task_timeout_secs: default_subagent_task_timeout_secs(),
            agents: default_agents(),
        }
    }
}

// ---------------------------------------------------------------------------
// Permission config
// ---------------------------------------------------------------------------

use crate::tools::permission::{PermissionLevel, PermissionRule};

/// Permission configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct PermissionConfig {
    /// Whether permission control is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Default permission level when no rule matches.
    #[serde(default = "default_permission_level")]
    pub default_level: PermissionLevel,
    /// Approval request timeout (seconds). Default 300 (5 minutes).
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
    300 // 5 minutes
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    300
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum WebSearchBackend {
    /// DuckDuckGo HTML scraping — no API key required (default).
    #[default]
    DuckDuckGo,
    /// SearxNG self-hosted instance — requires `searxng_url`.
    SearxNG,
    /// Brave Search API — requires `brave_api_key`.
    Brave,
    /// Tavily Search API — requires `tavily_api_key` (https://tavily.com).
    Tavily,
    /// Firecrawl Search API — requires `firecrawl_api_key` (https://firecrawl.dev).
    Firecrawl,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct WebToolConfig {
    /// Legacy Brave API key (kept for backwards compatibility; sets backend=brave when non-empty).
    #[serde(default)]
    pub brave_api_key: String,

    /// Tavily Search API key (used when backend == Tavily). Get one at https://app.tavily.com/
    #[serde(default)]
    pub tavily_api_key: String,

    /// Active search backend. Defaults to duckDuckGo when not set.
    #[serde(default)]
    pub search_backend: WebSearchBackend,

    /// SearxNG instance base URL, e.g. "https://searx.example.com".
    #[serde(default)]
    pub searxng_url: String,

    /// Firecrawl API key (used when backend == Firecrawl). Get one at https://firecrawl.dev
    #[serde(default)]
    pub firecrawl_api_key: String,

    /// Maximum results to return (default 5).
    #[serde(default = "default_search_count")]
    pub search_count: u32,
}

// ---------------------------------------------------------------------------
// Log config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct WebAuthConfig {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
// Browser tool config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct BrowserToolConfig {
    /// Enable the browser tool (default true; requires agent-browser to be installed).
    #[serde(default = "default_browser_enabled")]
    pub enabled: bool,
    /// Path or name of the agent-browser executable (default "agent-browser").
    #[serde(default = "default_browser_executable")]
    pub executable: String,
    /// Per-command timeout in seconds (default 30).
    #[serde(default = "default_browser_timeout")]
    pub timeout_secs: u64,
}

fn default_browser_enabled() -> bool {
    true
}

fn default_browser_executable() -> String {
    "agent-browser".to_string()
}

fn default_browser_timeout() -> u64 {
    30
}

impl Default for BrowserToolConfig {
    fn default() -> Self {
        Self {
            enabled: default_browser_enabled(),
            executable: default_browser_executable(),
            timeout_secs: default_browser_timeout(),
        }
    }
}

// ---------------------------------------------------------------------------
// Generation tools config (image, video, speech)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ImageGenConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Provider name (e.g. "openai" or a key in providers.extra).
    #[serde(default)]
    pub provider: String,
    /// Output directory relative to workspace (e.g. "generated/images").
    #[serde(default = "default_generation_output_dir_image")]
    pub output_dir: String,
    /// Model (e.g. "dall-e-3", "gpt-image-1").
    #[serde(default = "default_image_model")]
    pub model: String,
    /// Size (e.g. "1024x1024", "1792x1024" for dall-e-3).
    #[serde(default = "default_image_size")]
    pub size: String,
    /// Quality: "standard" or "hd" (dall-e-3).
    #[serde(default = "default_image_quality")]
    pub quality: String,
}

fn default_generation_output_dir_image() -> String {
    "generated/images".to_string()
}
fn default_image_model() -> String {
    "dall-e-3".to_string()
}
fn default_image_size() -> String {
    "1024x1024".to_string()
}
fn default_image_quality() -> String {
    "standard".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct VideoGenConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: String,
    #[serde(default = "default_generation_output_dir_video")]
    pub output_dir: String,
    #[serde(default)]
    pub model: String,
}

fn default_generation_output_dir_video() -> String {
    "generated/video".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct SpeechGenConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: String,
    #[serde(default = "default_generation_output_dir_speech")]
    pub output_dir: String,
    #[serde(default = "default_speech_model")]
    pub model: String,
    /// Voice (e.g. "alloy", "echo", "fable", "onyx", "nova", "shimmer" for OpenAI TTS).
    #[serde(default = "default_speech_voice")]
    pub voice: String,
    /// Output format: "mp3", "opus", "aac", "flac".
    #[serde(default = "default_speech_format")]
    pub format: String,
}

fn default_generation_output_dir_speech() -> String {
    "generated/speech".to_string()
}
fn default_speech_model() -> String {
    "tts-1".to_string()
}
fn default_speech_voice() -> String {
    "alloy".to_string()
}
fn default_speech_format() -> String {
    "mp3".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    #[serde(default)]
    pub image: ImageGenConfig,
    #[serde(default)]
    pub video: VideoGenConfig,
    #[serde(default)]
    pub speech: SpeechGenConfig,
}

// ---------------------------------------------------------------------------
// Tools config
// ---------------------------------------------------------------------------

/// MCP server transport: stdio (subprocess) or SSE (HTTP).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    #[default]
    Stdio,
    Sse,
}

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    /// Unique id for this server (used in logs and optional tool name prefix).
    pub id: String,
    /// Transport type: "stdio" or "sse".
    #[serde(default)]
    pub transport: McpTransport,
    /// For stdio: command to run (e.g. "npx", "uvx").
    #[serde(default)]
    pub command: String,
    /// For stdio: arguments (e.g. ["-y", "mcp-server-filesystem"]).
    #[serde(default)]
    pub args: Vec<String>,
    /// For SSE: server URL (e.g. "http://localhost:8000/sse").
    #[serde(default)]
    pub url: String,
    /// Optional prefix for registered tool names to avoid collisions (e.g. "mcp_fs_").
    #[serde(default)]
    pub tool_name_prefix: Option<String>,
}

/// MCP (Model Context Protocol) tools configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ToolsConfig {
    #[serde(default)]
    pub exec: ExecToolConfig,
    #[serde(default)]
    pub web: WebToolConfig,
    #[serde(default)]
    pub browser: BrowserToolConfig,
    /// Image, video, and speech generation tools (provider + output_dir per type).
    #[serde(default)]
    pub generation: GenerationConfig,
    /// MCP servers to connect; their tools are registered as synbot tools.
    #[serde(default)]
    pub mcp: Option<McpConfig>,
}

// ---------------------------------------------------------------------------
// Heartbeat config
// ---------------------------------------------------------------------------

/// A single heartbeat task: run periodically and send result to the given channel/chat.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct CronConfig {
    #[serde(default)]
    pub tasks: Vec<CronTaskConfig>,
}

/// TurboWorkflow config: persistent resumable workflows.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct WorkflowConfig {
    /// Timeout in seconds when a step waits for user input (default 1800 = 30 min).
    #[serde(default = "default_workflow_user_input_timeout")]
    pub user_input_timeout_secs: u64,
    /// Root directory for workflow state files. When empty, uses config_dir/workflows.
    #[serde(default)]
    pub workflows_root: Option<String>,
}

fn default_workflow_user_input_timeout() -> u64 {
    1800 // 30 minutes
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            user_input_timeout_secs: default_workflow_user_input_timeout(),
            workflows_root: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Sandbox config (app_sandbox / tool_sandbox / monitoring)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct SandboxFilesystemConfig {
    #[serde(default)]
    pub readonly_paths: Vec<String>,
    #[serde(default)]
    pub writable_paths: Vec<String>,
    #[serde(default)]
    pub hidden_paths: Vec<String>,
    /// When true (default), mount host skills dir into tool sandbox at /skills (read-only). When false, do not mount; exec in sandbox cannot access skills by path.
    #[serde(default)]
    pub mount_skills_dir: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct SandboxProcessConfig {
    #[serde(default)]
    pub allow_fork: Option<bool>,
    #[serde(default)]
    pub max_processes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct WindowsSandboxConfig {
    #[serde(default)]
    pub r#type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// Working directory for the child process in app sandbox. When omitted, defaults to the
    /// config root directory (`~/.synbot`, or `--root-dir`), not the whole home folder.
    #[serde(default)]
    pub work_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ToolSandboxConfig {
    /// Container name for the tool sandbox. Default "synbot-tool".
    #[serde(default)]
    pub sandbox_name: Option<String>,
    /// When true, remove existing container with the same name and create fresh on each start. When false (default), reuse existing container if found (start it if stopped).
    #[serde(default)]
    pub delete_on_start: Option<bool>,
    /// Tool sandbox backend: "gvisor-docker" (default), "plain-docker"; on Windows also "wsl2-gvisor" or host-native "appcontainer". On Linux/macOS host-native: "nono"; on macOS only: "seatbelt" (sandbox-exec). If the environment does not match, tool sandbox creation fails; pick an available type.
    #[serde(default)]
    pub sandbox_type: Option<String>,
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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

fn default_tool_result_preview_chars() -> u32 {
    2048
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// When true (default), all channels may receive tool execution progress; each channel can override via channels.*.showToolCalls.
    #[serde(default = "default_true")]
    pub show_tool_calls: bool,
    /// Max length (chars) of tool result preview sent to users (default 2048).
    #[serde(default = "default_tool_result_preview_chars")]
    pub tool_result_preview_chars: u32,
    #[serde(default)]
    pub channels: ChannelsConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default, rename = "mainAgent")]
    pub main_agent: MainAgent,
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
    /// TurboWorkflow: persistent resumable workflows (user_input_timeout_secs, workflows_root).
    #[serde(default)]
    pub workflow: WorkflowConfig,
    /// Optional app sandbox (isolates main process; platform-specific).
    #[serde(default)]
    pub app_sandbox: Option<AppSandboxConfig>,
    /// Optional tool sandbox (exec and other tools run inside this).
    #[serde(default)]
    pub tool_sandbox: Option<ToolSandboxConfig>,
    /// Optional sandbox monitoring (log_level, log_output for sandbox audit).
    #[serde(default)]
    pub sandbox_monitoring: Option<SandboxMonitoringConfig>,
    /// Plugin-specific configuration. Keys are plugin names; values are arbitrary JSON for each plugin.
    #[serde(default)]
    pub plugins: std::collections::HashMap<String, serde_json::Value>,
    /// Pairing approvals: supplement allowlist; matched by channel provider + MD5(chat_id) prefix.
    #[serde(default)]
    pub pairings: Vec<PairingEntry>,
    /// Config file format version. Used by ConfigMigrator to apply incremental migrations.
    #[serde(default = "default_config_version")]
    pub config_version: u32,
}

fn default_config_version() -> u32 {
    1
}

/// Generates the JSON Schema for the root config. Available only when the `schema` feature is enabled.
#[cfg(feature = "schema")]
pub fn config_json_schema() -> schemars::schema::RootSchema {
    schemars::schema_for!(Config)
}

/// Strip Win32 verbatim prefix (`\\?\` / `\\?\UNC\`) so paths work as `CreateProcessW` cwd and for cmd.exe.
/// `std::fs::canonicalize` on Windows returns `\\?\C:\...`, which many shells reject as a working directory.
#[cfg(windows)]
fn strip_windows_verbatim_prefix(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    let Some(rest) = s.strip_prefix(r"\\?\") else {
        return path.to_path_buf();
    };
    if let Some(unc_tail) = rest.strip_prefix("UNC\\") {
        PathBuf::from(format!(r"\\{}", unc_tail))
    } else {
        PathBuf::from(rest.to_string())
    }
}

/// Collapse `.` / `..` in path strings and canonicalize when the path exists (fixes e.g. `C:\Users\x\.\.synbot`).
fn finalize_sandbox_path_string(expanded: String) -> String {
    let t = expanded.trim();
    if t.is_empty() {
        return expanded;
    }
    let cleaned: PathBuf = Path::new(t).components().collect();
    let resolved = std::fs::canonicalize(&cleaned).unwrap_or(cleaned);
    #[cfg(windows)]
    let resolved = strip_windows_verbatim_prefix(&resolved);
    let resolved = collapse_path_components(&resolved);
    resolved.to_string_lossy().into_owned()
}

/// Expand paths that start with "~" to the user's home directory.
fn expand_sandbox_paths(paths: &[String]) -> Vec<String> {
    let home = dirs::home_dir();
    paths
        .iter()
        .map(|p| {
            let s = p.trim();
            let expanded = if s.starts_with("~/") || s == "~" {
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
            };
            finalize_sandbox_path_string(expanded)
        })
        .collect()
}

/// Docker-backed tool sandboxes use `workspace_mount` / `skills_mount`; host-native backends use expanded paths in `writable_paths` / `readonly_paths`.
pub fn tool_sandbox_exec_kind(cfg: &ToolSandboxConfig) -> crate::sandbox::types::ToolSandboxExecKind {
    let s = cfg.sandbox_type.as_deref().unwrap_or("gvisor-docker");
    if tool_sandbox_backend_is_docker(s) {
        crate::sandbox::types::ToolSandboxExecKind::Docker
    } else {
        crate::sandbox::types::ToolSandboxExecKind::HostNative
    }
}

fn tool_sandbox_backend_is_docker(sandbox_type: &str) -> bool {
    matches!(
        sandbox_type,
        "gvisor-docker" | "plain-docker" | "wsl2-gvisor"
    )
}

fn sandbox_path_list_contains(paths: &[String], candidate: &str) -> bool {
    fn norm(s: &str) -> String {
        #[cfg(windows)]
        {
            s.replace('\\', "/").trim_end_matches('/').to_lowercase()
        }
        #[cfg(not(windows))]
        {
            s.trim_end_matches('/').to_string()
        }
    }
    let n = norm(candidate);
    paths.iter().any(|p| norm(p) == n)
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

/// Merge dirs the main synbot process always needs (config, workspace, logs, workflows).
/// Same idea as [`build_tool_sandbox_config`] for host-native backends: keeps `appSandbox.filesystem`
/// optional while AppContainer setup / nono still get correct path grants.
fn merge_implicit_app_sandbox_host_paths(full: &Config, writable: &mut Vec<String>) {
    let mut push_w = |pb: PathBuf| {
        let host = pb.to_string_lossy().to_string();
        let exp = expand_sandbox_paths(&[host])
            .into_iter()
            .next()
            .unwrap_or_else(|| pb.to_string_lossy().to_string());
        if !sandbox_path_list_contains(writable, &exp) {
            writable.push(exp);
        }
    };
    push_w(config_dir());
    push_w(effective_workspace_path(full));
    push_w(normalize_workspace_path(&log_dir_path(full)));
    push_w(normalize_workspace_path(&workflows_root(full)));
}

/// Build SandboxConfig for app sandbox from Config.
pub fn build_app_sandbox_config(
    cfg: &AppSandboxConfig,
    full: &Config,
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
        mount_skills_dir: f.mount_skills_dir,
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
    // Default cwd to `config_dir()` (~/.synbot) so AppContainer ACL matches the data root; using "~"
    // alone made setup grant `%USERPROFILE%` first and (with the old order) skipped inheritable ACL on `.synbot`.
    let child_work_dir = match cfg.work_dir.as_deref() {
        Some(w) => expand_sandbox_paths(&[w.to_string()])
            .into_iter()
            .next()
            .map(rewrite_legacy_synbot_path_string),
        None => {
            let p = config_dir();
            expand_sandbox_paths(&[p.to_string_lossy().to_string()])
                .into_iter()
                .next()
                .map(rewrite_legacy_synbot_path_string)
        }
    };
    let mut readonly_paths = rewrite_legacy_synbot_paths_for_root_override(expand_sandbox_paths(&fs.readonly_paths));
    let mut writable_paths = rewrite_legacy_synbot_paths_for_root_override(expand_sandbox_paths(&fs.writable_paths));
    merge_implicit_app_sandbox_host_paths(full, &mut writable_paths);
    Ok(crate::sandbox::types::SandboxConfig {
        sandbox_id: "synbot-app".to_string(),
        platform,
        filesystem: crate::sandbox::types::FilesystemConfig {
            readonly_paths,
            writable_paths,
            hidden_paths: rewrite_legacy_synbot_paths_for_root_override(expand_sandbox_paths(&fs.hidden_paths)),
            ..Default::default()
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
        delete_on_start: false,
        requested_tool_sandbox_type: None,
        image: None,
    })
}

/// Build SandboxConfig for tool sandbox from Config.
/// For Docker backends: `workspace_path` is bind-mounted at `/workspace` (exec cwd); skills at `/skills` when enabled.
/// For host-native backends (`appcontainer`, `nono`, `seatbelt`): workspace and skills are merged into `writable_paths` / `readonly_paths` on the host.
pub fn build_tool_sandbox_config(
    cfg: &ToolSandboxConfig,
    monitoring: &Option<SandboxMonitoringConfig>,
    workspace_path: &std::path::Path,
    skills_dir: &std::path::Path,
) -> anyhow::Result<crate::sandbox::types::SandboxConfig> {
    let platform = "auto".to_string();
    let tool_type = cfg.sandbox_type.as_deref().unwrap_or("gvisor-docker");
    let is_docker = tool_sandbox_backend_is_docker(tool_type);
    let fs = cfg.filesystem.as_ref().map(|f| SandboxFilesystemConfig {
        readonly_paths: f.readonly_paths.clone(),
        writable_paths: f.writable_paths.clone(),
        hidden_paths: f.hidden_paths.clone(),
        mount_skills_dir: f.mount_skills_dir,
    }).unwrap_or_default();
    let mount_skills = fs.mount_skills_dir != Some(false) && skills_dir.exists();
    let mut readonly_paths = rewrite_legacy_synbot_paths_for_root_override(expand_sandbox_paths(&fs.readonly_paths));
    let mut writable_paths = rewrite_legacy_synbot_paths_for_root_override(expand_sandbox_paths(&fs.writable_paths));
    let workspace_host = workspace_path.to_string_lossy().to_string();
    let workspace_expanded = rewrite_legacy_synbot_path_string(
        expand_sandbox_paths(&[workspace_host.clone()])
            .into_iter()
            .next()
            .unwrap_or_else(|| workspace_host.clone()),
    );

    if !is_docker {
        if !sandbox_path_list_contains(&writable_paths, &workspace_expanded) {
            writable_paths.push(workspace_expanded.clone());
        }
        if mount_skills {
            let sk = skills_dir.to_string_lossy().to_string();
            let sk_exp = rewrite_legacy_synbot_path_string(
                expand_sandbox_paths(&[sk])
                    .into_iter()
                    .next()
                    .unwrap(),
            );
            if !sandbox_path_list_contains(&readonly_paths, &sk_exp) {
                readonly_paths.push(sk_exp);
            }
        }
    }

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
    let sandbox_id = cfg
        .sandbox_name
        .as_deref()
        .unwrap_or("synbot-tool")
        .to_string();

    let (workspace_mount, skills_mount) = if is_docker {
        (
            Some((workspace_host, "/workspace".to_string())),
            if mount_skills {
                Some((
                    skills_dir.to_string_lossy().to_string(),
                    "/skills".to_string(),
                ))
            } else {
                None
            },
        )
    } else {
        (None, None)
    };

    let child_work_dir = if tool_type == "appcontainer" {
        Some(workspace_expanded)
    } else {
        None
    };

    Ok(crate::sandbox::types::SandboxConfig {
        sandbox_id,
        platform,
        filesystem: crate::sandbox::types::FilesystemConfig {
            readonly_paths,
            writable_paths,
            hidden_paths: rewrite_legacy_synbot_paths_for_root_override(expand_sandbox_paths(&fs.hidden_paths)),
            workspace_mount,
            skills_mount,
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
        child_work_dir,
        monitoring: build_sandbox_monitoring(monitoring),
        delete_on_start: cfg.delete_on_start.unwrap_or(false),
        requested_tool_sandbox_type: Some(tool_type.to_string()),
        image: cfg.image.clone(),
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
/// - `mainAgent.max_tokens > 0`
/// - `mainAgent.temperature` in `[0.0, 2.0]`
/// - `mainAgent.max_tool_iterations > 0`
/// - `tools.exec.timeout_secs > 0`
/// - Enabled channels must have non-empty credentials
pub fn validate_config(config: &Config) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    // --- Agent defaults ---
    if config.main_agent.max_tokens == 0 {
        errors.push(ValidationError {
            field: "mainAgent.max_tokens".into(),
            value: config.main_agent.max_tokens.to_string(),
            constraint: "must be greater than 0".into(),
        });
    }

    if config.main_agent.temperature < 0.0 || config.main_agent.temperature > 2.0 {
        errors.push(ValidationError {
            field: "mainAgent.temperature".into(),
            value: config.main_agent.temperature.to_string(),
            constraint: "must be between 0.0 and 2.0".into(),
        });
    }

    if config.main_agent.max_tool_iterations == 0 {
        errors.push(ValidationError {
            field: "mainAgent.max_tool_iterations".into(),
            value: config.main_agent.max_tool_iterations.to_string(),
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

    // --- MCP servers validation ---
    if let Some(ref mcp) = config.tools.mcp {
        for (i, server) in mcp.servers.iter().enumerate() {
            let prefix = format!("tools.mcp.servers[{}]", i);
            if server.id.trim().is_empty() {
                errors.push(ValidationError {
                    field: format!("{}.id", prefix),
                    value: server.id.clone(),
                    constraint: "must be non-empty".into(),
                });
            }
            match server.transport {
                McpTransport::Stdio => {
                    if server.command.trim().is_empty() {
                        errors.push(ValidationError {
                            field: format!("{}.command", prefix),
                            value: server.command.clone(),
                            constraint: "must be non-empty for stdio transport".into(),
                        });
                    }
                }
                McpTransport::Sse => {
                    if server.url.trim().is_empty() {
                        errors.push(ValidationError {
                            field: format!("{}.url", prefix),
                            value: server.url.clone(),
                            constraint: "must be non-empty for sse transport".into(),
                        });
                    }
                }
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
    for (i, c) in config.channels.slack.iter().enumerate() {
        if c.enabled {
            if c.token.is_empty() {
                errors.push(ValidationError {
                    field: format!("channels.slack[{}].token", i),
                    value: String::new(),
                    constraint: "must be non-empty when enabled (use Bot token xoxb-...)".into(),
                });
            }
            if c.app_token.is_empty() {
                errors.push(ValidationError {
                    field: format!("channels.slack[{}].appToken", i),
                    value: String::new(),
                    constraint: "must be non-empty when enabled (use App-level token xapp-... for Socket Mode)".into(),
                });
            }
        }
        if !c.name.is_empty() && !all_channel_names.insert(c.name.clone()) {
            errors.push(ValidationError {
                field: format!("channels.slack[{}].name", i),
                value: c.name.clone(),
                constraint: "channel name must be globally unique".into(),
            });
        }
    }
    for (i, c) in config.channels.email.iter().enumerate() {
        if c.enabled {
            if c.imap.host.is_empty() || c.imap.username.is_empty() || c.imap.password.is_empty() {
                errors.push(ValidationError {
                    field: format!("channels.email[{}].imap", i),
                    value: String::new(),
                    constraint: "host, username, password must be non-empty when enabled".into(),
                });
            }
            if c.smtp.host.is_empty() || c.smtp.username.is_empty() || c.smtp.password.is_empty() {
                errors.push(ValidationError {
                    field: format!("channels.email[{}].smtp", i),
                    value: String::new(),
                    constraint: "host, username, password must be non-empty when enabled".into(),
                });
            }
            if c.from_sender.is_empty() {
                errors.push(ValidationError {
                    field: format!("channels.email[{}].fromSender", i),
                    value: String::new(),
                    constraint: "must be non-empty when enabled (only emails from this address are processed)".into(),
                });
            }
            if c.poll_interval_secs == 0 {
                errors.push(ValidationError {
                    field: format!("channels.email[{}].pollIntervalSecs", i),
                    value: c.poll_interval_secs.to_string(),
                    constraint: "must be greater than 0".into(),
                });
            }
        }
        if !c.name.is_empty() && !all_channel_names.insert(c.name.clone()) {
            errors.push(ValidationError {
                field: format!("channels.email[{}].name", i),
                value: c.name.clone(),
                constraint: "channel name must be globally unique".into(),
            });
        }
    }
    for (i, c) in config.channels.matrix.iter().enumerate() {
        if c.enabled {
            if c.homeserver_url.trim().is_empty() {
                errors.push(ValidationError {
                    field: format!("channels.matrix[{}].homeserverUrl", i),
                    value: String::new(),
                    constraint: "must be non-empty when enabled".into(),
                });
            }
            let has_token = c.access_token.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false);
            if !has_token && c.password.is_empty() && c.username.trim().is_empty() {
                errors.push(ValidationError {
                    field: format!("channels.matrix[{}]", i),
                    value: String::new(),
                    constraint: "when enabled, set accessToken or both username and password".into(),
                });
            }
        }
        if !c.name.is_empty() && !all_channel_names.insert(c.name.clone()) {
            errors.push(ValidationError {
                field: format!("channels.matrix[{}].name", i),
                value: c.name.clone(),
                constraint: "channel name must be globally unique".into(),
            });
        }
    }
    for (i, c) in config.channels.dingtalk.iter().enumerate() {
        if c.enabled {
            let has_id = !c.client_id.trim().is_empty()
                || c.app_key
                    .as_deref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
            if !has_id {
                errors.push(ValidationError {
                    field: format!("channels.dingtalk[{}].clientId", i),
                    value: String::new(),
                    constraint: "must be non-empty when enabled (or set appKey)".into(),
                });
            }
            let has_secret = !c.client_secret.trim().is_empty()
                || c.app_secret
                    .as_deref()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
            if !has_secret {
                errors.push(ValidationError {
                    field: format!("channels.dingtalk[{}].clientSecret", i),
                    value: String::new(),
                    constraint: "must be non-empty when enabled (or set appSecret)".into(),
                });
            }
        }
        if !c.name.is_empty() && !all_channel_names.insert(c.name.clone()) {
            errors.push(ValidationError {
                field: format!("channels.dingtalk[{}].name", i),
                value: c.name.clone(),
                constraint: "channel name must be globally unique".into(),
            });
        }
    }
    if let Some(wa_list) = &config.channels.whatsapp {
        for (i, c) in wa_list.iter().enumerate() {
            if c.enabled && c.session_dir.trim().is_empty() {
                errors.push(ValidationError {
                    field: format!("channels.whatsapp[{}].sessionDir", i),
                    value: String::new(),
                    constraint: "must be non-empty when enabled (wa-rs session persistence)".into(),
                });
            }
            if !c.name.is_empty() && !all_channel_names.insert(c.name.clone()) {
                errors.push(ValidationError {
                    field: format!("channels.whatsapp[{}].name", i),
                    value: c.name.clone(),
                    constraint: "channel name must be globally unique".into(),
                });
            }
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
        for c in &config.channels.slack {
            if c.enabled && !c.name.is_empty() {
                ch.push(c.name.clone());
            }
        }
        for c in &config.channels.email {
            if c.enabled && !c.name.is_empty() {
                ch.push(c.name.clone());
            }
        }
        for c in &config.channels.matrix {
            if c.enabled && !c.name.is_empty() {
                ch.push(c.name.clone());
            }
        }
        for c in &config.channels.dingtalk {
            if c.enabled && !c.name.is_empty() {
                ch.push(c.name.clone());
            }
        }
        if let Some(list) = &config.channels.whatsapp {
            for c in list {
                if c.enabled && !c.name.is_empty() {
                    ch.push(c.name.clone());
                }
            }
        }
        if let Some(list) = &config.channels.irc {
            for c in list {
                if c.enabled && !c.name.is_empty() {
                    ch.push(c.name.clone());
                }
            }
        }
        ch
    };

    // --- main_channel validation ---
    let has_multi_agent_features = !config.main_agent.agents.is_empty();

    if has_multi_agent_features && !enabled_channels.is_empty() && config.main_channel.is_empty() {
        errors.push(ValidationError {
            field: "main_channel".into(),
            value: String::new(),
            constraint: "must be non-empty when multiple agents are configured".into(),
        });
    }
    if !config.main_channel.is_empty() && !enabled_channels.contains(&config.main_channel) {
        errors.push(ValidationError {
            field: "main_channel".into(),
            value: config.main_channel.clone(),
            constraint: format!(
                "must reference an enabled channel (available: {})",
                enabled_channels.join(", ")
            ),
        });
    }

    // --- Agent validation (main is implicit; agents list must not define "main") ---
    let mut seen_agent_names = std::collections::HashSet::new();
    for (i, agent) in config.main_agent.agents.iter().enumerate() {
        let agent_label = if agent.name.is_empty() {
            format!("mainAgent.agents[{}]", i)
        } else {
            format!("mainAgent.agents[{}] ({})", i, agent.name)
        };

        if agent.name.is_empty() {
            errors.push(ValidationError {
                field: format!("{}.name", agent_label),
                value: String::new(),
                constraint: "agent name must be non-empty".into(),
            });
        }
        if agent.name == "main" {
            errors.push(ValidationError {
                field: format!("{}.name", agent_label),
                value: "main".into(),
                constraint: "agent name must not be 'main' (main agent is implicit from mainAgent and uses role main)".into(),
            });
        }
        if !agent.name.is_empty()
            && !agent
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            errors.push(ValidationError {
                field: format!("{}.name", agent_label),
                value: agent.name.clone(),
                constraint: "agent name must contain only letters, digits, and underscores"
                    .into(),
            });
        }
        if agent.role.trim().is_empty() {
            errors.push(ValidationError {
                field: format!("{}.role", agent_label),
                value: agent.role.clone(),
                constraint: "agent role must be non-empty (must match a subdir under ~/.synbot/roles/)".into(),
            });
        }
        if !agent.role.is_empty()
            && !agent
                .role
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            errors.push(ValidationError {
                field: format!("{}.role", agent_label),
                value: agent.role.clone(),
                constraint: "agent role must contain only letters, digits, and underscores".into(),
            });
        }
        if !agent.name.is_empty() && !seen_agent_names.insert(agent.name.clone()) {
            errors.push(ValidationError {
                field: format!("{}.name", agent_label),
                value: agent.name.clone(),
                constraint: "duplicate agent name".into(),
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

/// Per-process root directory override (set from CLI --root-dir). When set, config_dir() returns this path
/// instead of ~/.synbot. Enables multiple synbot instances with different workspaces.
static ROOT_DIR_OVERRIDE: OnceLock<RwLock<Option<PathBuf>> > = OnceLock::new();

fn root_dir_guard() -> &'static RwLock<Option<PathBuf>> {
    ROOT_DIR_OVERRIDE.get_or_init(|| RwLock::new(None))
}

/// Set the root directory for this process (called from CLI when --root-dir is passed).
/// Should be called once at startup before any config path is used.
pub fn set_root_dir(path: Option<PathBuf>) {
    if let Ok(mut g) = root_dir_guard().write() {
        *g = path;
    }
}

/// Return the current root directory override, if any. Used by sandbox to pass --root-dir to child process.
pub fn get_root_dir_override() -> Option<PathBuf> {
    root_dir_guard().read().ok().and_then(|g| g.clone())
}

/// Root directory for this instance: config, roles, memory, sessions, etc. Default is ~/.synbot.
pub fn config_dir() -> PathBuf {
    root_dir_guard()
        .read()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".synbot")
        })
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

fn normalize_path_cmp_key(p: &std::path::Path) -> String {
    let mut s = p.to_string_lossy().replace('\\', "/").to_lowercase();
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

fn path_is_same_or_under(desc: &std::path::Path, root: &std::path::Path) -> bool {
    let rk = normalize_path_cmp_key(root);
    let dk = normalize_path_cmp_key(desc);
    if dk == rk {
        return true;
    }
    let prefix = format!("{}/", rk);
    dk.starts_with(&prefix)
}

/// Resolved workspace directory for this process.
///
/// When [`set_root_dir`] is in effect, a config that still points [`workspace_path`] at the legacy
/// `%USERPROFILE%\.synbot\...` tree would make `synbot sandbox setup` apply inheritable ACLs under
/// the user profile (very slow) and disagree with the instance root. In that case we use
/// [`config_dir`]`/workspace` instead.
pub fn effective_workspace_path(cfg: &Config) -> PathBuf {
    let root = config_dir();
    let ws_abs = normalize_workspace_path(&workspace_path(cfg));
    let Some(home) = dirs::home_dir() else {
        return ws_abs;
    };
    let legacy_synbot_tree = home.join(".synbot");
    if get_root_dir_override().is_some()
        && path_is_same_or_under(&ws_abs, &legacy_synbot_tree)
        && normalize_path_cmp_key(&ws_abs) != normalize_path_cmp_key(&root)
    {
        log::warn!(
            "mainAgent.workspace is under {} but --root-dir is {}; using {} for workspace and sandbox paths",
            legacy_synbot_tree.display(),
            root.display(),
            root.join("workspace").display()
        );
        normalize_workspace_path(&root.join("workspace"))
    } else {
        ws_abs
    }
}

/// When `--root-dir` is set, config may still list absolute paths under `%USERPROFILE%\.synbot\...`.
/// Remap those to the same relative path under [`config_dir`], so sandbox ACL setup matches the instance root.
fn rewrite_legacy_synbot_paths_for_root_override(paths: Vec<String>) -> Vec<String> {
    if get_root_dir_override().is_none() {
        return paths;
    }
    let Some(home) = dirs::home_dir() else {
        return paths;
    };
    let legacy_root = normalize_workspace_path(&home.join(".synbot"));
    let instance_root = normalize_workspace_path(&config_dir());
    if normalize_path_cmp_key(&legacy_root) == normalize_path_cmp_key(&instance_root) {
        return paths;
    }

    paths
        .into_iter()
        .map(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return s;
            }
            let p = normalize_workspace_path(Path::new(trimmed));
            if !path_is_same_or_under(&p, &legacy_root) {
                return s;
            }
            let rel = if normalize_path_cmp_key(&p) == normalize_path_cmp_key(&legacy_root) {
                PathBuf::new()
            } else {
                match p.strip_prefix(&legacy_root) {
                    Ok(r) => r.to_path_buf(),
                    Err(_) => return s,
                }
            };
            let mapped = if rel.as_os_str().is_empty() {
                instance_root.clone()
            } else {
                instance_root.join(&rel)
            };
            let out = normalize_workspace_path(&mapped)
                .to_string_lossy()
                .into_owned();
            if out != s {
                log::info!(
                    "sandbox path remapped (--root-dir): {} -> {}",
                    trimmed,
                    out
                );
            }
            out
        })
        .collect()
}

fn rewrite_legacy_synbot_path_string(s: String) -> String {
    rewrite_legacy_synbot_paths_for_root_override(vec![s])
        .into_iter()
        .next()
        .unwrap_or_default()
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

/// Workflows root directory: `~/.synbot/workflows/` or config `workflow.workflowsRoot`.
/// Used by WorkflowStore for persistent workflow state per session.
pub fn workflows_root(cfg: &Config) -> PathBuf {
    cfg.workflow
        .workflows_root
        .as_deref()
        .map(|s| {
            let s = s.trim();
            if s.starts_with('~') {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(s.trim_start_matches("~/").trim_start_matches('~'))
            } else {
                PathBuf::from(s)
            }
        })
        .unwrap_or_else(|| config_dir().join("workflows"))
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
    let raw = &cfg.main_agent.workspace;
    if raw.starts_with('~') {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let rest = raw
            .trim_start_matches('~')
            .trim_start_matches('/')
            .trim_start_matches('\\');
        // Avoid `C:\Users\you\.\.synbot` when config has `~/.\.synbot` — collapse `.` segments.
        let tail: PathBuf = Path::new(rest).components().collect();
        home.join(tail)
    } else {
        PathBuf::from(raw)
    }
}

/// Collapse redundant `.` / `..` segments (e.g. `C:\Users\x\.\.synbot` → `C:\Users\x\.synbot`).
fn collapse_path_components(path: &Path) -> PathBuf {
    path.components().collect()
}

/// Resolve a workspace or exec `cwd` to an absolute path (canonicalize when possible).
///
/// Relative paths (e.g. `.\.synbot\...`) break Windows `CreateProcessW` current directory and
/// AppContainer ACL grants; tool sandbox + exec should use a stable absolute path.
pub fn normalize_workspace_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let resolved = std::fs::canonicalize(&absolute).unwrap_or(absolute);
    let resolved = {
        #[cfg(windows)]
        {
            strip_windows_verbatim_prefix(&resolved)
        }
        #[cfg(not(windows))]
        {
            resolved
        }
    };
    collapse_path_components(&resolved)
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

// ---------------------------------------------------------------------------
// Environment variable substitutor
// ---------------------------------------------------------------------------

/// Substitutes `${VAR_NAME}` and `${VAR_NAME:-default}` patterns in JSON string values.
/// Only operates on JSON string values; numbers, booleans, and null are left unchanged.
/// The escape sequence `\${...}` is preserved as the literal `${...}`.
pub struct EnvSubstitutor;

impl EnvSubstitutor {
    /// Perform environment variable substitution on a raw JSON string.
    /// Only JSON string values are processed; other value types are unchanged.
    pub fn substitute(raw_json: &str) -> Result<String> {
        let mut result = String::with_capacity(raw_json.len());
        let chars: Vec<char> = raw_json.chars().collect();
        let len = chars.len();
        let mut i = 0;

        // State tracking for JSON structure
        // We need to know if we're inside a string that is a VALUE (not a key).
        // JSON structure: object keys are strings followed by ':', values follow ':' or ','.
        // We track nesting depth and whether the next string is a key or value.
        #[derive(Clone, Copy, PartialEq)]
        enum Context {
            TopLevel,
            // Inside an object: tracks whether next string is a key or value
            Object { expect_key: bool },
            Array,
        }

        let mut context_stack: Vec<Context> = vec![Context::TopLevel];

        while i < len {
            let ch = chars[i];

            match ch {
                '{' => {
                    result.push(ch);
                    i += 1;
                    // Next string in this object is a key
                    context_stack.push(Context::Object { expect_key: true });
                }
                '[' => {
                    result.push(ch);
                    i += 1;
                    context_stack.push(Context::Array);
                }
                '}' | ']' => {
                    result.push(ch);
                    i += 1;
                    context_stack.pop();
                    // After closing a nested value, the parent object expects a comma or end
                    // We don't need to change parent state here
                }
                ':' => {
                    result.push(ch);
                    i += 1;
                    // After a colon in an object, the next string is a value
                    if let Some(Context::Object { expect_key }) = context_stack.last_mut() {
                        *expect_key = false;
                    }
                }
                ',' => {
                    result.push(ch);
                    i += 1;
                    // After a comma in an object, the next string is a key
                    if let Some(Context::Object { expect_key }) = context_stack.last_mut() {
                        *expect_key = true;
                    }
                    // In an array, commas separate values — no state change needed
                }
                '"' => {
                    // Determine if this string is a value we should substitute
                    let is_value = match context_stack.last() {
                        Some(Context::Object { expect_key }) => !expect_key,
                        Some(Context::Array) => true,
                        Some(Context::TopLevel) => true,
                        None => true,
                    };

                    // Parse the JSON string, collecting its raw (JSON-escaped) content
                    result.push('"');
                    i += 1; // skip opening quote

                    // Collect the raw string content (between the quotes)
                    let mut raw_value = String::new();
                    while i < len {
                        let c = chars[i];
                        if c == '"' {
                            // End of string
                            break;
                        } else if c == '\\' && i + 1 < len {
                            // JSON escape sequence
                            raw_value.push(c);
                            raw_value.push(chars[i + 1]);
                            i += 2;
                        } else {
                            raw_value.push(c);
                            i += 1;
                        }
                    }
                    // i now points at closing '"' or end of input

                    if is_value {
                        // Perform env var substitution on the raw value
                        let substituted = Self::substitute_value(&raw_value)?;
                        result.push_str(&substituted);
                    } else {
                        result.push_str(&raw_value);
                    }

                    result.push('"');
                    if i < len {
                        i += 1; // skip closing quote
                    }

                    // After a string value in an object, next string will be a key
                    if is_value {
                        if let Some(Context::Object { expect_key }) = context_stack.last_mut() {
                            *expect_key = true;
                        }
                    }
                }
                _ => {
                    result.push(ch);
                    i += 1;
                }
            }
        }

        Ok(result)
    }

    /// Substitute all `${VAR}` and `${VAR:-default}` patterns in a single string value.
    /// `\${...}` is preserved as the literal `${...}`.
    fn substitute_value(value: &str) -> Result<String> {
        let mut result = String::with_capacity(value.len());
        let chars: Vec<char> = value.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            // Check for escape sequence: \${
            if chars[i] == '\\' && i + 1 < len && chars[i + 1] == '$' && i + 2 < len && chars[i + 2] == '{' {
                // Escaped: preserve as literal ${...}
                // Skip the backslash, emit the rest literally until closing '}'
                i += 1; // skip backslash
                // Emit '${' and everything up to and including '}'
                result.push('$');
                result.push('{');
                i += 2; // skip '${'
                // Find the closing '}'
                while i < len && chars[i] != '}' {
                    result.push(chars[i]);
                    i += 1;
                }
                if i < len {
                    result.push('}');
                    i += 1; // skip '}'
                }
            } else if chars[i] == '$' && i + 1 < len && chars[i + 1] == '{' {
                // Start of ${...} pattern
                i += 2; // skip '${'
                let mut pattern = String::new();
                while i < len && chars[i] != '}' {
                    pattern.push(chars[i]);
                    i += 1;
                }
                if i < len {
                    i += 1; // skip '}'
                }

                // Parse VAR_NAME or VAR_NAME:-default
                let (var_name, default_val) = if let Some(pos) = pattern.find(":-") {
                    (&pattern[..pos], Some(&pattern[pos + 2..]))
                } else {
                    (pattern.as_str(), None)
                };

                match std::env::var(var_name) {
                    Ok(val) => result.push_str(&val),
                    Err(_) => {
                        if let Some(default) = default_val {
                            result.push_str(default);
                        } else {
                            anyhow::bail!(
                                "environment variable '{}' is not set and no default value was provided",
                                var_name
                            );
                        }
                    }
                }
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }

        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Config migration system
// ---------------------------------------------------------------------------

/// Migration function type: takes old JSON Value, returns new JSON Value.
pub type MigrationFn = fn(serde_json::Value) -> Result<serde_json::Value>;

/// Registry of config migrations. Each entry is (from_version, to_version, migration_fn).
pub struct ConfigMigrator {
    migrations: Vec<(u32, u32, MigrationFn)>,
    current_version: u32,
}

impl ConfigMigrator {
    pub fn new(current_version: u32) -> Self {
        Self {
            migrations: Vec::new(),
            current_version,
        }
    }

    pub fn register(&mut self, from: u32, to: u32, f: MigrationFn) {
        self.migrations.push((from, to, f));
        // Keep sorted by from_version so the chain executes in order
        self.migrations.sort_by_key(|(from, _, _)| *from);
    }

    /// Execute the migration chain:
    /// 1. Backup the original file (filename includes timestamp)
    /// 2. Apply each migration step in order
    /// 3. Write the migrated config back to disk
    /// 4. On any error, restore from backup and return an error
    pub fn migrate(
        &self,
        config_path: &Path,
        mut value: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let file_version = value
            .get("configVersion")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(1);

        if file_version >= self.current_version {
            return Ok(value);
        }

        // --- Step 1: create timestamped backup ---
        let backup_path = Self::backup_path(config_path);
        if config_path.exists() {
            std::fs::copy(config_path, &backup_path).with_context(|| {
                format!(
                    "creating migration backup at {}",
                    backup_path.display()
                )
            })?;
        }

        // --- Step 2: run migration chain ---
        let mut current = file_version;
        let mut migration_error: Option<anyhow::Error> = None;
        for &(from, to, f) in &self.migrations {
            if from == current && to <= self.current_version {
                // Use a sentinel to avoid partial-move issues: replace value with null temporarily
                let old_value = std::mem::replace(&mut value, serde_json::Value::Null);
                match f(old_value).with_context(|| format!("migration step {} -> {} failed", from, to)) {
                    Ok(new_value) => {
                        value = new_value;
                        current = to;
                    }
                    Err(e) => {
                        migration_error = Some(e);
                        break;
                    }
                }
            }
        }

        if let Some(e) = migration_error {
            // --- Step 4: rollback from backup ---
            if backup_path.exists() {
                let _ = std::fs::copy(&backup_path, config_path);
            }
            return Err(e);
        }

        // Update config_version in the JSON value
        if let serde_json::Value::Object(ref mut map) = value {
            map.insert(
                "configVersion".to_string(),
                serde_json::Value::Number(self.current_version.into()),
            );
        }

        // --- Step 3: write migrated config ---
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&value)?;
        std::fs::write(config_path, json).with_context(|| {
            format!("writing migrated config to {}", config_path.display())
        })?;
        Ok(value)
    }

    fn backup_path(config_path: &Path) -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let name = config_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "config.json".to_string());
        let backup_name = format!("{}.migration_backup_{}", name, ts);
        config_path
            .parent()
            .map(|p| p.join(&backup_name))
            .unwrap_or_else(|| PathBuf::from(&backup_name))
    }
}

/// Build the application's ConfigMigrator with all registered migration steps.
/// The current version is 1. Add new migration steps here as the config schema evolves.
fn build_config_migrator() -> ConfigMigrator {
    // Current version is 1; no migrations needed yet.
    // Example of adding a migration in the future:
    //   migrator.register(1, 2, |mut v| { /* transform v */ Ok(v) });
    ConfigMigrator::new(1)
}

pub fn load_config(path: Option<&Path>) -> Result<Config> {
    let p = path
        .map(PathBuf::from)
        .unwrap_or_else(config_path);

    let cfg = if p.exists() {
        let text = std::fs::read_to_string(&p)
            .with_context(|| format!("reading config from {}", p.display()))?;
        let text = EnvSubstitutor::substitute(&text)
            .with_context(|| format!("substituting environment variables in config from {}", p.display()))?;

        // Run config migration if needed
        let mut value: serde_json::Value = serde_json::from_str(&text)
            .with_context(|| format!("parsing config from {}", p.display()))?;

        let migrator = build_config_migrator();
        value = migrator.migrate(&p, value)?;

        serde_json::from_value(value)
            .with_context(|| format!("deserializing config from {}", p.display()))?
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

/// Registry channel type names valid for `synbot pairing` and pairing hints.
pub const PAIRING_CHANNEL_PROVIDERS: &[&str] = &[
    "telegram", "feishu", "discord", "slack", "email", "matrix", "dingtalk", "whatsapp", "irc",
];

/// Whether `name` is a built-in channel provider for pairing (CLI and messages).
pub fn is_pairing_channel_provider(name: &str) -> bool {
    PAIRING_CHANNEL_PROVIDERS
        .iter()
        .any(|p| p.eq_ignore_ascii_case(name))
}

/// 12 lowercase hex chars: MD5(chat_id) prefix.
pub fn pairing_code_from_chat_id(chat_id: &str) -> String {
    let digest = md5::compute(chat_id.as_bytes());
    format!("{:x}", digest)
        .chars()
        .take(12)
        .collect()
}

/// User-facing hint when allowlist blocks the chat (English, single line breaks avoided).
pub fn pairing_message(channel_provider: &str, chat_id: &str) -> String {
    let code = pairing_code_from_chat_id(chat_id);
    format!(
        "Synbot: Chat requires pairing, pairing code is {}. Current chat id is {}. Use following command to finish pairing: synbot pairing approve {} {}.",
        code, chat_id, channel_provider, code
    )
}

/// True if `pairings` contains an entry for this provider with matching code (case-insensitive).
pub fn pairing_allows(chat_id: &str, provider: &str, pairings: &[PairingEntry]) -> bool {
    let code = pairing_code_from_chat_id(chat_id);
    pairings.iter().any(|p| {
        p.channel.eq_ignore_ascii_case(provider) && p.pairing_code.eq_ignore_ascii_case(&code)
    })
}

fn read_pairings_from_file(path: &Path) -> Result<Vec<PairingEntry>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let v: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parse JSON {}", path.display()))?;
    match v.get("pairings") {
        Some(p) => Ok(serde_json::from_value(p.clone()).unwrap_or_default()),
        None => Ok(Vec::new()),
    }
}

type PairingsCacheMap = HashMap<PathBuf, (SystemTime, Vec<PairingEntry>)>;

fn pairings_cache_mutex() -> &'static Mutex<PairingsCacheMap> {
    static CACHE: OnceLock<Mutex<PairingsCacheMap>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Load `pairings` from the config JSON on disk, using file mtime to avoid redundant parses.
/// On read/parse failure, logs a warning and returns the last cached value for this path, or empty.
pub fn pairings_from_config_file_cached(config_path: &Path) -> Vec<PairingEntry> {
    let mtime = match std::fs::metadata(config_path) {
        Ok(m) => m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
        Err(e) => {
            tracing::warn!(
                path = %config_path.display(),
                error = %e,
                "pairings: config file metadata unreadable"
            );
            return Vec::new();
        }
    };

    let mut guard = pairings_cache_mutex()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some((t, list)) = guard.get(config_path) {
        if *t == mtime {
            return list.clone();
        }
    }

    match read_pairings_from_file(config_path) {
        Ok(pairings) => {
            guard.insert(config_path.to_path_buf(), (mtime, pairings.clone()));
            pairings
        }
        Err(e) => {
            tracing::warn!(
                path = %config_path.display(),
                error = %e,
                "pairings: failed to load from config; using stale cache if any"
            );
            guard
                .get(config_path)
                .map(|(_, p)| p.clone())
                .unwrap_or_default()
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- EnvSubstitutor tests ---

    #[test]
    fn env_substitutor_replaces_set_variable() {
        std::env::set_var("TEST_ENV_SUB_VAR", "hello");
        let json = r#"{"key": "${TEST_ENV_SUB_VAR}"}"#;
        let result = EnvSubstitutor::substitute(json).unwrap();
        assert_eq!(result, r#"{"key": "hello"}"#);
        std::env::remove_var("TEST_ENV_SUB_VAR");
    }

    #[test]
    fn env_substitutor_uses_default_when_var_unset() {
        std::env::remove_var("TEST_ENV_SUB_UNSET_XYZ");
        let json = r#"{"key": "${TEST_ENV_SUB_UNSET_XYZ:-fallback}"}"#;
        let result = EnvSubstitutor::substitute(json).unwrap();
        assert_eq!(result, r#"{"key": "fallback"}"#);
    }

    #[test]
    fn env_substitutor_errors_on_unset_var_without_default() {
        std::env::remove_var("TEST_ENV_SUB_MISSING_ABC");
        let json = r#"{"key": "${TEST_ENV_SUB_MISSING_ABC}"}"#;
        let result = EnvSubstitutor::substitute(json);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("TEST_ENV_SUB_MISSING_ABC"));
    }

    #[test]
    fn env_substitutor_does_not_modify_numbers() {
        let json = r#"{"count": 42, "ratio": 3.14}"#;
        let result = EnvSubstitutor::substitute(json).unwrap();
        assert_eq!(result, json);
    }

    #[test]
    fn env_substitutor_does_not_modify_booleans_and_null() {
        let json = r#"{"flag": true, "other": false, "nothing": null}"#;
        let result = EnvSubstitutor::substitute(json).unwrap();
        assert_eq!(result, json);
    }

    #[test]
    fn env_substitutor_preserves_escaped_dollar_brace() {
        let json = r#"{"key": "\${NOT_A_VAR}"}"#;
        let result = EnvSubstitutor::substitute(json).unwrap();
        assert_eq!(result, r#"{"key": "${NOT_A_VAR}"}"#);
    }

    #[test]
    fn env_substitutor_does_not_substitute_in_object_keys() {
        std::env::set_var("TEST_ENV_SUB_KEY_VAR", "replaced");
        // The key itself should not be substituted
        let json = r#"{"${TEST_ENV_SUB_KEY_VAR}": "value"}"#;
        let result = EnvSubstitutor::substitute(json).unwrap();
        assert_eq!(result, r#"{"${TEST_ENV_SUB_KEY_VAR}": "value"}"#);
        std::env::remove_var("TEST_ENV_SUB_KEY_VAR");
    }

    #[test]
    fn env_substitutor_substitutes_in_array_values() {
        std::env::set_var("TEST_ENV_SUB_ARR", "item");
        let json = r#"{"list": ["${TEST_ENV_SUB_ARR}", "static"]}"#;
        let result = EnvSubstitutor::substitute(json).unwrap();
        assert_eq!(result, r#"{"list": ["item", "static"]}"#);
        std::env::remove_var("TEST_ENV_SUB_ARR");
    }

    #[test]
    fn env_substitutor_multiple_patterns_in_one_value() {
        std::env::set_var("TEST_ENV_SUB_A", "foo");
        std::env::set_var("TEST_ENV_SUB_B", "bar");
        let json = r#"{"key": "${TEST_ENV_SUB_A}-${TEST_ENV_SUB_B}"}"#;
        let result = EnvSubstitutor::substitute(json).unwrap();
        assert_eq!(result, r#"{"key": "foo-bar"}"#);
        std::env::remove_var("TEST_ENV_SUB_A");
        std::env::remove_var("TEST_ENV_SUB_B");
    }

    #[test]
    fn load_config_substitutes_env_vars() {
        std::env::set_var("TEST_LOAD_CFG_TOKEN", "bot123:abc");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config_env.json");
        std::fs::write(
            &path,
            r#"{"channels":{"telegram":[{"name":"telegram","enabled":true,"token":"${TEST_LOAD_CFG_TOKEN}"}]},"mainChannel":"telegram"}"#,
        ).unwrap();
        let cfg = load_config(Some(&path)).unwrap();
        assert_eq!(cfg.channels.telegram[0].token, "bot123:abc");
        std::env::remove_var("TEST_LOAD_CFG_TOKEN");
    }

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
        cfg.main_agent.max_tokens = 0;
        let errors = validate_config(&cfg).unwrap_err();
        let err = find_error(&errors, "mainAgent.max_tokens").expect("expected error for max_tokens");
        assert_eq!(err.value, "0");
        assert!(err.constraint.contains("greater than 0"));
    }

    #[test]
    fn max_tokens_positive_is_accepted() {
        let mut cfg = valid_config();
        cfg.main_agent.max_tokens = 1;
        assert!(validate_config(&cfg).is_ok());
    }

    // --- agent.temperature ---

    #[test]
    fn temperature_below_zero_is_rejected() {
        let mut cfg = valid_config();
        cfg.main_agent.temperature = -0.1;
        let errors = validate_config(&cfg).unwrap_err();
        assert!(find_error(&errors, "mainAgent.temperature").is_some());
    }

    #[test]
    fn temperature_above_two_is_rejected() {
        let mut cfg = valid_config();
        cfg.main_agent.temperature = 2.1;
        let errors = validate_config(&cfg).unwrap_err();
        assert!(find_error(&errors, "mainAgent.temperature").is_some());
    }

    #[test]
    fn temperature_at_boundaries_is_accepted() {
        let mut cfg = valid_config();
        cfg.main_agent.temperature = 0.0;
        assert!(validate_config(&cfg).is_ok());

        cfg.main_agent.temperature = 2.0;
        assert!(validate_config(&cfg).is_ok());
    }

    // --- agent.max_tool_iterations ---

    #[test]
    fn max_tool_iterations_zero_is_rejected() {
        let mut cfg = valid_config();
        cfg.main_agent.max_tool_iterations = 0;
        let errors = validate_config(&cfg).unwrap_err();
        assert!(find_error(&errors, "mainAgent.max_tool_iterations").is_some());
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
        cfg.main_channel = "telegram".into();
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
        cfg.main_channel = "discord".into();
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
        cfg.main_channel = "feishu".into();
        assert!(validate_config(&cfg).is_ok());
    }

    // --- Multiple errors collected at once ---

    #[test]
    fn multiple_errors_are_collected() {
        let mut cfg = valid_config();
        cfg.main_agent.max_tokens = 0;
        cfg.main_agent.temperature = 5.0;
        cfg.tools.exec.timeout_secs = 0;
        cfg.channels.telegram = vec![TelegramConfig {
            name: "telegram".into(),
            enabled: true,
            token: String::new(),
            ..Default::default()
        }];

        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.len() >= 4, "expected at least 4 errors, got {}", errors.len());
        assert!(find_error(&errors, "mainAgent.max_tokens").is_some());
        assert!(find_error(&errors, "mainAgent.temperature").is_some());
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
            r#"{"mainAgent":{"maxTokens":0,"temperature":3.0}}"#,
        )
        .unwrap();

        let result = load_config(Some(&path));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("config validation failed"));
        assert!(msg.contains("mainAgent.max_tokens"));
        assert!(msg.contains("mainAgent.temperature"));
    }

    #[test]
    fn load_config_with_valid_file_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("good_config.json");
        std::fs::write(
            &path,
            r#"{"mainAgent":{"maxTokens":4096,"temperature":0.5,"maxToolIterations":10},"tools":{"exec":{"timeoutSecs":30}}}"#,
        )
        .unwrap();

        let result = load_config(Some(&path));
        assert!(result.is_ok());
        let cfg = result.unwrap();
        assert_eq!(cfg.main_agent.max_tokens, 4096);
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

    fn make_agent(name: &str, role: &str) -> AgentConfig {
        AgentConfig {
            name: name.into(),
            role: role.into(),
            provider: None,
            model: None,
            max_tokens: None,
            temperature: None,
            max_iterations: None,
            skills: Vec::new(),
            tools: Vec::new(),
        }
    }

    // --- main_channel validation ---

    #[test]
    fn main_channel_empty_with_multiple_agents_is_rejected() {
        let mut cfg = config_with_telegram();
        cfg.main_channel = String::new();
        cfg.main_agent.agents = vec![make_agent("helper", "dev")];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "main_channel"));
    }

    #[test]
    fn main_channel_referencing_disabled_channel_is_rejected() {
        let mut cfg = valid_config();
        cfg.main_channel = "telegram".into();
        cfg.main_agent.agents = vec![make_agent("helper", "dev")];
        // telegram is not enabled
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "main_channel"));
    }

    #[test]
    fn main_channel_referencing_enabled_channel_is_accepted() {
        let mut cfg = config_with_telegram();
        cfg.main_agent.agents = vec![make_agent("helper", "dev")];
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn main_channel_not_required_with_single_agent() {
        let mut cfg = valid_config();
        cfg.main_channel = String::new();
        cfg.main_agent.agents = vec![];
        assert!(validate_config(&cfg).is_ok());
    }

    // --- Agent validation (main is implicit; must not define agent named "main") ---

    #[test]
    fn agent_named_main_is_rejected() {
        let mut cfg = config_with_telegram();
        cfg.main_agent.agents = vec![make_agent("main", "main")];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.constraint.contains("must not be 'main'")));
    }

    #[test]
    fn agent_role_with_special_chars_is_rejected() {
        let mut cfg = config_with_telegram();
        cfg.main_agent.agents = vec![make_agent("helper", "bad-role!")];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.constraint.contains("letters, digits, and underscores")));
    }

    #[test]
    fn agent_role_alphanumeric_underscore_is_accepted() {
        let mut cfg = config_with_telegram();
        cfg.main_agent.agents = vec![make_agent("helper", "good_Role_123")];
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn agent_role_empty_is_rejected() {
        let mut cfg = config_with_telegram();
        cfg.main_agent.agents = vec![
            AgentConfig { name: "helper".into(), role: String::new(), provider: None, model: None, max_tokens: None, temperature: None, max_iterations: None, skills: Vec::new(), tools: Vec::new() },
        ];
        let errors = validate_config(&cfg).unwrap_err();
        assert!(errors.iter().any(|e| e.constraint.contains("role must be non-empty")));
    }

    // --- ValidationError Display ---

    #[test]
    fn validation_error_display_format() {
        let err = ValidationError {
            field: "mainAgent.temperature".into(),
            value: "3.0".into(),
            constraint: "must be between 0.0 and 2.0".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("mainAgent.temperature"));
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

    #[test]
    fn pairing_code_from_chat_id_is_twelve_lowercase_hex() {
        let code = pairing_code_from_chat_id("room!abc:example.org");
        assert_eq!(code.len(), 12);
        assert!(code.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn pairing_allows_matches_case_insensitive() {
        let chat = "12345";
        let lower = pairing_code_from_chat_id(chat);
        let pairings = vec![PairingEntry {
            channel: "discord".to_string(),
            pairing_code: lower.to_uppercase(),
        }];
        assert!(pairing_allows(chat, "discord", &pairings));
        assert!(pairing_allows(chat, "DISCORD", &pairings));
        assert!(!pairing_allows(chat, "slack", &pairings));
    }

    #[test]
    fn pairing_message_contains_channel_and_code() {
        let m = pairing_message("feishu", "oc_xxx");
        assert!(m.contains("feishu"));
        assert!(m.contains("oc_xxx"));
        assert!(m.contains(&pairing_code_from_chat_id("oc_xxx")));
        assert!(m.contains("synbot pairing approve"));
    }
}

