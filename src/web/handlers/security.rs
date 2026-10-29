//! `/api/security` — what the agent is allowed to do, and what it has been stopped from doing.
//!
//! - `overview`: the effective configuration (profile, sandbox backends, exec policy, plugin
//!   packages) in one payload, so the UI never has to recompute the defaults itself.
//! - `events`: the process-wide audit trail ([`crate::security::audit`]).
//! - `policy`: dry-run one command against the deny/allow patterns and the permission rules,
//!   reporting *which* rule decided it. Read-only: a dry run must not pollute the audit trail.

use actix_web::{web, HttpResponse, Result};
use serde::Deserialize;
use serde_json::json;

use crate::config;
use crate::plugin::install;
use crate::sandbox::types::ToolSandboxExecKind;
use crate::sandbox::PlatformDetector;
use crate::security::audit::{self, SecurityKind};
use crate::security::SecretMaskerLayer;
use crate::tools::permission::{CommandPermissionPolicy, PermissionLevel};
use crate::tools::shell::CommandPolicy;
use crate::web::handlers::api::{ApiError, ApiResponse};
use crate::web::state::AppState;

/// Retention cap of the trail, so the UI can show "showing N of M".
const MAX_EVENT_LIMIT: usize = 500;

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
    pub limit: Option<usize>,
    /// A [`SecurityKind::key`] value; absent or `all` means no filter.
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ExplainBody {
    pub command: String,
}

fn exec_kind_label(kind: ToolSandboxExecKind) -> &'static str {
    match kind {
        ToolSandboxExecKind::Docker => "docker",
        ToolSandboxExecKind::HostNative => "host_native",
    }
}

fn level_label(level: PermissionLevel) -> &'static str {
    match level {
        PermissionLevel::Allow => "allow",
        PermissionLevel::RequireApproval => "require_approval",
        PermissionLevel::Deny => "deny",
    }
}

/// GET /api/security/overview
pub async fn overview(state: web::Data<AppState>) -> Result<HttpResponse> {
    let cfg = state.config.read().await;
    let platform = PlatformDetector::detect();
    let exec = &cfg.tools.exec;
    let permissions = &exec.permissions;

    let tool_sandbox = cfg.tool_sandbox.as_ref().map(|sandbox| {
        json!({
            "backend": sandbox.sandbox_type.as_deref().unwrap_or("gvisor-docker"),
            "backend_explicit": sandbox.sandbox_type.is_some(),
            "exec_kind": exec_kind_label(config::tool_sandbox_exec_kind(sandbox)),
            "image": sandbox.image,
            "network": sandbox.network.as_ref().map(|n| json!({
                "enabled": n.enabled,
                "hosts": n.allowed_hosts,
                "ports": n.allowed_ports,
            })),
            "filesystem": sandbox.filesystem.as_ref().map(|f| json!({
                "readonly": f.readonly_paths,
                "writable": f.writable_paths,
                "hidden": f.hidden_paths,
            })),
            "resources": sandbox.resources.as_ref().map(|r| json!({
                "max_memory": r.max_memory,
                "max_cpu": r.max_cpu,
                "max_disk": r.max_disk,
            })),
        })
    });

    let app_sandbox = cfg.app_sandbox.as_ref().map(|sandbox| {
        json!({
            "platform": sandbox.platform,
            "work_dir": sandbox.work_dir,
        })
    });

    // Cheap on purpose: no wasm hashing here, the Plugins page owns trust verdicts.
    let plugins_root = install::plugins_root();
    let packaged = cfg
        .plugins
        .keys()
        .filter(|id| {
            plugins_root
                .join(id)
                .join(crate::plugin::manifest::MANIFEST_FILE)
                .exists()
        })
        .count();

    let trail = audit::audit();

    Ok(HttpResponse::Ok().json(ApiResponse::success(json!({
        "profile": cfg.security_profile,
        "platform": {
            "os": platform.os,
            "version": platform.version,
            "arch": platform.arch,
            "supported": platform.supported,
            "recommended_app_sandbox": platform.recommended_app_sandbox,
            "recommended_tool_sandbox": platform.recommended_tool_sandbox,
        },
        "workspace": cfg.main_agent.workspace,
        "app_sandbox": app_sandbox,
        "tool_sandbox": tool_sandbox,
        "exec": {
            "timeout_secs": exec.timeout_secs,
            "restrict_to_workspace": exec.restrict_to_workspace,
            "permissions_enabled": permissions.enabled,
            "default_level": permissions.default_level,
            "rule_count": permissions.rules.len(),
            "approval_timeout_secs": permissions.approval_timeout_secs,
            "deny_patterns": exec.deny_patterns,
            "allow_patterns": exec.allow_patterns,
        },
        "secrets": {
            // Masking is installed unconditionally when logging starts.
            "masking": true,
            "patterns": SecretMaskerLayer::pattern_count(),
        },
        "plugins": {
            "configured": cfg.plugins.len(),
            "packaged": packaged,
            "legacy": cfg.plugins.len().saturating_sub(packaged),
        },
        "audit": {
            "capacity": trail.capacity(),
            "retained": trail.len(),
            "dropped": trail.dropped(),
            "counts": trail.counts(),
        },
    }))))
}

