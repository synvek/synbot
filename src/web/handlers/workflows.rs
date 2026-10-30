//! `/api/workflows` — the workbench view of TurboWorkflow runs.
//!
//! The runner (`src/workflow/runner.rs`) persists one `WorkflowState` per session under
//! `workflows_root`, so this API is a read-mostly projection of that folder plus two control
//! actions:
//!
//! * **run** — resume a run that paused/failed/timed out, or restart it from step 0;
//! * **cancel** — mark a non-finished run as cancelled so `/workflow continue` refuses to pick it
//!   up again.
//!
//! `cancel` only edits the persisted state: a run that is *executing right now* keeps going until
//! its current step finishes, because the runner owns its copy of the state and rewrites the file.
//! Anything else would require a per-session cancel token map, which the chat path already handles
//! through `/stop`.
//!
//! Session keys (`channel:chat_id`) travel as a query parameter rather than a path segment: they
//! contain colons, and some chat ids contain `+`/`=` (DingTalk), which are hostile to path parsing.

use std::collections::HashMap;

use actix_web::{web, HttpResponse, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::web::handlers::api::{ApiError, ApiResponse};
use crate::web::state::AppState;
use crate::workflow::store::WorkflowStore;
use crate::workflow::types::{WorkflowState, WorkflowStatus};

#[derive(Debug, Deserialize)]
pub struct WorkflowQuery {
    /// Session key (`channel:chat_id`) identifying one run.
    pub session: Option<String>,
    /// `true` clears inputs/step outputs and runs from step 0.
    #[serde(default)]
    pub restart: bool,
}

/// One row of the run list.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowSummary {
    pub session_key: String,
    pub workflow_id: String,
    pub name: String,
    pub description: String,
    pub status: WorkflowStatus,
    pub current_step_index: usize,
    pub steps_total: usize,
    /// Steps that produced output — `step_outputs` is append-only, so its length is the progress.
    pub steps_completed: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// False once the run finished; the UI then offers "restart" instead of "resume".
    pub can_resume: bool,
}

/// One step of the definition, with the output it produced (if any).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepView {
    pub id: String,
    #[serde(rename = "type")]
    pub step_type: String,
    pub description: String,
    /// `completed` | `current` | `pending`
    pub status: &'static str,
    pub output: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDetail {
    #[serde(flatten)]
    pub summary: WorkflowSummary,
    pub inputs: HashMap<String, String>,
    pub user_input_timeout_secs: u64,
    pub steps: Vec<WorkflowStepView>,
}

fn summary(state: &WorkflowState) -> WorkflowSummary {
    WorkflowSummary {
        session_key: state.session_key.clone(),
        workflow_id: state.workflow_id.clone(),
        name: state.definition.name.clone(),
        description: state.definition.description.clone(),
        status: state.status,
        current_step_index: state.current_step_index,
        steps_total: state.definition.steps.len(),
        steps_completed: state.step_outputs.len().min(state.definition.steps.len()),
        created_at: state.created_at,
        updated_at: state.updated_at,
        can_resume: !state.is_finished(),
    }
}

fn detail(state: &WorkflowState) -> WorkflowDetail {
    let steps = state
        .definition
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let output = state.step_outputs.get(index).cloned();
            let status = if output.is_some() {
                "completed"
            } else if index == state.current_step_index && !state.is_finished() {
                "current"
            } else {
                "pending"
            };
            WorkflowStepView {
                id: step.id.clone(),
                step_type: step.step_type.clone(),
                description: step.description.clone(),
                status,
                output,
            }
        })
        .collect();
    WorkflowDetail {
        summary: summary(state),
        inputs: state.inputs.clone(),
        user_input_timeout_secs: state.user_input_timeout_secs,
        steps,
    }
}

async fn workflow_store(state: &AppState) -> WorkflowStore {
    let root = {
        let config = state.config.read().await;
        crate::config::workflows_root(&config)
    };
    WorkflowStore::new(&root)
}

fn require_session(query: &WorkflowQuery) -> Result<String, ApiError> {
    match query.session.as_deref().map(str::trim) {
        Some(session) if !session.is_empty() => Ok(session.to_string()),
        _ => Err(ApiError::BadRequest(
            "`session` (channel:chat_id) is required".into(),
        )),
    }
}

