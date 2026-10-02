//! Current-installation subscription intent and verified relay setup.
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::HeaderMap,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use sea_orm::{ActiveModelTrait, EntityTrait, Set, TransactionTrait};
use serde::{Deserialize, Serialize};

use super::mobile::{extract_bearer, push_session};
use crate::{
    entity::{mobile_push_registration as registration, user},
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
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PushSetupResponse {
    pub revision: i64,
    pub preferences: PushPreferences,
    pub registered: bool,
    pub grant_expires_at: Option<DateTime<Utc>>,
    pub relay_url: String,
    /// Setup ships before category delivery, which has its own acceptance gate.
    pub delivery_available: bool,
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
}

fn response(row: Option<&registration::Model>, relay_url: &str) -> PushSetupResponse {
    PushSetupResponse {
        revision: row.map_or(0, |r| r.revision),
        preferences: PushPreferences {
            enabled: row.is_some_and(|r| r.enabled),
            alerts: row.is_some_and(|r| r.alerts),
            security: row.is_some_and(|r| r.security),
            task_failure: row.is_some_and(|r| r.task_failure),
            task_success: row.is_some_and(|r| r.task_success),
        },
        registered: row.is_some_and(|r| {
            r.enabled
                && r.grant_token.is_some()
                && r.grant_expires_at.is_some_and(|e| e > Utc::now())
        }),
        grant_expires_at: row.and_then(|r| r.grant_expires_at),
        relay_url: relay_url.to_owned(),
        delivery_available: false,
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
        response(Some(row), &state.config.push_relay.url).registered
            && relay_confirmed(row, &state.config.push_relay.url).await
    } else {
        false
    };
    let txn = state.db.begin().await?;
    let (session, mobile) = push_session(&txn, &token).await?;
    let current = owned_row(&txn, &mobile.installation_id, &session.user_id, &mobile.id).await?;
    let mut result = response(current.as_ref(), &state.config.push_relay.url);
    result.registered &= verified
        && current
            .as_ref()
            .zip(snapshot.as_ref())
            .is_some_and(|(a, b)| a.revision == b.revision && a.grant_token == b.grant_token);
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
    let mut model: registration::ActiveModel = row.into();
    model.revision = Set(body.expected_revision + 1);
    model.device_token = Set(Some(body.device_token));
    model.environment = Set(Some(body.environment));
    model.key_id = Set(Some(body.key_id));
    model.grant_id = Set(Some(body.grant_id));
    model.grant_token = Set(Some(body.grant_token));
    model.grant_expires_at = Set(Some(expires));
    model.updated_at = Set(Utc::now());
    let row = model.update(&txn).await?;
    let result = response(Some(&row), url);
    txn.commit().await?;
    ok(result)
}
