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
    // In AppContainer, inject Google DNS resolver; otherwise use default reqwest client.
    // This is the single injection point for all rig providers — no per-provider patching needed.
    // reqwest::Client is cheap to clone (Arc-backed), so we clone once per provider branch.
    #[cfg(target_os = "windows")]
    let mk_http = || crate::appcontainer_dns::build_reqwest_client();
    #[cfg(not(target_os = "windows"))]
    let mk_http = || reqwest::Client::new();

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
        let client = rig::providers::deepseek::Client::<RC>::builder()
            .api_key(api_key.to_string())
            .http_client(mk_http())
            .build()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let m = client.completion_model(model_name.to_string());
        Arc::new(DeepSeekModel(client, m)) as Arc<dyn SynbotCompletionModel>
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
impl_model!(DeepSeekModel, rig::providers::deepseek::Client);
impl_model!(MoonshotModel, rig::providers::moonshot::Client);
impl_model!(OllamaModel, rig::providers::ollama::Client);
