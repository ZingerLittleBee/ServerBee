mod config;
mod entity;
mod error;
mod middleware;
mod migration;
mod router;
mod service;
mod state;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use tracing_subscriber::EnvFilter;

use crate::config::AppConfig;
use crate::migration::Migrator;
use crate::service::auth::AuthService;
use crate::service::config::ConfigService;
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

    // Initialize admin user (creates if users table is empty)
    AuthService::init_admin(&db, &config.admin).await?;

    // Initialize auto-discovery key
    let auto_discovery_key = init_auto_discovery_key(&db, &config).await?;

    // Build AppState
    let state = AppState::new(db, config.clone());

    // Build router
    let app = router::create_router(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.server.listen).await?;

    // Print startup info
    tracing::info!("========================================");
    tracing::info!(
        "ServerBee v{} is ready",
        serverbee_common::constants::VERSION
    );
    tracing::info!("Listening on {}", listener.local_addr()?);
    tracing::info!("Auto-discovery key: {}", auto_discovery_key);
    tracing::info!("========================================");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Server stopped");
    Ok(())
}

/// Initialize or retrieve the auto-discovery key.
///
/// Priority:
/// 1. If `config.auth.auto_discovery_key` is non-empty, use and persist that value.
/// 2. If a key already exists in the DB (`auto_discovery_key` config entry), reuse it.
/// 3. Otherwise, generate a random 32-byte base64url key and persist it.
async fn init_auto_discovery_key(
    db: &sea_orm::DatabaseConnection,
    config: &AppConfig,
) -> anyhow::Result<String> {
    const CONFIG_KEY: &str = "auto_discovery_key";

    // If the user explicitly configured a key, always use that
    if !config.auth.auto_discovery_key.is_empty() {
        let key = config.auth.auto_discovery_key.clone();
        ConfigService::set(db, CONFIG_KEY, &key).await?;
        tracing::info!("Auto-discovery key set from configuration");
        return Ok(key);
    }

    // Check if a key already exists in the database
    if let Some(existing) = ConfigService::get(db, CONFIG_KEY).await? {
        if !existing.is_empty() {
            return Ok(existing);
        }
    }

    // Generate a new random key
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let key = URL_SAFE_NO_PAD.encode(bytes);

    ConfigService::set(db, CONFIG_KEY, &key).await?;
    tracing::info!("Generated new auto-discovery key");

    Ok(key)
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    tracing::info!("Shutdown signal received");
}
