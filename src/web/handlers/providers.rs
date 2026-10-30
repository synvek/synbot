//! `GET /api/providers` — provider metadata plus the configured state of every entry.
//!
//! The settings page used to render three bare inputs per provider (`apiKey`, `apiBase`,
//! `apiStyle`) and expect the user to know the rest: which wire format the name implies, whether a
//! key is required, what endpoint is used when `apiBase` is empty, and whether the name typed into
//! `mainAgent.provider` actually matches anything. This endpoint answers those questions from
//! [`crate::provider_catalog`], so the dashboard never has to duplicate the facts that
//! [`crate::rig_provider::build_completion_model`] dispatches on.
//!
//! Keys are never returned — only whether one is set. `providers.extra` is free-form, so unknown
//! names in use are reported separately instead of being silently treated as typos or as gateways.

use actix_web::{web, HttpResponse, Result};
use serde::Serialize;

use crate::config::{Config, ProviderApiStyle, ProviderEntry};
use crate::provider_catalog::{self, ProviderSpec};
use crate::web::handlers::api::ApiResponse;
use crate::web::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderView {
    key: &'static str,
    label: &'static str,
    aliases: &'static [&'static str],
    api_style: &'static str,
    requires_key: bool,
    local: bool,
    embeddings: bool,
    default_base: &'static str,
    docs_url: &'static str,
    /// `apiBase` from config, or `None` when the default is used.
    api_base: Option<String>,
    /// The endpoint a request would actually hit.
    api_base_effective: String,
    max_tokens_cap: Option<u32>,
    /// A key is present (always true for local runtimes, which need none).
    configured: bool,
    /// Where this provider is selected: `main` or an agent name.
    used_by: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtraProviderView {
    name: String,
    api_style: String,
    api_base: Option<String>,
    api_base_effective: Option<String>,
    max_tokens_cap: Option<u32>,
    configured: bool,
    used_by: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UnknownProviderView {
    name: String,
    used_by: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DefaultsView {
    provider: Option<String>,
    model: Option<String>,
}

/// The `providers.<key>` entry for a built-in key.
fn builtin_entry<'a>(cfg: &'a Config, key: &str) -> Option<&'a ProviderEntry> {
    Some(match key {
        "anthropic" => &cfg.providers.anthropic,
        "openai" => &cfg.providers.openai,
        "gemini" => &cfg.providers.gemini,
        "openrouter" => &cfg.providers.openrouter,
        "deepseek" => &cfg.providers.deepseek,
        "moonshot" => &cfg.providers.moonshot,
        "kimiCode" => &cfg.providers.kimi_code,
        "ollama" => &cfg.providers.ollama,
        _ => return None,
    })
}

/// Every place a provider name is selected, as `(name, owner)`.
fn selections(cfg: &Config) -> Vec<(&str, String)> {
    let mut out = Vec::new();
    let main = cfg.main_agent.provider.trim();
    if !main.is_empty() {
        out.push((main, "main".to_string()));
    }
    for agent in &cfg.main_agent.agents {
        if let Some(name) = agent.provider.as_deref() {
            out.push((name, agent.name.clone()));
        }
    }
    out
}

fn used_by(spec: &ProviderSpec, picks: &[(&str, String)]) -> Vec<String> {
    picks
        .iter()
        .filter(|(name, _)| {
            provider_catalog::by_provider_name(name).map(|found| found.key) == Some(spec.key)
        })
        .map(|(_, owner)| owner.clone())
        .collect()
}

fn extra_used_by(name: &str, picks: &[(&str, String)]) -> Vec<String> {
    picks
        .iter()
        .filter(|(picked, _)| {
            picked.trim() == name && provider_catalog::by_provider_name(picked).is_none()
        })
        .map(|(_, owner)| owner.clone())
        .collect()
}

/// Names in `mainAgent` / agents that resolve to neither a built-in nor an `extra` key.
fn unknown_in_use(cfg: &Config, picks: &[(&str, String)]) -> Vec<UnknownProviderView> {
    let mut out: Vec<UnknownProviderView> = Vec::new();
    for (name, owner) in picks {
        let trimmed = name.trim();
        if trimmed.is_empty() || provider_catalog::by_provider_name(trimmed).is_some() {
            continue;
        }
        let known_extra = cfg.providers.extra.contains_key(trimmed);
        if known_extra {
            continue;
        }
        match out.iter_mut().find(|view| view.name == trimmed) {
            Some(view) => view.used_by.push(owner.clone()),
            None => out.push(UnknownProviderView {
                name: trimmed.to_string(),
                used_by: vec![owner.clone()],
            }),
        }
    }
    out
}

/// GET /api/providers
pub async fn list_providers(state: web::Data<AppState>) -> Result<HttpResponse> {
    let cfg = state.config.read().await;
    let picks = selections(&cfg);

    let providers: Vec<ProviderView> = provider_catalog::PROVIDERS
        .iter()
        .filter_map(|spec| {
            let entry = builtin_entry(&cfg, spec.key)?;
            let has_base = entry
                .api_base
                .as_deref()
                .map(str::trim)
                .is_some_and(|base| !base.is_empty());
            Some(ProviderView {
                key: spec.key,
                label: spec.label,
                aliases: spec.aliases,
                api_style: spec.api_style,
                requires_key: spec.requires_key,
                local: spec.local,
                embeddings: spec.embeddings,
                default_base: spec.default_base,
                docs_url: spec.docs_url,
                api_base: has_base.then(|| entry.api_base.clone().unwrap_or_default()),
                api_base_effective: provider_catalog::effective_api_base(
                    spec,
                    entry.api_base.as_deref(),
                ),
                max_tokens_cap: entry.max_tokens_cap,
                // A local runtime answers without a key, so `configured` would otherwise be false.
                configured: spec.local || !entry.api_key.trim().is_empty(),
                used_by: used_by(spec, &picks),
            })
        })
        .collect();

    let mut extra: Vec<ExtraProviderView> = cfg
        .providers
        .extra
        .iter()
        .map(|(name, entry)| ExtraProviderView {
            name: name.clone(),
            api_style: match entry.api_style {
                ProviderApiStyle::Openai => "openai".to_string(),
                ProviderApiStyle::Anthropic => "anthropic".to_string(),
            },
            api_base: entry
                .api_base
                .as_deref()
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .map(str::to_string),
            api_base_effective: entry
                .api_base
                .as_deref()
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .map(|base| base.trim_end_matches('/').to_string()),
            max_tokens_cap: entry.max_tokens_cap,
            configured: !entry.api_key.trim().is_empty(),
            used_by: extra_used_by(name, &picks),
        })
        .collect();
    extra.sort_by(|a, b| a.name.cmp(&b.name));

    // `mainAgent.provider` / `model` are plain strings, where empty means "not set".
    let non_empty = |value: &str| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    };

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "providers": providers,
            "extra": extra,
            "unknownInUse": unknown_in_use(&cfg, &picks),
            "defaults": DefaultsView {
                provider: non_empty(&cfg.main_agent.provider),
                model: non_empty(&cfg.main_agent.model),
            },
        }))),
    )
}
