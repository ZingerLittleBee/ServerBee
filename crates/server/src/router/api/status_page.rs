//! Admin status-page router (singleton).
//!
//! After R1 the `status_page` table is a singleton config row; admins read
//! the row with `GET /api/status-page` and patch it with
//! `PUT /api/status-page`. The public-facing surface lives separately under
//! `/api/status/*` (see `router::api::status`).

use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, put};
use axum::{Json, Router};

use crate::entity::status_page;
use crate::error::{ApiResponse, AppError, ok};
use crate::service::status_page::{StatusPageService, UpdateStatusPage};
use crate::state::AppState;

/// Read route for the singleton status-page config — accessible to any
/// authenticated user (admin UI and member dashboards both surface it).
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route("/status-page", get(get_status_page))
}

/// Write route for the singleton status-page config — admin-only.
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new().route("/status-page", put(update_status_page))
}

#[utoipa::path(
    get,
    path = "/api/status-page",
    operation_id = "get_status_page",
    tag = "status-pages",
    responses(
        (status = 200, description = "Singleton status page config", body = status_page::Model),
        (status = 404, description = "Singleton row missing"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_status_page(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<status_page::Model>>, AppError> {
    let page = StatusPageService::get_singleton(&state.db).await?;
    ok(page)
}

#[utoipa::path(
    put,
    path = "/api/status-page",
    operation_id = "update_status_page",
    tag = "status-pages",
    request_body = UpdateStatusPage,
    responses(
        (status = 200, description = "Status page updated", body = status_page::Model),
        (status = 404, description = "Singleton row missing"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn update_status_page(
    State(state): State<Arc<AppState>>,
    Json(input): Json<UpdateStatusPage>,
) -> Result<Json<ApiResponse<status_page::Model>>, AppError> {
    let page = StatusPageService::update_singleton(&state.db, input).await?;
    ok(page)
}
