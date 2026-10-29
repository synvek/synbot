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
use crate::plugin::install::{self, InstalledPlugin};
use crate::plugin::manifest::HostPermissions;
use crate::rig_provider;
use crate::tools::ToolRegistry;

/// Timeout and memory ceiling applied to managed (manifest-carrying) plugins.
const MANAGED_PLUGIN_TIMEOUT_SECS: u64 = 120;
/// 64 MiB of wasm linear memory (16 pages = 1 MiB in Extism's counting).
const MANAGED_PLUGIN_MAX_PAGES: u32 = 1024;

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
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(p.trim_start_matches("~/"))
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

/// `enabled: false` keeps the entry configured but unloaded (the UI toggle writes this).
fn is_disabled(plugin_value: &serde_json::Value) -> bool {
    plugin_value
        .get("enabled")
        .and_then(|v| v.as_bool())
        .map(|enabled| !enabled)
        .unwrap_or(false)
}

/// Resolve a managed package (`<plugins>/<id>/plugin.json`) when one exists.
fn managed_package(plugin_id: &str, configs: &config::Config) -> Option<InstalledPlugin> {
    let dir = install::plugins_root().join(plugin_id);
    if !dir.join(crate::plugin::manifest::MANIFEST_FILE).exists() {
        return None;
    }
    match install::read_installed(&dir, &configs.plugins) {
        Ok(pkg) => Some(pkg),
        Err(e) => {
            warn!(plugin = %plugin_id, error = %e, "plugin package is unreadable, skip");
            None
        }
    }
}

