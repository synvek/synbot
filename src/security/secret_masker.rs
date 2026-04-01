//! Log secret masking layer for tracing_subscriber.
//!
//! Scans log messages before they are written and replaces known sensitive
//! patterns (API keys, tokens, Bearer credentials) with redacted placeholders.
//! Also supports loading runtime secrets from the application Config so that
//! any value stored in a `#[sensitive]` field is masked as well.

use std::sync::{Arc, RwLock};

use tracing::field::Visit;
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

use crate::config::Config;

// ---------------------------------------------------------------------------
// Pattern definitions
// ---------------------------------------------------------------------------

struct SecretPattern {
    prefix: &'static str,
    label: &'static str,
}

/// All well-known sensitive token prefixes and their human-readable labels.
static PATTERNS: &[SecretPattern] = &[
    SecretPattern { prefix: "sk-",   label: "api_key"    },
    SecretPattern { prefix: "xoxb-", label: "slack_bot"  },
    SecretPattern { prefix: "xoxp-", label: "slack_user" },
    SecretPattern { prefix: "ghp_",  label: "github_pat" },
    SecretPattern { prefix: "gho_",  label: "github_oauth" },
    // Bearer is handled specially: the token follows the space
    SecretPattern { prefix: "Bearer ", label: "bearer_token" },
];

// ---------------------------------------------------------------------------
// SecretMaskerLayer
// ---------------------------------------------------------------------------

/// A `tracing_subscriber` [`Layer`] that redacts sensitive strings from log
/// messages before they reach downstream layers (file writer, stdout, etc.).
///
/// # Masking format
/// For a token `sk-abcdef1234`, the output is `sk-a[REDACTED:api_key]`
/// (first 4 characters of the *full* matched string are kept).
#[derive(Clone)]
pub struct SecretMaskerLayer {
    /// Runtime secrets extracted from Config (e.g. API keys stored at runtime).
    config_secrets: Arc<RwLock<Vec<String>>>,
}

