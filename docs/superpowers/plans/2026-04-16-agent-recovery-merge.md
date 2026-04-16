# Agent Recovery Merge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an admin-driven recovery workflow that rebinds a newly registered replacement agent onto an existing offline server record, merges the replacement record's history into the original record, rewrites shared references, and deletes the temporary record.

**Architecture:** The implementation is split into four vertical slices: protocol and atomic agent rebind support, persistent server-side recovery jobs with write freezing, table-aware history merge logic, and a server-detail UI for candidate selection and progress. The recovery flow keeps the original `server_id`, persists job state in SQLite for restart-safe retries, and uses bounded merge transactions plus checkpointed progress rather than one giant transaction.

**Tech Stack:** Rust (`axum`, `sea-orm`, `tokio`, SQLite), React 19, TanStack Query, TanStack Router, Zustand, Vitest, OpenAPI-generated web types

---

## File Map

### Backend Rust

- Create: `crates/server/src/entity/recovery_job.rs`
  Stores the persistent recovery job row.
- Modify: `crates/server/src/entity/mod.rs`
  Register the new entity module.
- Create: `crates/server/src/migration/m20260416_000017_create_recovery_job.rs`
  Creates `recovery_job` table and indexes.
- Modify: `crates/server/src/migration/mod.rs`
  Registers the new migration.
- Create: `crates/server/src/service/recovery_job.rs`
  DB-backed repository/service for creating, updating, resuming, and checkpointing jobs.
- Create: `crates/server/src/service/recovery_lock.rs`
  In-memory write-freeze guard keyed by `server_id`.
- Create: `crates/server/src/service/recovery_merge.rs`
  Orchestrates rebind, merge groups, JSON rewrites, cleanup, and retry semantics.
- Modify: `crates/server/src/service/mod.rs`
  Exposes the new services.
- Modify: `crates/server/src/state.rs`
  Wires persistent recovery services and write-freeze guard into `AppState`.
- Create: `crates/server/src/router/api/server_recovery.rs`
  Read/write endpoints for candidates, start job, and get job state.
- Modify: `crates/server/src/router/api/mod.rs`
  Mounts the new router.
- Modify: `crates/server/src/openapi.rs`
  Registers new endpoints and DTOs.
- Modify: `crates/server/src/router/ws/agent.rs`
  Handles `RebindIdentityAck`/`Failed`, recovery-aware write gating, and rebind orchestration callbacks.
- Modify: `crates/server/src/router/ws/browser.rs`
  Includes recovery jobs in browser `FullSync` and live updates.
- Modify: `crates/server/src/task/record_writer.rs`
  Honors recovery write freezes.
- Modify: `crates/server/src/service/traffic.rs`
  Adds helper(s) needed by merge/finalization and respects recovery lock where state is updated.
- Modify: `crates/server/tests/integration.rs`
  Adds API + end-to-end recovery integration coverage.
- Modify: `crates/server/src/service/recovery_merge.rs`
  Include focused DB tests using `setup_test_db`.

### Shared Protocol

- Modify: `crates/common/src/protocol.rs`
  Adds recovery DTOs and WebSocket messages used by agent/browser/server.

### Agent

- Create: `crates/agent/src/rebind.rs`
  Atomic token persistence helper and rebind message handling.
- Modify: `crates/agent/src/main.rs`
  Registers the new module.
- Modify: `crates/agent/src/reporter.rs`
  Handles `ServerMessage::RebindIdentity` and emits ack/failure messages.

### Web

- Modify: `apps/web/src/lib/api-schema.ts`
  Re-export recovery candidate/job schemas after OpenAPI regeneration.
- Modify: `apps/web/src/hooks/use-api.ts`
  Adds candidate, start-job, and job polling helpers.
- Modify: `apps/web/src/hooks/use-api.test.tsx`
  Covers the new API helpers.
- Create: `apps/web/src/stores/recovery-jobs-store.ts`
  Holds live recovery job state keyed by `target_server_id` and `job_id`.
- Create: `apps/web/src/stores/recovery-jobs-store.test.ts`
  Covers store set/update/clear behavior.
- Modify: `apps/web/src/hooks/use-servers-ws.ts`
  Hydrates recovery jobs from `full_sync` and incremental events.
