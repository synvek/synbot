//! Memory embeddings use `memory.embeddingProvider` as a provider **name** (e.g. `ollama`, `openai`, `deepseek`, or `providers.extra` keys).
//! [`crate::config::resolve_provider`] supplies `apiBase` / `apiKey` — same mechanism as chat, but the name is chosen independently from the agent’s dialogue provider.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::{self, Config};

const DEFAULT_OLLAMA_BASE: &str = "http://127.0.0.1:11434";

/// Returns a zero vector of the configured dimension (used when `memory.embeddingProvider` is `none` / empty, or for providers without a wired embedding API here).
pub fn stub_embedding(dim: u32) -> Vec<f32> {
    vec![0.0; dim as usize]
}

/// Build `/v1/embeddings` URL from an API base (OpenAI-compatible providers).
fn openai_compatible_embeddings_url(api_base: &str) -> String {
    let b = api_base.trim().trim_end_matches('/');
    if b.is_empty() {
        return "https://api.openai.com/v1/embeddings".to_string();
    }
    if b.ends_with("/embeddings") {
        return b.to_string();
    }
    if b.ends_with("/v1") {
        format!("{}/embeddings", b)
    } else {
        format!("{}/v1/embeddings", b)
    }
}

fn embedding_provider_name(config: &Config) -> &str {
    config.memory.embedding_provider.trim()
}

/// Async: fetch embedding using `memory.embeddingProvider` + [`resolve_provider`] (not the chat agent’s provider).
pub async fn embed_text(config: &Config, text: &str) -> Result<Vec<f32>> {
    let dim = config.memory.embedding_dimensions;
    let prov = embedding_provider_name(config);
    if prov.is_empty() || prov.eq_ignore_ascii_case("none") {
        return Ok(stub_embedding(dim));
    }
    let lower = prov.to_lowercase();

    if lower.contains("ollama") {
        return embed_ollama(config, prov, text).await;
    }

    if lower.contains("anthropic")
        || lower.contains("claude")
        || lower.contains("gemini")
    {
        return Ok(stub_embedding(dim));
    }

    embed_openai_compatible_for_provider(config, prov, text).await
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embedding: Vec<f32>,
}

async fn embed_ollama(config: &Config, provider_name: &str, text: &str) -> Result<Vec<f32>> {
    let (_, base_opt) = config::resolve_provider(config, provider_name);
    let base = base_opt
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(DEFAULT_OLLAMA_BASE)
        .trim_end_matches('/');
    let url = format!("{}/api/embeddings", base);
    let client = crate::appcontainer_dns::build_reqwest_client();
    let model = if config.memory.embedding_model.is_empty()
        || config.memory.embedding_model == "local/default"
    {
        "nomic-embed-text"
    } else {
        config.memory.embedding_model.as_str()
    };
    let body = serde_json::json!({ "model": model, "prompt": text });
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("ollama embeddings POST {}", url))?;
    if !resp.status().is_success() {
        let t = resp.text().await.unwrap_or_default();
        anyhow::bail!("ollama embeddings failed: {}", t);
    }
    let parsed: OllamaEmbedResponse = resp.json().await.context("ollama embeddings json")?;
    validate_dim(parsed.embedding.len(), config.memory.embedding_dimensions)?;
    Ok(parsed.embedding)
}

#[derive(Deserialize)]
struct OpenAIEmbedResponse {
    data: Vec<OpenAIEmbedItem>,
}
#[derive(Deserialize)]
struct OpenAIEmbedItem {
    embedding: Vec<f32>,
}

async fn embed_openai_compatible_for_provider(
    config: &Config,
    provider_name: &str,
    text: &str,
) -> Result<Vec<f32>> {
    let (key, base_opt) = config::resolve_provider(config, provider_name);
    if key.trim().is_empty() {
        anyhow::bail!(
            "no API key for embedding provider '{}'; set the matching providers.* entry",
            provider_name
        );
    }
    let base = base_opt
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("https://api.openai.com/v1");
    let url = openai_compatible_embeddings_url(base);
    let model = if config.memory.embedding_model.is_empty()
        || config.memory.embedding_model == "local/default"
    {
        "text-embedding-3-small"
    } else {
        config.memory.embedding_model.as_str()
    };
    let client = crate::appcontainer_dns::build_reqwest_client();
    let body = serde_json::json!({
        "model": model,
        "input": text,
    });
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", key))
        .json(&body)
        .send()
        .await
        .with_context(|| format!("openai-compatible embeddings POST {}", url))?;
    if !resp.status().is_success() {
        let t = resp.text().await.unwrap_or_default();
        anyhow::bail!("embeddings failed: {}", t);
    }
    let parsed: OpenAIEmbedResponse = resp.json().await.context("embeddings json")?;
    let emb = parsed
        .data
        .first()
        .map(|d| d.embedding.clone())
        .context("embeddings empty data")?;
    validate_dim(emb.len(), config.memory.embedding_dimensions)?;
    Ok(emb)
}

fn validate_dim(got: usize, expected: u32) -> Result<()> {
    if got != expected as usize {
        anyhow::bail!(
            "embedding dimension mismatch: got {} floats, memory.embeddingDimensions is {} — use a matching model or update embeddingDimensions",
            got,
            expected
        );
    }
    Ok(())
}

/// Run [`embed_text`] synchronously for hybrid search (`MemoryBackend` / `ContextBuilder` sync paths).
///
/// Cannot call [`tokio::runtime::Runtime::block_on`] on the **current** thread when it is already a
/// Tokio worker (multi-thread or nested runtimes both trip `enter_runtime`). Spawning a **new std
/// thread** gives a clean thread with no Tokio context; we build a short-lived `current_thread`
/// runtime there and drive the HTTP client.
fn embed_text_on_dedicated_runtime(config: &Config, query: &str) -> Result<Vec<f32>> {
    let config = config.clone();
    let query = query.to_string();
    std::thread::Builder::new()
        .name("synbot-embed-query".into())
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build embedding runtime")
                .block_on(embed_text(&config, &query))
        })
        .context("spawn embedding thread")?
        .join()
        .map_err(|_| anyhow::anyhow!("embedding thread panicked"))?
}

/// Sync helper: query embedding for hybrid search.
pub fn try_embed_query_sync(config: &Config, query: &str) -> Option<Vec<f32>> {
    let prov = embedding_provider_name(config);
    if prov.is_empty() || prov.eq_ignore_ascii_case("none") || query.trim().is_empty() {
        return None;
    }
    let lower = prov.to_lowercase();
    if lower.contains("anthropic") || lower.contains("claude") || lower.contains("gemini") {
        return None;
    }
    match embed_text_on_dedicated_runtime(config, query) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(error = %e, "memory query embedding failed; falling back to stub for hybrid search");
            Some(stub_embedding(config.memory.embedding_dimensions))
        }
    }
}
