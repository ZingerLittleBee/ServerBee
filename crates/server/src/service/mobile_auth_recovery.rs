//! Account verification permits deletion of one captured mobile identity only.
//! Expired credentials identify that login; they never authenticate or mint tokens.

use std::collections::BTreeSet;

use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction, EntityTrait,
    QueryFilter, QuerySelect, TransactionTrait,
};
use serde::Serialize;
use uuid::Uuid;

use crate::entity::{
    device_token, mobile_push_registration, mobile_session, mobile_session_revocation_proof,
    session, user,
};
use crate::error::AppError;
use crate::service::{auth::AuthService, mobile_auth::MobileAuthService};

const MAX_RECOVERY_CANDIDATES: u64 = 64;

pub struct MobileRecoveryParams<'a> {
    pub username: &'a str,
    pub password: &'a str,
    pub totp_code: Option<&'a str>,
    pub expected_user_id: &'a str,
    pub installation_id: &'a str,
    pub expected_session_id: Option<&'a str>,
    pub access_token: &'a str,
    pub refresh_token: &'a str,
    pub revocation_token: Option<&'a str>,
}

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MobileRecoveryOutcome {
    Ok,
    AlreadyAbsent,
    SelectionRequired,
}

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct MobileRecoveryCandidate {
    pub mobile_session_id: String,
    pub device_name: String,
    pub created_at: String,
    pub last_used_at: String,
}

#[derive(Clone, Serialize, utoipa::ToSchema)]
pub struct MobileRecoveryResponse {
    pub outcome: MobileRecoveryOutcome,
    pub user_id: String,
    pub installation_id: String,
    pub mobile_session_id: Option<String>,
    pub candidates: Vec<MobileRecoveryCandidate>,
    /// Five-minute cleanup-only grant for QR recovery. Never a login token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_token: Option<String>,
}

pub async fn recover(
    db: &DatabaseConnection,
    params: MobileRecoveryParams<'_>,
) -> Result<MobileRecoveryResponse, AppError> {
    let verified_user = MobileAuthService::validate_credentials(
        db,
        params.username,
        params.password,
        params.totp_code,
    )
    .await?;
    recover_for_verified_user(db, params, verified_user, None, false).await
}

/// The pairing-code/grant adapter supplies account authority, never mobile tokens.
pub(crate) async fn recover_for_verified_user(
    db: &DatabaseConnection,
    params: MobileRecoveryParams<'_>,
    verified_user: user::Model,
    verified_until: Option<chrono::DateTime<chrono::Utc>>,
    absence_only: bool,
) -> Result<MobileRecoveryResponse, AppError> {
    let expected = normalize_session_id(params.expected_session_id)?;
    if verified_user.id != params.expected_user_id {
        return Err(AppError::Unauthorized);
    }
    ensure_mobile_policy(&verified_user)?;

    // Argon2 runs before acquiring SQLite's writer lock. Both the identity and
    // exact hash are reread below; a rotated secret cannot authorize its successor.
    let mut refresh_matches = Vec::new();
    if !params.refresh_token.is_empty() {
        let candidates = mobile_session::Entity::find()
            .filter(mobile_session::Column::InstallationId.eq(params.installation_id))
            .limit(MAX_RECOVERY_CANDIDATES + 1)
            .all(db)
            .await?;
        if candidates.len() > MAX_RECOVERY_CANDIDATES as usize {
            return Err(capacity_exceeded());
        }
        for candidate in candidates {
            if MobileAuthService::verify_refresh_token(
                params.refresh_token,
                &candidate.refresh_token_hash,
            )? {
                refresh_matches.push(candidate);
            }
        }
    }

    recover_validated(
        db,
        params,
        expected,
        verified_user,
        refresh_matches,
        verified_until,
        absence_only,
    )
    .await
}

pub(crate) fn normalize_session_id(id: Option<&str>) -> Result<Option<String>, AppError> {
    id.map(|id| {
        Uuid::parse_str(id)
            .map(|id| id.to_string())
            .map_err(|_| AppError::Validation("Invalid expected_session_id".into()))
    })
    .transpose()
}

