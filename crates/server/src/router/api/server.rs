use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};

use crate::router::utils::extract_client_ip;
use chrono::{DateTime, Utc};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};
use serde::{Deserialize, Serialize};

use crate::entity::server;
use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::api::network_probe::{
    get_server_network_anomalies, get_server_network_records, get_server_network_summary,
    get_server_network_targets,
};
use crate::service::audit::AuditService;
use crate::service::network_probe::NetworkProbeService;
use crate::service::ping::PingService;
use crate::service::record::{QueryHistoryResult, RecordService};
use crate::service::server::{ServerService, UpdateServerInput};
use crate::state::AppState;
use serverbee_common::protocol::{BrowserMessage, ServerMessage};
use serverbee_common::types::NetworkProbeTarget;

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

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CleanupResponse {
    deleted_count: u64,
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
    billing_start_day: Option<i32>,
    pub capabilities: i32,
    pub protocol_version: i32,
    features: Vec<String>,
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
            billing_start_day: s.billing_start_day,
            capabilities: s.capabilities,
            protocol_version: s.protocol_version,
            features: serde_json::from_str(&s.features).unwrap_or_default(),
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
        .route(
            "/servers/{id}/network-probes/targets",
            get(get_server_network_targets),
        )
        .route(
            "/servers/{id}/network-probes/records",
            get(get_server_network_records),
        )
        .route(
            "/servers/{id}/network-probes/summary",
            get(get_server_network_summary),
        )
        .route(
            "/servers/{id}/network-probes/anomalies",
            get(get_server_network_anomalies),
        )
}

/// Write endpoints (PUT/DELETE/POST) restricted to admin users only.
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/servers/{id}", put(update_server))
        .route("/servers/{id}", delete(delete_server))
        .route("/servers/batch-delete", post(batch_delete))
        .route("/servers/cleanup", delete(cleanup_orphaned_servers))
        .route(
            "/servers/batch-capabilities",
            put(batch_update_capabilities),
        )
        .route("/servers/{id}/upgrade", post(trigger_upgrade))
        .route(
            "/servers/{id}/network-probes/targets",
            put(set_server_network_targets),
        )
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<UpdateServerInput>,
) -> Result<Json<ApiResponse<ServerResponse>>, AppError> {
    use serverbee_common::constants::{
        CAP_DOCKER, CAP_PING_HTTP, CAP_PING_ICMP, CAP_PING_TCP, has_capability,
    };
    let user_id = &current_user.user_id;
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();

    // Capture old caps before update for diffing
    let old_caps = if input.capabilities.is_some() {
        Some(
            ServerService::get_server(&state.db, &id)
                .await?
                .capabilities as u32,
        )
    } else {
        None
    };

    let server = ServerService::update_server(&state.db, &id, input).await?;

    // If capabilities changed, broadcast + re-sync
    if let Some(old) = old_caps {
        let new_caps = server.capabilities as u32;
        state.agent_manager.update_capabilities(&id, new_caps);

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

        // Re-sync ping tasks only if ping bits changed
        let ping_mask = CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP;
        if old & ping_mask != new_caps & ping_mask {
            PingService::sync_tasks_to_agent(&state.db, &state.agent_manager, &id).await;
        }

        // Docker capability revoked — teardown
        if has_capability(old, CAP_DOCKER) && !has_capability(new_caps, CAP_DOCKER) {
            // Clear in-memory Docker caches
            state.agent_manager.clear_docker_caches(&id);
            // Remove all docker viewer subscriptions for this server
            state.docker_viewers.remove_all_for_server(&id);
            // Remove all docker log sessions for this server
            let log_session_ids = state
                .agent_manager
                .remove_docker_log_sessions_for_server(&id);
            // Tell agent to stop docker streams
            if let Some(tx) = state.agent_manager.get_sender(&id) {
                let _ = tx.send(ServerMessage::DockerStopStats).await;
                let _ = tx.send(ServerMessage::DockerEventsStop).await;
                for sid in &log_session_ids {
                    let _ = tx
                        .send(ServerMessage::DockerLogsStop {
                            session_id: sid.clone(),
                        })
                        .await;
                }
            }
            // Broadcast unavailability
            state
                .agent_manager
                .broadcast_browser(BrowserMessage::DockerAvailabilityChanged {
                    server_id: id.clone(),
                    available: false,
                });
        }

        // Audit log
        let detail = serde_json::json!({
            "server_id": id,
            "old": old,
            "new": new_caps,
        })
        .to_string();
        let _ = AuditService::log(
            &state.db,
            user_id,
            "capabilities_changed",
            Some(&detail),
            &ip,
        )
        .await;
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
        (status = 200, description = "Server metric records", body = Vec<crate::entity::record::Model>),
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
    let records = RecordService::query_gpu_history(&state.db, &id, params.from, params.to).await?;
    ok(records)
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpgradeRequest {
    /// Target version string (e.g. "0.2.0" or "v0.2.0")
    version: String,
}

/// Map agent-reported OS string to release asset platform suffix.
fn map_os(os: &str) -> Option<&'static str> {
    let lower = os.to_lowercase();
    if lower.contains("linux") {
        Some("linux")
    } else if lower.contains("mac") || lower.contains("darwin") {
        Some("darwin")
    } else if lower.contains("windows") {
        Some("windows")
    } else {
        None
    }
}

/// Map Rust arch string to release asset arch suffix.
fn map_arch(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" => Some("amd64"),
        "aarch64" => Some("arm64"),
        _ => None,
    }
}

