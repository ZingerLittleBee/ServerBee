use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};

use crate::router::utils::extract_client_ip;
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{agent_enrollment, server, server_tag};
use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::api::network_probe::{
    get_server_network_anomalies, get_server_network_records, get_server_network_summary,
    get_server_network_targets,
};
use crate::service::agent_manager::AgentManager;
use crate::service::audit::AuditService;
use crate::service::enrollment::{DEFAULT_TTL_SECS, EnrollmentService};
use crate::service::ip_quality::IpQualityService;
use crate::service::network_probe::NetworkProbeService;
use crate::service::ping::PingService;
use crate::service::record::{QueryHistoryResult, RecordService};
use crate::service::server::{ServerService, UpdateServerInput};
use crate::service::server_tag as server_tag_service;
use crate::service::upgrade_tracker::{StartUpgradeJobError, UpgradeLookup};
use crate::state::AppState;
use serverbee_common::constants::effective_capabilities;
use serverbee_common::protocol::{BrowserMessage, ServerMessage};
use serverbee_common::types::NetworkProbeTarget;

const DEFAULT_SERVER_NAME: &str = "New Server";

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
    pub agent_local_capabilities: Option<i32>,
    pub effective_capabilities: Option<i32>,
    pub protocol_version: i32,
    features: Vec<String>,
    /// `true` iff the server row has a non-NULL `token_hash`. Pending servers
    /// (created via `POST /api/servers` but not yet enrolled by an agent) have
    /// `has_token = false`; the UI uses this to render a "pending" badge.
    pub has_token: bool,
    /// The single outstanding (not consumed, not revoked) bound enrollment for
    /// this server, if any. Plaintext code is intentionally NOT included — it
    /// is only returned at mint time. The UI uses this to surface a "show
    /// install command" button on pending or recovering servers.
    pub outstanding_enrollment: Option<OutstandingEnrollmentSummary>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// Outstanding-enrollment summary returned alongside a `ServerResponse` so the
