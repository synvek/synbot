use crate::config::WebConfig;
use crate::web::handlers::{api, static_files, ws};
use crate::web::state::AppState;
use crate::web::{BasicAuth, Cors};
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

    let auth = BasicAuth::new(config.auth.clone());
    let cors = Cors::new(config.cors_origins.clone());

    HttpServer::new(move || {
        App::new()
            .wrap(cors.clone())
            .app_data(web::Data::new(state.clone()))
            // API routes (protected by auth if configured)
            .service(
                web::scope("/api")
                    .wrap(auth.clone())
                    .route("/status", web::get().to(api::get_status))
                    .route("/sessions", web::get().to(api::get_sessions))
                    .route("/sessions/{id}", web::get().to(api::get_session_by_id))
                    .route("/channels", web::get().to(api::get_channels))
                    .route("/cron", web::get().to(api::get_cron_jobs))
                    .route("/cron/{id}", web::patch().to(api::update_cron_job))
                    .route("/roles", web::get().to(api::get_roles))
                    .route("/skills", web::get().to(api::get_skills))
                    .route("/skills/{name}", web::get().to(api::get_skill_by_name))
                    .route("/config", web::get().to(api::get_config))
                    .route("/logs", web::get().to(api::get_logs))
            )
            // WebSocket routes
            .route("/ws/chat", web::get().to(ws::ws_chat))
            .route("/ws/logs", web::get().to(ws::ws_logs))
            // Static files (root and catch-all for SPA routing)
            .route("/", web::get().to(static_files::serve_index))
            .route("/{path:.*}", web::get().to(static_files::serve_static))
    })
    .bind(&bind_addr)
    .with_context(|| format!("Failed to bind web server to {}", bind_addr))?
    .run()
    .await
    .context("Web server error")?;

    Ok(())
}
