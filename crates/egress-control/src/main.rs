mod api;
mod auth;
mod config;
mod db;
mod error;
mod models;

use anyhow::{Context, Result};
use axum::middleware;
use axum::routing::{get, patch, post, put};
use axum::Router;
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing::warn;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub cfg: Arc<config::Config>,
    pub admin_hash: Arc<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();
    let cfg = config::Config::from_env()?;
    let db = db::connect(&cfg.database_url).await?;
    db::migrate(&db).await?;
    db::seed(&db).await?;
    let admin_hash = auth::admin_password_hash(&cfg)?;
    let state = AppState {
        db,
        cfg: Arc::new(cfg.clone()),
        admin_hash: Arc::new(admin_hash),
    };
    auth::spawn_session_cleanup(state.clone());

    let public = Router::new().route("/auth/login", post(auth::login));
    let protected = Router::new()
        .route("/auth/me", get(auth::me))
        .route("/auth/logout", post(auth::logout))
        .route("/overview", get(api::overview::overview))
        .route(
            "/regions",
            get(api::regions::list).post(api::regions::create),
        )
        .route(
            "/regions/{id}",
            patch(api::regions::update).delete(api::regions::delete),
        )
        .route("/nodes", get(api::nodes::list).post(api::nodes::create))
        .route(
            "/nodes/{id}",
            patch(api::nodes::update).delete(api::nodes::delete),
        )
        .route("/lines", get(api::lines::list).post(api::lines::create))
        .route(
            "/lines/{id}",
            get(api::lines::get)
                .patch(api::lines::update)
                .delete(api::lines::delete),
        )
        .route("/lines/{id}/nodes", put(api::lines::set_nodes))
        .route("/audit", get(api::audit::list))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_admin,
        ));

    let internal = Router::new()
        .route("/v1/config/{line}", get(api::internal::line_config))
        .route("/v1/node-config/{node}", get(api::internal::node_config))
        .route("/v1/health/{node}", post(api::internal::report_health))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_internal,
        ));

    let app = Router::new()
        .nest("/api/v1", public.merge(protected))
        .nest("/internal", internal)
        .route("/healthz", get(healthz))
        .with_state(state)
        .fallback_service(
            ServeDir::new(&cfg.web_dir)
                .not_found_service(ServeFile::new(cfg.web_dir.join("index.html"))),
        )
        .layer(TraceLayer::new_for_http());

    if !cfg.web_dir.exists() {
        warn!(
            web_dir = %cfg.web_dir.display(),
            "web build not found; admin UI will be unavailable until `bun run build`"
        );
    }

    let listener = tokio::net::TcpListener::bind(cfg.bind)
        .await
        .with_context(|| format!("bind {}", cfg.bind))?;
    tracing::info!(bind = %cfg.bind, "egress control listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("serve")
}

async fn healthz() -> &'static str {
    "ok\n"
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::Level::INFO.into())
        .from_env_lossy();
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