/// Normalize version string: strip optional 'v' prefix.
fn normalize_version(version: &str) -> &str {
    version.strip_prefix('v').unwrap_or(version)
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
    use serverbee_common::constants::{CAP_UPGRADE, has_capability};

    let server = ServerService::get_server(&state.db, &id).await?;
    let caps = server.capabilities as u32;
    if !has_capability(caps, CAP_UPGRADE) {
        return Err(AppError::Forbidden(
            "Upgrade capability not enabled for this server".into(),
        ));
    }

    let version = normalize_version(&body.version);

    // Validate version format
    if version.is_empty() || !version.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return Err(AppError::BadRequest("Invalid version format".into()));
    }

    // Get agent platform info
    let (os_raw, arch_raw) = state
        .agent_manager
        .get_agent_platform(&id)
        .ok_or_else(|| {
            AppError::NotFound("Agent not connected or platform info unavailable".into())
        })?;

    let os = map_os(&os_raw)
        .ok_or_else(|| AppError::BadRequest(format!("Unsupported agent OS: {os_raw}")))?;
    let arch = map_arch(&arch_raw)
        .ok_or_else(|| AppError::BadRequest(format!("Unsupported agent arch: {arch_raw}")))?;

    // Build asset name
    let asset_name = if os == "windows" {
        format!("serverbee-agent-{os}-{arch}.exe")
    } else {
        format!("serverbee-agent-{os}-{arch}")
    };

    let base_url = &state.config.upgrade.release_base_url;
    let download_url = format!("{base_url}/download/v{version}/{asset_name}");

    // Fetch checksums.txt
    let checksums_url = format!("{base_url}/download/v{version}/checksums.txt");
    let checksums_response = reqwest::get(&checksums_url)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch checksums: {e}")))?;

    if !checksums_response.status().is_success() {
        return Err(AppError::NotFound(format!(
            "Checksums not found for version v{version} (HTTP {})",
            checksums_response.status()
        )));
    }

    let checksums_body = checksums_response
        .text()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read checksums: {e}")))?;

    // Parse: each line is "<sha256>  <filename>" or "<sha256> <filename>"
    let sha256 = checksums_body
        .lines()
        .find_map(|line| {
            let mut parts = line.splitn(2, |c: char| c.is_whitespace());
            let hash = parts.next()?;
            let name = parts.next()?.trim();
            if name == asset_name {
                Some(hash.to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "Checksum not found for {asset_name} in v{version} release"
            ))
        })?;

    let sender = state
        .agent_manager
        .get_sender(&id)
        .ok_or_else(|| AppError::NotFound("Agent not connected".into()))?;

    let msg = ServerMessage::Upgrade {
        version: version.to_string(),
        download_url,
        sha256,
    };
    sender
        .send(msg)
        .await
        .map_err(|_| AppError::Internal("Failed to send upgrade command".into()))?;

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

