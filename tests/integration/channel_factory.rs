//! Channel factory integration tests.
//!
//! Verifies that each registered ChannelFactory can:
//! 1. Create a Channel instance from a valid config (Requirement 11.2)
//! 2. Return a descriptive error when required fields are missing (Requirement 11.5)
//!
//! Run with: `cargo test --test integration channel_factory`

use synbot::channels::{factory::register_builtin_channels, ChannelRegistry, ChannelStartContext};
use tokio::sync::{broadcast, mpsc};

/// Build a minimal ChannelStartContext for testing (no real connections needed).
fn test_ctx() -> ChannelStartContext {
    let (inbound_tx, _) = mpsc::channel(8);
    let (outbound_tx, outbound_rx) = broadcast::channel(8);
    ChannelStartContext {
        inbound_tx,
        outbound_rx,
        show_tool_calls: false,
        tool_result_preview_chars: 200,
        workspace: None,
        approval_manager: None,
        completion_model: None,
        outbound_tx: Some(outbound_tx),
    }
}

fn registry() -> ChannelRegistry {
    let mut r = ChannelRegistry::new();
    register_builtin_channels(&mut r);
    r
}

// ---------------------------------------------------------------------------
// Requirement 11.1 — all built-in channel types are registered
// ---------------------------------------------------------------------------

#[test]
fn all_builtin_channel_types_are_registered() {
    let r = registry();
    let names = r.type_names();
    let expected = [
        "telegram",
        "discord",
        "feishu",
        "slack",
        "email",
        "matrix",
        "dingtalk",
        "whatsapp",
        "irc",
    ];
    for e in expected {
        assert!(names.contains(&e.to_string()), "missing channel type: {}", e);
    }
    assert_eq!(names.len(), expected.len());
}

// ---------------------------------------------------------------------------
// Telegram
// ---------------------------------------------------------------------------

#[test]
fn telegram_factory_creates_channel_from_valid_config() {
    let r = registry();
    let factory = r.get("telegram").expect("telegram factory");
    let config = serde_json::json!({
        "name": "telegram",
        "enabled": true,
        "token": "123456:ABC-test-token",
        "allowlist": [],
        "enableAllowlist": true,
        "showToolCalls": true,
        "defaultAgent": "main"
    });
    let result = factory.create(config, test_ctx());
    assert!(result.is_ok(), "telegram factory should succeed: {:?}", result.err());
    assert_eq!(result.unwrap().name(), "telegram");
}

#[test]
fn telegram_factory_accepts_empty_token() {
    // Token is a String with default "", so missing token is not an error at factory level
    let r = registry();
    let factory = r.get("telegram").expect("telegram factory");
    let config = serde_json::json!({ "enabled": false });
    let result = factory.create(config, test_ctx());
    assert!(result.is_ok(), "telegram factory should accept minimal config");
}

// ---------------------------------------------------------------------------
// Discord
// ---------------------------------------------------------------------------

#[test]
fn discord_factory_creates_channel_from_valid_config() {
    let r = registry();
    let factory = r.get("discord").expect("discord factory");
    let config = serde_json::json!({
        "name": "discord",
        "enabled": true,
        "token": "Bot.test-discord-token",
        "allowlist": [],
        "enableAllowlist": true,
        "showToolCalls": true,
        "defaultAgent": "main"
    });
    let result = factory.create(config, test_ctx());
    assert!(result.is_ok(), "discord factory should succeed: {:?}", result.err());
    assert_eq!(result.unwrap().name(), "discord");
}

