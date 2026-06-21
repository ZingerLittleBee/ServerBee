use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, TransactionTrait};
use serde::{Deserialize, Serialize};

use crate::entity::server;
use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::utils::extract_client_ip;
use crate::service::audit::AuditService;
use crate::service::auth::AuthService;
use crate::service::enrollment::EnrollmentService;
use crate::service::upgrade_release::LatestAgentVersionResponse;
use crate::state::AppState;

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

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EnrollmentSummary {
    pub id: String,
    pub target_server_id: String,
    pub code_prefix: String,
    pub created_by: String,
    pub expires_at: String,
    pub consumed_at: Option<String>,
    pub revoked_at: Option<String>,
    pub created_at: String,
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
        (status = 200, description = "Agent registered against the bound server", body = RegisterResponse),
        (status = 400, description = "Invalid fingerprint format"),
        (status = 401, description = "Invalid, expired, revoked, or already-used enrollment code"),
    ),
    security(("bearer_token" = []))
)]
async fn register(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Option<Json<RegisterRequest>>,
) -> Result<Json<ApiResponse<RegisterResponse>>, AppError> {
    // 1. Rate limiting by client IP.
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

    // 2. Extract Bearer enrollment code.
    let bearer = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?
        .to_string();

    // 3. Validate fingerprint format BEFORE opening the transaction so a
    //    malformed payload cannot burn the operator's single-use code.
    //    The fingerprint is informational only — it is recorded on the
    //    server row but NEVER used for lookup or deduplication.
    let fingerprint = body
        .as_ref()
        .map(|b| b.fingerprint.clone())
        .filter(|f| !f.is_empty());
    if let Some(ref fp) = fingerprint
        && (fp.len() != 64 || !fp.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(AppError::BadRequest(
            "Invalid fingerprint format".to_string(),
        ));
    }

    // 4. Single transaction: consume enrollment + stamp token on the bound
    //    server. If anything fails the consume is rolled back so the operator
    //    can retry with the same code.
    let tx_bearer = bearer.clone();
    let tx_ip = ip.clone();
    let tx_fp = fingerprint.clone();
    let (server_id, plaintext_token, enrollment_id, enrollment_prefix) = state
        .db
        .transaction::<_, (String, String, String, String), AppError>(move |tx| {
            Box::pin(async move {
                let enrollment =
                    EnrollmentService::verify_and_consume_tx(tx, &tx_bearer)
                        .await?
                        .ok_or(AppError::Unauthorized)?;

                let server_row = server::Entity::find_by_id(&enrollment.target_server_id)
                    .one(tx)
                    .await?
                    .ok_or_else(|| {
                        // Should be impossible: the FK on agent_enrollments
                        // guarantees the bound server exists.
                        AppError::Internal("Bound server vanished".to_string())
                    })?;

                let plaintext = AuthService::generate_session_token();
                let token_hash = AuthService::hash_password(&plaintext)?;
                let token_prefix = plaintext[..8.min(plaintext.len())].to_string();
                let server_id = server_row.id.clone();
                let enrollment_id = enrollment.id.clone();
                let enrollment_prefix = enrollment.code_prefix.clone();

                let mut active: server::ActiveModel = server_row.into();
                active.token_hash = Set(Some(token_hash));
                active.token_prefix = Set(Some(token_prefix));
                active.last_remote_addr = Set(Some(tx_ip));
                active.fingerprint = Set(tx_fp);
                active.updated_at = Set(Utc::now());
                active.update(tx).await?;

                Ok((server_id, plaintext, enrollment_id, enrollment_prefix))
            })
        })
        .await
        .map_err(|e| match e {
            sea_orm::TransactionError::Connection(db_err) => AppError::from(db_err),
            sea_orm::TransactionError::Transaction(app_err) => app_err,
        })?;

    // Audit log AFTER commit so we don't log fictitious enrollments on rollback.
    let _ = AuditService::log(
        &state.db,
        "system",
        "agent_enrolled",
        Some(&format!(
            "server_id={server_id} enrollment={enrollment_id} prefix={enrollment_prefix}"
        )),
        &ip,
    )
    .await;

    // No ServerOnline broadcast here — that event belongs to
    // `AgentManager::add_connection` when the WS actually connects. The UI
    // picks up the pending→registered flip on its next list refresh, and the
    // agent immediately opens a WS after this response which triggers the
    // proper online event.
    ok(RegisterResponse {
        server_id,
        token: plaintext_token,
    })
}