/// GET /api/security/events?limit=&kind=
pub async fn list_events(query: web::Query<EventsQuery>) -> Result<HttpResponse> {
    let kind =
        match query.kind.as_deref().map(str::trim) {
            None | Some("") | Some("all") => None,
            Some(key) => Some(SecurityKind::from_key(key).ok_or_else(|| {
                ApiError::BadRequest(format!("unknown security event kind '{key}'"))
            })?),
        };
    let limit = query.limit.unwrap_or(50).clamp(1, MAX_EVENT_LIMIT);
    let trail = audit::audit();

    Ok(HttpResponse::Ok().json(ApiResponse::success(json!({
        "events": trail.snapshot(kind, limit),
        "counts": trail.counts(),
        "capacity": trail.capacity(),
        "retained": trail.len(),
        "dropped": trail.dropped(),
    }))))
}

/// DELETE /api/security/events — drop the in-memory trail.
pub async fn clear_events() -> Result<HttpResponse> {
    let trail = audit::audit();
    let cleared = trail.len();
    trail.clear();
    Ok(HttpResponse::Ok().json(ApiResponse::success(json!({ "cleared": cleared }))))
}

/// POST /api/security/policy — explain one command without executing or auditing it.
pub async fn explain_policy(
    state: web::Data<AppState>,
    body: web::Json<ExplainBody>,
) -> Result<HttpResponse> {
    let command = body.command.trim();
    if command.is_empty() {
        return Err(ApiError::BadRequest("`command` is required".to_string()).into());
    }

    let cfg = state.config.read().await;
    let exec = &cfg.tools.exec;
    let permissions = &exec.permissions;

    // 1. Hard deny/allow patterns run before the permission policy in the exec tool.
    let blocked_by_pattern =
        CommandPolicy::new(exec.deny_patterns.clone(), exec.allow_patterns.clone())
            .validate(command)
            .err();

    // 2. The rule table decides allow / require_approval / deny.
    let verdict =
        CommandPermissionPolicy::new(permissions.rules.clone(), permissions.default_level)
            .explain(command);

    let blocked = blocked_by_pattern.is_some()
        || (permissions.enabled && verdict.level == PermissionLevel::Deny);
    let final_decision = if blocked {
        "blocked"
    } else if !permissions.enabled || verdict.level == PermissionLevel::Allow {
        "allowed"
    } else {
        "needs_approval"
    };

    Ok(HttpResponse::Ok().json(ApiResponse::success(json!({
        "command": command,
        "final": final_decision,
        "blocked_by_pattern": blocked_by_pattern,
        "permissions_enabled": permissions.enabled,
        "level": level_label(verdict.level),
        "matched_rule": verdict.matched_rule,
        "default_applied": verdict.default_applied,
        "approval_timeout_secs": permissions.approval_timeout_secs,
    }))))
}
