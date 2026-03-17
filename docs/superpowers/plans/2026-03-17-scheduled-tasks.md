# Scheduled Tasks Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add cron-based scheduled task execution with failure retry, cancellation, and frontend management UI.

**Architecture:** A `TaskScheduler` struct in `service/task_scheduler.rs` wraps `tokio-cron-scheduler`, is shared via `AppState`, and provides CRUD synchronization for API routes. Execution uses the existing `ServerMessage::Exec` / `AgentMessage::TaskResult` protocol. The agent WS handler is modified to route responses through `pending_requests` for scheduler consumption. Each cron trigger spawns parallel per-server execution chains with retry and cancellation support.

**Tech Stack:** Rust (tokio-cron-scheduler, tokio-util CancellationToken, sea-orm), React (TanStack Query, cronstrue)

**Spec:** `docs/superpowers/specs/2026-03-17-traffic-stats-scheduled-tasks-design.md` sections 2.1-2.6

**Prerequisite:** Plan A (migration, entities, config) must be complete. All tables, columns, and entity models already exist.

---

## File Structure

### New files:
- `crates/server/src/service/task_scheduler.rs` — TaskScheduler struct (shared via AppState)
- `crates/server/src/task/task_scheduler.rs` — background task startup (loads jobs, starts scheduler loop)
- `apps/web/src/components/task/scheduled-task-list.tsx` — scheduled tasks list component
- `apps/web/src/components/task/scheduled-task-dialog.tsx` — create/edit dialog
- `apps/web/src/hooks/use-scheduled-tasks.ts` — React Query hooks for scheduled task API

### Modified files:
- `crates/server/src/service/agent_manager.rs` — pending_requests per-entry TTL
- `crates/server/src/task/offline_checker.rs` — use per-entry TTL in cleanup
- `crates/server/src/router/ws/agent.rs` — TaskResult/CapabilityDenied pending dispatch
- `crates/server/src/service/mod.rs` — export task_scheduler module
- `crates/server/src/task/mod.rs` — export task_scheduler module
- `crates/server/src/state.rs` — add task_scheduler to AppState
- `crates/server/src/main.rs` — spawn scheduler background task
- `crates/server/src/router/api/task.rs` — extend with GET list, PUT, DELETE, POST run
- `crates/server/src/router/api/mod.rs` — update task route registration
- `crates/server/src/openapi.rs` — register new handler paths and schemas
- `apps/web/src/routes/_authed/settings/tasks.tsx` — tab layout + scheduled tasks UI
- `apps/web/src/lib/api-schema.ts` — re-export new types
- `ENV.md` — document scheduler timezone
- `TESTING.md` — update test counts

---

## Task 0: Add missing dependencies

`CancellationToken` requires the `"sync"` feature on `tokio-util` (currently only `"io"`). `next_run_at` computation needs the `cron` crate for schedule parsing.

**Files:**
- Modify: `crates/server/Cargo.toml`

- [ ] **Step 1: Add sync feature and cron dependency**

Change line 41 from:
```toml
tokio-util = { version = "0.7", features = ["io"] }
```
to:
```toml
tokio-util = { version = "0.7", features = ["io", "sync"] }
```

Add after `tokio-cron-scheduler`:
```toml
cron = "0.13"
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add crates/server/Cargo.toml Cargo.lock
git commit -m "chore: add cron crate and enable tokio-util sync feature"
```

---

## Task 1: pending_requests per-entry TTL

The existing `pending_requests` uses a global 60s cleanup. Scheduled tasks need per-entry TTL (up to 310s for default 300s timeout + 10s buffer).

**Files:**
- Modify: `crates/server/src/service/agent_manager.rs`
- Modify: `crates/server/src/task/offline_checker.rs`

- [ ] **Step 1: Write test for per-entry TTL cleanup**

In `crates/server/src/service/agent_manager.rs`, add to existing test module:
```rust
#[test]
fn test_cleanup_expired_requests_per_entry_ttl() {
    let (browser_tx, _) = broadcast::channel(16);
    let mgr = AgentManager::new(browser_tx);

    // Register with short TTL
    let _rx1 = mgr.register_pending_request_with_ttl(
        "short".into(), Duration::from_millis(10),
    );
    // Register with long TTL
    let _rx2 = mgr.register_pending_request_with_ttl(
        "long".into(), Duration::from_secs(300),
    );

    std::thread::sleep(std::time::Duration::from_millis(50));
    mgr.cleanup_expired_requests();

    // Short TTL should be cleaned up, long TTL should remain
    assert!(!mgr.has_pending_request("short"));
    assert!(mgr.has_pending_request("long"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p serverbee-server service::agent_manager::tests::test_cleanup_expired_requests_per_entry_ttl -v`
