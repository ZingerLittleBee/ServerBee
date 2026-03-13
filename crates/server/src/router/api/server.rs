use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::entity::server;
use crate::error::{ok, ApiResponse, AppError};
use crate::service::record::{QueryHistoryResult, RecordService};
use crate::service::server::{ServerService, UpdateServerInput};
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct BatchDeleteRequest {
    ids: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct RecordQueryParams {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    #[serde(default = "default_interval")]
    interval: String,
}

fn default_interval() -> String {
    "auto".to_string()
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct GpuRecordQueryParams {
    from: DateTime<Utc>,
    to: DateTime<Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BatchDeleteResponse {
    deleted: u64,
}

/// Server response DTO — excludes sensitive fields (token_hash, token_prefix).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ServerResponse {
    id: String,
    name: String,
    cpu_name: Option<String>,
    cpu_cores: Option<i32>,
    cpu_arch: Option<String>,
    os: Option<String>,
    kernel_version: Option<String>,
    mem_total: Option<i64>,
    swap_total: Option<i64>,
    disk_total: Option<i64>,
    ipv4: Option<String>,
    ipv6: Option<String>,
    region: Option<String>,
    country_code: Option<String>,
    virtualization: Option<String>,
    agent_version: Option<String>,
    group_id: Option<String>,
    weight: i32,
    hidden: bool,
    remark: Option<String>,
    public_remark: Option<String>,
    price: Option<f64>,
    billing_cycle: Option<String>,
    currency: Option<String>,
    expired_at: Option<DateTime<Utc>>,
    traffic_limit: Option<i64>,
    traffic_limit_type: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<server::Model> for ServerResponse {
    fn from(s: server::Model) -> Self {
        Self {
            id: s.id,
            name: s.name,
            cpu_name: s.cpu_name,
            cpu_cores: s.cpu_cores,
            cpu_arch: s.cpu_arch,
            os: s.os,
            kernel_version: s.kernel_version,
            mem_total: s.mem_total,
            swap_total: s.swap_total,
            disk_total: s.disk_total,
            ipv4: s.ipv4,
            ipv6: s.ipv6,
            region: s.region,
            country_code: s.country_code,
            virtualization: s.virtualization,
            agent_version: s.agent_version,
            group_id: s.group_id,
            weight: s.weight,
            hidden: s.hidden,
            remark: s.remark,
            public_remark: s.public_remark,
            price: s.price,
            billing_cycle: s.billing_cycle,
            currency: s.currency,
            expired_at: s.expired_at,
            traffic_limit: s.traffic_limit,
            traffic_limit_type: s.traffic_limit_type,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/servers", get(list_servers))
        .route("/servers/{id}", get(get_server))
        .route("/servers/{id}", put(update_server))
        .route("/servers/{id}", delete(delete_server))
        .route("/servers/batch-delete", post(batch_delete))
        .route("/servers/{id}/records", get(get_records))
        .route("/servers/{id}/gpu-records", get(get_gpu_records))
        .route("/servers/{id}/upgrade", post(trigger_upgrade))
}

#[utoipa::path(
    get,
    path = "/api/servers",
    tag = "servers",
    responses(
        (status = 200, description = "List all servers", body = Vec<ServerResponse>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_servers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<ServerResponse>>>, AppError> {
    let servers = ServerService::list_servers(&state.db).await?;
    ok(servers.into_iter().map(ServerResponse::from).collect())
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}",
    tag = "servers",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Server details", body = ServerResponse),
        (status = 404, description = "Server not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ServerResponse>>, AppError> {
    let server = ServerService::get_server(&state.db, &id).await?;
    ok(ServerResponse::from(server))
}

#[utoipa::path(
    put,
    path = "/api/servers/{id}",
    tag = "servers",
    params(("id" = String, Path, description = "Server ID")),
    request_body = UpdateServerInput,
    responses(
        (status = 200, description = "Server updated", body = ServerResponse),
        (status = 404, description = "Server not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn update_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateServerInput>,
) -> Result<Json<ApiResponse<ServerResponse>>, AppError> {
    let server = ServerService::update_server(&state.db, &id, input).await?;
    ok(ServerResponse::from(server))
}

#[utoipa::path(
    delete,
    path = "/api/servers/{id}",
    tag = "servers",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Server deleted"),
        (status = 404, description = "Server not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn delete_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    ServerService::delete_server(&state.db, &id).await?;
    ok("ok")
}

#[utoipa::path(
    post,
    path = "/api/servers/batch-delete",
    tag = "servers",
    request_body = BatchDeleteRequest,
    responses(
        (status = 200, description = "Batch delete result", body = BatchDeleteResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn batch_delete(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BatchDeleteRequest>,
) -> Result<Json<ApiResponse<BatchDeleteResponse>>, AppError> {
    let deleted = ServerService::batch_delete(&state.db, &body.ids).await?;
    ok(BatchDeleteResponse { deleted })
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/records",
    operation_id = "get_server_records",
    tag = "servers",
    params(
        ("id" = String, Path, description = "Server ID"),
        RecordQueryParams,
    ),
    responses(
        (status = 200, description = "Server metric records"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_records(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<RecordQueryParams>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let result =
        RecordService::query_history(&state.db, &id, params.from, params.to, &params.interval)
            .await?;

    let data = match result {
        QueryHistoryResult::Raw(records) => serde_json::to_value(records)
            .map_err(|e| AppError::Internal(format!("Serialization error: {e}")))?,
        QueryHistoryResult::Hourly(records) => serde_json::to_value(records)
            .map_err(|e| AppError::Internal(format!("Serialization error: {e}")))?,
    };

    ok(data)
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/gpu-records",
    tag = "servers",
    params(
        ("id" = String, Path, description = "Server ID"),
        GpuRecordQueryParams,
    ),
    responses(
        (status = 200, description = "GPU metric records", body = Vec<crate::entity::gpu_record::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_gpu_records(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<GpuRecordQueryParams>,
) -> Result<Json<ApiResponse<Vec<crate::entity::gpu_record::Model>>>, AppError> {
    let records =
        RecordService::query_gpu_history(&state.db, &id, params.from, params.to).await?;
    ok(records)
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpgradeRequest {
    /// Target version string (e.g. "0.2.0")
    version: String,
    /// URL to download the new agent binary from
    download_url: String,
}

#[utoipa::path(
    post,
    path = "/api/servers/{id}/upgrade",
    tag = "servers",
    params(("id" = String, Path, description = "Server ID")),
    request_body = UpgradeRequest,
    responses(
        (status = 200, description = "Upgrade command sent to agent"),
        (status = 404, description = "Server not found or not online"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn trigger_upgrade(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpgradeRequest>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    let sender = state
        .agent_manager
        .get_sender(&id)
        .ok_or_else(|| AppError::NotFound("Server not online".to_string()))?;

    let msg = serverbee_common::protocol::ServerMessage::Upgrade {
        version: body.version,
        download_url: body.download_url,
    };
    sender
        .send(msg)
        .await
        .map_err(|_| AppError::Internal("Failed to send upgrade command".to_string()))?;

    ok("ok")
}
