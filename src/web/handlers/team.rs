//! `/api/subagents` — the agent team panel.
//!
//! The same pool the tools spawn into (`spawn`, `run_team`, directive routing), so the panel shows
//! background work that the chat stream alone hides. Live updates arrive as `subagent_updated`
//! events on `/ws/agent`; this API is the snapshot a freshly opened panel starts from.

use actix_web::{web, HttpResponse, Result};
use serde::Deserialize;

use crate::web::handlers::api::{ApiError, ApiResponse};
use crate::web::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CancelBody {
    #[serde(default)]
    pub all: bool,
}

/// GET /api/subagents — every teammate (running + finished), oldest first.
pub async fn list_subagents(state: web::Data<AppState>) -> Result<HttpResponse> {
    let manager = state.subagents.lock().await;
    let handles = manager.list().await;
    let running = handles.iter().filter(|h| h.is_running()).count();
    let agents: Vec<_> = handles.iter().map(|h| h.to_info()).collect();
    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "agents": agents,
            "running": running,
        }))),
    )
}

/// POST /api/subagents/{id}/cancel — stop one teammate, or `{"all": true}` to stop every one.
pub async fn cancel_subagent(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: Option<web::Json<CancelBody>>,
) -> Result<HttpResponse> {
    let id = path.into_inner();
    let all = body.map(|b| b.all).unwrap_or(false);
    let mut manager = state.subagents.lock().await;
    let cancelled = if all {
        manager.cancel_all().await
    } else if manager.cancel(&id).await {
        1
    } else {
        0
    };
    if cancelled == 0 {
        return Err(ApiError::NotFound(format!(
            "Subagent '{id}' is not running (unknown id, or it already finished)"
        ))
        .into());
    }
    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "cancelled": cancelled,
        }))),
    )
}
