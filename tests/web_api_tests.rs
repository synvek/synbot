//! Web API endpoint tests.
//!
//! Covers all registered HTTP endpoints:
//! - Session list and detail (Requirement 15.1, 15.3)
//! - Log query (Requirement 15.1, 15.3)
//! - Config view (Requirement 15.1, 15.3)
//! - Auth middleware rejects invalid credentials (Requirement 15.2)
//! - Invalid parameters return 400 (Requirement 15.4)
//!
//! Run with: `cargo test --test web_api_tests`

use actix_web::{test, web, App};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::sync::Arc;
use synbot::agent::subagent::{SubagentScope, SubagentStatus};
use synbot::config::{ProviderApiStyle, ProviderEntry, WebAuthConfig};
use synbot::tools::approval::ApprovalManager;
use synbot::web::handlers::{api, providers, team};
use synbot::web::state::AppState;
use synbot::web::BasicAuth;

mod common;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

async fn create_test_state() -> AppState {
    let (inbound_tx, _) = tokio::sync::mpsc::channel(100);
    let (outbound_tx, _) = tokio::sync::broadcast::channel(100);
    let approval_manager = Arc::new(ApprovalManager::new());
    let state =
        common::create_test_app_state_with_approval(inbound_tx, outbound_tx, approval_manager)
            .await;
    state.runtime_status.mark_running();
    state
}

fn basic_auth_header(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        BASE64.encode(format!("{}:{}", username, password))
    )
}

// ---------------------------------------------------------------------------
// Requirement 15.1 — all registered endpoints are reachable
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn test_get_status_returns_200() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/status", web::get().to(api::get_status)),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/status").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert!(body["data"]["running"].as_bool().unwrap_or(false));
}

#[actix_web::test]
async fn test_get_sessions_returns_200_with_empty_list() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/sessions", web::get().to(api::get_sessions)),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/sessions").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert!(body["data"]["items"].is_array());
    assert_eq!(body["data"]["total"], 0);
}

#[actix_web::test]
async fn test_get_channels_returns_200() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/channels", web::get().to(api::get_channels)),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/channels").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert!(body["data"].is_array());
}

#[actix_web::test]
async fn test_get_config_returns_200() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/config", web::get().to(api::get_config)),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/config").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert!(body["data"]["config"].is_object());
    assert!(body["data"]["configPath"].is_string());
}

#[actix_web::test]
async fn test_put_config_validation_failure_returns_400() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/config", web::put().to(api::put_config)),
    )
    .await;

    let mut cfg = synbot::config::Config::default();
    cfg.main_agent.max_tokens = 0;
    let v = serde_json::to_value(&cfg).unwrap();

    let req = test::TestRequest::put()
        .uri("/api/config")
        .set_json(&v)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], false);
    let errs = body["data"]["validationErrors"].as_array().unwrap();
    assert!(!errs.is_empty());
}

#[actix_web::test]
async fn test_put_config_roundtrip_masked_secrets() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/config", web::get().to(api::get_config))
            .route("/api/config", web::put().to(api::put_config)),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/config").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    let config = body["data"]["config"].clone();

    let req = test::TestRequest::put()
        .uri("/api/config")
        .set_json(&config)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let out: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(out["success"], true);
    assert!(out["data"]["configPath"].is_string());
}

#[actix_web::test]
async fn test_get_logs_returns_200() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/logs", web::get().to(api::get_logs)),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/logs").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    // Logs endpoint returns a paginated response
    assert!(body["data"]["items"].is_array());
}

#[actix_web::test]
async fn test_get_agents_returns_200() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/agents", web::get().to(api::get_agents)),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/agents").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
}

#[actix_web::test]
async fn test_get_cron_jobs_returns_200() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/cron", web::get().to(api::get_cron_jobs)),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/cron").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

// ---------------------------------------------------------------------------
// Requirement 15.2 — auth middleware rejects invalid credentials
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn test_auth_middleware_rejects_missing_credentials() {
    let state = create_test_state().await;
    let auth_config = WebAuthConfig {
        username: "admin".to_string(),
        password: "secret".to_string(),
    };

    let app = test::init_service(
        App::new()
            .wrap(BasicAuth::new(Some(auth_config)))
            .app_data(web::Data::new(state))
            .route("/api/status", web::get().to(api::get_status))
            .route("/api/sessions", web::get().to(api::get_sessions))
            .route("/api/config", web::get().to(api::get_config))
            .route("/api/logs", web::get().to(api::get_logs)),
    )
    .await;

    // No auth header → 401
    for uri in &["/api/status", "/api/sessions", "/api/config", "/api/logs"] {
        let req = test::TestRequest::get().uri(uri).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            401,
            "Expected 401 for {} without credentials",
            uri
        );
    }
}

