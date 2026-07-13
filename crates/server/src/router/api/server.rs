use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Extension, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};

use crate::router::utils::extract_client_ip;
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait,
};
use serde::{Deserialize, Serialize};

use crate::entity::server;
use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::api::network_probe::{
    get_server_network_anomalies, get_server_network_records, get_server_network_summary,
    get_server_network_targets,
};
use crate::service::agent_manager::AgentManager;
use crate::service::agent_reconcile::AgentDesiredStateDomain;
use crate::service::audit::AuditService;
use crate::service::network_probe::NetworkProbeService;
use crate::service::record::{QueryHistoryResult, RecordService};
use crate::service::server::{ServerService, UpdateServerInput};
use crate::service::server_onboarding::{
    OnboardServer, OnboardingError, OnboardingRequestId, OnboardingResult, ServerProfile,
};
use crate::service::task_scheduler;
use crate::service::upgrade_tracker::{StartUpgradeJobError, UpgradeLookup};
use crate::state::AppState;
use serverbee_common::protocol::ServerMessage;
use serverbee_common::types::{
    AgentAuthorityStateSummary, AgentAuthorityStatus, OutstandingEnrollmentSummary,
};

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

/// A capability that is temporarily enabled on the agent host until
/// `expires_at`. Mirrors `serverbee_common::protocol::TemporaryGrant` but adds a
/// `ToSchema` derive so the REST `ServerResponse` can advertise it; the UI uses
/// it to render countdowns from a plain HTTP fetch.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct TemporaryGrantDto {
    pub cap: String,
    pub granted_at: i64,
    pub expires_at: i64,
}

