use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use serde::Serialize;

use crate::error::{ok, ApiResponse, AppError};
use crate::service::server::ServerService;
use crate::service::traffic::{
    DailyTraffic, HourlyTraffic, TrafficPrediction, TrafficService, compute_prediction,
    get_cycle_range,
};
use crate::state::AppState;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TrafficResponse {
    pub cycle_start: String,
    pub cycle_end: String,
    pub bytes_in: i64,
    pub bytes_out: i64,
    pub bytes_total: i64,
    pub traffic_limit: Option<i64>,
    pub traffic_limit_type: Option<String>,
    pub usage_percent: Option<f64>,
    pub prediction: Option<TrafficPrediction>,
    pub daily: Vec<DailyTraffic>,
    pub hourly: Vec<HourlyTraffic>,
}

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route("/servers/{id}/traffic", get(get_traffic))
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/traffic",
    params(
        ("id" = String, Path, description = "Server ID"),
    ),
    responses(
        (status = 200, description = "Traffic statistics", body = ApiResponse<TrafficResponse>),
        (status = 404, description = "Server not found"),
    ),
    tag = "servers"
)]
async fn get_traffic(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TrafficResponse>>, AppError> {
    let server = ServerService::get_server(&state.db, &id).await?;

    let billing_cycle = server.billing_cycle.as_deref().unwrap_or("monthly");
    let today = Utc::now().date_naive();
    let (cycle_start, cycle_end) =
        get_cycle_range(billing_cycle, server.billing_start_day, today);

    // Get total traffic for the cycle
    let (bytes_in, bytes_out) =
        TrafficService::query_cycle_traffic(&state.db, &id, cycle_start, cycle_end).await?;
    let bytes_total = bytes_in + bytes_out;

    // Calculate usage percent
    let usage_percent = server.traffic_limit.map(|limit| {
        if limit > 0 {
            let relevant = match server.traffic_limit_type.as_deref() {
                Some("up") => bytes_out,
                Some("down") => bytes_in,
                _ => bytes_total,
            };
            relevant as f64 / limit as f64 * 100.0
        } else {
            0.0
        }
    });

    // Calculate prediction
    let days_elapsed = (today - cycle_start).num_days();
    let days_remaining = (cycle_end - today).num_days();
    let recent_sum = match server.traffic_limit_type.as_deref() {
        Some("up") => bytes_out,
        Some("down") => bytes_in,
        _ => bytes_total,
    };
    let prediction = compute_prediction(
        recent_sum,
        days_elapsed,
        days_remaining,
        server.traffic_limit,
        server.traffic_limit_type.as_deref().unwrap_or("sum"),
    );

    // Get daily breakdown
    let daily =
        TrafficService::query_daily_breakdown(&state.db, &id, cycle_start, cycle_end).await?;

    // Get hourly breakdown for today
    let hourly = TrafficService::query_hourly_breakdown(&state.db, &id, today).await?;

    let response = TrafficResponse {
        cycle_start: cycle_start.format("%Y-%m-%d").to_string(),
        cycle_end: cycle_end.format("%Y-%m-%d").to_string(),
        bytes_in,
        bytes_out,
        bytes_total,
        traffic_limit: server.traffic_limit,
        traffic_limit_type: server.traffic_limit_type,
        usage_percent,
        prediction,
        daily,
        hourly,
    };

    ok(response)
}
