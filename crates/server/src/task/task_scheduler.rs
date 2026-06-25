use std::str::FromStr;
use std::sync::Arc;

use chrono::Utc;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use sea_orm::prelude::Expr;
use sea_orm::*;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::entity::{task, task_result};
use crate::service::audit::AuditService;
use crate::service::high_risk_audit::ExecAuditContext;
use crate::state::AppState;
use serverbee_common::constants::CAP_EXEC;
use serverbee_common::protocol::{AgentMessage, ServerMessage};

/// Build correlation ID: {task_id}:{run_id}:{server_id}:{attempt}
pub fn build_correlation_id(task_id: &str, run_id: &str, server_id: &str, attempt: i32) -> String {
    format!("{task_id}:{run_id}:{server_id}:{attempt}")
}

/// Called by cron trigger or manual /run endpoint.
/// `skip_retry`: if true, executes once without retry (used by manual trigger).
/// Returns true if execution was started, false if skipped (overlap).
pub async fn execute_scheduled_task(
    state: &Arc<AppState>,
    task_id: &str,
    skip_retry: bool,
    audit_context: Option<ExecAuditContext>,
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
    let timeout_secs = task_model.timeout.unwrap_or(300).max(1) as u64;
    let retry_count = if skip_retry {
        0
    } else {
        task_model.retry_count.max(0)
    };
    let retry_interval = task_model.retry_interval.max(1) as u64;
    let command = task_model.command.clone();
    let audit_context_ref = audit_context.as_ref();
    if let Some(context) = audit_context.clone() {
        state.exec_audit_contexts.insert(run_id.clone(), context);
    }

    // Step 3: Update last_run_at and compute next_run_at (using configured timezone)
    let tz: chrono_tz::Tz = scheduler.timezone().parse().unwrap_or(chrono_tz::UTC);
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
        if let Some(reason) =
            state
                .agent_manager
                .capability_denied_reason(sid, caps as u32, CAP_EXEC)
        {
            let _ = write_synthetic_result(
                &state.db,
                task_id,
                &run_id,
                sid,
                -2,
                crate::router::api::task::exec_capability_denied_output(reason),
            )
            .await;
            if let Some(context) = audit_context_ref {
                let detail = serde_json::json!({
                    "server_id": sid,
                    "task_id": task_id,
                    "command": command,
                    "deny_reason": reason,
                })
                .to_string();
                let _ = AuditService::log(
                    &state.db,
                    &context.user_id,
                    "exec_denied",
                    Some(&detail),
                    &context.ip,
                )
                .await;
            }
            continue;
        }

        if let Some(context) = audit_context_ref {
            let detail = serde_json::json!({
                "server_id": sid,
                "task_id": task_id,
                "command": command,
                "timeout": Some(timeout_secs as u32),
            })
            .to_string();
            let _ = AuditService::log(
                &state.db,
                &context.user_id,
                "exec_started",
                Some(&detail),
                &context.ip,
            )
            .await;
        }

        let state = state.clone();
        let task_id = task_id.to_string();
        let run_id = run_id.clone();
        let sid = sid.clone();
        let command = command.clone();
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

    // Step 5: Wait for all to complete, then clear active_runs.
    // Use Arc::clone so the guard operates on the *shared* DashMap, not a clone.
    let task_id_owned = task_id.to_string();
    let active_runs = Arc::clone(&scheduler.active_runs);
    let run_id_for_cleanup = run_id.clone();
    let state_for_cleanup = state.clone();
    tokio::spawn(async move {
        struct ActiveRunGuard {
            active_runs: Arc<DashMap<String, (String, CancellationToken)>>,
            task_id: String,
            state: Arc<AppState>,
            run_id: String,
        }
        impl Drop for ActiveRunGuard {
            fn drop(&mut self) {
                self.active_runs.remove(&self.task_id);
                self.state.exec_audit_contexts.remove(&self.run_id);
            }
        }
        let _guard = ActiveRunGuard {
            active_runs,
            task_id: task_id_owned,
            state: state_for_cleanup,
            run_id: run_id_for_cleanup,
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
        if token.is_cancelled() {
            break;
        }

        if attempt > 1 {
            // Wait for retry interval, but abort early if cancelled
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(retry_interval)) => {}
                _ = token.cancelled() => { break; }
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
        let rx = state.agent_manager.register_pending_request_with_ttl(
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

        // Wait for response, but abort if the task is cancelled (deleted/disabled)
        let result = tokio::select! {
            r = tokio::time::timeout(timeout_duration, rx) => r,
            _ = token.cancelled() => {
                break;
            }
        };

        // Double-check cancellation before writing — select! may resolve both
        // futures simultaneously and pick the result branch.
        if token.is_cancelled() {
            break;
        }

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
        db,
        task_id,
        run_id,
        server_id,
        1,
        Utc::now(),
        exit_code,
        output,
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
            let tz: chrono_tz::Tz = state
                .task_scheduler
                .timezone()
                .parse()
                .unwrap_or(chrono_tz::UTC);
            for t in &tasks {
                if let Err(e) = state.task_scheduler.add_job(t, state.clone()).await {
                    tracing::error!("Failed to register scheduled task {}: {e}", t.id);
                    continue;
                }
                // Refresh next_run_at on startup so it reflects the current time
                if let Some(cron_expr) = &t.cron_expression {
                    let next = cron::Schedule::from_str(cron_expr)
                        .ok()
                        .and_then(|s| s.upcoming(tz).next().map(|dt| dt.with_timezone(&Utc)));
                    let _ = task::Entity::update_many()
                        .filter(task::Column::Id.eq(&t.id))
                        .col_expr(task::Column::NextRunAt, Expr::value(next))
                        .exec(&state.db)
                        .await;
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
    use chrono::TimeZone;
    use serverbee_common::constants::CAP_DEFAULT;

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

    // build_correlation_id must not collapse empty segments — the colon-joined
    // shape is preserved even when every component is the empty string.
    #[test]
    fn test_correlation_id_empty_segments_keep_delimiters() {
        assert_eq!(build_correlation_id("", "", "", 0), ":::0");
    }

    // Negative and large attempt numbers are formatted verbatim (no clamping).
    #[test]
    fn test_correlation_id_attempt_boundaries() {
        assert_eq!(build_correlation_id("t", "r", "s", -1), "t:r:s:-1");
        assert_eq!(
            build_correlation_id("t", "r", "s", i32::MAX),
            format!("t:r:s:{}", i32::MAX)
        );
    }

    // ---- Helpers -------------------------------------------------------

    /// Build an `Arc<AppState>` backed by a fresh migrated test DB. The two
    /// returned `TempDir` guards must outlive the test: one owns the SQLite
    /// file, the other backs `data_dir` so GeoIP/ASN/transfer paths never
    /// touch the working directory.
    async fn build_test_state() -> (
        Arc<AppState>,
        tempfile::TempDir,
        tempfile::TempDir,
    ) {
        let (db, db_guard) = crate::test_utils::setup_test_db().await;
        let data_dir = tempfile::TempDir::new().unwrap();
        let mut config = crate::config::AppConfig::default();
        config.server.data_dir = data_dir.path().to_str().unwrap().to_string();
        let state = AppState::new(db, config).await.unwrap();
        (state, db_guard, data_dir)
    }

    /// Insert a server row with the given id and persisted capability mirror.
    /// The server is NOT registered with the agent_manager, so it is offline.
    async fn seed_server(db: &DatabaseConnection, id: &str, capabilities: i32) {
        use crate::entity::server;
        let now = chrono::Utc::now();
        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(Some("hash".to_string())),
            token_prefix: Set(Some("sb_pref".to_string())),
            name: Set(format!("server-{id}")),
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
            capabilities: Set(capabilities),
            protocol_version: Set(1),
            features: Set("[]".to_string()),
            last_remote_addr: Set(None),
            fingerprint: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .expect("seed server");
    }

    /// Insert a scheduled task row targeting `server_ids`.
    async fn seed_task(db: &DatabaseConnection, id: &str, server_ids: &[&str]) {
        let now = chrono::Utc::now();
        task::ActiveModel {
            id: Set(id.to_string()),
            command: Set("echo hi".to_string()),
            server_ids_json: Set(serde_json::to_string(server_ids).unwrap()),
            created_by: Set("tester".to_string()),
            task_type: Set("scheduled".to_string()),
            name: Set(Some("test task".to_string())),
            cron_expression: Set(Some("0 0 0 1 1 *".to_string())),
            enabled: Set(true),
            timeout: Set(Some(5)),
            retry_count: Set(0),
            retry_interval: Set(1),
            last_run_at: Set(None),
            next_run_at: Set(None),
            created_at: Set(now),
        }
        .insert(db)
        .await
        .expect("seed task");
    }

    /// Count persisted task_result rows for a given task id.
    async fn result_count(db: &DatabaseConnection, task_id: &str) -> u64 {
        task_result::Entity::find()
            .filter(task_result::Column::TaskId.eq(task_id))
            .count(db)
            .await
            .expect("count task results")
    }

    /// Poll until at least `expected` task_result rows exist for `task_id` (the
    /// offline executor path runs inside a detached tokio task spawned by
    /// `execute_scheduled_task`).
    async fn wait_for_results(db: &DatabaseConnection, task_id: &str, expected: u64) -> u64 {
        for _ in 0..100 {
            let n = result_count(db, task_id).await;
            if n >= expected {
                return n;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        result_count(db, task_id).await
    }

    // ---- write_result (pure DB write) ---------------------------------

    // write_result persists a row with every field set verbatim, including a
    // negative synthetic exit code and the supplied run_id/attempt/timestamps.
    #[tokio::test]
    async fn test_write_result_persists_all_fields() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let started = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        write_result(&db, "task-w", "run-w", "srv-w", 2, started, -3, "Server offline")
            .await
            .expect("write_result should insert");

        let row = task_result::Entity::find()
            .filter(task_result::Column::TaskId.eq("task-w"))
            .one(&db)
            .await
            .unwrap()
            .expect("row present");
        assert_eq!(row.server_id, "srv-w");
        assert_eq!(row.output, "Server offline");
        assert_eq!(row.exit_code, -3);
        assert_eq!(row.run_id.as_deref(), Some("run-w"));
        assert_eq!(row.attempt, 2);
        assert_eq!(row.started_at, Some(started));
    }

    // ---- write_synthetic_result --------------------------------------

    // write_synthetic_result delegates to write_result with a fixed attempt of
    // 1 and stamps started_at itself (so it is Some, not the caller's value).
    #[tokio::test]
    async fn test_write_synthetic_result_uses_attempt_one() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        write_synthetic_result(&db, "task-s", "run-s", "srv-s", -2, "capability denied")
            .await
            .expect("synthetic write should insert");

        let row = task_result::Entity::find()
            .filter(task_result::Column::TaskId.eq("task-s"))
            .one(&db)
            .await
            .unwrap()
            .expect("row present");
        assert_eq!(row.attempt, 1);
        assert_eq!(row.exit_code, -2);
        assert_eq!(row.output, "capability denied");
        assert!(row.started_at.is_some());
    }

    // ---- execute_for_server (offline agent) ---------------------------

    // With no connected agent, execute_for_server falls into the get_sender
    // None branch and writes exactly one "Server offline" result (exit -3) when
    // retries are exhausted (retry_count == 0 → single attempt).
    #[tokio::test]
    async fn test_execute_for_server_offline_writes_single_result() {
        let (state, _db, _dir) = build_test_state().await;
        let token = CancellationToken::new();
        execute_for_server(&state, "task-off", "run-off", "srv-off", "echo hi", 1, 0, 1, token)
            .await;

        assert_eq!(result_count(&state.db, "task-off").await, 1);
        let row = task_result::Entity::find()
            .filter(task_result::Column::TaskId.eq("task-off"))
            .one(&state.db)
            .await
            .unwrap()
            .expect("offline result present");
        assert_eq!(row.exit_code, -3);
        assert_eq!(row.output, "Server offline");
        assert_eq!(row.attempt, 1);
    }

    // An already-cancelled token short-circuits before the attempt loop body,
    // so no result row is written at all.
    #[tokio::test]
    async fn test_execute_for_server_cancelled_writes_nothing() {
        let (state, _db, _dir) = build_test_state().await;
        let token = CancellationToken::new();
        token.cancel();
        execute_for_server(&state, "task-cancel", "run-c", "srv-c", "echo hi", 1, 2, 1, token)
            .await;

        assert_eq!(result_count(&state.db, "task-cancel").await, 0);
    }

    // ---- execute_scheduled_task: missing task -------------------------

    // A task id that is not in the DB is rejected: the function returns false
    // and clears its claimed active_runs entry (no result rows written).
    #[tokio::test]
    async fn test_execute_scheduled_task_missing_task_returns_false() {
        let (state, _db, _dir) = build_test_state().await;
        let started = execute_scheduled_task(&state, "ghost-task", true, None).await;
        assert!(!started);
        assert!(!state.task_scheduler.is_running("ghost-task"));
        assert_eq!(result_count(&state.db, "ghost-task").await, 0);
    }

    // ---- execute_scheduled_task: overlap guard ------------------------

    // When an active run already occupies the active_runs slot, a second
    // trigger is skipped (returns false) without touching the existing entry.
    #[tokio::test]
    async fn test_execute_scheduled_task_overlap_is_skipped() {
        let (state, _db, _dir) = build_test_state().await;
        seed_task(&state.db, "task-overlap", &[]).await;
        let token = CancellationToken::new();
        state
            .task_scheduler
            .active_runs
            .insert("task-overlap".to_string(), ("prior-run".to_string(), token));

        let started = execute_scheduled_task(&state, "task-overlap", true, None).await;
        assert!(!started);
        // The pre-existing run_id must remain untouched.
        let entry = state
            .task_scheduler
            .active_runs
            .get("task-overlap")
            .expect("entry retained");
        assert_eq!(entry.value().0, "prior-run");
    }

    // ---- execute_scheduled_task: capability denied --------------------

    // A target server whose mirror lacks CAP_EXEC produces a synthetic
    // capability-denied result (exit -2) written synchronously before any
    // server execution is spawned.
    #[tokio::test]
    async fn test_execute_scheduled_task_cap_exec_denied_writes_synthetic() {
        let (state, _db, _dir) = build_test_state().await;
        // CAP_DEFAULT intentionally excludes CAP_EXEC.
        seed_server(&state.db, "srv-nocap", CAP_DEFAULT as i32).await;
        seed_task(&state.db, "task-nocap", &["srv-nocap"]).await;

        let started = execute_scheduled_task(&state, "task-nocap", true, None).await;
        assert!(started);

        let row = task_result::Entity::find()
            .filter(task_result::Column::TaskId.eq("task-nocap"))
            .one(&state.db)
            .await
            .unwrap()
            .expect("synthetic denied result present");
        assert_eq!(row.exit_code, -2);
        assert_eq!(row.server_id, "srv-nocap");
        assert!(row.output.contains("Capability denied"));
    }

    // With an audit context supplied, the capability-denied branch also records
    // an `exec_denied` audit log entry alongside the synthetic result.
    #[tokio::test]
    async fn test_execute_scheduled_task_cap_denied_logs_audit() {
        use crate::entity::audit_log;
        let (state, _db, _dir) = build_test_state().await;
        seed_server(&state.db, "srv-audit", CAP_DEFAULT as i32).await;
        seed_task(&state.db, "task-audit", &["srv-audit"]).await;

        let ctx = ExecAuditContext {
            user_id: "admin".to_string(),
            ip: "127.0.0.1".to_string(),
        };
        let started = execute_scheduled_task(&state, "task-audit", true, Some(ctx)).await;
        assert!(started);

        let denied = audit_log::Entity::find()
            .filter(audit_log::Column::Action.eq("exec_denied"))
            .count(&state.db)
            .await
            .unwrap();
        assert_eq!(denied, 1);
    }

    // ---- execute_scheduled_task: offline agent dispatch ---------------

    // A server that HAS CAP_EXEC but is not connected reaches execute_for_server
    // via the spawned join_set, which writes a "Server offline" result (exit
    // -3). retry_count is forced to 0 here because skip_retry == true.
    #[tokio::test]
    async fn test_execute_scheduled_task_offline_agent_writes_offline_result() {
        let (state, _db, _dir) = build_test_state().await;
        seed_server(&state.db, "srv-online-cap", (CAP_DEFAULT | CAP_EXEC) as i32).await;
        seed_task(&state.db, "task-dispatch", &["srv-online-cap"]).await;

        let started = execute_scheduled_task(&state, "task-dispatch", true, None).await;
        assert!(started);

        let n = wait_for_results(&state.db, "task-dispatch", 1).await;
        assert_eq!(n, 1);
        let row = task_result::Entity::find()
            .filter(task_result::Column::TaskId.eq("task-dispatch"))
            .one(&state.db)
            .await
            .unwrap()
            .expect("offline dispatch result present");
        assert_eq!(row.exit_code, -3);
        assert_eq!(row.output, "Server offline");
    }

    // An empty server list produces no work and no result rows, but the trigger
    // is still considered "started" (returns true) and updates last_run_at.
    #[tokio::test]
    async fn test_execute_scheduled_task_empty_server_list_starts_with_no_results() {
        let (state, _db, _dir) = build_test_state().await;
        seed_task(&state.db, "task-empty", &[]).await;

        let started = execute_scheduled_task(&state, "task-empty", true, None).await;
        assert!(started);

        // Give any (non-existent) spawned work a chance to run, then assert none.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(result_count(&state.db, "task-empty").await, 0);

        let updated = task::Entity::find_by_id("task-empty")
            .one(&state.db)
            .await
            .unwrap()
            .expect("task present");
        assert!(updated.last_run_at.is_some(), "last_run_at should be stamped");
    }
}