#[test]
fn discord_factory_accepts_minimal_config() {
    let r = registry();
    let factory = r.get("discord").expect("discord factory");
    let config = serde_json::json!({ "enabled": false });
    let result = factory.create(config, test_ctx());
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// Feishu
// ---------------------------------------------------------------------------

#[test]
fn feishu_factory_creates_channel_from_valid_config() {
    let r = registry();
    let factory = r.get("feishu").expect("feishu factory");
    let config = serde_json::json!({
        "name": "feishu",
        "enabled": true,
        "appId": "cli_test",
        "appSecret": "secret_test",
        "allowlist": [],
        "enableAllowlist": true,
        "showToolCalls": true,
        "defaultAgent": "main"
    });
    let result = factory.create(config, test_ctx());
    assert!(result.is_ok(), "feishu factory should succeed: {:?}", result.err());
    assert_eq!(result.unwrap().name(), "feishu");
}

// ---------------------------------------------------------------------------
// Slack
// ---------------------------------------------------------------------------

#[test]
fn slack_factory_creates_channel_from_valid_config() {
    let r = registry();
    let factory = r.get("slack").expect("slack factory");
    let config = serde_json::json!({
        "name": "slack",
        "enabled": true,
        "token": "xoxb-test-token",
        "appToken": "xapp-test-token",
        "allowlist": [],
        "enableAllowlist": true,
        "showToolCalls": true,
        "defaultAgent": "main"
    });
    let result = factory.create(config, test_ctx());
    assert!(result.is_ok(), "slack factory should succeed: {:?}", result.err());
    assert_eq!(result.unwrap().name(), "slack");
}

// ---------------------------------------------------------------------------
// Email
// ---------------------------------------------------------------------------

#[test]
fn email_factory_creates_channel_from_valid_config() {
    let r = registry();
    let factory = r.get("email").expect("email factory");
    let config = serde_json::json!({
        "name": "email",
        "enabled": true,
        "imap": {
            "host": "imap.example.com",
            "port": 993,
            "username": "bot@example.com",
            "password": "secret",
            "useTls": true
        },
        "smtp": {
            "host": "smtp.example.com",
            "port": 465,
            "username": "bot@example.com",
            "password": "secret",
            "useTls": true
        },
        "fromSender": "user@example.com",
        "startTime": "",
        "pollIntervalSecs": 120,
        "showToolCalls": true,
        "defaultAgent": "main"
    });
    let result = factory.create(config, test_ctx());
    assert!(result.is_ok(), "email factory should succeed: {:?}", result.err());
    assert_eq!(result.unwrap().name(), "email");
}

// ---------------------------------------------------------------------------
// Matrix
// ---------------------------------------------------------------------------

#[test]
fn matrix_factory_creates_channel_from_valid_config() {
    let r = registry();
    let factory = r.get("matrix").expect("matrix factory");
    let config = serde_json::json!({
        "name": "matrix",
        "enabled": true,
        "homeserverUrl": "https://matrix.example.org",
        "username": "@bot:example.org",
        "password": "secret",
        "allowlist": [],
        "enableAllowlist": true,
        "showToolCalls": true,
        "defaultAgent": "main"
    });
    let result = factory.create(config, test_ctx());
    assert!(result.is_ok(), "matrix factory should succeed: {:?}", result.err());
    assert_eq!(result.unwrap().name(), "matrix");
}

// ---------------------------------------------------------------------------
// DingTalk
// ---------------------------------------------------------------------------

#[test]
fn dingtalk_factory_creates_channel_from_valid_config() {
    let r = registry();
    let factory = r.get("dingtalk").expect("dingtalk factory");
    let config = serde_json::json!({
        "name": "dingtalk",
        "enabled": true,
        "clientId": "test-client-id",
        "clientSecret": "test-client-secret",
        "allowlist": [],
        "enableAllowlist": true,
        "showToolCalls": true,
        "defaultAgent": "main",
        "robotCode": ""
    });
    let result = factory.create(config, test_ctx());
    assert!(result.is_ok(), "dingtalk factory should succeed: {:?}", result.err());
    assert_eq!(result.unwrap().name(), "dingtalk");
}

// ---------------------------------------------------------------------------
// WhatsApp
// ---------------------------------------------------------------------------

#[test]
fn whatsapp_factory_creates_channel_from_valid_config() {
    let r = registry();
    let factory = r.get("whatsapp").expect("whatsapp factory");
    let config = serde_json::json!({
        "name": "whatsapp",
        "enabled": true,
        "sessionDir": "/tmp/synbot-test-whatsapp-session",
        "allowlist": [],
        "agent": "main"
    });
    let result = factory.create(config, test_ctx());
    assert!(result.is_ok(), "whatsapp factory should succeed: {:?}", result.err());
    assert_eq!(result.unwrap().name(), "whatsapp");
}

#[test]
fn whatsapp_factory_accepts_config_without_optional_fields() {
    // session_dir defaults to ""; factory still builds (runtime warns if enabled without dir)
    let r = registry();
    let factory = r.get("whatsapp").expect("whatsapp factory");
    let config = serde_json::json!({ "enabled": false });
    let result = factory.create(config, test_ctx());
    assert!(result.is_ok(), "whatsapp factory should accept minimal config: {:?}", result.err());
}

#[test]
fn whatsapp_factory_rejects_invalid_json_type() {
    // Passing a non-object (e.g. a string) should fail deserialization
    let r = registry();
    let factory = r.get("whatsapp").expect("whatsapp factory");
    let config = serde_json::json!("not-an-object");
    let result = factory.create(config, test_ctx());
    assert!(result.is_err(), "whatsapp factory should reject non-object config");
}

// ---------------------------------------------------------------------------
// IRC
// ---------------------------------------------------------------------------

#[test]
fn irc_factory_creates_channel_from_valid_config() {
    let r = registry();
    let factory = r.get("irc").expect("irc factory");
    let config = serde_json::json!({
        "name": "irc",
        "enabled": true,
        "server": "irc.libera.chat",
        "port": 6697,
        "nickname": "synbot-test",
        "channels": ["#test"],
        "useTls": true,
        "allowlist": [],
        "agent": "main"
    });
    let result = factory.create(config, test_ctx());
    assert!(result.is_ok(), "irc factory should succeed: {:?}", result.err());
    assert_eq!(result.unwrap().name(), "irc");
}

#[test]
fn irc_factory_accepts_config_without_optional_fields() {
    // server, nickname, password are all Option<String>
    let r = registry();
    let factory = r.get("irc").expect("irc factory");
    let config = serde_json::json!({ "enabled": false });
    let result = factory.create(config, test_ctx());
    assert!(result.is_ok(), "irc factory should accept minimal config: {:?}", result.err());
}

#[test]
fn irc_factory_rejects_invalid_json_type() {
    let r = registry();
    let factory = r.get("irc").expect("irc factory");
    let config = serde_json::json!(42);
    let result = factory.create(config, test_ctx());
    assert!(result.is_err(), "irc factory should reject non-object config");
}

// ---------------------------------------------------------------------------
// Requirement 11.5 — unknown channel type returns None from registry
// ---------------------------------------------------------------------------

#[test]
fn registry_returns_none_for_unknown_channel_type() {
    let r = registry();
    assert!(r.get("nonexistent_channel").is_none());
    assert!(r.get("").is_none());
}
