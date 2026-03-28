use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::error::{ApiResponse, AppError};
use crate::service::mobile_alert::{MobileAlertDetail, MobileAlertEvent, MobileAlertService};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mobile/alerts/{alert_key}", get(get_detail))
}

#[derive(Deserialize)]
struct ListEventsQuery {
    #[serde(default = "default_limit")]
    limit: u64,
}
fn default_limit() -> u64 {
    50
}

#[utoipa::path(
    get,
    path = "/api/alert-events",
    params(("limit" = Option<u64>, Query, description = "Max events to return, default 50")),
    responses((status = 200, body = ApiResponse<Vec<MobileAlertEvent>>)),
    tag = "Mobile Alerts"
)]
async fn list_events(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListEventsQuery>,
) -> Result<Json<ApiResponse<Vec<MobileAlertEvent>>>, AppError> {
    let events = MobileAlertService::list_events(&state.db, query.limit).await?;
    Ok(Json(ApiResponse { data: events }))
}

#[utoipa::path(
    get,
    path = "/api/mobile/alerts/{alert_key}",
    params(("alert_key" = String, Path, description = "Alert key in format rule_id:server_id")),
    responses((status = 200, body = ApiResponse<MobileAlertDetail>)),
    tag = "Mobile Alerts"
)]
async fn get_detail(
    State(state): State<Arc<AppState>>,
    Path(alert_key): Path<String>,
) -> Result<Json<ApiResponse<MobileAlertDetail>>, AppError> {
    let detail = MobileAlertService::get_detail(&state.db, &alert_key).await?;
    Ok(Json(ApiResponse { data: detail }))
}
