//! `/api/commands` — slash-command templates for the active workspace.
//!
//! Listing only: expansion happens server-side when a message enters the agent loop, so the UI just
//! needs names and descriptions for its picker and the model always receives the expanded prompt.

use actix_web::{web, HttpResponse, Result};
use serde::Deserialize;
use std::path::PathBuf;

use crate::web::handlers::api::{ApiError, ApiResponse};
use crate::web::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CommandsQuery {
    pub project_id: Option<String>,
}

/// Resolve the folder to scan: the requested project, else the configured workspace.
async fn workspace_root(state: &AppState, project_id: Option<&str>) -> Result<PathBuf> {
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

/// GET /api/commands
pub async fn list_commands(
    state: web::Data<AppState>,
    query: web::Query<CommandsQuery>,
) -> Result<HttpResponse> {
    let root = workspace_root(&state, query.project_id.as_deref()).await?;
    let commands = web::block(move || crate::commands::load(&root))
        .await
        .map_err(|e| ApiError::InternalError(format!("command scan failed: {e}")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(commands)))
}
