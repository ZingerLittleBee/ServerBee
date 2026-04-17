use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Deserialize;

use crate::error::{ApiResponse, AppError, ok};
use crate::service::server_tag;
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetTagsRequest {
    tags: Vec<String>,
}

/// Read router — all authenticated users.
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route("/servers/{id}/tags", get(get_tags))
}

/// Write router — admin only (mounted under the require_admin layer in api::mod).
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new().route("/servers/{id}/tags", put(put_tags))
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/tags",
    operation_id = "get_server_tags",
    tag = "server-tags",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Tags for the server", body = Vec<String>),
        (status = 401, description = "Unauthenticated"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_tags(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<String>>>, AppError> {
    let tags = server_tag::list_tags(&state.db, &id).await?;
    ok(tags)
}

#[utoipa::path(
    put,
    path = "/api/servers/{id}/tags",
    operation_id = "set_server_tags",
    tag = "server-tags",
    params(("id" = String, Path, description = "Server ID")),
    request_body = SetTagsRequest,
    responses(
        (status = 200, description = "Canonical tag list after update", body = Vec<String>),
        (status = 422, description = "Validation error"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden (non-admin)"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn put_tags(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<SetTagsRequest>,
) -> Result<Json<ApiResponse<Vec<String>>>, AppError> {
    let normalized = server_tag::set_tags(&state.db, &id, body.tags).await?;
    ok(normalized)
}