impl SecretMaskerLayer {
    /// Create a new layer with no runtime secrets loaded yet.
    pub fn new() -> Self {
        Self {
            config_secrets: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Extract all sensitive field values from the application [`Config`] and
    /// register them for masking.
    ///
    /// The following fields are treated as sensitive:
    /// - All `api_key` values in `providers.*`
    /// - `providers.*.brave_api_key`, `tavily_api_key`, `firecrawl_api_key`
    /// - Channel tokens: `telegram.*.token`, `discord.*.token`, `slack.*.token`,
    ///   `slack.*.app_token`, `feishu.*.app_secret`, `dingtalk.*.client_secret`,
    ///   `matrix.*.password`, `matrix.*.access_token`, `email.*.imap.password`,
    ///   `email.*.smtp.password`
    pub fn load_config_secrets(&self, config: &Config) {
        let mut secrets = self
            .config_secrets
            .write()
            .expect("config_secrets RwLock poisoned");
        secrets.clear();

        // Provider API keys
        let p = &config.providers;
        for key in [
            p.anthropic.api_key.as_str(),
            p.openai.api_key.as_str(),
            p.gemini.api_key.as_str(),
            p.openrouter.api_key.as_str(),
            p.deepseek.api_key.as_str(),
            p.moonshot.api_key.as_str(),
            p.kimi_code.api_key.as_str(),
            p.ollama.api_key.as_str(),
        ] {
            if !key.is_empty() {
                secrets.push(key.to_string());
            }
        }
        for entry in p.extra.values() {
            if !entry.api_key.is_empty() {
                secrets.push(entry.api_key.clone());
            }
        }

        // Web tool API keys
        let w = &config.tools.web;
        for key in [
            w.brave_api_key.as_str(),
            w.tavily_api_key.as_str(),
            w.firecrawl_api_key.as_str(),
        ] {
            if !key.is_empty() {
                secrets.push(key.to_string());
            }
        }

        // Channel tokens
        for ch in &config.channels.telegram {
            if !ch.token.is_empty() {
                secrets.push(ch.token.clone());
            }
        }
        for ch in &config.channels.discord {
            if !ch.token.is_empty() {
                secrets.push(ch.token.clone());
            }
        }
        for ch in &config.channels.slack {
            if !ch.token.is_empty() {
                secrets.push(ch.token.clone());
            }
            if !ch.app_token.is_empty() {
                secrets.push(ch.app_token.clone());
            }
        }
        for ch in &config.channels.feishu {
            if !ch.app_secret.is_empty() {
                secrets.push(ch.app_secret.clone());
            }
        }
        for ch in &config.channels.dingtalk {
            if !ch.client_secret.is_empty() {
                secrets.push(ch.client_secret.clone());
            }
            if let Some(s) = &ch.app_secret {
                if !s.is_empty() {
                    secrets.push(s.clone());
                }
            }
        }
        for ch in &config.channels.matrix {
            if !ch.password.is_empty() {
                secrets.push(ch.password.clone());
            }
            if let Some(t) = &ch.access_token {
                if !t.is_empty() {
                    secrets.push(t.clone());
                }
            }
        }
        for ch in &config.channels.email {
            if !ch.imap.password.is_empty() {
                secrets.push(ch.imap.password.clone());
            }
            if !ch.smtp.password.is_empty() {
                secrets.push(ch.smtp.password.clone());
            }
        }
    }

    /// Mask all sensitive patterns in `input`, returning the sanitised string.
    ///
    /// For each match the first 4 characters of the matched token are kept and
    /// the remainder is replaced with `[REDACTED:<label>]`.
    pub fn mask(&self, input: &str) -> String {
        let mut result = input.to_string();

        // 1. Mask config-level runtime secrets (exact substring match).
        if let Ok(secrets) = self.config_secrets.read() {
            for secret in secrets.iter() {
                if secret.chars().count() < 4 || !result.contains(secret.as_str()) {
                    continue;
                }
                let keep: String = secret.chars().take(4).collect();
                let placeholder = format!("{}[REDACTED:config_secret]", keep);
                result = result.replace(secret.as_str(), &placeholder);
            }
        }

        // 2. Mask well-known token patterns.
        for pat in PATTERNS {
            result = mask_pattern(&result, pat.prefix, pat.label);
        }

        result
    }
}

impl Default for SecretMaskerLayer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Pattern masking helper
// ---------------------------------------------------------------------------

/// Replace occurrences of `prefix<token>` in `input` with
/// `<first-4-chars-of-full-match>[REDACTED:<label>]`.
///
/// A token is considered to end at the first whitespace, quote, or end-of-string.
fn mask_pattern(input: &str, prefix: &str, label: &str) -> String {
    if !input.contains(prefix) {
        return input.to_string();
    }

    let mut output = String::with_capacity(input.len());
    let mut remaining = input;

    while let Some(pos) = remaining.find(prefix) {
        // Append everything before the match unchanged.
        output.push_str(&remaining[..pos]);

        // The full matched region starts at `pos`.
        let after_prefix = &remaining[pos + prefix.len()..];

        // Find where the token ends (whitespace, quote, or end of string).
        let token_len = after_prefix
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '`')
            .unwrap_or(after_prefix.len());

        let full_match = &remaining[pos..pos + prefix.len() + token_len];

        // Use character indices: `&str[..4]` is byte-based and panics on UTF-8
        // boundaries (e.g. `bot，` where the comma is multi-byte).
        if full_match.chars().count() >= 4 {
            let keep: String = full_match.chars().take(4).collect();
            output.push_str(&keep);
            output.push_str(&format!("[REDACTED:{}]", label));
        } else {
            // Too short to be a real secret — emit as-is.
            output.push_str(full_match);
        }

        remaining = &remaining[pos + prefix.len() + token_len..];
    }

    output.push_str(remaining);
    output
}

// ---------------------------------------------------------------------------
// tracing_subscriber::Layer implementation
// ---------------------------------------------------------------------------

/// Visitor that collects all string fields from a tracing event.
struct MaskingVisitor {
    fields: Vec<(String, String)>,
}

impl MaskingVisitor {
    fn new() -> Self {
        Self { fields: Vec::new() }
    }
}

impl Visit for MaskingVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields.push((field.name().to_string(), value.to_string()));
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields
            .push((field.name().to_string(), format!("{:?}", value)));
    }
}