/// GET /api/workflows — every persisted run, newest first.
pub async fn list_workflows(state: web::Data<AppState>) -> Result<HttpResponse> {
    let store = workflow_store(&state).await;
    let states = store
        .list_states()
        .await
        .map_err(|e| ApiError::InternalError(format!("workflow scan failed: {e:#}")))?;
    let items: Vec<WorkflowSummary> = states.iter().map(summary).collect();
    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "workflows": items,
        }))),
    )
}

/// GET /api/workflows/detail?session=… — definition + per-step output.
pub async fn get_workflow(
    state: web::Data<AppState>,
    query: web::Query<WorkflowQuery>,
) -> Result<HttpResponse> {
    let session = require_session(&query)?;
    let state_file = workflow_store(&state)
        .await
        .load_state(&session)
        .await
        .map_err(|e| ApiError::InternalError(format!("workflow read failed: {e:#}")))?;
    match state_file {
        Some(found) => Ok(HttpResponse::Ok().json(ApiResponse::success(detail(&found)))),
        None => Err(ApiError::NotFound(format!("no workflow for session '{session}'")).into()),
    }
}

/// POST /api/workflows/run?session=…&restart=… — resume or restart through the agent loop.
///
/// The loop is taken with `try_lock`: a chat turn holds it for its whole duration, and queuing an
/// HTTP request behind that would hang the settings page. The caller is told to retry instead.
pub async fn run_workflow(
    state: web::Data<AppState>,
    query: web::Query<WorkflowQuery>,
) -> Result<HttpResponse> {
    let session = require_session(&query)?;
    let restart = query.restart;

    let loaded = workflow_store(&state)
        .await
        .load_state(&session)
        .await
        .map_err(|e| ApiError::InternalError(format!("workflow read failed: {e:#}")))?;
    let Some(loaded) = loaded else {
        return Err(ApiError::NotFound(format!("no workflow for session '{session}'")).into());
    };
    if !restart && loaded.is_finished() {
        return Err(ApiError::Conflict(
            "workflow already ended; use restart to run it again from the first step".into(),
        )
        .into());
    }

    let Some(agent_loop) = state.agent_loop.clone() else {
        return Err(ApiError::InternalError(
            "the workflow runner is not available in this process".into(),
        )
        .into());
    };
    // `try_lock` on purpose: see the doc comment above.
    let Ok(mut loop_guard) = agent_loop.try_lock() else {
        return Err(ApiError::Conflict(
            "the agent is busy with another turn; retry when it finishes".into(),
        )
        .into());
    };
    let outcome = loop_guard
        .web_run_workflow(&session, restart)
        .await
        .map_err(|e| ApiError::BadRequest(format!("{e:#}")))?;
    drop(loop_guard);

    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "sessionKey": session,
            "action": outcome,
        }))),
    )
}

/// POST /api/workflows/cancel?session=… — mark a non-finished run as cancelled.
pub async fn cancel_workflow(
    state: web::Data<AppState>,
    query: web::Query<WorkflowQuery>,
) -> Result<HttpResponse> {
    let session = require_session(&query)?;
    let store = workflow_store(&state).await;
    let loaded = store
        .load_state(&session)
        .await
        .map_err(|e| ApiError::InternalError(format!("workflow read failed: {e:#}")))?;
    let Some(mut found) = loaded else {
        return Err(ApiError::NotFound(format!("no workflow for session '{session}'")).into());
    };
    if found.is_finished() {
        return Err(
            ApiError::Conflict(format!("workflow already ended ({:?})", found.status)).into(),
        );
    }
    found.status = WorkflowStatus::Cancelled;
    found.updated_at = Utc::now();
    store
        .save_state(&session, &found)
        .await
        .map_err(|e| ApiError::InternalError(format!("workflow write failed: {e:#}")))?;
    Ok(HttpResponse::Ok().json(ApiResponse::success(summary(&found))))
}

/// DELETE /api/workflows?session=… — forget a run (its chat history is untouched).
pub async fn delete_workflow(
    state: web::Data<AppState>,
    query: web::Query<WorkflowQuery>,
) -> Result<HttpResponse> {
    let session = require_session(&query)?;
    let store = workflow_store(&state).await;
    store
        .delete_state(&session)
        .await
        .map_err(|e| ApiError::InternalError(format!("workflow delete failed: {e:#}")))?;
    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "sessionKey": session,
            "deleted": true,
        }))),
    )
}