Expected: FAIL (methods don't exist yet)

- [ ] **Step 3: Implement per-entry TTL**

Change `pending_requests` field type from:
```rust
pending_requests: DashMap<String, (oneshot::Sender<AgentMessage>, std::time::Instant)>,
```
to:
```rust
pending_requests: DashMap<String, (oneshot::Sender<AgentMessage>, std::time::Instant, std::time::Duration)>,
```

Add new method:
```rust
pub fn register_pending_request_with_ttl(
    &self,
    msg_id: String,
    ttl: std::time::Duration,
) -> oneshot::Receiver<AgentMessage> {
    let (tx, rx) = oneshot::channel();
    self.pending_requests.insert(msg_id, (tx, std::time::Instant::now(), ttl));
    rx
}

pub fn has_pending_request(&self, msg_id: &str) -> bool {
    self.pending_requests.contains_key(msg_id)
}
```

Update existing `register_pending_request` to use default 60s TTL:
```rust
pub fn register_pending_request(&self, msg_id: String) -> oneshot::Receiver<AgentMessage> {
    self.register_pending_request_with_ttl(msg_id, std::time::Duration::from_secs(60))
}
```

Update `cleanup_expired_requests` to use per-entry TTL (no argument):
```rust
pub fn cleanup_expired_requests(&self) {
    let now = std::time::Instant::now();
    self.pending_requests.retain(|_, (_, created_at, ttl)| {
        now.duration_since(*created_at) < *ttl
    });
}
```

Update `dispatch_pending_response` to match new tuple:
```rust
pub fn dispatch_pending_response(&self, msg_id: &str, message: AgentMessage) -> bool {
    if let Some((_, (tx, _, _))) = self.pending_requests.remove(msg_id) {
        let _ = tx.send(message);
        true
    } else {
        false
    }
}
```

- [ ] **Step 4: Update offline_checker**

In `crates/server/src/task/offline_checker.rs`, change:
```rust
state.agent_manager.cleanup_expired_requests(Duration::from_secs(60));
```
to:
```rust
state.agent_manager.cleanup_expired_requests();
```
Remove the `Duration` import if no longer used.

- [ ] **Step 5: Run test**

Run: `cargo test -p serverbee-server service::agent_manager::tests -v`
Expected: all tests PASS

- [ ] **Step 6: Verify full compilation**

Run: `cargo check --workspace`
Expected: compiles (all callers of the old API must be updated — check `router/ws/agent.rs` file operation handlers that call `register_pending_request`, they continue to work since the default overload still exists)

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/service/agent_manager.rs crates/server/src/task/offline_checker.rs
git commit -m "feat: change pending_requests to per-entry TTL"
```

---

## Task 2: agent.rs — TaskResult and CapabilityDenied dispatch

Modify the agent WS handler to try `dispatch_pending_response` before direct DB save, enabling the scheduler to receive results via oneshot channels.

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs`

- [ ] **Step 1: Modify TaskResult handler**

Find the `AgentMessage::TaskResult` match arm (around line 262). Change from:
```rust
AgentMessage::TaskResult { msg_id, result } => {
    if let Err(e) = save_task_result(&state.db, server_id, &result).await {
        tracing::error!("Failed to save task result for {server_id}: {e}");
    }
    if let Some(tx) = state.agent_manager.get_sender(server_id) {
        let _ = tx.send(ServerMessage::Ack { msg_id }).await;
    }
}
```
to:
```rust
AgentMessage::TaskResult { msg_id, result } => {
    // Try pending dispatch first (scheduler or other waiters)
    let dispatched = state.agent_manager.dispatch_pending_response(
        &result.task_id,
        AgentMessage::TaskResult { msg_id: msg_id.clone(), result: result.clone() },
    );
    if !dispatched {
        // No waiter — one-shot task, save directly
        if let Err(e) = save_task_result(&state.db, server_id, &result).await {
            tracing::error!("Failed to save task result for {server_id}: {e}");
        }
    }
    if let Some(tx) = state.agent_manager.get_sender(server_id) {
        let _ = tx.send(ServerMessage::Ack { msg_id }).await;
    }
}
```

- [ ] **Step 2: Update save_task_result function**

Find the `save_task_result` function (around line 442). It constructs `task_result::ActiveModel` without the new fields added by Plan A's migration. Add the missing fields:
```rust
async fn save_task_result(
    db: &DatabaseConnection,
    server_id: &str,
    result: &serverbee_common::types::TaskResult,
) -> Result<(), DbErr> {
    let model = task_result::ActiveModel {
        id: NotSet,
        task_id: Set(result.task_id.clone()),
        server_id: Set(server_id.to_string()),
        output: Set(result.output.clone()),
        exit_code: Set(result.exit_code),
        finished_at: Set(chrono::Utc::now()),
        // New fields (from Plan A migration) — NULL/default for one-shot tasks
        run_id: Set(None),
        attempt: Set(1),
        started_at: Set(None),
    };
    model.insert(db).await?;
    Ok(())
}
```

Without this change, the code will not compile after Plan A adds new non-defaulted fields to `task_result::ActiveModel`.

- [ ] **Step 3: Modify CapabilityDenied handler**

Find the `AgentMessage::CapabilityDenied` match arm (around line 292). Unify exit code to -2 and add pending dispatch. Change the exec branch from `exit_code: Set(-1)` to:

```rust
AgentMessage::CapabilityDenied { msg_id, session_id, capability } => {
    if let Some(task_id) = &msg_id {
        // Build synthetic TaskResult
        let synthetic = serverbee_common::types::TaskResult {
            task_id: task_id.clone(),
            output: format!("Capability denied: {capability}"),
            exit_code: -2,
        };
        // Try pending dispatch first
        let dispatched = state.agent_manager.dispatch_pending_response(
            task_id,
            AgentMessage::TaskResult {
                msg_id: task_id.clone(),
                result: synthetic.clone(),
            },
        );
        if !dispatched {
            // No waiter — save directly with unified exit_code=-2
            let result = task_result::ActiveModel {
                id: NotSet,
                task_id: Set(task_id.clone()),
                server_id: Set(server_id.to_string()),
                output: Set(format!("Capability denied: {capability}")),
                exit_code: Set(-2),
                finished_at: Set(chrono::Utc::now()),
                // New fields (from Plan A migration)
                run_id: Set(None),
                attempt: Set(1),
                started_at: Set(None),
            };
            if let Err(e) = result.insert(&state.db).await {
                tracing::error!("Failed to save capability denied result: {e}");
            }
        }
    }
    // Terminal session handling unchanged
    if let Some(sid) = &session_id {
        state.agent_manager.unregister_terminal_session(sid);
    }
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles (may need to add `Clone` derive to `TaskResult` in `common/types.rs` if not already present)

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/router/ws/agent.rs
git commit -m "feat: route TaskResult/CapabilityDenied through pending dispatch"
```

---

## Task 3: TaskScheduler service — struct and job management

**Files:**
- Create: `crates/server/src/service/task_scheduler.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Write tests for job management**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_scheduler() {
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        assert!(!scheduler.is_running("nonexistent"));
    }

    #[tokio::test]
    async fn test_overlap_detection() {
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let token = CancellationToken::new();
        scheduler.active_runs.insert(
            "task-1".to_string(),
            ("run-1".to_string(), token),
        );
        assert!(scheduler.is_running("task-1"));
        assert!(!scheduler.is_running("task-2"));
    }

    #[tokio::test]
    async fn test_cancel_active_run() {
        let scheduler = TaskScheduler::new("UTC").await.unwrap();
        let token = CancellationToken::new();
        let token_clone = token.clone();
        scheduler.active_runs.insert(
            "task-1".to_string(),
            ("run-1".to_string(), token),
        );
        scheduler.cancel_active_run("task-1");
        assert!(token_clone.is_cancelled());
        assert!(!scheduler.is_running("task-1"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p serverbee-server service::task_scheduler::tests -v`
Expected: FAIL (module doesn't exist)

- [ ] **Step 3: Implement TaskScheduler struct**

Create `crates/server/src/service/task_scheduler.rs`:
```rust
use std::sync::Arc;

use dashmap::DashMap;
use tokio_cron_scheduler::{Job, JobScheduler};
use tokio_util::sync::CancellationToken;

use crate::entity::task;
use crate::error::AppError;

pub struct TaskScheduler {
    scheduler: JobScheduler,
    job_map: DashMap<String, uuid::Uuid>,                        // task_id -> job UUID
    pub(crate) active_runs: DashMap<String, (String, CancellationToken)>, // task_id -> (run_id, token)
    timezone: String,
}

impl TaskScheduler {
    pub async fn new(timezone: &str) -> Result<Self, AppError> {
        let scheduler = JobScheduler::new().await
            .map_err(|e| AppError::Internal(format!("Failed to create scheduler: {e}")))?;
        Ok(Self {
            scheduler,
            job_map: DashMap::new(),
            active_runs: DashMap::new(),
            timezone: timezone.to_string(),
        })
    }

    pub fn is_running(&self, task_id: &str) -> bool {
        self.active_runs.contains_key(task_id)
    }

    pub fn cancel_active_run(&self, task_id: &str) {
        if let Some((_, (_, token))) = self.active_runs.remove(task_id) {
            token.cancel();
        }
    }

    pub fn timezone(&self) -> &str {
        &self.timezone
    }

    pub async fn start(&self) -> Result<(), AppError> {
        self.scheduler.start().await
            .map_err(|e| AppError::Internal(format!("Failed to start scheduler: {e}")))?;
        Ok(())
    }
}
```

- [ ] **Step 4: Register module**

In `crates/server/src/service/mod.rs`, add:
```rust
pub mod task_scheduler;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p serverbee-server service::task_scheduler::tests -v`
Expected: all 3 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/service/task_scheduler.rs crates/server/src/service/mod.rs
git commit -m "feat: add TaskScheduler struct with overlap detection and cancellation"
```

---

## Task 4: TaskScheduler — job registration (add/remove/update)

**Files:**
- Modify: `crates/server/src/service/task_scheduler.rs`

- [ ] **Step 1: Implement add_job**

```rust
pub async fn add_job(
    &self,
    task_model: &task::Model,
    state: Arc<crate::state::AppState>,
) -> Result<(), AppError> {
    let cron = task_model.cron_expression.as_deref()
        .ok_or_else(|| AppError::BadRequest("Missing cron_expression".into()))?;
    let task_id = task_model.id.clone();

    // Parse timezone for cron
    let tz: chrono_tz::Tz = self.timezone.parse()
        .map_err(|_| AppError::Internal(format!("Invalid timezone: {}", self.timezone)))?;

    let job = Job::new_async_tz(cron, tz, move |_uuid, _lock| {
        let state = state.clone();
        let task_id = task_id.clone();
        Box::pin(async move {
            crate::task::task_scheduler::execute_scheduled_task(&state, &task_id, false).await;
        })
    })
    .map_err(|e| AppError::BadRequest(format!("Invalid cron expression: {e}")))?;

    let job_id = job.guid();
    self.scheduler.add(job).await
        .map_err(|e| AppError::Internal(format!("Failed to add job: {e}")))?;
    self.job_map.insert(task_model.id.clone(), job_id);
    Ok(())
}

pub async fn remove_job(&self, task_id: &str) -> Result<(), AppError> {
    self.cancel_active_run(task_id);
    if let Some((_, job_id)) = self.job_map.remove(task_id) {
        self.scheduler.remove(&job_id).await
            .map_err(|e| AppError::Internal(format!("Failed to remove job: {e}")))?;
    }
    Ok(())
}

pub async fn update_job(
    &self,
    task_model: &task::Model,
    state: Arc<crate::state::AppState>,
) -> Result<(), AppError> {
    self.remove_job(&task_model.id).await?;
    if task_model.enabled {
        self.add_job(task_model, state).await?;
    }
    Ok(())
}

pub async fn disable_job(&self, task_id: &str) -> Result<(), AppError> {
    self.remove_job(task_id).await
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles (the `execute_scheduled_task` function will be created in Task 5)

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/service/task_scheduler.rs
git commit -m "feat: add job registration (add/remove/update/disable)"
```

---

## Task 5: TaskScheduler — execution flow

The core execution function triggered by cron or manual run.

**Files:**
- Create: `crates/server/src/task/task_scheduler.rs`
- Modify: `crates/server/src/task/mod.rs`

- [ ] **Step 1: Write test for correlation ID format**

In `crates/server/src/task/task_scheduler.rs`:
```rust
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
```

- [ ] **Step 2: Implement execution flow**

Create `crates/server/src/task/task_scheduler.rs`:
```rust
use std::sync::Arc;

use chrono::Utc;
use sea_orm::*;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::entity::{task, task_result};
use crate::state::AppState;
use serverbee_common::constants::{has_capability, CAP_EXEC};
use serverbee_common::protocol::ServerMessage;

/// Build correlation ID: {task_id}:{run_id}:{server_id}:{attempt}
pub fn build_correlation_id(task_id: &str, run_id: &str, server_id: &str, attempt: i32) -> String {
    format!("{task_id}:{run_id}:{server_id}:{attempt}")
}

/// Called by cron trigger or manual /run endpoint.
/// `skip_retry`: if true, executes once without retry (used by manual trigger).
/// Returns true if execution was started, false if skipped (overlap).
pub async fn execute_scheduled_task(state: &Arc<AppState>, task_id: &str, skip_retry: bool) -> bool {
    let scheduler = &state.task_scheduler;

    // Step 0: Atomic overlap check using DashMap entry API
    if scheduler.active_runs.contains_key(task_id) {
        tracing::warn!("Task {task_id} still running, skipping trigger");
        return false;
    }

    // Step 1: Load task from DB
    let task_model = match task::Entity::find_by_id(task_id).one(&state.db).await {
        Ok(Some(t)) => t,
        Ok(None) => { tracing::error!("Task {task_id} not found"); return; }
        Err(e) => { tracing::error!("Failed to load task {task_id}: {e}"); return; }
    };

    let server_ids: Vec<String> = serde_json::from_str(&task_model.server_ids_json)
        .unwrap_or_default();
    let timeout_secs = task_model.timeout.unwrap_or(300) as u64;
    let retry_count = if skip_retry { 0 } else { task_model.retry_count };
    let retry_interval = task_model.retry_interval as u64;

    // Step 2: Generate run_id, create cancellation token
    let run_id = uuid::Uuid::new_v4().to_string();
    let token = CancellationToken::new();
    scheduler.active_runs.insert(task_id.to_string(), (run_id.clone(), token.clone()));

    // Step 3: Update last_run_at and compute next_run_at from cron expression
    // Note: add `cron = "0.13"` to crates/server/Cargo.toml for Schedule parsing
    let next_run = task_model.cron_expression.as_deref().and_then(|cron_expr| {
        cron::Schedule::from_str(cron_expr).ok()
            .and_then(|s| s.upcoming(Utc).next())
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

    // Get capabilities for target servers only (not all servers)
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
                &state.db, task_id, &run_id, sid, -2,
                "Capability denied: exec not enabled",
            ).await;
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
                &state, &task_id, &run_id, &sid, &command,
                timeout_secs, retry_count, retry_interval, token,
            ).await;
        });
    }

    // Step 5: For servers without CAP_EXEC — already handled above

    // Step 6: Wait for all to complete, then clear active_runs
    // Uses a drop guard to ensure cleanup even on panic/cancellation
    let task_id_owned = task_id.to_string();
    let active_runs = scheduler.active_runs.clone();
    tokio::spawn(async move {
        // Drop guard: remove from active_runs when this scope exits (success or panic)
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
            active_runs: active_runs,
            task_id: task_id_owned,
        };
        while join_set.join_next().await.is_some() {}
        // _guard drops here, removing from active_runs
    });

    return true;
}

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
            // Check cancellation before retry
            if token.is_cancelled() { break; }
            tokio::time::sleep(std::time::Duration::from_secs(retry_interval)).await;
            if token.is_cancelled() { break; }
        }

        let correlation_id = build_correlation_id(task_id, run_id, server_id, attempt);
        let started_at = Utc::now();

        // Check agent online
        let sender = match state.agent_manager.get_sender(server_id) {
            Some(tx) => tx,
            None => {
                let _ = write_result(
                    &state.db, task_id, run_id, server_id, attempt, started_at,
                    -3, "Server offline",
                ).await;
                if attempt < max_attempts { continue; } else { break; }
            }
        };

        // Register pending request with TTL
        let rx = state.agent_manager.register_pending_request_with_ttl(
            correlation_id.clone(),
            std::time::Duration::from_secs(timeout_secs + 10),
        );

        // Send Exec
        let send_result = sender.send(ServerMessage::Exec {
            task_id: correlation_id,
            command: command.to_string(),
            timeout: Some(timeout_secs as u32),
        }).await;

        if send_result.is_err() {
            let _ = write_result(
                &state.db, task_id, run_id, server_id, attempt, started_at,
                -3, "Dispatch failed",
            ).await;
            if attempt < max_attempts { continue; } else { break; }
        }

        // Wait for response
        let result = tokio::time::timeout(timeout_duration, rx).await;

        match result {
            Ok(Ok(AgentMessage::TaskResult { result, .. })) => {
                let _ = write_result(
                    &state.db, task_id, run_id, server_id, attempt, started_at,
                    result.exit_code, &result.output,
                ).await;
                if result.exit_code == 0 { break; }
                // Non-zero: continue to retry if attempts remain
            }
            _ => {
                // Timeout or channel error
                let _ = write_result(
                    &state.db, task_id, run_id, server_id, attempt, started_at,
                    -4, &format!("No response within {timeout_secs}s"),
                ).await;
                // Counts as failure, eligible for retry
            }
        }

        if attempt == max_attempts { break; }
    }
}

