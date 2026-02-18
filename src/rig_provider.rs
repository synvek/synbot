//! Rig-core provider bridge: build completion models from provider name without rig-dyn.
//!
//! Uses rig-core 0.30 providers directly and exposes a unified `SynbotCompletionModel` trait
//! so the rest of the app can use `Arc<dyn SynbotCompletionModel>`.

use anyhow::Result;
use rig::client::CompletionClient;
use rig::client::Nothing;
use rig::completion::request::{
    CompletionError, CompletionRequest, CompletionResponse,
};
use rig::completion::CompletionModel;
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
/// Note: api_base is currently unused (rig-core 0.30 does not expose from_url for all providers); reserved for OpenRouter etc.
pub fn build_completion_model(
    provider_name: &str,
    model_name: &str,
    api_key: &str,
    api_base: Option<&str>,
) -> Result<Arc<dyn SynbotCompletionModel>> {
    let _ = api_base;
    let lower = provider_name.to_lowercase();
    let model = if lower.contains("anthropic") || lower.contains("claude") {
        let client = rig::providers::anthropic::Client::new(api_key.to_string())
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let m = client.completion_model(model_name.to_string());
        Arc::new(AnthropicModel(client, m)) as Arc<dyn SynbotCompletionModel>
    } else if lower.contains("deepseek") {
        let client = rig::providers::deepseek::Client::new(api_key.to_string())
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let m = client.completion_model(model_name.to_string());
        Arc::new(DeepSeekModel(client, m)) as Arc<dyn SynbotCompletionModel>
    } else if lower.contains("moonshot") {
        let client = rig::providers::moonshot::Client::new(api_key.to_string())
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let m = client.completion_model(model_name.to_string());
        Arc::new(MoonshotModel(client, m)) as Arc<dyn SynbotCompletionModel>
    } else if lower.contains("ollama") {
        let client =
            rig::providers::ollama::Client::new(Nothing).map_err(|e| anyhow::anyhow!("{}", e))?;
        let m = client.completion_model(model_name.to_string());
        Arc::new(OllamaModel(client, m)) as Arc<dyn SynbotCompletionModel>
    } else {
        // OpenAI or default (OpenRouter-compatible). rig 0.30 Client::new returns Result; no from_url in 0.30, use base from env or default.
        let client = rig::providers::openai::Client::new(api_key.to_string())
            .map_err(|e| anyhow::anyhow!("{}", e))?;
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
impl_model!(DeepSeekModel, rig::providers::deepseek::Client);
impl_model!(MoonshotModel, rig::providers::moonshot::Client);
impl_model!(OllamaModel, rig::providers::ollama::Client);
