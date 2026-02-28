//! Rig-core provider bridge: build completion models from provider name without rig-dyn.
//!
//! Uses rig-core 0.30 providers directly and exposes a unified `SynbotCompletionModel` trait
//! so the rest of the app can use `Arc<dyn SynbotCompletionModel>`.
//!
//! # AppContainer DNS
//!
//! In Windows AppContainer, system DNS is unavailable (hickory-dns reports "no connections
//! available"). Each provider client is built via `ClientBuilder::http_client(...)` so we can
//! inject a `reqwest::Client` with an explicit Google DNS resolver (8.8.8.8) when running inside
//! the sandbox. Outside AppContainer the default reqwest client is used unchanged.

use anyhow::{anyhow, Result};
use rig::client::CompletionClient;
use rig::client::Nothing;
use rig::completion::request::{
    CompletionError, CompletionRequest, CompletionResponse,
};
use rig::completion::CompletionModel;
use rig::message::{AssistantContent, UserContent};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Provider-agnostic completion model trait (replaces rig_dyn::CompletionModel).
pub trait SynbotCompletionModel: Send + Sync {
    fn completion(
        &self,
        request: CompletionRequest,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<CompletionResponse<()>, CompletionError>> + Send + '_,
        >,
    >;
}

// ---------------------------------------------------------------------------
// Provider factory and registry (for plugins)
// ---------------------------------------------------------------------------

/// Factory that builds a completion model from provider name and credentials.
/// Plugins implement this trait and register with [ProviderRegistry].
pub trait ProviderFactory: Send + Sync {
    fn build(
        &self,
        provider_name: &str,
        model_name: &str,
        api_key: &str,
        api_base: Option<&str>,
    ) -> Result<Arc<dyn SynbotCompletionModel>>;
}

/// Registry of provider names to factories. Built-in providers are registered at first use;
/// plugins can register additional providers via [default_registry].
pub struct ProviderRegistry {
    factories: HashMap<String, Arc<dyn ProviderFactory>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Register a factory for the given provider name(s). Names are matched case-insensitively at build time.
    pub fn register(&mut self, name: &str, factory: Arc<dyn ProviderFactory>) {
        self.factories.insert(name.to_lowercase(), factory);
    }

