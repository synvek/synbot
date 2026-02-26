//! Adapters that wrap Extism Plugin and implement synbot traits (DynTool, Hook, SkillProvider, etc.).

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use extism::Plugin;
use rig::completion::request::{CompletionRequest, CompletionResponse};
use rig::completion::CompletionError;
use serde_json::Value;
use tokio::task;

use crate::agent::skills::SkillProvider;
use crate::background::{BackgroundContext, BackgroundService};
use crate::hooks::{Hook, HookEvent};
use crate::plugin::abi;
use crate::rig_provider::{ProviderFactory, SynbotCompletionModel};
use crate::tools::DynTool;

/// Shared plugin handle for all adapters.
pub type SharedPlugin = Arc<Mutex<Plugin>>;

/// Tool adapter: calls synbot_tool_manifest and synbot_tool_call.
pub struct ExtismTool {
    plugin: SharedPlugin,
    plugin_id: String,
    cache: Mutex<Option<(String, String, Value)>>,
    /// Cached static refs for name/description (set once after first manifest load).
    name_ref: Mutex<Option<&'static str>>,
    description_ref: Mutex<Option<&'static str>>,
}

impl ExtismTool {
    pub fn new(plugin: SharedPlugin, plugin_id: String) -> Self {
        Self {
            plugin,
            plugin_id,
            cache: Mutex::new(None),
            name_ref: Mutex::new(None),
            description_ref: Mutex::new(None),
        }
    }

    fn get_manifest(&self) -> Result<(String, String, Value)> {
        let mut guard = self.cache.lock().map_err(|_| anyhow::anyhow!("lock cache"))?;
        if let Some(ref c) = *guard {
            return Ok(c.clone());
        }
        let out: String = self
            .plugin
            .lock()
            .map_err(|_| anyhow::anyhow!("lock plugin"))?
            .call(abi::FN_TOOL_MANIFEST, "")?;
        let obj: Value = serde_json::from_str(&out).map_err(|e| anyhow::anyhow!("tool manifest json: {}", e))?;
        let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let description = obj.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let parameters_schema = obj.get("parameters_schema").cloned().unwrap_or(Value::Object(serde_json::Map::new()));
        let cached = (name.clone(), description.clone(), parameters_schema.clone());
        *guard = Some(cached);
        Ok((name, description, parameters_schema))
    }

    fn ensure_name_description_refs(&self) {
        if self.name_ref.lock().map(|g| g.is_some()).unwrap_or(false) {
            return;
        }
        if let Ok((name, description, _)) = self.get_manifest() {
            let name_static: &'static str = Box::leak(name.into_boxed_str());
            let desc_static: &'static str = Box::leak(description.into_boxed_str());
            let _ = self.name_ref.lock().map(|mut g| *g = Some(name_static));
            let _ = self.description_ref.lock().map(|mut g| *g = Some(desc_static));
        }
    }
}

#[async_trait]
impl DynTool for ExtismTool {
    fn name(&self) -> &str {
        self.ensure_name_description_refs();
        self.name_ref
            .lock()
            .ok()
            .and_then(|g| *g)
            .unwrap_or("")
    }

    fn description(&self) -> &str {
        self.ensure_name_description_refs();
        self.description_ref
            .lock()
            .ok()
            .and_then(|g| *g)
            .unwrap_or("")
    }

    fn parameters_schema(&self) -> Value {
        self.get_manifest()
            .map(|(_, _, s)| s)
            .unwrap_or_else(|_| Value::Object(serde_json::Map::new()))
    }

    async fn call(&self, args: Value) -> Result<String> {
        let plugin = Arc::clone(&self.plugin);
        let input = serde_json::json!({ "args": args }).to_string();
        let out: String = task::spawn_blocking(move || {
            plugin
                .lock()
                .map_err(|_| anyhow::anyhow!("lock plugin"))?
                .call(abi::FN_TOOL_CALL, &input)
        })
        .await??;
        let obj: Value = serde_json::from_str(&out).unwrap_or(Value::Object(serde_json::Map::new()));
        if let Some(err) = obj.get("err").and_then(|v| v.as_str()) {
            anyhow::bail!("plugin tool error: {}", err);
        }
        Ok(obj
            .get("ok")
            .and_then(|v| v.as_str())
            .unwrap_or(&out)
            .to_string())
    }
}

/// Hook adapter: forwards HookEvent to synbot_hook_event.
pub struct ExtismHook {
    plugin: SharedPlugin,
    plugin_id: String,
}

impl ExtismHook {
    pub fn new(plugin: SharedPlugin, plugin_id: String) -> Self {
        Self { plugin, plugin_id }
    }
}

