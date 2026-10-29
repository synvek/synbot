//! `/api/plugins` and `/api/plugin-keys` — plugin packages, permissions and trust.
//!
//! Installing a package copies it into `<config dir>/plugins/<id>/` and adds a `config.plugins.<id>`
//! entry, which is what the loader iterates. Disabling keeps the entry but flips `enabled: false`, so
//! the package stays installed and visible.

use std::path::PathBuf;

use actix_web::{web, HttpResponse, Result};
use serde::Deserialize;

use crate::config::save_config;
use crate::plugin::install;
use crate::web::handlers::api::{ApiError, ApiResponse};
use crate::web::state::AppState;

#[derive(Debug, Deserialize)]
pub struct InstallBody {
    /// Directory holding `plugin.json` and the wasm entry.
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct EnabledBody {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct KeyBody {
    pub name: String,
    pub public_key: String,
}

/// Plugins that are configured but were not installed as a package (legacy `path` / `internal`).
#[derive(Debug, serde::Serialize)]
pub struct LegacyPlugin {
    pub id: String,
    pub enabled: bool,
    pub internal: bool,
}

fn to_error(e: anyhow::Error) -> actix_web::Error {
    ApiError::BadRequest(e.to_string()).into()
}

/// Persist the current in-memory config to the daemon's config file.
pub(crate) async fn persist(state: &web::Data<AppState>) -> Result<(), actix_web::Error> {
    let cfg = state.config.read().await;
    save_config(&cfg, Some(state.config_path.as_path()))
        .map_err(|e| ApiError::InternalError(format!("Failed to write config file: {}", e)))?;
    Ok(())
}

/// GET /api/plugins — installed packages + trust key count + legacy config entries.
pub async fn list_plugins(state: web::Data<AppState>) -> Result<HttpResponse> {
    let cfg = state.config.read().await;
    let plugins = install::list(&install::plugins_root(), &cfg.plugins);
    let managed: Vec<&str> = plugins.iter().map(|p| p.id.as_str()).collect();
    let legacy: Vec<LegacyPlugin> = cfg
        .plugins
        .iter()
        .filter(|(id, _)| !managed.contains(&id.as_str()))
        .map(|(id, value)| LegacyPlugin {
            id: id.clone(),
            enabled: value
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            internal: value
                .get("internal")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
        .collect();
    let trust_keys = install::load_trust_store(&install::trust_store_path())
        .keys
        .len();
    let trust_path = install::trust_store_path();

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "plugins": plugins,
            "legacy": legacy,
            "trust_keys": trust_keys,
            "trust_store": trust_path.display().to_string(),
            "root": install::plugins_root().display().to_string(),
        }))),
    )
}

/// POST /api/plugins/install — copy a package directory in and enable it.
pub async fn install_plugin(
    state: web::Data<AppState>,
    body: web::Json<InstallBody>,
) -> Result<HttpResponse> {
    let path = PathBuf::from(body.path.trim());
    if body.path.trim().is_empty() {
        return Err(ApiError::BadRequest("`path` is required".to_string()).into());
    }
    let root = install::plugins_root();
    let trust = install::trust_store_path();
    let installed = web::block(move || install::install_from_dir(&root, &path, &trust))
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?
        .map_err(to_error)?;
    let id = installed.id.clone();

    {
        let mut cfg = state.config.write().await;
        let entry = cfg
            .plugins
            .entry(id.clone())
            .or_insert_with(|| serde_json::json!({}));
        if let Some(obj) = entry.as_object_mut() {
            obj.insert("enabled".to_string(), serde_json::json!(true));
        }
    }
    persist(&state).await?;

    Ok(HttpResponse::Ok().json(ApiResponse::success(installed)))
}

/// DELETE /api/plugins/{id} — remove the package and its config entry.
pub async fn uninstall_plugin(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let id = path.into_inner();
    let root = install::plugins_root();
    let target = id.clone();
    let removed = web::block(move || install::uninstall(&root, &target))
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?
        .map_err(to_error)?;
    if !removed {
        return Err(
            ApiError::NotFound("no managed plugin package with that id".to_string()).into(),
        );
    }

    {
        let mut cfg = state.config.write().await;
        cfg.plugins.remove(&id);
    }
    persist(&state).await?;

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "removed": id,
        }))),
    )
}

/// POST /api/plugins/{id}/enabled — toggle loading without uninstalling.
pub async fn set_enabled(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<EnabledBody>,
) -> Result<HttpResponse> {
    let id = path.into_inner();
    if !state.config.read().await.plugins.contains_key(&id) {
        return Err(ApiError::NotFound(format!("plugin '{id}' is not configured")).into());
    }
    {
        let mut cfg = state.config.write().await;
        let entry = cfg
            .plugins
            .entry(id.clone())
            .or_insert_with(|| serde_json::json!({}));
        if let Some(obj) = entry.as_object_mut() {
            obj.insert("enabled".to_string(), serde_json::json!(body.enabled));
        } else {
            *entry = serde_json::json!({ "enabled": body.enabled });
        }
    }
    persist(&state).await?;

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "id": id,
            "enabled": body.enabled,
        }))),
    )
}

/// GET /api/plugin-keys — trusted publisher keys.
pub async fn list_keys() -> Result<HttpResponse> {
    let store = install::load_trust_store(&install::trust_store_path());
    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "keys": store.keys,
        }))),
    )
}

/// POST /api/plugin-keys — trust a publisher public key (base64 ed25519).
pub async fn add_key(body: web::Json<KeyBody>) -> Result<HttpResponse> {
    let key = install::add_trusted_key(&install::trust_store_path(), &body.name, &body.public_key)
        .map_err(to_error)?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(key)))
}

/// DELETE /api/plugin-keys/{name} — forget a publisher key.
pub async fn remove_key(path: web::Path<String>) -> Result<HttpResponse> {
    let name = path.into_inner();
    let removed =
        install::remove_trusted_key(&install::trust_store_path(), &name).map_err(to_error)?;
    if !removed {
        return Err(ApiError::NotFound(format!("no trusted key named '{name}'")).into());
    }
    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "removed": name,
        }))),
    )
}
