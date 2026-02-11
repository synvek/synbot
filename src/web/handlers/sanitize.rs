use crate::config::{
    ChannelsConfig, Config, DiscordConfig, FeishuConfig, ProviderEntry, ProvidersConfig,
    TelegramConfig, WebAuthConfig, WebConfig,
};
use serde::Serialize;

const MASK: &str = "********";

/// Sanitized configuration that masks sensitive fields
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizedConfig {
    pub channels: SanitizedChannelsConfig,
    pub providers: SanitizedProvidersConfig,
    pub web: SanitizedWebConfig,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizedChannelsConfig {
    pub telegram: SanitizedTelegramConfig,
    pub discord: SanitizedDiscordConfig,
    pub feishu: SanitizedFeishuConfig,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizedTelegramConfig {
    pub enabled: bool,
    pub token: String,
    pub allow_from: Vec<String>,
    pub proxy: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizedDiscordConfig {
    pub enabled: bool,
    pub token: String,
    pub allow_from: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizedFeishuConfig {
    pub enabled: bool,
    pub app_id: String,
    pub app_secret: String,
    pub allow_from: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizedProvidersConfig {
    pub anthropic: SanitizedProviderEntry,
    pub openai: SanitizedProviderEntry,
    pub openrouter: SanitizedProviderEntry,
    pub deepseek: SanitizedProviderEntry,
    pub ollama: SanitizedProviderEntry,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizedProviderEntry {
    pub api_key: String,
    pub api_base: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizedWebConfig {
    pub enabled: bool,
    pub port: u16,
    pub host: String,
    pub auth: Option<SanitizedWebAuthConfig>,
    pub cors_origins: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SanitizedWebAuthConfig {
    pub username: String,
    pub password: String,
}

/// Sanitize a configuration by masking sensitive fields
pub fn sanitize_config(config: &Config) -> SanitizedConfig {
    SanitizedConfig {
        channels: sanitize_channels(&config.channels),
        providers: sanitize_providers(&config.providers),
        web: sanitize_web_config(&config.web),
    }
}

fn sanitize_channels(channels: &ChannelsConfig) -> SanitizedChannelsConfig {
    SanitizedChannelsConfig {
        telegram: sanitize_telegram(&channels.telegram),
        discord: sanitize_discord(&channels.discord),
        feishu: sanitize_feishu(&channels.feishu),
    }
}

fn sanitize_telegram(config: &TelegramConfig) -> SanitizedTelegramConfig {
    SanitizedTelegramConfig {
        enabled: config.enabled,
        token: mask_if_not_empty(&config.token),
        allow_from: config.allow_from.clone(),
        proxy: config.proxy.clone(),
    }
}

fn sanitize_discord(config: &DiscordConfig) -> SanitizedDiscordConfig {
    SanitizedDiscordConfig {
        enabled: config.enabled,
        token: mask_if_not_empty(&config.token),
        allow_from: config.allow_from.clone(),
    }
}

fn sanitize_feishu(config: &FeishuConfig) -> SanitizedFeishuConfig {
    SanitizedFeishuConfig {
        enabled: config.enabled,
        app_id: mask_if_not_empty(&config.app_id),
        app_secret: mask_if_not_empty(&config.app_secret),
        allow_from: config.allow_from.clone(),
    }
}

fn sanitize_providers(providers: &ProvidersConfig) -> SanitizedProvidersConfig {
    SanitizedProvidersConfig {
        anthropic: sanitize_provider_entry(&providers.anthropic),
        openai: sanitize_provider_entry(&providers.openai),
        openrouter: sanitize_provider_entry(&providers.openrouter),
        deepseek: sanitize_provider_entry(&providers.deepseek),
        ollama: sanitize_provider_entry(&providers.ollama),
    }
}

fn sanitize_provider_entry(entry: &ProviderEntry) -> SanitizedProviderEntry {
    SanitizedProviderEntry {
        api_key: mask_if_not_empty(&entry.api_key),
        api_base: entry.api_base.clone(),
    }
}

fn sanitize_web_config(config: &WebConfig) -> SanitizedWebConfig {
    SanitizedWebConfig {
        enabled: config.enabled,
        port: config.port,
        host: config.host.clone(),
        auth: config.auth.as_ref().map(sanitize_web_auth),
        cors_origins: config.cors_origins.clone(),
    }
}

fn sanitize_web_auth(auth: &WebAuthConfig) -> SanitizedWebAuthConfig {
    SanitizedWebAuthConfig {
        username: auth.username.clone(),
        password: MASK.to_string(),
    }
}

/// Mask a string if it's not empty, otherwise return empty string
fn mask_if_not_empty(s: &str) -> String {
    if s.is_empty() {
        String::new()
    } else {
        MASK.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_if_not_empty_masks_non_empty_strings() {
        assert_eq!(mask_if_not_empty("secret123"), MASK);
        assert_eq!(mask_if_not_empty("a"), MASK);
    }

    #[test]
    fn mask_if_not_empty_preserves_empty_strings() {
        assert_eq!(mask_if_not_empty(""), "");
    }

    #[test]
    fn sanitize_telegram_masks_token() {
        let config = TelegramConfig {
            enabled: true,
            token: "bot123:secret".to_string(),
            allow_from: vec!["user1".to_string()],
            proxy: Some("http://proxy".to_string()),
        };

        let sanitized = sanitize_telegram(&config);
        assert_eq!(sanitized.token, MASK);
        assert_eq!(sanitized.enabled, true);
        assert_eq!(sanitized.allow_from, vec!["user1"]);
        assert_eq!(sanitized.proxy, Some("http://proxy".to_string()));
    }

    #[test]
    fn sanitize_discord_masks_token() {
        let config = DiscordConfig {
            enabled: true,
            token: "discord_token".to_string(),
            allow_from: vec![],
        };

        let sanitized = sanitize_discord(&config);
        assert_eq!(sanitized.token, MASK);
        assert_eq!(sanitized.enabled, true);
    }

    #[test]
    fn sanitize_feishu_masks_credentials() {
        let config = FeishuConfig {
            enabled: true,
            app_id: "app123".to_string(),
            app_secret: "secret456".to_string(),
            allow_from: vec![],
        };

        let sanitized = sanitize_feishu(&config);
        assert_eq!(sanitized.app_id, MASK);
        assert_eq!(sanitized.app_secret, MASK);
        assert_eq!(sanitized.enabled, true);
    }

    #[test]
    fn sanitize_provider_entry_masks_api_key() {
        let entry = ProviderEntry {
            api_key: "sk-1234567890".to_string(),
            api_base: Some("https://api.example.com".to_string()),
        };

        let sanitized = sanitize_provider_entry(&entry);
        assert_eq!(sanitized.api_key, MASK);
        assert_eq!(sanitized.api_base, Some("https://api.example.com".to_string()));
    }

    #[test]
    fn sanitize_web_auth_masks_password() {
        let auth = WebAuthConfig {
            username: "admin".to_string(),
            password: "secret123".to_string(),
        };

        let sanitized = sanitize_web_auth(&auth);
        assert_eq!(sanitized.username, "admin");
        assert_eq!(sanitized.password, MASK);
    }

    #[test]
    fn sanitize_config_masks_all_sensitive_fields() {
        let config = Config {
            channels: ChannelsConfig {
                telegram: TelegramConfig {
                    enabled: true,
                    token: "bot_token".to_string(),
                    allow_from: vec![],
                    proxy: None,
                },
                discord: DiscordConfig {
                    enabled: false,
                    token: "discord_token".to_string(),
                    allow_from: vec![],
                },
                feishu: FeishuConfig {
                    enabled: false,
                    app_id: "app_id".to_string(),
                    app_secret: "app_secret".to_string(),
                    allow_from: vec![],
                },
            },
            providers: ProvidersConfig {
                anthropic: ProviderEntry {
                    api_key: "anthropic_key".to_string(),
                    api_base: None,
                },
                openai: ProviderEntry {
                    api_key: "openai_key".to_string(),
                    api_base: None,
                },
                openrouter: ProviderEntry {
                    api_key: "".to_string(),
                    api_base: None,
                },
                deepseek: ProviderEntry {
                    api_key: "".to_string(),
                    api_base: None,
                },
                ollama: ProviderEntry {
                    api_key: "".to_string(),
                    api_base: Some("http://localhost:11434".to_string()),
                },
            },
            web: WebConfig {
                enabled: true,
                port: 8080,
                host: "127.0.0.1".to_string(),
                auth: Some(WebAuthConfig {
                    username: "admin".to_string(),
                    password: "password123".to_string(),
                }),
                cors_origins: vec![],
            },
            ..Default::default()
        };

        let sanitized = sanitize_config(&config);

        // Check channels
        assert_eq!(sanitized.channels.telegram.token, MASK);
        assert_eq!(sanitized.channels.discord.token, MASK);
        assert_eq!(sanitized.channels.feishu.app_id, MASK);
        assert_eq!(sanitized.channels.feishu.app_secret, MASK);

        // Check providers
        assert_eq!(sanitized.providers.anthropic.api_key, MASK);
        assert_eq!(sanitized.providers.openai.api_key, MASK);
        assert_eq!(sanitized.providers.openrouter.api_key, ""); // empty preserved
        assert_eq!(sanitized.providers.deepseek.api_key, "");
        assert_eq!(sanitized.providers.ollama.api_key, "");

        // Check web auth
        assert_eq!(sanitized.web.auth.as_ref().unwrap().username, "admin");
        assert_eq!(sanitized.web.auth.as_ref().unwrap().password, MASK);
    }
}