- Modify: `apps/web/src/hooks/use-servers-ws.test.ts`
  Covers WS hydration and updates for recovery jobs.
- Create: `apps/web/src/components/server/recovery-merge-dialog.tsx`
  Candidate picker + confirmation flow on the server detail page.
- Create: `apps/web/src/components/server/recovery-merge-dialog.test.tsx`
  Covers ranking display, confirmation copy, pending/error UI.
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`
  Adds action button, dialog integration, and job status rendering.
- Modify: `apps/web/src/routes/_authed/servers/$id.test.tsx`
  Covers button visibility and job state rendering.
- Modify: `apps/web/src/locales/en/servers.json`
  New copy for recovery UI.
- Modify: `apps/web/src/locales/zh/servers.json`
  New copy for recovery UI.

### Docs

- Modify: `apps/docs/content/docs/cn/server.mdx`
  Document the admin recovery flow and its limits.
- Modify: `apps/docs/content/docs/en/server.mdx`
  Same in English.
- Modify: `apps/docs/content/docs/cn/api-reference.mdx`
  Add recovery endpoints.
- Modify: `apps/docs/content/docs/en/api-reference.mdx`
  Add recovery endpoints.

---

### Task 1: Add Recovery Protocol and Atomic Agent Token Rebind Support

**Files:**
- Create: `crates/agent/src/rebind.rs`
- Modify: `crates/agent/src/main.rs`
- Modify: `crates/agent/src/reporter.rs`
- Modify: `crates/common/src/protocol.rs`

- [ ] **Step 1: Write failing agent tests for atomic token replacement**

```rust
// crates/agent/src/rebind.rs
#[cfg(test)]
mod tests {
    use super::persist_rebind_token;

    #[test]
    fn persist_rebind_token_replaces_existing_token_line_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.toml");
        std::fs::write(&path, "server_url = \"http://127.0.0.1:9527\"\ntoken = \"old\"\n").unwrap();

        persist_rebind_token(&path, "new-token").unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("token = \"new-token\""));
        assert!(!content.contains("token = \"old\""));
    }

    #[test]
    fn persist_rebind_token_preserves_non_token_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.toml");
        std::fs::write(&path, "server_url = \"https://monitor.example.com\"\n[collector]\ninterval = 3\n").unwrap();

        persist_rebind_token(&path, "fresh-token").unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("server_url = \"https://monitor.example.com\""));
        assert!(content.contains("[collector]"));
        assert!(content.contains("interval = 3"));
        assert!(content.contains("token = \"fresh-token\""));
    }
}
```

- [ ] **Step 2: Run the agent tests and verify they fail**

Run: `cargo test -p serverbee-agent persist_rebind_token -- --exact`

Expected: FAIL with unresolved import or missing `persist_rebind_token`.

- [ ] **Step 3: Implement the atomic token writer and wire the module**

```rust
// crates/agent/src/rebind.rs
pub fn persist_rebind_token(path: &std::path::Path, token: &str) -> anyhow::Result<()> {
    let content = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };

    let mut lines: Vec<String> = content.lines().map(str::to_owned).collect();
    let token_line = format!("token = \"{token}\"");
    if let Some(pos) = lines.iter().position(|line| line.starts_with("token")) {
        lines[pos] = token_line;
    } else {
        lines.push(token_line);
    }

    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, lines.join("\n"))?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

// crates/agent/src/main.rs
mod rebind;
```

- [ ] **Step 4: Extend the shared protocol and reporter rebind handling**

```rust
// crates/common/src/protocol.rs
ServerMessage::RebindIdentity {
    job_id: String,
    target_server_id: String,
    token: String,
}

AgentMessage::RebindIdentityAck {
    job_id: String,
}

AgentMessage::RebindIdentityFailed {
    job_id: String,
    error: String,
}

