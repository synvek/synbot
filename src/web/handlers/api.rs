use actix_web::{error::ResponseError, http::StatusCode, web, HttpResponse, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use crate::web::state::AppState;
use chrono::{DateTime, Utc};

/// Standard API response wrapper
#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
        }
    }
}

/// Error response structure
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    pub fn new(error: String, code: String) -> Self {
        Self {
            error,
            code,
            details: None,
        }
    }

    pub fn with_details(error: String, code: String, details: serde_json::Value) -> Self {
        Self {
            error,
            code,
            details: Some(details),
        }
    }
}

/// Custom API error type
#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    InternalError(String),
    Unauthorized(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::NotFound(msg) => write!(f, "Not found: {}", msg),
            ApiError::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            ApiError::InternalError(msg) => write!(f, "Internal error: {}", msg),
            ApiError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
        }
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let status = self.status_code();
        let code = match self {
            ApiError::NotFound(_) => "NOT_FOUND",
            ApiError::BadRequest(_) => "BAD_REQUEST",
            ApiError::InternalError(_) => "INTERNAL_ERROR",
            ApiError::Unauthorized(_) => "UNAUTHORIZED",
        };

        let error_response = ErrorResponse::new(self.to_string(), code.to_string());

        HttpResponse::build(status).json(error_response)
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::InternalError(err.to_string())
    }
}

/// System status response
#[derive(Serialize)]
pub struct SystemStatus {
    pub running: bool,
    pub uptime_secs: u64,
    pub session_count: usize,
    pub channel_count: usize,
    pub cron_job_count: usize,
    pub role_count: usize,
}

/// GET /api/status - Returns system status
pub async fn get_status(state: web::Data<AppState>) -> Result<HttpResponse> {
    // Count enabled channels
    let channel_count = {
        let mut count = 0;
        if state.config.channels.telegram.enabled {
            count += 1;
        }
        if state.config.channels.discord.enabled {
            count += 1;
        }
        if state.config.channels.feishu.enabled {
            count += 1;
        }
        count
    };

    // Get session count
    let session_count = {
        let sm = state.session_manager.read().await;
        sm.session_count()
    };

    // Get cron job count
    let cron_job_count = {
        let cron = state.cron_service.read().await;
        cron.job_count()
    };

    // Get role count
    let role_count = state.config.agent.roles.len();

    let status = SystemStatus {
        running: true,
        uptime_secs: 0, // TODO: Track actual uptime
        session_count,
        channel_count,
        cron_job_count,
        role_count,
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(status)))
}

/// Paginated response wrapper
#[derive(Serialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}

/// Session summary for list view
#[derive(Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub channel: String,
    pub scope: String,
    pub identifier: String,
    pub message_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Query parameters for session list
#[derive(Deserialize)]
pub struct SessionQuery {
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_page_size")]
    pub page_size: usize,
    pub channel: Option<String>,
    pub scope: Option<String>,
}

fn default_page() -> usize {
    1
}

fn default_page_size() -> usize {
    20
}

