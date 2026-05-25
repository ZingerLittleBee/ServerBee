//! REST API for custom SPA theme management.
//!
//! All routes are admin-only; they inherit the `require_admin` middleware from
//! the enclosing router block in `api/mod.rs`.

use std::sync::Arc;

use axum::extract::multipart::MultipartRejection;
use axum::extract::{
    ConnectInfo, DefaultBodyLimit, Extension, FromRequest, Multipart, Path, Request, State,
};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};

use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::utils::extract_client_ip;
use crate::service::audit::AuditService;
use crate::service::spa_theme::error::SpaThemeError;
use crate::service::spa_theme::service::{SpaThemeSummary, UploadResult};
use crate::service::spa_theme::{SpaThemeService, UPLOAD_LIMIT_BYTES};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Custom multipart extractor that translates 413 → our JSON contract
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
        return SpaThemeError::UploadTooLarge {
            limit_bytes: UPLOAD_LIMIT_BYTES,
        }
        .into();
    }
    SpaThemeError::InvalidMultipart(rej.to_string()).into()
}

/// Map an error returned by `field.bytes()` / `mp.next_field()` in the upload
/// handler. When the body limit is exceeded during lazy field reads (the common
/// path for oversize uploads), `axum::extract::multipart::MultipartError` has
/// status 413 — propagate that as `UploadTooLarge` so the client gets our JSON
/// error contract instead of a generic 400.
fn map_field_error(e: axum::extract::multipart::MultipartError) -> AppError {
    if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return SpaThemeError::UploadTooLarge {
            limit_bytes: UPLOAD_LIMIT_BYTES,
        }
        .into();
    }
    SpaThemeError::InvalidMultipart(e.to_string()).into()
}

