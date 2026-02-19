//! Web tools: web_search (multi-backend), web_fetch.
//!
//! Supported search backends:
//!   - DuckDuckGo  — HTML scraping, no API key needed (default)
//!   - SearxNG     — self-hosted JSON API, requires `searxng_url`
//!   - Brave       — Brave Search REST API, requires `brave_api_key`

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::config::WebSearchBackend;
use crate::tools::DynTool;

// ---------------------------------------------------------------------------
// Shared HTTP client helper
// ---------------------------------------------------------------------------

fn build_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (compatible; synbot/1.0)")
        .build()?)
}

// ---------------------------------------------------------------------------
// Backend implementations
// ---------------------------------------------------------------------------

async fn search_duckduckgo(query: &str, count: usize) -> Result<Vec<SearchResult>> {
    let client = build_client()?;

    // DDG lite HTML endpoint — no JS, no API key
    let resp = client
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", query)])
        .send()
        .await
        .context("DuckDuckGo request failed")?
        .text()
        .await?;

    // Minimal HTML scraping: extract result blocks
    // Each result looks like:
    //   <a class="result__a" href="...">title</a>
    //   <a class="result__snippet">snippet</a>
    let mut results = Vec::new();

    // Split on result anchors
    for chunk in resp.split(r#"class="result__a""#).skip(1) {
        if results.len() >= count {
            break;
        }
        let url = extract_between(chunk, r#"href=""#, '"').unwrap_or_default();
        let title = extract_between(chunk, ">", '<').unwrap_or_default();
        let snippet = extract_between(chunk, r#"class="result__snippet""#, '<')
            .and_then(|s| extract_between(s, ">", '<'))
            .unwrap_or_default();

        // DDG wraps URLs in a redirect; unwrap uddg= param when present
        let clean_url = if url.contains("uddg=") {
            url.split("uddg=")
                .nth(1)
                .and_then(|s| s.split('&').next())
                .map(|s| urlencoding_decode(s))
                .unwrap_or_else(|| url.to_string())
        } else {
            url.to_string()
        };

        if !clean_url.is_empty() && !title.is_empty() {
            results.push(SearchResult {
                title: html_unescape(title.trim()),
                url: clean_url,
                snippet: html_unescape(snippet.trim()),
            });
        }
    }

    Ok(results)
}

async fn search_searxng(base_url: &str, query: &str, count: usize) -> Result<Vec<SearchResult>> {
    let client = build_client()?;
    let url = format!("{}/search", base_url.trim_end_matches('/'));

    let resp = client
        .get(&url)
        .query(&[
            ("q", query),
            ("format", "json"),
            ("categories", "general"),
        ])
        .send()
        .await
        .context("SearxNG request failed")?
        .json::<Value>()
        .await
        .context("SearxNG response parse failed")?;

    let results = resp["results"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .take(count)
                .map(|r| SearchResult {
                    title: r["title"].as_str().unwrap_or("").to_string(),
                    url: r["url"].as_str().unwrap_or("").to_string(),
                    snippet: r["content"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(results)
}

async fn search_brave(api_key: &str, query: &str, count: usize) -> Result<Vec<SearchResult>> {
    let client = build_client()?;

    let resp = client
        .get("https://api.search.brave.com/res/v1/web/search")
        .header("X-Subscription-Token", api_key)
        .query(&[("q", query), ("count", &count.to_string())])
        .send()
        .await
        .context("Brave Search request failed")?
        .json::<Value>()
        .await
        .context("Brave Search response parse failed")?;

    let results = resp["web"]["results"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .take(count)
                .map(|r| SearchResult {
                    title: r["title"].as_str().unwrap_or("").to_string(),
                    url: r["url"].as_str().unwrap_or("").to_string(),
                    snippet: r["description"].as_str().unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(results)
}

// ---------------------------------------------------------------------------
// Shared result type
// ---------------------------------------------------------------------------

struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

fn format_results(results: &[SearchResult]) -> String {
    if results.is_empty() {
        return "No results found.".to_string();
    }
    results
        .iter()
        .map(|r| format!("- {}\n  {}\n  {}", r.title, r.url, r.snippet))
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// WebSearchTool
// ---------------------------------------------------------------------------

pub struct WebSearchTool {
    pub backend: WebSearchBackend,
    /// Brave API key (used when backend == Brave)
    pub brave_api_key: String,
    /// SearxNG base URL (used when backend == SearxNG)
    pub searxng_url: String,
    /// Max results
    pub count: u32,
}

impl WebSearchTool {
    pub fn from_config(cfg: &crate::config::WebToolConfig) -> Self {
        // Backwards compat: if brave_api_key is set but backend is still default, use Brave.
        let backend = if cfg.search_backend == WebSearchBackend::DuckDuckGo
            && !cfg.brave_api_key.is_empty()
        {
            WebSearchBackend::Brave
        } else {
            cfg.search_backend.clone()
        };
        Self {
            backend,
            brave_api_key: cfg.brave_api_key.clone(),
            searxng_url: cfg.searxng_url.clone(),
            count: cfg.search_count,
        }
    }
}

#[async_trait::async_trait]
impl DynTool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web and return a list of relevant results (title, URL, snippet)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                },
                "count": {
                    "type": "integer",
                    "description": "Number of results to return (default 5)",
                    "default": 5
                }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, args: Value) -> Result<String> {
        let query = args["query"].as_str().unwrap_or("").trim();
        if query.is_empty() {
            anyhow::bail!("query must not be empty");
        }
        let count = args["count"].as_u64().unwrap_or(self.count as u64) as usize;

        let results = match &self.backend {
            WebSearchBackend::DuckDuckGo => search_duckduckgo(query, count).await?,
            WebSearchBackend::SearxNG => {
                if self.searxng_url.is_empty() {
                    anyhow::bail!("searxng_url is not configured");
                }
                search_searxng(&self.searxng_url, query, count).await?
            }
            WebSearchBackend::Brave => {
                if self.brave_api_key.is_empty() {
                    anyhow::bail!("brave_api_key is not configured");
                }
                search_brave(&self.brave_api_key, query, count).await?
            }
        };

        Ok(format_results(&results))
    }
}

// ---------------------------------------------------------------------------
// WebFetchTool
// ---------------------------------------------------------------------------

pub struct WebFetchTool;

#[async_trait::async_trait]
impl DynTool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch and return the raw content of a URL (HTML or text)."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "max_chars": {
                    "type": "integer",
                    "description": "Maximum characters to return (default 50000)",
                    "default": 50000
                }
            },
            "required": ["url"]
        })
    }

    async fn call(&self, args: Value) -> Result<String> {
        let raw_url = args["url"].as_str().unwrap_or("");
        let max_chars = args["max_chars"].as_u64().unwrap_or(50000) as usize;

        let url = crate::url_utils::normalize_http_url(raw_url)
            .with_context(|| format!("invalid or unsupported URL: {}", raw_url))?;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let mut content = client.get(&url).send().await?.text().await?;

        if content.len() > max_chars {
            content.truncate(max_chars);
            content.push_str("\n...[truncated]");
        }
        Ok(content)
    }
}

// ---------------------------------------------------------------------------
// Tiny HTML helpers (avoid pulling in a full HTML parser)
// ---------------------------------------------------------------------------

fn extract_between<'a>(s: &'a str, start: &str, end: char) -> Option<&'a str> {
    let pos = s.find(start)?;
    let after = &s[pos + start.len()..];
    let end_pos = after.find(end)?;
    Some(&after[..end_pos])
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn urlencoding_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte as char);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}
