//! `/api/security/*` end-to-end tests.
//!
//! Two process globals are involved: the config root (resolved through `config::config_dir()`) and
//! the audit trail (`audit::audit()`). Both are shared by every test in the binary, so the whole
//! scenario lives in one sequential test.
//!
//! Run with: `cargo test --test security_api_tests`

use std::sync::Arc;

use actix_web::{test, web, App};
use synbot::security::audit::{self, SecurityKind, SecurityOutcome};
use synbot::tools::approval::ApprovalManager;
use synbot::tools::permission::{PermissionLevel, PermissionRule};
use synbot::web::handlers::security;
use synbot::web::state::AppState;

mod common;

async fn create_state() -> AppState {
    let (inbound_tx, _) = tokio::sync::mpsc::channel(100);
    let (outbound_tx, _) = tokio::sync::broadcast::channel(100);
    common::create_test_app_state_with_approval(
        inbound_tx,
        outbound_tx,
        Arc::new(ApprovalManager::new()),
    )
    .await
}

fn rule(pattern: &str, level: PermissionLevel) -> PermissionRule {
    PermissionRule {
        pattern: pattern.to_string(),
        level,
        description: None,
    }
}

#[actix_web::test]
async fn security_surface_reports_policy_and_trail() {
    let root = tempfile::tempdir().expect("security root");
    synbot::config::set_root_dir(Some(root.path().to_path_buf()));
    audit::audit().clear();

    let state = create_state().await;
    let config_handle = state.config.clone();
    {
        // A policy where patterns win first, one rule escalates, and everything else is allowed.
        let mut cfg = config_handle.write().await;
        cfg.tools.exec.deny_patterns = vec!["rm -rf /".to_string()];
        cfg.tools.exec.allow_patterns = None;
        cfg.tools.exec.permissions.enabled = true;
        cfg.tools.exec.permissions.default_level = PermissionLevel::Allow;
        cfg.tools.exec.permissions.approval_timeout_secs = 42;
        cfg.tools.exec.permissions.rules =
            vec![rule("git push*", PermissionLevel::RequireApproval)];
    }

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/security/overview", web::get().to(security::overview))
            .route("/api/security/events", web::get().to(security::list_events))
            .route(
                "/api/security/events",
                web::delete().to(security::clear_events),
            )
            .route(
                "/api/security/policy",
                web::post().to(security::explain_policy),
            ),
    )
    .await;

    // --- overview: the effective configuration, before anything was recorded ------------------
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/security/overview")
            .to_request(),
    )
    .await;
    assert_eq!(body["success"], true, "{body}");
    let data = &body["data"];
    let profile = data["profile"].as_str().expect("profile");
    assert!(
        ["safe", "developer", "trusted"].contains(&profile),
        "{profile}"
    );
    assert!(!data["platform"]["os"]
        .as_str()
        .unwrap_or_default()
        .is_empty());
    assert_eq!(data["exec"]["permissions_enabled"], true);
    assert_eq!(data["exec"]["default_level"], "allow");
    assert_eq!(data["exec"]["rule_count"], 1);
    assert_eq!(data["exec"]["approval_timeout_secs"], 42);
    assert_eq!(data["exec"]["deny_patterns"][0], "rm -rf /");
    assert_eq!(data["secrets"]["masking"], true);
    assert!(data["secrets"]["patterns"].as_u64().unwrap_or(0) > 0);
    // Nothing has been recorded yet in this process.
    assert_eq!(data["audit"]["retained"], 0);
    assert_eq!(data["audit"]["capacity"], audit::DEFAULT_CAPACITY as u64);
    assert_eq!(data["audit"]["dropped"], 0);

    // --- policy: dry-run, ordered exactly like the exec tool ---------------------------------
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/security/policy")
            .set_json(serde_json::json!({ "command": "rm -rf /" }))
            .to_request(),
    )
    .await;
    assert_eq!(body["success"], true, "{body}");
    assert_eq!(body["data"]["final"], "blocked");
    assert_eq!(
        body["data"]["blocked_by_pattern"],
        "Command rejected: matches deny pattern 'rm -rf /'. Command: rm -rf /"
    );
    assert_eq!(body["data"]["matched_rule"], serde_json::Value::Null);
    assert_eq!(body["data"]["default_applied"], true);

    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/security/policy")
            .set_json(serde_json::json!({ "command": "git push origin main" }))
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["final"], "needs_approval");
    assert_eq!(body["data"]["level"], "require_approval");
    assert_eq!(body["data"]["matched_rule"]["pattern"], "git push*");
    assert_eq!(body["data"]["default_applied"], false);
    assert_eq!(body["data"]["blocked_by_pattern"], serde_json::Value::Null);

    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/security/policy")
            .set_json(serde_json::json!({ "command": "ls -la" }))
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["final"], "allowed");
    assert_eq!(body["data"]["matched_rule"], serde_json::Value::Null);
    assert_eq!(body["data"]["default_applied"], true);

    // A dry run never becomes an audit event, otherwise the page would pollute its own trail.
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/security/events")
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["retained"], 0);

    // Blank and missing commands are client errors.
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/security/policy")
            .set_json(serde_json::json!({ "command": "   " }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/security/policy")
            .set_json(serde_json::json!({}))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);

    // --- events: newest first, filterable, clearable -----------------------------------------
    audit::record(
        SecurityKind::CommandBlocked,
        SecurityOutcome::Blocked,
        "rm -rf /",
        "command matches deny pattern rm -rf /",
    );
    audit::record(
        SecurityKind::PathEscape,
        SecurityOutcome::Blocked,
        "../outside.txt",
        "resolved outside the workspace",
    );
    audit::record(
        SecurityKind::ApprovalTimeout,
        SecurityOutcome::Blocked,
        "git push",
        "no response within 42s",
    );

    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/security/events")
            .to_request(),
    )
    .await;
    assert_eq!(body["success"], true, "{body}");
    let events = body["data"]["events"].as_array().unwrap();
    assert_eq!(events.len(), 3);
    // Newest first.
    assert_eq!(events[0]["kind"], "approval_timeout");
    assert_eq!(events[0]["outcome"], "blocked");
    assert_eq!(events[0]["subject"], "git push");
    assert_eq!(events[2]["kind"], "command_blocked");
    assert!(events[0]["id"].as_u64() > events[2]["id"].as_u64());
    assert_eq!(body["data"]["retained"], 3);

    let counts = body["data"]["counts"].as_array().unwrap();
    let blocked = counts
        .iter()
        .find(|c| c["kind"] == "command_blocked")
        .expect("command_blocked count");
    assert_eq!(blocked["count"], 1);

    // Filter by kind, and bound the result with `limit`.
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/security/events?kind=path_escape")
            .to_request(),
    )
    .await;
    let events = body["data"]["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["kind"], "path_escape");

    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/security/events?kind=all&limit=2")
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["events"].as_array().unwrap().len(), 2);

    // An unknown kind is rejected rather than silently returning everything.
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/security/events?kind=nope")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);

    // The overview reflects the trail.
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/security/overview")
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["audit"]["retained"], 3);

    // Clearing empties the trail and reports how much was dropped.
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::delete()
            .uri("/api/security/events")
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["cleared"], 3);
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/security/events")
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["events"].as_array().unwrap().len(), 0);
    assert_eq!(body["data"]["retained"], 0);

    synbot::config::set_root_dir(None);
}
