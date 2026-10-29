//! `/api/market` — the plugin market: list published plugins, install one, update everything.
//!
//! Downloads and installs are two separate steps on purpose: the network work (fetch the index,
//! download the packages, verify every digest) runs on the async runtime, while the blocking
//! staging → install handoff runs through `web::block`. Nothing is enabled in `config.plugins`
//! until the packages actually landed on disk — the loader only reads config, so enabling first
//! would advertise plugins that are not installed.

use std::collections::HashMap;

use actix_web::{web, HttpResponse, Result};
use serde::Deserialize;

use crate::plugin::install;
use crate::plugin::market;
use crate::web::handlers::api::{ApiError, ApiResponse};
use crate::web::handlers::plugins::persist;
use crate::web::state::AppState;

#[derive(Debug, Deserialize)]
pub struct InstallBody {
    pub id: String,
    /// Exact version; omitted installs the newest one the market offers.
    #[serde(default)]
    pub version: Option<String>,
}

fn to_error(e: anyhow::Error) -> actix_web::Error {
    ApiError::BadRequest(format!("{e:#}")).into()
}

fn no_sources() -> actix_web::Error {
    ApiError::BadRequest("no plugin market source is configured (pluginMarket.sources)".to_string())
        .into()
}

/// Market sources + timeout from the live config.
async fn settings(state: &web::Data<AppState>) -> (Vec<String>, u64) {
    let cfg = state.config.read().await;
    (
        cfg.plugin_market.sources.clone(),
        cfg.plugin_market.timeout_secs,
    )
}

/// Installed `id → version`, read from the same package root the loader uses.
async fn installed_versions(state: &web::Data<AppState>) -> HashMap<String, String> {
    let cfg = state.config.read().await;
    let plugins = install::list(&install::plugins_root(), &cfg.plugins);
    market::installed_versions(&plugins)
}

/// GET /api/market — every published plugin annotated with the local install state.
///
/// Broken sources come back in `errors` instead of failing the request, so one dead registry does
/// not hide the plugins from the others.
pub async fn list_market(state: web::Data<AppState>) -> Result<HttpResponse> {
    let (sources, timeout_secs) = settings(&state).await;
    let installed = installed_versions(&state).await;
    let view = market::view(&sources, timeout_secs, &installed).await;
    Ok(HttpResponse::Ok().json(ApiResponse::success(view)))
}

/// POST /api/market/install — resolve `id` + dependencies, download, verify and install.
pub async fn install_plugin(
    state: web::Data<AppState>,
    body: web::Json<InstallBody>,
) -> Result<HttpResponse> {
    let id = body.id.trim().to_string();
    if id.is_empty() {
        return Err(ApiError::BadRequest("`id` is required".to_string()).into());
    }
    let (sources, timeout_secs) = settings(&state).await;
    if sources.is_empty() {
        return Err(no_sources());
    }
    let installed = installed_versions(&state).await;
    let root = install::plugins_root();
    let trust = install::trust_store_path();

    let staged = market::fetch_packages(
        &sources,
        timeout_secs,
        &root,
        &id,
        body.version.as_deref(),
        &installed,
    )
    .await
    .map_err(to_error)?;

    let report = web::block(move || market::install_staged(&root, &trust, staged))
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?;
    finish(state, report).await
}

/// POST /api/market/update-all — install every newer version the market offers.
pub async fn update_all(state: web::Data<AppState>) -> Result<HttpResponse> {
    let (sources, timeout_secs) = settings(&state).await;
    if sources.is_empty() {
        return Err(no_sources());
    }
    let installed = installed_versions(&state).await;
    if installed.is_empty() {
        return Ok(
            HttpResponse::Ok().json(ApiResponse::success(market::MarketInstallReport::default()))
        );
    }
    let root = install::plugins_root();
    let trust = install::trust_store_path();

    let staged = market::fetch_updates(&sources, timeout_secs, &root, &installed)
        .await
        .map_err(to_error)?;
    if staged.is_empty() {
        return Ok(
            HttpResponse::Ok().json(ApiResponse::success(market::MarketInstallReport::default()))
        );
    }

    let report = web::block(move || market::install_staged(&root, &trust, staged))
        .await
        .map_err(|e| ApiError::InternalError(e.to_string()))?;
    finish(state, report).await
}

/// Enable what landed, persist the config, and turn a total failure into a 400.
///
/// Partial success is a 200: the report lists both halves, and the caller decides what to show.
async fn finish(
    state: web::Data<AppState>,
    report: market::MarketInstallReport,
) -> Result<HttpResponse> {
    if report.is_empty() {
        if let Some(first) = report.failed.first() {
            return Err(ApiError::BadRequest(format!("'{}': {}", first.id, first.error)).into());
        }
        return Ok(HttpResponse::Ok().json(ApiResponse::success(report)));
    }
    {
        let mut cfg = state.config.write().await;
        for item in &report.installed {
            let entry = cfg
                .plugins
                .entry(item.id.clone())
                .or_insert_with(|| serde_json::json!({}));
            match entry.as_object_mut() {
                Some(obj) => {
                    obj.insert("enabled".to_string(), serde_json::json!(true));
                }
                None => *entry = serde_json::json!({ "enabled": true }),
            }
        }
    }
    persist(&state).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(report)))
}