/// GET /api/sessions - Returns session list with pagination and filtering
pub async fn get_sessions(
    state: web::Data<AppState>,
    query: web::Query<SessionQuery>,
) -> Result<HttpResponse> {
    let sm = state.session_manager.read().await;
    
    // Get all sessions from SessionManager
    let all_sessions = sm.get_all_sessions();
    
    // Convert to SessionSummary and apply filters
    let filtered: Vec<SessionSummary> = all_sessions
        .into_iter()
        .filter_map(|(meta, message_count)| {
            // Apply channel filter
            if let Some(ref channel) = query.channel {
                if &meta.id.channel != channel {
                    return None;
                }
            }
            
            // Apply scope filter
            if let Some(ref scope_str) = query.scope {
                let scope_match = match meta.id.scope.as_ref() {
                    Some(scope) => {
                        let scope_name = match scope {
                            crate::agent::session_id::SessionScope::Dm => "dm",
                            crate::agent::session_id::SessionScope::Group => "group",
                            crate::agent::session_id::SessionScope::Topic => "topic",
                        };
                        scope_name == scope_str
                    }
                    None => scope_str.is_empty(),
                };
                if !scope_match {
                    return None;
                }
            }
            
            // Build SessionSummary
            let scope = match meta.id.scope.as_ref() {
                Some(s) => match s {
                    crate::agent::session_id::SessionScope::Dm => "dm",
                    crate::agent::session_id::SessionScope::Group => "group",
                    crate::agent::session_id::SessionScope::Topic => "topic",
                }.to_string(),
                None => String::new(),
            };
            
            let identifier = meta.id.identifier.clone().unwrap_or_default();
            
            Some(SessionSummary {
                id: meta.id.format(),
                channel: meta.id.channel.clone(),
                scope,
                identifier,
                message_count,
                created_at: meta.created_at,
                updated_at: meta.updated_at,
            })
        })
        .collect();
    
    let total = filtered.len();
    let page = query.page.max(1);
    let page_size = query.page_size.min(100).max(1);
    let start = (page - 1) * page_size;
    
    let items = filtered
        .into_iter()
        .skip(start)
        .take(page_size)
        .collect();
    
    let response = PaginatedResponse {
        items,
        total,
        page,
        page_size,
    };
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(response)))
}

/// Session detail with full message history
#[derive(Serialize)]
pub struct SessionDetail {
    pub id: String,
    pub channel: String,
    pub scope: String,
    pub identifier: String,
    pub participants: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<SessionMessageDto>,
}

/// Message DTO for API responses
#[derive(Serialize)]
pub struct SessionMessageDto {
    pub role: String,
    pub content: String,
}

/// GET /api/sessions/{id} - Returns session details with message history
pub async fn get_session_by_id(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let session_id_str = path.into_inner();
    
    // Parse the session ID
    let session_id = crate::agent::session_id::SessionId::parse(&session_id_str)
        .map_err(|e| ApiError::BadRequest(format!("Invalid session ID: {}", e)))?;
    
    let sm = state.session_manager.read().await;
    
    // Get session metadata
    let meta = sm.get_meta(&session_id)
        .ok_or_else(|| ApiError::NotFound(format!("Session not found: {}", session_id_str)))?;
    
    // Get message history
    let messages = sm.get_history(&session_id)
        .ok_or_else(|| ApiError::NotFound(format!("Session not found: {}", session_id_str)))?;
    
    // Convert messages to DTOs
    let message_dtos: Vec<SessionMessageDto> = messages
        .iter()
        .map(|msg| SessionMessageDto {
            role: msg.role.clone(),
            content: msg.content.clone(),
        })
        .collect();
    
    // Build scope string
    let scope = match meta.id.scope.as_ref() {
        Some(s) => match s {
            crate::agent::session_id::SessionScope::Dm => "dm",
            crate::agent::session_id::SessionScope::Group => "group",
            crate::agent::session_id::SessionScope::Topic => "topic",
        }.to_string(),
        None => String::new(),
    };
    
    let identifier = meta.id.identifier.clone().unwrap_or_default();
    
    let detail = SessionDetail {
        id: meta.id.format(),
        channel: meta.id.channel.clone(),
        scope,
        identifier,
        participants: meta.participants.clone(),
        created_at: meta.created_at,
        updated_at: meta.updated_at,
        messages: message_dtos,
    };
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(detail)))
}

/// Channel information for API responses
#[derive(Serialize)]
pub struct ChannelInfo {
    pub name: String,
    pub enabled: bool,
    pub status: ChannelStatus,
}

/// Channel connection status
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatus {
    Connected,
    Disconnected,
    Error,
    Disabled,
}

/// GET /api/channels - Returns list of channels with status
pub async fn get_channels(state: web::Data<AppState>) -> Result<HttpResponse> {
    let mut channels = Vec::new();
    
    // Telegram channel
    channels.push(ChannelInfo {
        name: "telegram".to_string(),
        enabled: state.config.channels.telegram.enabled,
        status: if state.config.channels.telegram.enabled {
            // For now, assume connected if enabled
            // TODO: Add actual connection status tracking
            ChannelStatus::Connected
        } else {
            ChannelStatus::Disabled
        },
    });
    
    // Discord channel
    channels.push(ChannelInfo {
        name: "discord".to_string(),
        enabled: state.config.channels.discord.enabled,
        status: if state.config.channels.discord.enabled {
            ChannelStatus::Connected
        } else {
            ChannelStatus::Disabled
        },
    });
    
    // Feishu channel
    channels.push(ChannelInfo {
        name: "feishu".to_string(),
        enabled: state.config.channels.feishu.enabled,
        status: if state.config.channels.feishu.enabled {
            ChannelStatus::Connected
        } else {
            ChannelStatus::Disabled
        },
    });
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(channels)))
}

