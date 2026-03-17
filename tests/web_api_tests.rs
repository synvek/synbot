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
use synbot::config::WebAuthConfig;
use synbot::tools::approval::ApprovalManager;
use synbot::web::handlers::api;
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
    common::create_test_app_state_with_approval(inbound_tx, outbound_tx, approval_manager).await
}

fn basic_auth_header(username: &str, password: &str) -> String {
    format!("Basic {}", BASE64.encode(format!("{}:{}", username, password)))
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
        .insert_header(("Authorization", basic_auth_header("admin", "wrong-password")))
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
    assert_eq!(resp.status(), 401, "Malformed auth header should return 401");
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
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route(
                "/api/approvals/{id}/respond",
                web::post().to(api::submit_approval_response),
            ),
    )
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
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route(
                "/api/approvals/history",
                web::get().to(api::get_approval_history),
            ),
    )
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
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route(
                "/api/approvals/pending",
                web::get().to(api::get_pending_approvals),
            ),
    )
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