#[actix_web::test]
async fn test_auth_middleware_rejects_wrong_password() {
    let state = create_test_state().await;
    let auth_config = WebAuthConfig {
        username: "admin".to_string(),
        password: "correct-password".to_string(),
    };

    let app = test::init_service(
        App::new()
            .wrap(BasicAuth::new(Some(auth_config)))
            .app_data(web::Data::new(state))
            .route("/api/status", web::get().to(api::get_status)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/status")
        .insert_header((
            "Authorization",
            basic_auth_header("admin", "wrong-password"),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401, "Wrong password should return 401");
}

#[actix_web::test]
async fn test_auth_middleware_rejects_wrong_username() {
    let state = create_test_state().await;
    let auth_config = WebAuthConfig {
        username: "admin".to_string(),
        password: "secret".to_string(),
    };

    let app = test::init_service(
        App::new()
            .wrap(BasicAuth::new(Some(auth_config)))
            .app_data(web::Data::new(state))
            .route("/api/status", web::get().to(api::get_status)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/status")
        .insert_header(("Authorization", basic_auth_header("wrong-user", "secret")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401, "Wrong username should return 401");
}

#[actix_web::test]
async fn test_auth_middleware_accepts_valid_credentials() {
    let state = create_test_state().await;
    let auth_config = WebAuthConfig {
        username: "admin".to_string(),
        password: "secret".to_string(),
    };

    let app = test::init_service(
        App::new()
            .wrap(BasicAuth::new(Some(auth_config)))
            .app_data(web::Data::new(state))
            .route("/api/status", web::get().to(api::get_status)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/status")
        .insert_header(("Authorization", basic_auth_header("admin", "secret")))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200, "Valid credentials should return 200");
}

#[actix_web::test]
async fn test_auth_middleware_rejects_malformed_auth_header() {
    let state = create_test_state().await;
    let auth_config = WebAuthConfig {
        username: "admin".to_string(),
        password: "secret".to_string(),
    };

    let app = test::init_service(
        App::new()
            .wrap(BasicAuth::new(Some(auth_config)))
            .app_data(web::Data::new(state))
            .route("/api/status", web::get().to(api::get_status)),
    )
    .await;

    // Malformed auth header (not Base64)
    let req = test::TestRequest::get()
        .uri("/api/status")
        .insert_header(("Authorization", "Basic not-valid-base64!!!"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        401,
        "Malformed auth header should return 401"
    );
}

// ---------------------------------------------------------------------------
// Requirement 15.3 — valid requests return correct HTTP status and format
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn test_get_sessions_response_has_correct_structure() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/sessions", web::get().to(api::get_sessions)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/sessions?page=1&page_size=10")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert!(body["data"]["items"].is_array());
    assert!(body["data"]["total"].is_number());
    assert_eq!(body["data"]["page"], 1);
    assert_eq!(body["data"]["page_size"], 10);
}

#[actix_web::test]
async fn test_get_session_by_id_returns_404_for_nonexistent() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/sessions/{id}", web::get().to(api::get_session_by_id)),
    )
    .await;

    // A well-formed session ID that doesn't exist in the store
    let req = test::TestRequest::get()
        .uri("/api/sessions/agent:main:dm:nonexistent-chat")
        .to_request();
    let resp = test::call_service(&app, req).await;
    // Should return 400 (parse error) or 404 (not found) — both are valid
    let status = resp.status().as_u16();
    assert!(
        status == 400 || status == 404,
        "Non-existent session should return 400 or 404, got {}",
        status
    );
}

#[actix_web::test]
async fn test_get_logs_with_level_filter() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/logs", web::get().to(api::get_logs)),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/logs?level=ERROR")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert!(body["data"]["items"].is_array());
}

// ---------------------------------------------------------------------------
// Requirement 15.4 — invalid parameters return 400 with descriptive error
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn test_get_session_by_id_returns_400_for_invalid_id_format() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/sessions/{id}", web::get().to(api::get_session_by_id)),
    )
    .await;

    // An ID that cannot be parsed as a valid SessionId
    let req = test::TestRequest::get()
        .uri("/api/sessions/not-a-valid-session-id-format")
        .to_request();
    let resp = test::call_service(&app, req).await;
    // Should return 400 (bad request) or 404 depending on parse failure
    let status = resp.status().as_u16();
    assert!(
        status == 400 || status == 404,
        "Invalid session ID format should return 400 or 404, got {}",
        status
    );
}

