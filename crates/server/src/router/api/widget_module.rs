use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Extension, Multipart, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
    routing::{delete, get, post},
};
use serde::Deserialize;

use crate::{
    error::{ApiResponse, AppError, ok},
    middleware::auth::CurrentUser,
    service::widget_module::{
        WidgetModuleService,
        service::{InstalledFrom, WidgetModuleListEntry},
    },
    state::AppState,
};

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/widget-modules", get(list_modules))
        .route("/widget-modules/{id}/{*asset_path}", get(serve_asset))
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/widget-modules", post(install_widget_module))
        .route("/widget-modules/{id}", delete(uninstall_module))
}

#[utoipa::path(
    get,
    path = "/api/widget-modules",
    tag = "widget-modules",
    responses(
        (status = 200, description = "List installed widget modules", body = Vec<WidgetModuleListEntry>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list_modules(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<WidgetModuleListEntry>>>, AppError> {
    let modules = WidgetModuleService::list(&state.db).await?;
    ok(modules)
}

#[utoipa::path(
    get,
    path = "/api/widget-modules/{id}/{asset_path}",
    tag = "widget-modules",
    params(
        ("id" = String, Path, description = "Module ID"),
        ("asset_path" = String, Path, description = "Asset path within the package"),
    ),
    responses(
        (status = 200, description = "Asset bytes"),
        (status = 404, description = "Module or asset not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn serve_asset(
    State(state): State<Arc<AppState>>,
    Path((id, asset_path)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let (bytes, mime) = WidgetModuleService::serve_asset(&state.db, &id, &asset_path).await?;
    let row = WidgetModuleService::get(&state.db, &id).await?;
    let etag_suffix_len = 8.min(row.code_sha256.len());
    let etag = format!("\"{}-{}\"", row.version, &row.code_sha256[..etag_suffix_len]);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&mime).map_err(|e| AppError::Internal(e.to_string()))?,
    );
    headers.insert(
        header::ETAG,
        HeaderValue::from_str(&etag).map_err(|e| AppError::Internal(e.to_string()))?,
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400, immutable"),
    );
    Ok((StatusCode::OK, headers, bytes))
}

#[derive(Debug, Deserialize)]
pub struct InstallQuery {
    pub url: Option<String>,
}

/// Max accepted module size (1 MiB) — applies to both URL fetch and multipart upload.
const MAX_MODULE_BYTES: usize = 1_048_576;

fn is_private_host(host: &str) -> bool {
    if host == "localhost" || host == "127.0.0.1" || host == "::1" {
        return true;
    }
    if host.starts_with("10.") || host.starts_with("192.168.") {
        return true;
    }
    // 172.16.0.0 – 172.31.255.255
    if let Some(rest) = host.strip_prefix("172.")
        && let Some((second, _)) = rest.split_once('.')
        && let Ok(n) = second.parse::<u8>()
        && (16..=31).contains(&n)
    {
        return true;
    }
    false
}

#[utoipa::path(
    post,
    path = "/api/widget-modules",
    tag = "widget-modules",
    params(
        ("url" = Option<String>, Query, description = "HTTPS URL to fetch the widget bundle from. Accepts either a single `.js` file or a `.zip` collection bundle."),
    ),
    request_body(
        content_type = "multipart/form-data",
        description = "Alternatively, upload the widget bundle in a `file` field. Accepts either a single `.js` file or a `.zip` collection bundle.",
    ),
    responses(
        (
            status = 200,
            description = "Installed (or upgraded) widget module(s). For a single `.js` file the response is `{ data: { id, version } }`. For a `.zip` collection it is `{ data: [{ id, version }, ...] }` — one entry per widget in the collection.",
        ),
        (status = 400, description = "Bad URL, unsupported source, or invalid manifest"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn install_widget_module(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<CurrentUser>,
    Query(q): Query<InstallQuery>,
    multipart: Option<Multipart>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let user_id = user.user_id.parse::<i64>().ok();

    let (bytes, from) = if let Some(url) = q.url {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(AppError::BadRequest("url must be http(s)".into()));
        }
        let parsed = url::Url::parse(&url)
            .map_err(|e| AppError::BadRequest(format!("bad url: {e}")))?;
        if let Some(host) = parsed.host_str()
            && is_private_host(host)
        {
            return Err(AppError::BadRequest(
                "private/loopback urls rejected".into(),
            ));
        }
        let resp = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .map_err(|e| AppError::BadRequest(format!("fetch: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::BadRequest(format!(
                "fetch {}: {}",
                url,
                resp.status()
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::Internal(format!("read body: {e}")))?
            .to_vec();
        if bytes.len() > MAX_MODULE_BYTES {
            return Err(AppError::BadRequest("module too large (>1MB)".into()));
        }
        (bytes, InstalledFrom::Url(url))
    } else if let Some(mut mp) = multipart {
        let mut bytes_opt: Option<Vec<u8>> = None;
        let mut name_opt: Option<String> = None;
        while let Some(field) = mp
            .next_field()
            .await
            .map_err(|e| AppError::BadRequest(format!("multipart: {e}")))?
        {
            if field.name() == Some("file") {
                name_opt = field.file_name().map(|s| s.to_string());
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("multipart body: {e}")))?;
                if data.len() > MAX_MODULE_BYTES {
                    return Err(AppError::BadRequest("module too large (>1MB)".into()));
                }
                bytes_opt = Some(data.to_vec());
                break;
            }
        }
        let bytes =
            bytes_opt.ok_or_else(|| AppError::BadRequest("missing 'file' part".into()))?;
        (
            bytes,
            InstalledFrom::Upload(name_opt.unwrap_or_else(|| "upload.js".into())),
        )
    } else {
        return Err(AppError::BadRequest(
            "provide ?url=... or multipart file".into(),
        ));
    };

    if bytes.starts_with(b"PK\x03\x04") {
        let rows = WidgetModuleService::install_collection_from_zip(
            &state.db, bytes, from, user_id,
        )
        .await?;
        let payload: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|r| serde_json::json!({ "id": r.id, "version": r.version }))
            .collect();
        ok(serde_json::Value::Array(payload))
    } else {
        let row =
            WidgetModuleService::install_single_file(&state.db, bytes, from, user_id).await?;
        ok(serde_json::json!({ "id": row.id, "version": row.version }))
    }
}

#[utoipa::path(
    delete,
    path = "/api/widget-modules/{id}",
    tag = "widget-modules",
    params(("id" = String, Path, description = "Module ID")),
    responses(
        (status = 204, description = "Module uninstalled"),
        (status = 400, description = "Cannot uninstall builtin module"),
        (status = 404, description = "Module not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn uninstall_module(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    WidgetModuleService::uninstall(&state.db, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
