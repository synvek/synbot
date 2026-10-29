//! `/api/tasks` — the persisted task plan for a project (or the default workspace).
//!
//! The agent writes the plan with `plan_write`; this API lets the panel render it, tick a task off
//! by hand, and reset it. Same file as the tool (`<workspace>/.synbot/plans/tasks.json`), so there
//! is exactly one source of truth.

use actix_web::{web, HttpResponse, Result};
use serde::Deserialize;
use std::path::PathBuf;

use crate::tasks::{TaskBoard, TaskInput, TaskStatus};
use crate::web::handlers::api::{ApiError, ApiResponse};
use crate::web::state::AppState;

#[derive(Debug, Deserialize)]
pub struct BoardQuery {
    pub project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReplaceBody {
    pub tasks: Vec<TaskInput>,
}

#[derive(Debug, Deserialize)]
pub struct StatusBody {
    pub status: String,
}

/// Resolve the folder holding the board: the requested project, else the configured workspace.
async fn board_root(state: &AppState, project_id: Option<&str>) -> Result<PathBuf> {
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

/// GET /api/tasks
pub async fn list_tasks(
    state: web::Data<AppState>,
    query: web::Query<BoardQuery>,
) -> Result<HttpResponse> {
    let root = board_root(&state, query.project_id.as_deref()).await?;
    let path = TaskBoard::path_for(&root);
    let board = web::block(move || TaskBoard::load(&root))
        .await
        .map_err(|e| ApiError::InternalError(format!("task board read failed: {e}")))?;
    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "tasks": board.tasks,
            "updated_at": board.updated_at,
            "path": path.display().to_string(),
        }))),
    )
}

/// PUT /api/tasks — replace the whole list (same contract as `plan_write`).
pub async fn replace_tasks(
    state: web::Data<AppState>,
    query: web::Query<BoardQuery>,
    body: web::Json<ReplaceBody>,
) -> Result<HttpResponse> {
    let root = board_root(&state, query.project_id.as_deref()).await?;
    let inputs = body.into_inner().tasks;
    let board = web::block(move || -> anyhow::Result<TaskBoard> {
        let mut board = TaskBoard::load(&root);
        board.replace(inputs)?;
        board.save(&root)?;
        Ok(board)
    })
    .await
    .map_err(|e| ApiError::InternalError(format!("task board write failed: {e}")))?
    .map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "tasks": board.tasks,
            "updated_at": board.updated_at,
        }))),
    )
}

/// PATCH /api/tasks/{id} — change one task's status.
pub async fn update_task(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<BoardQuery>,
    body: web::Json<StatusBody>,
) -> Result<HttpResponse> {
    let root = board_root(&state, query.project_id.as_deref()).await?;
    let id = path.into_inner();
    let status = TaskStatus::parse(&body.status).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "unknown task status '{}' (use pending/in_progress/completed/cancelled)",
            body.status
        ))
    })?;
    let board = web::block(move || -> anyhow::Result<TaskBoard> {
        let mut board = TaskBoard::load(&root);
        board.set_status(&id, status)?;
        board.save(&root)?;
        Ok(board)
    })
    .await
    .map_err(|e| ApiError::InternalError(format!("task update failed: {e}")))?
    .map_err(|e| ApiError::NotFound(format!("{e:#}")))?;
    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "tasks": board.tasks,
            "updated_at": board.updated_at,
        }))),
    )
}