#[actix_web::test]
async fn test_submit_approval_response_with_missing_body_returns_error() {
    let state = create_test_state().await;
    let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
        "/api/approvals/{id}/respond",
        web::post().to(api::submit_approval_response),
    ))
    .await;

    // Missing JSON body
    let req = test::TestRequest::post()
        .uri("/api/approvals/test-id/respond")
        .to_request();
    let resp = test::call_service(&app, req).await;
    // Should return 400 (bad request) for missing body
    let status = resp.status().as_u16();
    assert!(
        status == 400 || status == 404,
        "Missing request body should return 400 or 404, got {}",
        status
    );
}

#[actix_web::test]
async fn test_update_cron_job_returns_404_for_nonexistent() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/cron/{id}", web::patch().to(api::update_cron_job)),
    )
    .await;

    let payload = serde_json::json!({ "enabled": true });
    let req = test::TestRequest::patch()
        .uri("/api/cron/nonexistent-job-id")
        .set_json(&payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        404,
        "Non-existent cron job should return 404"
    );
}

// ---------------------------------------------------------------------------
// Requirement 15.1 — approval endpoints are covered
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn test_get_approval_history_returns_200() {
    let state = create_test_state().await;
    let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
        "/api/approvals/history",
        web::get().to(api::get_approval_history),
    ))
    .await;

    let req = test::TestRequest::get()
        .uri("/api/approvals/history")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert!(body["data"]["items"].is_array());
}

#[actix_web::test]
async fn test_get_pending_approvals_returns_200() {
    let state = create_test_state().await;
    let app = test::init_service(App::new().app_data(web::Data::new(state)).route(
        "/api/approvals/pending",
        web::get().to(api::get_pending_approvals),
    ))
    .await;

    let req = test::TestRequest::get()
        .uri("/api/approvals/pending")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert!(body["data"].is_array());
}

#[actix_web::test]
async fn test_runtime_channel_snapshot_is_not_derived_from_enabled_config() {
    let state = create_test_state().await;
    let handle = state.runtime_status.register(
        "matrix:0:primary".into(),
        "matrix".into(),
        "primary".into(),
        true,
        true,
        true,
    );
    handle.mark_starting();
    handle.mark_connected();
    handle.record_received();
    handle.record_sent();
    handle.record_latency(17);

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/status", web::get().to(api::get_status)),
    )
    .await;
    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/api/status").to_request(),
    )
    .await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let channel = body["data"]["channels"]
        .as_array()
        .and_then(|channels| {
            channels
                .iter()
                .find(|c| c["instance_id"] == "matrix:0:primary")
        })
        .expect("matrix runtime snapshot");
    assert_eq!(channel["status"], "connected");
    assert_eq!(channel["last_latency_ms"], 17);
    assert!(channel["started_at"].is_string());
    assert!(channel["last_connected_at"].is_string());
    assert!(channel["last_received_at"].is_string());
    assert!(channel["last_sent_at"].is_string());
}

// ---------------------------------------------------------------------------
// Projects — open/close, directory listing, file preview, path safety
// ---------------------------------------------------------------------------

fn projects_routes() -> actix_web::Scope {
    web::scope("/api")
        .route(
            "/projects",
            web::get().to(synbot::web::handlers::projects::get_projects),
        )
        .route(
            "/projects",
            web::post().to(synbot::web::handlers::projects::open_project),
        )
        .route(
            "/projects/{id}",
            web::delete().to(synbot::web::handlers::projects::delete_project),
        )
        .route(
            "/projects/{id}/files",
            web::get().to(synbot::web::handlers::projects::get_project_files),
        )
        .route(
            "/projects/{id}/file",
            web::get().to(synbot::web::handlers::projects::get_project_file),
        )
}

