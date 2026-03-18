use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use sea_orm::prelude::Expr;
use sea_orm::*;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::entity::{task, task_result};
use crate::state::AppState;
use serverbee_common::constants::{has_capability, CAP_EXEC};
use serverbee_common::protocol::{AgentMessage, ServerMessage};

/// Build correlation ID: {task_id}:{run_id}:{server_id}:{attempt}
pub fn build_correlation_id(
    task_id: &str,
    run_id: &str,
    server_id: &str,
    attempt: i32,
) -> String {
    format!("{task_id}:{run_id}:{server_id}:{attempt}")
}

/// Called by cron trigger or manual /run endpoint.
/// `skip_retry`: if true, executes once without retry (used by manual trigger).
/// Returns true if execution was started, false if skipped (overlap).
pub async fn execute_scheduled_task(
    state: &Arc<AppState>,
    task_id: &str,
    skip_retry: bool,
) -> bool {
    let scheduler = &state.task_scheduler;

    // Step 0: Atomic overlap check using DashMap entry API
    let run_id = uuid::Uuid::new_v4().to_string();
    let token = CancellationToken::new();
    match scheduler.active_runs.entry(task_id.to_string()) {
        Entry::Occupied(_) => {
            tracing::warn!("Task {task_id} still running, skipping trigger");
            return false;
        }
        Entry::Vacant(e) => {
            e.insert((run_id.clone(), token.clone()));
        }
    }

    // Step 1: Load task from DB (entry already claimed, remove on failure)
    let task_model = match task::Entity::find_by_id(task_id).one(&state.db).await {
        Ok(Some(t)) => t,
        Ok(None) => {
            tracing::error!("Task {task_id} not found");
            scheduler.active_runs.remove(task_id);
            return false;
        }
        Err(e) => {
            tracing::error!("Failed to load task {task_id}: {e}");
            scheduler.active_runs.remove(task_id);
            return false;
        }
    };

    let server_ids: Vec<String> =
        serde_json::from_str(&task_model.server_ids_json).unwrap_or_default();
    let timeout_secs = task_model.timeout.unwrap_or(300) as u64;
    let retry_count = if skip_retry {
        0
    } else {
        task_model.retry_count
    };
    let retry_interval = task_model.retry_interval as u64;

    // Step 3: Update last_run_at and compute next_run_at (using configured timezone)
    let tz: chrono_tz::Tz = scheduler
        .timezone()
        .parse()
        .unwrap_or(chrono_tz::UTC);
    let next_run = task_model.cron_expression.as_deref().and_then(|cron_expr| {
        cron::Schedule::from_str(cron_expr)
            .ok()
            .and_then(|s| s.upcoming(tz).next().map(|dt| dt.with_timezone(&Utc)))
    });
    let mut update = task::Entity::update_many()
        .filter(task::Column::Id.eq(task_id))
        .col_expr(task::Column::LastRunAt, Expr::value(Utc::now()));
    if let Some(next) = next_run {
        update = update.col_expr(task::Column::NextRunAt, Expr::value(next));
    }
    let _ = update.exec(&state.db).await;

    // Step 4: Dispatch to each server
    let mut join_set = JoinSet::new();

    // Get capabilities for target servers
    use crate::entity::server;
    let target_servers = server::Entity::find()
        .filter(server::Column::Id.is_in(server_ids.clone()))
        .all(&state.db)
        .await
        .unwrap_or_default();
    let server_caps: std::collections::HashMap<String, i32> = target_servers
        .iter()
        .map(|s| (s.id.clone(), s.capabilities))
        .collect();

    for sid in &server_ids {
        let caps = server_caps.get(sid).copied().unwrap_or(0);

        // Check CAP_EXEC
        if !has_capability(caps as u32, CAP_EXEC) {
            let _ = write_synthetic_result(
                &state.db,
                task_id,
                &run_id,
                sid,
                -2,
                "Capability denied: exec not enabled",
            )
            .await;
            continue;
        }

        let state = state.clone();
        let task_id = task_id.to_string();
        let run_id = run_id.clone();
        let sid = sid.clone();
        let command = task_model.command.clone();
        let token = token.clone();

        join_set.spawn(async move {
            execute_for_server(
                &state,
                &task_id,
                &run_id,
                &sid,
                &command,
                timeout_secs,
                retry_count,
                retry_interval,
                token,
            )
            .await;
        });
    }

    // Step 5: Wait for all to complete, then clear active_runs
    let task_id_owned = task_id.to_string();
    let active_runs = scheduler.active_runs.clone();
    tokio::spawn(async move {
        // Drop guard: remove from active_runs when this scope exits
        struct ActiveRunGuard {
            active_runs: DashMap<String, (String, CancellationToken)>,
            task_id: String,
        }
        impl Drop for ActiveRunGuard {
            fn drop(&mut self) {
                self.active_runs.remove(&self.task_id);
            }
        }
        let _guard = ActiveRunGuard {
            active_runs,
            task_id: task_id_owned,
        };
        while join_set.join_next().await.is_some() {}
    });

    true
}