pub(crate) async fn recheck_verified_user(
    txn: &DatabaseTransaction,
    verified_user: &user::Model,
) -> Result<(), AppError> {
    let current = user::Entity::find_by_id(&verified_user.id)
        .one(txn)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if current.password_hash != verified_user.password_hash
        || current.totp_secret != verified_user.totp_secret
        || current.password_changed_at != verified_user.password_changed_at
        || current.username != verified_user.username
        || current.role != verified_user.role
    {
        return Err(AppError::Unauthorized);
    }
    ensure_mobile_policy(&current)
}

/// Keep slow validation outside the writer transaction, then recheck its
/// snapshots and make every storage-dependent authorization decision together.
async fn recover_validated(
    db: &DatabaseConnection,
    params: MobileRecoveryParams<'_>,
    expected: Option<String>,
    verified_user: user::Model,
    refresh_matches: Vec<mobile_session::Model>,
    verified_until: Option<chrono::DateTime<chrono::Utc>>,
    absence_only: bool,
) -> Result<MobileRecoveryResponse, AppError> {
    let txn = db.begin().await?;
    txn.execute_unprepared("UPDATE mobile_sessions SET id = id WHERE 0")
        .await?;
    if verified_until.is_some_and(|deadline| chrono::Utc::now() >= deadline) {
        return Err(AppError::Unauthorized);
    }
    // Reauthentication must fail if password, TOTP or onboarding policy changed
    // while the expensive credential verification was in flight.
    recheck_verified_user(&txn, &verified_user).await?;

    let identities = resolve_captured_identities(&txn, &params, refresh_matches).await?;
    if absence_only && !identities.is_empty() {
        return Err(unresolved());
    }
    let target_id = if let Some(expected) = expected {
        if identities.iter().any(|id| id != &expected) {
            return Err(AppError::Unauthorized);
        }
        match mobile_session::Entity::find_by_id(&expected)
            .one(&txn)
            .await?
        {
            Some(target) => {
                ensure_owner(&target, &params)?;
                Some(expected)
            }
            None => {
                if MobileAuthService::has_operational_mobile_state(&txn, &expected).await? {
                    return Err(unresolved());
                }
                txn.commit().await?;
                return Ok(response(
                    &params,
                    Some(expected),
                    MobileRecoveryOutcome::AlreadyAbsent,
                ));
            }
        }
    } else {
        if identities.len() > 1 {
            return Err(unresolved());
        }
        match identities.into_iter().next() {
            Some(id) => Some(id),
            None => {
                let candidates = recovery_candidates(&txn, &params).await?;
                if !candidates.is_empty() {
                    // Even a single candidate requires explicit user selection.
                    // Reauthentication may inspect this installation's rows; it
                    // must never silently choose or delete a replacement login.
                    txn.commit().await?;
                    let mut result =
                        response(&params, None, MobileRecoveryOutcome::SelectionRequired);
                    result.candidates = candidates;
                    return Ok(result);
                }
                txn.commit().await?;
                return Ok(response(
                    &params,
                    None,
                    MobileRecoveryOutcome::AlreadyAbsent,
                ));
            }
        }
    };

    let target_id = target_id.ok_or_else(unresolved)?;
    device_token::Entity::delete_many()
        .filter(device_token::Column::MobileSessionId.eq(&target_id))
        .exec(&txn)
        .await?;
    session::Entity::delete_many()
        .filter(session::Column::MobileSessionId.eq(&target_id))
        .exec(&txn)
        .await?;
    // Explicit removal also handles disabled registrations and historical proofs.
    // Outbox records are not independent authority and retain their expiry policy.
    mobile_push_registration::Entity::delete_many()
        .filter(mobile_push_registration::Column::MobileSessionId.eq(&target_id))
        .exec(&txn)
        .await?;
    mobile_session_revocation_proof::Entity::delete_many()
        .filter(mobile_session_revocation_proof::Column::MobileSessionId.eq(&target_id))
        .exec(&txn)
        .await?;
    if mobile_session::Entity::delete_by_id(&target_id)
        .exec(&txn)
        .await?
        .rows_affected
        != 1
    {
        return Err(unresolved());
    }
    txn.commit().await?;
    Ok(response(
        &params,
        Some(target_id),
        MobileRecoveryOutcome::Ok,
    ))
}