    /// Build a completion model using the registered factory for this provider name.
    pub fn build(
        &self,
        provider_name: &str,
        model_name: &str,
        api_key: &str,
        api_base: Option<&str>,
    ) -> Result<Arc<dyn SynbotCompletionModel>> {
        let key = provider_name.trim().to_lowercase();
        let factory = self
            .factories
            .get(&key)
            .or_else(|| {
                // Match substrings for backward compatibility (e.g. "claude" -> anthropic)
                self.factories
                    .iter()
                    .find(|(k, _)| key.contains(k.as_str()))
                    .map(|(_, v)| v)
            })
            .ok_or_else(|| anyhow!("Unknown provider: {}", provider_name))?;
        factory.build(provider_name, model_name, api_key, api_base)
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the default global registry, with built-in providers registered on first use.
/// Plugins can call `default_registry().write().unwrap().register("name", factory)` to add providers.
pub fn default_registry() -> &'static std::sync::RwLock<ProviderRegistry> {
    static R: std::sync::OnceLock<std::sync::RwLock<ProviderRegistry>> = std::sync::OnceLock::new();
    R.get_or_init(|| {
        let mut reg = ProviderRegistry::new();
        reg.register_builtins();
        std::sync::RwLock::new(reg)
    })
}

/// Built-in provider factory: implements the original if-else dispatch logic.
struct BuiltinProviderFactory;

impl ProviderFactory for BuiltinProviderFactory {
    fn build(
        &self,
        provider_name: &str,
        model_name: &str,
        api_key: &str,
        api_base: Option<&str>,
    ) -> Result<Arc<dyn SynbotCompletionModel>> {
        build_completion_model_builtin(provider_name, model_name, api_key, api_base)
    }
}

impl ProviderRegistry {
    /// Register all built-in providers (OpenAI, Anthropic, DeepSeek, etc.).
    pub fn register_builtins(&mut self) {
        let factory: Arc<dyn ProviderFactory> = Arc::new(BuiltinProviderFactory);
        for name in &[
            "openai",
            "anthropic",
            "claude",
            "gemini",
            "deepseek",
            "moonshot",
            "ollama",
            "kimi",
            "kimi_code",
            "openrouter",
        ] {
            self.register(name, Arc::clone(&factory));
        }
    }
}

// ---------------------------------------------------------------------------
// Extra providers: config-only OpenAI-compatible providers (no code change)
// ---------------------------------------------------------------------------

/// Built-in provider names that must not be overridden by config.providers.extra.
const BUILTIN_PROVIDER_NAMES: &[&str] = &[
    "openai",
    "anthropic",
    "claude",
    "gemini",
    "deepseek",
    "moonshot",
    "ollama",
    "kimi",
    "kimi_code",
    "openrouter",
];

const DEFAULT_OPENAI_API_BASE: &str = "https://api.openai.com/v1";

/// Factory that builds an OpenAI Chat Completions–compatible client for any base URL.
/// Used for config.providers.extra entries so users can add providers (e.g. Minimax, proxies) by config only.
struct OpenAiCompatibleProviderFactory;

impl ProviderFactory for OpenAiCompatibleProviderFactory {
    fn build(
        &self,
        _provider_name: &str,
        model_name: &str,
        api_key: &str,
        api_base: Option<&str>,
    ) -> Result<Arc<dyn SynbotCompletionModel>> {
        let base = api_base
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .unwrap_or_else(|| DEFAULT_OPENAI_API_BASE.to_string());
        let http = crate::appcontainer_dns::build_reqwest_client();
        type RC = reqwest::Client;
        let client = rig::providers::openai::CompletionsClient::<RC>::builder()
            .api_key(api_key.to_string())
            .http_client(http)
            .base_url(&base)
            .build()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let m = client.completion_model(model_name.to_string());
        Ok(Arc::new(OpenAiCompletionsModel(client, m)) as Arc<dyn SynbotCompletionModel>)
    }
}

/// Register an OpenAI-compatible provider for each key in `config.providers.extra` that is not a built-in.
/// Allows users to add new OpenAI-compatible providers (e.g. Minimax, local proxies) by config only.
pub fn register_extra_openai_compatible_providers(cfg: &crate::config::Config) {
    let factory: Arc<dyn ProviderFactory> = Arc::new(OpenAiCompatibleProviderFactory);
    let mut reg = default_registry()
        .write()
        .expect("provider registry lock");
    for name in cfg.providers.extra.keys() {
        let key = name.trim().to_lowercase();
        if BUILTIN_PROVIDER_NAMES.iter().any(|n| key == *n) {
            continue;
        }
        reg.register(name, Arc::clone(&factory));
    }
}

/// Build an Arc<dyn SynbotCompletionModel> from provider name, model name, API key and optional base URL.
/// Uses the default provider registry (built-ins + any plugin-registered providers).
pub fn build_completion_model(
    provider_name: &str,
    model_name: &str,
    api_key: &str,
    api_base: Option<&str>,
) -> Result<Arc<dyn SynbotCompletionModel>> {
    default_registry()
        .read()
        .map_err(|e| anyhow!("provider registry lock: {}", e))?
        .build(provider_name, model_name, api_key, api_base)
}

/// Internal: built-in provider dispatch (used by BuiltinProviderFactory).
fn build_completion_model_builtin(
    provider_name: &str,
    model_name: &str,
    api_key: &str,
    api_base: Option<&str>,
) -> Result<Arc<dyn SynbotCompletionModel>> {
    // In app sandbox (Windows AppContainer or macOS nono), use client with Google DNS and
    // (on macOS) rustls+webpki only; otherwise use default reqwest client.
    let mk_http = || crate::appcontainer_dns::build_reqwest_client();

    let lower = provider_name.to_lowercase();
    // Turbofish `<reqwest::Client>` pins H so the compiler knows the initial http client type
    // before .http_client(mk_http()) swaps it in.
    type RC = reqwest::Client;
    const ANTHROPIC_API_BASE: &str = "https://api.anthropic.com";
    const OPENAI_API_BASE: &str = "https://api.openai.com/v1";
    const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com";
    let model = if lower.contains("anthropic") || lower.contains("claude") {
        let base = api_base
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .unwrap_or_else(|| ANTHROPIC_API_BASE.to_string());
        let client = rig::providers::anthropic::Client::<RC>::builder()
            .api_key(api_key.to_string())
            .http_client(mk_http())
            .base_url(&base)
            .build()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let m = client.completion_model(model_name.to_string());
        Arc::new(AnthropicModel(client, m)) as Arc<dyn SynbotCompletionModel>
    } else if lower.contains("gemini") {
        let base = api_base
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .unwrap_or_else(|| GEMINI_API_BASE.to_string());
        let client = rig::providers::gemini::Client::<RC>::builder()
            .api_key(api_key.to_string())
            .http_client(mk_http())
            .base_url(&base)
            .build()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let m = client.completion_model(model_name.to_string());
        Arc::new(GeminiModel(client, m)) as Arc<dyn SynbotCompletionModel>
    } else if lower.contains("deepseek") {
        let base = api_base
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| DEEPSEEK_API_BASE.to_string());
        let m = DeepSeekDirectModel::new(mk_http(), api_key.to_string(), model_name.to_string(), base);
        Arc::new(m) as Arc<dyn SynbotCompletionModel>
    } else if lower.contains("moonshot") {
        let client = rig::providers::moonshot::Client::<RC>::builder()
            .api_key(api_key.to_string())
            .http_client(mk_http())
            .build()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let m = client.completion_model(model_name.to_string());
        Arc::new(MoonshotModel(client, m)) as Arc<dyn SynbotCompletionModel>
    } else if lower.contains("ollama") {
        let client = rig::providers::ollama::Client::<RC>::builder()
            .api_key(Nothing)
            .http_client(mk_http())
            .build()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let m = client.completion_model(model_name.to_string());
        Arc::new(OllamaModel(client, m)) as Arc<dyn SynbotCompletionModel>
    } else if lower.contains("kimi") {
        // Kimi Code: use direct HTTP so we control URL (must be .../v1/chat/completions) and User-Agent.
        let base = api_base
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .unwrap_or_else(|| "https://api.kimi.com/coding".to_string());
        let http_kimi = crate::appcontainer_dns::build_reqwest_client_with_user_agent("KimiCLI/1.3");
        let m = KimiCodeDirectModel::new(http_kimi, api_key.to_string(), model_name.to_string(), base);
        Arc::new(m) as Arc<dyn SynbotCompletionModel>
    } else if lower.contains("openrouter") {
        // OpenRouter: use direct HTTP with /chat/completions. rig's OpenAI client uses /v1/responses
        // which OpenRouter does not support.
        const OPENROUTER_API_BASE: &str = "https://openrouter.ai/api/v1";
        let base = api_base
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .unwrap_or_else(|| OPENROUTER_API_BASE.to_string());
        let m = OpenRouterDirectModel::new(mk_http(), api_key.to_string(), model_name.to_string(), base);
        Arc::new(m) as Arc<dyn SynbotCompletionModel>
    } else {
        // OpenAI or default: use Responses API (/v1/responses) for official OpenAI;
        // use Chat Completions API (/v1/chat/completions) for custom base (proxy/compatible).
        let base = api_base
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().trim_end_matches('/').to_string())
            .unwrap_or_else(|| OPENAI_API_BASE.to_string());
        let use_responses_api = api_base
            .map(|b| b.trim().trim_end_matches('/') == OPENAI_API_BASE)
            .unwrap_or(true);
        if use_responses_api {
            let client = rig::providers::openai::Client::<RC>::builder()
                .api_key(api_key.to_string())
                .http_client(mk_http())
                .base_url(&base)
                .build()
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            let m = client.completion_model(model_name.to_string());
            Arc::new(OpenAiModel(client, m)) as Arc<dyn SynbotCompletionModel>
        } else {
            let client = rig::providers::openai::CompletionsClient::<RC>::builder()
                .api_key(api_key.to_string())
                .http_client(mk_http())
                .base_url(&base)
                .build()
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            let m = client.completion_model(model_name.to_string());
            Arc::new(OpenAiCompletionsModel(client, m)) as Arc<dyn SynbotCompletionModel>
        }
    };
    Ok(model)
}