/// UI can render pending state and offer the install command without a second
/// fetch. The plaintext code is only ever returned by the mint endpoints —
/// never here.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OutstandingEnrollmentSummary {
    pub id: String,
    pub code_prefix: String,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct CreateServerRequest {
    pub name: String,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub remark: Option<String>,
    #[serde(default)]
    pub public_remark: Option<String>,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub billing_cycle: Option<String>,
    #[serde(default)]
    pub billing_start_day: Option<i32>,
    #[serde(default)]
    pub expired_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub traffic_limit: Option<i64>,
    #[serde(default)]
    pub traffic_limit_type: Option<String>,
    /// Capabilities to encode into the install.sh `--caps` arg only; not
    /// persisted on the server row (which always uses `CAP_DEFAULT`).
    #[serde(default)]
    pub caps: Option<Vec<String>>,
    /// Defaults to 600 (10 min) per spec.
    #[serde(default)]
    pub ttl_secs: Option<i64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EnrollmentIssueResponse {
    pub id: String,
    /// Plaintext enrollment code — shown exactly once at mint time. The UI
    /// must surface this to the operator and warn that it cannot be recovered.
    pub code: String,
    pub code_prefix: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreateServerResponse {
    pub server_id: String,
    pub enrollment: EnrollmentIssueResponse,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct RecoverRequest {
    /// If `true`, clear the server's `token_hash`/`token_prefix` and kick the
    /// currently connected agent WebSocket as part of the same transaction.
    /// Use this when the operator suspects the existing agent token has been
    /// compromised. If `false`, the existing token remains valid and only a
    /// new bound enrollment is minted alongside it.
    pub revoke_immediately: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RecoverResponse {
    pub enrollment: EnrollmentIssueResponse,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct RegenerateCodeRequest {
    /// Optimistic concurrency token. If `Some`, must match the current
    /// outstanding enrollment id exactly; otherwise the server returns 409.
    /// If `None`, last-writer-wins: any outstanding enrollment is revoked
    /// and a fresh one is minted.
    #[serde(default)]
    pub expected_enrollment_id: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RegenerateCodeResponse {
    pub enrollment: EnrollmentIssueResponse,
}

fn runtime_capability_fields(
    agent_manager: &AgentManager,
    server_id: &str,
) -> (Option<i32>, Option<i32>) {
    (
        agent_manager
            .get_agent_local_capabilities(server_id)
            .map(|caps| caps as i32),
        agent_manager
            .get_effective_capabilities(server_id)
            .map(|caps| caps as i32),
    )
}

fn build_server_response(
    s: server::Model,
    agent_manager: &AgentManager,
    outstanding_enrollment: Option<OutstandingEnrollmentSummary>,
) -> ServerResponse {
    let (agent_local_capabilities, effective_capabilities) =
        runtime_capability_fields(agent_manager, &s.id);

    let has_token = s.token_hash.is_some();

    ServerResponse {
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
        agent_local_capabilities,
        effective_capabilities,
        protocol_version: s.protocol_version,
        features: serde_json::from_str(&s.features).unwrap_or_default(),
        has_token,
        outstanding_enrollment,
        created_at: s.created_at,
        updated_at: s.updated_at,
    }
}

/// Fetch the single outstanding (not consumed, not revoked) enrollment for a
/// server, mapped to the response DTO. Returns `Ok(None)` when there is no
/// outstanding enrollment.
async fn fetch_outstanding_enrollment(
    db: &sea_orm::DatabaseConnection,
    server_id: &str,
) -> Result<Option<OutstandingEnrollmentSummary>, AppError> {
    let row = agent_enrollment::Entity::find()
        .filter(agent_enrollment::Column::TargetServerId.eq(server_id))
        .filter(agent_enrollment::Column::ConsumedAt.is_null())
        .filter(agent_enrollment::Column::RevokedAt.is_null())
        .one(db)
        .await?;
    Ok(row.map(|m| OutstandingEnrollmentSummary {
        id: m.id,
        code_prefix: m.code_prefix,
        expires_at: m.expires_at.to_rfc3339(),
        created_at: m.created_at.to_rfc3339(),
    }))
}

/// Batch fetch of outstanding enrollments for a set of server ids. Avoids
/// the N+1 pattern when serializing the `GET /api/servers` list. Returns a
/// map keyed by `target_server_id`.
async fn fetch_outstanding_enrollments_batch(
    db: &sea_orm::DatabaseConnection,
    server_ids: &[String],
) -> Result<std::collections::HashMap<String, OutstandingEnrollmentSummary>, AppError> {
    let mut out = std::collections::HashMap::new();
    if server_ids.is_empty() {
        return Ok(out);
    }
    let rows = agent_enrollment::Entity::find()
        .filter(agent_enrollment::Column::TargetServerId.is_in(server_ids.iter().cloned()))
        .filter(agent_enrollment::Column::ConsumedAt.is_null())
        .filter(agent_enrollment::Column::RevokedAt.is_null())
        .all(db)
        .await?;
    for m in rows {
        // The partial unique index `idx_enrollments_active_per_server`
        // guarantees at most one outstanding row per server, so the last-write
        // wins behavior here is fine (and unreachable in practice).
        out.insert(
            m.target_server_id.clone(),
            OutstandingEnrollmentSummary {
                id: m.id,
                code_prefix: m.code_prefix,
                expires_at: m.expires_at.to_rfc3339(),
                created_at: m.created_at.to_rfc3339(),
            },
        );
    }
    Ok(out)
}

fn capability_change_message(
    agent_manager: &AgentManager,
    server_id: &str,
    capabilities: u32,
) -> BrowserMessage {
    let agent_local_capabilities = agent_manager.get_agent_local_capabilities(server_id);
    let effective_capabilities =
        agent_local_capabilities.map(|bits| effective_capabilities(capabilities, bits));

    BrowserMessage::CapabilitiesChanged {
        server_id: server_id.to_string(),
        capabilities,
        agent_local_capabilities,
        effective_capabilities,
    }
}

/// Send `IpQualitySync` to a single online agent.
///
/// Called when a server newly gains `CAP_IP_QUALITY` so the agent receives the
/// service catalog and check interval without waiting for a reconnect (spec §4
/// requires `IpQualitySync` on connect, on catalog change, and on capability
/// change). A failed DB read here is logged and ignored — the agent will still
/// receive the sync on its next reconnect.
async fn send_ip_quality_sync_to_agent(state: &Arc<AppState>, server_id: &str) {
    let Some(tx) = state.agent_manager.get_sender(server_id) else {
        return;
    };

    let services = match IpQualityService::enabled_service_defs(&state.db).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("IpQualitySync skipped for {server_id}: {e}");
            return;
        }
    };
    let setting = match IpQualityService::get_setting(&state.db).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("IpQualitySync skipped for {server_id}: {e}");
            return;
        }
    };

    let _ = tx
        .send(ServerMessage::IpQualitySync {
            services,
            interval_hours: setting.check_interval_hours as u32,
        })
        .await;
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
        .route("/servers", post(create_server))
        .route("/servers/{id}", put(update_server))
        .route("/servers/{id}", delete(delete_server))
        .route("/servers/batch-delete", post(batch_delete))
        .route("/servers/cleanup", delete(cleanup_orphaned_servers))
        .route(
            "/servers/batch-capabilities",
            put(batch_update_capabilities),
        )
        .route("/servers/{id}/upgrade", post(trigger_upgrade))
        .route("/servers/{id}/recover", post(recover_server))
        .route(
            "/servers/{id}/regenerate-code",
            post(regenerate_code),
        )
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
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn list_servers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<ServerResponse>>>, AppError> {
    let servers = ServerService::list_servers(&state.db).await?;
    let ids: Vec<String> = servers.iter().map(|s| s.id.clone()).collect();
    let mut outstanding = fetch_outstanding_enrollments_batch(&state.db, &ids).await?;
    ok(servers
        .into_iter()
        .map(|server| {
            let pending = outstanding.remove(&server.id);
            build_server_response(server, &state.agent_manager, pending)
        })
        .collect())
}

/// Create a pending server row and a server-bound enrollment in a single
/// transaction. The server row is inserted with `token_hash = NULL` (pending),
/// `capabilities = CAP_DEFAULT`, and `protocol_version = 1`. The operator-
/// supplied `tags` are persisted in `server_tags`, and the global default
/// network probe targets are applied to the new server. The returned plaintext
/// enrollment `code` is shown exactly once — the install command on the agent
/// will consume it via `POST /api/agent/register`.
///
/// `caps` is accepted in the request for the install.sh `--caps` arg but is
/// NOT persisted on the server row. The server row always starts at
/// `CAP_DEFAULT`; the operator can edit capabilities afterwards.
#[utoipa::path(
    post,
    path = "/api/servers",
    tag = "servers",
    request_body = CreateServerRequest,
    responses(
        (status = 200, description = "Server created (pending) and bound enrollment minted", body = CreateServerResponse),
        (status = 400, description = "Validation error or max_servers cap reached"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn create_server(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Json(body): Json<CreateServerRequest>,
) -> Result<Json<ApiResponse<CreateServerResponse>>, AppError> {
    use serverbee_common::constants::CAP_DEFAULT;

    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }

    // Validate tags up-front so we fail before opening the tx. Reuses the
    // shared validator so the rules stay identical to PUT /api/servers/{id}/tags.
    let normalized_tags = server_tag_service::validate_tags(&body.tags)?;

    // Soft max_servers cap. `max_servers == 0` means "no cap" per AuthConfig
    // default; only fire the pre-check when an actual limit is configured.
    let max_servers = state.config.auth.max_servers;
    if max_servers > 0 {
        let count = server::Entity::find().count(&state.db).await?;
        if count >= max_servers as u64 {
            return Err(AppError::BadRequest(format!(
                "Server limit reached ({max_servers}). Delete unused servers or increase max_servers in config."
            )));
        }
    }

    // Fetch default probe targets BEFORE the tx — ConfigService::get_typed
    // takes &DatabaseConnection, not a generic conn. The targets array is
    // small and stable, so reading it outside the tx is fine.
    let probe_setting = NetworkProbeService::get_setting(&state.db).await?;
    let default_target_ids = probe_setting.default_target_ids.clone();

    let ttl = body.ttl_secs.unwrap_or(DEFAULT_TTL_SECS);
    let server_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let user_id = current_user.user_id.clone();
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();

    let tx_server_id = server_id.clone();
    let tx_user_id = user_id.clone();
    let tx_name = name.clone();
    let tx_tags = normalized_tags.clone();
    let tx_body = body.clone();

    let (enrollment_model, plaintext_code) = state
        .db
        .transaction::<_, (agent_enrollment::Model, String), AppError>(move |tx| {
            Box::pin(async move {
                // 1. Insert the pending server row. token_hash = None marks
                //    the row as "pending" until the agent enrolls.
                server::ActiveModel {
                    id: Set(tx_server_id.clone()),
                    token_hash: Set(None),
                    token_prefix: Set(None),
                    name: Set(tx_name),
                    cpu_name: Set(None),
                    cpu_cores: Set(None),
                    cpu_arch: Set(None),
                    os: Set(None),
                    kernel_version: Set(None),
                    mem_total: Set(None),
                    swap_total: Set(None),
                    disk_total: Set(None),
                    ipv4: Set(None),
                    ipv6: Set(None),
                    region: Set(None),
                    country_code: Set(None),
                    virtualization: Set(None),
                    agent_version: Set(None),
                    group_id: Set(tx_body.group_id.clone()),
                    weight: Set(0),
                    hidden: Set(false),
                    remark: Set(tx_body.remark.clone()),
                    public_remark: Set(tx_body.public_remark.clone()),
                    price: Set(tx_body.price),
                    billing_cycle: Set(tx_body.billing_cycle.clone()),
                    currency: Set(tx_body.currency.clone()),
                    expired_at: Set(tx_body.expired_at),
                    traffic_limit: Set(tx_body.traffic_limit),
                    traffic_limit_type: Set(tx_body.traffic_limit_type.clone()),
                    billing_start_day: Set(tx_body.billing_start_day),
                    capabilities: Set(CAP_DEFAULT as i32),
                    protocol_version: Set(1),
                    features: Set("[]".to_string()),
                    last_remote_addr: Set(None),
                    fingerprint: Set(None),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(tx)
                .await?;

                // 2. Persist operator-supplied tags.
                for tag in &tx_tags {
                    server_tag::ActiveModel {
                        server_id: Set(tx_server_id.clone()),
                        tag: Set(tag.clone()),
                    }
                    .insert(tx)
                    .await?;
                }

                // 3. Apply default network probe targets inside the same tx
                //    so a failure rolls back the server row too.
                NetworkProbeService::apply_defaults_tx(
                    tx,
                    &tx_server_id,
                    &default_target_ids,
                )
                .await?;

                // 4. Mint the bound enrollment. The partial unique index
                //    `idx_enrollments_active_per_server` makes this atomic
                //    with the server insert: if two `POST /api/servers`
                //    requests raced on the same id (impossible — UUID), the
                //    second would also fail. With unique UUIDs the only way
                //    this errors is downstream of bad input, in which case
                //    we want the whole tx to roll back.
                let (model, plaintext) = EnrollmentService::mint_for_server(
                    tx,
                    &tx_server_id,
                    &tx_user_id,
                    ttl,
                )
                .await?;

                Ok((model, plaintext))
            })
        })
        .await
        .map_err(|e| match e {
            sea_orm::TransactionError::Connection(db_err) => AppError::from(db_err),
            sea_orm::TransactionError::Transaction(app_err) => app_err,
        })?;

    // Audit log AFTER commit so we don't log fictitious creations on rollback.
    let _ = AuditService::log(
        &state.db,
        &user_id,
        "server_created",
        Some(&format!(
            "server_id={server_id} enrollment={} prefix={}",
            enrollment_model.id, enrollment_model.code_prefix
        )),
        &ip,
    )
    .await;

    ok(CreateServerResponse {
        server_id,
        enrollment: EnrollmentIssueResponse {
            id: enrollment_model.id,
            code: plaintext_code,
            code_prefix: enrollment_model.code_prefix,
            expires_at: enrollment_model.expires_at.to_rfc3339(),
        },
    })
}

/// Mint a fresh bound enrollment for an already-enrolled server so the operator
/// can reinstall the agent. The target server MUST already have a token
/// (`token_hash IS NOT NULL`) — recover on a pending server is rejected with
/// `400`, use `regenerate-code` for that path.
///
/// Recover NEVER auto-supersedes an outstanding enrollment: if one is still
/// active, this returns `409` and the operator is expected to either wait for
/// it to expire or revoke it first. Only `regenerate-code` auto-supersedes.
///
/// `revoke_immediately`:
/// - `true` — clear `token_hash`/`token_prefix` inside the same transaction
///   and kick the currently connected agent WS after commit. The server
///   returns to pending until the new code is consumed.
/// - `false` — the existing token stays valid; the new code only becomes
///   active once the agent registers with it (`verify_and_consume_tx` then
///   rotates the token via `mint_token_for_server`).
#[utoipa::path(
    post,
    path = "/api/servers/{id}/recover",
    tag = "servers",
    params(("id" = String, Path, description = "Server ID")),
    request_body = RecoverRequest,
    responses(
        (status = 200, description = "Recover enrollment minted", body = RecoverResponse),
        (status = 400, description = "Server is pending (use regenerate-code instead)"),
        (status = 404, description = "Server not found"),
        (status = 409, description = "Outstanding enrollment exists; revoke it first"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn recover_server(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<RecoverRequest>,
) -> Result<Json<ApiResponse<RecoverResponse>>, AppError> {
    let user_id = current_user.user_id.clone();
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();

    let tx_id = id.clone();
    let tx_user_id = user_id.clone();
    let revoke = body.revoke_immediately;

    let (enrollment_model, plaintext_code, kicked) = state
        .db
        .transaction::<_, (agent_enrollment::Model, String, bool), AppError>(move |tx| {
            Box::pin(async move {
                // 1. Load the server row; 404 if it doesn't exist.
                let row = server::Entity::find_by_id(&tx_id)
                    .one(tx)
                    .await?
                    .ok_or_else(|| AppError::NotFound("server not found".into()))?;

                // 2. Recover is only for already-enrolled servers. A pending
                //    server (token_hash IS NULL) should use regenerate-code.
                if row.token_hash.is_none() {
                    return Err(AppError::BadRequest(
                        "server is pending; use regenerate-code instead".into(),
                    ));
                }

                // 3. Recover NEVER auto-supersedes an outstanding enrollment.
                //    The partial unique index `idx_enrollments_active_per_server`
                //    would also reject the mint below, but checking first lets
                //    us return a precise 409 instead of a generic constraint
                //    error.
                let outstanding = agent_enrollment::Entity::find()
                    .filter(agent_enrollment::Column::TargetServerId.eq(&tx_id))
                    .filter(agent_enrollment::Column::ConsumedAt.is_null())
                    .filter(agent_enrollment::Column::RevokedAt.is_null())
                    .one(tx)
                    .await?;
                if outstanding.is_some() {
                    return Err(AppError::Conflict(
                        "an outstanding enrollment exists; revoke it before recovering".into(),
                    ));
                }

                // 4. Optionally clear the server token inside the same tx.
                let kicked = if revoke {
                    let mut active: server::ActiveModel = row.into();
                    active.token_hash = Set(None);
                    active.token_prefix = Set(None);
                    active.updated_at = Set(Utc::now());
                    active.update(tx).await?;
                    true
                } else {
                    false
                };

                // 5. Mint the new bound enrollment.
                let (model, plaintext) =
                    EnrollmentService::mint_for_server(tx, &tx_id, &tx_user_id, DEFAULT_TTL_SECS)
                        .await?;

                Ok((model, plaintext, kicked))
            })
        })
        .await
        .map_err(|e| match e {
            sea_orm::TransactionError::Connection(db_err) => AppError::from(db_err),
            sea_orm::TransactionError::Transaction(app_err) => app_err,
        })?;

    // Post-commit side effects.
    if kicked {
        // Drop the agent WS connection; the agent will reconnect, see its
        // token has been cleared, and exit/back off. Operator then runs the
        // install command with the new code.
        state.agent_manager.remove_connection(&id);
    }

    let _ = AuditService::log(
        &state.db,
        &user_id,
        "server_recover",
        Some(&format!(
            "server_id={id} enrollment={} prefix={} revoke_immediately={}",
            enrollment_model.id, enrollment_model.code_prefix, kicked
        )),
        &ip,
    )
    .await;

    ok(RecoverResponse {
        enrollment: EnrollmentIssueResponse {
            id: enrollment_model.id,
            code: plaintext_code,
            code_prefix: enrollment_model.code_prefix,
            expires_at: enrollment_model.expires_at.to_rfc3339(),
        },
    })
}

/// Mint a fresh bound enrollment for a pending server, auto-superseding the
/// previous outstanding enrollment (if any) inside one transaction. The target
/// server MUST be pending (`token_hash IS NULL`); use `recover` for an already-
/// enrolled server.
///
/// Optimistic concurrency: callers pass `expected_enrollment_id` to guard
/// against stomping on a concurrent operator's regenerated code. Semantics:
/// - `Some(id) && matches current outstanding` → proceed (CAS pass)
/// - `Some(id) && does NOT match` (including: there is no outstanding row, or
///   the row referenced has been revoked/consumed) → 409
/// - `None && outstanding exists` → proceed (last-writer-wins)
/// - `None && no outstanding` → proceed (fresh mint)
#[utoipa::path(
    post,
    path = "/api/servers/{id}/regenerate-code",
    tag = "servers",
    params(("id" = String, Path, description = "Server ID")),
    request_body = RegenerateCodeRequest,
    responses(
        (status = 200, description = "Regenerate enrollment minted", body = RegenerateCodeResponse),
        (status = 400, description = "Server is not pending; use recover instead"),
        (status = 404, description = "Server not found"),
        (status = 409, description = "expected_enrollment_id mismatch"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn regenerate_code(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<RegenerateCodeRequest>,
) -> Result<Json<ApiResponse<RegenerateCodeResponse>>, AppError> {
    let user_id = current_user.user_id.clone();
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();

    let tx_id = id.clone();
    let tx_user_id = user_id.clone();
    let expected = body.expected_enrollment_id.clone();

    let (enrollment_model, plaintext_code) = state
        .db
        .transaction::<_, (agent_enrollment::Model, String), AppError>(move |tx| {
            Box::pin(async move {
                // 1. Load the server row; 404 if it doesn't exist.
                let row = server::Entity::find_by_id(&tx_id)
                    .one(tx)
                    .await?
                    .ok_or_else(|| AppError::NotFound("server not found".into()))?;

                // 2. regenerate-code is only for pending servers. An already-
                //    enrolled server (token_hash IS NOT NULL) must use recover.
                if row.token_hash.is_some() {
                    return Err(AppError::BadRequest(
                        "server is not pending; use recover instead".into(),
                    ));
                }

                // 3. Optimistic CAS: if caller provided expected_enrollment_id,
                //    it must match the current OUTSTANDING enrollment exactly.
                //    A None value means "I don't care what's outstanding"
                //    (last-writer-wins).
                let current = agent_enrollment::Entity::find()
                    .filter(agent_enrollment::Column::TargetServerId.eq(&tx_id))
                    .filter(agent_enrollment::Column::ConsumedAt.is_null())
                    .filter(agent_enrollment::Column::RevokedAt.is_null())
                    .one(tx)
                    .await?;
                let current_id = current.as_ref().map(|m| m.id.clone());
                if expected.is_some() && expected != current_id {
                    return Err(AppError::Conflict(
                        "expected_enrollment_id mismatch".into(),
                    ));
                }

                // 4. Revoke any outstanding row, then mint a fresh one.
                EnrollmentService::revoke_outstanding_tx(tx, &tx_id).await?;
                let (model, plaintext) =
                    EnrollmentService::mint_for_server(tx, &tx_id, &tx_user_id, DEFAULT_TTL_SECS)
                        .await?;
                Ok((model, plaintext))
            })
        })
        .await
        .map_err(|e| match e {
            sea_orm::TransactionError::Connection(db_err) => AppError::from(db_err),
            sea_orm::TransactionError::Transaction(app_err) => app_err,
        })?;

    let _ = AuditService::log(
        &state.db,
        &user_id,
        "server_regenerate_code",
        Some(&format!(
            "server_id={id} enrollment={} prefix={}",
            enrollment_model.id, enrollment_model.code_prefix
        )),
        &ip,
    )
    .await;

    ok(RegenerateCodeResponse {
        enrollment: EnrollmentIssueResponse {
            id: enrollment_model.id,
            code: plaintext_code,
            code_prefix: enrollment_model.code_prefix,
            expires_at: enrollment_model.expires_at.to_rfc3339(),
        },
    })
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
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ServerResponse>>, AppError> {
    let server = ServerService::get_server(&state.db, &id).await?;
    let outstanding = fetch_outstanding_enrollment(&state.db, &id).await?;
    ok(build_server_response(server, &state.agent_manager, outstanding))
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
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
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
        CAP_DOCKER, CAP_FIREWALL_BLOCK, CAP_IP_QUALITY, CAP_PING_HTTP, CAP_PING_ICMP, CAP_PING_TCP,
        has_capability,
    };
    use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;
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
        state
            .agent_manager
            .update_server_capabilities(&id, new_caps);

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
            .broadcast_browser(capability_change_message(
                &state.agent_manager,
                &id,
                new_caps,
            ));

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

        // Firewall capability transitioned — sync the agent's view to ours.
        let was_fw = has_capability(old, CAP_FIREWALL_BLOCK);
        let now_fw = has_capability(new_caps, CAP_FIREWALL_BLOCK);
        if was_fw != now_fw
            && let Some(pv) = state.agent_manager.get_protocol_version(&id)
            && pv >= FIREWALL_MIN_PROTOCOL
        {
            if now_fw {
                if let Err(e) = state
                    .firewall
                    .push_sync_to(&id, &state.agent_manager)
                    .await
                {
                    tracing::warn!(server_id = %id, error = %e, "firewall sync push failed");
                }
            } else {
                state
                    .firewall
                    .push_reset_to(&id, &state.agent_manager)
                    .await;
            }
        }

        // IP quality capability newly gained — re-send IpQualitySync so the
        // agent receives the service catalog without waiting for a reconnect.
        if !has_capability(old, CAP_IP_QUALITY) && has_capability(new_caps, CAP_IP_QUALITY) {
            send_ip_quality_sync_to_agent(&state, &id).await;
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

    let outstanding = fetch_outstanding_enrollment(&state.db, &id).await?;
    ok(build_server_response(server, &state.agent_manager, outstanding))
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
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn delete_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    ServerService::delete_server(&state.db, &id).await?;
    // Close any live agent connection so it doesn't linger after the row is gone.
    state.agent_manager.remove_connection(&id);
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
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn batch_delete(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BatchDeleteRequest>,
) -> Result<Json<ApiResponse<BatchDeleteResponse>>, AppError> {
    let deleted = ServerService::batch_delete(&state.db, &body.ids).await?;
    // Kick any live connections for the requested ids. `remove_connection` is
    // a no-op when nothing is connected, so it's safe to call for ids that
    // weren't actually deleted (e.g. unknown ids in the request).
    for id in &body.ids {
        state.agent_manager.remove_connection(id);
    }
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
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
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
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
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
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
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

    let sender = state
        .agent_manager
        .get_sender(&id)
        .ok_or_else(|| AppError::NotFound("Agent not connected".into()))?;

    let job = state
        .upgrade_tracker
        .start_job(&id, version.to_string())
        .map_err(|error| match error {
            StartUpgradeJobError::Conflict(existing) => AppError::Conflict(format!(
                "Upgrade already running for server {} (job_id={}, target_version={})",
                existing.server_id, existing.job_id, existing.target_version
            )),
        })?;

    let msg = ServerMessage::Upgrade {
        version: version.to_string(),
        download_url: String::new(),
        sha256: String::new(),
        job_id: Some(job.job_id.clone()),
    };
    if let Err(_send_error) = sender.send(msg).await {
        state.upgrade_tracker.mark_failed(
            UpgradeLookup::from_job(&job),
            job.stage,
            "Failed to send upgrade command".into(),
            None,
        );
        return Err(AppError::Internal("Failed to send upgrade command".into()));
    }

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
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
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

        state
            .agent_manager
            .update_server_capabilities(server_id, new_caps);

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
            .broadcast_browser(capability_change_message(
                &state.agent_manager,
                server_id,
                new_caps,
            ));

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

        // Firewall capability transitioned — sync the agent's view to ours.
        let was_fw = has_capability(old_caps, CAP_FIREWALL_BLOCK);
        let now_fw = has_capability(new_caps, CAP_FIREWALL_BLOCK);
        if was_fw != now_fw
            && let Some(pv) = state.agent_manager.get_protocol_version(server_id)
            && pv >= serverbee_common::firewall::FIREWALL_MIN_PROTOCOL
        {
            if now_fw {
                if let Err(e) = state
                    .firewall
                    .push_sync_to(server_id, &state.agent_manager)
                    .await
                {
                    tracing::warn!(server_id, error = %e, "firewall sync push failed");
                }
            } else {
                state
                    .firewall
                    .push_reset_to(server_id, &state.agent_manager)
                    .await;
            }
        }

        // IP quality capability newly gained — re-send IpQualitySync so the
        // agent receives the service catalog without waiting for a reconnect.
        if !has_capability(old_caps, CAP_IP_QUALITY)
            && has_capability(new_caps, CAP_IP_QUALITY)
        {
            send_ip_quality_sync_to_agent(&state, server_id).await;
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
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
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

    let txn = state.db.begin().await?;

    let candidates = server::Entity::find()
        .filter(server::Column::Name.eq("New Server"))
        .filter(server::Column::Os.is_null())
        .all(&txn)
        .await?;

    let orphan_ids = collect_orphan_server_ids(&candidates, |id| state.agent_manager.is_online(id));
    if orphan_ids.is_empty() {
        return ok(CleanupResponse { deleted_count: 0 });
    }

    // Purge all server_id-scoped rows through the shared service helper so
    // this path cannot drift from delete_server again.
    ServerService::delete_server_scoped_rows(&txn, &orphan_ids).await?;
    server_tag::Entity::delete_many()
        .filter(server_tag::Column::ServerId.is_in(&orphan_ids))
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
        if let Some(ref json) = rule.server_ids_json
            && let Some(new_json) = remove_ids_from_json(json, orphan_ids)
        {
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

    // maintenances: delete if empty
    for m in maintenance::Entity::find().all(txn).await? {
        if let Some(ref json) = m.server_ids_json
            && let Some(new_json) = remove_ids_from_json(json, orphan_ids)
        {
            if new_json == "[]" {
                maintenance::Entity::delete_by_id(&m.id).exec(txn).await?;
            } else {
                let mut active: maintenance::ActiveModel = m.into();
                active.server_ids_json = Set(Some(new_json));
                active.update(txn).await?;
            }
        }
    }

    // service_monitors: set to NULL if empty (preserve monitor + history)
    for monitor in service_monitor::Entity::find().all(txn).await? {
        if let Some(ref json) = monitor.server_ids_json
            && let Some(new_json) = remove_ids_from_json(json, orphan_ids)
        {
            let mut active: service_monitor::ActiveModel = monitor.into();
            if new_json == "[]" {
                active.server_ids_json = Set(None);
            } else {
                active.server_ids_json = Set(Some(new_json));
            }
            active.update(txn).await?;
        }
    }

    // incidents: keep row, just update array
    for inc in incident::Entity::find().all(txn).await? {
        if let Some(ref json) = inc.server_ids_json
            && let Some(new_json) = remove_ids_from_json(json, orphan_ids)
        {
            let mut active: incident::ActiveModel = inc.into();
            active.server_ids_json = Set(Some(new_json));
            active.update(txn).await?;
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

fn collect_orphan_server_ids<F>(servers: &[server::Model], is_online: F) -> Vec<String>
where
    F: Fn(&str) -> bool,
{
    servers
        .iter()
        .filter(|server| {
            server.name == DEFAULT_SERVER_NAME && server.os.is_none() && !is_online(&server.id)
        })
        .map(|server| server.id.clone())
        .collect()
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
mod cleanup_tests {
    use super::{DEFAULT_SERVER_NAME, collect_orphan_server_ids, remove_ids_from_json};
    use crate::entity::server;
    use chrono::Utc;
    use serverbee_common::constants::CAP_DEFAULT;
    use std::collections::HashSet;

    fn make_server(id: &str, name: &str, os: Option<&str>) -> server::Model {
        let now = Utc::now();
        server::Model {
            id: id.to_string(),
            token_hash: Some("hash".to_string()),
            token_prefix: Some("prefix".to_string()),
            name: name.to_string(),
            cpu_name: None,
            cpu_cores: None,
            cpu_arch: None,
            os: os.map(str::to_string),
            kernel_version: None,
            mem_total: None,
            swap_total: None,
            disk_total: None,
            ipv4: None,
            ipv6: None,
            region: None,
            country_code: None,
            virtualization: None,
            agent_version: None,
            group_id: None,
            weight: 0,
            hidden: false,
            remark: None,
            public_remark: None,
            price: None,
            billing_cycle: None,
            currency: None,
            expired_at: None,
            traffic_limit: None,
            traffic_limit_type: None,
            billing_start_day: None,
            capabilities: CAP_DEFAULT as i32,
            protocol_version: 1,
            features: "[]".to_string(),
            last_remote_addr: None,
            fingerprint: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn test_no_match_returns_none() {
        assert_eq!(remove_ids_from_json(r#"["a","b"]"#, &["c".into()]), None);
    }

    #[test]
    fn test_partial_removal() {
        let result = remove_ids_from_json(r#"["a","b","c"]"#, &["b".into()]);
        assert_eq!(result, Some(r#"["a","c"]"#.to_string()));
    }

    #[test]
    fn test_remove_all() {
        let result = remove_ids_from_json(r#"["a"]"#, &["a".into()]);
        assert_eq!(result, Some("[]".to_string()));
    }

    #[test]
    fn test_empty_array() {
        assert_eq!(remove_ids_from_json("[]", &["a".into()]), None);
    }

    #[test]
    fn test_invalid_json() {
        assert_eq!(remove_ids_from_json("not json", &["a".into()]), None);
    }

    #[test]
    fn test_multiple_orphans() {
        let result = remove_ids_from_json(r#"["a","b","c","d"]"#, &["b".into(), "d".into()]);
        assert_eq!(result, Some(r#"["a","c"]"#.to_string()));
    }

    #[test]
    fn test_collect_orphan_server_ids_skips_online_servers() {
        let servers = vec![
            make_server("offline-orphan", DEFAULT_SERVER_NAME, None),
            make_server("online-orphan", DEFAULT_SERVER_NAME, None),
            make_server("initialized", DEFAULT_SERVER_NAME, Some("Linux")),
            make_server("renamed", "Production", None),
        ];
        let online_ids = HashSet::from([String::from("online-orphan")]);

        let orphans = collect_orphan_server_ids(&servers, |id| online_ids.contains(id));

        assert_eq!(orphans, vec![String::from("offline-orphan")]);
    }
}

#[cfg(test)]
mod upgrade_tests {
    use super::*;

    #[test]
    fn test_normalize_version() {
        assert_eq!(normalize_version("v0.7.1"), "0.7.1");
        assert_eq!(normalize_version("0.7.1"), "0.7.1");
        assert_eq!(normalize_version("v1.0.0"), "1.0.0");
    }
}