/// Side effects to execute after transaction commit.
///
/// Collected during the DB transaction phase so that WebSocket broadcasts
/// and audit log writes happen only after a successful commit.
struct CapabilityChangeEffect {
    server_id: String,
    old_caps: u32,
    new_caps: u32,
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Json(input): Json<BatchCapabilitiesRequest>,
) -> Result<Json<ApiResponse<BatchCapabilitiesResponse>>, AppError> {
    use serverbee_common::constants::*;
    let user_id = &current_user.user_id;
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();

    // Validate bits within mask
    if input.set & !CAP_VALID_MASK != 0 || input.unset & !CAP_VALID_MASK != 0 {
        return Err(AppError::Validation("Invalid capability bits".into()));
    }
    // No overlap
    if input.set & input.unset != 0 {
        return Err(AppError::Validation(
            "set and unset must not overlap".into(),
        ));
    }
    if input.server_ids.is_empty() {
        return ok(BatchCapabilitiesResponse { updated: 0 });
    }

    let servers = server::Entity::find()
        .filter(server::Column::Id.is_in(input.server_ids.iter().cloned()))
        .all(&state.db)
        .await?;

    let mut count = 0u64;
    let mut effects: Vec<CapabilityChangeEffect> = Vec::new();

    // Phase 1: All DB updates in a single transaction
    let txn = state.db.begin().await?;
    for s in &servers {
        let old_caps = s.capabilities as u32;
        let new_caps = (old_caps & !input.unset) | input.set;
        if new_caps == old_caps {
            continue;
        }

        let mut active: server::ActiveModel = s.clone().into();
        active.capabilities = Set(new_caps as i32);
        active.updated_at = Set(chrono::Utc::now());
        active.update(&txn).await?;
        count += 1;

        effects.push(CapabilityChangeEffect {
            server_id: s.id.clone(),
            old_caps,
            new_caps,
        });
    }
    txn.commit().await?;

    // Phase 2: Side effects (fire-and-forget, all idempotent)
    for effect in &effects {
        let CapabilityChangeEffect {
            server_id,
            old_caps,
            new_caps,
        } = effect;
        let new_caps = *new_caps;
        let old_caps = *old_caps;

        state.agent_manager.update_capabilities(server_id, new_caps);

        // Sync to agent if online and protocol v2+
        if let Some(pv) = state.agent_manager.get_protocol_version(server_id)
            && pv >= 2
            && let Some(tx) = state.agent_manager.get_sender(server_id)
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
                server_id: server_id.clone(),
                capabilities: new_caps,
            });

        // Re-sync ping tasks if ping bits changed
        let ping_mask = CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP;
        if old_caps & ping_mask != new_caps & ping_mask {
            PingService::sync_tasks_to_agent(&state.db, &state.agent_manager, server_id).await;
        }

        // Docker capability revoked — teardown
        if has_capability(old_caps, CAP_DOCKER) && !has_capability(new_caps, CAP_DOCKER) {
            state.agent_manager.clear_docker_caches(server_id);
            state.docker_viewers.remove_all_for_server(server_id);
            let log_session_ids = state
                .agent_manager
                .remove_docker_log_sessions_for_server(server_id);
            if let Some(tx) = state.agent_manager.get_sender(server_id) {
                let _ = tx.send(ServerMessage::DockerStopStats).await;
                let _ = tx.send(ServerMessage::DockerEventsStop).await;
                for sid in &log_session_ids {
                    let _ = tx
                        .send(ServerMessage::DockerLogsStop {
                            session_id: sid.clone(),
                        })
                        .await;
                }
            }
            state
                .agent_manager
                .broadcast_browser(BrowserMessage::DockerAvailabilityChanged {
                    server_id: server_id.clone(),
                    available: false,
                });
        }

        // Audit log
        let detail = serde_json::json!({
            "server_id": server_id,
            "old": old_caps,
            "new": new_caps,
        })
        .to_string();
        let _ = AuditService::log(
            &state.db,
            user_id,
            "capabilities_changed",
            Some(&detail),
            &ip,
        )
        .await;
    }

    ok(BatchCapabilitiesResponse { updated: count })
}

// ---------------------------------------------------------------------------
// Per-server network probe write handler
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetServerNetworkTargetsRequest {
    target_ids: Vec<String>,
}