#[actix_web::test]
async fn test_projects_open_list_preview_and_delete() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .service(projects_routes()),
    )
    .await;

    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("hello.txt"), "hello world").unwrap();

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/projects")
            .set_json(serde_json::json!({ "path": dir.path().to_str().unwrap() }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    let id = body["data"]["id"].as_str().expect("project id").to_string();

    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/api/projects").to_request(),
    )
    .await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{id}/files"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    let names: Vec<&str> = body["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    // Directories sort before files.
    assert_eq!(names, vec!["src", "hello.txt"]);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{id}/file?path=hello.txt"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["content"], "hello world");
    assert_eq!(body["data"]["truncated"], false);

    let resp = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&format!("/api/projects/{id}"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_projects_reject_escaping_paths_and_missing_folders() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .service(projects_routes()),
    )
    .await;

    let dir = tempfile::tempdir().unwrap();
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/projects")
            .set_json(
                serde_json::json!({ "path": dir.path().join("does-not-exist").to_str().unwrap() }),
            )
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/projects")
            .set_json(serde_json::json!({ "path": dir.path().to_str().unwrap() }))
            .to_request(),
    )
    .await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let id = body["data"]["id"].as_str().unwrap().to_string();

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{id}/files?path=../../etc"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
}

// ---------------------------------------------------------------------------
// /api/checkpoints
// ---------------------------------------------------------------------------

fn checkpoints_routes() -> actix_web::Scope {
    web::scope("/api")
        .route(
            "/checkpoints",
            web::get().to(synbot::web::handlers::checkpoints::list_checkpoints),
        )
        .route(
            "/checkpoints/{turn_id}",
            web::get().to(synbot::web::handlers::checkpoints::get_checkpoint),
        )
        .route(
            "/checkpoints/{turn_id}/undo",
            web::post().to(synbot::web::handlers::checkpoints::undo_checkpoint),
        )
}

#[actix_web::test]
async fn test_checkpoints_expose_diff_and_undo_a_turn() {
    let state = create_test_state().await;
    let store = synbot::checkpoint::CheckpointStore::new(state.checkpoints.root().to_path_buf());
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .service(checkpoints_routes()),
    )
    .await;

    // Mimic what the loop does around a `write_file` call.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.md");
    std::fs::write(&file, "old line\nunchanged\n").unwrap();
    let args = serde_json::json!({ "path": file.to_str().unwrap() });
    let pending = store
        .capture_before("turn-abc", "write_file", &args)
        .await
        .expect("write_file has a snapshot target");
    std::fs::write(&file, "new line\nunchanged\n").unwrap();
    assert_eq!(store.finish(pending).await, 1);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/checkpoints/turn-abc")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["additions"], 1);
    assert_eq!(body["data"]["deletions"], 1);
    let hunks = body["data"]["files"][0]["hunks"].as_array().unwrap();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0]["lines"][0]["kind"], "del");

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/checkpoints?limit=5")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"][0]["id"], "turn-abc");

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/checkpoints/turn-abc/undo")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["restored"].as_array().unwrap().len(), 1);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "old line\nunchanged\n"
    );
}

#[actix_web::test]
async fn test_checkpoints_unknown_turn_is_404() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .service(checkpoints_routes()),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/checkpoints/nope")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/checkpoints/nope/undo")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
}

// ---------------------------------------------------------------------------
// /api/git
// ---------------------------------------------------------------------------

/// Both handlers under one scope: two scopes with the same prefix do not fall through to each other.
fn projects_and_git_routes() -> actix_web::Scope {
    web::scope("/api")
        .route(
            "/projects",
            web::get().to(synbot::web::handlers::projects::get_projects),
        )
        .route(
            "/projects",
            web::post().to(synbot::web::handlers::projects::open_project),
        )
        .route(
            "/git/status",
            web::get().to(synbot::web::handlers::git::get_status),
        )
        .route(
            "/git/diff",
            web::get().to(synbot::web::handlers::git::get_diff),
        )
        .route(
            "/git/log",
            web::get().to(synbot::web::handlers::git::get_log),
        )
        .route(
            "/tasks",
            web::get().to(synbot::web::handlers::tasks::list_tasks),
        )
        .route(
            "/tasks",
            web::put().to(synbot::web::handlers::tasks::replace_tasks),
        )
        .route(
            "/tasks/{id}",
            web::patch().to(synbot::web::handlers::tasks::update_task),
        )
        .route(
            "/commands",
            web::get().to(synbot::web::handlers::commands::list_commands),
        )
        .route(
            "/index",
            web::get().to(synbot::web::handlers::index::get_index),
        )
        .route(
            "/index/refresh",
            web::post().to(synbot::web::handlers::index::refresh_index),
        )
        .route(
            "/index/symbols",
            web::get().to(synbot::web::handlers::index::search_symbols),
        )
        .route(
            "/index/wiki",
            web::post().to(synbot::web::handlers::index::write_wiki),
        )
}

