//! Web tools: web_search (Brave), web_fetch.

use anyhow::Result;
use serde_json::{json, Value};

use crate::tools::DynTool;

// ---- WebSearch (Brave Search API) ----

pub struct WebSearchTool {
    pub api_key: String,
}

#[async_trait::async_trait]
impl DynTool for WebSearchTool {
    fn name(&self) -> &str { "web_search" }
    fn description(&self) -> &str { "Search the web using Brave Search API." }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "count": { "type": "integer", "default": 5 }
            },
            "required": ["query"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        if self.api_key.is_empty() {
            anyhow::bail!("Brave Search API key not configured");
        }
        let query = args["query"].as_str().unwrap_or("");
        let count = args["count"].as_u64().unwrap_or(5);

        let client = reqwest::Client::new();
        let resp = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", &self.api_key)
            .query(&[("q", query), ("count", &count.to_string())])
            .send()
            .await?
            .json::<Value>()
            .await?;

        let results = resp["web"]["results"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|r| {
                        format!(
                            "- {} ({})\n  {}",
                            r["title"].as_str().unwrap_or(""),
                            r["url"].as_str().unwrap_or(""),
                            r["description"].as_str().unwrap_or("")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|| "No results found.".into());

        Ok(results)
    }
}

// ---- WebFetch ----

pub struct WebFetchTool;

#[async_trait::async_trait]
impl DynTool for WebFetchTool {
    fn name(&self) -> &str { "web_fetch" }
    fn description(&self) -> &str { "Fetch and extract main content from a URL." }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" },
                "max_chars": { "type": "integer", "default": 50000 }
            },
            "required": ["url"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let url = args["url"].as_str().unwrap_or("");
        let max_chars = args["max_chars"].as_u64().unwrap_or(50000) as usize;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let text = client.get(url).send().await?.text().await?;

        let mut content = text;
        if content.len() > max_chars {
            content.truncate(max_chars);
            content.push_str("\n...[truncated]");
        }
        Ok(content)
    }
}