#[async_trait]
impl Hook for ExtismHook {
    async fn on_event(&self, event: HookEvent) {
        let plugin = Arc::clone(&self.plugin);
        let input = match serde_json::to_string(&event) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(plugin = %self.plugin_id, error = %e, "serialize HookEvent");
                return;
            }
        };
        let _ = task::spawn_blocking(move || {
            if let Err(e) = plugin.lock().map_err(|_| anyhow::anyhow!("lock")).and_then(|mut p| p.call::<&str, String>(abi::FN_HOOK_EVENT, &input)) {
                tracing::warn!(error = %e, "extism hook_event");
            }
        })
        .await;
    }
}

/// Skill provider adapter: synbot_skills_list and synbot_skill_load.
pub struct ExtismSkillProvider {
    plugin: SharedPlugin,
    plugin_id: String,
}

impl ExtismSkillProvider {
    pub fn new(plugin: SharedPlugin, plugin_id: String) -> Self {
        Self { plugin, plugin_id }
    }
}

impl SkillProvider for ExtismSkillProvider {
    fn list_skills(&self) -> Vec<String> {
        let out: String = match self.plugin.lock() {
            Ok(mut p) => match p.call(abi::FN_SKILLS_LIST, "") {
                Ok(s) => s,
                Err(_) => return vec![],
            },
            Err(_) => return vec![],
        };
        serde_json::from_str(&out).unwrap_or_default()
    }

    fn load_skill(&self, name: &str) -> Option<String> {
        let input = serde_json::json!({ "name": name }).to_string();
        let out: String = self.plugin.lock().ok()?.call(abi::FN_SKILL_LOAD, &input).ok()?;
        let v: Option<String> = serde_json::from_str(&out).ok().flatten();
        v.filter(|s| !s.is_empty())
    }
}

/// Background service adapter: runs synbot_background_run in a task.
pub struct ExtismBackgroundService {
    plugin: SharedPlugin,
    plugin_id: String,
}

impl ExtismBackgroundService {
    pub fn new(plugin: SharedPlugin, plugin_id: String) -> Self {
        Self { plugin, plugin_id }
    }
}

#[async_trait]
impl BackgroundService for ExtismBackgroundService {
    fn name(&self) -> &str {
        &self.plugin_id
    }

    async fn run(&self, ctx: BackgroundContext) -> Result<()> {
        let plugin = Arc::clone(&self.plugin);
        let config_json = {
            let cfg = ctx.config.read().await;
            serde_json::to_string(&*cfg).unwrap_or_else(|_| "{}".to_string())
        };
        let input = serde_json::json!({ "config": config_json }).to_string();
        task::spawn_blocking(move || {
            plugin
                .lock()
                .map_err(|_| anyhow::anyhow!("lock plugin"))?
                .call::<&str, String>(abi::FN_BACKGROUND_RUN, &input)
        })
        .await??;
        Ok(())
    }
}

/// Completion model backed by an Extism plugin (synbot_completion). Used for provider plugins.
pub struct ExtismCompletionModel {
    plugin: SharedPlugin,
    plugin_id: String,
}

impl ExtismCompletionModel {
    pub fn new(plugin: SharedPlugin, plugin_id: String) -> Self {
        Self { plugin, plugin_id }
    }
}

impl SynbotCompletionModel for ExtismCompletionModel {
    fn completion(
        &self,
        _request: CompletionRequest,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<CompletionResponse<()>, CompletionError>>
                + Send
                + '_,
        >,
    > {
        let plugin_id = self.plugin_id.clone();
        Box::pin(async move {
            Err(CompletionError::ProviderError(format!(
                "Extism provider '{}': CompletionRequest/CompletionResponse serialization not yet implemented; use a built-in provider",
                plugin_id
            )))
        })
    }
}

/// Provider factory that builds ExtismCompletionModel for a given plugin (provider name = plugin_id).
pub struct ExtismProviderFactory {
    plugin: SharedPlugin,
    plugin_id: String,
}

impl ExtismProviderFactory {
    pub fn new(plugin: SharedPlugin, plugin_id: String) -> Self {
        Self { plugin, plugin_id }
    }
}

impl ProviderFactory for ExtismProviderFactory {
    fn build(
        &self,
        _provider_name: &str,
        _model_name: &str,
        _api_key: &str,
        _api_base: Option<&str>,
    ) -> Result<Arc<dyn SynbotCompletionModel>> {
        Ok(Arc::new(ExtismCompletionModel::new(
            Arc::clone(&self.plugin),
            self.plugin_id.clone(),
        )))
    }
}