use serverbee_common::protocol::AgentMessage;

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
    write_result(db, task_id, run_id, server_id, 1, Utc::now(), exit_code, output).await
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
```

- [ ] **Step 3: Register module**

In `crates/server/src/task/mod.rs`, add:
```rust
pub mod task_scheduler;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p serverbee-server task::task_scheduler::tests -v`
Expected: all tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/task/task_scheduler.rs crates/server/src/task/mod.rs
git commit -m "feat: add scheduled task execution flow with retry and cancellation"
```

---

## Task 6: AppState integration + main.rs spawn

**Files:**
- Modify: `crates/server/src/state.rs`
- Modify: `crates/server/src/main.rs`

- [ ] **Step 1: Add task_scheduler to AppState**

In `crates/server/src/state.rs`, add import:
```rust
use crate::service::task_scheduler::TaskScheduler;
```

Add field to `AppState` struct (after `file_transfers`):
```rust
pub task_scheduler: Arc<TaskScheduler>,
```

Update `AppState::new` to create and include the scheduler:
```rust
pub async fn new(db: DatabaseConnection, config: AppConfig) -> Result<Arc<Self>, AppError> {
    // ... existing code ...
    let task_scheduler = Arc::new(
        TaskScheduler::new(&config.scheduler.timezone).await?
    );
    Arc::new(Self {
        // ... existing fields ...
        task_scheduler,
    })
}
```