async fn resolve_captured_identities(
    txn: &DatabaseTransaction,
    params: &MobileRecoveryParams<'_>,
    refresh_matches: Vec<mobile_session::Model>,
) -> Result<BTreeSet<String>, AppError> {
    let mut identities = BTreeSet::new();
    if !params.access_token.is_empty() {
        let access = session::Entity::find()
            .filter(session::Column::Token.eq(AuthService::hash_session_token(params.access_token)))
            .all(txn)
            .await?;
        for access in access {
            if access.user_id != params.expected_user_id || access.source != "mobile" {
                return Err(AppError::Unauthorized);
            }
            let id = access.mobile_session_id.ok_or_else(unresolved)?;
            add_identity(txn, params, &mut identities, id).await?;
        }
    }
    // Current and consumed refresh secrets may also have deletion-only
    // history. Consult both captured secrets, regardless of their expiry.
    for secret in [Some(params.refresh_token), params.revocation_token]
        .into_iter()
        .flatten()
        .filter(|secret| !secret.is_empty())
    {
        let hash = AuthService::hash_session_token(secret);
        for target in mobile_session::Entity::find()
            .filter(mobile_session::Column::RevocationTokenHash.eq(&hash))
            .all(txn)
            .await?
        {
            ensure_owner(&target, params)?;
            identities.insert(target.id);
        }
        for proof in mobile_session_revocation_proof::Entity::find()
            .filter(mobile_session_revocation_proof::Column::TokenHash.eq(&hash))
            .all(txn)
            .await?
        {
            add_identity(txn, params, &mut identities, proof.mobile_session_id).await?;
        }
    }
    for verified in refresh_matches {
        if let Some(current) = mobile_session::Entity::find_by_id(&verified.id)
            .one(txn)
            .await?
        {
            ensure_owner(&current, params)?;
            if current.refresh_token_hash == verified.refresh_token_hash {
                identities.insert(current.id);
            }
        }
    }
    Ok(identities)
}

fn ensure_mobile_policy(user: &user::Model) -> Result<(), AppError> {
    if user.must_change_password {
        return Err(AppError::Forbidden(
            "MUST_CHANGE_PASSWORD: complete onboarding via the web UI before using mobile".into(),
        ));
    }
    Ok(())
}

