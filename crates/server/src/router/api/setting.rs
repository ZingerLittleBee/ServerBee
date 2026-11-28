use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::{Body, Bytes};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, header};
use axum::routing::{get, post, put};
use axum::{Extension, Json, Router};
use futures_util::Stream;
use sea_orm::SqlxSqliteConnector;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tokio_util::io::ReaderStream;

use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::utils::extract_client_ip;
use crate::service::audit::AuditService;
use crate::service::config::ConfigService;
use crate::state::AppState;

struct CleanupBackupStream {
    inner: ReaderStream<tokio::fs::File>,
    path: PathBuf,
}

impl Drop for CleanupBackupStream {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl Stream for CleanupBackupStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

const CONFIG_KEY_SETTINGS: &str = "system_settings";

#[derive(Debug, Clone, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct SystemSettings {
    site_name: Option<String>,
    site_description: Option<String>,
    custom_css: Option<String>,
    custom_js: Option<String>,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/settings", get(get_settings))
        .route("/settings", put(update_settings))
        .route("/settings/backup", post(create_backup))
        .route("/settings/restore", post(restore_backup))
}

#[utoipa::path(
    get,
    path = "/api/settings",
    tag = "settings",
    responses(
        (status = 200, description = "System settings", body = SystemSettings),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<SystemSettings>>, AppError> {
    let settings: SystemSettings = ConfigService::get_typed(&state.db, CONFIG_KEY_SETTINGS)
        .await?
        .unwrap_or_default();
    ok(settings)
}

#[utoipa::path(
    put,
    path = "/api/settings",
    tag = "settings",
    request_body = SystemSettings,
    responses(
        (status = 200, description = "Settings updated", body = SystemSettings),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn update_settings(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SystemSettings>,
) -> Result<Json<ApiResponse<SystemSettings>>, AppError> {
    ConfigService::set_typed(&state.db, CONFIG_KEY_SETTINGS, &body).await?;
    ok(body)
}

/// Download a backup of the SQLite database.
#[utoipa::path(
    post,
    path = "/api/settings/backup",
    tag = "settings",
    responses(
        (status = 200, description = "Database backup file (SQLite)"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn create_backup(
    State(state): State<Arc<AppState>>,
    Extension(actor): Extension<CurrentUser>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Result<axum::response::Response, AppError> {
    let db_path = resolve_db_path(&state.config);

    let backup_path = format!("{db_path}.backup.{}", uuid::Uuid::new_v4());
    use sea_orm::ConnectionTrait;
    let stmt = sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Sqlite,
        format!("VACUUM INTO '{backup_path}'"),
    );
    state
        .db
        .execute(stmt)
        .await
        .map_err(|e| AppError::Internal(format!("Backup failed: {e}")))?;

    let file = match tokio::fs::File::open(&backup_path).await {
        Ok(file) => file,
        Err(e) => {
            let _ = tokio::fs::remove_file(&backup_path).await;
            return Err(AppError::Internal(format!(
                "Failed to open backup file: {e}"
            )));
        }
    };
    let file_len = match file.metadata().await {
        Ok(meta) => meta.len(),
        Err(e) => {
            drop(file);
            let _ = tokio::fs::remove_file(&backup_path).await;
            return Err(AppError::Internal(format!(
                "Failed to stat backup file: {e}"
            )));
        }
    };

    let caller_ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &actor.user_id,
        "settings.backup",
        Some(&format!("bytes={file_len}")),
        &caller_ip,
    )
    .await;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("serverbee_backup_{timestamp}.db");

    let stream = CleanupBackupStream {
        inner: ReaderStream::new(file),
        path: PathBuf::from(&backup_path),
    };

    axum::response::Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, file_len.to_string())
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(Body::from_stream(stream))
        .map_err(|e| AppError::Internal(format!("Response build error: {e}")))
}

/// Restore the database from an uploaded backup file.
/// The server should be restarted after restore.
/// Note: request body is raw binary (application/octet-stream).
#[utoipa::path(
    post,
    path = "/api/settings/restore",
    tag = "settings",
    request_body(content = String, content_type = "application/octet-stream", description = "SQLite backup file"),
    responses(
        (status = 200, description = "Database restored, restart required"),
        (status = 400, description = "Invalid backup file"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn restore_backup(
    State(state): State<Arc<AppState>>,
    Extension(actor): Extension<CurrentUser>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    if body.len() < 16 {
        return Err(AppError::Validation(
            "Invalid backup file: too small".to_string(),
        ));
    }

    // Validate SQLite header magic
    if &body[..16] != b"SQLite format 3\0" {
        return Err(AppError::Validation(
            "Invalid backup file: not a SQLite database".to_string(),
        ));
    }

    let db_path = resolve_db_path(&state.config);

    // Write the uploaded file to a staging path first
    let staging_path = format!("{db_path}.restore");
    tokio::fs::write(&staging_path, &body)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to write restore file: {e}")))?;

    // Replace current database
    let backup_path = format!("{db_path}.pre-restore");
    if std::path::Path::new(&backup_path).exists() {
        let _ = tokio::fs::remove_file(&backup_path).await;
    }

    // Backup current DB
    if std::path::Path::new(&db_path).exists() {
        tokio::fs::rename(&db_path, &backup_path)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to backup current DB: {e}")))?;
    }

    // Move staged file to DB path
    tokio::fs::rename(&staging_path, &db_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to restore DB: {e}")))?;

    // Audit: replacing the live DB with an uploaded file can backdoor the whole
    // instance — record it before the operator restarts.
    //
    // `state.db`'s pooled connections still hold the pre-restore inode (the file
    // we just renamed to `.pre-restore`), so a row written through `state.db`
    // would land in the now-discarded database and be lost after the mandatory
    // restart. Open a short-lived connection to the freshly restored file so the
    // forensic record persists into the DB the server will reopen. A tracing
    // line is emitted unconditionally as a durable fallback.
    let caller_ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    tracing::warn!(
        user_id = %actor.user_id,
        ip = %caller_ip,
        bytes = body.len(),
        "audit: settings.restore — live database replaced via restore endpoint"
    );
    match SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&db_path))
        .await
    {
        Ok(pool) => {
            let restored_db = SqlxSqliteConnector::from_sqlx_sqlite_pool(pool);
            let _ = AuditService::log(
                &restored_db,
                &actor.user_id,
                "settings.restore",
                Some(&format!("bytes={}", body.len())),
                &caller_ip,
            )
            .await;
            let _ = restored_db.close().await;
        }
        Err(e) => {
            tracing::error!("failed to open restored DB to persist restore audit log: {e}");
        }
    }

    ok("Database restored. Please restart the server.")
}

fn resolve_db_path(config: &crate::config::AppConfig) -> String {
    let data_dir = &config.server.data_dir;
    let db_path = &config.database.path;
    if std::path::Path::new(db_path).is_absolute() {
        db_path.clone()
    } else {
        format!("{}/{}", data_dir.trim_end_matches('/'), db_path)
    }
}