macro_rules! impl_model {
    ($name:ident, $client:ty) => {
        struct $name(
            $client,
            <$client as CompletionClient>::CompletionModel,
        );
        impl SynbotCompletionModel for $name {
            fn completion(
                &self,
                request: CompletionRequest,
            ) -> Pin<
                Box<
                    dyn Future<Output = Result<CompletionResponse<()>, CompletionError>>
                        + Send
                        + '_,
                >,
            > {
                let m = &self.1;
                let fut = async move {
                    let r = m.completion(request).await?;
                    Ok(CompletionResponse {
                        choice: r.choice,
                        usage: r.usage,
                        raw_response: (),
                    })
                };
                Box::pin(fut)
            }
        }
    };
}

impl_model!(OpenAiModel, rig::providers::openai::Client);
impl_model!(OpenAiCompletionsModel, rig::providers::openai::CompletionsClient);
impl_model!(AnthropicModel, rig::providers::anthropic::Client);
impl_model!(GeminiModel, rig::providers::gemini::Client);
impl_model!(MoonshotModel, rig::providers::moonshot::Client);
impl_model!(OllamaModel, rig::providers::ollama::Client);

// ---------------------------------------------------------------------------
// DeepSeek: custom implementation that correctly handles reasoning_content
// in multi-turn tool-call conversations.
//
// The rig-core 0.30 DeepSeek provider splits a single assistant message that
// contains both Reasoning and ToolCall into two separate messages, then its
// merge logic only fixes the *last* pair — leaving earlier turns without
// reasoning_content on the tool-call message, which causes a 400 error.
//
// We bypass rig's serialization entirely and build the JSON payload ourselves,
// following the DeepSeek API spec:
//   - Within a turn (tool-call loop): reasoning_content + tool_calls in ONE message
//   - Across turns (new user question): drop reasoning_content from history
// ---------------------------------------------------------------------------

