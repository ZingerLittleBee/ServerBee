//! `TaskResult` / `CapabilityDenied` handling: pending-request dispatch for
//! exec waiters, one-shot result persistence, and exec audit trail.

use std::sync::Arc;

use crate::service::audit::AuditService;
use crate::service::upgrade_tracker::UpgradeLookup;
use crate::state::AppState;
use serverbee_common::protocol::{AgentMessage, ServerMessage};

pub(super) async fn on_task_result(
    state: &Arc<AppState>,
    server_id: &str,
    msg_id: String,
    result: serverbee_common::types::TaskResult,
) {
    // Try pending dispatch first (scheduler or other waiters)
    let dispatched = state.agent_manager.dispatch_pending_response(
        server_id,
        &result.task_id,
        AgentMessage::TaskResult {
            msg_id: msg_id.clone(),
            result: result.clone(),
        },
    );
    // No waiter — one-shot task, save directly
    let accepted = dispatched || save_oneshot_result(state, server_id, &result).await;
    if accepted && let Err(e) = audit_exec_finished(state, server_id, &result).await {
        tracing::error!("Failed to write exec_finished audit log for {server_id}: {e}");
    }
    // Send Ack
    if let Some(tx) = state.agent_manager.get_sender(server_id) {
        let _ = tx.send(ServerMessage::Ack { msg_id }).await;
    }
}

pub(super) async fn on_capability_denied(
    state: &Arc<AppState>,
    server_id: &str,
    msg_id: Option<String>,
    session_id: Option<String>,
    capability: String,
    reason: serverbee_common::constants::CapabilityDeniedReason,
) {
    tracing::warn!(
        "Agent {server_id} denied capability '{capability}' with reason {reason:?} (msg_id={msg_id:?}, session_id={session_id:?})"
    );
    // For exec: try pending dispatch first, then save directly
    if let Some(task_id) = &msg_id {
        let synthetic = serverbee_common::types::TaskResult {
            task_id: task_id.clone(),
            output: capability_denied_output(&capability, reason),
            exit_code: -2,
        };
        let dispatched = state.agent_manager.dispatch_pending_response(
            server_id,
            task_id,
            AgentMessage::TaskResult {
                msg_id: task_id.clone(),
                result: synthetic,
            },
        );
        let is_target = !dispatched
            && is_oneshot_target(&state.db, task_id, server_id)
                .await
                .unwrap_or_else(|e| {
                    tracing::error!("Failed to look up task {task_id} for {server_id}: {e}");
                    false
                });
        if is_target {
            use crate::entity::task_result;
            use sea_orm::{ActiveModelTrait, NotSet, Set};
            let result = task_result::ActiveModel {
                id: NotSet,
                task_id: Set(task_id.clone()),
                server_id: Set(server_id.to_string()),
                output: Set(capability_denied_output(&capability, reason)),
                exit_code: Set(-2),
                run_id: Set(None),
                attempt: Set(1),
                started_at: Set(None),
                finished_at: Set(chrono::Utc::now()),
            };
            if let Err(e) = result.insert(&state.db).await {
                tracing::error!("Failed to write CapabilityDenied task result: {e}");
            }
        }
    }
    if capability == "upgrade"
        && let Some(job) = state.upgrade_tracker.get(server_id)
    {
        state
            .upgrade_tracker
            .mark_failed_by_capability_denied(UpgradeLookup::from_job(&job), reason);
    }
    // For terminal: unregister session so browser gets notified
    if let Some(sid) = &session_id {
        state
            .agent_manager
            .unregister_agent_terminal_session(server_id, sid);
    }
}

/// Persist a task result that had no pending waiter. Returns whether it was
/// accepted, i.e. it answers a one-shot task that was sent to `server_id`.
async fn save_oneshot_result(
    state: &Arc<AppState>,
    server_id: &str,
    result: &serverbee_common::types::TaskResult,
) -> bool {
    match is_oneshot_target(&state.db, &result.task_id, server_id).await {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!(
                "Dropping task result {} from {server_id}: no matching exec was sent to this server",
                result.task_id
            );
            return false;
        }
        Err(e) => {
            tracing::error!(
                "Failed to look up task {} for {server_id}: {e}",
                result.task_id
            );
            return false;
        }
    }
    if let Err(e) = save_task_result(&state.db, server_id, result).await {
        tracing::error!("Failed to save task result for {server_id}: {e}");
    }
    true
}

