use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use sea_orm::{ConnectOptions, ConnectionTrait, Database};
use sea_orm_migration::MigratorTrait;
use tracing_subscriber::EnvFilter;

use serverbee_server::config::AppConfig;
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
use serverbee_server::service::config::ConfigService;
use serverbee_server::state::AppState;
use serverbee_server::task;

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

    if !config.server.trusted_proxies.is_empty() {
        tracing::info!(
            "Trusting X-Forwarded-For from {} CIDR range(s). Set server.trusted_proxies = [] to disable.",
            config.server.trusted_proxies.len()
        );
    }

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
    db.execute_unprepared("PRAGMA foreign_keys=ON").await?;

    // Run migrations
    Migrator::up(&db, None).await?;
    tracing::info!("Database migrations complete");

    // Initialize admin user (creates if users table is empty)
    let generated_admin_password = AuthService::init_admin(&db, &config.admin).await?;

    // Initialize auto-discovery key
    let auto_discovery_key = init_auto_discovery_key(&db, &config).await?;

    // Build AppState
    let state = AppState::new(db, config.clone())
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Spawn background tasks
    let s = state.clone();
    tokio::spawn(async move { task::record_writer::run(s).await });
    let s = state.clone();
    tokio::spawn(async move { task::offline_checker::run(s).await });
    let s = state.clone();
    tokio::spawn(async move { task::aggregator::run(s).await });
    let s = state.clone();
    tokio::spawn(async move { task::cleanup::run(s).await });
    let s = state.clone();
    tokio::spawn(async move { task::session_cleaner::run(s).await });
    let s = state.clone();
    tokio::spawn(async move { task::alert_evaluator::run(s).await });
    let s = state.clone();
    tokio::spawn(async move { task::task_scheduler::run(s).await });
    let s = state.clone();
    tokio::spawn(async move { task::service_monitor_checker::run(s).await });

    // Build router
    let app = create_router(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.server.listen).await?;

    // Print startup info
    tracing::info!("========================================");
    tracing::info!(
        "ServerBee v{} is ready",
        serverbee_common::constants::VERSION
    );
    tracing::info!("Listening on {}", listener.local_addr()?);
    tracing::info!("========================================");

    // Print credentials block — grouped so users can spot them immediately
    if generated_admin_password.is_some() || !auto_discovery_key.is_empty() {
        let mut credentials = String::from("\n\n********************************************");
        credentials.push_str("\n***       IMPORTANT: Save these now       ***");
        credentials.push_str("\n********************************************\n");
        if let Some(ref pwd) = generated_admin_password {
            credentials.push_str(&format!("\n  Admin username:      {}", config.admin.username));
            credentials.push_str(&format!("\n  Admin password:      {}", pwd));
        }
        credentials.push_str(&format!("\n  Auto-discovery key:  {}", auto_discovery_key));
        credentials.push_str("\n\n********************************************\n");
        tracing::info!("{}", credentials);
    }

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
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
    if let Some(existing) = ConfigService::get(db, CONFIG_KEY).await?
        && !existing.is_empty()
    {
        return Ok(existing);
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
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received");
}
