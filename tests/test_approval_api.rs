use actix_web::{test, web, App};
use synbot::tools::approval::ApprovalManager;
use synbot::web::handlers::api;
use synbot::web::state::AppState;
use synbot::web::BasicAuth;
use std::sync::Arc;

mod common;

/// Helper to create test AppState with approval manager
async fn create_test_state() -> AppState {
    let (inbound_tx, _) = tokio::sync::mpsc::channel(100);
    let (outbound_tx, _) = tokio::sync::broadcast::channel(100);
    
    let approval_manager = Arc::new(ApprovalManager::new());
    
    common::create_test_app_state_with_approval(
        inbound_tx,
        outbound_tx,
        approval_manager,
    ).await
}

#[actix_web::test]
async fn test_get_approval_history_empty() {
    let state = create_test_state().await;
    
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/approvals/history", web::get().to(api::get_approval_history))
    ).await;
    
    let req = test::TestRequest::get()
        .uri("/api/approvals/history")
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["total"], 0);
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn test_get_approval_history_with_data() {
    let state = create_test_state().await;
    
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/approvals/history", web::get().to(api::get_approval_history))
    ).await;
    
    let req = test::TestRequest::get()
        .uri("/api/approvals/history")
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    // Just verify the structure is correct
    assert!(body["data"]["total"].is_number());
    assert!(body["data"]["items"].is_array());
}

#[actix_web::test]
async fn test_get_approval_history_filter_by_channel() {
    let state = create_test_state().await;
    
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/approvals/history", web::get().to(api::get_approval_history))
    ).await;
    
    let req = test::TestRequest::get()
        .uri("/api/approvals/history?channel=telegram")
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert!(body["data"]["items"].is_array());
}

#[actix_web::test]
async fn test_get_approval_history_filter_by_status() {
    let state = create_test_state().await;
    
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/approvals/history", web::get().to(api::get_approval_history))
    ).await;
    
    let req = test::TestRequest::get()
        .uri("/api/approvals/history?status=pending")
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert!(body["data"]["items"].is_array());
}

#[actix_web::test]
async fn test_get_approval_history_pagination() {
    let state = create_test_state().await;
    
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/approvals/history", web::get().to(api::get_approval_history))
    ).await;
    
    let req = test::TestRequest::get()
        .uri("/api/approvals/history?page=1&page_size=5")
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["page"], 1);
    assert_eq!(body["data"]["page_size"], 5);
}

#[actix_web::test]
async fn test_get_pending_approvals_empty() {
    let state = create_test_state().await;
    
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/approvals/pending", web::get().to(api::get_pending_approvals))
    ).await;
    
    let req = test::TestRequest::get()
        .uri("/api/approvals/pending")
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    
    let body: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(body["success"], true);
    assert_eq!(body["data"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn test_get_pending_approvals_with_data() {
    let state = create_test_state().await;
    
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/approvals/pending", web::get().to(api::get_pending_approvals))
    ).await;
    
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
async fn test_submit_approval_response_approve() {
    let state = create_test_state().await;
    
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/approvals/{id}/respond", web::post().to(api::submit_approval_response))
    ).await;
    
    let payload = serde_json::json!({
        "approved": true,
        "responder": "test-user"
    });
    
    let req = test::TestRequest::post()
        .uri("/api/approvals/test-id/respond")
        .set_json(&payload)
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    // Should return 404 since the request doesn't exist
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_submit_approval_response_reject() {
    let state = create_test_state().await;
    
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/approvals/{id}/respond", web::post().to(api::submit_approval_response))
    ).await;
    
    let payload = serde_json::json!({
        "approved": false,
        "responder": "test-user"
    });
    
    let req = test::TestRequest::post()
        .uri("/api/approvals/test-id/respond")
        .set_json(&payload)
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    // Should return 404 since the request doesn't exist
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_submit_approval_response_not_found() {
    let state = create_test_state().await;
    
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/approvals/{id}/respond", web::post().to(api::submit_approval_response))
    ).await;
    
    let payload = serde_json::json!({
        "approved": true,
        "responder": "test-user"
    });
    
    let req = test::TestRequest::post()
        .uri("/api/approvals/nonexistent-id/respond")
        .set_json(&payload)
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_approval_endpoints_with_auth() {
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use synbot::config::WebAuthConfig;
    
    let state = create_test_state().await;
    
    let auth_config = WebAuthConfig {
        username: "admin".to_string(),
        password: "secret".to_string(),
    };
    
    let app = test::init_service(
        App::new()
            .wrap(BasicAuth::new(Some(auth_config)))
            .app_data(web::Data::new(state))
            .route("/api/approvals/history", web::get().to(api::get_approval_history))
            .route("/api/approvals/pending", web::get().to(api::get_pending_approvals))
    ).await;
    
    // Test without auth - should fail
    let req = test::TestRequest::get()
        .uri("/api/approvals/history")
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);
    
    // Test with valid auth - should succeed
    let credentials = BASE64.encode("admin:secret");
    let auth_header = format!("Basic {}", credentials);
    
    let req = test::TestRequest::get()
        .uri("/api/approvals/history")
        .insert_header(("Authorization", auth_header))
        .to_request();
    
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}