/// Admin-only routes for managing enrollment codes.
pub fn admin_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/agent/enrollments", get(list_enrollments))
        .route(
            "/agent/enrollments/{id}",
            axum::routing::delete(delete_enrollment),
        )
        .route("/agent/{id}/rotate-token", post(rotate_token))
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
            target_server_id: m.target_server_id,
            code_prefix: m.code_prefix,
            created_by: m.created_by,
            expires_at: m.expires_at.to_rfc3339(),
            consumed_at: m.consumed_at.map(|d| d.to_rfc3339()),
            revoked_at: m.revoked_at.map(|d| d.to_rfc3339()),
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
    // DELETE is mapped to revoke(): the row stays in the table with a
    // non-null `revoked_at` for audit/history. Idempotent.
    EnrollmentService::revoke(&state.db, &id).await?;
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
        "agent_enrollment_revoked",
        Some(&detail),
        &ip,
    )
    .await;
    ok("revoked")
}

#[utoipa::path(
    post,
    path = "/api/agent/{id}/rotate-token",
    tag = "agent",
    params(("id" = String, Path, description = "Server id")),
    responses(
        (status = 200, description = "Token rotated; old token revoked", body = RotateTokenResponse),
        (status = 400, description = "Server is pending (no token to rotate); use recover instead"),
        (status = 404, description = "Server not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
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

    if existing.token_hash.is_none() {
        return Err(AppError::BadRequest(
            "cannot rotate token of a pending server; use recover instead".into(),
        ));
    }

    let plaintext = AuthService::generate_session_token();
    let token_hash = AuthService::hash_password(&plaintext)?;
    let token_prefix = plaintext[..8.min(plaintext.len())].to_string();

    let mut active: server::ActiveModel = existing.into();
    active.token_hash = Set(Some(token_hash));
    active.token_prefix = Set(Some(token_prefix));
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
    use crate::entity::{server, user};
    use crate::service::enrollment::EnrollmentService;
    use crate::test_utils::setup_test_db;
    use chrono::Utc;
    use sea_orm::*;
    use serverbee_common::constants::CAP_DEFAULT;
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
            must_change_password: Set(false),
            password_changed_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .expect("seed user");
        id
    }

    /// Seed a pending server (no token yet) so the new `target_server_id`
    /// FK on `agent_enrollments` is satisfied.
    async fn seed_pending_server(db: &DatabaseConnection) -> String {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.clone()),
            token_hash: Set(None),
            token_prefix: Set(None),
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
            geo_manual: Set(false),
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
        }
        .insert(db)
        .await
        .expect("seed pending server");
        id
    }

    #[tokio::test]
    async fn mint_then_list_shows_prefix_not_code() {
        let (db, _tmp) = setup_test_db().await;
        let uid = seed_user(&db).await;
        let sid = seed_pending_server(&db).await;
        let (_m, code) = EnrollmentService::mint_for_server(&db, &sid, &uid, 600)
            .await
            .unwrap();
        let list = EnrollmentService::list(&db).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].code_prefix, &code[..8]);
        assert!(
            !list[0].code_hash.contains(&code),
            "plaintext code never stored"
        );
    }

    // `register_flow_consumes_code_single_use` deleted: the new service-level
    // `verify_and_consume_single_use` test in `service::enrollment::tests`
    // already covers the same property at higher fidelity (real tx). The
    // end-to-end register flow will get its own integration test in T8.

    #[tokio::test]
    async fn rotate_token_invalidates_old_token() {
        use crate::service::auth::AuthService;

        let (db, _tmp) = setup_test_db().await;

        let old_plain = AuthService::generate_session_token();
        let old_hash = AuthService::hash_password(&old_plain).unwrap();
        let sid = Uuid::new_v4().to_string();
        let now = Utc::now();

        let server_model = server::ActiveModel {
            id: Set(sid.clone()),
            token_hash: Set(Some(old_hash.clone())),
            token_prefix: Set(Some(old_plain[..8].to_string())),
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
            geo_manual: Set(false),
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
        active.token_hash = Set(Some(new_hash.clone()));
        active.token_prefix = Set(Some(new_plain[..8].to_string()));
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
        let sid = seed_pending_server(&db).await;
        let (model, code) = EnrollmentService::mint_for_server(&db, &sid, &uid, 600)
            .await
            .unwrap();

        // Mirror exactly the mapping in `list_enrollments`.
        let summary = super::EnrollmentSummary {
            id: model.id,
            target_server_id: model.target_server_id,
            code_prefix: model.code_prefix,
            created_by: model.created_by,
            expires_at: model.expires_at.to_rfc3339(),
            consumed_at: model.consumed_at.map(|d| d.to_rfc3339()),
            revoked_at: model.revoked_at.map(|d| d.to_rfc3339()),
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
        // The bound server id is part of the DTO post-T11.
        assert!(json.contains(&format!("\"target_server_id\":\"{sid}\"")));
        // The `label` field is gone post-T11.
        assert!(
            !json.contains("\"label\""),
            "EnrollmentSummary must not expose `label` after T11"
        );
    }
}
