//! Integration tests for authentication at the production WebSocket routes.

use actix_web::{test, App};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use std::sync::Arc;
use synbot::config::WebAuthConfig;
use synbot::tools::approval::ApprovalManager;
use synbot::web::server::configure_routes;
use synbot::web::BasicAuth;

mod common;

async fn test_state_and_auth() -> (synbot::web::state::AppState, BasicAuth) {
    let (inbound_tx, _) = tokio::sync::mpsc::channel(8);
    let (outbound_tx, _) = tokio::sync::broadcast::channel(8);
    let state = common::create_test_app_state_with_approval(
        inbound_tx,
        outbound_tx,
        Arc::new(ApprovalManager::new()),
    )
    .await;
    let auth = BasicAuth::new(Some(WebAuthConfig {
        username: "admin".to_string(),
        password: "secret".to_string(),
    }));
    (state, auth)
}

fn websocket_request(uri: &str) -> test::TestRequest {
    test::TestRequest::get()
        .uri(uri)
        .insert_header(("Connection", "Upgrade"))
        .insert_header(("Upgrade", "websocket"))
        .insert_header(("Sec-WebSocket-Version", "13"))
        .insert_header(("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ=="))
}

fn auth_header(username: &str, password: &str) -> String {
    format!("Basic {}", BASE64.encode(format!("{username}:{password}")))
}

#[actix_web::test]
async fn websocket_routes_reject_missing_authentication() {
    let (state, auth) = test_state_and_auth().await;
    let app = test::init_service(
        App::new().configure(move |cfg| configure_routes(cfg, state.clone(), auth.clone())),
    )
    .await;

    for uri in ["/ws/chat", "/ws/logs"] {
        let response = test::call_service(&app, websocket_request(uri).to_request()).await;

        assert_eq!(
            response.status(),
            401,
            "unauthenticated {uri} must be rejected"
        );
        assert!(response.headers().contains_key("WWW-Authenticate"));
    }
}

#[actix_web::test]
async fn websocket_routes_reject_invalid_authentication() {
    let (state, auth) = test_state_and_auth().await;
    let app = test::init_service(
        App::new().configure(move |cfg| configure_routes(cfg, state.clone(), auth.clone())),
    )
    .await;

    for uri in ["/ws/chat", "/ws/logs"] {
        let request = websocket_request(uri)
            .insert_header(("Authorization", auth_header("admin", "wrong-password")))
            .to_request();
        let response = test::call_service(&app, request).await;

        assert_eq!(
            response.status(),
            401,
            "invalid credentials for {uri} must be rejected"
        );
    }
}

#[actix_web::test]
async fn websocket_routes_accept_authenticated_upgrade() {
    let (state, auth) = test_state_and_auth().await;
    let app = test::init_service(
        App::new().configure(move |cfg| configure_routes(cfg, state.clone(), auth.clone())),
    )
    .await;

    for uri in ["/ws/chat", "/ws/logs"] {
        let request = websocket_request(uri)
            .insert_header(("Authorization", auth_header("admin", "secret")))
            .to_request();
        let response = test::call_service(&app, request).await;

        assert_eq!(response.status(), 101, "authenticated {uri} must upgrade");
    }
}

#[actix_web::test]
async fn websocket_accepts_session_cookie_created_by_rest_authentication() {
    let (state, auth) = test_state_and_auth().await;
    let app = test::init_service(
        App::new().configure(move |cfg| configure_routes(cfg, state.clone(), auth.clone())),
    )
    .await;

    let login = test::TestRequest::get()
        .uri("/api/status")
        .insert_header(("Authorization", auth_header("admin", "secret")))
        .to_request();
    let login_response = test::call_service(&app, login).await;
    assert_eq!(login_response.status(), 200);

    let set_cookie = login_response
        .headers()
        .get("set-cookie")
        .expect("REST authentication must create a WebSocket session cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let request = websocket_request("/ws/logs")
        .insert_header(("Cookie", set_cookie))
        .to_request();
    let response = test::call_service(&app, request).await;

    assert_eq!(
        response.status(),
        101,
        "REST-authenticated browser session must upgrade"
    );
}
