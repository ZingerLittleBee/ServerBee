use std::time::Duration;

use sea_orm::SqlxSqliteConnector;
use sea_orm_migration::MigratorTrait;
use sqlx::ConnectOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use tracing_subscriber::EnvFilter;

use serverbee_server::config::AppConfig;
use serverbee_server::migration::Migrator;
use serverbee_server::router::create_router;
use serverbee_server::service::auth::AuthService;
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

    // Connect database. SQLite pragmas are set on the connect options so
    // they apply to every connection in the pool. journal_mode, synchronous,
    // foreign_keys and busy_timeout are per-connection settings; running them
    // once via execute_unprepared previously only configured a single pooled
    // connection, leaving the rest on sqlx defaults.
    let db_path = format!("{}/{}", data_dir, config.database.path);

    let connect_options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        // Preserve the previous sea-orm behavior (sqlx_logging(false)); raw
        // SqliteConnectOptions otherwise logs every statement at debug and
        // slow statements at warn.
        .disable_statement_logging();

    let pool = SqlitePoolOptions::new()
        .max_connections(config.database.max_connections)
        .connect_with(connect_options)
        .await?;

    let db = SqlxSqliteConnector::from_sqlx_sqlite_pool(pool);

    // Run migrations
    Migrator::up(&db, None).await?;
    tracing::info!("Database migrations complete");

    // Initialize admin user (creates if users table is empty)
    let generated_admin_password = AuthService::init_admin(&db).await?;

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
    let s = state.clone();
    tokio::spawn(async move { task::upgrade_timeout::run(s).await });

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

    // Print credentials block — grouped + warn-level so users can spot them immediately.
    if let Some(ref pwd) = generated_admin_password {
        let banner = format!(
            "\n\n\
             ============================================================\n\
             ==                                                        ==\n\
             ==   FIRST-RUN ADMIN CREDENTIALS — SHOWN ONLY ONCE        ==\n\
             ==                                                        ==\n\
             ============================================================\n\
             \n\
             Username:  {}\n\
             Password:  {}\n\
             \n\
             You will be forced to change this password the first time\n\
             you log in. This password is NOT recoverable from the logs\n\
             afterwards — copy it now.\n\
             \n\
             ============================================================\n",
            AuthService::DEFAULT_ADMIN_USERNAME, pwd
        );
        tracing::warn!("{}", banner);
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
