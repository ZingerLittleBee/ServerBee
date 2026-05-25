//! REST API for custom SPA theme management.
//!
//! All routes are admin-only; they inherit the `require_admin` middleware from
//! the enclosing router block in `api/mod.rs`.

use std::sync::Arc;

use axum::extract::{ConnectInfo, DefaultBodyLimit, Extension, Path, State};
use axum::extract::multipart::MultipartRejection;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use axum::extract::{FromRequest, Multipart, Request};
use axum::http::HeaderMap;

use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::utils::extract_client_ip;
use crate::service::audit::AuditService;
use crate::service::spa_theme::SpaThemeService;
use crate::service::spa_theme::error::SpaThemeError;
use crate::service::spa_theme::service::{SpaThemeSummary, UploadResult};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Upload size limit
// ---------------------------------------------------------------------------

pub const UPLOAD_LIMIT_BYTES: u64 = 25 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Task 11: custom multipart extractor that translates 413 → our JSON contract
// ---------------------------------------------------------------------------

pub struct SpaThemeUpload(pub Multipart);

impl<S> FromRequest<S> for SpaThemeUpload
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Multipart::from_request(req, state).await {
            Ok(m) => Ok(Self(m)),
            Err(rej) => Err(map_rejection(rej)),
        }
    }
}

fn map_rejection(rej: MultipartRejection) -> AppError {
    if rej.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return SpaThemeError::UploadTooLarge { limit_bytes: UPLOAD_LIMIT_BYTES }.into();
    }
    SpaThemeError::InvalidMultipart(rej.to_string()).into()
}

// ---------------------------------------------------------------------------
// Task 12: router + handlers
// ---------------------------------------------------------------------------

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/settings/spa-themes", get(list).post(upload))
        .route("/settings/spa-themes/{uuid}", get(get_one).delete(delete_one))
        .route("/settings/spa-themes/{uuid}/preview", get(get_preview))
        .route("/settings/spa-themes/{uuid}/package", get(get_package))
        .route("/settings/active-spa-theme", get(get_active).put(put_active))
        .layer(DefaultBodyLimit::max(UPLOAD_LIMIT_BYTES as usize))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/settings/spa-themes",
    tag = "spa-themes",
    responses((status = 200, description = "List of uploaded SPA themes", body = Vec<SpaThemeSummary>)),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<SpaThemeSummary>>>, AppError> {
    ok(SpaThemeService::list(&state.db).await?)
}

