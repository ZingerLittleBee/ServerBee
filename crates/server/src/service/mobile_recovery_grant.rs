//! Ephemeral QR authority can clean up a captured login, never establish one.
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use super::{
    auth::AuthService,
    mobile_auth_recovery::{
        self, MobileRecoveryOutcome, MobileRecoveryParams, MobileRecoveryResponse,
    },
};
use crate::{entity::user, error::AppError};

pub const RECOVERY_GRANT_TTL_SECS: i64 = 300;
pub const MAX_RECOVERY_GRANTS: usize = 128;

/// No Debug implementation: account snapshots and cleanup tokens are secrets.
#[derive(Default)]
pub struct RecoveryGrantStore {
    pub entries: Mutex<HashMap<String, Arc<RecoveryGrant>>>,
}

pub struct RecoveryGrant {
    pub created_at: DateTime<Utc>,
    account: user::Model,
    installation_id: String,
    fingerprint: [u8; 32],
    progress: Mutex<GrantProgress>,
}

#[derive(Default)]
struct GrantProgress {
    selected: Option<String>,
    offered: Option<MobileRecoveryResponse>,
    absent_without_id: bool,
}

fn fingerprint(params: &MobileRecoveryParams<'_>) -> [u8; 32] {
    let mut hash = Sha256::new();
    for secret in [
        Some(params.access_token),
        Some(params.refresh_token),
        params.revocation_token,
    ] {
        hash.update([u8::from(secret.is_some())]);
        if let Some(secret) = secret {
            hash.update((secret.len() as u64).to_be_bytes());
            hash.update(secret.as_bytes());
        }
    }
    hash.finalize().into()
}

impl RecoveryGrantStore {
    pub(crate) async fn issue_and_recover(
        &self,
        db: &DatabaseConnection,
        params: MobileRecoveryParams<'_>,
        account: user::Model,
    ) -> Result<MobileRecoveryResponse, AppError> {
        if account.id != params.expected_user_id {
            return Err(AppError::Unauthorized);
        }
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let token = format!("sb_recover_{}", URL_SAFE_NO_PAD.encode(bytes));
        let key = AuthService::hash_session_token(&token);
        let grant = Arc::new(RecoveryGrant {
            created_at: Utc::now(),
            account,
            installation_id: params.installation_id.to_owned(),
            fingerprint: fingerprint(&params),
            progress: Mutex::new(GrantProgress::default()),
        });
        {
            let mut entries = self.entries.lock().await;
            entries.retain(|_, grant| Utc::now() < grant.expires_at());
            if entries.len() >= MAX_RECOVERY_GRANTS || entries.contains_key(&key) {
                return Err(AppError::TooManyRequests(
                    "Too many recovery attempts. Please try again later.".into(),
                ));
            }
            entries.insert(key.clone(), grant.clone());
        }
        match grant.recover(db, params).await {
            Ok(mut response) => {
                response.recovery_token = Some(token);
                Ok(response)
            }
            Err(error) => {
                self.entries.lock().await.remove(&key);
                Err(error)
            }
        }
    }

    pub(crate) async fn recover(
        &self,
        db: &DatabaseConnection,
        params: MobileRecoveryParams<'_>,
        token: &str,
    ) -> Result<MobileRecoveryResponse, AppError> {
        let grant = {
            let mut entries = self.entries.lock().await;
            entries.retain(|_, grant| Utc::now() < grant.expires_at());
            entries
                .get(&AuthService::hash_session_token(token))
                .cloned()
                .ok_or(AppError::Unauthorized)?
        };
        let mut response = grant.recover(db, params).await?;
        response.recovery_token = Some(token.to_owned());
        Ok(response)
    }
}

impl RecoveryGrant {
    fn expires_at(&self) -> DateTime<Utc> {
        self.created_at + Duration::seconds(RECOVERY_GRANT_TTL_SECS)
    }

    async fn recover(
        &self,
        db: &DatabaseConnection,
        params: MobileRecoveryParams<'_>,
    ) -> Result<MobileRecoveryResponse, AppError> {
        let mut progress = self.progress.lock().await;
        if Utc::now() >= self.expires_at()
            || self.account.id != params.expected_user_id
            || self.installation_id != params.installation_id
            || self.fingerprint != fingerprint(&params)
        {
            return Err(AppError::Unauthorized);
        }
        let requested = mobile_auth_recovery::normalize_session_id(params.expected_session_id)?;
        if let Some(selected) = &progress.selected {
            if requested.as_ref().is_some_and(|id| id != selected) {
                return Err(AppError::Unauthorized);
            }
        } else if let Some(requested) = requested {
            if progress.absent_without_id {
                return Err(AppError::Unauthorized);
            }
            if let Some(offered) = &progress.offered {
                let ids: BTreeSet<_> = offered
                    .candidates
                    .iter()
                    .map(|candidate| &candidate.mobile_session_id)
                    .collect();
                if !ids.contains(&requested) {
                    return Err(AppError::Unauthorized);
                }
            }
            // Pin before dispatch, including failed/lost-response retries. Never broaden later.
            progress.selected = Some(requested);
        } else if let Some(offered) = &progress.offered {
            // Repeat only the originally offered candidates. Account state and TTL
            // are checked under the same writer lock used by actual deletion.
            let txn = db.begin().await?;
            txn.execute_unprepared("UPDATE mobile_sessions SET id = id WHERE 0")
                .await?;
            if Utc::now() >= self.expires_at() {
                return Err(AppError::Unauthorized);
            }
            mobile_auth_recovery::recheck_verified_user(&txn, &self.account).await?;
            txn.commit().await?;
            return Ok(offered.clone());
        }
        let selected = progress.selected.clone();
        let params = MobileRecoveryParams {
            expected_session_id: selected.as_deref(),
            ..params
        };
        // These borrowed parameters live only across this call, and the selected
        // ID is owned locally while the per-grant mutex prevents concurrent drift.
        let result = mobile_auth_recovery::recover_for_verified_user(
            db,
            params,
            self.account.clone(),
            Some(self.expires_at()),
            progress.absent_without_id,
        )
        .await?;
        if progress.absent_without_id
            && !matches!(result.outcome, MobileRecoveryOutcome::AlreadyAbsent)
        {
            return Err(AppError::Conflict(
                "New state requires a fresh recovery QR code".into(),
            ));
        }
        match result.outcome {
            MobileRecoveryOutcome::SelectionRequired => progress.offered = Some(result.clone()),
            MobileRecoveryOutcome::Ok | MobileRecoveryOutcome::AlreadyAbsent => {
                if let Some(id) = &result.mobile_session_id {
                    progress.selected = Some(id.clone());
                } else {
                    progress.absent_without_id = true;
                }
            }
        }
        Ok(result)
    }
}
