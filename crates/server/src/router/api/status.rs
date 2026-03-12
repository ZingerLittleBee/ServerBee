use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use sea_orm::*;
use serde::Serialize;

use crate::entity::{server, server_group};
use crate::error::{ok, ApiResponse, AppError};
use crate::state::AppState;

/// Public status info for a single server.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StatusServer {
    pub id: String,
    pub name: String,
    pub region: Option<String>,
    pub country_code: Option<String>,
    pub os: Option<String>,
    pub group_id: Option<String>,
    pub public_remark: Option<String>,
    pub online: bool,
    /// Live metrics (present only when online).
    pub metrics: Option<StatusMetrics>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StatusMetrics {
    pub cpu: f64,
    pub mem_used: i64,
    pub mem_total: i64,
    pub disk_used: i64,
    pub disk_total: i64,
    pub net_in_speed: i64,
    pub net_out_speed: i64,
    pub net_in_transfer: i64,
    pub net_out_transfer: i64,
    pub uptime: u64,
    pub load1: f64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StatusGroup {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StatusPageResponse {
    pub servers: Vec<StatusServer>,
    pub groups: Vec<StatusGroup>,
    pub online_count: usize,
    pub total_count: usize,
}

/// Public route (no auth required).
pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/status", get(public_status))
}

#[utoipa::path(
    get,
    path = "/api/status",
    tag = "status",
    responses(
        (status = 200, description = "Public server status page data", body = StatusPageResponse),
    )
)]
pub async fn public_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<StatusPageResponse>>, AppError> {
    // Fetch non-hidden servers
    let servers = server::Entity::find()
        .filter(server::Column::Hidden.eq(false))
        .order_by_desc(server::Column::Weight)
        .order_by_desc(server::Column::CreatedAt)
        .all(&state.db)
        .await?;

    // Fetch all groups for labeling
    let groups = server_group::Entity::find()
        .all(&state.db)
        .await?
        .into_iter()
        .map(|g| StatusGroup {
            id: g.id,
            name: g.name,
        })
        .collect();

    let total_count = servers.len();
    let mut online_count = 0;

    let status_servers: Vec<StatusServer> = servers
        .into_iter()
        .map(|s| {
            let online = state.agent_manager.is_online(&s.id);
            if online {
                online_count += 1;
            }

            let metrics = if online {
                state
                    .agent_manager
                    .get_latest_report(&s.id)
                    .map(|r| StatusMetrics {
                        cpu: r.cpu,
                        mem_used: r.mem_used,
                        mem_total: s.mem_total.unwrap_or(0),
                        disk_used: r.disk_used,
                        disk_total: s.disk_total.unwrap_or(0),
                        net_in_speed: r.net_in_speed,
                        net_out_speed: r.net_out_speed,
                        net_in_transfer: r.net_in_transfer,
                        net_out_transfer: r.net_out_transfer,
                        uptime: r.uptime,
                        load1: r.load1,
                    })
            } else {
                None
            };

            StatusServer {
                id: s.id,
                name: s.name,
                region: s.region,
                country_code: s.country_code,
                os: s.os,
                group_id: s.group_id,
                public_remark: s.public_remark,
                online,
                metrics,
            }
        })
        .collect();

    ok(StatusPageResponse {
        servers: status_servers,
        groups,
        online_count,
        total_count,
    })
}
