//! `/api/plugins` + `/api/plugin-keys` end-to-end tests.
//!
//! The handlers resolve their storage through `config::config_dir()`, which is a **process global**
//! (normally `~/.synbot`). The whole scenario therefore lives in one sequential test that first
//! points the root override at a temp dir, so nothing touches the developer's real plugin store.
//!
//! Run with: `cargo test --test plugin_api_tests`

use std::sync::Arc;

use actix_web::{test, web, App};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::SigningKey;
use synbot::tools::approval::ApprovalManager;
use synbot::web::handlers::plugins;
use synbot::web::state::AppState;

mod common;

const MANIFEST: &str = r#"{
  "id": "demo",
  "name": "Demo plugin",
  "version": "1.0.0",
  "description": "does demo things",
  "entry": "demo.wasm",
  "permissions": {
    "filesystem": { "read": true, "write": false, "paths": ["docs"] },
    "network": { "hosts": ["api.example.com"] }
  }
}"#;

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

/// Write a package directory that `install_from_dir` accepts.
fn write_package(dir: &std::path::Path, wasm: &[u8]) {
    std::fs::write(dir.join("plugin.json"), MANIFEST).unwrap();
    std::fs::write(dir.join("demo.wasm"), wasm).unwrap();
}

#[actix_web::test]
async fn plugin_package_lifecycle_over_rest() {
    let root = tempfile::tempdir().expect("plugin root");
    synbot::config::set_root_dir(Some(root.path().to_path_buf()));

    let source = tempfile::tempdir().expect("package source");
    let wasm = b"\0asm\x01\0\0\0installed";
    write_package(source.path(), wasm);

    let state = create_state().await;
    let config_handle = state.config.clone();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .route("/api/plugins", web::get().to(plugins::list_plugins))
            .route(
                "/api/plugins/install",
                web::post().to(plugins::install_plugin),
            )
            .route(
                "/api/plugins/{id}",
                web::delete().to(plugins::uninstall_plugin),
            )
            .route(
                "/api/plugins/{id}/enabled",
                web::post().to(plugins::set_enabled),
            )
            .route("/api/plugin-keys", web::get().to(plugins::list_keys))
            .route("/api/plugin-keys", web::post().to(plugins::add_key))
            .route(
                "/api/plugin-keys/{name}",
                web::delete().to(plugins::remove_key),
            ),
    )
    .await;

    // Nothing installed yet.
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get().uri("/api/plugins").to_request(),
    )
    .await;
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["plugins"].as_array().unwrap().len(), 0);
    assert!(body["data"]["root"]
        .as_str()
        .unwrap()
        .starts_with(&root.path().display().to_string()));

    // A directory without `plugin.json` is rejected before anything is copied in.
    let empty = tempfile::tempdir().expect("empty source");
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/plugins/install")
            .set_json(serde_json::json!({ "path": empty.path().display().to_string() }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);

    // Install the package: the digest is recorded and nobody signed it.
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/plugins/install")
            .set_json(serde_json::json!({ "path": source.path().display().to_string() }))
            .to_request(),
    )
    .await;
    assert_eq!(body["success"], true, "{body}");
    assert_eq!(body["data"]["id"], "demo");
    assert_eq!(body["data"]["trust"], "unsigned");
    assert_eq!(body["data"]["enabled"], true);
    assert_eq!(body["data"]["loadable"], true);
    assert_eq!(body["data"]["wasm_bytes"], wasm.len());
    let summary = body["data"]["permission_summary"].as_str().unwrap();
    assert!(summary.contains("fs:r"), "{summary}");
    assert!(summary.contains("net:api.example.com"), "{summary}");
    // Installing enables the plugin by writing the config entry the loader iterates.
    assert!(config_handle.read().await.plugins.contains_key("demo"));

    // Listed back, and the file landed under <root>/plugins/demo.
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get().uri("/api/plugins").to_request(),
    )
    .await;
    let listed = body["data"]["plugins"].as_array().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["id"], "demo");
    assert!(root.path().join("plugins/demo/plugin.json").exists());

    // Disabling keeps the package on disk but flips the loader flag.
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/plugins/demo/enabled")
            .set_json(serde_json::json!({ "enabled": false }))
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["enabled"], false);
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get().uri("/api/plugins").to_request(),
    )
    .await;
    assert_eq!(body["data"]["plugins"][0]["enabled"], false);

    // Trusting a publisher key is stored in the trust store.
    let public = SigningKey::from_bytes(&[7u8; 32])
        .verifying_key()
        .to_bytes()
        .to_vec();
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/plugin-keys")
            .set_json(serde_json::json!({
                "name": "publisher",
                "public_key": BASE64.encode(&public),
            }))
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["name"], "publisher");
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/plugin-keys")
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["keys"].as_array().unwrap().len(), 1);

    // Rewriting the wasm after installation is detected and stops the plugin from loading.
    std::fs::write(root.path().join("plugins/demo/demo.wasm"), b"tampered").unwrap();
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get().uri("/api/plugins").to_request(),
    )
    .await;
    assert_eq!(body["data"]["plugins"][0]["trust"], "tampered");
    assert_eq!(body["data"]["plugins"][0]["loadable"], false);

    // Forgetting the key leaves the store empty.
    let resp = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri("/api/plugin-keys/publisher")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri("/api/plugin-keys")
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["keys"].as_array().unwrap().len(), 0);

    // Unknown ids are 404 rather than silently succeeding.
    let resp = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri("/api/plugin-keys/missing")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);

    // Uninstall removes both the directory and the config entry.
    let resp = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri("/api/plugins/demo")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert!(!root.path().join("plugins/demo").exists());
    assert!(!config_handle.read().await.plugins.contains_key("demo"));
    let resp = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri("/api/plugins/demo")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);

    synbot::config::set_root_dir(None);
}
