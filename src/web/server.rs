use crate::config::WebConfig;
use crate::web::handlers::{
    api, checkpoints, commands, git, index, plugins, projects, security, static_files, tasks, team,
    ws, ws_agent, ws_terminal,
};
use crate::web::state::AppState;
use crate::web::{BasicAuth, Cors};
use actix_web::{web, App, HttpServer};
use anyhow::{Context, Result};

/// Configure the web application routes.
///
/// REST and WebSocket endpoints share the same authentication middleware. Static
/// assets remain public so unauthenticated users can load the login page.
///
/// Important: do **not** wrap authenticated routes in `web::scope("")` — an empty
/// scope matches every path and returns 404 for `/` and other static assets before
/// the public static routes can run.
pub fn configure_routes(cfg: &mut web::ServiceConfig, state: AppState, auth: BasicAuth) {
    cfg.app_data(web::Data::new(state))
        .service(
            web::scope("/api")
                .wrap(auth.clone())
                .route("/status", web::get().to(api::get_status))
                .route("/sessions", web::get().to(api::get_sessions))
                .route("/sessions/{id}", web::get().to(api::get_session_by_id))
                .route("/channels", web::get().to(api::get_channels))
                .route("/cron", web::get().to(api::get_cron_jobs))
                .route("/cron/{id}", web::patch().to(api::update_cron_job))
                .route("/agents", web::get().to(api::get_agents))
                .route("/skills", web::get().to(api::get_skills))
                .route("/skills/{name}", web::get().to(api::get_skill_by_name))
                .route("/config", web::get().to(api::get_config))
                .route("/config", web::put().to(api::put_config))
                .route("/logs", web::get().to(api::get_logs))
                .route(
                    "/approvals/history",
                    web::get().to(api::get_approval_history),
                )
                .route(
                    "/approvals/pending",
                    web::get().to(api::get_pending_approvals),
                )
                .route("/projects", web::get().to(projects::get_projects))
                .route("/projects", web::post().to(projects::open_project))
                .route("/projects/{id}", web::delete().to(projects::delete_project))
                .route(
                    "/projects/{id}/files",
                    web::get().to(projects::get_project_files),
                )
                .route(
                    "/projects/{id}/file",
                    web::get().to(projects::get_project_file),
                )
                .route("/checkpoints", web::get().to(checkpoints::list_checkpoints))
                .route(
                    "/checkpoints/{turn_id}",
                    web::get().to(checkpoints::get_checkpoint),
                )
                .route(
                    "/checkpoints/{turn_id}/undo",
                    web::post().to(checkpoints::undo_checkpoint),
                )
                .route("/git/status", web::get().to(git::get_status))
                .route("/git/diff", web::get().to(git::get_diff))
                .route("/git/log", web::get().to(git::get_log))
                .route("/tasks", web::get().to(tasks::list_tasks))
                .route("/tasks", web::put().to(tasks::replace_tasks))
                .route("/tasks/{id}", web::patch().to(tasks::update_task))
                .route("/commands", web::get().to(commands::list_commands))
                .route("/plugins", web::get().to(plugins::list_plugins))
                .route("/plugins/install", web::post().to(plugins::install_plugin))
                .route("/plugins/{id}", web::delete().to(plugins::uninstall_plugin))
                .route(
                    "/plugins/{id}/enabled",
                    web::post().to(plugins::set_enabled),
                )
                .route("/plugin-keys", web::get().to(plugins::list_keys))
                .route("/plugin-keys", web::post().to(plugins::add_key))
                .route("/plugin-keys/{name}", web::delete().to(plugins::remove_key))
                .route("/security/overview", web::get().to(security::overview))
                .route("/security/events", web::get().to(security::list_events))
                .route("/security/events", web::delete().to(security::clear_events))
                .route("/security/policy", web::post().to(security::explain_policy))
                .route("/subagents", web::get().to(team::list_subagents))
                .route(
                    "/subagents/{id}/cancel",
                    web::post().to(team::cancel_subagent),
                )
                .route("/index", web::get().to(index::get_index))
                .route("/index/refresh", web::post().to(index::refresh_index))
                .route("/index/symbols", web::get().to(index::search_symbols))
                .route("/index/wiki", web::post().to(index::write_wiki))
                .route(
                    "/approvals/{id}/respond",
                    web::post().to(api::submit_approval_response),
                ),
        )
        .service(
            web::scope("/ws")
                .wrap(auth)
                .route("/chat", web::get().to(ws::ws_chat))
                .route("/agent", web::get().to(ws_agent::ws_agent))
                .route("/terminal", web::get().to(ws_terminal::ws_terminal))
                .route("/logs", web::get().to(ws::ws_logs)),
        )
        .route("/", web::get().to(static_files::serve_index))
        .route("/{path:.*}", web::get().to(static_files::serve_static));
}

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
        let state = state.clone();
        let auth = auth.clone();
        App::new()
            .wrap(cors.clone())
            .configure(move |cfg| configure_routes(cfg, state.clone(), auth.clone()))
    })
    .bind(&bind_addr)
    .with_context(|| format!("Failed to bind web server to {}", bind_addr))?
    .run()
    .await
    .context("Web server error")?;

    Ok(())
}