/// Cron job information for API responses
#[derive(Serialize)]
pub struct CronJobInfo {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub schedule: serde_json::Value,
    pub payload: serde_json::Value,
    pub state: CronJobStateDto,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// Cron job state DTO
#[derive(Serialize)]
pub struct CronJobStateDto {
    pub next_run_at_ms: Option<i64>,
    pub last_run_at_ms: Option<i64>,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
}

/// GET /api/cron - Returns list of cron jobs
pub async fn get_cron_jobs(state: web::Data<AppState>) -> Result<HttpResponse> {
    let cron = state.cron_service.read().await;
    
    let jobs: Vec<CronJobInfo> = cron
        .list_jobs()
        .iter()
        .map(|job| {
            let schedule = serde_json::to_value(&job.schedule).unwrap_or(serde_json::Value::Null);
            let payload = serde_json::to_value(&job.payload).unwrap_or(serde_json::Value::Null);
            
            CronJobInfo {
                id: job.id.clone(),
                name: job.name.clone(),
                enabled: job.enabled,
                schedule,
                payload,
                state: CronJobStateDto {
                    next_run_at_ms: job.state.next_run_at_ms,
                    last_run_at_ms: job.state.last_run_at_ms,
                    last_status: job.state.last_status.clone(),
                    last_error: job.state.last_error.clone(),
                },
                created_at_ms: job.created_at_ms,
                updated_at_ms: job.updated_at_ms,
            }
        })
        .collect();
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(jobs)))
}

/// Request body for updating cron job
#[derive(Deserialize)]
pub struct UpdateCronJobRequest {
    pub enabled: bool,
}

/// PATCH /api/cron/{id} - Update cron job enabled status
pub async fn update_cron_job(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<UpdateCronJobRequest>,
) -> Result<HttpResponse> {
    let job_id = path.into_inner();
    
    let mut cron = state.cron_service.write().await;
    
    let updated = cron
        .update_job_enabled(&job_id, body.enabled)
        .map_err(|e| ApiError::InternalError(format!("Failed to update job: {}", e)))?;
    
    if !updated {
        return Err(ApiError::NotFound(format!("Cron job not found: {}", job_id)).into());
    }
    
    // Get the updated job
    let job = cron
        .list_jobs()
        .iter()
        .find(|j| j.id == job_id)
        .ok_or_else(|| ApiError::NotFound(format!("Cron job not found: {}", job_id)))?;
    
    let schedule = serde_json::to_value(&job.schedule).unwrap_or(serde_json::Value::Null);
    let payload = serde_json::to_value(&job.payload).unwrap_or(serde_json::Value::Null);
    
    let job_info = CronJobInfo {
        id: job.id.clone(),
        name: job.name.clone(),
        enabled: job.enabled,
        schedule,
        payload,
        state: CronJobStateDto {
            next_run_at_ms: job.state.next_run_at_ms,
            last_run_at_ms: job.state.last_run_at_ms,
            last_status: job.state.last_status.clone(),
            last_error: job.state.last_error.clone(),
        },
        created_at_ms: job.created_at_ms,
        updated_at_ms: job.updated_at_ms,
    };
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(job_info)))
}

/// Role information for API responses
#[derive(Serialize)]
pub struct RoleInfo {
    pub name: String,
    pub system_prompt: String,
    pub skills: Vec<String>,
    pub tools: Vec<String>,
    pub provider: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub max_iterations: u32,
    pub workspace_dir: String,
}