#[utoipa::path(
    put,
    path = "/api/servers/{id}/network-probes/targets",
    operation_id = "set_server_network_targets",
    tag = "network-probes",
    params(("id" = String, Path, description = "Server ID")),
    request_body = SetServerNetworkTargetsRequest,
    responses(
        (status = 200, description = "Network probe targets updated for server"),
        (status = 422, description = "Validation error (max 20 targets)"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn set_server_network_targets(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<SetServerNetworkTargetsRequest>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    NetworkProbeService::set_server_targets(&state.db, &id, body.target_ids).await?;

    // Push updated NetworkProbeSync to agent if online
    if let Some(tx) = state.agent_manager.get_sender(&id) {
        let targets = NetworkProbeService::get_server_targets(&state.db, &id).await?;
        let setting = NetworkProbeService::get_setting(&state.db).await?;
        let probe_targets: Vec<NetworkProbeTarget> = targets
            .into_iter()
            .map(|t| NetworkProbeTarget {
                target_id: t.id,
                name: t.name,
                target: t.target,
                probe_type: t.probe_type,
            })
            .collect();
        let msg = ServerMessage::NetworkProbeSync {
            targets: probe_targets,
            interval: setting.interval,
            packet_count: setting.packet_count,
        };
        let _ = tx.send(msg).await;
    }

    ok("ok")
}

#[utoipa::path(
    delete,
    path = "/api/servers/cleanup",
    tag = "servers",
    responses(
        (status = 200, description = "Orphaned servers cleaned up", body = CleanupResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn cleanup_orphaned_servers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<CleanupResponse>>, AppError> {
    use crate::entity::*;

    let orphans = server::Entity::find()
        .filter(server::Column::Name.eq("New Server"))
        .filter(server::Column::Os.is_null())
        .all(&state.db)
        .await?;

    let orphan_ids: Vec<String> = orphans.iter().map(|s| s.id.clone()).collect();
    if orphan_ids.is_empty() {
        return ok(CleanupResponse { deleted_count: 0 });
    }

    let txn = state.db.begin().await?;

    // Tables with server_id FK — delete rows
    record::Entity::delete_many()
        .filter(record::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    record_hourly::Entity::delete_many()
        .filter(record_hourly::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    gpu_record::Entity::delete_many()
        .filter(gpu_record::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    alert_state::Entity::delete_many()
        .filter(alert_state::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    network_probe_config::Entity::delete_many()
        .filter(network_probe_config::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    network_probe_record::Entity::delete_many()
        .filter(network_probe_record::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    network_probe_record_hourly::Entity::delete_many()
        .filter(network_probe_record_hourly::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    traffic_state::Entity::delete_many()
        .filter(traffic_state::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    traffic_hourly::Entity::delete_many()
        .filter(traffic_hourly::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    traffic_daily::Entity::delete_many()
        .filter(traffic_daily::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    uptime_daily::Entity::delete_many()
        .filter(uptime_daily::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    task_result::Entity::delete_many()
        .filter(task_result::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    server_tag::Entity::delete_many()
        .filter(server_tag::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    docker_event::Entity::delete_many()
        .filter(docker_event::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;
    ping_record::Entity::delete_many()
        .filter(ping_record::Column::ServerId.is_in(&orphan_ids))
        .exec(&txn)
        .await?;

    // Tables with server_ids_json — per-table rules
    cleanup_json_array_tables(&txn, &orphan_ids).await?;

    let deleted = server::Entity::delete_many()
        .filter(server::Column::Id.is_in(&orphan_ids))
        .exec(&txn)
        .await?;

    txn.commit().await?;

    tracing::info!("Cleaned up {} orphaned servers", deleted.rows_affected);
    ok(CleanupResponse {
        deleted_count: deleted.rows_affected,
    })
}

async fn cleanup_json_array_tables(
    txn: &sea_orm::DatabaseTransaction,
    orphan_ids: &[String],
) -> Result<(), AppError> {
    use crate::entity::*;

    // ping_tasks: delete if empty
    for task in ping_task::Entity::find().all(txn).await? {
        if let Some(new_json) = remove_ids_from_json(&task.server_ids_json, orphan_ids) {
            if new_json == "[]" {
                ping_task::Entity::delete_by_id(&task.id).exec(txn).await?;
            } else {
                let mut active: ping_task::ActiveModel = task.into();
                active.server_ids_json = Set(new_json);
                active.update(txn).await?;
            }
        }
    }

    // tasks: delete if empty
    for t in task::Entity::find().all(txn).await? {
        if let Some(new_json) = remove_ids_from_json(&t.server_ids_json, orphan_ids) {
            if new_json == "[]" {
                task::Entity::delete_by_id(&t.id).exec(txn).await?;
            } else {
                let mut active: task::ActiveModel = t.into();
                active.server_ids_json = Set(new_json);
                active.update(txn).await?;
            }
        }
    }

    // alert_rules: delete if empty (+ related alert_states)
    for rule in alert_rule::Entity::find().all(txn).await? {
        if let Some(ref json) = rule.server_ids_json {
            if let Some(new_json) = remove_ids_from_json(json, orphan_ids) {
                if new_json == "[]" {
                    alert_state::Entity::delete_many()
                        .filter(alert_state::Column::RuleId.eq(&rule.id))
                        .exec(txn)
                        .await?;
                    alert_rule::Entity::delete_by_id(&rule.id).exec(txn).await?;
                } else {
                    let mut active: alert_rule::ActiveModel = rule.into();
                    active.server_ids_json = Set(Some(new_json));
                    active.update(txn).await?;
                }
            }
        }
    }

    // maintenances: delete if empty
    for m in maintenance::Entity::find().all(txn).await? {
        if let Some(ref json) = m.server_ids_json {
            if let Some(new_json) = remove_ids_from_json(json, orphan_ids) {
                if new_json == "[]" {
                    maintenance::Entity::delete_by_id(&m.id).exec(txn).await?;
                } else {
                    let mut active: maintenance::ActiveModel = m.into();
                    active.server_ids_json = Set(Some(new_json));
                    active.update(txn).await?;
                }
            }
        }
    }

    // service_monitors: set to NULL if empty (preserve monitor + history)
    for monitor in service_monitor::Entity::find().all(txn).await? {
        if let Some(ref json) = monitor.server_ids_json {
            if let Some(new_json) = remove_ids_from_json(json, orphan_ids) {
                let mut active: service_monitor::ActiveModel = monitor.into();
                if new_json == "[]" {
                    active.server_ids_json = Set(None);
                } else {
                    active.server_ids_json = Set(Some(new_json));
                }
                active.update(txn).await?;
            }
        }
    }

    // incidents: keep row, just update array
    for inc in incident::Entity::find().all(txn).await? {
        if let Some(ref json) = inc.server_ids_json {
            if let Some(new_json) = remove_ids_from_json(json, orphan_ids) {
                let mut active: incident::ActiveModel = inc.into();
                active.server_ids_json = Set(Some(new_json));
                active.update(txn).await?;
            }
        }
    }

    // status_pages: keep row, just update array
    for page in status_page::Entity::find().all(txn).await? {
        if let Some(new_json) = remove_ids_from_json(&page.server_ids_json, orphan_ids) {
            let mut active: status_page::ActiveModel = page.into();
            active.server_ids_json = Set(new_json);
            active.update(txn).await?;
        }
    }

    Ok(())
}

fn remove_ids_from_json(json: &str, orphan_ids: &[String]) -> Option<String> {
    let ids: Vec<String> = serde_json::from_str(json).unwrap_or_default();
    let filtered: Vec<&String> = ids.iter().filter(|id| !orphan_ids.contains(id)).collect();
    if filtered.len() == ids.len() {
        return None;
    }
    Some(serde_json::to_string(&filtered).unwrap_or_else(|_| "[]".to_string()))
}

#[cfg(test)]
mod upgrade_tests {
    use super::*;

    #[test]
    fn test_map_os() {
        assert_eq!(map_os("Linux 5.15.0-123-generic"), Some("linux"));
        assert_eq!(map_os("macOS 14.1.2 23B92 arm64"), Some("darwin"));
        assert_eq!(map_os("Mac OS X 13.0"), Some("darwin"));
        assert_eq!(map_os("Windows 10 Pro 22H2"), Some("windows"));
        assert_eq!(map_os("FreeBSD 13.2"), None);
    }

    #[test]
    fn test_map_arch() {
        assert_eq!(map_arch("x86_64"), Some("amd64"));
        assert_eq!(map_arch("aarch64"), Some("arm64"));
        assert_eq!(map_arch("arm"), None);
    }

    #[test]
    fn test_normalize_version() {
        assert_eq!(normalize_version("v0.7.1"), "0.7.1");
        assert_eq!(normalize_version("0.7.1"), "0.7.1");
        assert_eq!(normalize_version("v1.0.0"), "1.0.0");
    }
}