#[utoipa::path(
    get,
    path = "/api/settings/spa-themes/{uuid}",
    tag = "spa-themes",
    params(("uuid" = String, Path, description = "Theme UUID")),
    responses(
        (status = 200, description = "Theme metadata", body = SpaThemeSummary),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_one(
    State(state): State<Arc<AppState>>,
    Path(uuid): Path<String>,
) -> Result<Json<ApiResponse<SpaThemeSummary>>, AppError> {
    let row = SpaThemeService::get(&state.db, &uuid).await?;
    let active = SpaThemeService::active_uuid(&state.db).await?.unwrap_or_default();
    ok(SpaThemeSummary {
        is_active: active == row.uuid,
        is_superseded: row.is_superseded != 0,
        has_preview: row.preview_data.is_some(),
        uuid: row.uuid,
        manifest_id: row.manifest_id,
        name: row.name,
        version: row.version,
        author: row.author,
        description: row.description,
        size_bytes: row.size_bytes,
        uploaded_by: row.uploaded_by,
        uploaded_at: row.uploaded_at.to_rfc3339(),
    })
}

#[utoipa::path(
    get,
    path = "/api/settings/spa-themes/{uuid}/preview",
    tag = "spa-themes",
    params(("uuid" = String, Path, description = "Theme UUID")),
    responses(
        (status = 200, description = "Preview image bytes"),
        (status = 404, description = "No preview or theme not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_preview(
    State(state): State<Arc<AppState>>,
    Path(uuid): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let row = SpaThemeService::get(&state.db, &uuid).await?;
    let Some(bytes) = row.preview_data else {
        return Err(AppError::NotFound("no preview".into()));
    };
    let mime = row.preview_mime.unwrap_or_else(|| "application/octet-stream".into());
    Ok(([(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response())
}

#[utoipa::path(
    get,
    path = "/api/settings/spa-themes/{uuid}/package",
    tag = "spa-themes",
    params(("uuid" = String, Path, description = "Theme UUID")),
    responses(
        (status = 200, description = "Theme package (zip)"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_package(
    State(state): State<Arc<AppState>>,
    Path(uuid): Path<String>,
) -> Result<axum::response::Response, AppError> {
    let row = SpaThemeService::get(&state.db, &uuid).await?;
    let filename = format!("{}-{}.sbtheme", row.manifest_id, row.version);
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "application/zip".to_string()),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        row.package_data,
    )
        .into_response())
}

#[utoipa::path(
    delete,
    path = "/api/settings/spa-themes/{uuid}",
    tag = "spa-themes",
    params(("uuid" = String, Path, description = "Theme UUID")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 403, description = "Forbidden — admin only"),
        (status = 404, description = "Not found"),
        (status = 409, description = "Theme is currently active"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn delete_one(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Path(uuid): Path<String>,
) -> Result<StatusCode, AppError> {
    // Fetch lightweight metadata for audit BEFORE deleting (so we can record manifest_id+version)
    let row = SpaThemeService::get(&state.db, &uuid).await?;
    let manifest_id = row.manifest_id.clone();
    let version = row.version.clone();
    drop(row); // release BLOB memory early
    SpaThemeService::delete(&state.db, &uuid).await?;
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    );
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "spa_theme.delete",
        Some(
            &serde_json::json!({
                "uuid": uuid,
                "manifest_id": manifest_id,
                "version": version,
            })
            .to_string(),
        ),
        &ip.to_string(),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/settings/spa-themes",
    tag = "spa-themes",
    request_body(content_type = "multipart/form-data", description = "Package file in `package` field"),
    responses(
        (status = 200, description = "Uploaded", body = UploadResult),
        (status = 400, description = "Invalid package / manifest"),
        (status = 403, description = "Forbidden — admin only"),
        (status = 409, description = "Version already exists or downgrade"),
        (status = 413, description = "Package too large"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn upload(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    SpaThemeUpload(mut mp): SpaThemeUpload,
) -> Result<Json<ApiResponse<UploadResult>>, AppError> {
    let mut package_bytes: Option<Vec<u8>> = None;
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| AppError::from(SpaThemeError::InvalidMultipart(e.to_string())))?
    {
        if field.name() == Some("package") {
            package_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::from(SpaThemeError::InvalidMultipart(e.to_string())))?
                    .to_vec(),
            );
            break;
        }
    }
    let bytes = package_bytes.ok_or_else(|| {
        AppError::from(SpaThemeError::InvalidMultipart(
            "missing 'package' field".into(),
        ))
    })?;
    let (row, upgrade) = SpaThemeService::upload(&state.db, bytes, &current_user.user_id).await?;
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    );
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "spa_theme.upload",
        Some(
            &serde_json::json!({
                "uuid": row.uuid,
                "manifest_id": row.manifest_id,
                "version": row.version,
                "size_bytes": row.size_bytes,
            })
            .to_string(),
        ),
        &ip.to_string(),
    )
    .await;
    ok(UploadResult {
        uuid: row.uuid.clone(),
        manifest: serde_json::from_str(&row.manifest_json).unwrap_or(serde_json::Value::Null),
        size_bytes: row.size_bytes,
        preview_url: if row.preview_data.is_some() {
            Some(format!("/api/settings/spa-themes/{}/preview", row.uuid))
        } else {
            None
        },
        is_upgrade_of: upgrade,
    })
}

// ---------------------------------------------------------------------------
// Active-theme DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct PutActiveBody {
    pub theme_id: Option<String>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct ActiveResp {
    pub theme_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Active-theme handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/settings/active-spa-theme",
    tag = "spa-themes",
    responses((status = 200, description = "Currently active theme UUID (null if default)", body = ActiveResp)),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_active(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<ActiveResp>>, AppError> {
    ok(ActiveResp {
        theme_id: SpaThemeService::active_uuid(&state.db).await?,
    })
}

#[utoipa::path(
    put,
    path = "/api/settings/active-spa-theme",
    tag = "spa-themes",
    request_body = PutActiveBody,
    responses(
        (status = 200, description = "Updated active theme", body = ActiveResp),
        (status = 403, description = "Forbidden — admin only"),
        (status = 404, description = "Theme UUID not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn put_active(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<PutActiveBody>,
) -> Result<Json<ApiResponse<ActiveResp>>, AppError> {
    let previous = SpaThemeService::active_uuid(&state.db).await?;
    let new =
        SpaThemeService::set_active(&state.db, &state.active_spa_theme, body.theme_id.as_deref())
            .await?;
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    );
    let (action, audit_uuid) = match (&previous, &new) {
        (_, Some(u)) => ("spa_theme.activate", u.clone()),
        (Some(p), None) => ("spa_theme.deactivate", p.clone()),
        (None, None) => ("spa_theme.deactivate", String::new()),
    };
    if !audit_uuid.is_empty() {
        let _ = AuditService::log(
            &state.db,
            &current_user.user_id,
            action,
            Some(&serde_json::json!({"uuid": audit_uuid}).to_string()),
            &ip.to_string(),
        )
        .await;
    }
    ok(ActiveResp { theme_id: new })
}