/// GET /api/roles - Returns list of roles with full configuration
pub async fn get_roles(state: web::Data<AppState>) -> Result<HttpResponse> {
    let role_names = state.role_registry.list_names();
    
    let roles: Vec<RoleInfo> = role_names
        .into_iter()
        .filter_map(|name| {
            state.role_registry.get(name).map(|ctx| RoleInfo {
                name: ctx.name.clone(),
                system_prompt: ctx.system_prompt.clone(),
                skills: ctx.skills.clone(),
                tools: ctx.tools.clone(),
                provider: ctx.params.provider.clone(),
                model: ctx.params.model.clone(),
                max_tokens: ctx.params.max_tokens,
                temperature: ctx.params.temperature,
                max_iterations: ctx.params.max_iterations,
                workspace_dir: ctx.workspace_dir.to_string_lossy().to_string(),
            })
        })
        .collect();
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(roles)))
}

/// Skill information for API responses
#[derive(Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub assigned_roles: Vec<String>,
}

/// Skill detail with content
#[derive(Serialize)]
pub struct SkillDetail {
    pub name: String,
    pub content: String,
    pub assigned_roles: Vec<String>,
}

/// GET /api/skills - Returns list of available skills
pub async fn get_skills(state: web::Data<AppState>) -> Result<HttpResponse> {
    let skill_names = state.skills_loader.list_skills();
    let role_names = state.role_registry.list_names();
    
    let skills: Vec<SkillInfo> = skill_names
        .into_iter()
        .map(|skill_name| {
            // Find which roles have this skill assigned
            let assigned_roles: Vec<String> = role_names
                .iter()
                .filter_map(|role_name| {
                    state.role_registry.get(role_name).and_then(|ctx| {
                        if ctx.skills.contains(&skill_name) {
                            Some((*role_name).to_string())
                        } else {
                            None
                        }
                    })
                })
                .collect();
            
            SkillInfo {
                name: skill_name,
                assigned_roles,
            }
        })
        .collect();
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(skills)))
}

/// GET /api/skills/{name} - Returns skill detail with SKILL.md content
pub async fn get_skill_by_name(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let skill_name = path.into_inner();
    
    // Load skill content
    let content = state
        .skills_loader
        .load_skill(&skill_name)
        .ok_or_else(|| ApiError::NotFound(format!("Skill not found: {}", skill_name)))?;
    
    // Find which roles have this skill assigned
    let role_names = state.role_registry.list_names();
    let assigned_roles: Vec<String> = role_names
        .iter()
        .filter_map(|role_name| {
            state.role_registry.get(role_name).and_then(|ctx| {
                if ctx.skills.contains(&skill_name) {
                    Some((*role_name).to_string())
                } else {
                    None
                }
            })
        })
        .collect();
    
    let detail = SkillDetail {
        name: skill_name,
        content,
        assigned_roles,
    };
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(detail)))
}

/// GET /api/config - Returns sanitized configuration
pub async fn get_config(state: web::Data<AppState>) -> Result<HttpResponse> {
    use crate::web::handlers::sanitize::sanitize_config;
    
    let sanitized = sanitize_config(&state.config);
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(sanitized)))
}

/// Query parameters for log filtering
#[derive(Deserialize)]
pub struct LogQuery {
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_page_size")]
    pub page_size: usize,
    pub level: Option<String>,
    pub keyword: Option<String>,
}

/// GET /api/logs - Returns log entries with filtering
pub async fn get_logs(
    state: web::Data<AppState>,
    query: web::Query<LogQuery>,
) -> Result<HttpResponse> {
    let log_buffer = state.log_buffer.read().await;
    
    // Get filtered logs
    let filtered_logs = log_buffer.get_filtered(
        query.level.as_deref(),
        query.keyword.as_deref(),
    );
    
    let total = filtered_logs.len();
    let page = query.page.max(1);
    let page_size = query.page_size.min(100).max(1);
    let start = (page - 1) * page_size;
    
    let items = filtered_logs
        .into_iter()
        .skip(start)
        .take(page_size)
        .collect();
    
    let response = PaginatedResponse {
        items,
        total,
        page,
        page_size,
    };
    
    Ok(HttpResponse::Ok().json(ApiResponse::success(response)))
}

