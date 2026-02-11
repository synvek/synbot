use crate::config::WebConfig;
use crate::web::handlers::{api, ws};
use crate::web::state::AppState;
use actix_web::{web, App, HttpServer};
use anyhow::{Context, Result};

/// Start the web server with the given configuration and application state
pub async fn start_web_server(config: WebConfig, state: AppState) -> Result<()> {
    if !config.enabled {
        tracing::info!("Web server is disabled in configuration");
        return Ok(());
    }

    let bind_addr = format!("{}:{}", config.host, config.port);
    tracing::info!("Starting web server on {}", bind_addr);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .route("/api/status", web::get().to(api::get_status))
            .route("/api/logs", web::get().to(api::get_logs))
            .route("/ws/chat", web::get().to(ws::ws_chat))
            .route("/ws/logs", web::get().to(ws::ws_logs))
    })
    .bind(&bind_addr)
    .with_context(|| format!("Failed to bind web server to {}", bind_addr))?
    .run()
    .await
    .context("Web server error")?;

    Ok(())
}