const DEEPSEEK_API_BASE: &str = "https://api.deepseek.com";

struct DeepSeekDirectModel {
    http: reqwest::Client,
    api_key: String,
    api_base: String,
    model: String,
}

impl DeepSeekDirectModel {
    fn new(http: reqwest::Client, api_key: String, model: String, api_base: String) -> Self {
        Self {
            http,
            api_key,
            api_base: if api_base.trim().is_empty() {
                DEEPSEEK_API_BASE.to_string()
            } else {
                api_base.trim().to_string()
            },
            model,
        }
    }

    /// Convert a rig `CompletionRequest` into the DeepSeek JSON body.
    /// Key rule: for each rig `Message::Assistant`, collect Reasoning text and
    /// ToolCalls together into ONE JSON assistant message with both
    /// `reasoning_content` and `tool_calls` fields.
    fn build_request_body(&self, req: &CompletionRequest) -> Value {
        let mut messages: Vec<Value> = Vec::new();

        // System prompt
        if let Some(preamble) = &req.preamble {
            if !preamble.is_empty() {
                messages.push(json!({ "role": "system", "content": preamble }));
            }
        }

        for msg in req.chat_history.clone().into_iter() {
            match msg {
                rig::message::Message::User { content } => {
                    // Collect tool results and text separately
                    let mut tool_results: Vec<Value> = Vec::new();
                    let mut text_parts: Vec<String> = Vec::new();
                    for c in content.into_iter() {
                        match c {
                            UserContent::ToolResult(tr) => {
                                let content_str = match tr.content.first() {
                                    rig::message::ToolResultContent::Text(t) => t.text.clone(),
                                    rig::message::ToolResultContent::Image(_) => "[Image]".to_string(),
                                };
                                tool_results.push(json!({
                                    "role": "tool",
                                    "tool_call_id": tr.id,
                                    "content": content_str,
                                }));
                            }
                            UserContent::Text(t) => text_parts.push(t.text),
                            _ => {}
                        }
                    }
                    // Tool results come before any text in the same user turn
                    messages.extend(tool_results);
                    if !text_parts.is_empty() {
                        messages.push(json!({
                            "role": "user",
                            "content": text_parts.join(""),
                        }));
                    }
                }
                rig::message::Message::Assistant { content, .. } => {
                    // Merge reasoning + tool_calls + text into ONE assistant message
                    let mut text_content = String::new();
                    let mut reasoning_content = String::new();
                    let mut tool_calls: Vec<Value> = Vec::new();

                    for c in content.into_iter() {
                        match c {
                            AssistantContent::Text(t) => text_content.push_str(&t.text),
                            AssistantContent::Reasoning(r) => {
                                reasoning_content.push_str(&r.reasoning.join("\n"))
                            }
                            AssistantContent::ToolCall(tc) => {
                                tool_calls.push(json!({
                                    "id": tc.id,
                                    "index": 0,
                                    "type": "function",
                                    "function": {
                                        "name": tc.function.name,
                                        "arguments": tc.function.arguments.to_string(),
                                    }
                                }));
                            }
                            _ => {}
                        }
                    }

                    let mut obj = json!({
                        "role": "assistant",
                        "content": text_content,
                    });
                    if !reasoning_content.is_empty() {
                        obj["reasoning_content"] = json!(reasoning_content);
                    }
                    if !tool_calls.is_empty() {
                        obj["tool_calls"] = json!(tool_calls);
                    }
                    messages.push(obj);
                }
            }
        }

        // Tools
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();

        let mut body = json!({
            "model": self.model,
            "messages": messages,
        });
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }
        if let Some(temp) = req.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(max_tok) = req.max_tokens {
            body["max_tokens"] = json!(max_tok);
        }
        // Merge any additional_params (e.g. thinking mode)
        if let Some(extra) = &req.additional_params {
            if let Value::Object(map) = extra {
                if let Value::Object(ref mut bmap) = body {
                    bmap.extend(map.clone());
                }
            }
        }
        body
    }
}

