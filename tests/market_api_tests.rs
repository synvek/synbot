//! `/api/market` end-to-end tests.
//!
//! The handlers resolve the plugin root through `config::config_dir()`, a **process global**, so the
//! whole scenario lives in one sequential test after pointing the override at a temp dir. The market
//! itself is a local directory here (a `file://`-style source), which is the same code path an HTTP
//! index takes — only the transport differs.
//!
//! Run with: `cargo test --test market_api_tests`

use std::path::{Path, PathBuf};
use std::sync::Arc;

use actix_web::{test, web, App};
use sha2::{Digest, Sha256};
use synbot::tools::approval::ApprovalManager;
use synbot::web::handlers::market;
use synbot::web::state::AppState;

mod common;

fn digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

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

/// Write `<market>/<id>-<version>/plugin.json` + wasm, returning the manifest path and digest.
fn write_package(market: &Path, id: &str, version: &str, wasm: &[u8]) -> (PathBuf, String) {
    let dir = market.join(format!("packages/{id}-{version}"));
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = serde_json::json!({
        "id": id,
        "name": format!("{id} plugin"),
        "version": version,
        "description": "published for tests",
        "entry": format!("{id}.wasm"),
    });
    std::fs::write(
        dir.join("plugin.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join(format!("{id}.wasm")), wasm).unwrap();
    (dir.join("plugin.json"), digest(wasm))
}

fn entry_value(
    id: &str,
    version: &str,
    manifest: &Path,
    sha: &str,
    deps: &[(&str, &str)],
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": format!("{id} plugin"),
        "version": version,
        "description": "published for tests",
        "manifest_url": manifest.display().to_string(),
        "sha256": sha,
        "dependencies": deps
            .iter()
            .map(|(id, version)| serde_json::json!({ "id": id, "version": version }))
            .collect::<Vec<_>>(),
    })
}

fn write_index(market: &Path, entries: Vec<serde_json::Value>) -> String {
    let path = market.join("index.json");
    let index = serde_json::json!({ "name": "test market", "plugins": entries });
    std::fs::write(&path, serde_json::to_string_pretty(&index).unwrap()).unwrap();
    path.display().to_string()
}

#[actix_web::test]
async fn market_lists_installs_and_updates_over_rest() {
    let config_root = tempfile::tempdir().expect("config root");
    synbot::config::set_root_dir(Some(config_root.path().to_path_buf()));
    let market_dir = tempfile::tempdir().expect("market dir");

    let (base_manifest, base_sha) = write_package(market_dir.path(), "base", "1.0.0", b"\0asm\x01");
    let (app_manifest, app_sha) = write_package(market_dir.path(), "app", "1.0.0", b"\0asm\x02");
    let index = write_index(
        market_dir.path(),
        vec![
            entry_value(
                "app",
                "1.0.0",
                &app_manifest,
                &app_sha,
                &[("base", ">=1.0.0")],
            ),
            entry_value("base", "1.0.0", &base_manifest, &base_sha, &[]),
        ],
    );

    let state = create_state().await;
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/api/market", web::get().to(market::list_market))
            .route(
                "/api/market/install",
                web::post().to(market::install_plugin),
            )
            .route("/api/market/update-all", web::post().to(market::update_all)),
    )
    .await;

    // Without configured sources the market is empty and no install is attempted.
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get().uri("/api/market").to_request(),
    )
    .await;
    assert_eq!(body["success"], true);
    assert!(body["data"]["plugins"].as_array().unwrap().is_empty());
    assert!(body["data"]["errors"].as_array().unwrap().is_empty());

    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/market/install")
            .set_json(serde_json::json!({ "id": "app" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);

    // Point the config at the local registry.
    {
        let mut cfg = state.config.write().await;
        cfg.plugin_market.sources = vec![index];
        cfg.plugin_market.timeout_secs = 5;
    }

    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get().uri("/api/market").to_request(),
    )
    .await;
    let plugins = body["data"]["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), 2, "{body}");
    assert!(plugins
        .iter()
        .all(|p| p["installed_version"].is_null() && p["update_available"] == false));
    assert_eq!(plugins[0]["dependencies"][0]["id"], "base");

    // Installing the root pulls its dependency in and enables both.
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/market/install")
            .set_json(serde_json::json!({ "id": "app" }))
            .to_request(),
    )
    .await;
    assert_eq!(body["success"], true, "{body}");
    assert!(
        body["data"]["failed"].as_array().unwrap().is_empty(),
        "{body}"
    );
    let installed: Vec<&str> = body["data"]["installed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert_eq!(installed, vec!["base", "app"]);
    {
        let cfg = state.config.read().await;
        assert_eq!(
            cfg.plugins.get("app").and_then(|v| v.get("enabled")),
            Some(&serde_json::json!(true))
        );
        assert!(cfg.plugins.contains_key("base"));
    }

    // The list now reports the installed versions, and a second install is a no-op update.
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get().uri("/api/market").to_request(),
    )
    .await;
    let plugins = body["data"]["plugins"].as_array().unwrap();
    assert_eq!(plugins[0]["installed_version"], "1.0.0");
    assert_eq!(plugins[0]["update_available"], false);

    // An unknown id fails with the resolver's message instead of a 500.
    let resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/market/install")
            .set_json(serde_json::json!({ "id": "ghost" }))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);

    // Publish app 1.1.0 → the list flags the update, update-all installs it.
    let (app_manifest_new, app_sha_new) =
        write_package(market_dir.path(), "app", "1.1.0", b"\0asm\x03");
    let index = write_index(
        market_dir.path(),
        vec![
            entry_value(
                "app",
                "1.1.0",
                &app_manifest_new,
                &app_sha_new,
                &[("base", ">=1.0.0")],
            ),
            entry_value("base", "1.0.0", &base_manifest, &base_sha, &[]),
        ],
    );
    {
        let mut cfg = state.config.write().await;
        cfg.plugin_market.sources = vec![index];
    }

    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get().uri("/api/market").to_request(),
    )
    .await;
    let app_entry = body["data"]["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "app")
        .expect("app listed");
    assert_eq!(app_entry["update_available"], true, "{body}");
    assert_eq!(app_entry["installed_version"], "1.0.0");

    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/market/update-all")
            .to_request(),
    )
    .await;
    assert_eq!(body["success"], true, "{body}");
    let updated: Vec<&str> = body["data"]["installed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert_eq!(updated, vec!["app"], "{body}");

    // Afterwards everything is up to date and update-all is a no-op.
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::post()
            .uri("/api/market/update-all")
            .to_request(),
    )
    .await;
    assert!(body["data"]["installed"].as_array().unwrap().is_empty());

    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get().uri("/api/market").to_request(),
    )
    .await;
    let app_entry = body["data"]["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "app")
        .expect("app listed");
    assert_eq!(app_entry["installed_version"], "1.1.0");
    assert_eq!(app_entry["update_available"], false);
}