/// Load all Extism plugins from config and register adapters. Single wasm load failure is logged and skipped.
pub async fn load_extism_plugins(
    cfg: &config::Config,
    tools: &mut ToolRegistry,
    hooks: &HookRegistry,
    background: &mut BackgroundServiceRegistry,
    skills: &mut CompositeSkillProvider,
) {
    // Register extra providers (OpenAI- or Anthropic-compatible per entry.apiStyle).
    rig_provider::register_extra_providers_from_config(cfg);

    let http_client = Arc::new(appcontainer_dns::build_reqwest_client());
    for (plugin_id, plugin_value) in &cfg.plugins {
        if plugin_value.as_object().is_none() {
            continue;
        }
        if is_internal(plugin_value) {
            continue;
        }
        if is_disabled(plugin_value) {
            info!(plugin = %plugin_id, "plugin disabled in config, skip");
            continue;
        }

        // A managed package carries its own permissions; a config-only plugin keeps the legacy
        // behaviour (workspace-wide file access, any host) so existing setups do not break.
        let managed = managed_package(plugin_id, cfg);
        let (path, permissions) = match &managed {
            Some(pkg) => {
                if !pkg.loadable {
                    warn!(
                        plugin = %plugin_id,
                        trust = ?pkg.trust,
                        "plugin failed the integrity check (untrusted signature or modified wasm), skip"
                    );
                    continue;
                }
                (
                    PathBuf::from(&pkg.path).join(&pkg.entry),
                    HostPermissions::from_permissions(&pkg.permissions),
                )
            }
            None => {
                let Some(path) = wasm_path(plugin_id, plugin_value) else {
                    warn!(plugin = %plugin_id, "extism plugin path not found, skip");
                    continue;
                };
                (path, HostPermissions::unrestricted())
            }
        };

        let plugin_config = plugin_value.clone();
        let workspace = Some(config::workspace_path(cfg));
        let host_data = PluginHostData {
            plugin_id: plugin_id.clone(),
            plugin_config: plugin_config.clone(),
            http_client: Arc::clone(&http_client),
            workspace,
            permissions,
        };
        let has_completion = true;
        let imports = host_fns::host_functions(host_data, has_completion);
        let mut manifest = Manifest::new([Wasm::file(path.clone())]);
        if managed.is_some() {
            // Same budget for every packaged plugin, whatever the wasm asks for.
            manifest = manifest
                .with_timeout(std::time::Duration::from_secs(MANAGED_PLUGIN_TIMEOUT_SECS))
                .with_memory_max(MANAGED_PLUGIN_MAX_PAGES);
        }
        let plugin = match Plugin::new(&manifest, imports, false) {
            Ok(p) => p,
            Err(e) => {
                warn!(plugin = %plugin_id, path = %path.display(), error = %e, "extism plugin load failed");
                continue;
            }
        };
        let plugin = Arc::new(std::sync::Mutex::new(plugin));
        info!(plugin = %plugin_id, "extism plugin loaded");

        if plugin
            .lock()
            .map(|p| p.function_exists(abi::FN_TOOL_MANIFEST))
            .unwrap_or(false)
            && plugin
                .lock()
                .map(|p| p.function_exists(abi::FN_TOOL_CALL))
                .unwrap_or(false)
        {
            let tool = Arc::new(adapters::ExtismTool::new(
                Arc::clone(&plugin),
                plugin_id.clone(),
            ));
            if let Err(e) = tools.register(tool) {
                warn!(plugin = %plugin_id, error = %e, "register extism tool failed");
            }
        }
        if plugin
            .lock()
            .map(|p| p.function_exists(abi::FN_HOOK_EVENT))
            .unwrap_or(false)
        {
            let hook = Arc::new(adapters::ExtismHook::new(
                Arc::clone(&plugin),
                plugin_id.clone(),
            ));
            hooks.register(hook).await;
        }
        if plugin
            .lock()
            .map(|p| p.function_exists(abi::FN_SKILLS_LIST))
            .unwrap_or(false)
            && plugin
                .lock()
                .map(|p| p.function_exists(abi::FN_SKILL_LOAD))
                .unwrap_or(false)
        {
            skills.add(Box::new(adapters::ExtismSkillProvider::new(
                Arc::clone(&plugin),
                plugin_id.clone(),
            )));
        }
        if plugin
            .lock()
            .map(|p| p.function_exists(abi::FN_BACKGROUND_RUN))
            .unwrap_or(false)
        {
            background.register(Arc::new(adapters::ExtismBackgroundService::new(
                Arc::clone(&plugin),
                plugin_id.clone(),
            )));
        }
        if plugin
            .lock()
            .map(|p| p.function_exists(abi::FN_COMPLETION))
            .unwrap_or(false)
        {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_internal_false_when_missing() {
        let v = serde_json::json!({});
        assert!(!is_internal(&v));
    }

    #[test]
    fn is_internal_false_when_false() {
        let v = serde_json::json!({ "internal": false });
        assert!(!is_internal(&v));
    }

    #[test]
    fn is_internal_true_when_true() {
        let v = serde_json::json!({ "internal": true });
        assert!(is_internal(&v));
    }

    #[test]
    fn is_disabled_reads_the_enabled_flag() {
        assert!(!is_disabled(&serde_json::json!({})));
        assert!(!is_disabled(&serde_json::json!({ "enabled": true })));
        assert!(is_disabled(&serde_json::json!({ "enabled": false })));
        // A non-boolean value is ignored rather than guessed.
        assert!(!is_disabled(&serde_json::json!({ "enabled": "no" })));
    }

    #[test]
    fn managed_package_is_none_without_a_manifest() {
        let cfg = config::Config::default();
        assert!(managed_package("definitely-not-installed-12345", &cfg).is_none());
    }

    #[test]
    fn wasm_path_uses_path_from_config_when_absolute() {
        let v = serde_json::json!({ "path": "/tmp/foo.wasm" });
        let p = wasm_path("my_plugin", &v);
        // Path may or may not exist; we only check it returns the path when it exists
        if let Some(path) = p {
            assert_eq!(path, std::path::Path::new("/tmp/foo.wasm"));
        }
    }

    #[test]
    fn wasm_path_none_when_path_missing_and_default_nonexistent() {
        let v = serde_json::json!({});
        let p = wasm_path("nonexistent_plugin_12345", &v);
        // Default would be plugins_dir()/nonexistent_plugin_12345.wasm which typically doesn't exist
        assert!(p.is_none() || p.as_ref().map(|x| x.exists()).unwrap_or(false));
    }
}
