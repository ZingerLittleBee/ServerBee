use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::header;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::{ApiResponse, AppError, ok};
use crate::service::auth::AuthService;
use crate::service::config::ConfigService;
use crate::state::AppState;

const CONFIG_KEY_SETTINGS: &str = "system_settings";
const CONFIG_KEY_AUTO_DISCOVERY: &str = "auto_discovery_key";

#[derive(Debug, Clone, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct SystemSettings {
    site_name: Option<String>,
    site_description: Option<String>,
    custom_css: Option<String>,
    custom_js: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AutoDiscoveryKeyResponse {
    key: String,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/settings", get(get_settings))
        .route("/settings", put(update_settings))
        .route("/settings/auto-discovery-key", get(get_auto_discovery_key))
        .route(
            "/settings/auto-discovery-key",
            put(regenerate_auto_discovery_key),
        )
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

#[utoipa::path(
    get,
    path = "/api/settings/auto-discovery-key",
    tag = "settings",
    responses(
        (status = 200, description = "Auto-discovery key", body = AutoDiscoveryKeyResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_auto_discovery_key(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<AutoDiscoveryKeyResponse>>, AppError> {
    let key = ConfigService::get(&state.db, CONFIG_KEY_AUTO_DISCOVERY)
        .await?
        .unwrap_or_default();
    ok(AutoDiscoveryKeyResponse { key })
}

#[utoipa::path(
    put,
    path = "/api/settings/auto-discovery-key",
    tag = "settings",
    responses(
        (status = 200, description = "Key regenerated", body = AutoDiscoveryKeyResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn regenerate_auto_discovery_key(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<AutoDiscoveryKeyResponse>>, AppError> {
    let new_key = AuthService::generate_session_token();
    ConfigService::set(&state.db, CONFIG_KEY_AUTO_DISCOVERY, &new_key).await?;
    ok(AutoDiscoveryKeyResponse { key: new_key })
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
) -> Result<axum::response::Response, AppError> {
    let db_path = resolve_db_path(&state.config);

    // Use SQLite VACUUM INTO for a consistent backup
    let backup_path = format!("{db_path}.backup");
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

    let bytes = tokio::fs::read(&backup_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read backup file: {e}")))?;

    // Clean up backup file
    let _ = tokio::fs::remove_file(&backup_path).await;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("serverbee_backup_{timestamp}.db");

    axum::response::Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(Body::from(bytes))
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
