use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;

use crate::error::{ApiResponse, AppError};
use crate::middleware::auth::CurrentUser;
use crate::service::mobile_device::{MobileDeviceService, MobileDeviceState};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mobile/devices/register", post(register))
        .route("/mobile/devices/unregister", post(unregister))
        .route("/mobile/devices/current", get(get_current))
}

#[derive(Deserialize, utoipa::ToSchema)]
struct RegisterRequest {
    installation_id: String,
    platform: String,
    push_token: Option<String>,
    app_version: String,
    locale: String,
    permission_status: String,
    #[serde(default = "default_true")]
    firing_alerts_push: bool,
    #[serde(default)]
    resolved_alerts_push: bool,
}
fn default_true() -> bool {
    true
}

#[utoipa::path(
    post,
    path = "/api/mobile/devices/register",
    request_body = RegisterRequest,
    responses((status = 200)),
    tag = "Mobile Device"
)]
async fn register(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<CurrentUser>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    MobileDeviceService::register(
        &state.db,
        &user.user_id,
        &req.installation_id,
        &req.platform,
        req.push_token.as_deref(),
        &req.app_version,
        &req.locale,
        &req.permission_status,
        req.firing_alerts_push,
        req.resolved_alerts_push,
    )
    .await?;
    Ok(Json(ApiResponse { data: () }))
}

#[derive(Deserialize, utoipa::ToSchema)]
struct UnregisterRequest {
    installation_id: String,
}

#[utoipa::path(
    post,
    path = "/api/mobile/devices/unregister",
    request_body = UnregisterRequest,
    responses((status = 200)),
    tag = "Mobile Device"
)]
async fn unregister(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<CurrentUser>,
    Json(req): Json<UnregisterRequest>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    MobileDeviceService::verify_ownership(&state.db, &req.installation_id, &user.user_id)
        .await?;
    MobileDeviceService::unregister(&state.db, &req.installation_id).await?;
    Ok(Json(ApiResponse { data: () }))
}

#[derive(Deserialize)]
struct CurrentDeviceQuery {
    installation_id: String,
}

#[utoipa::path(
    get,
    path = "/api/mobile/devices/current",
    params(("installation_id" = String, Query, description = "Device installation ID")),
    responses((status = 200, body = ApiResponse<MobileDeviceState>)),
    tag = "Mobile Device"
)]
async fn get_current(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<CurrentDeviceQuery>,
) -> Result<Json<ApiResponse<MobileDeviceState>>, AppError> {
    let state_dto = MobileDeviceService::get_current_owned(
        &state.db,
        &query.installation_id,
        &user.user_id,
    )
    .await?;
    Ok(Json(ApiResponse { data: state_dto }))
}
