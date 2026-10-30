//! Static metadata for the built-in providers.
//!
//! The config structs only store *overrides* (`apiKey`, `apiBase`, `apiStyle`, `maxTokensCap`).
//! Everything else a settings UI needs to explain a provider — which wire format it speaks,
//! whether a key is mandatory, the endpoint used when `apiBase` is empty, and where the current
//! model ids live — is a property of the provider, not of the user's config.
//!
//! Keeping those facts here (instead of hard-coding them in the dashboard) means they cannot drift
//! from [`crate::rig_provider::build_completion_model`], which dispatches on the same names, and
//! from [`crate::config::resolve_provider`], whose matching order is mirrored by
//! [`NAME_PRECEDENCE`].

/// Static description of one built-in provider.
pub struct ProviderSpec {
    /// Key under `providers` in `config.json` (camelCase, e.g. `kimiCode`).
    pub key: &'static str,
    /// Brand name. Not translated — it is a proper noun.
    pub label: &'static str,
    /// Substrings matched by [`crate::config::resolve_provider`].
    pub aliases: &'static [&'static str],
    /// Wire format: `"openai"` (chat completions) or `"anthropic"` (messages).
    pub api_style: &'static str,
    /// The provider cannot answer without a key; local runtimes do not need one.
    pub requires_key: bool,
    /// Runs on the user's machine: no cloud key, no outbound network.
    pub local: bool,
    /// Usable as `memory.embeddingProvider` (Anthropic/Gemini are chat-only here).
    pub embeddings: bool,
    /// Endpoint used when `apiBase` is omitted.
    pub default_base: &'static str,
    /// Where the provider publishes its current model ids.
    pub docs_url: &'static str,
}

/// Built-in providers, in the order the settings page lists them.
pub const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        key: "anthropic",
        label: "Anthropic",
        aliases: &["anthropic", "claude"],
        api_style: "anthropic",
        requires_key: true,
        local: false,
        embeddings: false,
        default_base: "https://api.anthropic.com",
        docs_url: "https://docs.anthropic.com/en/docs/about-claude/models",
    },
    ProviderSpec {
        key: "openai",
        label: "OpenAI",
        aliases: &["openai"],
        api_style: "openai",
        requires_key: true,
        local: false,
        embeddings: true,
        default_base: "https://api.openai.com/v1",
        docs_url: "https://platform.openai.com/docs/models",
    },
    ProviderSpec {
        key: "gemini",
        label: "Gemini",
        aliases: &["gemini"],
        api_style: "openai",
        requires_key: true,
        local: false,
        embeddings: false,
        default_base: "https://generativelanguage.googleapis.com",
        docs_url: "https://ai.google.dev/gemini-api/docs/models",
    },
    ProviderSpec {
        key: "openrouter",
        label: "OpenRouter",
        aliases: &["openrouter"],
        api_style: "openai",
        requires_key: true,
        local: false,
        embeddings: true,
        default_base: "https://openrouter.ai/api/v1",
        docs_url: "https://openrouter.ai/models",
    },
    ProviderSpec {
        key: "deepseek",
        label: "DeepSeek",
        aliases: &["deepseek"],
        api_style: "openai",
        requires_key: true,
        local: false,
        embeddings: true,
        default_base: "https://api.deepseek.com",
        docs_url: "https://api-docs.deepseek.com/quick_start/pricing",
    },
    ProviderSpec {
        key: "moonshot",
        label: "Moonshot",
        aliases: &["moonshot"],
        api_style: "openai",
        requires_key: true,
        local: false,
        embeddings: true,
        default_base: "https://api.moonshot.cn/v1",
        docs_url: "https://platform.moonshot.cn/docs",
    },
    ProviderSpec {
        key: "kimiCode",
        label: "Kimi Code",
        aliases: &["kimi"],
        api_style: "openai",
        requires_key: true,
        local: false,
        embeddings: true,
        default_base: "https://api.kimi.com/coding",
        docs_url: "https://platform.moonshot.cn/docs",
    },
    ProviderSpec {
        key: "ollama",
        label: "Ollama",
        aliases: &["ollama"],
        api_style: "openai",
        requires_key: false,
        local: true,
        embeddings: true,
        default_base: "http://localhost:11434",
        docs_url: "https://ollama.com/library",
    },
];

/// Name precedence, mirroring `config::resolve_provider`: the first entry whose alias is a
/// substring of the configured name wins. `openrouter` therefore has to precede `openai`, and
/// `kimi` has to precede `ollama` (a "kimi via ollama" style name is dispatched to Kimi Code).
const NAME_PRECEDENCE: &[&str] = &[
    "openrouter",
    "anthropic",
    "openai",
    "gemini",
    "deepseek",
    "moonshot",
    "kimiCode",
    "ollama",
];