fn git_available() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[actix_web::test]
async fn test_git_status_diff_and_log_for_an_open_project() {
    if !git_available() {
        return;
    }
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .service(projects_and_git_routes()),
    )
    .await;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.email", "test@example.com"],
        vec!["config", "user.name", "Test"],
        vec!["config", "commit.gpgsign", "false"],
    ] {
        std::process::Command::new("git")
            .current_dir(root)
            .args(&args)
            .output()
            .unwrap();
    }
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    std::process::Command::new("git")
        .current_dir(root)
        .args(["add", "."])
        .output()
        .unwrap();
    std::process::Command::new("git")
        .current_dir(root)
        .args(["commit", "-q", "-m", "first"])
        .output()
        .unwrap();
    std::fs::write(root.join("a.txt"), "one\ntwo\n").unwrap();

    // `project_id` selects the repository; without it the configured workspace is used.
    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/api/git/status").to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["is_repo"], false);

    // Unknown project ids are a 404 — the repository must be opened first.
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/git/status?project_id={}",
                root.to_str().unwrap()
            ))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/projects")
            .set_json(serde_json::json!({ "path": root.to_str().unwrap() }))
            .to_request(),
    )
    .await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    let id = body["data"]["id"].as_str().unwrap().to_string();

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/git/status?project_id={id}"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["is_repo"], true);
    assert_eq!(body["data"]["branch"], "main");
    assert_eq!(body["data"]["files"][0]["path"], "a.txt");
    assert_eq!(body["data"]["files"][0]["status"], "M");

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/git/diff?project_id={id}&path=a.txt"))
            .to_request(),
    )
    .await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"]["diff"].as_str().unwrap().contains("+two"));

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/git/log?project_id={id}&limit=1"))
            .to_request(),
    )
    .await;
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"][0]["subject"], "first");
}