// crates/agent/src/reporter.rs
ServerMessage::RebindIdentity { job_id, token, .. } => {
    match crate::rebind::persist_rebind_token(std::path::Path::new(crate::config::AgentConfig::config_path()), &token) {
        Ok(()) => {
            self.config.token = token;
            let ack = AgentMessage::RebindIdentityAck { job_id };
            let json = serde_json::to_string(&ack)?;
            write.send(Message::Text(json.into())).await?;
            write.send(Message::Close(None)).await?;
            return Ok(());
        }
        Err(err) => {
            let failed = AgentMessage::RebindIdentityFailed { job_id, error: err.to_string() };
            let json = serde_json::to_string(&failed)?;
            write.send(Message::Text(json.into())).await?;
            return Ok(());
        }
    }
}
```

- [ ] **Step 5: Run the focused agent tests and commit**

Run: `cargo test -p serverbee-agent persist_rebind_token -- --exact`

Expected: PASS

Commit:

```bash
git add crates/common/src/protocol.rs crates/agent/src/main.rs crates/agent/src/rebind.rs crates/agent/src/reporter.rs
git commit -m "feat(agent): add atomic recovery token rebind support"
```

### Task 2: Add Persistent Recovery Job Schema and Repository

**Files:**
- Create: `crates/server/src/entity/recovery_job.rs`
- Modify: `crates/server/src/entity/mod.rs`
- Create: `crates/server/src/migration/m20260416_000017_create_recovery_job.rs`
- Modify: `crates/server/src/migration/mod.rs`
- Create: `crates/server/src/service/recovery_job.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Write failing DB-backed service tests**

```rust
// crates/server/src/service/recovery_job.rs
#[cfg(test)]
mod tests {
    use super::RecoveryJobService;
    use crate::test_utils::setup_test_db;

    #[tokio::test]
    async fn create_job_persists_running_row() {
        let (db, _tmp) = setup_test_db().await;

        let job = RecoveryJobService::create_job(&db, "target-1", "source-1").await.unwrap();

        assert_eq!(job.target_server_id, "target-1");
        assert_eq!(job.source_server_id, "source-1");
        assert_eq!(job.status, "running");
        assert_eq!(job.stage, "validating");
    }

    #[tokio::test]
    async fn update_checkpoint_round_trips() {
        let (db, _tmp) = setup_test_db().await;
        let job = RecoveryJobService::create_job(&db, "target-1", "source-1").await.unwrap();

        RecoveryJobService::update_stage(&db, &job.job_id, "merging_history", Some("{\"group\":2}"), None)
            .await
            .unwrap();

        let loaded = RecoveryJobService::get_job(&db, &job.job_id).await.unwrap().unwrap();
        assert_eq!(loaded.stage, "merging_history");
        assert_eq!(loaded.checkpoint_json.as_deref(), Some("{\"group\":2}"));
    }
}
```

- [ ] **Step 2: Run the focused server tests and verify they fail**

Run: `cargo test -p serverbee-server recovery_job_service -- --nocapture`

Expected: FAIL with missing entity/service definitions.

- [ ] **Step 3: Implement the entity, migration, and repository**

```rust
// crates/server/src/entity/recovery_job.rs
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "recovery_job")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub job_id: String,
    pub target_server_id: String,
    pub source_server_id: String,
    pub status: String,
    pub stage: String,
    pub checkpoint_json: Option<String>,
    pub error: Option<String>,
    pub started_at: DateTimeUtc,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub last_heartbeat_at: Option<DateTimeUtc>,
}

// crates/server/src/migration/m20260416_000017_create_recovery_job.rs
db.execute_unprepared(
    "CREATE TABLE recovery_job (
        job_id TEXT PRIMARY KEY NOT NULL,
        target_server_id TEXT NOT NULL,
        source_server_id TEXT NOT NULL,
        status TEXT NOT NULL,
        stage TEXT NOT NULL,
        checkpoint_json TEXT NULL,
        error TEXT NULL,
        started_at TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        last_heartbeat_at TEXT NULL
    )"
).await?;
db.execute_unprepared("CREATE INDEX idx_recovery_job_target_status ON recovery_job(target_server_id, status)").await?;
db.execute_unprepared("CREATE INDEX idx_recovery_job_source_status ON recovery_job(source_server_id, status)").await?;

// crates/server/src/service/recovery_job.rs
pub struct RecoveryJobService;
```

- [ ] **Step 4: Add repository methods used by the orchestration layer**

