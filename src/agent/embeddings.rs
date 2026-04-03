//! HTTP embeddings for memory index (Ollama + OpenAI-compatible).

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::Config;

const DEFAULT_OLLAMA_BASE: &str = "http://127.0.0.1:11434";

/// Returns a zero vector of the configured dimension (used when provider is `none`).
pub fn stub_embedding(dim: u32) -> Vec<f32> {
    vec![0.0; dim as usize]
}

/// Async: fetch embedding for `text` using `config.memory` and `config.providers`.
pub async fn embed_text(config: &Config, text: &str) -> Result<Vec<f32>> {
    let dim = config.memory.embedding_dimensions;
    let prov = config.memory.embedding_provider.trim().to_ascii_lowercase();
    if prov.is_empty() || prov == "none" {
        return Ok(stub_embedding(dim));
    }

    match prov.as_str() {
        "ollama" => embed_ollama(config, text).await,
        "openai" => embed_openai_compatible(config, text).await,
        _ => anyhow::bail!(
            "memory.embeddingProvider must be none, ollama, or openai; got {:?}",
            config.memory.embedding_provider
        ),
    }
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embedding: Vec<f32>,
}

async fn embed_ollama(config: &Config, text: &str) -> Result<Vec<f32>> {
    let base = config
        .providers
        .ollama
        .api_base
        .as_deref()
        .unwrap_or(DEFAULT_OLLAMA_BASE)
        .trim_end_matches('/');
    let url = format!("{}/api/embeddings", base);
    let client = crate::appcontainer_dns::build_reqwest_client();
    let model = if config.memory.embedding_model.is_empty() || config.memory.embedding_model == "local/default" {
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

async fn embed_openai_compatible(config: &Config, text: &str) -> Result<Vec<f32>> {
    let base = config
        .providers
        .openai
        .api_base
        .as_deref()
        .unwrap_or("https://api.openai.com/v1")
        .trim_end_matches('/');
    let url = format!("{}/embeddings", base);
    let key = config.providers.openai.api_key.as_str();
    if key.is_empty() {
        anyhow::bail!("memory.embeddingProvider is openai but providers.openai.apiKey is empty");
    }
    let model = if config.memory.embedding_model.is_empty() || config.memory.embedding_model == "local/default" {
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
        .with_context(|| format!("openai embeddings POST {}", url))?;
    if !resp.status().is_success() {
        let t = resp.text().await.unwrap_or_default();
        anyhow::bail!("openai embeddings failed: {}", t);
    }
    let parsed: OpenAIEmbedResponse = resp.json().await.context("openai embeddings json")?;
    let emb = parsed
        .data
        .first()
        .map(|d| d.embedding.clone())
        .context("openai embeddings empty data")?;
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

/// Sync helper: embed inside an existing Tokio runtime (for `ContextBuilder` / memory backend).
pub fn try_embed_query_sync(config: &Config, query: &str) -> Option<Vec<f32>> {
    let prov = config.memory.embedding_provider.trim().to_ascii_lowercase();
    if prov.is_empty() || prov == "none" || query.trim().is_empty() {
        return None;
    }
    let handle = tokio::runtime::Handle::try_current().ok()?;
    match handle.block_on(embed_text(config, query)) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(error = %e, "memory query embedding failed; falling back to stub for hybrid search");
            Some(stub_embedding(config.memory.embedding_dimensions))
        }
    }
}