// ---------------------------------------------------------------------------
// Router + handlers
// ---------------------------------------------------------------------------

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/settings/spa-themes", get(list).post(upload))
        .route(
            "/settings/spa-themes/{uuid}",
            get(get_one).delete(delete_one),
        )
        .route("/settings/spa-themes/{uuid}/preview", get(get_preview))
        .route("/settings/spa-themes/{uuid}/package", get(get_package))
        .route(
            "/settings/active-spa-theme",
            get(get_active).put(put_active),
        )
        .layer(DefaultBodyLimit::max(UPLOAD_LIMIT_BYTES as usize))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/settings/spa-themes",
    tag = "spa-themes",
    responses(
        (status = 200, description = "List of uploaded SPA themes", body = Vec<SpaThemeSummary>),
        (status = 403, description = "Forbidden — admin only"),
    ),
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
        (status = 403, description = "Forbidden — admin only"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_one(
    State(state): State<Arc<AppState>>,
    Path(uuid): Path<String>,
) -> Result<Json<ApiResponse<SpaThemeSummary>>, AppError> {
    let row = SpaThemeService::get(&state.db, &uuid).await?;
    let active = SpaThemeService::active_uuid(&state.db)
        .await?
        .unwrap_or_default();
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
        (status = 403, description = "Forbidden — admin only"),
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
    let mime = row
        .preview_mime
        .unwrap_or_else(|| "application/octet-stream".into());
    Ok(([(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response())
}

#[utoipa::path(
    get,
    path = "/api/settings/spa-themes/{uuid}/package",
    tag = "spa-themes",
    params(("uuid" = String, Path, description = "Theme UUID")),
    responses(
        (status = 200, description = "Theme package (zip)"),
        (status = 403, description = "Forbidden — admin only"),
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
            (
                axum::http::header::CONTENT_TYPE,
                "application/zip".to_string(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                content_disposition_attachment(&filename),
            ),
        ],
        row.package_data,
    )
        .into_response())
}

/// Build a `Content-Disposition: attachment` header value that is safe for
/// arbitrary filenames.
///
/// Emits both the legacy `filename="..."` (ASCII-sanitized, for old clients)
/// and the RFC 5987 `filename*=UTF-8''<percent-encoded>` (preferred by modern
/// clients). Using both maximizes compatibility while keeping the value
/// well-formed even when the input contains characters like `+` (semver build
/// metadata), spaces, quotes, or non-ASCII.
fn content_disposition_attachment(filename: &str) -> String {
    // For the RFC 5987 `filename*` value we percent-encode anything outside a
    // conservative subset of the spec's `attr-char` set. RFC 8187 `attr-char`
    // technically allows `+`, but we intentionally encode it because some HTTP
    // clients and intermediaries treat raw `+` in encoded extensions as a
    // space, which corrupts semver build-metadata filenames like
    // `theme-1.0.0+build1.sbtheme`.
    fn is_safe(b: u8) -> bool {
        b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'!' | b'#' | b'$' | b'&' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
            )
    }

    let mut encoded = String::with_capacity(filename.len());
    for &b in filename.as_bytes() {
        if is_safe(b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }

    // ASCII fallback: keep safe bytes plus space; replace everything else with
    // `_`. This guarantees the quoted `filename="..."` form is well-formed and
    // cannot contain `"` (header injection) or `\` (line folding / smuggling).
    let mut ascii_fallback = String::with_capacity(filename.len());
    for &b in filename.as_bytes() {
        if is_safe(b) || b == b' ' {
            ascii_fallback.push(b as char);
        } else {
            ascii_fallback.push('_');
        }
    }

    format!("attachment; filename=\"{ascii_fallback}\"; filename*=UTF-8''{encoded}")
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
    while let Some(field) = mp.next_field().await.map_err(map_field_error)? {
        if field.name() == Some("package") {
            package_bytes = Some(field.bytes().await.map_err(map_field_error)?.to_vec());
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
    responses(
        (status = 200, description = "Currently active theme UUID (null if default)", body = ActiveResp),
        (status = 403, description = "Forbidden — admin only"),
    ),
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
    let audit = match (&previous, &new) {
        (_, Some(u)) => Some(("spa_theme.activate", u.clone())),
        (Some(p), None) => Some(("spa_theme.deactivate", p.clone())),
        (None, None) => {
            // No state change: there was no active theme and the request also
            // asks for none. Skip the audit entry so the log reflects actual
            // transitions only.
            tracing::debug!(
                user_id = %current_user.user_id,
                "spa_theme.put_active no-op: no previous active theme and request requests none"
            );
            None
        }
    };
    if let Some((action, audit_uuid)) = audit {
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

#[cfg(test)]
mod tests {
    use super::content_disposition_attachment;

    #[test]
    fn plain_ascii_filename() {
        let v = content_disposition_attachment("acme-1.0.0.sbtheme");
        assert_eq!(
            v,
            "attachment; filename=\"acme-1.0.0.sbtheme\"; filename*=UTF-8''acme-1.0.0.sbtheme"
        );
    }

    #[test]
    fn semver_build_metadata_plus_is_encoded() {
        // The `+` in semver build metadata must be percent-encoded — some
        // clients otherwise interpret raw `+` as a space.
        let v = content_disposition_attachment("acme-1.0.0+build1.sbtheme");
        assert!(v.contains("filename*=UTF-8''acme-1.0.0%2Bbuild1.sbtheme"));
        // ASCII fallback replaces `+` with `_`.
        assert!(v.contains("filename=\"acme-1.0.0_build1.sbtheme\""));
    }

    #[test]
    fn quotes_and_backslashes_are_stripped_from_fallback() {
        let v = content_disposition_attachment("a\"b\\c.sbtheme");
        // Fallback must not contain `"` or `\` (header injection / smuggling).
        let fallback_start = v.find("filename=\"").unwrap() + "filename=\"".len();
        let fallback_end = v[fallback_start..].find('"').unwrap() + fallback_start;
        let fallback = &v[fallback_start..fallback_end];
        assert!(!fallback.contains('"'));
        assert!(!fallback.contains('\\'));
    }

    #[test]
    fn non_ascii_is_percent_encoded() {
        // UTF-8 bytes for "é" are 0xC3 0xA9 → "%C3%A9".
        let v = content_disposition_attachment("café.sbtheme");
        assert!(v.contains("filename*=UTF-8''caf%C3%A9.sbtheme"));
    }
}
