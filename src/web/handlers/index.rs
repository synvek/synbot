//! `/api/index` — the repository symbol index behind the files panel's symbol search.
//!
//! Same file the agent's `find_symbols` tool reads (`<workspace>/.synbot/index/code.json`). Lookups
//! build the index on first use, exactly like the tool, so the panel works before the agent has ever
//! touched the workspace.

use actix_web::{web, HttpResponse, Result};
use serde::Deserialize;
use std::path::PathBuf;

use crate::codeindex::CodeIndex;
use crate::web::handlers::api::{ApiError, ApiResponse};
use crate::web::state::AppState;

#[derive(Debug, Deserialize)]
pub struct IndexQuery {
    pub project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SymbolsQuery {
    pub project_id: Option<String>,
    pub q: String,
    pub limit: Option<usize>,
}

/// Resolve the folder to index: the requested project, else the configured workspace.
async fn index_root(state: &AppState, project_id: Option<&str>) -> Result<PathBuf> {
    if let Some(id) = project_id {
        let store = state.projects.read().await;
        let project = store
            .get(id)
            .ok_or_else(|| ApiError::NotFound(format!("Project not found: {id}")))?;
        return Ok(project.path.clone());
    }
    let config = state.config.read().await;
    Ok(crate::config::normalize_workspace_path(
        std::path::Path::new(&config.main_agent.workspace),
    ))
}

/// GET /api/index — index summary (counts, languages, module sizes).
pub async fn get_index(
    state: web::Data<AppState>,
    query: web::Query<IndexQuery>,
) -> Result<HttpResponse> {
    let root = index_root(&state, query.project_id.as_deref()).await?;
    let summary = web::block(move || {
        let path = CodeIndex::path_for(&root);
        let mut summary = if CodeIndex::is_built(&root) {
            CodeIndex::load(&root).summary()
        } else {
            CodeIndex::empty(&root).summary()
        };
        // Report the real path even when nothing was ever indexed.
        summary.path = path.display().to_string();
        summary
    })
    .await
    .map_err(|e| ApiError::InternalError(format!("index read failed: {e}")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(summary)))
}

/// POST /api/index/refresh — rebuild incrementally and return the stats.
pub async fn refresh_index(
    state: web::Data<AppState>,
    query: web::Query<IndexQuery>,
) -> Result<HttpResponse> {
    let root = index_root(&state, query.project_id.as_deref()).await?;
    let stats = web::block(move || -> anyhow::Result<_> {
        let mut index = CodeIndex::load(&root);
        index.refresh(&root)
    })
    .await
    .map_err(|e| ApiError::InternalError(format!("index refresh failed: {e}")))?
    .map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(stats)))
}

/// GET /api/index/symbols?q=&limit= — ranked symbol lookup.
pub async fn search_symbols(
    state: web::Data<AppState>,
    query: web::Query<SymbolsQuery>,
) -> Result<HttpResponse> {
    let root = index_root(&state, query.project_id.as_deref()).await?;
    let needle = query.q.trim().to_string();
    if needle.is_empty() {
        return Err(ApiError::BadRequest("`q` is required".to_string()).into());
    }
    let limit = query.limit.unwrap_or(20);
    let (hits, files) = web::block(move || -> anyhow::Result<_> {
        let mut index = CodeIndex::load(&root);
        if index.files.is_empty() {
            index.refresh(&root)?;
        }
        let hits = index.search(&needle, limit);
        Ok((hits, index.files.len()))
    })
    .await
    .map_err(|e| ApiError::InternalError(format!("symbol search failed: {e}")))?
    .map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "hits": hits,
            "files": files,
        }))),
    )
}

/// POST /api/index/wiki — (re)generate `.synbot/repowiki/*.md`.
pub async fn write_wiki(
    state: web::Data<AppState>,
    query: web::Query<IndexQuery>,
) -> Result<HttpResponse> {
    let root = index_root(&state, query.project_id.as_deref()).await?;
    let report = web::block(move || -> anyhow::Result<_> {
        let mut index = CodeIndex::load(&root);
        if index.files.is_empty() {
            index.refresh(&root)?;
        }
        index.write_wiki(&root)
    })
    .await
    .map_err(|e| ApiError::InternalError(format!("wiki write failed: {e}")))?
    .map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(report)))
}