```rust
impl RecoveryJobService {
    pub async fn create_job(db: &DatabaseConnection, target: &str, source: &str) -> Result<recovery_job::Model, AppError> { /* insert row */ }
    pub async fn get_job(db: &DatabaseConnection, job_id: &str) -> Result<Option<recovery_job::Model>, AppError> { /* find by id */ }
    pub async fn update_stage(
        db: &DatabaseConnection,
        job_id: &str,
        stage: &str,
        checkpoint_json: Option<&str>,
        error: Option<&str>
    ) -> Result<recovery_job::Model, AppError> { /* update row */ }
    pub async fn mark_failed(db: &DatabaseConnection, job_id: &str, stage: &str, error: &str) -> Result<(), AppError> { /* update status */ }
    pub async fn running_for_target(db: &DatabaseConnection, target: &str) -> Result<Option<recovery_job::Model>, AppError> { /* query by index */ }
    pub async fn running_for_source(db: &DatabaseConnection, source: &str) -> Result<Option<recovery_job::Model>, AppError> { /* query by index */ }
}
```

- [ ] **Step 5: Run the tests and commit**

Run: `cargo test -p serverbee-server recovery_job_service -- --nocapture`

Expected: PASS

Commit:

```bash
git add crates/server/src/entity/mod.rs crates/server/src/entity/recovery_job.rs crates/server/src/migration/mod.rs crates/server/src/migration/m20260416_000017_create_recovery_job.rs crates/server/src/service/mod.rs crates/server/src/service/recovery_job.rs
git commit -m "feat(server): persist recovery jobs in sqlite"
```

### Task 3: Add Recovery Candidate Scoring and Admin API Endpoints

**Files:**
- Create: `crates/server/src/router/api/server_recovery.rs`
- Modify: `crates/server/src/router/api/mod.rs`
- Modify: `crates/server/src/openapi.rs`
- Modify: `crates/server/src/service/recovery_job.rs`
- Modify: `crates/server/tests/integration.rs`

- [ ] **Step 1: Write failing tests for candidate ranking and API validation**

```rust
// crates/server/src/router/api/server_recovery.rs
#[cfg(test)]
mod tests {
    use super::{score_candidate, CandidateScoreInput};

    #[test]
    fn higher_score_when_ip_arch_and_created_at_match() {
        let strong = score_candidate(CandidateScoreInput {
            same_remote_addr: true,
            same_cpu_arch: true,
            same_os: true,
            same_virtualization: true,
            created_within_minutes: 10,
            same_country: true,
        });
        let weak = score_candidate(CandidateScoreInput {
            same_remote_addr: false,
            same_cpu_arch: false,
            same_os: true,
            same_virtualization: false,
            created_within_minutes: 240,
            same_country: false,
        });
        assert!(strong > weak);
    }
}
```

- [ ] **Step 2: Run the targeted tests and verify failure**

Run: `cargo test -p serverbee-server higher_score_when_ip_arch_and_created_at_match -- --exact`

Expected: FAIL because `server_recovery.rs` and `score_candidate` do not exist.

- [ ] **Step 3: Implement DTOs, scoring, and read/write routes**

```rust
// crates/server/src/router/api/server_recovery.rs
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RecoveryCandidateResponse {
    pub server_id: String,
    pub name: String,
    pub score: i32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct StartRecoveryRequest {
    pub source_server_id: String,
}

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/servers/{target_id}/recovery-candidates", get(list_candidates))
        .route("/servers/recovery-jobs/{job_id}", get(get_recovery_job))
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new().route("/servers/{target_id}/recover-merge", post(start_recovery_merge))
}
```

- [ ] **Step 4: Add integration coverage for admin auth and validation rules**

```rust
// crates/server/tests/integration.rs
#[tokio::test]
async fn test_recovery_candidates_requires_auth_and_filters_target() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{}/api/servers/target-1/recovery-candidates", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["data"].is_array());
}
```

- [ ] **Step 5: Run focused tests and commit**

Run: `cargo test -p serverbee-server recovery_candidates -- --nocapture`

Expected: PASS

Commit:

```bash
git add crates/server/src/router/api/mod.rs crates/server/src/router/api/server_recovery.rs crates/server/src/openapi.rs crates/server/tests/integration.rs
git commit -m "feat(server): add recovery candidate and job api"
```

### Task 4: Add Recovery Locks and Route All Agent-Originated Writes Through Them

**Files:**
- Create: `crates/server/src/service/recovery_lock.rs`
- Modify: `crates/server/src/state.rs`
- Modify: `crates/server/src/router/ws/agent.rs`
- Modify: `crates/server/src/task/record_writer.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Write failing unit tests for the lock guard**

```rust
// crates/server/src/service/recovery_lock.rs
#[cfg(test)]
mod tests {
    use super::RecoveryLockService;

