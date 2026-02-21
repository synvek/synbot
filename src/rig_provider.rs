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

use anyhow::Result;
use rig::client::CompletionClient;
use rig::client::Nothing;
use rig::completion::request::{
    CompletionError, CompletionRequest, CompletionResponse,
};
use rig::completion::CompletionModel;
use rig::message::{AssistantContent, UserContent};
use serde_json::{json, Value};
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

/// Build an Arc<dyn SynbotCompletionModel> from provider name, model name, API key and optional base URL.
pub fn build_completion_model(
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
    let model = if lower.contains("anthropic") || lower.contains("claude") {
        let client = rig::providers::anthropic::Client::<RC>::builder()
            .api_key(api_key.to_string())
            .http_client(mk_http())
            .build()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let m = client.completion_model(model_name.to_string());
        Arc::new(AnthropicModel(client, m)) as Arc<dyn SynbotCompletionModel>
    } else if lower.contains("deepseek") {
        let m = DeepSeekDirectModel::new(mk_http(), api_key.to_string(), model_name.to_string());
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
    } else {
        // OpenAI or default (OpenRouter-compatible).
        let mut builder = rig::providers::openai::Client::<RC>::builder()
            .api_key(api_key.to_string())
            .http_client(mk_http());
        if let Some(base) = api_base {
            if !base.is_empty() {
                builder = builder.base_url(base);
            }
        }
        let client = builder.build().map_err(|e| anyhow::anyhow!("{}", e))?;
        let m = client.completion_model(model_name.to_string());
        Arc::new(OpenAiModel(client, m)) as Arc<dyn SynbotCompletionModel>
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
impl_model!(AnthropicModel, rig::providers::anthropic::Client);
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
    fn new(http: reqwest::Client, api_key: String, model: String) -> Self {
        Self {
            http,
            api_key,
            api_base: DEEPSEEK_API_BASE.to_string(),
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