impl SynbotCompletionModel for DeepSeekDirectModel {
    fn completion(
        &self,
        request: CompletionRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CompletionResponse<()>, CompletionError>> + Send + '_>>
    {
        let body = self.build_request_body(&request);
        let url = format!("{}/chat/completions", self.api_base);
        let model_name = self.model.clone();
        let http = self.http.clone();
        let api_key = self.api_key.clone();

        Box::pin(async move {
            let resp = http
                .post(&url)
                .bearer_auth(&api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| CompletionError::ProviderError(format!(
                    "Request failed (url={}, model={}): {}",
                    url,
                    model_name,
                    e
                )))?;

            let status = resp.status();
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| CompletionError::ProviderError(format!(
                    "Reading response failed (url={}, model={}): {}",
                    url,
                    model_name,
                    e
                )))?;

            if !status.is_success() {
                let msg = String::from_utf8_lossy(&bytes).to_string();
                return Err(CompletionError::ProviderError(format!(
                    "Invalid status code {} (url={}, model={}) with message: {}",
                    status, url, model_name, msg
                )));
            }

            let json: Value = serde_json::from_slice(&bytes)
                .map_err(|e| CompletionError::ResponseError(e.to_string()))?;

            // Parse response into rig types
            let choice_obj = json["choices"]
                .as_array()
                .and_then(|a| a.first())
                .ok_or_else(|| CompletionError::ResponseError("No choices".into()))?;

            let msg = &choice_obj["message"];
            let content_str = msg["content"].as_str().unwrap_or("").to_string();
            let reasoning_str = msg["reasoning_content"].as_str().unwrap_or("").to_string();
            let tool_calls_json = msg["tool_calls"].as_array();

            let mut contents: Vec<AssistantContent> = Vec::new();

            // Reasoning first (so it's preserved in history for next sub-turn)
            if !reasoning_str.is_empty() {
                contents.push(AssistantContent::reasoning(&reasoning_str));
            }

            if let Some(tcs) = tool_calls_json {
                for tc in tcs {
                    let id = tc["id"].as_str().unwrap_or("").to_string();
                    let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                    let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                    let args: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                    contents.push(AssistantContent::tool_call(&id, &name, args));
                }
            }

            if !content_str.is_empty() {
                contents.push(AssistantContent::text(&content_str));
            }

            if contents.is_empty() {
                contents.push(AssistantContent::text(""));
            }

            let choice = rig::OneOrMany::many(contents)
                .unwrap_or_else(|_| rig::OneOrMany::one(AssistantContent::text("")));

            let usage_obj = &json["usage"];
            let usage = rig::completion::Usage {
                input_tokens: usage_obj["prompt_tokens"].as_u64().unwrap_or(0),
                output_tokens: usage_obj["completion_tokens"].as_u64().unwrap_or(0),
                total_tokens: usage_obj["total_tokens"].as_u64().unwrap_or(0),
                cached_input_tokens: usage_obj["prompt_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .unwrap_or(0),
            };

            Ok(CompletionResponse {
                choice,
                usage,
                raw_response: (),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// Kimi Code: direct HTTP so we control URL (.../v1/chat/completions) and User-Agent (Kimi-CLI/1.0).
// ---------------------------------------------------------------------------

struct KimiCodeDirectModel {
    http: reqwest::Client,
    api_key: String,
    api_base: String,
    model: String,
}

impl KimiCodeDirectModel {
    fn new(http: reqwest::Client, api_key: String, model: String, api_base: String) -> Self {
        Self {
            http,
            api_key,
            api_base: api_base.trim().trim_end_matches('/').to_string(),
            model,
        }
    }

    /// Build OpenAI-format request body (messages + optional tools).
    fn build_request_body(&self, req: &CompletionRequest) -> Value {
        let mut messages: Vec<Value> = Vec::new();
        if let Some(preamble) = &req.preamble {
            if !preamble.is_empty() {
                messages.push(json!({ "role": "system", "content": preamble }));
            }
        }
        for msg in req.chat_history.clone().into_iter() {
            match msg {
                rig::message::Message::User { content } => {
                    let mut tool_results: Vec<Value> = Vec::new();
                    let mut text_parts: Vec<String> = Vec::new();
                    for c in content.into_iter() {
                        match c {
                            UserContent::ToolResult(tr) => {
                                let content_str = match tr.content.first() {
                                    rig::message::ToolResultContent::Text(t) => t.text.clone(),
                                    rig::message::ToolResultContent::Image(_) => "[Image]".to_string(),
                                };
                                tool_results.push(json!({
                                    "role": "tool",
                                    "tool_call_id": tr.id,
                                    "content": content_str,
                                }));
                            }
                            UserContent::Text(t) => text_parts.push(t.text),
                            _ => {}
                        }
                    }
                    messages.extend(tool_results);
                    if !text_parts.is_empty() {
                        messages.push(json!({ "role": "user", "content": text_parts.join("") }));
                    }
                }
                rig::message::Message::Assistant { content, .. } => {
                    let mut text_content = String::new();
                    let mut reasoning_content = String::new();
                    let mut tool_calls: Vec<Value> = Vec::new();
                    for c in content.into_iter() {
                        match c {
                            AssistantContent::Text(t) => text_content.push_str(&t.text),
                            AssistantContent::Reasoning(r) => {
                                reasoning_content.push_str(&r.reasoning.join("\n"));
                            }
                            AssistantContent::ToolCall(tc) => {
                                tool_calls.push(json!({
                                    "id": tc.id,
                                    "type": "function",
                                    "function": {
                                        "name": tc.function.name,
                                        "arguments": tc.function.arguments.to_string(),
                                    }
                                }));
                            }
                            _ => {}
                        }
                    }
                    let mut obj = json!({ "role": "assistant", "content": text_content });
                    if !tool_calls.is_empty() {
                        obj["tool_calls"] = json!(tool_calls);
                        // Kimi Code requires reasoning_content when thinking is enabled and message has tool_calls.
                        obj["reasoning_content"] = json!(if reasoning_content.is_empty() { "" } else { &*reasoning_content });
                    }
                    messages.push(obj);
                }
            }
        }
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
        let mut body = json!({ "model": self.model, "messages": messages });
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }
        if let Some(temp) = req.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(max_tok) = req.max_tokens {
            body["max_tokens"] = json!(max_tok);
        }
        if let Some(extra) = &req.additional_params {
            if let Value::Object(map) = extra {
                if let Value::Object(ref mut bmap) = body {
                    bmap.extend(map.clone());
                }
            }
        }
        body
    }
}

impl SynbotCompletionModel for KimiCodeDirectModel {
    fn completion(
        &self,
        request: CompletionRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CompletionResponse<()>, CompletionError>> + Send + '_>> {
        let body = self.build_request_body(&request);
        let url = format!("{}/v1/chat/completions", self.api_base);
        let model_name = self.model.clone();
        let http = self.http.clone();
        let api_key = self.api_key.clone();

        Box::pin(async move {
            let resp = http
                .post(&url)
                .bearer_auth(&api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| CompletionError::ProviderError(format!(
                    "Request failed (url={}, model={}): {}",
                    url, model_name, e
                )))?;
            let status = resp.status();
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| CompletionError::ProviderError(format!(
                    "Reading response failed (url={}, model={}): {}",
                    url, model_name, e
                )))?;
            if !status.is_success() {
                let msg = String::from_utf8_lossy(&bytes).to_string();
                return Err(CompletionError::ProviderError(format!(
                    "Invalid status code {} (url={}, model={}) with message: {}",
                    status, url, model_name, msg
                )));
            }
            let json: Value = serde_json::from_slice(&bytes)
                .map_err(|e| CompletionError::ResponseError(e.to_string()))?;
            let choice_obj = json["choices"]
                .as_array()
                .and_then(|a| a.first())
                .ok_or_else(|| CompletionError::ResponseError("No choices".into()))?;
            let msg = &choice_obj["message"];
            let content_str = msg["content"].as_str().unwrap_or("").to_string();
            let reasoning_str = msg["reasoning_content"].as_str().unwrap_or("").to_string();
            let tool_calls_json = msg["tool_calls"].as_array();
            let mut contents: Vec<AssistantContent> = Vec::new();
            if !reasoning_str.is_empty() {
                contents.push(AssistantContent::reasoning(&reasoning_str));
            }
            if let Some(tcs) = tool_calls_json {
                for tc in tcs {
                    let id = tc["id"].as_str().unwrap_or("").to_string();
                    let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                    let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                    let args: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                    contents.push(AssistantContent::tool_call(&id, &name, args));
                }
            }
            if !content_str.is_empty() {
                contents.push(AssistantContent::text(&content_str));
            }
            if contents.is_empty() {
                contents.push(AssistantContent::text(""));
            }
            let choice = rig::OneOrMany::many(contents)
                .unwrap_or_else(|_| rig::OneOrMany::one(AssistantContent::text("")));
            let usage_obj = &json["usage"];
            let usage = rig::completion::Usage {
                input_tokens: usage_obj["prompt_tokens"].as_u64().unwrap_or(0),
                output_tokens: usage_obj["completion_tokens"].as_u64().unwrap_or(0),
                total_tokens: usage_obj["total_tokens"].as_u64().unwrap_or(0),
                cached_input_tokens: usage_obj["prompt_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .unwrap_or(0),
            };
            Ok(CompletionResponse {
                choice,
                usage,
                raw_response: (),
            })
        })
    }
}

// ---------------------------------------------------------------------------
// OpenRouter: direct HTTP to /chat/completions (OpenAI-format).
// rig's OpenAI client calls /v1/responses which OpenRouter does not support.
// ---------------------------------------------------------------------------

struct OpenRouterDirectModel {
    http: reqwest::Client,
    api_key: String,
    api_base: String,
    model: String,
}

impl OpenRouterDirectModel {
    fn new(http: reqwest::Client, api_key: String, model: String, api_base: String) -> Self {
        Self {
            http,
            api_key,
            api_base: api_base.trim().trim_end_matches('/').to_string(),
            model,
        }
    }

    /// Build OpenAI-format request body (messages + optional tools).
    fn build_request_body(&self, req: &CompletionRequest) -> Value {
        let mut messages: Vec<Value> = Vec::new();
        if let Some(preamble) = &req.preamble {
            if !preamble.is_empty() {
                messages.push(json!({ "role": "system", "content": preamble }));
            }
        }
        for msg in req.chat_history.clone().into_iter() {
            match msg {
                rig::message::Message::User { content } => {
                    let mut tool_results: Vec<Value> = Vec::new();
                    let mut text_parts: Vec<String> = Vec::new();
                    for c in content.into_iter() {
                        match c {
                            UserContent::ToolResult(tr) => {
                                let content_str = match tr.content.first() {
                                    rig::message::ToolResultContent::Text(t) => t.text.clone(),
                                    rig::message::ToolResultContent::Image(_) => "[Image]".to_string(),
                                };
                                tool_results.push(json!({
                                    "role": "tool",
                                    "tool_call_id": tr.id,
                                    "content": content_str,
                                }));
                            }
                            UserContent::Text(t) => text_parts.push(t.text),
                            _ => {}
                        }
                    }
                    messages.extend(tool_results);
                    if !text_parts.is_empty() {
                        messages.push(json!({ "role": "user", "content": text_parts.join("") }));
                    }
                }
                rig::message::Message::Assistant { content, .. } => {
                    let mut text_content = String::new();
                    let mut tool_calls: Vec<Value> = Vec::new();
                    for c in content.into_iter() {
                        match c {
                            AssistantContent::Text(t) => text_content.push_str(&t.text),
                            AssistantContent::ToolCall(tc) => {
                                tool_calls.push(json!({
                                    "id": tc.id,
                                    "type": "function",
                                    "function": {
                                        "name": tc.function.name,
                                        "arguments": tc.function.arguments.to_string(),
                                    }
                                }));
                            }
                            _ => {}
                        }
                    }
                    let mut obj = json!({ "role": "assistant", "content": text_content });
                    if !tool_calls.is_empty() {
                        obj["tool_calls"] = json!(tool_calls);
                    }
                    messages.push(obj);
                }
            }
        }
        let tools: Vec<Value> = req
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
        let mut body = json!({ "model": self.model, "messages": messages });
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }
        if let Some(temp) = req.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(max_tok) = req.max_tokens {
            body["max_tokens"] = json!(max_tok);
        }
        if let Some(extra) = &req.additional_params {
            if let Value::Object(map) = extra {
                if let Value::Object(ref mut bmap) = body {
                    bmap.extend(map.clone());
                }
            }
        }
        body
    }
}

impl SynbotCompletionModel for OpenRouterDirectModel {
    fn completion(
        &self,
        request: CompletionRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CompletionResponse<()>, CompletionError>> + Send + '_>> {
        let body = self.build_request_body(&request);
        let url = format!("{}/chat/completions", self.api_base);
        let model_name = self.model.clone();
        let http = self.http.clone();
        let api_key = self.api_key.clone();

        Box::pin(async move {
            let resp = http
                .post(&url)
                .bearer_auth(&api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| CompletionError::ProviderError(format!(
                    "Request failed (url={}, model={}): {}",
                    url, model_name, e
                )))?;
            let status = resp.status();
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| CompletionError::ProviderError(format!(
                    "Reading response failed (url={}, model={}): {}",
                    url, model_name, e
                )))?;
            if !status.is_success() {
                let msg = String::from_utf8_lossy(&bytes).to_string();
                return Err(CompletionError::ProviderError(format!(
                    "Invalid status code {} (url={}, model={}) with message: {}",
                    status, url, model_name, msg
                )));
            }
            let json: Value = serde_json::from_slice(&bytes)
                .map_err(|e| CompletionError::ResponseError(e.to_string()))?;
            let choice_obj = json["choices"]
                .as_array()
                .and_then(|a| a.first())
                .ok_or_else(|| CompletionError::ResponseError("No choices".into()))?;
            let msg = &choice_obj["message"];
            let content_str = msg["content"].as_str().unwrap_or("").to_string();
            let tool_calls_json = msg["tool_calls"].as_array();
            let mut contents: Vec<AssistantContent> = Vec::new();
            if let Some(tcs) = tool_calls_json {
                for tc in tcs {
                    let id = tc["id"].as_str().unwrap_or("").to_string();
                    let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                    let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                    let args: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                    contents.push(AssistantContent::tool_call(&id, &name, args));
                }
            }
            if !content_str.is_empty() {
                contents.push(AssistantContent::text(&content_str));
            }
            if contents.is_empty() {
                contents.push(AssistantContent::text(""));
            }
            let choice = rig::OneOrMany::many(contents)
                .unwrap_or_else(|_| rig::OneOrMany::one(AssistantContent::text("")));
            let usage_obj = &json["usage"];
            let usage = rig::completion::Usage {
                input_tokens: usage_obj["prompt_tokens"].as_u64().unwrap_or(0),
                output_tokens: usage_obj["completion_tokens"].as_u64().unwrap_or(0),
                total_tokens: usage_obj["total_tokens"].as_u64().unwrap_or(0),
                cached_input_tokens: usage_obj["prompt_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .unwrap_or(0),
            };
            Ok(CompletionResponse {
                choice,
                usage,
                raw_response: (),
            })
        })
    }
}