fn ensure_owner(
    target: &mobile_session::Model,
    params: &MobileRecoveryParams<'_>,
) -> Result<(), AppError> {
    if target.user_id != params.expected_user_id || target.installation_id != params.installation_id
    {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

async fn add_identity(
    txn: &DatabaseTransaction,
    params: &MobileRecoveryParams<'_>,
    identities: &mut BTreeSet<String>,
    id: String,
) -> Result<(), AppError> {
    let target = mobile_session::Entity::find_by_id(&id)
        .one(txn)
        .await?
        .ok_or_else(unresolved)?;
    ensure_owner(&target, params)?;
    identities.insert(id);
    Ok(())
}

async fn recovery_candidates(
    txn: &DatabaseTransaction,
    params: &MobileRecoveryParams<'_>,
) -> Result<Vec<MobileRecoveryCandidate>, AppError> {
    if has_unattributed_mobile_access(txn, params.expected_user_id).await? {
        return Err(unresolved());
    }
    let candidates = mobile_session::Entity::find()
        .filter(mobile_session::Column::UserId.eq(params.expected_user_id))
        .filter(mobile_session::Column::InstallationId.eq(params.installation_id))
        .limit(MAX_RECOVERY_CANDIDATES + 1)
        .all(txn)
        .await?;
    if candidates.len() > MAX_RECOVERY_CANDIDATES as usize {
        return Err(capacity_exceeded());
    }
    let identities: BTreeSet<_> = candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect();
    // An orphan or incorrectly linked registration cannot be assigned to an
    // offered identity, so selection cannot establish cleanup of that state.
    for device in device_token::Entity::find()
        .filter(device_token::Column::UserId.eq(params.expected_user_id))
        .filter(device_token::Column::InstallationId.eq(params.installation_id))
        .all(txn)
        .await?
    {
        if !identities.contains(device.mobile_session_id.as_str()) {
            return Err(unresolved());
        }
    }
    for registration in mobile_push_registration::Entity::find()
        .filter(mobile_push_registration::Column::UserId.eq(params.expected_user_id))
        .filter(mobile_push_registration::Column::InstallationId.eq(params.installation_id))
        .all(txn)
        .await?
    {
        if !identities.contains(registration.mobile_session_id.as_str()) {
            return Err(unresolved());
        }
    }
    Ok(candidates
        .into_iter()
        .map(|candidate| MobileRecoveryCandidate {
            mobile_session_id: candidate.id,
            device_name: candidate.device_name,
            created_at: candidate.created_at.to_rfc3339(),
            last_used_at: candidate.last_used_at.to_rfc3339(),
        })
        .collect())
}

async fn has_unattributed_mobile_access(
    txn: &DatabaseTransaction,
    user_id: &str,
) -> Result<bool, AppError> {
    // A dangling mobile access row has no trustworthy installation boundary.
    // Fail closed for this account, while leaving ordinary linked sessions on
    // other installations independent of the recovery attempt.
    Ok(txn
        .query_one(sea_orm::Statement::from_sql_and_values(
            sea_orm::DatabaseBackend::Sqlite,
            "SELECT 1 FROM sessions s WHERE s.user_id = ? AND s.source = 'mobile' AND \
         (s.mobile_session_id IS NULL OR NOT EXISTS \
         (SELECT 1 FROM mobile_sessions m WHERE m.id = s.mobile_session_id)) LIMIT 1",
            [user_id.to_string().into()],
        ))
        .await?
        .is_some())
}

fn capacity_exceeded() -> AppError {
    AppError::Conflict("Too many mobile sessions share this installation to recover safely".into())
}

fn unresolved() -> AppError {
    AppError::Conflict(
        "The original mobile session cannot be safely identified or confirmed absent".into(),
    )
}

fn response(
    params: &MobileRecoveryParams<'_>,
    mobile_session_id: Option<String>,
    outcome: MobileRecoveryOutcome,
) -> MobileRecoveryResponse {
    MobileRecoveryResponse {
        outcome,
        user_id: params.expected_user_id.to_string(),
        installation_id: params.installation_id.to_string(),
        mobile_session_id,
        candidates: Vec::new(),
        recovery_token: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ActiveModelTrait, Set};

    #[tokio::test]
    async fn credential_change_after_validation_preserves_original_session() {
        for mutation in ["password", "totp", "expired_grant"] {
            let (db, _tmp) = crate::test_utils::setup_test_db().await;
            let account = AuthService::create_user(&db, "alice", "old-password", "member")
                .await
                .unwrap();
            let tokens = MobileAuthService::login_for_user(
                &db,
                &crate::config::MobileConfig::default(),
                &account,
                "installation",
                "phone",
                "",
                "",
            )
            .await
            .unwrap();
            let snapshot =
                MobileAuthService::validate_credentials(&db, "alice", "old-password", None)
                    .await
                    .unwrap();
            // The transaction boundary accepts the snapshot produced by actual
            // authentication. Change durable policy before entering that same
            // production boundary, without sleeps or a test-only hook.
            let mut current: user::ActiveModel = account.clone().into();
            if mutation == "password" {
                current.password_hash = Set(AuthService::hash_password("new-password").unwrap());
            } else if mutation == "totp" {
                current.totp_secret =
                    Set(Some(AuthService::generate_totp_secret("alice").unwrap().0));
            }
            if mutation != "expired_grant" {
                current.update(&db).await.unwrap();
            }
            // A QR grant expiring while awaiting the writer must also fail
            // inside the production transaction, independently of map lookup.
            let deadline = (mutation == "expired_grant")
                .then(|| chrono::Utc::now() - chrono::Duration::minutes(1));
            let result = recover_validated(
                &db,
                MobileRecoveryParams {
                    username: "alice",
                    password: "old-password",
                    totp_code: None,
                    expected_user_id: &account.id,
                    installation_id: "installation",
                    expected_session_id: Some(&tokens.mobile_session_id),
                    access_token: &tokens.access_token,
                    refresh_token: &tokens.refresh_token,
                    revocation_token: tokens.revocation_token.as_deref(),
                },
                Some(tokens.mobile_session_id.clone()),
                snapshot,
                Vec::new(),
                deadline,
                false,
            )
            .await;
            assert!(matches!(result, Err(AppError::Unauthorized)));
            assert!(
                mobile_session::Entity::find_by_id(&tokens.mobile_session_id)
                    .one(&db)
                    .await
                    .unwrap()
                    .is_some()
            );
            assert!(
                session::Entity::find()
                    .filter(session::Column::MobileSessionId.eq(&tokens.mobile_session_id))
                    .one(&db)
                    .await
                    .unwrap()
                    .is_some()
            );
        }
    }
}
