use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, QueryFilter, ColumnTrait};
use serde::{Deserialize, Serialize};

use crate::entity::server;
use crate::error::{ok, ApiResponse, AppError};
use crate::service::audit::AuditService;
use crate::service::ping::PingService;
use crate::service::record::{QueryHistoryResult, RecordService};
use crate::service::server::{ServerService, UpdateServerInput};
use crate::state::AppState;
use serverbee_common::protocol::{BrowserMessage, ServerMessage};

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
    pub capabilities: i32,
    pub protocol_version: i32,
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
            capabilities: s.capabilities,
            protocol_version: s.protocol_version,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}

/// GET endpoints accessible to all authenticated users (admin + member).
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/servers", get(list_servers))
        .route("/servers/{id}", get(get_server))
        .route("/servers/{id}/records", get(get_records))
        .route("/servers/{id}/gpu-records", get(get_gpu_records))
}

/// Write endpoints (PUT/DELETE/POST) restricted to admin users only.
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/servers/{id}", put(update_server))
        .route("/servers/{id}", delete(delete_server))
        .route("/servers/batch-delete", post(batch_delete))
        .route("/servers/batch-capabilities", put(batch_update_capabilities))
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
    let caps_changed = input.capabilities.is_some();
    let server = ServerService::update_server(&state.db, &id, input).await?;

    // If capabilities changed, broadcast + re-sync
    if caps_changed {
        let new_caps = server.capabilities as u32;

        // Send CapabilitiesSync to Agent (if online and protocol_version >= 2)
        if let Some(pv) = state.agent_manager.get_protocol_version(&id)
            && pv >= 2
            && let Some(tx) = state.agent_manager.get_sender(&id)
        {
            let _ = tx
                .send(ServerMessage::CapabilitiesSync {
                    capabilities: new_caps,
                })
                .await;
        }

        // Broadcast to browsers
        state
            .agent_manager
            .broadcast_browser(BrowserMessage::CapabilitiesChanged {
                server_id: id.clone(),
                capabilities: new_caps,
            });

        // Re-sync ping tasks
        PingService::sync_tasks_to_agent(&state.db, &state.agent_manager, &id).await;
    }

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
    let server = ServerService::get_server(&state.db, &id).await?;
    if !serverbee_common::constants::has_capability(
        server.capabilities as u32,
        serverbee_common::constants::CAP_UPGRADE,
    ) {
        return Err(AppError::Forbidden("Upgrade is disabled for this server".into()));
    }

    let sender = state
        .agent_manager
        .get_sender(&id)
        .ok_or_else(|| AppError::NotFound("Server not online".to_string()))?;

    let msg = ServerMessage::Upgrade {
        version: body.version,
        download_url: body.download_url,
    };
    sender
        .send(msg)
        .await
        .map_err(|_| AppError::Internal("Failed to send upgrade command".to_string()))?;

    ok("ok")
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct BatchCapabilitiesRequest {
    server_ids: Vec<String>,
    #[serde(default)]
    set: u32,
    #[serde(default)]
    unset: u32,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BatchCapabilitiesResponse {
    updated: u64,
}

#[utoipa::path(
    put,
    path = "/api/servers/batch-capabilities",
    tag = "servers",
    request_body = BatchCapabilitiesRequest,
    responses(
        (status = 200, description = "Batch capabilities update result", body = BatchCapabilitiesResponse),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn batch_update_capabilities(
    State(state): State<Arc<AppState>>,
    Extension((user_id, _role, ip)): Extension<(String, String, String)>,
    Json(input): Json<BatchCapabilitiesRequest>,
) -> Result<Json<ApiResponse<BatchCapabilitiesResponse>>, AppError> {
    use serverbee_common::constants::*;

    // Validate bits within mask
    if input.set & !CAP_VALID_MASK != 0 || input.unset & !CAP_VALID_MASK != 0 {
        return Err(AppError::Validation("Invalid capability bits".into()));
    }
    // No overlap
    if input.set & input.unset != 0 {
        return Err(AppError::Validation("set and unset must not overlap".into()));
    }
    if input.server_ids.is_empty() {
        return ok(BatchCapabilitiesResponse { updated: 0 });
    }

    let servers = server::Entity::find()
        .filter(server::Column::Id.is_in(input.server_ids.iter().cloned()))
        .all(&state.db)
        .await?;

    let mut count = 0u64;
    for s in &servers {
        let old_caps = s.capabilities as u32;
        let new_caps = (old_caps & !input.unset) | input.set;
        if new_caps == old_caps {
            continue;
        }

        let mut active: server::ActiveModel = s.clone().into();
        active.capabilities = Set(new_caps as i32);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&state.db).await?;
        count += 1;

        // Sync to agent if online and protocol v2+
        if let Some(pv) = state.agent_manager.get_protocol_version(&s.id)
            && pv >= 2
            && let Some(tx) = state.agent_manager.get_sender(&s.id)
        {
            let _ = tx.send(ServerMessage::CapabilitiesSync { capabilities: new_caps }).await;
        }

        // Broadcast to browsers
        state.agent_manager.broadcast_browser(BrowserMessage::CapabilitiesChanged {
            server_id: s.id.clone(),
            capabilities: new_caps,
        });

        // Re-sync ping tasks if ping bits changed
        let ping_mask = CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP;
        if old_caps & ping_mask != new_caps & ping_mask {
            PingService::sync_tasks_to_agent(&state.db, &state.agent_manager, &s.id).await;
        }

        // Audit log
        let detail = serde_json::json!({
            "server_id": s.id,
            "old": old_caps,
            "new": new_caps,
        })
        .to_string();
        let _ = AuditService::log(&state.db, &user_id, "capabilities_changed", Some(&detail), &ip).await;
    }

    ok(BatchCapabilitiesResponse { updated: count })
}