impl From<serverbee_common::protocol::TemporaryGrant> for TemporaryGrantDto {
    fn from(g: serverbee_common::protocol::TemporaryGrant) -> Self {
        Self {
            cap: g.cap,
            granted_at: g.granted_at,
            expires_at: g.expires_at,
        }
    }
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
    /// `true` when `country_code`/`region` were pinned manually by an operator
    /// and are no longer auto-updated from GeoIP. The UI uses this to show that
    /// the flag is a manual override.
    geo_manual: bool,
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
    /// Currently-active temporary capability grants reported by the agent, used
    /// by the UI to render countdowns. Empty when the agent is offline or has no
    /// active grants.
    #[serde(default)]
    pub temporary: Vec<TemporaryGrantDto>,
    pub protocol_version: i32,
    features: Vec<String>,
    pub agent_authority: AgentAuthorityStateSummary,
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

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct CreateServerRequest {
    pub onboarding_request_id: String,
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
    pub replayed: bool,
    pub enrollment: Option<EnrollmentIssueResponse>,
    pub outstanding_offer: Option<OutstandingEnrollmentSummary>,
}

#[derive(Debug, Clone, Copy, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ReenrollmentModeRequest {
    Graceful,
    Emergency,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct ReenrollmentRequest {
    pub mode: ReenrollmentModeRequest,
    #[serde(default)]
    pub ttl_secs: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct IssueOfferRequest {
    #[serde(default)]
    pub ttl_secs: Option<i64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EnrollmentOfferResponse {
    pub enrollment: EnrollmentIssueResponse,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RevokeOfferResponse {
    pub offer_id: String,
    pub already_revoked: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RevokeAuthorityResponse {
    pub server_id: String,
    pub changed: bool,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct AuthorityHistoryQuery {
    pub server_id: String,
    #[serde(default = "default_authority_history_limit")]
    pub limit: u64,
}

fn default_authority_history_limit() -> u64 {
    100
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuthorityEventResponse {
    pub id: String,
    pub server_id: String,
    pub server_name: String,
    pub actor_kind: String,
    pub actor_id: Option<String>,
    pub request_source: String,
    pub offer_id: Option<String>,
    pub transition: String,
    pub mode: Option<String>,
    pub offer_outcome: Option<String>,
    pub authority_before: String,
    pub authority_after: String,
    pub created_at: DateTime<Utc>,
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
    agent_authority: AgentAuthorityStateSummary,
) -> ServerResponse {
    let (agent_local_capabilities, effective_capabilities) =
        runtime_capability_fields(agent_manager, &s.id);

    let temporary = agent_manager
        .get_temporary_grants(&s.id)
        .into_iter()
        .map(Into::into)
        .collect();

    let has_token = agent_authority.status == AgentAuthorityStatus::Claimed;
    let outstanding_enrollment = agent_authority.outstanding_offer.clone();

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
        geo_manual: s.geo_manual,
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
        temporary,
        protocol_version: s.protocol_version,
        features: serde_json::from_str(&s.features).unwrap_or_default(),
        agent_authority,
        has_token,
        outstanding_enrollment,
        created_at: s.created_at,
        updated_at: s.updated_at,
    }
}

async fn fetch_authority_states_batch(
    authority: &crate::service::agent_authority::AgentAuthority,
    server_ids: &[String],
) -> Result<std::collections::HashMap<String, AgentAuthorityStateSummary>, AppError> {
    let ids = server_ids
        .iter()
        .cloned()
        .map(|server_id| {
            crate::service::agent_authority::ServerId::parse(server_id)
                .map_err(|error| AppError::Internal(format!("invalid stored server id: {error}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    authority
        .states(&ids)
        .await
        .map_err(|error| match error {
            crate::service::agent_authority::StateError::NotFound => {
                AppError::Internal("batch authority projection lost a server".to_string())
            }
            crate::service::agent_authority::StateError::Store(error) => error,
        })
        .map(|states| {
            states
                .into_iter()
                .map(|state| {
                    (
                        state.server_id.as_str().to_string(),
                        authority_state_response(state),
                    )
                })
                .collect()
        })
}

/// GET endpoints accessible to all authenticated users (admin + member).
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/servers", get(list_servers))
        .route("/servers/{id}", get(get_server))
        .route("/servers/{id}/agent-authority", get(get_agent_authority))
        .route("/agent-authority/events", get(get_authority_history))
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
        .route("/servers/{id}/upgrade", post(trigger_upgrade))
        .route(
            "/servers/{id}/agent-authority/re-enrollment",
            post(begin_reenrollment),
        )
        .route(
            "/servers/{id}/agent-authority/offers",
            post(issue_offer_for_unclaimed),
        )
        .route(
            "/servers/{id}/agent-authority/offers/{offer_id}/replace",
            post(replace_offer),
        )
        .route(
            "/servers/{id}/agent-authority/offers/{offer_id}",
            delete(revoke_offer),
        )
        .route(
            "/servers/{id}/agent-authority",
            delete(revoke_agent_authority),
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
    let mut authority_states = fetch_authority_states_batch(&state.agent_authority, &ids).await?;
    let response = servers
        .into_iter()
        .map(|server| {
            let authority = authority_states.remove(&server.id).ok_or_else(|| {
                AppError::Internal(format!(
                    "Agent Authority projection missing Server {}",
                    server.id
                ))
            })?;
            Ok(build_server_response(
                server,
                &state.agent_manager,
                authority,
            ))
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    ok(response)
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
/// `CAP_DEFAULT`; the Agent later reports its locally configured capabilities.
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
    Extension(current_user): Extension<CurrentUser>,
    Json(body): Json<CreateServerRequest>,
) -> Result<Json<ApiResponse<CreateServerResponse>>, AppError> {
    let request_id =
        OnboardingRequestId::parse(body.onboarding_request_id).map_err(AppError::BadRequest)?;
    let offer_ttl = parse_offer_ttl(body.ttl_secs)?;
    let result = state
        .server_onboarding
        .onboard(OnboardServer {
            actor_id: current_user.user_id,
            request_id,
            source: request_source("api:create-server")?,
            profile: ServerProfile {
                name: body.name,
                group_id: body.group_id,
                tags: body.tags,
                remark: body.remark,
                public_remark: body.public_remark,
                price: body.price,
                currency: body.currency,
                billing_cycle: body.billing_cycle,
                billing_start_day: body.billing_start_day,
                expired_at: body.expired_at,
                traffic_limit: body.traffic_limit,
                traffic_limit_type: body.traffic_limit_type,
            },
            offer_ttl,
        })
        .await
        .map_err(map_onboarding_error)?;

    match result {
        OnboardingResult::Created {
            server_id,
            enrollment,
        } => ok(CreateServerResponse {
            server_id: server_id.into_inner(),
            replayed: false,
            enrollment: Some(enrollment_issue_response(enrollment)),
            outstanding_offer: None,
        }),
        OnboardingResult::Replayed {
            server_id,
            outstanding_offer,
        } => ok(CreateServerResponse {
            server_id: server_id.into_inner(),
            replayed: true,
            enrollment: None,
            outstanding_offer: outstanding_offer.map(outstanding_offer_response),
        }),
    }
}

fn request_source(value: &str) -> Result<crate::service::agent_authority::RequestSource, AppError> {
    crate::service::agent_authority::RequestSource::parse(value)
        .map_err(|error| AppError::Internal(format!("invalid request source: {error}")))
}

fn parse_offer_ttl(
    value: Option<i64>,
) -> Result<crate::service::agent_authority::OfferTtl, AppError> {
    crate::service::agent_authority::OfferTtl::seconds(
        value.unwrap_or(crate::service::agent_authority::OfferTtl::DEFAULT_SECONDS),
    )
    .map_err(AppError::BadRequest)
}

fn parse_server_id(value: String) -> Result<crate::service::agent_authority::ServerId, AppError> {
    crate::service::agent_authority::ServerId::parse(value).map_err(AppError::BadRequest)
}

fn parse_offer_id(value: String) -> Result<crate::service::agent_authority::OfferId, AppError> {
    crate::service::agent_authority::OfferId::parse(value).map_err(AppError::BadRequest)
}

fn authority_actor(user: &CurrentUser) -> crate::service::agent_authority::Actor {
    crate::service::agent_authority::Actor::User {
        id: user.user_id.clone(),
    }
}

fn enrollment_issue_response(
    enrollment: crate::service::agent_authority::IssuedOffer,
) -> EnrollmentIssueResponse {
    EnrollmentIssueResponse {
        id: enrollment.id.into_inner(),
        code: enrollment.code.expose().to_string(),
        code_prefix: enrollment.code_prefix,
        expires_at: enrollment.expires_at.to_rfc3339(),
    }
}

fn outstanding_offer_response(
    offer: crate::service::agent_authority::OutstandingOffer,
) -> OutstandingEnrollmentSummary {
    OutstandingEnrollmentSummary {
        id: offer.id.into_inner(),
        code_prefix: offer.code_prefix,
        expires_at: offer.expires_at.to_rfc3339(),
        created_at: offer.created_at.to_rfc3339(),
    }
}

fn authority_state_response(
    state: crate::service::agent_authority::AuthorityState,
) -> AgentAuthorityStateSummary {
    AgentAuthorityStateSummary {
        status: match state.authority {
            crate::service::agent_authority::AuthorityStatus::Claimed => {
                AgentAuthorityStatus::Claimed
            }
            crate::service::agent_authority::AuthorityStatus::Unclaimed => {
                AgentAuthorityStatus::Unclaimed
            }
        },
        outstanding_offer: state.outstanding_offer.map(outstanding_offer_response),
    }
}

fn current_offer_details(
    current: Option<crate::service::agent_authority::OutstandingOffer>,
) -> Option<serde_json::Value> {
    current.map(|offer| {
        serde_json::json!({
            "current_offer": {
                "id": offer.id.into_inner(),
                "code_prefix": offer.code_prefix,
                "expires_at": offer.expires_at.to_rfc3339(),
                "created_at": offer.created_at.to_rfc3339()
            }
        })
    })
}

fn conflict(
    code: &'static str,
    message: impl Into<String>,
    details: Option<serde_json::Value>,
) -> AppError {
    AppError::Domain {
        status: StatusCode::CONFLICT,
        code,
        message: message.into(),
        details,
    }
}

fn map_onboarding_error(error: OnboardingError) -> AppError {
    match error {
        OnboardingError::Invalid(message) => AppError::BadRequest(message),
        OnboardingError::Validation(message) => AppError::Validation(message),
        OnboardingError::LimitReached(limit) => AppError::BadRequest(format!(
            "Server limit reached ({limit}). Delete unused servers or increase max_servers in config."
        )),
        OnboardingError::IdempotencyConflict => conflict(
            "ONBOARDING_IDEMPOTENCY_CONFLICT",
            "onboarding_request_id was already used with different input",
            None,
        ),
        OnboardingError::Store(error) => error,
    }
}

fn map_issue_offer_error(error: crate::service::agent_authority::IssueOfferError) -> AppError {
    use crate::service::agent_authority::IssueOfferError;
    match error {
        IssueOfferError::NotFound => AppError::NotFound("server not found".to_string()),
        IssueOfferError::AlreadyClaimed => conflict(
            "AGENT_AUTHORITY_ALREADY_CLAIMED",
            "server authority is already claimed; begin re-enrollment instead",
            None,
        ),
        IssueOfferError::OutstandingExists(current) => conflict(
            "ENROLLMENT_OFFER_OUTSTANDING",
            "an Outstanding enrollment offer already exists",
            current_offer_details(Some(current)),
        ),
        IssueOfferError::Store(error) => error,
    }
}

fn map_reenrollment_error(error: crate::service::agent_authority::ReenrollmentError) -> AppError {
    use crate::service::agent_authority::ReenrollmentError;
    match error {
        ReenrollmentError::NotFound => AppError::NotFound("server not found".to_string()),
        ReenrollmentError::Unclaimed => conflict(
            "AGENT_AUTHORITY_UNCLAIMED",
            "server authority is Unclaimed; issue an offer instead",
            None,
        ),
        ReenrollmentError::OutstandingExists(current) => conflict(
            "ENROLLMENT_OFFER_OUTSTANDING",
            "an Outstanding enrollment offer already exists",
            current_offer_details(Some(current)),
        ),
        ReenrollmentError::Store(error) => error,
    }
}

fn map_replace_offer_error(error: crate::service::agent_authority::ReplaceOfferError) -> AppError {
    use crate::service::agent_authority::ReplaceOfferError;
    match error {
        ReplaceOfferError::ServerNotFound => AppError::NotFound("server not found".to_string()),
        ReplaceOfferError::OfferNotFound => {
            AppError::NotFound("enrollment offer not found".to_string())
        }
        ReplaceOfferError::NotOutstanding { outcome, current } => conflict(
            "ENROLLMENT_OFFER_TERMINAL",
            format!("enrollment offer is already {}", outcome.as_str()),
            current_offer_details(current),
        ),
        ReplaceOfferError::Stale { current } => conflict(
            "ENROLLMENT_OFFER_STALE",
            "the exact offer is not the current Outstanding offer",
            current_offer_details(current),
        ),
        ReplaceOfferError::Store(error) => error,
    }
}

fn map_revoke_offer_error(error: crate::service::agent_authority::RevokeOfferError) -> AppError {
    use crate::service::agent_authority::RevokeOfferError;
    match error {
        RevokeOfferError::ServerNotFound => AppError::NotFound("server not found".to_string()),
        RevokeOfferError::OfferNotFound => {
            AppError::NotFound("enrollment offer not found".to_string())
        }
        RevokeOfferError::Terminal(outcome) => conflict(
            "ENROLLMENT_OFFER_TERMINAL",
            format!("enrollment offer is already {}", outcome.as_str()),
            None,
        ),
        RevokeOfferError::Store(error) => error,
    }
}

#[utoipa::path(
    post,
    path = "/api/servers/{id}/agent-authority/re-enrollment",
    tag = "servers",
    params(("id" = String, Path, description = "Server ID")),
    request_body = ReenrollmentRequest,
    responses(
        (status = 200, description = "Re-enrollment offer issued", body = EnrollmentOfferResponse),
        (status = 404, description = "Server not found"),
        (status = 409, description = "Authority or offer state conflict"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn begin_reenrollment(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(id): Path<String>,
    Json(body): Json<ReenrollmentRequest>,
) -> Result<Json<ApiResponse<EnrollmentOfferResponse>>, AppError> {
    let enrollment = state
        .agent_authority
        .begin_reenrollment(crate::service::agent_authority::BeginReenrollment {
            server_id: parse_server_id(id)?,
            mode: match body.mode {
                ReenrollmentModeRequest::Graceful => {
                    crate::service::agent_authority::ReenrollmentMode::Graceful
                }
                ReenrollmentModeRequest::Emergency => {
                    crate::service::agent_authority::ReenrollmentMode::Emergency
                }
            },
            actor: authority_actor(&current_user),
            source: request_source("api:begin-re-enrollment")?,
            ttl: parse_offer_ttl(body.ttl_secs)?,
        })
        .await
        .map_err(map_reenrollment_error)?;
    ok(EnrollmentOfferResponse {
        enrollment: enrollment_issue_response(enrollment),
    })
}

#[utoipa::path(
    post,
    path = "/api/servers/{id}/agent-authority/offers",
    tag = "servers",
    params(("id" = String, Path, description = "Server ID")),
    request_body = IssueOfferRequest,
    responses(
        (status = 200, description = "Enrollment offer issued", body = EnrollmentOfferResponse),
        (status = 404, description = "Server not found"),
        (status = 409, description = "Authority or offer state conflict"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn issue_offer_for_unclaimed(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(id): Path<String>,
    Json(body): Json<IssueOfferRequest>,
) -> Result<Json<ApiResponse<EnrollmentOfferResponse>>, AppError> {
    let enrollment = state
        .agent_authority
        .issue_offer_for_unclaimed(crate::service::agent_authority::IssueOfferForUnclaimed {
            server_id: parse_server_id(id)?,
            actor: authority_actor(&current_user),
            source: request_source("api:issue-enrollment-offer")?,
            ttl: parse_offer_ttl(body.ttl_secs)?,
        })
        .await
        .map_err(map_issue_offer_error)?;
    ok(EnrollmentOfferResponse {
        enrollment: enrollment_issue_response(enrollment),
    })
}

#[utoipa::path(
    post,
    path = "/api/servers/{id}/agent-authority/offers/{offer_id}/replace",
    tag = "servers",
    params(
        ("id" = String, Path, description = "Server ID"),
        ("offer_id" = String, Path, description = "Exact current offer ID"),
    ),
    responses(
        (status = 200, description = "Enrollment offer replaced", body = EnrollmentOfferResponse),
        (status = 404, description = "Server or offer not found"),
        (status = 409, description = "Offer is stale or terminal"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn replace_offer(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path((id, offer_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<EnrollmentOfferResponse>>, AppError> {
    let enrollment = state
        .agent_authority
        .replace_offer(crate::service::agent_authority::ReplaceOffer {
            server_id: parse_server_id(id)?,
            offer_id: parse_offer_id(offer_id)?,
            actor: authority_actor(&current_user),
            source: request_source("api:replace-enrollment-offer")?,
            ttl: crate::service::agent_authority::OfferTtl::default(),
        })
        .await
        .map_err(map_replace_offer_error)?;
    ok(EnrollmentOfferResponse {
        enrollment: enrollment_issue_response(enrollment),
    })
}

#[utoipa::path(
    delete,
    path = "/api/servers/{id}/agent-authority/offers/{offer_id}",
    tag = "servers",
    params(
        ("id" = String, Path, description = "Server ID"),
        ("offer_id" = String, Path, description = "Offer ID"),
    ),
    responses(
        (status = 200, description = "Enrollment offer revoked", body = RevokeOfferResponse),
        (status = 404, description = "Server or offer not found"),
        (status = 409, description = "Offer has another terminal outcome"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn revoke_offer(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path((id, offer_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<RevokeOfferResponse>>, AppError> {
    let receipt = state
        .agent_authority
        .revoke_offer(crate::service::agent_authority::RevokeOffer {
            server_id: parse_server_id(id)?,
            offer_id: parse_offer_id(offer_id)?,
            actor: authority_actor(&current_user),
            source: request_source("api:revoke-enrollment-offer")?,
        })
        .await
        .map_err(map_revoke_offer_error)?;
    ok(RevokeOfferResponse {
        offer_id: receipt.offer_id.into_inner(),
        already_revoked: receipt.already_revoked,
    })
}

#[utoipa::path(
    delete,
    path = "/api/servers/{id}/agent-authority",
    tag = "servers",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Agent authority revoked", body = RevokeAuthorityResponse),
        (status = 404, description = "Server not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn revoke_agent_authority(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<RevokeAuthorityResponse>>, AppError> {
    let receipt = state
        .agent_authority
        .revoke_authority(crate::service::agent_authority::RevokeAuthority {
            server_id: parse_server_id(id)?,
            actor: authority_actor(&current_user),
            source: request_source("api:revoke-agent-authority")?,
        })
        .await
        .map_err(|error| match error {
            crate::service::agent_authority::RevokeAuthorityError::NotFound => {
                AppError::NotFound("server not found".to_string())
            }
            crate::service::agent_authority::RevokeAuthorityError::Store(error) => error,
        })?;
    ok(RevokeAuthorityResponse {
        server_id: receipt.server_id.into_inner(),
        changed: receipt.changed,
    })
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/agent-authority",
    tag = "servers",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Agent authority state", body = AgentAuthorityStateSummary),
        (status = 404, description = "Server not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_agent_authority(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<AgentAuthorityStateSummary>>, AppError> {
    let state = state
        .agent_authority
        .state(parse_server_id(id)?)
        .await
        .map_err(|error| match error {
            crate::service::agent_authority::StateError::NotFound => {
                AppError::NotFound("server not found".to_string())
            }
            crate::service::agent_authority::StateError::Store(error) => error,
        })?;
    ok(authority_state_response(state))
}

#[utoipa::path(
    get,
    path = "/api/agent-authority/events",
    tag = "servers",
    params(AuthorityHistoryQuery),
    responses(
        (status = 200, description = "Agent authority event history", body = Vec<AuthorityEventResponse>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_authority_history(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AuthorityHistoryQuery>,
) -> Result<Json<ApiResponse<Vec<AuthorityEventResponse>>>, AppError> {
    let events = state
        .agent_authority
        .history(crate::service::agent_authority::HistoryQuery {
            server_id: parse_server_id(query.server_id)?,
            limit: query.limit,
        })
        .await
        .map_err(|error| match error {
            crate::service::agent_authority::HistoryError::Store(error) => error,
        })?;
    ok(events
        .into_iter()
        .map(|event| AuthorityEventResponse {
            id: event.id,
            server_id: event.server_id.into_inner(),
            server_name: event.server_name,
            actor_kind: event.actor_kind.as_str().to_string(),
            actor_id: event.actor_id,
            request_source: event.request_source,
            offer_id: event.offer_id.map(|id| id.into_inner()),
            transition: event.transition.as_str().to_string(),
            mode: event.mode.map(|mode| mode.as_str().to_string()),
            offer_outcome: event
                .offer_outcome
                .map(|outcome| outcome.as_str().to_string()),
            authority_before: event.authority_before.as_str().to_string(),
            authority_after: event.authority_after.as_str().to_string(),
            created_at: event.created_at,
        })
        .collect())
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
    let authority = state
        .agent_authority
        .state(parse_server_id(id)?)
        .await
        .map_err(|error| match error {
            crate::service::agent_authority::StateError::NotFound => {
                AppError::NotFound("server not found".to_string())
            }
            crate::service::agent_authority::StateError::Store(error) => error,
        })?;
    ok(build_server_response(
        server,
        &state.agent_manager,
        authority_state_response(authority),
    ))
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
    Path(id): Path<String>,
    Json(input): Json<UpdateServerInput>,
) -> Result<Json<ApiResponse<ServerResponse>>, AppError> {
    // Capabilities are agent-owned and not writable here (the `capabilities`
    // field was removed from `UpdateServerInput`), so updating a server can no
    // longer change what the agent is allowed to do.
    let server = ServerService::update_server(&state.db, &id, input).await?;

    let authority = state
        .agent_authority
        .state(parse_server_id(id)?)
        .await
        .map_err(|error| match error {
            crate::service::agent_authority::StateError::NotFound => {
                AppError::NotFound("server not found".to_string())
            }
            crate::service::agent_authority::StateError::Store(error) => error,
        })?;
    ok(build_server_response(
        server,
        &state.agent_manager,
        authority_state_response(authority),
    ))
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
pub async fn delete_server(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let deleted = state
        .agent_authority
        .delete_servers(
            &[parse_server_id(id.clone())?],
            &authority_actor(&current_user),
            &request_source("api:delete-server")?,
        )
        .await?;
    let deleted = deleted
        .into_iter()
        .next()
        .ok_or_else(|| AppError::NotFound("server not found".to_string()))?;
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "server_deleted",
        Some(&format!("server_id={id} name={}", deleted.name)),
        &ip,
    )
    .await;
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
pub async fn batch_delete(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Json(body): Json<BatchDeleteRequest>,
) -> Result<Json<ApiResponse<BatchDeleteResponse>>, AppError> {
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let server_ids = body
        .ids
        .iter()
        .cloned()
        .map(parse_server_id)
        .collect::<Result<Vec<_>, _>>()?;
    let deleted = state
        .agent_authority
        .delete_servers(
            &server_ids,
            &authority_actor(&current_user),
            &request_source("api:batch-delete-servers")?,
        )
        .await?
        .len() as u64;
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "server_batch_deleted",
        Some(&format!("ids={} deleted={}", body.ids.join(","), deleted)),
        &ip,
    )
    .await;
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpgradeRequest>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    use serverbee_common::constants::CAP_UPGRADE;

    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    crate::service::capability_gate::require_capability_audited(
        &state,
        &id,
        CAP_UPGRADE,
        &current_user.user_id,
        &ip,
        "server_upgrade_denied",
    )
    .await?;

    let version = normalize_version(&body.version);

    // Validate version format (SemVer, with optional pre-release / build metadata).
    if semver::Version::parse(version).is_err() {
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

    // Audit the upgrade trigger (best-effort). Remote code is delivered to the
    // agent here, so the actor and target version belong in the trail.
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "server_upgrade",
        Some(&format!("server_id={id} version={version}")),
        &ip,
    )
    .await;

    ok("ok")
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

    state
        .agent_desired_state
        .reconcile_agent_or_warn(&id, AgentDesiredStateDomain::NetworkProbes)
        .await;

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
    let mut task_cleanup = task_scheduler::begin_server_cleanup(&state).await;
    let candidates = server::Entity::find()
        .filter(server::Column::Name.eq("New Server"))
        .filter(server::Column::Os.is_null())
        .all(&state.db)
        .await?;

    let orphan_ids = collect_orphan_server_ids(&candidates, |id| state.agent_manager.is_online(id));
    if orphan_ids.is_empty() {
        return ok(CleanupResponse { deleted_count: 0 });
    }

    let cleanup_actor = crate::service::agent_authority::Actor::System;
    let cleanup_source = request_source("system:orphan-cleanup")?;
    let typed_ids = orphan_ids
        .into_iter()
        .map(parse_server_id)
        .collect::<Result<Vec<_>, _>>()?;
    let deleted_rows = state
        .agent_authority
        .delete_servers(&typed_ids, &cleanup_actor, &cleanup_source)
        .await?;
    let deleted_ids: Vec<String> = deleted_rows
        .iter()
        .map(|server| server.id.clone())
        .collect();
    if deleted_ids.is_empty() {
        return ok(CleanupResponse { deleted_count: 0 });
    }

    let txn = state.db.begin().await?;
    task_cleanup
        .remove_server_references(&txn, &deleted_ids)
        .await?;

    // Remaining tables with server_ids_json — per-table rules
    cleanup_json_array_tables(&txn, &deleted_ids).await?;

    txn.commit().await?;
    task_cleanup.apply_after_commit(&state).await;

    let deleted_count = deleted_rows.len() as u64;
    tracing::info!("Cleaned up {deleted_count} orphaned servers");
    ok(CleanupResponse { deleted_count })
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
            geo_manual: false,
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

#[cfg(test)]
mod delete_audit_tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::entity::audit_log;
    use crate::middleware::auth::CurrentUser;
    use crate::test_utils::setup_test_db;
    use chrono::Utc;
    use sea_orm::IntoActiveModel;
    use serverbee_common::constants::CAP_DEFAULT;

    fn admin() -> CurrentUser {
        CurrentUser {
            user_id: "admin-1".to_string(),
            username: "admin".to_string(),
            role: "admin".to_string(),
            must_change_password: false,
        }
    }

    fn conn() -> ConnectInfo<SocketAddr> {
        ConnectInfo("203.0.113.9:4444".parse().unwrap())
    }

    async fn insert_server(db: &sea_orm::DatabaseConnection, id: &str, name: &str) {
        let now = Utc::now();
        let model = server::Model {
            id: id.to_string(),
            token_hash: Some("hash".to_string()),
            token_prefix: Some("prefix".to_string()),
            name: name.to_string(),
            cpu_name: None,
            cpu_cores: None,
            cpu_arch: None,
            os: None,
            kernel_version: None,
            mem_total: None,
            swap_total: None,
            disk_total: None,
            ipv4: None,
            ipv6: None,
            region: None,
            country_code: None,
            geo_manual: false,
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
            created_at: now,
            updated_at: now,
        };
        model.into_active_model().insert(db).await.unwrap();
    }

    #[tokio::test]
    async fn delete_server_writes_audit_log() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "srv-del", "Doomed").await;
        let state = AppState::new(db.clone(), AppConfig::default()).await.unwrap();

        let res = delete_server(
            State(state.clone()),
            conn(),
            Extension(admin()),
            HeaderMap::new(),
            Path("srv-del".to_string()),
        )
        .await;
        assert!(res.is_ok(), "delete should succeed: {res:?}");

        let logs = audit_log::Entity::find().all(&db).await.unwrap();
        assert!(
            logs.iter().any(|l| l.action == "server_deleted"
                && l.user_id == "admin-1"
                && l
                    .detail
                    .as_deref()
                    .is_some_and(|d| d.contains("srv-del") && d.contains("Doomed"))),
            "expected a server_deleted audit row, got: {logs:?}"
        );
    }

    #[tokio::test]
    async fn batch_delete_writes_audit_log() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "srv-a", "A").await;
        insert_server(&db, "srv-b", "B").await;
        let state = AppState::new(db.clone(), AppConfig::default()).await.unwrap();

        let res = batch_delete(
            State(state.clone()),
            conn(),
            Extension(admin()),
            HeaderMap::new(),
            Json(BatchDeleteRequest {
                ids: vec!["srv-a".to_string(), "srv-b".to_string()],
            }),
        )
        .await;
        assert!(res.is_ok(), "batch delete should succeed: {res:?}");

        let logs = audit_log::Entity::find().all(&db).await.unwrap();
        assert!(
            logs.iter().any(|l| l.action == "server_batch_deleted"
                && l.detail
                    .as_deref()
                    .is_some_and(|d| d.contains("srv-a") && d.contains("deleted=2"))),
            "expected a server_batch_deleted audit row, got: {logs:?}"
        );
    }
}
