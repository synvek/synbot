//! Full-config JSON redaction and merge for the web config API.

use crate::config::Config;
use serde_json::Value;

use super::sanitize::CONFIG_SECRET_MASK;

fn is_secret_json_key(key: &str) -> bool {
    matches!(
        key,
        "token"
            | "appToken"
            | "apiKey"
            | "password"
            | "appSecret"
            | "clientSecret"
            | "accessToken"
            | "appId"
            | "refreshToken"
    ) || key.ends_with("ApiKey")
}

/// Serialize config to JSON and mask known secret string fields (non-empty).
pub fn config_to_redacted_value(config: &Config) -> Result<Value, serde_json::Error> {
    let mut v = serde_json::to_value(config)?;
    redact_secrets_in_value(&mut v);
    Ok(v)
}

fn redact_secrets_in_value(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if let Value::String(s) = val {
                    if !s.is_empty() && is_secret_json_key(k) {
                        *s = CONFIG_SECRET_MASK.to_string();
                    }
                } else {
                    redact_secrets_in_value(val);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                redact_secrets_in_value(item);
            }
        }
        _ => {}
    }
}

/// When the client sends the same masked placeholder back, restore secrets from `old`.
pub fn merge_incoming_preserving_secrets(old: &Config, mut incoming: Value) -> Result<Config, serde_json::Error> {
    let old_v = serde_json::to_value(old)?;
    patch_secret_masks_from_old(&old_v, &mut incoming);
    serde_json::from_value(incoming)
}

fn patch_secret_masks_from_old(old: &Value, new: &mut Value) {
    if let (Value::String(os), Value::String(ns)) = (old, &*new) {
        if ns == CONFIG_SECRET_MASK {
            *new = Value::String(os.clone());
            return;
        }
    }
    match (old, new) {
        (Value::Object(om), Value::Object(nm)) => {
            for (k, nv) in nm.iter_mut() {
                if let Some(ov) = om.get(k) {
                    patch_secret_masks_from_old(ov, nv);
                }
            }
        }
        (Value::Array(oa), Value::Array(na)) => {
            let n = oa.len().min(na.len());
            for i in 0..n {
                patch_secret_masks_from_old(&oa[i], &mut na[i]);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ChannelsConfig, Config, ProviderEntry, ProvidersConfig, TelegramConfig};

    #[test]
    fn redact_masks_telegram_token() {
        let mut cfg = Config::default();
        cfg.channels = ChannelsConfig {
            telegram: vec![TelegramConfig {
                token: "secret-token".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let v = config_to_redacted_value(&cfg).unwrap();
        let tok = v["channels"]["telegram"][0]["token"].as_str().unwrap();
        assert_eq!(tok, CONFIG_SECRET_MASK);
    }

    #[test]
    fn merge_restores_masked_strings() {
        let mut cfg = Config::default();
        cfg.providers = ProvidersConfig {
            anthropic: ProviderEntry {
                api_key: "sk-real".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut incoming = config_to_redacted_value(&cfg).unwrap();
        incoming["mainAgent"]["maxTokens"] = 8192.into();
        let merged = merge_incoming_preserving_secrets(&cfg, incoming).unwrap();
        assert_eq!(merged.providers.anthropic.api_key, "sk-real");
        assert_eq!(merged.main_agent.max_tokens, 8192);
    }
}
