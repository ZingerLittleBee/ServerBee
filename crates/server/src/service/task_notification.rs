//! Durable scheduler-drain proof precedes recoverable, atomic outbox admission.
use std::{collections::HashMap, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::Utc;
use sea_orm::*;

use crate::{
    entity::{
        mobile_push_outbox as outbox, mobile_push_registration as registration, task_result,
        task_run,
    },
    error::AppError,
    service::{
        mobile_push_outbox,
        push_envelope::{PushContent, TaskRunSummary, encrypt},
    },
    state::AppState,
};

pub(crate) async fn record_completion(
    state: &Arc<AppState>,
    run_id: &str,
    executors_complete: bool,
    finished_at: i64,
) -> Result<(), AppError> {
    let txn = state.db.begin().await?;
    // Serialize finalization and admission with account, preference and task writes.
    txn.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "UPDATE task_runs SET status=status WHERE run_id=?",
        [run_id.into()],
    ))
    .await?;
    let Some(run) = task_run::Entity::find_by_id(run_id).one(&txn).await? else {
        txn.commit().await?;
        return Ok(());
    };
    if run.status != "running" {
        txn.commit().await?;
        return Ok(());
    }
    let targets: Vec<String> = serde_json::from_str(&run.targets_json)
        .map_err(|_| AppError::Internal("Invalid run targets".into()))?;
    let results = task_result::Entity::find()
        .filter(task_result::Column::TaskId.eq(&run.task_id))
        .filter(task_result::Column::RunId.eq(run_id))
        .order_by_asc(task_result::Column::Attempt)
        .order_by_asc(task_result::Column::Id)
        .all(&txn)
        .await?;
    let mut final_results = HashMap::new();
    for result in results {
        final_results.insert(result.server_id.clone(), result);
    }
    let complete = executors_complete
        && !targets.is_empty()
        && targets.iter().all(|id| final_results.contains_key(id));
    let mut update: task_run::ActiveModel = run.clone().into();
    update.completed_at = Set(Some(finished_at));
    if !complete {
        update.status = Set("incomplete".into());
        update.update(&txn).await?;
        txn.commit().await?;
        return Ok(());
    }
    let mut summary = TaskRunSummary {
        task_id: run.task_id,
        run_id: run_id.into(),
        total: targets.len(),
        failed: 0,
        timed_out: 0,
        offline: 0,
        denied: 0,
    };
    for id in targets {
        if let Some(result) = final_results.get(&id) {
            match result.exit_code {
                0 => {}
                -2 => summary.denied += 1,
                -3 => summary.offline += 1,
                -4 => summary.timed_out += 1,
                _ => summary.failed += 1,
            }
        }
    }
    update.status = Set("drained".into());
    update.summary_json =
        Set(Some(serde_json::to_string(&summary).map_err(|_| {
            AppError::Internal("Invalid run summary".into())
        })?));
    update.update(&txn).await?;
    txn.commit().await?;
    Ok(())
}

/// The production push-worker owner retries only durable scheduler-drained runs.
/// Intermediate result rows alone never constitute a completion proof.
pub(crate) async fn recover_pending(state: &Arc<AppState>) -> Result<(), AppError> {
    let runs = task_run::Entity::find()
        .filter(task_run::Column::Status.eq("drained"))
        .order_by_asc(task_run::Column::CompletedAt)
        .limit(32)
        .all(&state.db)
        .await?;
    let mut failure = None;
    for run in runs {
        if let Err(error) = finish_run(state, &run.run_id).await {
            failure = Some(error);
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(())
}

pub(crate) async fn finish_run(state: &Arc<AppState>, run_id: &str) -> Result<(), AppError> {
    let txn = state.db.begin().await?;
    txn.execute(Statement::from_sql_and_values(
        DatabaseBackend::Sqlite,
        "UPDATE task_runs SET status=status WHERE run_id=?",
        [run_id.into()],
    ))
    .await?;
    let Some(run) = task_run::Entity::find_by_id(run_id).one(&txn).await? else {
        txn.commit().await?;
        return Ok(());
    };
    if run.status != "drained" {
        txn.commit().await?;
        return Ok(());
    }
    let summary: TaskRunSummary = serde_json::from_str(
        run.summary_json
            .as_deref()
            .ok_or_else(|| AppError::Internal("Missing drained run summary".into()))?,
    )
    .map_err(|_| AppError::Internal("Invalid drained run summary".into()))?;
    let created_at = run
        .completed_at
        .ok_or_else(|| AppError::Internal("Missing drained run time".into()))?;
    if summary.task_id != run.task_id || summary.run_id != run.run_id {
        return Err(AppError::Internal("Invalid drained run identity".into()));
    }
    let mut update: task_run::ActiveModel = run.clone().into();
    update.status = Set("completed".into());
    update.update(&txn).await?;
    let now = Utc::now().timestamp();
    // Keep the original 30-minute window even after admission failure/restart.
    // Success delivery is a separate ticket; completed successes stay silent.
    if created_at + 1800 <= now
        || summary.failed + summary.timed_out + summary.offline + summary.denied == 0
    {
        txn.commit().await?;
        return Ok(());
    }
    let registrations = registration::Entity::find()
        .filter(registration::Column::UserId.eq(&run.owner_id))
        .filter(registration::Column::Enabled.eq(true))
        .filter(registration::Column::TaskFailure.eq(true))
        .all(&txn)
        .await?;
    for row in registrations {
        let job = outbox::Model {
            event_id: run_id.into(),
            installation_id: row.installation_id.clone(),
            user_id: run.owner_id.clone(),
            mobile_session_id: row.mobile_session_id.clone(),
            registration_revision: row.revision,
            recipient_role: "admin".into(),
            task_run_id: Some(run_id.into()),
            created_at,
            expires_at: created_at + 1800,
            envelope: None,
            outcome: "pending".into(),
            reason: "Queued".into(),
            attempts: 0,
            next_attempt_at: now,
            lease_id: None,
            lease_until: 0,
        };
        if mobile_push_outbox::eligible(&txn, &job).await?.is_none() {
            continue;
        }
        let (Some(key_id), Some(secret), Some(deployment)) =
            (&row.content_key_id, &row.content_key, &row.deployment_id)
        else {
            continue;
        };
        let Ok(secret) = STANDARD.decode(secret) else {
            continue;
        };
        let content = PushContent {
            kind: "task_failure".into(),
            deployment_id: deployment.clone(),
            user_id: run.owner_id.clone(),
            installation_id: row.installation_id,
            event_id: job.event_id.clone(),
            created_at,
            expires_at: created_at + 1800,
            task_run: Some(summary.clone()),
        };
        let Ok(envelope) = encrypt(key_id, &secret, &content) else {
            tracing::warn!("Task notification content key unavailable");
            continue;
        };
        let mut active: outbox::ActiveModel = job.into();
        active.envelope =
            Set(Some(serde_json::to_string(&envelope).map_err(|_| {
                AppError::Internal("Push encryption failed".into())
            })?));
        outbox::Entity::insert(active)
            .on_conflict(
                sea_orm::sea_query::OnConflict::columns([
                    outbox::Column::EventId,
                    outbox::Column::InstallationId,
                ])
                .do_nothing()
                .to_owned(),
            )
            .do_nothing()
            .exec(&txn)
            .await?;
    }
    txn.commit().await?;
    Ok(())
}