Note: `AppState::new` changes from sync to async, and now returns `Result`. Update `main.rs` accordingly.

- [ ] **Step 2: Update main.rs**

Update the `AppState::new` call in `main.rs` to `.await?`.

Add scheduler spawn after existing background tasks (after line 77):
```rust
let s = state.clone();
tokio::spawn(async move { task::task_scheduler::run(s).await });
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/state.rs crates/server/src/main.rs
git commit -m "feat: integrate TaskScheduler into AppState and spawn on startup"
```

---

## Task 7: Task API — GET list, PUT, DELETE

**Files:**
- Modify: `crates/server/src/router/api/task.rs`

- [ ] **Step 1: Add GET /api/tasks (list)**

Add new handler and request types:
```rust
#[derive(Deserialize, utoipa::IntoParams)]
pub struct ListTasksQuery {
    #[serde(rename = "type")]
    pub task_type: Option<String>,
}

#[utoipa::path(get, path = "/api/tasks", params(ListTasksQuery),
    responses((status = 200, body = Vec<TaskResponse>)),
    tag = "tasks"
)]
pub async fn list_tasks(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<ApiResponse<Vec<TaskResponse>>>, AppError> {
    let mut q = task::Entity::find();
    if let Some(t) = &query.task_type {
        q = q.filter(task::Column::TaskType.eq(t));
    }
    let tasks = q.order_by_desc(task::Column::CreatedAt).all(&state.db).await?;
    let results: Vec<TaskResponse> = tasks.into_iter().map(|t| t.into()).collect();
    Ok(Json(ApiResponse { data: results }))
}
```