#[allow(clippy::too_many_arguments)]
async fn execute_for_server(
    state: &Arc<AppState>,
    task_id: &str,
    run_id: &str,
    server_id: &str,
    command: &str,
    timeout_secs: u64,
    retry_count: i32,
    retry_interval: u64,
    token: CancellationToken,
) {
    let max_attempts = retry_count + 1;
    let timeout_duration = std::time::Duration::from_secs(timeout_secs + 10);

    for attempt in 1..=max_attempts {
        if attempt > 1 {
            if token.is_cancelled() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_secs(retry_interval)).await;
            if token.is_cancelled() {
                break;
            }
        }

        let correlation_id = build_correlation_id(task_id, run_id, server_id, attempt);
        let started_at = Utc::now();

        // Check agent online
        let sender = match state.agent_manager.get_sender(server_id) {
            Some(tx) => tx,
            None => {
                let _ = write_result(
                    &state.db,
                    task_id,
                    run_id,
                    server_id,
                    attempt,
                    started_at,
                    -3,
                    "Server offline",
                )
                .await;
                if attempt < max_attempts {
                    continue;
                } else {
                    break;
                }
            }
        };

        // Register pending request with TTL
        let rx = state
            .agent_manager
            .register_pending_request_with_ttl(
                correlation_id.clone(),
                std::time::Duration::from_secs(timeout_secs + 10),
            );

        // Send Exec
        let send_result = sender
            .send(ServerMessage::Exec {
                task_id: correlation_id,
                command: command.to_string(),
                timeout: Some(timeout_secs as u32),
            })
            .await;

        if send_result.is_err() {
            let _ = write_result(
                &state.db,
                task_id,
                run_id,
                server_id,
                attempt,
                started_at,
                -3,
                "Dispatch failed",
            )
            .await;
            if attempt < max_attempts {
                continue;
            } else {
                break;
            }
        }

        // Wait for response
        let result = tokio::time::timeout(timeout_duration, rx).await;

        match result {
            Ok(Ok(AgentMessage::TaskResult { result, .. })) => {
                let _ = write_result(
                    &state.db,
                    task_id,
                    run_id,
                    server_id,
                    attempt,
                    started_at,
                    result.exit_code,
                    &result.output,
                )
                .await;
                if result.exit_code == 0 {
                    break;
                }
                // Non-zero: continue to retry if attempts remain
            }
            _ => {
                // Timeout or channel error
                let _ = write_result(
                    &state.db,
                    task_id,
                    run_id,
                    server_id,
                    attempt,
                    started_at,
                    -4,
                    &format!("No response within {timeout_secs}s"),
                )
                .await;
            }
        }

        if attempt == max_attempts {
            break;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn write_result(
    db: &DatabaseConnection,
    task_id: &str,
    run_id: &str,
    server_id: &str,
    attempt: i32,
    started_at: chrono::DateTime<Utc>,
    exit_code: i32,
    output: &str,
) -> Result<(), DbErr> {
    task_result::ActiveModel {
        id: NotSet,
        task_id: Set(task_id.to_string()),
        server_id: Set(server_id.to_string()),
        output: Set(output.to_string()),
        exit_code: Set(exit_code),
        finished_at: Set(Utc::now()),
        run_id: Set(Some(run_id.to_string())),
        attempt: Set(attempt),
        started_at: Set(Some(started_at)),
    }
    .insert(db)
    .await?;
    Ok(())
}

async fn write_synthetic_result(
    db: &DatabaseConnection,
    task_id: &str,
    run_id: &str,
    server_id: &str,
    exit_code: i32,
    output: &str,
) -> Result<(), DbErr> {
    write_result(
        db, task_id, run_id, server_id, 1, Utc::now(), exit_code, output,
    )
    .await
}

/// Startup function: load all enabled scheduled tasks and register jobs.
pub async fn run(state: Arc<AppState>) {
    let tasks = task::Entity::find()
        .filter(task::Column::TaskType.eq("scheduled"))
        .filter(task::Column::Enabled.eq(true))
        .all(&state.db)
        .await;

    match tasks {
        Ok(tasks) => {
            for t in &tasks {
                if let Err(e) = state.task_scheduler.add_job(t, state.clone()).await {
                    tracing::error!("Failed to register scheduled task {}: {e}", t.id);
                }
            }
            tracing::info!("Loaded {} scheduled tasks", tasks.len());
        }
        Err(e) => {
            tracing::error!("Failed to load scheduled tasks: {e}");
        }
    }

    // Start the scheduler tick loop
    if let Err(e) = state.task_scheduler.start().await {
        tracing::error!("Failed to start task scheduler: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correlation_id_format() {
        let cid = build_correlation_id("task-1", "run-abc", "srv-1", 1);
        assert_eq!(cid, "task-1:run-abc:srv-1:1");
    }

    #[test]
    fn test_correlation_id_uniqueness() {
        let a = build_correlation_id("t1", "r1", "s1", 1);
        let b = build_correlation_id("t1", "r1", "s2", 1);
        let c = build_correlation_id("t1", "r1", "s1", 2);
        assert_ne!(a, b);
        assert_ne!(a, c);
    }
}
