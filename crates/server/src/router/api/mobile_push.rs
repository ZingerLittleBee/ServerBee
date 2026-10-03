//! Current-installation subscription intent and verified relay setup.
use base64::{Engine, engine::general_purpose::STANDARD};
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, QueryFilter, Set,
    Statement, TransactionTrait,
};
use serde::{Deserialize, Serialize};

use super::mobile::{extract_bearer, push_session};
use crate::{
    entity::{mobile_push_outbox as outbox, mobile_push_registration as registration, user},
    error::{ApiResponse, AppError, ok},
    state::AppState,
};

#[derive(Debug, Clone, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PushPreferences {
    pub enabled: bool,
    pub alerts: bool,
    pub security: bool,
    pub task_failure: bool,
    pub task_success: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PushPreferencesRequest {
    pub expected_revision: i64,
    pub preferences: PushPreferences,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifiedPushRequest {
    pub expected_revision: i64,
    pub device_token: String,
    pub environment: String,
    pub key_id: String,
    pub grant_id: String,
    pub grant_token: String,
    pub content_key_id: String,
    pub content_key: String,
    pub deployment_id: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PushSetupResponse {
    pub revision: i64,
    pub preferences: PushPreferences,
    /// Whether the current account role permits security subscriptions.
    pub security_allowed: bool,
    /// Task routes require the current administrator role.
    pub tasks_allowed: bool,
    pub task_failure_available: bool,
    pub registered: bool,
    pub grant_expires_at: Option<DateTime<Utc>>,
    pub relay_url: String,
    /// Whether at least one event category is available for the current role.
    pub delivery_available: bool,
    /// A test is scoped to the authenticated installation.
    pub test_available: bool,
}

#[derive(Deserialize)]
struct RelayGrant {
    grant_id: String,
    key_id: String,
    device_token: String,
    environment: String,
    expires_at: i64,
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/mobile/push/settings", get(settings).put(save_preferences))
        .route("/mobile/push/verified-register", post(verified_register))
        .route("/mobile/push/test", post(test_notification))
        .route("/mobile/push/test/{event_id}", get(test_status))
}

fn response(
    row: Option<&registration::Model>,
    relay_url: &str,
    security_allowed: bool,
) -> PushSetupResponse {
    PushSetupResponse {
        revision: row.map_or(0, |r| r.revision),
        preferences: PushPreferences {
            enabled: row.is_some_and(|r| r.enabled),
            alerts: row.is_some_and(|r| r.alerts),
            security: row.is_some_and(|r| r.security),
            task_failure: row.is_some_and(|r| r.task_failure),
            task_success: row.is_some_and(|r| r.task_success),
        },
        security_allowed,
        tasks_allowed: security_allowed,
        task_failure_available: true,
        registered: row.is_some_and(|r| {
            r.enabled
                && r.grant_token.is_some()
                && r.content_key.is_some()
                && r.grant_expires_at.is_some_and(|e| e > Utc::now())
        }),
        grant_expires_at: row.and_then(|r| r.grant_expires_at),
        relay_url: relay_url.to_owned(),
        delivery_available: true,
        test_available: row
            .is_some_and(|r| r.enabled && r.content_key.is_some() && r.grant_token.is_some()),
    }
}

async fn inspect_grant(url: &str, secret: &str) -> Result<RelayGrant, AppError> {
    let url = url.trim_end_matches('/');
    if !(url.starts_with("https://") || url.starts_with("http://127.0.0.1:")) {
        return Err(AppError::BadRequest(
            "Verified push relay is not configured".into(),
        ));
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let res = client
        .post(format!("{url}/v1/grants/inspect"))
        .bearer_auth(secret)
        .send()
        .await
        .map_err(|_| {
            AppError::BadRequest("Relay verification is unavailable; retry setup".into())
        })?;
    if !res.status().is_success() {
        return Err(AppError::Forbidden(
            "Relay rejected the device grant".into(),
        ));
    }
    let grant: RelayGrant = res
        .json()
        .await
        .map_err(|_| AppError::BadRequest("Invalid relay response".into()))?;
    Ok(grant)
}

async fn relay_confirmed(row: &registration::Model, url: &str) -> bool {
    let Some(secret) = row.grant_token.as_deref() else {
        return false;
    };
    let Ok(grant) = inspect_grant(url, secret).await else {
        return false;
    };
    grant.expires_at > Utc::now().timestamp()
        && Some(grant.grant_id.as_str()) == row.grant_id.as_deref()
        && Some(grant.key_id.as_str()) == row.key_id.as_deref()
        && Some(grant.device_token.as_str()) == row.device_token.as_deref()
        && Some(grant.environment.as_str()) == row.environment.as_deref()
}

/// Installation identifiers never confer ownership. Login/session proof does.
async fn owned_row(
    txn: &sea_orm::DatabaseTransaction,
    installation: &str,
    owner: &str,
    session: &str,
) -> Result<Option<registration::Model>, AppError> {
    let row = registration::Entity::find_by_id(installation)
        .one(txn)
        .await?;
    if row
        .as_ref()
        .is_some_and(|r| r.user_id != owner || r.mobile_session_id != session)
    {
        return Err(AppError::Forbidden(
            "This installation belongs to another login".into(),
        ));
    }
    Ok(row)
}

fn check_revision(row: Option<&registration::Model>, expected: i64) -> Result<(), AppError> {
    if row.map_or(0, |r| r.revision) != expected {
        return Err(AppError::Conflict(
            "Notification settings changed; reload before saving".into(),
        ));
    }
    Ok(())
}

#[utoipa::path(get, path = "/api/mobile/push/settings", tag = "mobile-auth", responses((status = 200, body = PushSetupResponse)), security(("bearer_token" = [])))]
pub async fn settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<PushSetupResponse>>, AppError> {
    let token = extract_bearer(&headers).ok_or(AppError::Unauthorized)?;
    let txn = state.db.begin().await?;
    let (session, mobile) = push_session(&txn, &token).await?;
    let row = owned_row(&txn, &mobile.installation_id, &session.user_id, &mobile.id).await?;
    let snapshot = row;
    txn.commit().await?;
    // A locally unexpired grant can already be revoked by Relay rotation. Never
    // report confirmation from the database alone, including after a restart.
    let verified = if let Some(row) = snapshot.as_ref() {
        response(Some(row), &state.config.push_relay.url, false).registered
            && relay_confirmed(row, &state.config.push_relay.url).await
    } else {
        false
    };
    let txn = state.db.begin().await?;
    let (session, mobile) = push_session(&txn, &token).await?;
    let current = owned_row(&txn, &mobile.installation_id, &session.user_id, &mobile.id).await?;
    let owner = user::Entity::find_by_id(&session.user_id)
        .one(&txn)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let mut result = response(
        current.as_ref(),
        &state.config.push_relay.url,
        owner.role == "admin",
    );
    result.registered &= verified
        && current
            .as_ref()
            .zip(snapshot.as_ref())
            .is_some_and(|(a, b)| a.revision == b.revision && a.grant_token == b.grant_token);
    result.test_available &= result.registered;
    txn.commit().await?;
    ok(result)
}

#[utoipa::path(put, path = "/api/mobile/push/settings", tag = "mobile-auth", request_body = PushPreferencesRequest, responses((status = 200, body = PushSetupResponse), (status = 403, description = "Category or installation forbidden"), (status = 409, description = "Stale revision")), security(("bearer_token" = [])))]
pub async fn save_preferences(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<PushPreferencesRequest>,
) -> Result<Json<ApiResponse<PushSetupResponse>>, AppError> {
    let token = extract_bearer(&headers).ok_or(AppError::Unauthorized)?;
    let txn = state.db.begin().await?;
    let (session, mobile) = push_session(&txn, &token).await?;
    let row = owned_row(&txn, &mobile.installation_id, &session.user_id, &mobile.id).await?;
    check_revision(row.as_ref(), body.expected_revision)?;
    let owner = user::Entity::find_by_id(&session.user_id)
        .one(&txn)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if body.preferences.security && owner.role != "admin" {
        return Err(AppError::Forbidden(
            "Security notifications require an administrator".into(),
        ));
    }
    if (body.preferences.task_failure || body.preferences.task_success) && owner.role != "admin" {
        return Err(AppError::Forbidden(
            "Task notifications require an administrator".into(),
        ));
    }
    let exists = row.is_some();
    let mut model: registration::ActiveModel = if let Some(row) = row {
        row.into()
    } else {
        registration::ActiveModel {
            installation_id: Set(mobile.installation_id),
            user_id: Set(session.user_id),
            mobile_session_id: Set(mobile.id),
            device_token: Set(None),
            environment: Set(None),
            key_id: Set(None),
            grant_id: Set(None),
            grant_token: Set(None),
            grant_expires_at: Set(None),
            content_key_id: Set(None),
            content_key: Set(None),
            deployment_id: Set(None),
            ..Default::default()
        }
    };
    model.revision = Set(body.expected_revision + 1);
    model.enabled = Set(body.preferences.enabled);
    model.alerts = Set(body.preferences.alerts);
    model.security = Set(body.preferences.security);
    model.task_failure = Set(body.preferences.task_failure);
    model.task_success = Set(body.preferences.task_success);
    model.updated_at = Set(Utc::now());
    // Disabling stops Server delivery immediately. Re-enabling needs new proof.
    if !body.preferences.enabled {
        model.grant_token = Set(None);
        model.grant_id = Set(None);
        model.grant_expires_at = Set(None);
    }
    if exists {
        model.update(&txn).await?;
    } else {
        model.insert(&txn).await?;
    }
    txn.commit().await?;
    // Reconcile the actual grant and revalidate the session after network work.
    settings(State(state), headers).await
}

#[utoipa::path(post, path = "/api/mobile/push/verified-register", tag = "mobile-auth", request_body = VerifiedPushRequest, responses((status = 200, body = PushSetupResponse), (status = 403, description = "Unverified or mismatched grant"), (status = 409, description = "Stale revision")), security(("bearer_token" = [])))]
pub async fn verified_register(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<VerifiedPushRequest>,
) -> Result<Json<ApiResponse<PushSetupResponse>>, AppError> {
    let token = extract_bearer(&headers).ok_or(AppError::Unauthorized)?;
    if !matches!(body.environment.as_str(), "sandbox" | "production")
        || body.device_token.len() != 64
        || !body.device_token.bytes().all(|b| b.is_ascii_hexdigit())
        || body.grant_token.len() > 256
    {
        return Err(AppError::Validation(
            "Invalid device token, environment or grant".into(),
        ));
    }
    let content_key = STANDARD
        .decode(&body.content_key)
        .map_err(|_| AppError::Validation("Invalid content key".into()))?;
    if content_key.len() != 32
        || uuid::Uuid::parse_str(&body.content_key_id).is_err()
        || body.deployment_id.len() > 512
        || !(body.deployment_id.starts_with("https://")
            || body.deployment_id.starts_with("http://127.0.0.1:"))
    {
        return Err(AppError::Validation(
            "Invalid content key or deployment identity".into(),
        ));
    }
    // Authenticate before network work, then revalidate under SQLite's writer lock.
    let txn = state.db.begin().await?;
    let (session, mobile) = push_session(&txn, &token).await?;
    let original_session = mobile.id.clone();
    let row = owned_row(&txn, &mobile.installation_id, &session.user_id, &mobile.id).await?;
    check_revision(row.as_ref(), body.expected_revision)?;
    if !row.is_some_and(|r| r.enabled) {
        return Err(AppError::Forbidden("Enable notifications first".into()));
    }
    txn.commit().await?;
    let url = state.config.push_relay.url.trim_end_matches('/');
    let grant = inspect_grant(url, &body.grant_token).await?;
    let expires = DateTime::from_timestamp(grant.expires_at, 0)
        .filter(|e| *e > Utc::now())
        .ok_or_else(|| AppError::Forbidden("Expired relay grant".into()))?;
    if grant.grant_id != body.grant_id
        || grant.key_id != body.key_id
        || grant.device_token != body.device_token
        || grant.environment != body.environment
    {
        return Err(AppError::Forbidden(
            "Relay grant does not match this device".into(),
        ));
    }
    let txn = state.db.begin().await?;
    let (session, mobile) = push_session(&txn, &token).await?;
    if mobile.id != original_session {
        return Err(AppError::Unauthorized);
    }
    let row = owned_row(&txn, &mobile.installation_id, &session.user_id, &mobile.id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    check_revision(Some(&row), body.expected_revision)?;
    if !row.enabled {
        return Err(AppError::Forbidden("Notifications are disabled".into()));
    }
    let owner = user::Entity::find_by_id(&session.user_id)
        .one(&txn)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let mut model: registration::ActiveModel = row.into();
    model.revision = Set(body.expected_revision + 1);
    model.device_token = Set(Some(body.device_token));
    model.environment = Set(Some(body.environment));
    model.key_id = Set(Some(body.key_id));
    model.grant_id = Set(Some(body.grant_id));
    model.grant_token = Set(Some(body.grant_token));
    model.grant_expires_at = Set(Some(expires));
    model.content_key_id = Set(Some(body.content_key_id));
    model.content_key = Set(Some(body.content_key));
    model.deployment_id = Set(Some(body.deployment_id));
    model.updated_at = Set(Utc::now());
    // A durable migration marker survives disabling, unregister and session
    // revocation. It prevents an old app from restoring plaintext delivery.
    txn.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "INSERT OR IGNORE INTO mobile_push_migrations (installation_id, user_id) VALUES (?, ?)",
        [
            mobile.installation_id.clone().into(),
            session.user_id.clone().into(),
        ],
    ))
    .await?;
    crate::entity::device_token::Entity::delete_many()
        .filter(crate::entity::device_token::Column::InstallationId.eq(&mobile.installation_id))
        .filter(crate::entity::device_token::Column::UserId.eq(&session.user_id))
        .exec(&txn)
        .await?;
    let row = model.update(&txn).await?;
    let result = response(Some(&row), url, owner.role == "admin");
    txn.commit().await?;
    ok(result)
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TestPushRequest {
    /// Client-generated UUID retained when retrying the same logical test.
    pub event_id: String,
    pub expected_revision: i64,
}

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TestPushOutcome {
    Pending,
    Accepted,
    Retryable,
    Permanent,
    Expired,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TestPushResponse {
    pub event_id: String,
    pub outcome: TestPushOutcome,
    pub reason: String,
    /// APNs acceptance cannot establish native presentation.
    pub presentation: String,
}

fn test_response(job: &outbox::Model) -> TestPushResponse {
    let expired = matches!(job.outcome.as_str(), "pending" | "retryable")
        && job.expires_at <= Utc::now().timestamp();
    TestPushResponse {
        event_id: job.event_id.clone(),
        outcome: if expired {
            TestPushOutcome::Expired
        } else {
            match job.outcome.as_str() {
                "accepted" => TestPushOutcome::Accepted,
                "retryable" => TestPushOutcome::Retryable,
                "permanent" => TestPushOutcome::Permanent,
                "expired" => TestPushOutcome::Expired,
                _ => TestPushOutcome::Pending,
            }
        },
        reason: if expired {
            "Expired".into()
        } else {
            job.reason.clone()
        },
        presentation: "unobserved".into(),
    }
}

#[utoipa::path(get, path = "/api/mobile/push/test/{event_id}", tag = "mobile-auth", params(("event_id" = String, Path, description = "Logical test UUID")), responses((status = 200, body = TestPushResponse), (status = 404, description = "Test unavailable for this login")), security(("bearer_token" = [])))]
pub async fn test_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(event_id): Path<String>,
) -> Result<Json<ApiResponse<TestPushResponse>>, AppError> {
    let token = extract_bearer(&headers).ok_or(AppError::Unauthorized)?;
    let txn = state.db.begin().await?;
    let (session, mobile) = push_session(&txn, &token).await?;
    let job = outbox::Entity::find_by_id((event_id, mobile.installation_id))
        .filter(outbox::Column::UserId.eq(session.user_id))
        .filter(outbox::Column::MobileSessionId.eq(mobile.id))
        .one(&txn)
        .await?
        .ok_or_else(|| AppError::NotFound("Test notification unavailable".into()))?;
    let result = test_response(&job);
    txn.commit().await?;
    ok(result)
}

#[utoipa::path(post, path = "/api/mobile/push/test", operation_id = "test_mobile_push", tag = "mobile-auth", request_body = TestPushRequest, responses((status = 200, body = TestPushResponse), (status = 403, description = "Setup unavailable"), (status = 409, description = "Stale revision")), security(("bearer_token" = [])))]
pub async fn test_notification(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<TestPushRequest>,
) -> Result<Json<ApiResponse<TestPushResponse>>, AppError> {
    use crate::service::push_envelope::{PushContent, encrypt};
    let event_id = uuid::Uuid::parse_str(&body.event_id)
        .map_err(|_| AppError::Validation("Invalid test event UUID".into()))?
        .to_string();
    let token = extract_bearer(&headers).ok_or(AppError::Unauthorized)?;
    let txn = state.db.begin().await?;
    let (session, mobile) = push_session(&txn, &token).await?;
    if let Some(job) =
        outbox::Entity::find_by_id((event_id.clone(), mobile.installation_id.clone()))
            .one(&txn)
            .await?
    {
        if job.user_id != session.user_id || job.mobile_session_id != mobile.id {
            return Err(AppError::Forbidden(
                "This test belongs to another login".into(),
            ));
        }
        if job.registration_revision != body.expected_revision {
            return Err(AppError::Conflict(
                "Test identity already used with another revision".into(),
            ));
        }
        let result = test_response(&job);
        txn.commit().await?;
        return ok(result);
    }
    let row = owned_row(&txn, &mobile.installation_id, &session.user_id, &mobile.id)
        .await?
        .ok_or_else(|| AppError::Forbidden("Enable notifications first".into()))?;
    check_revision(Some(&row), body.expected_revision)?;
    if !response(Some(&row), "", false).registered {
        return Err(AppError::Forbidden("Retry notification setup first".into()));
    }
    let owner = user::Entity::find_by_id(&session.user_id)
        .one(&txn)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if !matches!(owner.role.as_str(), "admin" | "member") {
        return Err(AppError::Forbidden("Account role unavailable".into()));
    }
    let unavailable = || AppError::Forbidden("Content key is not registered".into());
    let key_id = row.content_key_id.as_deref().ok_or_else(unavailable)?;
    let secret = STANDARD
        .decode(row.content_key.as_deref().ok_or_else(unavailable)?)
        .map_err(|_| unavailable())?;
    let now = Utc::now().timestamp();
    let content = PushContent {
        kind: "test".into(),
        deployment_id: row.deployment_id.clone().ok_or_else(unavailable)?,
        user_id: session.user_id.clone(),
        installation_id: mobile.installation_id.clone(),
        event_id: event_id.clone(),
        created_at: now,
        expires_at: now + 1800,
        task_run: None,
        alert: None,
        server_id: None,
        security_event_id: None,
        security_event_type: None,
    };
    let envelope = serde_json::to_string(&encrypt(key_id, &secret, &content)?)
        .map_err(|_| AppError::Internal("Push encryption failed".into()))?;
    // The writer lock from push_session serializes duplicate admission. No Relay
    // round trip is required to persist locally eligible work during an outage.
    let job = outbox::ActiveModel {
        event_id: Set(event_id),
        installation_id: Set(mobile.installation_id),
        user_id: Set(session.user_id),
        mobile_session_id: Set(mobile.id),
        registration_revision: Set(row.revision),
        recipient_role: Set(owner.role),
        task_run_id: Set(None),
        category: Set("test".into()),
        created_at: Set(now),
        expires_at: Set(now + 1800),
        envelope: Set(Some(envelope)),
        outcome: Set("pending".into()),
        reason: Set("Queued".into()),
        attempts: Set(0),
        next_attempt_at: Set(now),
        lease_id: Set(None),
        lease_until: Set(0),
    }
    .insert(&txn)
    .await?;
    let result = test_response(&job);
    txn.commit().await?;
    ok(result)
}