Update `TaskResponse` to include scheduled task fields:
```rust
#[derive(Serialize, utoipa::ToSchema)]
pub struct TaskResponse {
    pub id: String,
    pub command: String,
    pub server_ids: Vec<String>,
    pub created_at: String,
    // New fields
    pub task_type: String,
    pub name: Option<String>,
    pub cron_expression: Option<String>,
    pub enabled: bool,
    pub timeout: Option<i32>,
    pub retry_count: i32,
    pub retry_interval: i32,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
}
```

- [ ] **Step 2: Add PUT /api/tasks/{id}**

```rust
#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateTaskRequest {
    pub name: Option<String>,
    pub command: Option<String>,
    pub server_ids: Option<Vec<String>>,
    pub cron_expression: Option<String>,
    pub enabled: Option<bool>,
    pub timeout: Option<i32>,
    pub retry_count: Option<i32>,
    pub retry_interval: Option<i32>,
}

#[utoipa::path(put, path = "/api/tasks/{id}",
    request_body = UpdateTaskRequest,
    responses((status = 200, body = TaskResponse)),
    tag = "tasks"
)]
pub async fn update_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateTaskRequest>,
) -> Result<Json<ApiResponse<TaskResponse>>, AppError> {
    // Load, update fields, save, sync scheduler
    // state.task_scheduler.update_job(&updated, state.clone()).await?;
}
```