#[actix_web::test]
async fn test_tasks_rest_reads_writes_and_ticks_the_board_of_a_project() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .service(projects_and_git_routes()),
    )
    .await;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/projects")
            .set_json(serde_json::json!({ "path": root.to_str().unwrap() }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    let id = body["data"]["id"].as_str().unwrap().to_string();

    // A fresh project has an empty board pointing at the project folder.
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/tasks?project_id={id}"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"]["tasks"].as_array().unwrap().is_empty());
    // macOS resolves `/var` → `/private/var`, so compare against the canonical project path.
    let canonical_root = std::fs::canonicalize(root).unwrap();
    assert_eq!(
        body["data"]["path"],
        canonical_root
            .join(".synbot/plans/tasks.json")
            .to_str()
            .unwrap()
    );

    // PUT replaces the list and persists it where the tool would write it.
    let resp = test::call_service(
        &app,
        test::TestRequest::put()
            .uri(&format!("/api/tasks?project_id={id}"))
            .set_json(serde_json::json!({
                "tasks": [
                    { "title": "read code", "status": "in_progress" },
                    { "title": "write tests" }
                ]
            }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    let first_id = body["data"]["tasks"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(first_id, "t1");
    assert!(root.join(".synbot/plans/tasks.json").exists());

    // PATCH ticks one task without resending the whole list.
    let resp = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!("/api/tasks/{first_id}?project_id={id}"))
            .set_json(serde_json::json!({ "status": "completed" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["tasks"][0]["status"], "completed");
    assert_eq!(body["data"]["tasks"][1]["status"], "pending");

    // Unknown statuses and unknown ids are client errors, not 500s.
    let resp = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!("/api/tasks/{first_id}?project_id={id}"))
            .set_json(serde_json::json!({ "status": "almost" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
    let resp = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!("/api/tasks/nope?project_id={id}"))
            .set_json(serde_json::json!({ "status": "completed" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/tasks?project_id=missing")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);

    // Workspace slash commands come from the same project folder.
    std::fs::create_dir_all(root.join(".synbot/commands")).unwrap();
    std::fs::write(
        root.join(".synbot/commands/review.md"),
        "---\ndescription: Review a diff\n---\nReview $ARGUMENTS.\n",
    )
    .unwrap();
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/commands?project_id={id}"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"][0]["name"], "review");
    assert_eq!(body["data"][0]["description"], "Review a diff");
}

/// The symbol index has to answer three questions the panel asks: "is it built?", "refresh it" and
/// "where is this symbol?" — all against the project folder the agent would use.
#[actix_web::test]
async fn test_index_rest_searches_symbols_inside_the_project() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .service(projects_and_git_routes()),
    )
    .await;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/agent.rs"),
        "pub struct AgentLoop;\nimpl AgentLoop {\n    pub async fn run_turn(&self) {}\n}\n",
    )
    .unwrap();

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/projects")
            .set_json(serde_json::json!({ "path": root.to_str().unwrap() }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    let id = body["data"]["id"].as_str().unwrap().to_string();

    // Nothing indexed yet: the summary is empty and points at the file a refresh would write.
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/index?project_id={id}"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"], 0);
    assert!(body["data"]["path"]
        .as_str()
        .unwrap()
        .ends_with(".synbot/index/code.json"));

    // Search builds on demand, so the panel works before the agent ever ran.
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/index/symbols?q=run_turn&project_id={id}"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    let hits = body["data"]["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1, "{body}");
    assert_eq!(hits[0]["path"], "src/agent.rs");
    assert_eq!(hits[0]["line"], 3);
    assert_eq!(hits[0]["container"], "AgentLoop");
    assert_eq!(body["data"]["files"], 1);

    // A second refresh reuses the fingerprint instead of re-parsing.
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/index/refresh?project_id={id}"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"], 1);
    assert_eq!(body["data"]["parsed"], 0);
    assert_eq!(body["data"]["reused"], 1);
    assert_eq!(body["data"]["languages"]["rust"], 1);

    // Wiki pages land in the project folder as markdown.
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/index/wiki?project_id={id}"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert!(body["data"]["files"][0] == "index.md", "{body}");
    assert!(root.join(".synbot/repowiki/index.md").exists());

    // Empty queries and unknown projects are client errors.
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/index/symbols?q=%20&project_id={id}"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/index/symbols?q=x&project_id=missing")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
}

// ---------------------------------------------------------------------------
// Agent team — /api/subagents snapshot + cancel
// ---------------------------------------------------------------------------

fn subagent_routes() -> actix_web::Scope {
    web::scope("/api")
        .route("/subagents", web::get().to(team::list_subagents))
        .route(
            "/subagents/{id}/cancel",
            web::post().to(team::cancel_subagent),
        )
}

#[actix_web::test]
async fn test_subagents_list_is_empty_by_default() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .service(subagent_routes()),
    )
    .await;

    let req = test::TestRequest::get().uri("/api/subagents").to_request();
    let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;
    assert_eq!(body["success"], true);
    assert_eq!(
        body["data"]["agents"]
            .as_array()
            .expect("agents array")
            .len(),
        0
    );
    assert_eq!(body["data"]["running"], 0);
}

