//! `/api/git` — repository status/diff/log for the active workspace.
//!
//! Read-only on purpose: the panel shows what `git_status` shows the agent, and any mutation goes
//! through the agent's `exec` tool where approval applies.

use actix_web::{web, HttpResponse, Result};
use serde::Deserialize;
use std::path::PathBuf;

use crate::web::handlers::api::{ApiError, ApiResponse};
use crate::web::state::AppState;

#[derive(Debug, Deserialize)]
pub struct RepoQuery {
    /// Project to inspect; without it the agent workspace is used.
    pub project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DiffQuery {
    pub project_id: Option<String>,
    pub path: Option<String>,
    #[serde(default)]
    pub staged: bool,
}

#[derive(Debug, Deserialize)]
pub struct LogQuery {
    pub project_id: Option<String>,
    pub path: Option<String>,
    pub limit: Option<usize>,
}

/// Resolve the folder to inspect: the requested project, else the configured workspace.
async fn repo_path(state: &AppState, project_id: Option<&str>) -> Result<PathBuf> {
    if let Some(id) = project_id {
        let store = state.projects.read().await;
        let project = store
            .get(id)
            .ok_or_else(|| ApiError::NotFound(format!("Project not found: {id}")))?;
        return Ok(project.path.clone());
    }
    let config = state.config.read().await;
    let configured =
        crate::config::normalize_workspace_path(std::path::Path::new(&config.main_agent.workspace));
    Ok(configured)
}

/// GET /api/git/status
pub async fn get_status(
    state: web::Data<AppState>,
    query: web::Query<RepoQuery>,
) -> Result<HttpResponse> {
    let root = repo_path(&state, query.project_id.as_deref()).await?;
    let status = web::block(move || crate::git::status(&root))
        .await
        .map_err(|e| ApiError::InternalError(format!("git status failed: {e}")))?
        .map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(status)))
}

/// GET /api/git/diff
pub async fn get_diff(
    state: web::Data<AppState>,
    query: web::Query<DiffQuery>,
) -> Result<HttpResponse> {
    let root = repo_path(&state, query.project_id.as_deref()).await?;
    let path = query.path.clone().filter(|p| !p.trim().is_empty());
    let staged = query.staged;
    let diff = web::block(move || {
        crate::git::diff(&root, path.as_deref(), staged, crate::git::MAX_DIFF_LINES)
    })
    .await
    .map_err(|e| ApiError::InternalError(format!("git diff failed: {e}")))?
    .map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({ "diff": diff }))))
}

/// GET /api/git/log
pub async fn get_log(
    state: web::Data<AppState>,
    query: web::Query<LogQuery>,
) -> Result<HttpResponse> {
    let root = repo_path(&state, query.project_id.as_deref()).await?;
    let path = query.path.clone().filter(|p| !p.trim().is_empty());
    let limit = query.limit.unwrap_or(20).min(200);
    let commits = web::block(move || crate::git::log(&root, limit, path.as_deref()))
        .await
        .map_err(|e| ApiError::InternalError(format!("git log failed: {e}")))?
        .map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(commits)))
}