- [ ] **Step 3: Add DELETE /api/tasks/{id}**

```rust
#[utoipa::path(delete, path = "/api/tasks/{id}",
    responses((status = 200)),
    tag = "tasks"
)]
pub async fn delete_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    // Cancel active run, remove scheduler job
    state.task_scheduler.remove_job(&id).await?;
    // Delete task_results, then task
    task_result::Entity::delete_many()
        .filter(task_result::Column::TaskId.eq(&id))
        .exec(&state.db).await?;
    task::Entity::delete_by_id(&id).exec(&state.db).await?;
    Ok(Json(ApiResponse { data: () }))
}
```

- [ ] **Step 4: Update router registration**

Update `pub fn router()` to include new routes:
```rust
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/tasks", get(list_tasks).post(create_task))
        .route("/tasks/{id}", get(get_task).put(update_task).delete(delete_task))
        .route("/tasks/{id}/results", get(get_task_results))
        .route("/tasks/{id}/run", post(run_task))
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/router/api/task.rs
git commit -m "feat: add task list, update, and delete API endpoints"
```

---

## Task 8: Task API — POST create (modified) + POST run

**Files:**
- Modify: `crates/server/src/router/api/task.rs`

- [ ] **Step 1: Modify create_task for scheduled type**

Update `CreateTaskRequest` to include scheduled fields:
```rust
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateTaskRequest {
    pub command: String,
    pub server_ids: Vec<String>,
    pub timeout: Option<u32>,
    // New fields
    pub task_type: Option<String>,      // "oneshot" (default) or "scheduled"
    pub name: Option<String>,
    pub cron_expression: Option<String>,
    pub retry_count: Option<i32>,
    pub retry_interval: Option<i32>,
}
```

In `create_task` handler, after inserting to DB:
```rust
if task_model.task_type == "scheduled" {
    state.task_scheduler.add_job(&task_model, state.clone()).await?;
} else {
    // Existing one-shot dispatch logic
}
```

- [ ] **Step 2: Add POST /api/tasks/{id}/run**

```rust
#[utoipa::path(post, path = "/api/tasks/{id}/run",
    responses(
        (status = 200, body = TaskResponse),
        (status = 409, description = "Task already running")
    ),
    tag = "tasks"
)]
pub async fn run_task(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<TaskResponse>>, AppError> {
    // Fire execution without retry — returns false if overlap detected
    let started = crate::task::task_scheduler::execute_scheduled_task(
        &state, &id, true,  // skip_retry = true for manual trigger
    ).await;
    if !started {
        return Err(AppError::Conflict("Task is currently running, try again later".into()));
    }
    // Return task info
    let task_model = task::Entity::find_by_id(&id).one(&state.db).await?
        .ok_or_else(|| AppError::NotFound("Task not found".into()))?;
    Ok(Json(ApiResponse { data: task_model.into() }))
}
```

- [ ] **Step 3: Update get_task_results for pagination**