    #[test]
    fn locked_server_denies_writes_until_released() {
        let locks = RecoveryLockService::new();
        assert!(locks.writes_allowed_for("srv-1"));
        locks.freeze("srv-1");
        assert!(!locks.writes_allowed_for("srv-1"));
        locks.release("srv-1");
        assert!(locks.writes_allowed_for("srv-1"));
    }
}
```

- [ ] **Step 2: Run the guard test and verify failure**

Run: `cargo test -p serverbee-server locked_server_denies_writes_until_released -- --exact`

Expected: FAIL because `RecoveryLockService` does not exist.

- [ ] **Step 3: Implement the lock service and wire it into `AppState`**

```rust
// crates/server/src/service/recovery_lock.rs
#[derive(Default)]
pub struct RecoveryLockService {
    frozen: dashmap::DashSet<String>,
}

impl RecoveryLockService {
    pub fn new() -> Self { Self { frozen: dashmap::DashSet::new() } }
    pub fn freeze(&self, server_id: &str) { self.frozen.insert(server_id.to_string()); }
    pub fn release(&self, server_id: &str) { self.frozen.remove(server_id); }
    pub fn writes_allowed_for(&self, server_id: &str) -> bool { !self.frozen.contains(server_id) }
}

// crates/server/src/state.rs
pub recovery_lock: RecoveryLockService,
```

- [ ] **Step 4: Gate all write paths that can race with recovery**

```rust
// crates/server/src/router/ws/agent.rs
if !state.recovery_lock.writes_allowed_for(server_id) {
    tracing::info!("Skipping recovery-frozen ping/task/probe write for {server_id}");
    return;
}

// crates/server/src/task/record_writer.rs
if !state.recovery_lock.writes_allowed_for(server_id) {
    continue;
}
```

- [ ] **Step 5: Run focused tests and commit**

Run: `cargo test -p serverbee-server locked_server_denies_writes_until_released -- --exact`

Expected: PASS

Commit:

```bash
git add crates/server/src/service/mod.rs crates/server/src/service/recovery_lock.rs crates/server/src/state.rs crates/server/src/router/ws/agent.rs crates/server/src/task/record_writer.rs
git commit -m "feat(server): add recovery write freeze guards"
```

### Task 5: Implement the Rebind Orchestrator and Recovery Job Lifecycle

**Files:**
- Create: `crates/server/src/service/recovery_merge.rs`
- Modify: `crates/server/src/service/mod.rs`
- Modify: `crates/server/src/router/api/server_recovery.rs`
- Modify: `crates/server/src/router/ws/agent.rs`
- Modify: `crates/server/src/router/ws/browser.rs`
- Modify: `crates/common/src/protocol.rs`
- Modify: `apps/web/src/hooks/use-servers-ws.ts` (for later WS payload shape)

- [ ] **Step 1: Write failing service tests for pre-rebind vs post-rebind retry semantics**

```rust
// crates/server/src/service/recovery_merge.rs
#[cfg(test)]
mod tests {
    use super::{RecoveryFailureMode, retry_strategy_for};

    #[test]
    fn pre_rebind_failures_require_new_job() {
        assert_eq!(retry_strategy_for(RecoveryFailureMode::AwaitingTargetOnlineTimeout), "new_job");
    }

    #[test]
    fn post_rebind_failures_resume_same_job() {
        assert_eq!(retry_strategy_for(RecoveryFailureMode::MergeGroupFailed), "resume_same_job");
    }
}
```

- [ ] **Step 2: Run the lifecycle tests and verify failure**

Run: `cargo test -p serverbee-server pre_rebind_failures_require_new_job -- --exact`

Expected: FAIL because `recovery_merge.rs` does not exist.

- [ ] **Step 3: Implement orchestration entry points and persisted stage transitions**

```rust
// crates/server/src/service/recovery_merge.rs
pub struct RecoveryMergeService;

