//! `/api/projects/{id}/deliverables|preview|download` end-to-end tests.
//!
//! One scenario per test, each against a fresh temp workspace with real OOXML packages built by
//! `synbot::office::testing` — so the ZIP reader, the XML tokenizer and the HTTP layer are all
//! exercised against the same bytes an agent would leave behind.
//!
//! Run with: `cargo test --test delivery_api_tests`

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use actix_web::{test, web, App};
use synbot::office::testing;
use synbot::tools::approval::ApprovalManager;
use synbot::web::handlers::{delivery, projects};
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

fn app() -> actix_web::Scope {
    web::scope("/api")
        .route("/projects", web::post().to(projects::open_project))
        .route(
            "/projects/{id}/deliverables",
            web::get().to(delivery::list_deliverables),
        )
        .route(
            "/projects/{id}/preview",
            web::get().to(delivery::preview_deliverable),
        )
        .route(
            "/projects/{id}/download",
            web::get().to(delivery::download_deliverable),
        )
}

fn write(dir: &Path, rel: &str, bytes: &[u8]) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, bytes).unwrap();
}

/// A workspace that looks like the output folder of a report-writing agent.
fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "output/report.docx",
        &testing::docx("Q3 report", "Ada", &["Revenue grew", "Costs fell"]),
    );
    write(
        dir.path(),
        "output/budget.xlsx",
        &testing::xlsx(&[("Budget", vec![vec!["Item", "Cost"], vec!["Rent", "1200"]])]),
    );
    write(
        dir.path(),
        "output/deck.pptx",
        &testing::pptx(&[vec!["Intro", "hello"], vec!["Numbers", "up 12%"]]),
    );
    write(dir.path(), "output/chart.pdf", b"%PDF-1.7 fake bytes");
    write(dir.path(), "output/notes.csv", b"a,b\n1,2\n");
    write(dir.path(), "main.rs", b"fn main() {}");
    write(dir.path(), "node_modules/pkg/README.md", b"# vendored");
    dir
}

#[actix_web::test]
async fn lists_previews_and_downloads_deliverables() {
    let dir = workspace();
    // The handler receives the path in the POST body, so the workspace is passed explicitly.
    let state = create_state().await;
    let app = test::init_service(App::new().app_data(web::Data::new(state)).service(app())).await;

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

    // --- listing ---------------------------------------------------------
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{id}/deliverables"))
            .to_request(),
    )
    .await;
    assert_eq!(body["success"], true, "{body}");
    let entries = body["data"]["entries"].as_array().unwrap();
    let mut paths: Vec<&str> = entries
        .iter()
        .map(|entry| entry["path"].as_str().unwrap())
        .collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        vec![
            "output/budget.xlsx",
            "output/chart.pdf",
            "output/deck.pptx",
            "output/notes.csv",
            "output/report.docx",
        ],
        "vendored markdown in node_modules and main.rs are not deliverables: {body}"
    );
    let docx = entries
        .iter()
        .find(|entry| entry["path"] == "output/report.docx")
        .unwrap();
    assert_eq!(docx["kind"], "docx");
    assert_eq!(docx["name"], "report.docx");
    assert!(docx["size"].as_u64().unwrap() > 0);
    assert_eq!(body["data"]["truncated"], false);

    // A scope on a subdirectory narrows the listing.
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{id}/deliverables?path=output"))
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["entries"].as_array().unwrap().len(), 5);
    assert_eq!(body["data"]["path"], "output");

    // Rewriting the document makes it the newest entry.
    std::thread::sleep(Duration::from_millis(20));
    write(
        dir.path(),
        "output/report.docx",
        &testing::docx("Q3 report", "Ada", &["Revenue grew"]),
    );
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{id}/deliverables?path=output"))
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["entries"][0]["path"], "output/report.docx");

    // --- docx preview ----------------------------------------------------
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{id}/preview?path=output/report.docx"
            ))
            .to_request(),
    )
    .await;
    assert_eq!(body["success"], true, "{body}");
    assert_eq!(body["data"]["name"], "report.docx");
    assert_eq!(body["data"]["preview"]["kind"], "docx");
    assert_eq!(body["data"]["preview"]["passthrough"], false);
    assert_eq!(
        body["data"]["preview"]["docx"]["paragraphs"],
        serde_json::json!(["Revenue grew"])
    );
    assert_eq!(body["data"]["preview"]["docx"]["title"], "Q3 report");

    // --- xlsx preview ----------------------------------------------------
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{id}/preview?path=output/budget.xlsx"
            ))
            .to_request(),
    )
    .await;
    let sheets = body["data"]["preview"]["xlsx"]["sheets"]
        .as_array()
        .unwrap();
    assert_eq!(sheets[0]["name"], "Budget");
    assert_eq!(sheets[0]["rows"][1], serde_json::json!(["Rent", "1200"]));

    // --- pptx preview ----------------------------------------------------
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{id}/preview?path=output/deck.pptx"))
            .to_request(),
    )
    .await;
    let slides = body["data"]["preview"]["pptx"]["slides"]
        .as_array()
        .unwrap();
    assert_eq!(slides.len(), 2);
    assert_eq!(slides[0]["title"], "Intro");
    assert_eq!(slides[1]["lines"], serde_json::json!(["up 12%"]));

    // --- pdf and text ----------------------------------------------------
    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{id}/preview?path=output/chart.pdf"))
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["preview"]["kind"], "pdf");
    assert_eq!(body["data"]["preview"]["passthrough"], true);

    let body: serde_json::Value = test::call_and_read_body_json(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{id}/preview?path=output/notes.csv"))
            .to_request(),
    )
    .await;
    assert_eq!(body["data"]["preview"]["text"], "a,b\n1,2\n");

    // --- download --------------------------------------------------------
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{id}/download?path=output/report.docx"
            ))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
    );
    assert!(resp
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("attachment; filename=\"report.docx\""));
    assert_eq!(
        resp.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    let body = test::read_body(resp).await;
    assert_eq!(
        body.as_ref(),
        std::fs::read(dir.path().join("output/report.docx")).unwrap()
    );
}

#[actix_web::test]
async fn rejects_unsupported_files_and_escaping_paths() {
    let dir = workspace();
    let state = create_state().await;
    let app = test::init_service(App::new().app_data(web::Data::new(state)).service(app())).await;

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

    // A source file has no previewer.
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{id}/preview?path=main.rs"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);

    // A docx-extension file that is not a package is reported as a bad file, not a 500.
    write(dir.path(), "output/broken.docx", b"not a zip");
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{id}/preview?path=output/broken.docx"
            ))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);

    // Escaping the project root is refused by both routes.
    for route in ["preview", "download"] {
        let resp = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/projects/{id}/{route}?path=../../etc/passwd"))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 400, "{route}");
    }

    // Missing file, missing path and unknown project.
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{id}/preview?path=output/ghost.docx"
            ))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/projects/{id}/download"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 400);

    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/projects/ghost/preview?path=output/report.docx")
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), 404);
}
