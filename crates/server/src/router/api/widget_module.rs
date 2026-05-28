use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
    routing::get,
};

use crate::{
    error::{ApiResponse, AppError, ok},
    service::widget_module::{WidgetModuleService, service::WidgetModuleListEntry},
    state::AppState,
};

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/widget-modules", get(list_modules))
        .route("/widget-modules/{id}/{*asset_path}", get(serve_asset))
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
