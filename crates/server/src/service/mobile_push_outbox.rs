//! Durable installation-scoped delivery. Network work never runs on event evaluation.
use std::{sync::Arc, time::Duration};

use chrono::Utc;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Statement, TransactionTrait,
};
use serde::Deserialize;
use tokio::task::JoinHandle;

use crate::{
    entity::{
        mobile_push_outbox as outbox, mobile_push_registration as registration, mobile_session,
        user,
    },
    error::AppError,
    state::AppState,
};

const SEND_TIMEOUT: u64 = 15;
const LEASE_SECONDS: i64 = 30;

#[derive(Deserialize)]
struct RelayDelivery {
    outcome: String,
    reason: String,
    device_invalid: bool,
}

/// Production startup entry point, also used by restart integration tests.
/// Four independent workers bound concurrency; durable leases fence overlapping
/// workers/restarts. Aborting this owner cancels its in-flight network requests.
pub fn start(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut workers = tokio::task::JoinSet::new();
        let recovery_state = state.clone();
        workers.spawn(async move {
            loop {
                if super::task_notification::recover_pending(&recovery_state)
                    .await
                    .is_err()
                {
                    tracing::warn!("Scheduled task notification recovery will retry");
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        for _ in 0..4 {
            let state = state.clone();
            workers.spawn(async move {
                loop {
                    if deliver_next(&state).await.is_err() {
                        // Do not print network bodies, credentials or ciphertext.
                        tracing::warn!("Mobile push worker could not update delivery state");
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            });
        }
        while workers.join_next().await.is_some() {}
    })
}

pub(crate) async fn eligible(
    txn: &sea_orm::DatabaseTransaction,
    job: &outbox::Model,
) -> Result<Option<registration::Model>, AppError> {
    let now = Utc::now();
    let mut task_success = None;
    if let Some(run_id) = job.task_run_id.as_deref() {
        if job.event_id != run_id {
            return Ok(None);
        }
        use crate::entity::{task, task_run};
        let run = task_run::Entity::find_by_id(run_id)
            .filter(task_run::Column::OwnerId.eq(&job.user_id))
            .filter(task_run::Column::Status.eq("completed"))
            .one(txn)
            .await?;
        let Some(run) = run else { return Ok(None) };
        let summary = run.summary_json.as_deref().and_then(|json| {
            serde_json::from_str::<super::push_envelope::TaskRunSummary>(json).ok()
        });
        let Some(summary) = summary.filter(|summary| {
            summary.task_id == run.task_id && summary.run_id == run.run_id && summary.total > 0
        }) else {
            return Ok(None);
        };
        task_success = Some(summary.is_success());
        if job.recipient_role != "admin"
            || task::Entity::find_by_id(&run.task_id)
                .filter(task::Column::TaskType.eq("scheduled"))
                .one(txn)
                .await?
                .is_none()
        {
            return Ok(None);
        }
    }
    let row = registration::Entity::find_by_id(&job.installation_id)
        .one(txn)
        .await?;
    let Some(row) = row.filter(|r| {
        r.user_id == job.user_id
            && r.mobile_session_id == job.mobile_session_id
            && r.revision == job.registration_revision
            && r.enabled
            && match (job.category.as_str(), task_success) {
                ("test", None) => true,
                ("alert", None) => r.alerts,
                // Jobs queued before the category migration retain their task
                // target and current outcome-specific subscription checks.
                ("test" | "task_failure", Some(false)) => r.task_failure,
                ("test" | "task_success", Some(true)) => r.task_success,
                _ => false,
            }
            && r.content_key.is_some()
            && r.grant_token.is_some()
            && r.grant_expires_at.is_some_and(|e| e > now)
    }) else {
        return Ok(None);
    };
    let mobile = mobile_session::Entity::find_by_id(&job.mobile_session_id)
        .filter(mobile_session::Column::UserId.eq(&job.user_id))
        .filter(mobile_session::Column::InstallationId.eq(&job.installation_id))
        .filter(mobile_session::Column::ExpiresAt.gt(now))
        .one(txn)
        .await?;
    let owner = user::Entity::find_by_id(&job.user_id).one(txn).await?;
    match (mobile, owner) {
        (Some(mobile), Some(owner))
            if !owner.must_change_password
                && owner.role == job.recipient_role
                && matches!(owner.role.as_str(), "admin" | "member")
                && owner
                    .password_changed_at
                    .is_none_or(|changed| mobile.created_at >= changed) =>
        {
            Ok(Some(row))
        }
        _ => Ok(None),
    }
}

async fn deliver_next(state: &AppState) -> Result<(), AppError> {
    let now = Utc::now().timestamp();
    // Idle workers only read. Avoid competing for SQLite's writer lock on
    // every poll when there is no due/expired work. Recheck under the lock below.
    let ready = outbox::Entity::find()
        .filter(outbox::Column::Outcome.is_in(["pending", "retryable"]))
        .filter(outbox::Column::LeaseUntil.lte(now))
        .filter(
            sea_orm::Condition::any()
                .add(outbox::Column::NextAttemptAt.lte(now))
                .add(outbox::Column::ExpiresAt.lte(now)),
        )
        .limit(1)
        .one(&state.db)
        .await?;
    if ready.is_none() {
        return Ok(());
    }
    let txn = state.db.begin().await?;
    // Take the SQLite writer lock before selection, eligibility and claim.
    txn.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite,
        "UPDATE mobile_push_outbox SET outcome='expired', reason='Expired', envelope=NULL, lease_id=NULL, lease_until=0 WHERE outcome IN ('pending','retryable') AND expires_at<=? AND lease_until<=?",
        [now.into(), now.into()])).await?;
    let job = outbox::Entity::find()
        .filter(outbox::Column::Outcome.is_in(["pending", "retryable"]))
        .filter(outbox::Column::NextAttemptAt.lte(now))
        .filter(outbox::Column::LeaseUntil.lte(now))
        .order_by_asc(outbox::Column::NextAttemptAt)
        .limit(1)
        .one(&txn)
        .await?;
    let Some(job) = job else {
        txn.commit().await?;
        return Ok(());
    };
    let row = eligible(&txn, &job).await?;
    let Some(row) = row else {
        txn.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite,
            "UPDATE mobile_push_outbox SET outcome='permanent', reason='Ineligible', envelope=NULL WHERE event_id=? AND installation_id=?",
            [job.event_id.into(), job.installation_id.into()])).await?;
        txn.commit().await?;
        return Ok(());
    };
    let lease = uuid::Uuid::new_v4().to_string();
    txn.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite,
        "UPDATE mobile_push_outbox SET lease_id=?, lease_until=?, attempts=attempts+1 WHERE event_id=? AND installation_id=?",
        [lease.clone().into(), (now+LEASE_SECONDS).into(), job.event_id.clone().into(), job.installation_id.clone().into()])).await?;
    txn.commit().await?;
    // The eligibility transaction is the start boundary. A subsequent revocation
    // cannot retract a request already in flight or an accepted provider receipt.
    let delivery = send(state, &job, &row).await;
    let finished = Utc::now().timestamp();
    let txn = state.db.begin().await?;
    // Serialize response handling with replacement and revocation writes.
    let locked = txn.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite,
        "UPDATE mobile_push_outbox SET lease_until=lease_until WHERE event_id=? AND installation_id=? AND lease_id=?",
        [job.event_id.clone().into(), job.installation_id.clone().into(), lease.clone().into()])).await?;
    if locked.rows_affected() != 1 {
        txn.commit().await?;
        return Ok(());
    }
    let (outcome, reason) = if delivery.outcome == "retryable" && finished >= job.expires_at {
        ("expired", "Expired")
    } else {
        (delivery.outcome.as_str(), delivery.reason.as_str())
    };
    if delivery.device_invalid && delivery.reason == "Unregistered" && outcome == "permanent" {
        // CAS all sending identity fields: late terminal responses cannot erase
        // a new token, key or renewed grant, even while this send is outstanding.
        txn.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite,
            "UPDATE mobile_push_registrations SET grant_token=NULL, revision=revision+1 WHERE installation_id=? AND user_id=? AND mobile_session_id=? AND revision=? AND content_key_id=? AND grant_token=?",
            [row.installation_id.into(), row.user_id.into(), row.mobile_session_id.into(), row.revision.into(), row.content_key_id.into(), row.grant_token.into()])).await?;
    }
    let backoff = (2_i64.pow((job.attempts + 1).min(8) as u32)).min(300);
    txn.execute(Statement::from_sql_and_values(DatabaseBackend::Sqlite,
        "UPDATE mobile_push_outbox SET outcome=?, reason=?, next_attempt_at=?, lease_id=NULL, lease_until=0, envelope=CASE WHEN ?='retryable' THEN envelope ELSE NULL END WHERE event_id=? AND installation_id=? AND lease_id=?",
        [outcome.into(), reason.into(), (finished+backoff).min(job.expires_at).into(), outcome.into(), job.event_id.into(), job.installation_id.into(), lease.into()])).await?;
    txn.commit().await?;
    Ok(())
}

