//! Load Extism plugins from config.plugins and register adapters into synbot registries.

use std::path::PathBuf;
use std::sync::Arc;

use extism::{Manifest, Plugin, Wasm};
use tracing::{info, warn};

use crate::agent::skills::CompositeSkillProvider;
use crate::appcontainer_dns;
use crate::background::BackgroundServiceRegistry;
use crate::config;
use crate::hooks::HookRegistry;
use crate::plugin::abi;
use crate::plugin::adapters;
use crate::plugin::host_fns::{self, PluginHostData};
use crate::rig_provider;
use crate::tools::ToolRegistry;

/// Default plugins directory: ~/.synbot/plugins/
fn plugins_dir() -> PathBuf {
    config::config_dir().join("plugins")
}

/// Resolve wasm path for a plugin: config path or plugins_dir()/plugin_id.wasm.
fn wasm_path(plugin_id: &str, plugin_value: &serde_json::Value) -> Option<PathBuf> {
    let path_str = plugin_value
        .get("path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let path = match path_str {
        Some(p) => {
            if p.starts_with("~/") {
                dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(p.trim_start_matches("~/"))
            } else {
                PathBuf::from(p)
            }
        }
        None => plugins_dir().join(format!("{}.wasm", plugin_id)),
    };
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

/// Check if plugin entry is internal-only (do not load wasm).
fn is_internal(plugin_value: &serde_json::Value) -> bool {
    plugin_value
        .get("internal")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Load all Extism plugins from config and register adapters. Single wasm load failure is logged and skipped.
pub async fn load_extism_plugins(
    cfg: &config::Config,
    tools: &mut ToolRegistry,
    hooks: &HookRegistry,
    background: &mut BackgroundServiceRegistry,
    skills: &mut CompositeSkillProvider,
) {
    // Register OpenAI-compatible providers for each config.providers.extra key (no code change needed).
    rig_provider::register_extra_openai_compatible_providers(cfg);

    let http_client = Arc::new(appcontainer_dns::build_reqwest_client());
    for (plugin_id, plugin_value) in &cfg.plugins {
        let Some(obj) = plugin_value.as_object() else { continue };
        if is_internal(plugin_value) {
            continue;
        }
        let Some(path) = wasm_path(plugin_id, plugin_value) else {
            warn!(plugin = %plugin_id, "extism plugin path not found, skip");
            continue;
        };
        let plugin_config = plugin_value.clone();
        let workspace = Some(config::workspace_path(cfg));
        let host_data = PluginHostData {
            plugin_id: plugin_id.clone(),
            plugin_config: plugin_config.clone(),
            http_client: Arc::clone(&http_client),
            workspace,
        };
        let has_completion = true;
        let imports = host_fns::host_functions(host_data, has_completion);
        let manifest = Manifest::new([Wasm::file(path.clone())]);
        let plugin = match Plugin::new(&manifest, imports, false) {
            Ok(p) => p,
            Err(e) => {
                warn!(plugin = %plugin_id, path = %path.display(), error = %e, "extism plugin load failed");
                continue;
            }
        };
        let plugin = Arc::new(std::sync::Mutex::new(plugin));
        info!(plugin = %plugin_id, "extism plugin loaded");

        if plugin.lock().map(|p| p.function_exists(abi::FN_TOOL_MANIFEST)).unwrap_or(false)
            && plugin.lock().map(|p| p.function_exists(abi::FN_TOOL_CALL)).unwrap_or(false)
        {
            let tool = Arc::new(adapters::ExtismTool::new(Arc::clone(&plugin), plugin_id.clone()));
            if let Err(e) = tools.register(tool) {
                warn!(plugin = %plugin_id, error = %e, "register extism tool failed");
            }
        }
        if plugin.lock().map(|p| p.function_exists(abi::FN_HOOK_EVENT)).unwrap_or(false) {
            let hook = Arc::new(adapters::ExtismHook::new(Arc::clone(&plugin), plugin_id.clone()));
            hooks.register(hook).await;
        }
        if plugin.lock().map(|p| p.function_exists(abi::FN_SKILLS_LIST)).unwrap_or(false)
            && plugin.lock().map(|p| p.function_exists(abi::FN_SKILL_LOAD)).unwrap_or(false)
        {
            skills.add(Box::new(adapters::ExtismSkillProvider::new(
                Arc::clone(&plugin),
                plugin_id.clone(),
            )));
        }
        if plugin.lock().map(|p| p.function_exists(abi::FN_BACKGROUND_RUN)).unwrap_or(false) {
            background.register(Arc::new(adapters::ExtismBackgroundService::new(
                Arc::clone(&plugin),
                plugin_id.clone(),
            )));
        }
        if plugin.lock().map(|p| p.function_exists(abi::FN_COMPLETION)).unwrap_or(false) {
            let factory = Arc::new(adapters::ExtismProviderFactory::new(
                Arc::clone(&plugin),
                plugin_id.clone(),
            ));
            rig_provider::default_registry()
                .write()
                .expect("provider registry lock")
                .register(plugin_id, factory);
        }
    }
}