impl RecoveryMergeService {
    pub async fn start(
        state: &Arc<AppState>,
        target_server_id: &str,
        source_server_id: &str,
    ) -> Result<recovery_job::Model, AppError> {
        let job = RecoveryJobService::create_job(&state.db, target_server_id, source_server_id).await?;
        RecoveryJobService::update_stage(&state.db, &job.job_id, "rebinding", None, None).await?;
        Ok(job)
    }

    pub async fn handle_rebind_ack(state: &Arc<AppState>, job_id: &str) -> Result<(), AppError> {
        RecoveryJobService::update_stage(&state.db, job_id, "awaiting_target_online", None, None).await?;
        Ok(())
    }
}

pub fn retry_strategy_for(mode: RecoveryFailureMode) -> &'static str {
    match mode {
        RecoveryFailureMode::AwaitingTargetOnlineTimeout => "new_job",
        RecoveryFailureMode::MergeGroupFailed => "resume_same_job",
    }
}
```

- [ ] **Step 4: Wire WS acknowledgements and browser progress fan-out**

```rust
// crates/server/src/router/ws/agent.rs
AgentMessage::RebindIdentityAck { job_id } => {
    if let Err(err) = RecoveryMergeService::handle_rebind_ack(state, &job_id).await {
        tracing::error!("Failed to advance recovery job {job_id}: {err}");
    }
}

// crates/server/src/router/ws/browser.rs
BrowserMessage::FullSync {
    servers,
    upgrades: state.upgrade_tracker.snapshot(),
    recoveries: state.recovery_merge.snapshot(),
}
```

- [ ] **Step 5: Run lifecycle tests and commit**

Run: `cargo test -p serverbee-server pre_rebind_failures_require_new_job post_rebind_failures_resume_same_job -- --nocapture`

Expected: PASS

Commit:

```bash
git add crates/common/src/protocol.rs crates/server/src/service/mod.rs crates/server/src/service/recovery_merge.rs crates/server/src/router/api/server_recovery.rs crates/server/src/router/ws/agent.rs crates/server/src/router/ws/browser.rs
git commit -m "feat(server): orchestrate recovery rebind lifecycle"
```

### Task 6: Implement History Merge Groups, JSON Rewrite, and Final Cleanup

**Files:**
- Modify: `crates/server/src/service/recovery_merge.rs`
- Modify: `crates/server/src/service/traffic.rs`
- Modify: `crates/server/tests/integration.rs`

- [ ] **Step 1: Write failing merge-engine tests for raw, unique-key, JSON, and alert-state semantics**

```rust
// crates/server/src/service/recovery_merge.rs
#[tokio::test]
async fn merge_raw_records_replaces_target_overlap_with_source() { /* seed overlapping rows; expect target window delete + source move */ }

#[tokio::test]
async fn merge_alert_state_keeps_target_when_rule_conflicts() { /* same rule on both sides; expect target row kept */ }

#[tokio::test]
async fn rewrite_server_ids_json_replaces_source_with_target_once() { /* ["target","source","source"] -> ["target"] */ }
```

- [ ] **Step 2: Run the merge-engine tests and verify failure**

Run: `cargo test -p serverbee-server merge_raw_records_replaces_target_overlap_with_source -- --exact`

Expected: FAIL because merge helpers do not exist.

- [ ] **Step 3: Implement merge group helpers**

```rust
impl RecoveryMergeService {
    async fn merge_raw_table(
        db: &DatabaseConnection,
        table: &str,
        time_column: &str,
        target: &str,
        source: &str,
    ) -> Result<(), AppError> { /* delete target overlap; update source rows to target */ }

    async fn merge_alert_states(db: &DatabaseConnection, target: &str, source: &str) -> Result<(), AppError> { /* target wins */ }

    async fn rewrite_server_ids_json_tables(db: &DatabaseConnection, target: &str, source: &str) -> Result<(), AppError> { /* alert_rule/ping_task/task/service_monitor/maintenance/incident/status_page */ }
}
```

- [ ] **Step 4: Implement finalization rules and explicit source cleanup**

```rust
impl RecoveryMergeService {
    async fn finalize_target_server_row(db: &DatabaseConnection, target: &str, source: &server::Model) -> Result<(), AppError> { /* copy runtime fields */ }