/// Look up a spec by its `providers` config key (exact, case-sensitive: the JSON is camelCase).
pub fn by_key(key: &str) -> Option<&'static ProviderSpec> {
    PROVIDERS.iter().find(|spec| spec.key == key)
}

/// Look up a spec by a `mainAgent.provider` / `agents[].provider` name.
///
/// Matching is substring based and follows [`NAME_PRECEDENCE`], so `"openrouter/anthropic/claude"`
/// resolves to OpenRouter and `"claude-3-5-sonnet"` to Anthropic. `None` means "not a built-in":
/// the name is either a `providers.extra` key (plugin or gateway) or a typo, which the caller
/// cannot tell apart — `extra` is free-form by design.
pub fn by_provider_name(name: &str) -> Option<&'static ProviderSpec> {
    let lower = name.trim().to_lowercase();
    NAME_PRECEDENCE.iter().find_map(|key| {
        let spec = by_key(key)?;
        spec.aliases
            .iter()
            .any(|alias| lower.contains(alias))
            .then_some(spec)
    })
}

/// The base URL a request will actually use: the configured override (trimmed, without a trailing
/// slash) or the provider default.
pub fn effective_api_base(spec: &ProviderSpec, api_base: Option<&str>) -> String {
    api_base
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .map(|base| base.trim_end_matches('/').to_string())
        .unwrap_or_else(|| spec.default_base.trim_end_matches('/').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_has_a_precedence_entry_and_unique_key() {
        for spec in PROVIDERS {
            assert!(
                NAME_PRECEDENCE.contains(&spec.key),
                "{} is missing from NAME_PRECEDENCE, so name lookups would skip it",
                spec.key
            );
            assert_eq!(by_key(spec.key).map(|s| s.key), Some(spec.key));
        }
        let mut keys: Vec<&str> = PROVIDERS.iter().map(|spec| spec.key).collect();
        let expected = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), expected, "provider keys must be unique");
    }

    #[test]
    fn metadata_is_internally_consistent() {
        for spec in PROVIDERS {
            assert!(matches!(spec.api_style, "openai" | "anthropic"));
            assert!(
                spec.default_base.starts_with("http://")
                    || spec.default_base.starts_with("https://")
            );
            assert!(!spec.default_base.ends_with('/'));
            assert!(
                spec.requires_key ^ spec.local,
                "{}: key/local disagree",
                spec.key
            );
            assert!(!spec.aliases.is_empty());
        }
    }

    #[test]
    fn provider_names_resolve_in_the_same_order_as_resolve_provider() {
        // `openrouter/anthropic/claude` must stay OpenRouter even though it contains `anthropic`.
        assert_eq!(
            by_provider_name("openrouter/anthropic/claude").map(|s| s.key),
            Some("openrouter")
        );
        assert_eq!(
            by_provider_name("claude-3-5-sonnet").map(|s| s.key),
            Some("anthropic")
        );
        // Names, not model ids: `gpt-4o` alone says nothing about the endpoint.
        assert_eq!(by_provider_name("openai").map(|s| s.key), Some("openai"));
        assert!(by_provider_name("gpt-4o").is_none());
        assert_eq!(
            by_provider_name("gemini-2.5-pro").map(|s| s.key),
            Some("gemini")
        );
        assert_eq!(
            by_provider_name("deepseek-chat").map(|s| s.key),
            Some("deepseek")
        );
        assert_eq!(by_provider_name("kimi-k2").map(|s| s.key), Some("kimiCode"));
        assert_eq!(
            by_provider_name("ollama-local").map(|s| s.key),
            Some("ollama")
        );
        // `extra` keys are free-form, so anything else stays unknown instead of guessing.
        assert!(by_provider_name("minimax").is_none());
        assert!(by_provider_name("").is_none());
    }

    #[test]
    fn effective_base_prefers_the_override_and_normalises_slashes() {
        let anthropic = by_key("anthropic").expect("anthropic is built in");
        assert_eq!(
            effective_api_base(anthropic, Some(" https://gateway.example.com/v1/ ")),
            "https://gateway.example.com/v1"
        );
        assert_eq!(
            effective_api_base(anthropic, Some("   ")),
            anthropic.default_base
        );
        assert_eq!(effective_api_base(anthropic, None), anthropic.default_base);

        let ollama = by_key("ollama").expect("ollama is built in");
        assert_eq!(
            effective_api_base(ollama, Some("http://192.168.1.9:11434/")),
            "http://192.168.1.9:11434"
        );
    }
}
