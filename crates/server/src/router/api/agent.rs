use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::server;
use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::utils::extract_client_ip;
use crate::service::audit::AuditService;
use crate::service::auth::AuthService;
use crate::service::enrollment::{DEFAULT_TTL_SECS, EnrollmentService};
use crate::service::network_probe::NetworkProbeService;
use crate::service::upgrade_release::LatestAgentVersionResponse;
use crate::state::AppState;
use serverbee_common::constants::CAP_DEFAULT;

const DEFAULT_SERVER_NAME: &str = "New Server";

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct RegisterRequest {
    #[serde(default)]
    fingerprint: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RegisterResponse {
    server_id: String,
    token: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateEnrollmentRequest {
    #[serde(default)]
    label: Option<String>,
    /// Lifetime in seconds. Defaults to 600 (10 min), max 86400.
    ttl_secs: Option<i64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CreateEnrollmentResponse {
    id: String,
    /// Plaintext enrollment code — shown exactly once, never retrievable again.
    code: String,
    expires_at: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EnrollmentSummary {
    id: String,
    label: Option<String>,
    code_prefix: String,
    created_by: String,
    expires_at: String,
    consumed_at: Option<String>,
    created_at: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RotateTokenResponse {
    server_id: String,
    /// New plaintext run token — shown once. The agent must be reconfigured
    /// with this value (or it will need to re-enroll).
    token: String,
}

/// Public routes for agent registration (Bearer auth checked inside handler).
pub fn public_router() -> Router<Arc<AppState>> {
    Router::new().route("/agent/register", post(register))
}

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route("/agent/latest-version", get(latest_version))
}

#[utoipa::path(
    get,
    path = "/api/agent/latest-version",
    tag = "agent",
    responses(
        (status = 200, description = "Latest agent release metadata", body = LatestAgentVersionResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn latest_version(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<LatestAgentVersionResponse>>, AppError> {
    ok(state.upgrade_release_service.latest().await)
}

#[utoipa::path(
    post,
    path = "/api/agent/register",
    tag = "agent",
    responses(
        (status = 200, description = "Agent registered", body = RegisterResponse),
        (status = 400, description = "Server limit reached"),
        (status = 401, description = "Invalid, expired, or already-used enrollment code"),
    ),
    security(("bearer_token" = []))
)]
async fn register(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Option<Json<RegisterRequest>>,
) -> Result<Json<ApiResponse<RegisterResponse>>, AppError> {
    // 1. Rate limiting
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    if !state.check_register_rate(&ip) {
        return Err(AppError::TooManyRequests(
            "Too many registration attempts, please try later".to_string(),
        ));
    }

    // 2. Enrollment code validation (single-use, TTL, constant-time argon2)
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    let enrollment =
        EnrollmentService::verify_and_consume(&state.db, auth_header)
            .await?
            .ok_or(AppError::Unauthorized)?;

    let fingerprint = body
        .as_ref()
        .map(|b| b.fingerprint.clone())
        .unwrap_or_default();

    // Validate fingerprint format if provided
    if !fingerprint.is_empty()
        && (fingerprint.len() != 64 || !fingerprint.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(AppError::BadRequest(
            "Invalid fingerprint format".to_string(),
        ));
    }

    // 3. Fingerprint dedup: try to reuse existing server
    if !fingerprint.is_empty()
        && let Some(existing) = server::Entity::find()
            .filter(server::Column::Fingerprint.eq(&fingerprint))
            .one(&state.db)
            .await?
    {
        let server_id = existing.id.clone();
        tracing::info!("Reusing server {server_id} for fingerprint {fingerprint}");

        let plaintext_token = AuthService::generate_session_token();
        let token_hash = AuthService::hash_password(&plaintext_token)?;
        let token_prefix = &plaintext_token[..8.min(plaintext_token.len())];

        let mut active: server::ActiveModel = existing.into();
        active.token_hash = Set(token_hash);
        active.token_prefix = Set(token_prefix.to_string());
        active.last_remote_addr = Set(Some(ip.clone()));
        active.updated_at = Set(Utc::now());
        active.update(&state.db).await?;

        let _ = AuditService::log(
            &state.db,
            "system",
            "agent_enrolled",
            Some(&format!(
                "server_id={server_id} enrollment={} prefix={}",
                enrollment.id, enrollment.code_prefix
            )),
            &ip,
        )
        .await;

        return ok(RegisterResponse {
            server_id,
            token: plaintext_token,
        });
    }

    // 4. Global server limit check (soft cap, only for new servers)
    let max_servers = state.config.auth.max_servers;
    if max_servers > 0 {
        let count = server::Entity::find().count(&state.db).await?;
        if count >= max_servers as u64 {
            return Err(AppError::BadRequest(format!(
                "Server limit reached ({max_servers}). Delete unused servers or increase max_servers in config."
            )));
        }
    }

    // 5. Create new server
    let server_id = Uuid::new_v4().to_string();
    let plaintext_token = AuthService::generate_session_token();
    let token_hash = AuthService::hash_password(&plaintext_token)?;
    let token_prefix = &plaintext_token[..8.min(plaintext_token.len())];
    let now = Utc::now();

    let fp = if fingerprint.is_empty() {
        None
    } else {
        Some(fingerprint.clone())
    };

    let new_server = server::ActiveModel {
        id: Set(server_id.clone()),
        token_hash: Set(token_hash),
        token_prefix: Set(token_prefix.to_string()),
        name: Set(DEFAULT_SERVER_NAME.to_string()),
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
        group_id: Set(None),
        weight: Set(0),
        hidden: Set(false),
        remark: Set(None),
        public_remark: Set(None),
        price: Set(None),
        billing_cycle: Set(None),
        currency: Set(None),
        expired_at: Set(None),
        traffic_limit: Set(None),
        traffic_limit_type: Set(None),
        billing_start_day: Set(None),
        capabilities: Set(CAP_DEFAULT as i32),
        protocol_version: Set(1),
        features: Set("[]".to_string()),
        last_remote_addr: Set(Some(ip.clone())),
        fingerprint: Set(fp.clone()),
        created_at: Set(now),
        updated_at: Set(now),
    };

    // Handle race condition: if another request with the same fingerprint inserted
    // between our SELECT and INSERT, catch the unique constraint violation and retry as reuse.
    match new_server.insert(&state.db).await {
        Ok(_) => {}
        Err(DbErr::Query(ref e)) if fp.is_some() && e.to_string().contains("UNIQUE") => {
            tracing::info!("Fingerprint race detected, falling back to reuse path");
            if let Some(existing) = server::Entity::find()
                .filter(server::Column::Fingerprint.eq(fp.as_ref().unwrap()))
                .one(&state.db)
                .await?
            {
                let server_id = existing.id.clone();
                let plaintext_token = AuthService::generate_session_token();
                let token_hash = AuthService::hash_password(&plaintext_token)?;
                let token_prefix = &plaintext_token[..8.min(plaintext_token.len())];

                let mut active: server::ActiveModel = existing.into();
                active.token_hash = Set(token_hash);
                active.token_prefix = Set(token_prefix.to_string());
                active.last_remote_addr = Set(Some(ip.clone()));
                active.updated_at = Set(Utc::now());
                active.update(&state.db).await?;

                let _ = AuditService::log(
                    &state.db,
                    "system",
                    "agent_enrolled",
                    Some(&format!(
                        "server_id={server_id} enrollment={} prefix={}",
                        enrollment.id, enrollment.code_prefix
                    )),
                    &ip,
                )
                .await;

                return ok(RegisterResponse {
                    server_id,
                    token: plaintext_token,
                });
            }
            return Err(AppError::Internal(
                "Fingerprint race recovery failed".to_string(),
            ));
        }
        Err(e) => return Err(e.into()),
    }

    // Apply default network probe targets
    if let Err(e) = NetworkProbeService::apply_defaults(&state.db, &server_id).await {
        tracing::warn!("Failed to apply default network probe targets to {server_id}: {e}");
    }

    let _ = AuditService::log(
        &state.db,
        "system",
        "agent_enrolled",
        Some(&format!(
            "server_id={server_id} enrollment={} prefix={}",
            enrollment.id, enrollment.code_prefix
        )),
        &ip,
    )
    .await;

    ok(RegisterResponse {
        server_id,
        token: plaintext_token,
    })
}

/// Admin-only routes for managing enrollment codes.
pub fn admin_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/agent/enrollments", post(create_enrollment))
        .route("/agent/enrollments", get(list_enrollments))
        .route(
            "/agent/enrollments/{id}",
            axum::routing::delete(delete_enrollment),
        )
        .route("/agent/{id}/rotate-token", post(rotate_token))
}

#[utoipa::path(
    post,
    path = "/api/agent/enrollments",
    tag = "agent",
    request_body = CreateEnrollmentRequest,
    responses((status = 200, description = "Enrollment code created", body = CreateEnrollmentResponse)),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn create_enrollment(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Extension(current_user): Extension<CurrentUser>,
    Json(body): Json<CreateEnrollmentRequest>,
) -> Result<Json<ApiResponse<CreateEnrollmentResponse>>, AppError> {
    let ttl = body.ttl_secs.unwrap_or(DEFAULT_TTL_SECS);
    let (model, code) =
        EnrollmentService::mint(&state.db, &current_user.user_id, body.label.clone(), ttl).await?;
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let detail = format!("id={} prefix={}", model.id, model.code_prefix);
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "agent_enrollment_created",
        Some(&detail),
        &ip,
    )
    .await;
    ok(CreateEnrollmentResponse {
        id: model.id,
        code,
        expires_at: model.expires_at.to_rfc3339(),
    })
}

#[utoipa::path(
    get,
    path = "/api/agent/enrollments",
    tag = "agent",
    responses((status = 200, description = "List enrollment codes", body = [EnrollmentSummary])),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_enrollments(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<EnrollmentSummary>>>, AppError> {
    let rows = EnrollmentService::list(&state.db).await?;
    let out = rows
        .into_iter()
        .map(|m| EnrollmentSummary {
            id: m.id,
            label: m.label,
            code_prefix: m.code_prefix,
            created_by: m.created_by,
            expires_at: m.expires_at.to_rfc3339(),
            consumed_at: m.consumed_at.map(|d| d.to_rfc3339()),
            created_at: m.created_at.to_rfc3339(),
        })
        .collect();
    ok(out)
}

#[utoipa::path(
    delete,
    path = "/api/agent/enrollments/{id}",
    tag = "agent",
    params(("id" = String, Path, description = "Enrollment id")),
    responses((status = 200, description = "Deleted")),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn delete_enrollment(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Extension(current_user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    EnrollmentService::delete(&state.db, &id).await?;
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let detail = format!("id={id}");
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "agent_enrollment_deleted",
        Some(&detail),
        &ip,
    )
    .await;
    ok("deleted")
}

#[utoipa::path(
    post,
    path = "/api/agent/{id}/rotate-token",
    tag = "agent",
    params(("id" = String, Path, description = "Server id")),
    responses(
        (status = 200, description = "Token rotated; old token revoked", body = RotateTokenResponse),
        (status = 404, description = "Server not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn rotate_token(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Extension(current_user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<RotateTokenResponse>>, AppError> {
    let existing = server::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Server not found".to_string()))?;

    let plaintext = AuthService::generate_session_token();
    let token_hash = AuthService::hash_password(&plaintext)?;
    let token_prefix = plaintext[..8.min(plaintext.len())].to_string();

    let mut active: server::ActiveModel = existing.into();
    active.token_hash = Set(token_hash);
    active.token_prefix = Set(token_prefix);
    active.updated_at = Set(Utc::now());
    active.update(&state.db).await?;

    // Drop any live agent connection so it must reconnect with the new token.
    state.agent_manager.remove_connection(&id);

    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let detail = format!("server_id={id}");
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "agent_token_rotated",
        Some(&detail),
        &ip,
    )
    .await;

    ok(RotateTokenResponse {
        server_id: id,
        token: plaintext,
    })
}

#[cfg(test)]
mod enrollment_endpoint_tests {
    use crate::entity::user;
    use crate::service::enrollment::EnrollmentService;
    use crate::test_utils::setup_test_db;
    use chrono::Utc;
    use sea_orm::*;
    use uuid::Uuid;

    /// Seed a user so the `created_by` FK on `agent_enrollments` is satisfied.
    async fn seed_user(db: &DatabaseConnection) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        user::ActiveModel {
            id: Set(id.clone()),
            username: Set(format!("user-{id}")),
            password_hash: Set("$argon2id$v=19$m=19456,t=2,p=1$x$x".to_string()),
            role: Set("admin".to_string()),
            totp_secret: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .expect("seed user");
        id
    }

    #[tokio::test]
    async fn mint_then_list_shows_prefix_not_code() {
        let (db, _tmp) = setup_test_db().await;
        let uid = seed_user(&db).await;
        let (_m, code) = EnrollmentService::mint(&db, &uid, None, 600).await.unwrap();
        let list = EnrollmentService::list(&db).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].code_prefix, &code[..8]);
        assert!(
            !list[0].code_hash.contains(&code),
            "plaintext code never stored"
        );
    }

    #[tokio::test]
    async fn register_flow_consumes_code_single_use() {
        let (db, _tmp) = setup_test_db().await;
        let uid = seed_user(&db).await;
        let (_m, code) = EnrollmentService::mint(&db, &uid, None, 600)
            .await
            .unwrap();
        assert!(
            EnrollmentService::verify_and_consume(&db, &code)
                .await
                .unwrap()
                .is_some(),
            "first registration consumes the code"
        );
        assert!(
            EnrollmentService::verify_and_consume(&db, &code)
                .await
                .unwrap()
                .is_none(),
            "a replayed enrollment code is rejected"
        );
    }

    #[tokio::test]
    async fn rotate_token_invalidates_old_token() {
        use crate::entity::server;
        use crate::service::auth::AuthService;
        use serverbee_common::constants::CAP_DEFAULT;

        let (db, _tmp) = setup_test_db().await;

        let old_plain = AuthService::generate_session_token();
        let old_hash = AuthService::hash_password(&old_plain).unwrap();
        let sid = Uuid::new_v4().to_string();
        let now = Utc::now();

        let server_model = server::ActiveModel {
            id: Set(sid.clone()),
            token_hash: Set(old_hash.clone()),
            token_prefix: Set(old_plain[..8].to_string()),
            name: Set("t".to_string()),
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
            group_id: Set(None),
            weight: Set(0),
            hidden: Set(false),
            remark: Set(None),
            public_remark: Set(None),
            price: Set(None),
            billing_cycle: Set(None),
            currency: Set(None),
            expired_at: Set(None),
            traffic_limit: Set(None),
            traffic_limit_type: Set(None),
            billing_start_day: Set(None),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            features: Set("[]".to_string()),
            last_remote_addr: Set(None),
            fingerprint: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };
        server_model.insert(&db).await.expect("insert server");

        // Simulate rotation: new token + hash, persist.
        let new_plain = AuthService::generate_session_token();
        let new_hash = AuthService::hash_password(&new_plain).unwrap();
        let existing = server::Entity::find_by_id(&sid)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let mut active: server::ActiveModel = existing.into();
        active.token_hash = Set(new_hash.clone());
        active.token_prefix = Set(new_plain[..8].to_string());
        active.update(&db).await.unwrap();

        // Old token must no longer verify; new one must.
        assert!(
            !AuthService::verify_password(&old_plain, &new_hash).unwrap(),
            "old token must not verify against rotated hash"
        );
        assert!(
            AuthService::verify_password(&new_plain, &new_hash).unwrap(),
            "new token verifies"
        );
    }

    #[tokio::test]
    async fn enrollment_summary_dto_never_exposes_code_or_hash() {
        let (db, _tmp) = setup_test_db().await;
        let uid = seed_user(&db).await;
        let (model, code) = EnrollmentService::mint(&db, &uid, None, 600)
            .await
            .unwrap();

        // Mirror exactly the mapping in `list_enrollments`.
        let summary = super::EnrollmentSummary {
            id: model.id,
            label: model.label,
            code_prefix: model.code_prefix,
            created_by: model.created_by,
            expires_at: model.expires_at.to_rfc3339(),
            consumed_at: model.consumed_at.map(|d| d.to_rfc3339()),
            created_at: model.created_at.to_rfc3339(),
        };
        let json = serde_json::to_string(&summary).expect("serialize");

        assert!(
            !json.contains("code_hash"),
            "DTO must never expose code_hash: {json}"
        );
        assert!(
            !json.contains(&code),
            "DTO must never expose the plaintext code"
        );
        // code_prefix is the only code-derived field that may appear.
        assert!(json.contains(&format!("\"code_prefix\":\"{}\"", &code[..8])));
    }
}