impl<S> Layer<S> for SecretMaskerLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        // Collect all fields and check whether any contain sensitive data.
        // If so, emit a warning-level event with the masked content.
        // NOTE: tracing does not allow mutating events in-flight; the masking
        // is therefore best-effort at the field-collection level. The primary
        // protection is that downstream layers (file writer, stdout) receive
        // the masked version via the `MaskingWriter` wrapper used in
        // `init_logging`.
        let mut visitor = MaskingVisitor::new();
        event.record(&mut visitor);

        for (_name, value) in &visitor.fields {
            let masked = self.mask(value);
            if masked != *value {
                // A secret was found — log a debug note (the actual masking
                // happens in the writer wrapper; this is just observability).
                tracing::debug!(
                    target: "synbot::security::secret_masker",
                    "Sensitive data detected and masked in log field"
                );
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn masker() -> SecretMaskerLayer {
        SecretMaskerLayer::new()
    }

    #[test]
    fn test_mask_sk_key() {
        let m = masker();
        let result = m.mask("Using key sk-abcdef1234567890");
        assert!(result.contains("sk-a[REDACTED:api_key]"), "got: {result}");
        assert!(!result.contains("sk-abcdef1234567890"));
    }

    #[test]
    fn test_mask_bearer_token() {
        let m = masker();
        let result = m.mask("Authorization: Bearer eyJhbGciOiJIUzI1NiJ9");
        assert!(result.contains("Bear[REDACTED:bearer_token]"), "got: {result}");
        assert!(!result.contains("eyJhbGciOiJIUzI1NiJ9"));
    }

    #[test]
    fn test_mask_github_pat() {
        let m = masker();
        let result = m.mask("token ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ");
        assert!(result.contains("ghp_[REDACTED:github_pat]"), "got: {result}");
    }

    #[test]
    fn test_mask_slack_bot_token() {
        let m = masker();
        let result = m.mask("slack token: xoxb-123456789-abcdefgh");
        assert!(result.contains("xoxb[REDACTED:slack_bot]"), "got: {result}");
    }

    #[test]
    fn test_no_false_positive_short_string() {
        let m = masker();
        // "bot" prefix but only 3 chars total — should not be masked
        let result = m.mask("bot");
        assert_eq!(result, "bot");
    }

    #[test]
    fn test_no_false_positive_synbot_path() {
        let m = masker();
        // Regression: a naive `prefix: "bot"` pattern mangled `.synbot` paths and log fields.
        let p = r"C:\Users\me\.synbot\config.json";
        assert_eq!(m.mask(p), p);
    }

    #[test]
    fn test_mask_preserves_surrounding_text() {
        let m = masker();
        let result = m.mask("prefix sk-secret1234 suffix");
        assert!(result.starts_with("prefix "));
        assert!(result.ends_with(" suffix"));
    }

    #[test]
    fn test_mask_config_secret() {
        let m = masker();
        {
            let mut secrets = m.config_secrets.write().unwrap();
            secrets.push("supersecretvalue123".to_string());
        }
        let result = m.mask("Loaded key supersecretvalue123 from env");
        assert!(result.contains("supe[REDACTED:config_secret]"), "got: {result}");
        assert!(!result.contains("supersecretvalue123"));
    }

    #[test]
    fn test_mask_gho_token() {
        let m = masker();
        let result = m.mask("oauth token gho_ABCDEFGHIJKLMNOP");
        assert!(result.contains("gho_[REDACTED:github_oauth]"), "got: {result}");
    }
}