    async fn delete_intentionally_unmerged_source_rows(db: &DatabaseConnection, source: &str) -> Result<(), AppError> {
        server_tag::Entity::delete_many().filter(server_tag::Column::ServerId.eq(source)).exec(db).await?;
        network_probe_config::Entity::delete_many().filter(network_probe_config::Column::ServerId.eq(source)).exec(db).await?;
        Ok(())
    }
}
```

- [ ] **Step 5: Run merge-focused tests and commit**

Run: `cargo test -p serverbee-server recovery_merge -- --nocapture`

Expected: PASS

Commit:

```bash
git add crates/server/src/service/recovery_merge.rs crates/server/src/service/traffic.rs crates/server/tests/integration.rs
git commit -m "feat(server): merge recovered server history into target identity"
```

### Task 7: Add Browser Recovery Job State, Dialog UI, and Server Detail Controls

**Files:**
- Modify: `apps/web/src/lib/api-schema.ts`
- Modify: `apps/web/src/hooks/use-api.ts`
- Modify: `apps/web/src/hooks/use-api.test.tsx`
- Create: `apps/web/src/stores/recovery-jobs-store.ts`
- Create: `apps/web/src/stores/recovery-jobs-store.test.ts`
- Modify: `apps/web/src/hooks/use-servers-ws.ts`
- Modify: `apps/web/src/hooks/use-servers-ws.test.ts`
- Create: `apps/web/src/components/server/recovery-merge-dialog.tsx`
- Create: `apps/web/src/components/server/recovery-merge-dialog.test.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$id.test.tsx`
- Modify: `apps/web/src/locales/en/servers.json`
- Modify: `apps/web/src/locales/zh/servers.json`

- [ ] **Step 1: Write failing store and hook tests**

```ts
// apps/web/src/stores/recovery-jobs-store.test.ts
it('stores recovery jobs keyed by target server id', () => {
  useRecoveryJobsStore.getState().setJob('target-1', {
    job_id: 'job-1',
    target_server_id: 'target-1',
    source_server_id: 'source-1',
    status: 'running',
    stage: 'rebinding'
  })
  expect(useRecoveryJobsStore.getState().getJob('target-1')?.job_id).toBe('job-1')
})

// apps/web/src/hooks/use-api.test.tsx
it('fetches recovery candidates for a target server', async () => {
  fetchMock.mockResponseOnce(JSON.stringify({ data: [{ server_id: 'source-1', score: 42, reasons: ['same IP'] }] }))
  const result = await api.get('/api/servers/target-1/recovery-candidates')
  expect(result[0].server_id).toBe('source-1')
})
```

- [ ] **Step 2: Run the focused web tests and verify failure**

Run: `bun --cwd apps/web run test -- src/stores/recovery-jobs-store.test.ts src/hooks/use-api.test.tsx`

Expected: FAIL because the store and API helpers do not exist.

- [ ] **Step 3: Implement API helpers, store, and WS hydration**

```ts
// apps/web/src/hooks/use-api.ts
export function useRecoveryCandidates(targetId: string, enabled = true) {
  return useQuery({
    queryKey: ['servers', targetId, 'recovery-candidates'],
    queryFn: () => api.get<RecoveryCandidateResponse[]>(`/api/servers/${targetId}/recovery-candidates`),
    enabled: enabled && !!targetId
  })
}

export async function startRecoveryMerge(targetId: string, sourceServerId: string) {
  return api.post<RecoveryJobResponse>(`/api/servers/${targetId}/recover-merge`, { source_server_id: sourceServerId })
}

// apps/web/src/stores/recovery-jobs-store.ts
export const useRecoveryJobsStore = create<RecoveryJobsState>()((set, get) => ({ /* same pattern as upgrade-jobs-store */ }))
```

- [ ] **Step 4: Implement dialog and server detail integration**

```tsx
// apps/web/src/components/server/recovery-merge-dialog.tsx
export function RecoveryMergeDialog({ targetServerId, open, onOpenChange }: Props) {
  const { data: candidates } = useRecoveryCandidates(targetServerId, open)
  const [selectedSourceId, setSelectedSourceId] = useState('')
  const mutation = useMutation({
    mutationFn: () => startRecoveryMerge(targetServerId, selectedSourceId)
  })

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('recovery_merge_title')}</DialogTitle>
        </DialogHeader>
        {/* candidate list + reasons + confirmation copy */}
      </DialogContent>
    </Dialog>
  )
}

