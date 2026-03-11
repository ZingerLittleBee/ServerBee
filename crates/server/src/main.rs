mod config;
mod entity;
mod error;
mod middleware;
mod migration;
mod service;
mod state;

use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use tracing_subscriber::EnvFilter;

use crate::config::AppConfig;
use crate::migration::Migrator;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::load().unwrap_or_default();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.log.level.parse().unwrap_or_else(|_| "info".into())),
        )
        .init();

    tracing::info!(
        "ServerBee v{} starting...",
        serverbee_common::constants::VERSION
    );

    // Ensure data dir exists
    let data_dir = &config.server.data_dir;
    std::fs::create_dir_all(data_dir)?;

    // Connect database
    let db_path = format!("{}/{}", data_dir, config.database.path);
    let db_url = format!("sqlite://{}?mode=rwc", db_path);

    let mut opt = ConnectOptions::new(&db_url);
    opt.max_connections(config.database.max_connections);
    opt.sqlx_logging(false);

    let db = Database::connect(opt).await?;

    // SQLite pragmas
    db.execute_unprepared("PRAGMA journal_mode=WAL").await?;
    db.execute_unprepared("PRAGMA synchronous=NORMAL").await?;
    db.execute_unprepared("PRAGMA busy_timeout=5000").await?;

    // Run migrations
    Migrator::up(&db, None).await?;
    tracing::info!("Database migrations complete");

    // Build AppState
    let state = AppState::new(db, config.clone());

    // Build router
    let app = axum::Router::new()
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.server.listen).await?;
    tracing::info!("Listening on {}", listener.local_addr()?);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Server stopped");
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    tracing::info!("Shutdown signal received");
}