Add pagination support:
```rust
#[derive(Deserialize, utoipa::IntoParams)]
pub struct PaginationQuery {
    pub page: Option<u64>,
    pub per_page: Option<u64>,
}
```

Update `get_task_results` to paginate and include new fields (`run_id`, `attempt`, `started_at`).

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/api/task.rs
git commit -m "feat: support scheduled task creation, manual run, and paginated results"
```

---

## Task 9: OpenAPI registration

**Files:**
- Modify: `crates/server/src/openapi.rs`

- [ ] **Step 1: Register new paths and schemas**

In the `paths()` block, add after existing task entries:
```rust
crate::router::api::task::list_tasks,
crate::router::api::task::update_task,
crate::router::api::task::delete_task,
crate::router::api::task::run_task,
```

In the `components(schemas(...))` block, add:
```rust
crate::router::api::task::UpdateTaskRequest,
crate::router::api::task::ListTasksQuery,
crate::router::api::task::PaginationQuery,
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/openapi.rs
git commit -m "feat: register scheduled task endpoints in OpenAPI"
```

---

## Task 10: Integration tests

**Files:**
- Modify: `crates/server/tests/integration.rs`

- [ ] **Step 1: Test scheduled task CRUD**

```rust
#[tokio::test]
async fn test_scheduled_task_crud() {
    // POST create scheduled task
    // GET list with ?type=scheduled
    // PUT update cron expression
    // DELETE task
    // Verify cascade deletes results
}
```

- [ ] **Step 2: Test manual run with 409 overlap**

```rust
#[tokio::test]
async fn test_scheduled_task_run_overlap_409() {
    // Create scheduled task
    // POST /run (should succeed)
    // POST /run while still running (should return 409)
}
```

- [ ] **Step 3: Run integration tests**

Run: `cargo test -p serverbee-server --test integration -v`
Expected: all tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/tests/integration.rs
git commit -m "test: add scheduled task integration tests"
```

---

## Task 11: Frontend — use-scheduled-tasks hook

**Files:**
- Create: `apps/web/src/hooks/use-scheduled-tasks.ts`

- [ ] **Step 1: Create hooks**

```typescript
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import { toast } from 'sonner'

export function useScheduledTasks() {
  return useQuery({
    queryKey: ['tasks', 'scheduled'],
    queryFn: () => api.get('/api/tasks?type=scheduled'),
    staleTime: 30_000,
  })
}

export function useTaskResults(taskId: string | null) {
  return useQuery({
    queryKey: ['tasks', taskId, 'results'],
    queryFn: () => api.get(`/api/tasks/${taskId}/results`),
    enabled: !!taskId,
    refetchInterval: taskId ? 5000 : false,
  })
}

export function useCreateScheduledTask() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateScheduledTaskInput) =>
      api.post('/api/tasks', { ...input, task_type: 'scheduled' }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] })
      toast.success('Scheduled task created')
    },
  })
}

export function useUpdateScheduledTask() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...input }: { id: string } & UpdateScheduledTaskInput) =>
      api.put(`/api/tasks/${id}`, input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] })
      toast.success('Task updated')
    },
  })
}

export function useDeleteScheduledTask() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api.delete(`/api/tasks/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] })
      toast.success('Task deleted')
    },
    onError: () => {
      toast.error('Failed to delete task')
    },
  })
}

export function useRunScheduledTask() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api.post(`/api/tasks/${id}/run`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tasks'] })
      toast.success('Task triggered')
    },
    onError: (error: any) => {
      if (error.status === 409) {
        toast.error('Task is currently running')
      }
    },
  })
}
```

- [ ] **Step 2: Commit**

```bash
git add apps/web/src/hooks/use-scheduled-tasks.ts
git commit -m "feat: add React Query hooks for scheduled task API"
```

---

## Task 12: Frontend — tasks page tab layout

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/tasks.tsx`
- Create: `apps/web/src/components/task/scheduled-task-list.tsx`

- [ ] **Step 1: Add tab layout to tasks page**

Wrap existing content in a Tabs component:
```tsx
<Tabs defaultValue="oneshot">
  <TabsList>
    <TabsTrigger value="oneshot">{t('tasks.oneshot')}</TabsTrigger>
    <TabsTrigger value="scheduled">{t('tasks.scheduled')}</TabsTrigger>
  </TabsList>
  <TabsContent value="oneshot">
    {/* Existing one-shot task UI — move here unchanged */}
  </TabsContent>
  <TabsContent value="scheduled">
    <ScheduledTaskList />
  </TabsContent>
</Tabs>
```

- [ ] **Step 2: Create ScheduledTaskList component**

Create `apps/web/src/components/task/scheduled-task-list.tsx`:
- Fetches tasks via `useScheduledTasks()`
- Table with columns: Name, Cron, Description (human-readable via `cronstrue`), Servers, Last Run, Next Run, Status (toggle), Actions
- "Create" button opens dialog
- "Run Now" button calls `useRunScheduledTask()`
- "Edit" opens dialog, "Delete" confirms then calls `useDeleteScheduledTask()`
- Row click expands execution history (Task 14)