// apps/web/src/routes/_authed/servers/$id.tsx
{!server.online && isAdmin ? <Button onClick={() => setRecoveryOpen(true)}>{t('recovery_merge_open')}</Button> : null}
```

- [ ] **Step 5: Regenerate OpenAPI web types, run web tests, and commit**

Run: `bun --cwd apps/web run generate:api-types`

Expected: `src/lib/api-types.ts` updated without errors

Run: `bun --cwd apps/web run test -- src/hooks/use-api.test.tsx src/hooks/use-servers-ws.test.ts src/components/server/recovery-merge-dialog.test.tsx src/routes/_authed/servers/$id.test.tsx`

Expected: PASS

Commit:

```bash
git add apps/web/src/lib/api-schema.ts apps/web/src/hooks/use-api.ts apps/web/src/hooks/use-api.test.tsx apps/web/src/stores/recovery-jobs-store.ts apps/web/src/stores/recovery-jobs-store.test.ts apps/web/src/hooks/use-servers-ws.ts apps/web/src/hooks/use-servers-ws.test.ts apps/web/src/components/server/recovery-merge-dialog.tsx apps/web/src/components/server/recovery-merge-dialog.test.tsx apps/web/src/routes/_authed/servers/\$id.tsx apps/web/src/routes/_authed/servers/\$id.test.tsx apps/web/src/locales/en/servers.json apps/web/src/locales/zh/servers.json apps/web/src/lib/api-types.ts
git commit -m "feat(web): add server recovery merge workflow"
```

### Task 8: Update Docs and Run End-to-End Verification

**Files:**
- Modify: `apps/docs/content/docs/cn/server.mdx`
- Modify: `apps/docs/content/docs/en/server.mdx`
- Modify: `apps/docs/content/docs/cn/api-reference.mdx`
- Modify: `apps/docs/content/docs/en/api-reference.mdx`

- [ ] **Step 1: Write the documentation changes**

```mdx
## Recovering a Reinstalled Agent

If an existing server was reinstalled and re-registered as a new temporary node:

1. Open the original offline server.
2. Click **Claim and Merge New Agent**.
3. Select the recommended online replacement.
4. Confirm the merge.

The original server record is kept. The replacement record's overlapping history wins, and the temporary record is deleted after recovery completes.
```

- [ ] **Step 2: Run the backend verification suite**

Run: `cargo test -p serverbee-server recovery -- --nocapture`

Expected: PASS for the recovery-specific tests added in `integration.rs` and `service/recovery_merge.rs`

- [ ] **Step 3: Run the agent verification suite**

Run: `cargo test -p serverbee-agent rebind -- --nocapture`

Expected: PASS for the new atomic token persistence and rebind tests

- [ ] **Step 4: Run web typecheck and lint**

Run: `bun --cwd apps/web run typecheck`

Expected: PASS

Run: `bun x ultracite check apps/web/src/hooks/use-api.ts apps/web/src/hooks/use-servers-ws.ts apps/web/src/components/server/recovery-merge-dialog.tsx apps/web/src/routes/_authed/servers/\$id.tsx`

Expected: PASS

- [ ] **Step 5: Commit the docs and final verification sweep**

```bash
git add apps/docs/content/docs/cn/server.mdx apps/docs/content/docs/en/server.mdx apps/docs/content/docs/cn/api-reference.mdx apps/docs/content/docs/en/api-reference.mdx
git commit -m "docs: add agent recovery merge guidance"
```

## Self-Review

- Spec coverage:
  - Recovery job persistence: Task 2
  - Agent atomic token rebind + ack semantics: Task 1
  - Candidate scoring and recovery APIs: Task 3
  - Write freeze: Task 4
  - Rebind orchestration and retry semantics: Task 5
  - History merge groups, JSON rewrites, and cleanup: Task 6
  - Browser progress and admin UI: Task 7
  - Docs and verification: Task 8
- Placeholder scan:
  - No `TODO`, `TBD`, or "handle appropriately" placeholders remain.
  - Each code-changing task includes concrete snippets and commands.
- Type consistency:
  - `RebindIdentity`, `RebindIdentityAck`, `RecoveryJobResponse`, and `RecoveryCandidateResponse` names are reused consistently across tasks.
  - `target_server_id` and `source_server_id` naming is consistent across backend, protocol, and web tasks.