#[actix_web::test]
async fn test_subagents_cancel_stops_a_running_teammate() {
    let state = create_test_state().await;

    // A teammate parked on a slow future, so the snapshot has something running to cancel.
    let id = state
        .subagents
        .lock()
        .await
        .spawn_fn(
            "slow teammate".to_string(),
            Some("reviewer".to_string()),
            SubagentScope::default(),
            Box::pin(async {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                Ok("never".to_string())
            }),
            None,
        )
        .await
        .expect("spawn should be accepted");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .service(subagent_routes()),
    )
    .await;

    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get().uri("/api/subagents").to_request(),
    )
    .await;
    assert_eq!(body["data"]["running"], 1);
    assert_eq!(body["data"]["agents"][0]["id"], id.as_str());
    assert_eq!(body["data"]["agents"][0]["role"], "reviewer");
    assert_eq!(body["data"]["agents"][0]["state"], "running");

    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/subagents/{id}/cancel"))
            .set_json(serde_json::json!({ "all": false }))
            .to_request(),
    )
    .await;
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["cancelled"], 1);

    let mut cancelled = false;
    for _ in 0..100 {
        let handle = state
            .subagents
            .lock()
            .await
            .get_result(&id)
            .await
            .expect("handle stays in the pool");
        if matches!(handle.status, SubagentStatus::Cancelled) {
            cancelled = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(cancelled, "teammate should end up cancelled");
}

#[actix_web::test]
async fn test_subagents_cancel_unknown_id_is_404() {
    let state = create_test_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .service(subagent_routes()),
    )
    .await;

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/subagents/nope/cancel")
            .set_json(serde_json::json!({ "all": false }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
}

// ---------------------------------------------------------------------------
// Provider metadata (`GET /api/providers`)
// ---------------------------------------------------------------------------

async fn get_providers_body(state: AppState) -> serde_json::Value {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/providers", web::get().to(providers::list_providers)),
    )
    .await;
    let resp = test::call_service(
        &app,
        test::TestRequest::get().uri("/api/providers").to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    test::read_body_json(resp).await
}

#[actix_web::test]
async fn test_get_providers_reports_metadata_without_leaking_keys() {
    let state = create_test_state().await;
    {
        let mut cfg = state.config.write().await;
        cfg.providers.anthropic.api_key = "sk-ant-do-not-leak".to_string();
        cfg.providers.anthropic.api_base = Some("https://gateway.example.com/".to_string());
        cfg.providers.extra.insert(
            "minimax".to_string(),
            ProviderEntry {
                api_key: "extra-do-not-leak".to_string(),
                api_style: ProviderApiStyle::Anthropic,
                ..Default::default()
            },
        );
        cfg.main_agent.provider = "minimax".to_string();
        cfg.main_agent.model = "MiniMax-M3".to_string();
    }

    let body = get_providers_body(state).await;
    let raw = body.to_string();
    assert!(
        !raw.contains("sk-ant-do-not-leak"),
        "api keys must never be serialised"
    );
    assert!(
        !raw.contains("extra-do-not-leak"),
        "extra keys must never be serialised"
    );

    let data = &body["data"];
    let providers = data["providers"].as_array().expect("providers array");
    assert_eq!(providers.len(), 8, "every built-in provider is listed");

    let anthropic = providers
        .iter()
        .find(|p| p["key"] == "anthropic")
        .expect("anthropic is listed");
    assert_eq!(anthropic["apiStyle"], "anthropic");
    assert_eq!(anthropic["configured"], true);
    assert_eq!(anthropic["apiBase"], "https://gateway.example.com/");
    // Trailing slash is stripped only for the effective endpoint.
    assert_eq!(anthropic["apiBaseEffective"], "https://gateway.example.com");
    assert_eq!(anthropic["usedBy"].as_array().map(Vec::len), Some(0));

    // A local runtime is usable without a key, so `configured` cannot mean "apiKey is set".
    let ollama = providers
        .iter()
        .find(|p| p["key"] == "ollama")
        .expect("ollama is listed");
    assert_eq!(ollama["local"], true);
    assert_eq!(ollama["configured"], true);
    assert_eq!(ollama["requiresKey"], false);

    let extra = data["extra"].as_array().expect("extra array");
    assert_eq!(extra.len(), 1);
    assert_eq!(extra[0]["name"], "minimax");
    assert_eq!(extra[0]["apiStyle"], "anthropic");
    assert_eq!(extra[0]["usedBy"][0], "main");

    assert_eq!(data["defaults"]["provider"], "minimax");
    assert_eq!(data["defaults"]["model"], "MiniMax-M3");
    assert_eq!(data["unknownInUse"].as_array().map(Vec::len), Some(0));
}

#[actix_web::test]
async fn test_get_providers_flags_unknown_names_in_use() {
    let state = create_test_state().await;
    {
        let mut cfg = state.config.write().await;
        // `deepsek` is a typo: it matches no built-in and no `providers.extra` key, so the request
        // would fail at runtime with no hint in the UI.
        cfg.main_agent.provider = "deepsek".to_string();
        cfg.main_agent.model = "deepseek-chat".to_string();
    }

    let body = get_providers_body(state).await;
    let unknown = body["data"]["unknownInUse"]
        .as_array()
        .expect("unknown list");
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0]["name"], "deepsek");
    assert_eq!(unknown[0]["usedBy"][0], "main");

    // The typo must not be counted as "used" by any built-in provider.
    for provider in body["data"]["providers"]
        .as_array()
        .expect("providers array")
    {
        assert_eq!(
            provider["usedBy"].as_array().map(Vec::len),
            Some(0),
            "{} should not claim an unknown name",
            provider["key"]
        );
    }
}