Note: Add `cronstrue` dependency: `cd apps/web && bun add cronstrue`

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/settings/tasks.tsx apps/web/src/components/task/scheduled-task-list.tsx apps/web/package.json apps/web/bun.lockb
git commit -m "feat: add tab layout and scheduled task list"
```

---

## Task 13: Frontend — create/edit dialog

**Files:**
- Create: `apps/web/src/components/task/scheduled-task-dialog.tsx`

- [ ] **Step 1: Create dialog component**

Fields:
- Name (required, text input)
- Cron expression (text input) + human-readable preview via `cronstrue.toString(cron)`
- Command (monospace textarea)
- Server multi-select (reuse existing checkbox grid from one-shot tab, filter by CAP_EXEC)
- Timeout (number input, default 300)
- Retry count (number input 0-10, default 0)
- Retry interval (number input, default 60, shown only when retry_count > 0)

Validation:
- Name required for scheduled tasks
- Cron expression validated client-side (try `cronstrue.toString()`, show error if invalid)
- At least one server selected
- Command non-empty

Uses `useCreateScheduledTask()` or `useUpdateScheduledTask()` depending on mode.

- [ ] **Step 2: Commit**

```bash
git add apps/web/src/components/task/scheduled-task-dialog.tsx
git commit -m "feat: add scheduled task create/edit dialog"
```

---

## Task 14: Frontend — execution history

**Files:**
- Modify: `apps/web/src/components/task/scheduled-task-list.tsx`

- [ ] **Step 1: Add expandable execution history**

When a scheduled task row is clicked:
- Fetch results via `useTaskResults(taskId)`
- Group results by `run_id`
- Display group header: Trigger time | Overall status (all OK / N failed) | Total servers
- Expand group to see per-server rows: Server | Exit code | Attempt | Duration
- Click row to view full output in a dialog
- Exit code color coding: 0=green, -2=yellow (skipped), -3/-4=orange (infrastructure), other negative=red

- [ ] **Step 2: Add i18n keys**

Add translation keys for scheduled task UI text in `apps/web/src/i18n/locales/en/` and `cn/` namespace files. Key namespace: `tasks` (extend existing).

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/task/scheduled-task-list.tsx
git commit -m "feat: add execution history with run_id grouping"
```

---

## Task 15: Frontend tests

**Files:**
- Create: `apps/web/src/hooks/use-scheduled-tasks.test.ts`

- [ ] **Step 1: Write hook tests**

Test `useScheduledTasks` query key, `useRunScheduledTask` error handling for 409.

- [ ] **Step 2: Write component tests**

Test dialog form validation (empty name, invalid cron, no servers selected).

- [ ] **Step 3: Run frontend tests**

Run: `bun run test`
Expected: all tests PASS

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/hooks/use-scheduled-tasks.test.ts
git commit -m "test: add scheduled task frontend tests"
```

---

## Task 16: Regenerate API types + lint

**Files:**
- Modify: `apps/web/src/lib/api-types.ts`
- Modify: `apps/web/src/lib/api-schema.ts`

- [ ] **Step 1: Regenerate OpenAPI types**

Run: `cargo run --example dump_openapi > /tmp/openapi.json && cd apps/web && npx openapi-typescript /tmp/openapi.json -o src/lib/api-types.ts`

- [ ] **Step 2: Update api-schema.ts**

Add re-exports for new scheduled task types.

- [ ] **Step 3: Run lint**

Run: `bun x ultracite fix && bun run typecheck`
Expected: no errors

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/lib/api-types.ts apps/web/src/lib/api-schema.ts
git commit -m "chore: regenerate OpenAPI types for scheduled task API"
```

---

## Task 17: Documentation

**Files:**
- Modify: `ENV.md`
- Modify: `apps/docs/content/docs/{en,cn}/configuration.mdx`
- Modify: `TESTING.md`

- [ ] **Step 1: Update ENV.md**

Ensure `SERVERBEE_SCHEDULER__TIMEZONE` and `SERVERBEE_RETENTION__TASK_RESULTS_DAYS` are documented (may already exist from Plan A).

- [ ] **Step 2: Update configuration docs**

Add scheduled task configuration guidance to both EN and CN docs.

- [ ] **Step 3: Update TESTING.md**

Update test counts with new scheduled task tests, add test file locations.

- [ ] **Step 4: Commit**

```bash
git add ENV.md apps/docs/content/docs/ TESTING.md
git commit -m "docs: document scheduled tasks config and update test counts"
```

---

## Task 18: Final verification

- [ ] **Step 1: Run all Rust tests**

Run: `cargo test --workspace`
Expected: all tests PASS

- [ ] **Step 2: Run all frontend tests**

Run: `bun run test`
Expected: all tests PASS

- [ ] **Step 3: Run lints**

Run: `cargo clippy --workspace -- -D warnings && bun x ultracite check && bun run typecheck`
Expected: 0 warnings, 0 errors

- [ ] **Step 4: Build check**

Run: `cargo build --workspace && cd apps/web && bun run build`
Expected: builds successfully