/// Whether `task_id` names a one-shot task that was sent to `server_id`.
///
/// Scheduled runs are answered through their pending waiter, so a result with
/// no waiter is only legitimate for a one-shot task targeting the sender. The
/// task id is agent-supplied; without this check an agent could attach results
/// and exec audit entries to tasks it was never asked to run.
async fn is_oneshot_target(
    db: &sea_orm::DatabaseConnection,
    task_id: &str,
    server_id: &str,
) -> Result<bool, crate::error::AppError> {
    use crate::entity::task;
    use sea_orm::EntityTrait;

    let Some(task_model) = task::Entity::find_by_id(task_id).one(db).await? else {
        return Ok(false);
    };
    if task_model.task_type != "oneshot" {
        return Ok(false);
    }
    let server_ids: Vec<String> =
        serde_json::from_str(&task_model.server_ids_json).map_err(|e| {
            crate::error::AppError::Internal(format!("Invalid task server_ids_json: {e}"))
        })?;
    Ok(server_ids.iter().any(|id| id == server_id))
}

/// Save a task result to the database.
async fn save_task_result(
    db: &sea_orm::DatabaseConnection,
    server_id: &str,
    result: &serverbee_common::types::TaskResult,
) -> Result<(), crate::error::AppError> {
    use crate::entity::task_result;
    use sea_orm::{ActiveModelTrait, NotSet, Set};

    let new_result = task_result::ActiveModel {
        id: NotSet,
        task_id: Set(result.task_id.clone()),
        server_id: Set(server_id.to_string()),
        output: Set(result.output.clone()),
        exit_code: Set(result.exit_code),
        run_id: NotSet,
        attempt: Set(1),
        started_at: NotSet,
        finished_at: Set(chrono::Utc::now()),
    };
    new_result.insert(db).await?;
    Ok(())
}

async fn audit_exec_finished(
    state: &Arc<AppState>,
    server_id: &str,
    result: &serverbee_common::types::TaskResult,
) -> Result<(), crate::error::AppError> {
    use crate::entity::task;
    use sea_orm::EntityTrait;

    let base_task_id = result.task_id.split(':').next().unwrap_or(&result.task_id);
    let Some(task_model) = task::Entity::find_by_id(base_task_id).one(&state.db).await? else {
        return Ok(());
    };

    if let Some(run_id) = result.task_id.split(':').nth(1)
        && let Some(context) = state.exec_audit_contexts.get(run_id)
    {
        let detail = serde_json::json!({
            "server_id": server_id,
            "task_id": task_model.id,
            "command": task_model.command,
            "exit_code": result.exit_code,
        })
        .to_string();
        AuditService::log(
            &state.db,
            &context.user_id,
            "exec_finished",
            Some(&detail),
            &context.ip,
        )
        .await?;
        return Ok(());
    }

    if task_model.task_type != "oneshot" {
        return Ok(());
    }

    let detail = serde_json::json!({
        "server_id": server_id,
        "task_id": task_model.id,
        "command": task_model.command,
        "exit_code": result.exit_code,
    })
    .to_string();
    AuditService::log(
        &state.db,
        &task_model.created_by,
        "exec_finished",
        Some(&detail),
        "system",
    )
    .await
}

fn capability_denied_output(
    capability: &str,
    reason: serverbee_common::constants::CapabilityDeniedReason,
) -> String {
    match (capability, reason) {
        ("exec", serverbee_common::constants::CapabilityDeniedReason::ServerCapabilityDisabled) => {
            "Capability denied: exec disabled on server".to_string()
        }
        ("exec", serverbee_common::constants::CapabilityDeniedReason::AgentCapabilityDisabled) => {
            "Capability denied: exec blocked by agent local policy".to_string()
        }
        _ => format!("Capability denied: {capability}"),
    }
}
