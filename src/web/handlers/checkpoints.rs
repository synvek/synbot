//! `/api/checkpoints` — review and undo what a turn wrote.
//!
//! A checkpoint is created by the agent loop before each mutating filesystem tool call, keyed by
//! `turn_id` (the same id carried by `TurnStarted` / `FileChanged` events). These routes only read
//! and restore it; nothing here can write outside the paths the turn itself touched.

use actix_web::{web, HttpResponse, Result};
use serde::Deserialize;

use crate::web::handlers::api::{ApiError, ApiResponse};
use crate::web::state::AppState;

const DEFAULT_LIMIT: usize = 20;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub limit: Option<usize>,
}

/// GET /api/checkpoints — newest checkpoints first (no file payloads).
pub async fn list_checkpoints(
    state: web::Data<AppState>,
    query: web::Query<ListQuery>,
) -> Result<HttpResponse> {
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).min(100);
    let store = state.checkpoints.clone();
    let items = web::block(move || store.recent(limit))
        .await
        .map_err(|e| ApiError::InternalError(format!("checkpoint scan failed: {e}")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(items)))
}

/// GET /api/checkpoints/{turn_id} — one turn with per-file diffs.
pub async fn get_checkpoint(
    state: web::Data<AppState>,
    turn_id: web::Path<String>,
) -> Result<HttpResponse> {
    let turn_id = turn_id.into_inner();
    let store = state.checkpoints.clone();
    let view = web::block(move || store.view(&turn_id))
        .await
        .map_err(|e| ApiError::InternalError(format!("checkpoint read failed: {e}")))?;
    match view {
        Some(view) => Ok(HttpResponse::Ok().json(ApiResponse::success(view))),
        None => Err(ApiError::NotFound("checkpoint not found".into()).into()),
    }
}

/// POST /api/checkpoints/{turn_id}/undo — restore every file the turn changed.
///
/// Idempotent: files that were already restored are simply written again. Files that could not be
/// snapshotted (binary, too large) are reported in `skipped` instead of failing the request.
pub async fn undo_checkpoint(
    state: web::Data<AppState>,
    turn_id: web::Path<String>,
) -> Result<HttpResponse> {
    let turn_id = turn_id.into_inner();
    let store = state.checkpoints.clone();
    let result = web::block(move || store.undo(&turn_id))
        .await
        .map_err(|e| ApiError::InternalError(format!("checkpoint undo failed: {e}")))?;
    match result {
        Ok(report) => Ok(HttpResponse::Ok().json(ApiResponse::success(report))),
        Err(e) => Err(ApiError::NotFound(format!("{e:#}")).into()),
    }
}