async fn send(state: &AppState, job: &outbox::Model, row: &registration::Model) -> RelayDelivery {
    let retry = || RelayDelivery {
        outcome: "retryable".into(),
        reason: "RelayUnavailable".into(),
        device_invalid: false,
    };
    let url = state.config.push_relay.url.trim_end_matches('/');
    if !(url.starts_with("https://") || url.starts_with("http://127.0.0.1:")) {
        return retry();
    }
    let Some(envelope) = job
        .envelope
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
    else {
        return RelayDelivery {
            outcome: "permanent".into(),
            reason: "InvalidEnvelope".into(),
            device_invalid: false,
        };
    };
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(SEND_TIMEOUT))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return retry();
    };
    let result = client.post(format!("{url}/v1/send")).bearer_auth(row.grant_token.as_deref().unwrap_or_default())
        .json(&serde_json::json!({"event_id":job.event_id,"expires_at":job.expires_at,"envelope":envelope})).send().await;
    match result {
        Ok(reply) if reply.status().is_success() => {
            match reply.json::<RelayDelivery>().await {
                Ok(mut verdict)
                    if matches!(
                        verdict.outcome.as_str(),
                        "accepted" | "retryable" | "permanent" | "expired"
                    ) =>
                {
                    // Persist a bounded classification, never arbitrary upstream text.
                    verdict.reason = match verdict.reason.as_str() {
                        "Unregistered"
                        | "Accepted"
                        | "Expired"
                        | "BadTopic"
                        | "BadDeviceToken"
                        | "DeviceTokenNotForTopic"
                        | "TooManyRequests"
                        | "ServiceUnavailable"
                        | "InternalServerError"
                        | "Shutdown"
                        | "ProviderUnavailable"
                        | "NetworkUnavailable"
                        | "DeviceOrEnvironmentMismatch"
                        | "ProviderConfigurationOrPayload"
                        | "PayloadTooLarge" => verdict.reason,
                        _ => match verdict.outcome.as_str() {
                            "accepted" => "Accepted",
                            "retryable" => "ProviderUnavailable",
                            "expired" => "Expired",
                            _ => "ProviderRejected",
                        }
                        .into(),
                    };
                    verdict
                }
                _ => retry(),
            }
        }
        Ok(reply) if reply.status().as_u16() == 503 => match reply.json::<RelayDelivery>().await {
            Ok(verdict)
                if verdict.outcome == "permanent" && verdict.reason == "RelayNotConfigured" =>
            {
                RelayDelivery {
                    outcome: "permanent".into(),
                    reason: "RelayNotConfigured".into(),
                    device_invalid: false,
                }
            }
            _ => retry(),
        },
        Ok(reply) if reply.status().is_client_error() && reply.status().as_u16() != 429 => {
            RelayDelivery {
                outcome: "permanent".into(),
                reason: "RelayAuthorizationOrPayload".into(),
                device_invalid: false,
            }
        }
        _ => retry(),
    }
}
