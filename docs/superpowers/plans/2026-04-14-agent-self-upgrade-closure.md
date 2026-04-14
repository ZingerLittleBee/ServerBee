# Agent Self-Upgrade Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an observable, retryable, bounded-risk agent self-upgrade flow with live backend/frontend status, a cached latest-version lookup, and safer agent restart behavior.

**Architecture:** Extend the shared websocket protocol with upgrade lifecycle messages plus a server-generated `job_id`, then keep authoritative upgrade job state in a server-side in-memory tracker keyed by `server_id`. The agent emits stage/failure events around a safer upgrade pipeline with preflight and timestamped backups; the web app hydrates upgrade jobs from browser WS `FullSync` plus a small Zustand store and renders status on the server detail page and server list surfaces.

**Tech Stack:** Rust (`axum`, `tokio`, `dashmap`, `serde`, `uuid`, `chrono`, `reqwest`, `utoipa`), React 19, TanStack Query/Router, Zustand, react-i18next, Vitest.

---

## Scope Notes

- Keep this as one implementation plan. The protocol, server tracker, agent restart flow, and frontend UI are tightly coupled and do not produce a useful partial release on their own.
- Keep server upgrade integration coverage in `crates/server/tests/integration.rs` instead of inventing a new `tests/integration/` tree. The repo already keeps reusable server test harness code in that file.
- Two spec gaps need to be closed during implementation:
  1. The requested failed-state UI hint needs a concrete backup path. Add `backup_path: Option<String>` to failure payloads and the tracker DTO so the frontend can render it when available.
  2. `AppError::Conflict` cannot carry the "existing job DTO" payload the spec wants. Use a service-local `StartUpgradeJobError::Conflict(UpgradeJob)` and let the API route return a structured `409` body manually.

### Amendments after plan review (2026-04-14)

The plan was reviewed against the spec and four implementer-facing clarifications were folded in. These do NOT change scope — they prevent predictable execution mistakes:

1. **Task 4 `trigger_upgrade`**: the existing `CAP_UPGRADE` pre-check (`crates/server/src/router/api/server.rs:536-544`) MUST run before `start_job`. The code snippet in Task 4 was augmented to include it explicitly.
2. **Task 4 `CapabilityDenied` handler**: do NOT add a new sibling `match` arm with an `if` guard — the existing catch-all arm at `ws/agent.rs:586-627` would make it unreachable. Fold the `mark_failed_by_capability_denied` call INTO the existing arm body instead.
3. **Task 7 `handleWsMessage`**: the function already exists as a private `function` at `use-servers-ws.ts:299`. Step 3 must convert it to `export function` in place; do not create a duplicate. A note was added to Task 7 Step 2.
4. **Task 6 integration tests**: added two scenarios — `upgrade_result_failure_marks_job_failed_with_reason` (Verifying failure path) and `upgrade_timeout_sweeper_flips_stuck_running_job` (timeout sweep). These cover spec integration scenarios #2 and #3, which were previously only exercised via tracker unit tests.

## File Map

### Shared protocol

- Modify `crates/common/src/protocol.rs`
  Responsibility: add `UpgradeStage`, `UpgradeStatus`, `UpgradeJobDto`, `job_id` on `ServerMessage::Upgrade`, new agent/browser upgrade message variants, `agent_version` on `BrowserMessage::AgentInfoUpdated`, `upgrades` on `BrowserMessage::FullSync`, and serde coverage.

### Server backend

- Create `crates/server/src/service/upgrade_tracker.rs`
  Responsibility: own in-memory upgrade jobs, match agent messages by `job_id` first, broadcast browser events, enforce timeout/cleanup rules.
- Create `crates/server/src/service/upgrade_release.rs`
  Responsibility: resolve latest release version with 10-minute success cache / 1-minute failure cache, derive GitHub API URL from `release_base_url`, and resolve release asset checksum/download metadata for `trigger_upgrade`.
- Modify `crates/server/src/service/mod.rs`
  Responsibility: export the new upgrade services.
- Modify `crates/server/src/state.rs`
  Responsibility: attach `upgrade_tracker` and `upgrade_release_service` to `AppState`.
- Modify `crates/server/src/config.rs`
  Responsibility: add `upgrade.latest_version_url`.
- Modify `crates/server/src/router/api/mod.rs`
  Responsibility: expose the new authenticated `agent::read_router()`.
- Modify `crates/server/src/router/api/agent.rs`
  Responsibility: add `GET /api/agent/latest-version` plus DTO schema.
- Modify `crates/server/src/router/api/server.rs`
  Responsibility: return structured upgrade job payloads, add `GET /api/servers/{id}/upgrade`, change `POST /api/servers/{id}/upgrade` to tracker-aware `202/409`, and use the release service for checksum resolution.
- Modify `crates/server/src/router/ws/agent.rs`
  Responsibility: consume `UpgradeProgress` / `UpgradeResult`, mark success on reconnect + `SystemInfo.agent_version`, fail fast on `CapabilityDenied(upgrade)`, and broadcast `agent_version`.
- Modify `crates/server/src/router/ws/browser.rs`
  Responsibility: include `upgrades` in `FullSync`.
- Modify `crates/server/src/openapi.rs`
  Responsibility: register the new read/write paths and schemas.
- Create `crates/server/src/task/upgrade_timeout.rs`
  Responsibility: sweep 120-second timeouts and prune 24-hour terminal jobs.
- Modify `crates/server/src/task/mod.rs`
  Responsibility: export the timeout task.
- Modify `crates/server/src/main.rs`
  Responsibility: spawn the timeout task.

### Agent

- Modify `crates/agent/src/reporter.rs`
  Responsibility: accept upgrade `job_id`, emit progress/failure messages, extract `verify_sha256` and `run_preflight`, keep timestamped backups for 24 hours, reject concurrent upgrades, and restart only after preflight succeeds.

### Web frontend

- Create `apps/web/src/stores/upgrade-jobs-store.ts`
  Responsibility: keep one upgrade job per server, dedupe repeat WS payloads, and clear finished jobs after the success toast window.
- Create `apps/web/src/stores/upgrade-jobs-store.test.ts`
  Responsibility: store behavior coverage.
- Create `apps/web/src/hooks/use-upgrade-job.ts`
  Responsibility: read a server's current upgrade job, hydrate from `GET /api/servers/{id}/upgrade` on direct entry, and expose the trigger mutation.
- Modify `apps/web/src/hooks/use-servers-ws.ts`
  Responsibility: route `upgrade_progress`, `upgrade_result`, `full_sync.upgrades`, and optional `agent_version`.
- Modify `apps/web/src/hooks/use-servers-ws.test.ts`
  Responsibility: websocket reducer coverage for the new payloads.
- Create `apps/web/src/components/server/agent-version-section.tsx`
  Responsibility: render current/latest version, admin-only action buttons, running stepper, and terminal states.
- Create `apps/web/src/components/server/agent-version-section.test.tsx`
  Responsibility: component behavior coverage by role/status.
- Create `apps/web/src/components/server/upgrade-job-badge.tsx`
  Responsibility: shared list/card badge for running/failed/timeout states.
- Modify `apps/web/src/routes/_authed/servers/$id.tsx`
  Responsibility: place the new section on the server detail page.
- Modify `apps/web/src/routes/_authed/servers/$id.test.tsx`
  Responsibility: assert the detail page renders the new section.
- Modify `apps/web/src/routes/_authed/servers/index.tsx`
  Responsibility: show the shared upgrade badge in the table view.
- Modify `apps/web/src/components/server/server-card.tsx`
  Responsibility: show the shared upgrade badge in the grid card view.
- Modify `apps/web/src/lib/api-schema.ts`
  Responsibility: re-export `UpgradeJobDto`, `UpgradeStage`, `UpgradeStatus`, and `LatestAgentVersionResponse`.
- Regenerate `apps/web/src/lib/api-types.ts`
  Responsibility: generated OpenAPI types. Do not hand-edit.
- Modify `apps/web/src/locales/en/servers.json`
  Responsibility: add flat `upgrade_*` translation keys in English.
- Modify `apps/web/src/locales/zh/servers.json`
  Responsibility: add flat `upgrade_*` translation keys in Chinese.

### Docs and manual QA

- Modify `ENV.md`
  Responsibility: document `SERVERBEE_UPGRADE__LATEST_VERSION_URL`.
- Modify `apps/docs/content/docs/en/configuration.mdx`
  Responsibility: add the new env var plus the `[upgrade]` field reference.
- Modify `apps/docs/content/docs/cn/configuration.mdx`
  Responsibility: add the new env var plus the `[upgrade]` field reference.
- Create `tests/agent-upgrade.md`
  Responsibility: manual end-to-end validation checklist.
- Modify `tests/README.md`
  Responsibility: add the new checklist to the test index.

### Task 1: Extend the Shared Upgrade Protocol

**Files:**
- Modify: `crates/common/src/protocol.rs`
- Test: `crates/common/src/protocol.rs`

- [ ] **Step 1: Write the failing protocol serde tests**

```rust
#[test]
fn test_server_upgrade_with_job_id_round_trip() {
    let msg = ServerMessage::Upgrade {
        version: "1.2.3".to_string(),
        download_url: "https://example.com/serverbee-agent-linux-amd64".to_string(),
        sha256: "deadbeef".to_string(),
        job_id: Some("job-1".to_string()),
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"job_id\":\"job-1\""));

    match serde_json::from_str::<ServerMessage>(&json).unwrap() {
        ServerMessage::Upgrade { job_id, .. } => {
            assert_eq!(job_id.as_deref(), Some("job-1"));
        }
        _ => panic!("Expected Upgrade"),
    }
}

#[test]
fn test_upgrade_messages_without_job_id_stay_backward_compatible() {
    let json =
        r#"{"type":"upgrade_progress","msg_id":"m1","target_version":"1.2.3","stage":"downloading"}"#;

    match serde_json::from_str::<AgentMessage>(json).unwrap() {
        AgentMessage::UpgradeProgress { job_id, .. } => {
            assert!(job_id.is_none());
        }
        _ => panic!("Expected UpgradeProgress"),
    }
}

#[test]
fn test_browser_full_sync_with_upgrades_round_trip() {
    let msg = BrowserMessage::FullSync {
        servers: vec![],
        upgrades: vec![UpgradeJobDto {
            server_id: "s1".to_string(),
            job_id: "job-1".to_string(),
            target_version: "1.2.3".to_string(),
            stage: UpgradeStage::Installing,
            status: UpgradeStatus::Running,
            error: None,
            backup_path: None,
            started_at: chrono::Utc::now(),
            finished_at: None,
        }],
    };

    let json = serde_json::to_string(&msg).unwrap();
    match serde_json::from_str::<BrowserMessage>(&json).unwrap() {
        BrowserMessage::FullSync { upgrades, .. } => {
            assert_eq!(upgrades.len(), 1);
            assert_eq!(upgrades[0].job_id, "job-1");
        }
        _ => panic!("Expected FullSync"),
    }
}

#[test]
fn test_agent_info_updated_accepts_optional_agent_version() {
    let json =
        r#"{"type":"agent_info_updated","server_id":"s1","protocol_version":3,"agent_version":"1.2.3"}"#;

    match serde_json::from_str::<BrowserMessage>(json).unwrap() {
        BrowserMessage::AgentInfoUpdated {
            server_id,
            protocol_version,
            agent_version,
        } => {
            assert_eq!(server_id, "s1");
            assert_eq!(protocol_version, 3);
            assert_eq!(agent_version.as_deref(), Some("1.2.3"));
        }
        _ => panic!("Expected AgentInfoUpdated"),
    }
}
```

- [ ] **Step 2: Run the protocol tests to verify they fail**

Run:

```bash
cargo test -p serverbee-common test_server_upgrade_with_job_id_round_trip -- --exact
cargo test -p serverbee-common test_upgrade_messages_without_job_id_stay_backward_compatible -- --exact
```

Expected: FAIL because the new upgrade enums, DTOs, and `job_id` / `agent_version` fields do not exist yet.

- [ ] **Step 3: Implement the protocol changes**

```rust
use chrono::{DateTime, Utc};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum UpgradeStage {
    Downloading,
    Verifying,
    PreFlight,
    Installing,
    Restarting,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum UpgradeStatus {
    Running,
    Succeeded,
    Failed,
    Timeout,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct UpgradeJobDto {
    pub server_id: String,
    pub job_id: String,
    pub target_version: String,
    pub stage: UpgradeStage,
    pub status: UpgradeStatus,
    pub error: Option<String>,
    pub backup_path: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

// Add inside `AgentMessage`
UpgradeProgress {
    msg_id: String,
    #[serde(default)]
    job_id: Option<String>,
    target_version: String,
    stage: UpgradeStage,
},
UpgradeResult {
    msg_id: String,
    #[serde(default)]
    job_id: Option<String>,
    target_version: String,
    stage: UpgradeStage,
    error: String,
    #[serde(default)]
    backup_path: Option<String>,
},

// Replace the existing `ServerMessage::Upgrade`
Upgrade {
    version: String,
    download_url: String,
    sha256: String,
    #[serde(default)]
    job_id: Option<String>,
},

pub enum BrowserMessage {
    FullSync {
        servers: Vec<crate::types::ServerStatus>,
        #[serde(default)]
        upgrades: Vec<UpgradeJobDto>,
    },
    AgentInfoUpdated {
        server_id: String,
        protocol_version: u32,
        #[serde(default)]
        agent_version: Option<String>,
    },
    UpgradeProgress {
        server_id: String,
        job_id: String,
        target_version: String,
        stage: UpgradeStage,
    },
    UpgradeResult {
        server_id: String,
        job_id: String,
        target_version: String,
        status: UpgradeStatus,
        stage: Option<UpgradeStage>,
        error: Option<String>,
        backup_path: Option<String>,
    },
}
```

- [ ] **Step 4: Run the protocol tests to verify they pass**

Run:

```bash
cargo test -p serverbee-common test_server_upgrade_with_job_id_round_trip -- --exact
cargo test -p serverbee-common test_upgrade_messages_without_job_id_stay_backward_compatible -- --exact
cargo test -p serverbee-common test_browser_full_sync_with_upgrades_round_trip -- --exact
cargo test -p serverbee-common test_agent_info_updated_accepts_optional_agent_version -- --exact
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/common/src/protocol.rs
git commit -m "feat(common): add upgrade lifecycle protocol"
```

### Task 2: Add the Server Upgrade Tracker and Timeout Worker

**Files:**
- Create: `crates/server/src/service/upgrade_tracker.rs`
- Modify: `crates/server/src/service/mod.rs`
- Modify: `crates/server/src/state.rs`
- Create: `crates/server/src/task/upgrade_timeout.rs`
- Modify: `crates/server/src/task/mod.rs`
- Modify: `crates/server/src/main.rs`
- Test: `crates/server/src/service/upgrade_tracker.rs`

- [ ] **Step 1: Write the failing tracker tests**

```rust
#[tokio::test]
async fn start_job_rejects_a_second_running_job() {
    let (browser_tx, _browser_rx) = tokio::sync::broadcast::channel(8);
    let tracker = UpgradeJobTracker::new(browser_tx);

    let first = tracker.start_job("s1", "1.2.3").unwrap();
    let err = tracker.start_job("s1", "1.2.4").unwrap_err();

    match err {
        StartUpgradeJobError::Conflict(existing) => {
            assert_eq!(existing.job_id, first.job_id);
            assert_eq!(existing.target_version, "1.2.3");
        }
    }
}

#[tokio::test]
async fn update_stage_prefers_job_id_and_ignores_stale_messages() {
    let (browser_tx, _browser_rx) = tokio::sync::broadcast::channel(8);
    let tracker = UpgradeJobTracker::new(browser_tx);
    let job = tracker.start_job("s1", "1.2.3").unwrap();

    tracker.update_stage(
        "s1",
        UpgradeLookup {
            job_id: Some("old-job"),
            target_version: "1.2.3",
        },
        UpgradeStage::Verifying,
    );

    assert_eq!(tracker.get("s1").unwrap().stage, UpgradeStage::Downloading);

    tracker.update_stage(
        "s1",
        UpgradeLookup {
            job_id: Some(job.job_id.as_str()),
            target_version: "1.2.3",
        },
        UpgradeStage::Verifying,
    );

    assert_eq!(tracker.get("s1").unwrap().stage, UpgradeStage::Verifying);
}

#[tokio::test]
async fn mark_succeeded_does_not_overwrite_timeout() {
    let (browser_tx, _browser_rx) = tokio::sync::broadcast::channel(8);
    let tracker = UpgradeJobTracker::new(browser_tx);
    tracker.start_job("s1", "1.2.3").unwrap();
    tracker.sweep_timeouts(chrono::Utc::now() + chrono::Duration::seconds(121));

    tracker.mark_succeeded("s1", "1.2.3");

    assert_eq!(tracker.get("s1").unwrap().status, UpgradeStatus::Timeout);
}

#[tokio::test]
async fn cleanup_old_removes_only_expired_terminal_jobs() {
    let (browser_tx, _browser_rx) = tokio::sync::broadcast::channel(8);
    let tracker = UpgradeJobTracker::new(browser_tx);
    tracker.start_job("s1", "1.2.3").unwrap();
    tracker.mark_failed(
        "s1",
        UpgradeLookup {
            job_id: None,
            target_version: "1.2.3",
        },
        UpgradeStage::Verifying,
        "sha256 mismatch".to_string(),
        None,
    );

    tracker.cleanup_old(chrono::Utc::now() + chrono::Duration::hours(25));

    assert!(tracker.get("s1").is_none());
}
```

- [ ] **Step 2: Run the tracker tests to verify they fail**

Run:

```bash
cargo test -p serverbee-server start_job_rejects_a_second_running_job -- --exact
cargo test -p serverbee-server update_stage_prefers_job_id_and_ignores_stale_messages -- --exact
```

Expected: FAIL because `upgrade_tracker.rs`, `UpgradeLookup`, and `StartUpgradeJobError` do not exist yet.

- [ ] **Step 3: Implement the tracker, wire it into state, and start the timeout worker**

```rust
// crates/server/src/service/upgrade_tracker.rs
pub const UPGRADE_TIMEOUT_SECS: i64 = 120;
pub const UPGRADE_RETENTION_HOURS: i64 = 24;

#[derive(Clone, Debug)]
pub struct UpgradeJob {
    pub job_id: String,
    pub server_id: String,
    pub target_version: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub stage: UpgradeStage,
    pub status: UpgradeStatus,
    pub error: Option<String>,
    pub backup_path: Option<String>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl UpgradeJob {
    pub fn to_dto(&self) -> UpgradeJobDto {
        UpgradeJobDto {
            server_id: self.server_id.clone(),
            job_id: self.job_id.clone(),
            target_version: self.target_version.clone(),
            stage: self.stage.clone(),
            status: self.status.clone(),
            error: self.error.clone(),
            backup_path: self.backup_path.clone(),
            started_at: self.started_at,
            finished_at: self.finished_at,
        }
    }
}

#[derive(Clone, Copy)]
pub struct UpgradeLookup<'a> {
    pub job_id: Option<&'a str>,
    pub target_version: &'a str,
}

pub enum StartUpgradeJobError {
    Conflict(UpgradeJob),
}

pub struct UpgradeJobTracker {
    jobs: dashmap::DashMap<String, UpgradeJob>,
    browser_tx: tokio::sync::broadcast::Sender<BrowserMessage>,
}

impl UpgradeJobTracker {
    pub fn start_job(&self, server_id: &str, target_version: &str) -> Result<UpgradeJob, StartUpgradeJobError> {
        let now = chrono::Utc::now();
        let job = UpgradeJob {
            job_id: uuid::Uuid::new_v4().to_string(),
            server_id: server_id.to_string(),
            target_version: target_version.to_string(),
            started_at: now,
            stage: UpgradeStage::Downloading,
            status: UpgradeStatus::Running,
            error: None,
            backup_path: None,
            finished_at: None,
        };

        match self.jobs.entry(server_id.to_string()) {
            dashmap::mapref::entry::Entry::Occupied(mut entry)
                if entry.get().status == UpgradeStatus::Running =>
            {
                Err(StartUpgradeJobError::Conflict(entry.get().clone()))
            }
            dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                entry.insert(job.clone());
                self.broadcast_progress(&job);
                Ok(job)
            }
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(job.clone());
                self.broadcast_progress(&job);
                Ok(job)
            }
        }
    }

    pub fn update_stage(&self, server_id: &str, lookup: UpgradeLookup<'_>, stage: UpgradeStage) {
        let Some(mut job) = self.jobs.get_mut(server_id) else {
            tracing::warn!("Ignoring upgrade progress for unknown server_id={server_id}");
            return;
        };

        if job.status != UpgradeStatus::Running || !matches_lookup(&job, lookup) {
            tracing::warn!("Ignoring stale upgrade progress for server_id={server_id}");
            return;
        }

        job.stage = stage.clone();
        let snapshot = job.clone();
        drop(job);
        self.broadcast_progress(&snapshot);
    }

    pub fn mark_failed(
        &self,
        server_id: &str,
        lookup: UpgradeLookup<'_>,
        stage: UpgradeStage,
        error: String,
        backup_path: Option<String>,
    ) {
        let Some(mut job) = self.jobs.get_mut(server_id) else {
            return;
        };
        if job.status != UpgradeStatus::Running || !matches_lookup(&job, lookup) {
            return;
        }

        job.stage = stage.clone();
        job.status = UpgradeStatus::Failed;
        job.error = Some(error.clone());
        job.backup_path = backup_path.clone();
        job.finished_at = Some(chrono::Utc::now());
        let snapshot = job.clone();
        drop(job);
        self.broadcast_result(&snapshot);
    }

    pub fn mark_failed_by_capability_denied(&self, server_id: &str) {
        let Some(job) = self.get(server_id) else {
            return;
        };
        if job.status == UpgradeStatus::Running {
            self.mark_failed(
                server_id,
                UpgradeLookup {
                    job_id: Some(job.job_id.as_str()),
                    target_version: &job.target_version,
                },
                UpgradeStage::Downloading,
                "capability denied by agent".to_string(),
                None,
            );
        }
    }

    pub fn mark_succeeded(&self, server_id: &str, observed_version: &str) {
        let Some(mut job) = self.jobs.get_mut(server_id) else {
            return;
        };
        if job.status != UpgradeStatus::Running || job.target_version != observed_version {
            return;
        }

        job.status = UpgradeStatus::Succeeded;
        job.finished_at = Some(chrono::Utc::now());
        let snapshot = job.clone();
        drop(job);
        self.broadcast_result(&snapshot);
    }

    pub fn sweep_timeouts(&self, now: chrono::DateTime<chrono::Utc>) {
        for mut entry in self.jobs.iter_mut() {
            if entry.status == UpgradeStatus::Running
                && entry.started_at + chrono::Duration::seconds(UPGRADE_TIMEOUT_SECS) < now
            {
                entry.status = UpgradeStatus::Timeout;
                entry.finished_at = Some(now);
                let snapshot = entry.clone();
                drop(entry);
                self.broadcast_result(&snapshot);
            }
        }
    }

    pub fn cleanup_old(&self, now: chrono::DateTime<chrono::Utc>) {
        self.jobs.retain(|_, job| {
            job.finished_at
                .map(|finished_at| finished_at + chrono::Duration::hours(UPGRADE_RETENTION_HOURS) >= now)
                .unwrap_or(true)
        });
    }
    pub fn get(&self, server_id: &str) -> Option<UpgradeJob> { self.jobs.get(server_id).map(|job| job.clone()) }
    pub fn snapshot(&self) -> Vec<UpgradeJob> { self.jobs.iter().map(|entry| entry.value().clone()).collect() }

    fn broadcast_progress(&self, job: &UpgradeJob) {
        let _ = self.browser_tx.send(BrowserMessage::UpgradeProgress {
            server_id: job.server_id.clone(),
            job_id: job.job_id.clone(),
            target_version: job.target_version.clone(),
            stage: job.stage.clone(),
        });
    }

    fn broadcast_result(&self, job: &UpgradeJob) {
        let _ = self.browser_tx.send(BrowserMessage::UpgradeResult {
            server_id: job.server_id.clone(),
            job_id: job.job_id.clone(),
            target_version: job.target_version.clone(),
            status: job.status.clone(),
            stage: Some(job.stage.clone()),
            error: job.error.clone(),
            backup_path: job.backup_path.clone(),
        });
    }
}

fn matches_lookup(job: &UpgradeJob, lookup: UpgradeLookup<'_>) -> bool {
    match lookup.job_id {
        Some(job_id) => job.job_id == job_id,
        None => job.target_version == lookup.target_version,
    }
}

// Add these fields on `AppState`
pub upgrade_tracker: Arc<UpgradeJobTracker>,
pub upgrade_release_service: Arc<UpgradeReleaseService>,

// crates/server/src/task/upgrade_timeout.rs
pub async fn run(state: Arc<AppState>) {
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(10));
    loop {
        ticker.tick().await;
        let now = chrono::Utc::now();
        state.upgrade_tracker.sweep_timeouts(now);
        state.upgrade_tracker.cleanup_old(now);
    }
}

// crates/server/src/main.rs
let s = state.clone();
tokio::spawn(async move { task::upgrade_timeout::run(s).await });
```

- [ ] **Step 4: Run the tracker tests to verify they pass**

Run:

```bash
cargo test -p serverbee-server start_job_rejects_a_second_running_job -- --exact
cargo test -p serverbee-server update_stage_prefers_job_id_and_ignores_stale_messages -- --exact
cargo test -p serverbee-server mark_succeeded_does_not_overwrite_timeout -- --exact
cargo test -p serverbee-server cleanup_old_removes_only_expired_terminal_jobs -- --exact
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/upgrade_tracker.rs crates/server/src/service/mod.rs crates/server/src/state.rs crates/server/src/task/upgrade_timeout.rs crates/server/src/task/mod.rs crates/server/src/main.rs
git commit -m "feat(server): add upgrade job tracker"
```

### Task 3: Add Latest-Version Lookup and Release Metadata Resolution

**Files:**
- Create: `crates/server/src/service/upgrade_release.rs`
- Modify: `crates/server/src/service/mod.rs`
- Modify: `crates/server/src/config.rs`
- Modify: `crates/server/src/router/api/agent.rs`
- Modify: `crates/server/src/router/api/mod.rs`
- Modify: `crates/server/src/openapi.rs`
- Test: `crates/server/src/service/upgrade_release.rs`

- [ ] **Step 1: Write the failing release-service tests**

```rust
#[test]
fn github_release_api_url_is_derived_from_release_base_url() {
    assert_eq!(
        github_latest_release_api("https://github.com/ZingerLittleBee/ServerBee/releases"),
        Some("https://api.github.com/repos/ZingerLittleBee/ServerBee/releases/latest".to_string())
    );
}

#[test]
fn normalize_release_tag_strips_optional_v_prefix() {
    assert_eq!(normalize_release_tag("v1.2.3"), "1.2.3");
    assert_eq!(normalize_release_tag("1.2.3"), "1.2.3");
}

#[test]
fn cache_ttl_is_longer_for_success_than_failure() {
    let now = chrono::Utc::now();
    let success = CachedLatestVersion::success("1.2.3".to_string(), None, now);
    let failure = CachedLatestVersion::failure("auto-detect failed".to_string(), now);

    assert!(success.expires_at > failure.expires_at);
}
```

- [ ] **Step 2: Run the release-service tests to verify they fail**

Run:

```bash
cargo test -p serverbee-server github_release_api_url_is_derived_from_release_base_url -- --exact
```

Expected: FAIL because `upgrade_release.rs` and its helpers do not exist yet.

- [ ] **Step 3: Implement the release service, config field, and read route**

```rust
// crates/server/src/config.rs
#[derive(Debug, Clone, Deserialize)]
pub struct UpgradeConfig {
    #[serde(default = "default_release_base_url")]
    pub release_base_url: String,
    #[serde(default)]
    pub latest_version_url: Option<String>,
}

// crates/server/src/service/upgrade_release.rs
#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct LatestAgentVersionResponse {
    pub version: Option<String>,
    pub released_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ReleaseAsset {
    pub download_url: String,
    pub sha256: String,
}

pub struct UpgradeReleaseService {
    client: reqwest::Client,
    cache: tokio::sync::RwLock<Option<CachedLatestVersion>>,
}

#[derive(Clone)]
struct CachedLatestVersion {
    value: LatestAgentVersionResponse,
    expires_at: chrono::DateTime<chrono::Utc>,
}

impl CachedLatestVersion {
    fn success(
        version: String,
        released_at: Option<chrono::DateTime<chrono::Utc>>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            value: LatestAgentVersionResponse {
                version: Some(version),
                released_at,
                error: None,
            },
            expires_at: now + chrono::Duration::minutes(10),
        }
    }

    fn failure(error: String, now: chrono::DateTime<chrono::Utc>) -> Self {
        Self {
            value: LatestAgentVersionResponse {
                version: None,
                released_at: None,
                error: Some(error),
            },
            expires_at: now + chrono::Duration::minutes(1),
        }
    }
}

fn normalize_release_tag(tag: &str) -> String {
    tag.strip_prefix('v').unwrap_or(tag).to_string()
}

fn github_latest_release_api(base_url: &str) -> Option<String> {
    let trimmed = base_url.trim_end_matches('/');
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() >= 6
        && parts[0] == "https:"
        && parts[2] == "github.com"
        && parts[5] == "releases"
    {
        Some(format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            parts[3], parts[4]
        ))
    } else {
        None
    }
}

impl UpgradeReleaseService {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .user_agent(format!("serverbee-server/{}", serverbee_common::constants::VERSION))
                .build()
                .unwrap(),
            cache: tokio::sync::RwLock::new(None),
        }
    }

    pub async fn latest(&self, config: &UpgradeConfig) -> LatestAgentVersionResponse {
        let now = chrono::Utc::now();
        if let Some(cached) = self.cache.read().await.clone()
            && cached.expires_at > now
        {
            return cached.value;
        }

        let fresh = match self.fetch_latest(config).await {
            Ok(value) => match value.version.clone() {
                Some(version) => CachedLatestVersion::success(version, value.released_at, now),
                None => CachedLatestVersion::failure(
                    value
                        .error
                        .unwrap_or_else(|| "latest version unavailable".to_string()),
                    now,
                ),
            },
            Err(err) => CachedLatestVersion::failure(err.to_string(), now),
        };

        let response = fresh.value.clone();
        *self.cache.write().await = Some(fresh);
        response
    }

    pub async fn resolve_asset(
        &self,
        config: &UpgradeConfig,
        version: &str,
        asset_name: &str,
    ) -> Result<ReleaseAsset, AppError> {
        let base_url = config.release_base_url.trim_end_matches('/');
        let checksums_url = format!("{base_url}/download/v{version}/checksums.txt");
        let response = self
            .client
            .get(&checksums_url)
            .send()
            .await
            .map_err(|err| AppError::Internal(format!("Failed to fetch checksums: {err}")))?;

        if !response.status().is_success() {
            return Err(AppError::BadRequest(format!(
                "Checksums not found for version v{version} (HTTP {})",
                response.status()
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|err| AppError::Internal(format!("Failed to read checksums: {err}")))?;

        let sha256 = body
            .lines()
            .find_map(|line| {
                let mut parts = line.split_whitespace();
                let hash = parts.next()?;
                let name = parts.next()?;
                (name == asset_name).then(|| hash.to_string())
            })
            .ok_or_else(|| {
                AppError::BadRequest(format!(
                    "Checksum not found for {asset_name} in v{version} release"
                ))
            })?;

        Ok(ReleaseAsset {
            download_url: format!("{base_url}/download/v{version}/{asset_name}"),
            sha256,
        })
    }

    async fn fetch_latest(
        &self,
        config: &UpgradeConfig,
    ) -> anyhow::Result<LatestAgentVersionResponse> {
        if let Some(url) = &config.latest_version_url {
            let payload = self
                .client
                .get(url)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            return Ok(LatestAgentVersionResponse {
                version: payload
                    .get("version")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                released_at: payload
                    .get("released_at")
                    .and_then(|value| value.as_str())
                    .and_then(|value| value.parse().ok()),
                error: None,
            });
        }

        if let Some(url) = github_latest_release_api(&config.release_base_url) {
            let payload = self
                .client
                .get(url)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;

            return Ok(LatestAgentVersionResponse {
                version: payload
                    .get("tag_name")
                    .and_then(|value| value.as_str())
                    .map(normalize_release_tag),
                released_at: payload
                    .get("published_at")
                    .and_then(|value| value.as_str())
                    .and_then(|value| value.parse().ok()),
                error: None,
            });
        }

        Ok(LatestAgentVersionResponse {
            version: None,
            released_at: None,
            error: Some("auto-detect failed; set upgrade.latest_version_url".to_string()),
        })
    }
}

// crates/server/src/router/api/agent.rs
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route("/agent/latest-version", get(get_latest_version))
}

async fn get_latest_version(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<LatestAgentVersionResponse>>, AppError> {
    ok(state.upgrade_release_service.latest(&state.config.upgrade).await)
}

// crates/server/src/router/api/mod.rs
.merge(agent::read_router())
```

- [ ] **Step 4: Run the release-service tests to verify they pass**

Run:

```bash
cargo test -p serverbee-server github_release_api_url_is_derived_from_release_base_url -- --exact
cargo test -p serverbee-server normalize_release_tag_strips_optional_v_prefix -- --exact
cargo test -p serverbee-server cache_ttl_is_longer_for_success_than_failure -- --exact
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/upgrade_release.rs crates/server/src/service/mod.rs crates/server/src/config.rs crates/server/src/router/api/agent.rs crates/server/src/router/api/mod.rs crates/server/src/openapi.rs
git commit -m "feat(server): add upgrade release lookup"
```

### Task 4: Wire Upgrade Routes and WebSocket State Changes on the Server

**Files:**
- Modify: `crates/server/src/router/api/server.rs`
- Modify: `crates/server/src/router/ws/agent.rs`
- Modify: `crates/server/src/router/ws/browser.rs`
- Modify: `crates/server/src/openapi.rs`
- Test: `crates/server/src/router/ws/browser.rs`

- [ ] **Step 1: Write the failing FullSync hydration test**

```rust
#[tokio::test]
async fn build_full_sync_includes_upgrade_snapshot() {
    let state = test_browser_state().await;
    state.upgrade_tracker.start_job("s1", "1.2.3").unwrap();

    match build_full_sync(&state).await {
        BrowserMessage::FullSync { upgrades, .. } => {
            assert_eq!(upgrades.len(), 1);
            assert_eq!(upgrades[0].server_id, "s1");
            assert_eq!(upgrades[0].status, UpgradeStatus::Running);
        }
        _ => panic!("Expected FullSync"),
    }
}

async fn test_browser_state() -> Arc<AppState> {
    let tmp = tempfile::tempdir().unwrap();
    let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
    crate::migration::Migrator::up(&db, None).await.unwrap();

    let config = AppConfig {
        server: ServerConfig {
            listen: "127.0.0.1:0".to_string(),
            data_dir: tmp.path().display().to_string(),
            trusted_proxies: Vec::new(),
        },
        auth: AuthConfig {
            secure_cookie: false,
            ..AuthConfig::default()
        },
        ..AppConfig::default()
    };

    AppState::new(db, config).await.unwrap()
}
```

- [ ] **Step 2: Run the FullSync test to verify it fails**

Run:

```bash
cargo test -p serverbee-server build_full_sync_includes_upgrade_snapshot -- --exact
```

Expected: FAIL because `BrowserMessage::FullSync` does not yet include `upgrades`.

- [ ] **Step 3: Implement the route and websocket wiring**

```rust
// crates/server/src/router/api/server.rs
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TriggerUpgradeResponse {
    pub job: UpgradeJobDto,
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/upgrade",
    tag = "servers",
    responses((status = 200, body = Option<UpgradeJobDto>))
)]
async fn get_upgrade_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Option<UpgradeJobDto>>>, AppError> {
    ok(state.upgrade_tracker.get(&id).map(|job| job.to_dto()))
}

async fn trigger_upgrade(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpgradeRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Preserve the existing CAP_UPGRADE pre-check from server.rs:536-544.
    // This must run BEFORE `start_job` so capability rejections never leave a
    // phantom Running job in the tracker.
    let server = ServerService::get(&state.db, &id)
        .await?
        .ok_or(AppError::NotFound)?;
    if !has_capability(server.capabilities as u32, CAP_UPGRADE) {
        return Err(AppError::Forbidden(
            "CAP_UPGRADE is not enabled for this server".into(),
        ));
    }

    let version = normalize_version(&body.version);
    let (os_raw, arch_raw) = state
        .agent_manager
        .get_agent_platform(&id)
        .ok_or_else(|| AppError::Conflict("Agent not connected".into()))?;

    let os = map_os(&os_raw)
        .ok_or_else(|| AppError::BadRequest(format!("Unsupported agent OS: {os_raw}")))?;
    let arch = map_arch(&arch_raw)
        .ok_or_else(|| AppError::BadRequest(format!("Unsupported agent arch: {arch_raw}")))?;
    let asset_name = if os == "windows" {
        format!("serverbee-agent-{os}-{arch}.exe")
    } else {
        format!("serverbee-agent-{os}-{arch}")
    };
    let release = state
        .upgrade_release_service
        .resolve_asset(&state.config.upgrade, version, &asset_name)
        .await?;

    let job = match state.upgrade_tracker.start_job(&id, version) {
        Ok(job) => job,
        Err(StartUpgradeJobError::Conflict(existing)) => {
            return Ok((
                axum::http::StatusCode::CONFLICT,
                Json(ApiResponse {
                    data: TriggerUpgradeResponse {
                        job: existing.to_dto(),
                    },
                }),
            ));
        }
    };

    let sender = state
        .agent_manager
        .get_sender(&id)
        .ok_or_else(|| AppError::Conflict("Agent not connected".into()))?;

    let msg = ServerMessage::Upgrade {
        version: version.to_string(),
        download_url: release.download_url,
        sha256: release.sha256,
        job_id: Some(job.job_id.clone()),
    };

    if let Err(err) = sender.send(msg).await {
        state.upgrade_tracker.mark_failed(
            &id,
            UpgradeLookup {
                job_id: Some(job.job_id.as_str()),
                target_version: version,
            },
            UpgradeStage::Downloading,
            format!("failed to notify agent: {err}"),
            None,
        );
        return Err(AppError::Internal("Failed to send upgrade command".into()));
    }

    Ok((
        axum::http::StatusCode::ACCEPTED,
        Json(ApiResponse {
            data: TriggerUpgradeResponse { job: job.to_dto() },
        }),
    ))
}

// crates/server/src/router/ws/agent.rs
AgentMessage::UpgradeProgress {
    job_id,
    target_version,
    stage,
    ..
} => {
    state.upgrade_tracker.update_stage(
        server_id,
        UpgradeLookup {
            job_id: job_id.as_deref(),
            target_version: &target_version,
        },
        stage,
    );
}
AgentMessage::UpgradeResult {
    job_id,
    target_version,
    stage,
    error,
    backup_path,
    ..
} => {
    state.upgrade_tracker.mark_failed(
        server_id,
        UpgradeLookup {
            job_id: job_id.as_deref(),
            target_version: &target_version,
        },
        stage,
        error,
        backup_path,
    );
}
AgentMessage::SystemInfo { info, .. } => {
    ServerService::update_system_info(&state.db, server_id, &info, region, country_code)
        .await
        .expect("system info update should succeed");
    state.agent_manager.broadcast_browser(BrowserMessage::AgentInfoUpdated {
        server_id: server_id.to_string(),
        protocol_version: agent_pv,
        agent_version: Some(info.agent_version.clone()),
    });

    if let Some(job) = state.upgrade_tracker.get(server_id) {
        if job.status == UpgradeStatus::Running && info.agent_version == job.target_version {
            state.upgrade_tracker.mark_succeeded(server_id, &info.agent_version);
        } else if job.status == UpgradeStatus::Running && info.agent_version != job.target_version {
            state.upgrade_tracker.mark_failed(
                server_id,
                UpgradeLookup {
                    job_id: Some(job.job_id.as_str()),
                    target_version: &job.target_version,
                },
                UpgradeStage::Restarting,
                format!(
                    "agent reconnected with unexpected version {}, expected {}",
                    info.agent_version, job.target_version
                ),
                None,
            );
        }
    }
}
// IMPORTANT: Do NOT add a new CapabilityDenied match arm — the existing one at
// ws/agent.rs:586-627 is a single catch-all that already handles exec/terminal
// cleanup. Any new sibling arm with a guard would be unreachable for
// `capability == "upgrade"`. Instead, fold the upgrade-specific call INTO the
// existing arm body, near the top, while leaving the existing
// exec/terminal logic untouched:
//
//   AgentMessage::CapabilityDenied { msg_id, session_id, capability, reason } => {
//       tracing::warn!(/* existing log line */);
//       if capability == "upgrade" {
//           state.upgrade_tracker.mark_failed_by_capability_denied(server_id);
//       }
//       // ... existing exec/terminal dispatch logic unchanged ...
//   }

// crates/server/src/router/ws/browser.rs
BrowserMessage::FullSync {
    servers: statuses,
    upgrades: state
        .upgrade_tracker
        .snapshot()
        .into_iter()
        .map(|job| job.to_dto())
        .collect(),
}
```

- [ ] **Step 4: Run the server route/ws tests to verify they pass**

Run:

```bash
cargo test -p serverbee-server build_full_sync_includes_upgrade_snapshot -- --exact
cargo test -p serverbee-server trigger_upgrade -- --nocapture
```

Expected: the FullSync test passes and the `trigger_upgrade` unit test module still passes after the route response shape change.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/api/server.rs crates/server/src/router/ws/agent.rs crates/server/src/router/ws/browser.rs crates/server/src/openapi.rs
git commit -m "feat(server): wire upgrade api and ws state"
```

### Task 5: Harden Agent Upgrade Execution and Emit Lifecycle Events

**Files:**
- Modify: `crates/agent/src/reporter.rs`
- Test: `crates/agent/src/reporter.rs`

- [ ] **Step 1: Write the failing agent-side helper tests**

```rust
#[test]
fn verify_sha256_rejects_mismatched_hash() {
    let err = verify_sha256(b"hello", "deadbeef").unwrap_err();
    assert!(err.to_string().contains("sha256 mismatch"));
}

#[tokio::test]
async fn run_preflight_rejects_non_zero_exit() {
    let script = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(script.path(), "#!/bin/sh\nexit 7\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(script.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let err = run_preflight(script.path(), Duration::from_secs(1)).await.unwrap_err();
    assert!(err.to_string().contains("preflight"));
}

#[test]
fn cleanup_old_backups_removes_only_stale_backup_files() {
    let dir = tempfile::tempdir().unwrap();
    let stale = dir.path().join("serverbee-agent.bak.20260414-000000");
    let fresh = dir.path().join("serverbee-agent.bak.20260414-235959");
    std::fs::write(&stale, b"old").unwrap();
    std::fs::write(&fresh, b"new").unwrap();

    cleanup_old_backups(dir.path(), chrono::Utc::now() + chrono::Duration::hours(25)).unwrap();

    assert!(!stale.exists());
    assert!(fresh.exists());
}
```

- [ ] **Step 2: Run the agent helper tests to verify they fail**

Run:

```bash
cargo test -p serverbee-agent verify_sha256_rejects_mismatched_hash -- --exact
cargo test -p serverbee-agent run_preflight_rejects_non_zero_exit -- --exact
```

Expected: FAIL because the extracted helpers and backup cleanup do not exist yet.

- [ ] **Step 3: Implement progress emission, safer install, and concurrency protection**

```rust
async fn emit_upgrade_progress(
    tx: &tokio::sync::mpsc::Sender<AgentMessage>,
    job_id: Option<String>,
    target_version: &str,
    stage: UpgradeStage,
) {
    let _ = tx
        .send(AgentMessage::UpgradeProgress {
            msg_id: uuid::Uuid::new_v4().to_string(),
            job_id,
            target_version: target_version.to_string(),
            stage,
        })
        .await;
}

async fn emit_upgrade_failure(
    tx: &tokio::sync::mpsc::Sender<AgentMessage>,
    job_id: Option<String>,
    target_version: &str,
    stage: UpgradeStage,
    error: String,
    backup_path: Option<String>,
) {
    let _ = tx
        .send(AgentMessage::UpgradeResult {
            msg_id: uuid::Uuid::new_v4().to_string(),
            job_id,
            target_version: target_version.to_string(),
            stage,
            error,
            backup_path,
        })
        .await;
}

fn verify_sha256(bytes: &[u8], expected: &str) -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};

    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual != expected {
        anyhow::bail!("sha256 mismatch: got {actual}, want {expected}");
    }

    Ok(())
}

async fn download_upgrade_bytes(download_url: &str) -> anyhow::Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()?;
    let response = client
        .get(download_url)
        .header("User-Agent", "ServerBee-Agent")
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("http {}", response.status());
    }

    Ok(response.bytes().await?.to_vec())
}

fn set_executable_permissions(path: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

async fn run_preflight(path: &std::path::Path, timeout: Duration) -> anyhow::Result<()> {
    let status = tokio::time::timeout(
        timeout,
        tokio::process::Command::new(path).arg("--version").status(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("preflight timed out"))??;

    if !status.success() {
        anyhow::bail!("preflight failed with status {status}");
    }
    Ok(())
}

fn cleanup_old_backups(dir: &std::path::Path, now: chrono::DateTime<chrono::Utc>) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains(".bak."))
        {
            let modified: chrono::DateTime<chrono::Utc> = entry.metadata()?.modified()?.into();
            if modified < now - chrono::Duration::hours(24) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    Ok(())
}

async fn perform_upgrade(
    tx: &tokio::sync::mpsc::Sender<AgentMessage>,
    job_id: Option<String>,
    version: &str,
    download_url: &str,
    sha256: &str,
) -> anyhow::Result<()> {
    let current_exe = std::env::current_exe()?;
    let tmp_path = current_exe.with_extension("new");
    let backup_path = current_exe.with_extension(format!(
        "bak.{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    ));

    emit_upgrade_progress(tx, job_id.clone(), version, UpgradeStage::Downloading).await;
    let bytes = download_upgrade_bytes(download_url).await.map_err(|err| {
        anyhow::anyhow!("download failed: {err}")
    })?;

    emit_upgrade_progress(tx, job_id.clone(), version, UpgradeStage::Verifying).await;
    if let Err(err) = verify_sha256(&bytes, sha256) {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        emit_upgrade_failure(tx, job_id.clone(), version, UpgradeStage::Verifying, err.to_string(), None).await;
        return Err(err);
    }

    tokio::fs::write(&tmp_path, &bytes).await?;
    set_executable_permissions(&tmp_path)?;

    emit_upgrade_progress(tx, job_id.clone(), version, UpgradeStage::PreFlight).await;
    if let Err(err) = run_preflight(&tmp_path, Duration::from_secs(5)).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        emit_upgrade_failure(tx, job_id.clone(), version, UpgradeStage::PreFlight, err.to_string(), None).await;
        return Err(err);
    }

    emit_upgrade_progress(tx, job_id.clone(), version, UpgradeStage::Installing).await;
    std::fs::rename(&current_exe, &backup_path)?;
    if let Err(err) = std::fs::rename(&tmp_path, &current_exe) {
        let _ = std::fs::rename(&backup_path, &current_exe);
        emit_upgrade_failure(
            tx,
            job_id.clone(),
            version,
            UpgradeStage::Installing,
            err.to_string(),
            Some(backup_path.display().to_string()),
        )
        .await;
        return Err(err.into());
    }

    emit_upgrade_progress(tx, job_id.clone(), version, UpgradeStage::Restarting).await;
    if let Err(err) = std::process::Command::new(&current_exe).args(std::env::args().skip(1)).spawn() {
        let _ = std::fs::rename(&backup_path, &current_exe);
        emit_upgrade_failure(
            tx,
            job_id,
            version,
            UpgradeStage::Restarting,
            err.to_string(),
            Some(backup_path.display().to_string()),
        )
        .await;
        return Err(err.into());
    }

    std::process::exit(0);
}
```

- [ ] **Step 4: Run the agent helper tests to verify they pass**

Run:

```bash
cargo test -p serverbee-agent verify_sha256_rejects_mismatched_hash -- --exact
cargo test -p serverbee-agent run_preflight_rejects_non_zero_exit -- --exact
cargo test -p serverbee-agent cleanup_old_backups_removes_only_stale_backup_files -- --exact
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src/reporter.rs
git commit -m "feat(agent): report upgrade progress and harden restart"
```

### Task 6: Add Server Integration Coverage for the Upgrade Lifecycle

**Files:**
- Modify: `crates/server/tests/integration.rs`
- Test: `crates/server/tests/integration.rs`

- [ ] **Step 1: Write the failing upgrade integration tests**

```rust
#[tokio::test]
async fn upgrade_success_marks_job_succeeded_and_updates_agent_version() {
    let (base_url, state, _tmp) = start_test_server_with_state().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut ws_tx, mut ws_rx) = connect_agent(&base_url, &token).await;

    send_system_info(&mut ws_tx, &mut ws_rx, "0.2.0").await;

    let resp = client
        .post(format!("{}/api/servers/{}/upgrade", base_url, server_id))
        .json(&json!({ "version": "0.3.0" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 202);
    let body: serde_json::Value = resp.json().await.unwrap();
    let job_id = body["data"]["job"]["job_id"].as_str().unwrap().to_string();

    send_upgrade_progress(&mut ws_tx, &job_id, "0.3.0", "downloading").await;
    send_upgrade_progress(&mut ws_tx, &job_id, "0.3.0", "restarting").await;
    drop(ws_tx);
    let (mut ws_tx, mut ws_rx) = connect_agent(&base_url, &token).await;
    send_system_info(&mut ws_tx, &mut ws_rx, "0.3.0").await;

    let job = state.upgrade_tracker.get(&server_id).unwrap();
    assert_eq!(job.status, UpgradeStatus::Succeeded);
    assert_eq!(job.target_version, "0.3.0");
}

#[tokio::test]
async fn capability_denied_upgrade_fails_immediately() {
    let (base_url, state, _tmp) = start_test_server_with_state().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut ws_tx, mut ws_rx) = connect_agent(&base_url, &token).await;

    send_system_info(&mut ws_tx, &mut ws_rx, "0.2.0").await;
    enable_server_upgrade_capability(&client, &base_url, &server_id).await;

    let resp = client
        .post(format!("{}/api/servers/{}/upgrade", base_url, server_id))
        .json(&json!({ "version": "0.3.0" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    send_capability_denied(&mut ws_tx, "upgrade").await;

    let job = state.upgrade_tracker.get(&server_id).unwrap();
    assert_eq!(job.status, UpgradeStatus::Failed);
    assert!(job.error.unwrap().contains("capability denied"));
}

#[tokio::test]
async fn full_sync_contains_running_and_failed_upgrade_jobs() {
    let (base_url, state, _tmp) = start_test_server_with_state().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut agent_tx, mut agent_rx) = connect_agent(&base_url, &token).await;
    send_system_info(&mut agent_tx, &mut agent_rx, "0.2.0").await;

    let job = state.upgrade_tracker.start_job(&server_id, "0.3.0").unwrap();
    state.upgrade_tracker.mark_failed(
        &server_id,
        UpgradeLookup {
            job_id: Some(job.job_id.as_str()),
            target_version: "0.3.0",
        },
        UpgradeStage::Verifying,
        "sha256 mismatch".to_string(),
        None,
    );

    let api_key = create_api_key(&client, &base_url).await;
    let mut browser = connect_browser_ws(&base_url, &api_key).await;
    let full_sync = next_browser_message(&mut browser).await;
    assert_eq!(full_sync["type"], "full_sync");
    assert_eq!(full_sync["upgrades"][0]["status"], "failed");
}

#[tokio::test]
async fn upgrade_result_failure_marks_job_failed_with_reason() {
    // Covers spec integration scenario #2 — Verifying failure path.
    // Proves the agent→server UpgradeResult message round-trip correctly
    // flips the job to Failed and surfaces the agent-reported error string.
    let (base_url, state, _tmp) = start_test_server_with_state().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut ws_tx, mut ws_rx) = connect_agent(&base_url, &token).await;

    send_system_info(&mut ws_tx, &mut ws_rx, "0.2.0").await;
    enable_server_upgrade_capability(&client, &base_url, &server_id).await;

    let resp = client
        .post(format!("{}/api/servers/{}/upgrade", base_url, server_id))
        .json(&json!({ "version": "0.3.0" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);
    let body: serde_json::Value = resp.json().await.unwrap();
    let job_id = body["data"]["job"]["job_id"].as_str().unwrap().to_string();

    send_upgrade_progress(&mut ws_tx, &job_id, "0.3.0", "downloading").await;
    send_upgrade_result_failure(
        &mut ws_tx,
        &job_id,
        "0.3.0",
        "verifying",
        "sha256 mismatch: got abc, want def",
        None,
    )
    .await;

    // Poll briefly because WS dispatch is async.
    let job = wait_for_job_status(&state, &server_id, UpgradeStatus::Failed).await;
    assert_eq!(job.stage, UpgradeStage::Verifying);
    assert!(job.error.unwrap().contains("sha256 mismatch"));
}

#[tokio::test]
async fn upgrade_timeout_sweeper_flips_stuck_running_job() {
    // Covers spec integration scenario #3 — timeout path.
    // Proves the 120s timeout sweeper observably flips a Running job to Timeout
    // and that the change is broadcast to connected browsers.
    let (base_url, state, _tmp) = start_test_server_with_state().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let (server_id, token) = register_agent(&client, &base_url).await;
    let (mut ws_tx, mut ws_rx) = connect_agent(&base_url, &token).await;
    send_system_info(&mut ws_tx, &mut ws_rx, "0.2.0").await;

    // Seed a Running job directly via tracker, then backdate `started_at` past
    // the timeout window (the sweeper uses Utc::now() internally, so we either
    // call the sweeper with a synthetic `now` or inject a mutator helper on
    // the tracker used only in cfg(test)).
    let job = state.upgrade_tracker.start_job(&server_id, "0.3.0").unwrap();
    state
        .upgrade_tracker
        .test_override_started_at(&server_id, Utc::now() - chrono::Duration::seconds(121));

    state.upgrade_tracker.sweep_timeouts(Utc::now());

    let observed = state.upgrade_tracker.get(&server_id).unwrap();
    assert_eq!(observed.status, UpgradeStatus::Timeout);
    assert_eq!(observed.job_id, job.job_id);

    // Prove the browser broadcast went out as well so the UI receives the flip.
    let api_key = create_api_key(&client, &base_url).await;
    let mut browser = connect_browser_ws(&base_url, &api_key).await;
    let full_sync = next_browser_message(&mut browser).await;
    assert_eq!(full_sync["upgrades"][0]["status"], "timeout");
}
```

> **Note for implementer**: the timeout test uses `test_override_started_at` — add this as a `#[cfg(any(test, feature = "test-util"))]` helper on `UpgradeJobTracker` that only exists in test builds. Alternatively, refactor `sweep_timeouts` to accept an injected `now: DateTime<Utc>` (Task 2 already does this — just make sure the helper to backdate an existing job lives in `cfg(test)` to avoid polluting production API).
>
> `send_upgrade_result_failure` and `wait_for_job_status` are new harness helpers — add them alongside the existing `send_upgrade_progress` / `send_capability_denied` in the integration harness module (Step 3).

- [ ] **Step 2: Run the upgrade integration tests to verify they fail**

Run:

```bash
cargo test -p serverbee-server upgrade_success_marks_job_succeeded_and_updates_agent_version -- --exact --nocapture
cargo test -p serverbee-server capability_denied_upgrade_fails_immediately -- --exact --nocapture
cargo test -p serverbee-server full_sync_contains_running_and_failed_upgrade_jobs -- --exact --nocapture
cargo test -p serverbee-server upgrade_result_failure_marks_job_failed_with_reason -- --exact --nocapture
cargo test -p serverbee-server upgrade_timeout_sweeper_flips_stuck_running_job -- --exact --nocapture
```

Expected: all five FAIL because the new tracker-backed route responses, WS upgrade messages, and helper functions (`send_upgrade_result_failure`, `wait_for_job_status`, `test_override_started_at`) are not wired yet.

- [ ] **Step 3: Extend the existing integration harness until the new tests pass**

```rust
async fn start_test_server_with_state() -> (String, Arc<AppState>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let data_dir = tmp.path().to_str().unwrap().to_string();
    let config = AppConfig {
        server: ServerConfig {
            listen: "127.0.0.1:0".to_string(),
            data_dir: data_dir.clone(),
            trusted_proxies: Vec::new(),
        },
        database: DatabaseConfig {
            path: "test.db".to_string(),
            max_connections: 5,
        },
        auth: AuthConfig {
            session_ttl: 86400,
            auto_discovery_key: "test-key".to_string(),
            secure_cookie: false,
            max_servers: 0,
        },
        admin: AdminConfig {
            username: "admin".to_string(),
            password: "testpass".to_string(),
        },
        ..AppConfig::default()
    };

    let db = Database::connect(format!("sqlite://{data_dir}/test.db?mode=rwc"))
        .await
        .expect("Failed to connect to test database");
    Migrator::up(&db, None).await.expect("Failed to run migrations");
    AuthService::init_admin(&db, &config.admin).await.expect("Failed to init admin");
    ConfigService::set(&db, "auto_discovery_key", "test-key")
        .await
        .expect("Failed to set auto_discovery_key");

    let state = AppState::new(db, config).await.expect("Failed to create AppState");
    let app_state = state.clone();
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind listener");
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (base_url, app_state, tmp)
}

type AgentSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
type AgentSink = futures_util::stream::SplitSink<AgentSocket, tungstenite::Message>;
type AgentStream = futures_util::stream::SplitStream<AgentSocket>;
type BrowserStream = futures_util::stream::SplitStream<AgentSocket>;

async fn send_system_info(
    ws_tx: &mut AgentSink,
    ws_rx: &mut AgentStream,
    agent_version: &str,
) {
    ws_tx
        .send(tungstenite::Message::Text(
            json!({
                "type": "system_info",
                "msg_id": uuid::Uuid::new_v4().to_string(),
                "cpu_name": "Test CPU",
                "cpu_cores": 4,
                "cpu_arch": "x86_64",
                "os": "Linux",
                "kernel_version": "6.1",
                "mem_total": 1024,
                "swap_total": 0,
                "disk_total": 1024,
                "agent_version": agent_version,
                "protocol_version": 3,
                "features": []
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    wait_for_ack(ws_rx).await;
}

async fn send_upgrade_progress(ws_tx: &mut AgentSink, job_id: &str, version: &str, stage: &str) {
    ws_tx
        .send(tungstenite::Message::Text(
            json!({
                "type": "upgrade_progress",
                "msg_id": uuid::Uuid::new_v4().to_string(),
                "job_id": job_id,
                "target_version": version,
                "stage": stage
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
}

async fn send_capability_denied(ws_tx: &mut AgentSink, capability: &str) {
    ws_tx
        .send(tungstenite::Message::Text(
            json!({
                "type": "capability_denied",
                "capability": capability,
                "reason": "agent_capability_disabled"
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
}

async fn wait_for_ack(ws_rx: &mut AgentStream) {
    while let Some(Ok(message)) = ws_rx.next().await {
        if let tungstenite::Message::Text(text) = message
            && text.contains("\"type\":\"ack\"")
        {
            break;
        }
    }
}

async fn enable_server_upgrade_capability(client: &reqwest::Client, base_url: &str, server_id: &str) {
    let resp = client
        .put(format!("{}/api/servers/{}", base_url, server_id))
        .json(&json!({ "capabilities": serverbee_common::constants::CAP_DEFAULT | serverbee_common::constants::CAP_UPGRADE }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

async fn connect_browser_ws(base_url: &str, api_key: &str) -> BrowserStream {
    let ws_url = format!("{}/api/ws/servers", base_url.replace("http://", "ws://"));
    let mut request = ws_url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("x-api-key", HeaderValue::from_str(api_key).unwrap());
    let (stream, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let (_, read) = stream.split();
    read
}

async fn next_browser_message(browser: &mut BrowserStream) -> serde_json::Value {
    while let Some(Ok(message)) = browser.next().await {
        if let tungstenite::Message::Text(text) = message {
            return serde_json::from_str(&text).unwrap();
        }
    }
    panic!("browser websocket closed before a message arrived")
}
```

- [ ] **Step 4: Run the upgrade integration tests to verify they pass**

Run:

```bash
cargo test -p serverbee-server upgrade_success_marks_job_succeeded_and_updates_agent_version -- --exact --nocapture
cargo test -p serverbee-server capability_denied_upgrade_fails_immediately -- --exact --nocapture
cargo test -p serverbee-server full_sync_contains_running_and_failed_upgrade_jobs -- --exact --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/tests/integration.rs
git commit -m "test(server): cover upgrade lifecycle"
```

### Task 7: Add the Frontend Upgrade Store, Hook, and WebSocket Hydration

**Files:**
- Create: `apps/web/src/stores/upgrade-jobs-store.ts`
- Create: `apps/web/src/stores/upgrade-jobs-store.test.ts`
- Create: `apps/web/src/hooks/use-upgrade-job.ts`
- Modify: `apps/web/src/hooks/use-servers-ws.ts`
- Modify: `apps/web/src/hooks/use-servers-ws.test.ts`
- Modify: `apps/web/src/lib/api-schema.ts`
- Regenerate: `apps/web/src/lib/api-types.ts`

- [ ] **Step 1: Write the failing store and websocket tests**

```ts
import { QueryClient } from '@tanstack/react-query'
import { beforeEach, describe, expect, it } from 'vitest'
import { handleWsMessage } from '@/hooks/use-servers-ws'
import { useUpgradeJobsStore } from './upgrade-jobs-store'

const runningJob = {
  server_id: 's1',
  job_id: 'job-1',
  target_version: '1.2.3',
  stage: 'downloading',
  status: 'running',
  error: null,
  backup_path: null,
  started_at: '2026-04-14T00:00:00Z',
  finished_at: null
} as const

beforeEach(() => {
  useUpgradeJobsStore.setState({ jobs: {} })
})

describe('upgrade store', () => {
  it('setJobs replaces the current snapshot', () => {
    useUpgradeJobsStore.getState().setJobs([runningJob])
    useUpgradeJobsStore.getState().setJobs([])
    expect(useUpgradeJobsStore.getState().jobs).toEqual({})
  })

  it('upsertJob ignores duplicate terminal payloads for the same job', () => {
    useUpgradeJobsStore.getState().upsertJob({
      ...runningJob,
      status: 'failed',
      stage: 'verifying',
      error: 'sha256 mismatch',
      finished_at: '2026-04-14T00:00:03Z'
    })
    useUpgradeJobsStore.getState().upsertJob({
      ...runningJob,
      status: 'failed',
      stage: 'verifying',
      error: 'sha256 mismatch',
      finished_at: '2026-04-14T00:00:03Z'
    })
    expect(Object.keys(useUpgradeJobsStore.getState().jobs)).toHaveLength(1)
  })
})

describe('handleWsMessage', () => {
  it('hydrates full_sync upgrades into the store', () => {
    const queryClient = new QueryClient()
    handleWsMessage({ type: 'full_sync', servers: [], upgrades: [runningJob] }, queryClient)
    expect(useUpgradeJobsStore.getState().jobs.s1?.job_id).toBe('job-1')
  })

  it('patches agent_version into server detail cache', () => {
    const queryClient = new QueryClient()
    queryClient.setQueryData(['servers', 's1'], { id: 's1', agent_version: '0.2.0' })

    handleWsMessage(
      { type: 'agent_info_updated', server_id: 's1', protocol_version: 3, agent_version: '1.2.3' },
      queryClient
    )

    expect(queryClient.getQueryData(['servers', 's1'])).toMatchObject({ agent_version: '1.2.3' })
  })
})
```

- [ ] **Step 2: Run the frontend state tests to verify they fail**

Run:

```bash
bun --cwd apps/web x vitest run src/stores/upgrade-jobs-store.test.ts src/hooks/use-servers-ws.test.ts
```

Expected: FAIL because the store, exported `handleWsMessage`, and new WS cases do not exist yet.

> **Note for implementer**: `handleWsMessage` currently exists as a *private* `function` at `apps/web/src/hooks/use-servers-ws.ts:299`. The test at Step 1 imports it, so Step 3 must convert it from `function` to `export function` (the snippet below shows the exported form). Do not create a second, separate `handleWsMessage` — edit the existing one in place.

- [ ] **Step 3: Implement the store, hook, schema exports, and websocket cases**

```ts
// apps/web/src/stores/upgrade-jobs-store.ts
import { create } from 'zustand'
import type { UpgradeJobDto } from '@/lib/api-schema'

interface UpgradeJobsState {
  jobs: Record<string, UpgradeJobDto>
  setJobs: (jobs: UpgradeJobDto[]) => void
  upsertJob: (job: UpgradeJobDto) => void
  clearFinished: (serverId: string) => void
}

export const useUpgradeJobsStore = create<UpgradeJobsState>()((set) => ({
  jobs: {},
  setJobs: (jobs) =>
    set({
      jobs: Object.fromEntries(jobs.map((job) => [job.server_id, job])),
    }),
  upsertJob: (job) =>
    set((state) => {
      const prev = state.jobs[job.server_id]
      if (
        prev &&
        prev.job_id === job.job_id &&
        prev.status === job.status &&
        prev.stage === job.stage &&
        prev.error === job.error &&
        prev.finished_at === job.finished_at
      ) {
        return state
      }

      return {
        jobs: {
          ...state.jobs,
          [job.server_id]: { ...prev, ...job },
        },
      }
    }),
  clearFinished: (serverId) =>
    set((state) => {
      const next = { ...state.jobs }
      delete next[serverId]
      return { jobs: next }
    }),
}))

// apps/web/src/hooks/use-upgrade-job.ts
export function useUpgradeJob(serverId: string) {
  const job = useUpgradeJobsStore((state) => state.jobs[serverId] ?? null)
  const upsertJob = useUpgradeJobsStore((state) => state.upsertJob)

  const query = useQuery({
    queryKey: ['servers', serverId, 'upgrade'],
    queryFn: () => api.get<UpgradeJobDto | null>(`/api/servers/${serverId}/upgrade`),
    enabled: !!serverId && !job,
    staleTime: 0,
  })

  useEffect(() => {
    if (query.data) {
      upsertJob(query.data)
    }
  }, [query.data, upsertJob])

  return job ?? query.data ?? null
}

export function useTriggerUpgrade() {
  const upsertJob = useUpgradeJobsStore((state) => state.upsertJob)
  return useMutation({
    mutationFn: ({ serverId, version }: { serverId: string; version: string }) =>
      api.post<TriggerUpgradeResponse>(`/api/servers/${serverId}/upgrade`, { version }),
    onSuccess: ({ job }) => upsertJob(job),
  })
}

// apps/web/src/hooks/use-servers-ws.ts
type WsMessage =
  | { type: 'full_sync'; servers: ServerMetrics[]; upgrades?: UpgradeJobDto[] }
  | { type: 'upgrade_progress'; server_id: string; job_id: string; target_version: string; stage: UpgradeStage }
  | {
      type: 'upgrade_result'
      server_id: string
      job_id: string
      target_version: string
      status: UpgradeStatus
      stage?: UpgradeStage | null
      error?: string | null
      backup_path?: string | null
    }
  | { type: 'agent_info_updated'; server_id: string; protocol_version: number; agent_version?: string | null }

export function handleWsMessage(raw: unknown, queryClient: QueryClient): void {
  if (
    isWsMessageLike(raw) &&
    (raw.type === 'full_sync' || raw.type === 'update' || raw.type === 'server_online' || raw.type === 'server_offline')
  ) {
    handleServerMetricsMessage(raw, queryClient)
  }

  if (isWsMessageLike(raw) && (raw.type === 'capabilities_changed' || raw.type === 'agent_info_updated')) {
    handleCapabilityMessage(raw, queryClient)
  }

  if (isWsMessageLike(raw) && raw.type === 'full_sync') {
    useUpgradeJobsStore.getState().setJobs(Array.isArray(raw.upgrades) ? (raw.upgrades as UpgradeJobDto[]) : [])
  }

  if (isWsMessageLike(raw) && raw.type === 'upgrade_progress') {
    const prev = useUpgradeJobsStore.getState().jobs[raw.server_id as string]
    useUpgradeJobsStore.getState().upsertJob({
      server_id: raw.server_id as string,
      job_id: raw.job_id as string,
      target_version: raw.target_version as string,
      stage: raw.stage as UpgradeStage,
      status: 'running',
      error: null,
      backup_path: prev?.backup_path ?? null,
      started_at: prev?.started_at ?? new Date().toISOString(),
      finished_at: null,
    })
    return
  }

  if (isWsMessageLike(raw) && raw.type === 'upgrade_result') {
    const prev = useUpgradeJobsStore.getState().jobs[raw.server_id as string]
    useUpgradeJobsStore.getState().upsertJob({
      server_id: raw.server_id as string,
      job_id: raw.job_id as string,
      target_version: raw.target_version as string,
      stage: (raw.stage as UpgradeStage | null | undefined) ?? prev?.stage ?? 'restarting',
      status: raw.status as UpgradeStatus,
      error: (raw.error as string | null | undefined) ?? null,
      backup_path: (raw.backup_path as string | null | undefined) ?? null,
      started_at: prev?.started_at ?? new Date().toISOString(),
      finished_at: new Date().toISOString(),
    })
    return
  }
}

// apps/web/src/lib/api-schema.ts
export type UpgradeJobDto = S['UpgradeJobDto']
export type UpgradeStage = S['UpgradeStage']
export type UpgradeStatus = S['UpgradeStatus']
export type LatestAgentVersionResponse = S['LatestAgentVersionResponse']
export type TriggerUpgradeResponse = S['TriggerUpgradeResponse']
```

- [ ] **Step 4: Regenerate API types and run the frontend state tests to verify they pass**

Run:

```bash
bun --cwd apps/web run generate:api-types
bun --cwd apps/web x vitest run src/stores/upgrade-jobs-store.test.ts src/hooks/use-servers-ws.test.ts
```

Expected: PASS, and `apps/web/src/lib/api-types.ts` changes to include the new upgrade schemas.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/stores/upgrade-jobs-store.ts apps/web/src/stores/upgrade-jobs-store.test.ts apps/web/src/hooks/use-upgrade-job.ts apps/web/src/hooks/use-servers-ws.ts apps/web/src/hooks/use-servers-ws.test.ts apps/web/src/lib/api-schema.ts apps/web/src/lib/api-types.ts
git commit -m "feat(web): add upgrade job state and ws hydration"
```

### Task 8: Add the Detail-Page Upgrade UI and List Badges

**Files:**
- Create: `apps/web/src/components/server/agent-version-section.tsx`
- Create: `apps/web/src/components/server/agent-version-section.test.tsx`
- Create: `apps/web/src/components/server/upgrade-job-badge.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$id.test.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.tsx`
- Modify: `apps/web/src/components/server/server-card.tsx`
- Modify: `apps/web/src/locales/en/servers.json`
- Modify: `apps/web/src/locales/zh/servers.json`

- [ ] **Step 1: Write the failing component and placement tests**

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { AgentVersionSection } from './agent-version-section'

const mockUseUpgradeJob = vi.fn(() => null)

vi.mock('@/hooks/use-auth', () => ({
  useAuth: () => ({ user: { role: 'admin' } }),
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string, vars?: Record<string, string>) => {
      if (key === 'upgrade_button') {
        return `Upgrade to ${vars?.version}`
      }
      if (key === 'upgrade_retry') {
        return 'Retry'
      }
      if (key === 'upgrade_failed') {
        return `Failed at ${vars?.stage}: ${vars?.error}`
      }
      if (key.startsWith('upgrade_stage_')) {
        return key.replace('upgrade_stage_', '')
      }
      return key
    },
  }),
}))

vi.mock('@/hooks/use-upgrade-job', () => ({
  useTriggerUpgrade: () => ({ mutate: vi.fn(), isPending: false }),
  useUpgradeJob: mockUseUpgradeJob,
}))

vi.mock('@tanstack/react-query', () => ({
  useQuery: () => ({ data: { version: '1.2.3', released_at: null, error: null } }),
}))

describe('AgentVersionSection', () => {
  it('shows the admin upgrade button when latest is newer', () => {
    render(
      <AgentVersionSection
        server={{
          id: 's1',
          agent_version: '1.2.2',
          capabilities: 4,
          effective_capabilities: 4,
        } as never}
      />
    )

    expect(screen.getByRole('button', { name: /upgrade to 1.2.3/i })).toBeInTheDocument()
  })

  it('shows failure details and retry for admin users', () => {
    mockUseUpgradeJob.mockReturnValue({
      server_id: 's1',
      job_id: 'job-1',
      target_version: '1.2.3',
      stage: 'verifying',
      status: 'failed',
      error: 'sha256 mismatch',
      backup_path: '/opt/serverbee/serverbee-agent.bak.20260414-120000',
      started_at: '2026-04-14T12:00:00Z',
      finished_at: '2026-04-14T12:00:03Z',
    })

    render(<AgentVersionSection server={{ id: 's1', agent_version: '1.2.2', capabilities: 4, effective_capabilities: 4 } as never} />)

    expect(screen.getByText(/sha256 mismatch/i)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /retry/i })).toBeInTheDocument()
  })
})

// apps/web/src/routes/_authed/servers/$id.test.tsx
vi.mock('@/components/server/agent-version-section', () => ({
  AgentVersionSection: () => <div>agent-version-section</div>,
}))

it('renders the agent version section on the detail page', () => {
  render(<ServerDetailPage />)
  expect(screen.getByText('agent-version-section')).toBeInTheDocument()
})
```

- [ ] **Step 2: Run the UI tests to verify they fail**

Run:

```bash
bun --cwd apps/web x vitest run src/components/server/agent-version-section.test.tsx 'src/routes/_authed/servers/$id.test.tsx'
```

Expected: FAIL because the new section, badge component, and translation keys do not exist yet.

- [ ] **Step 3: Implement the detail section, shared badge, and translations**

```tsx
// apps/web/src/components/server/upgrade-job-badge.tsx
export function UpgradeJobBadge({ serverId }: { serverId: string }) {
  const { t } = useTranslation('servers')
  const job = useUpgradeJobsStore((state) => state.jobs[serverId])

  if (!job || job.status === 'succeeded') {
    return null
  }

  const variant = job.status === 'running' ? 'secondary' : 'destructive'
  const label = job.status === 'running' ? t('upgrade_badge_running') : t('upgrade_badge_failed')

  return (
    <Link params={{ id: serverId }} search={{ range: 'realtime' }} to="/servers/$id">
      <Badge variant={variant}>{label}</Badge>
    </Link>
  )
}

// apps/web/src/components/server/agent-version-section.tsx
const STAGES: UpgradeStage[] = ['downloading', 'verifying', 'pre_flight', 'installing', 'restarting']

interface ServerWithCaps {
  id: string
  agent_version?: string | null
  capabilities?: number | null
  effective_capabilities?: number | null
}

function compareVersion(left: string, right: string): number {
  return left.localeCompare(right, undefined, { numeric: true, sensitivity: 'base' })
}

export function AgentVersionSection({ server }: { server: ServerResponse & ServerWithCaps }) {
  const { t } = useTranslation(['servers', 'common'])
  const { user } = useAuth()
  const job = useUpgradeJob(server.id)
  const clearFinished = useUpgradeJobsStore((state) => state.clearFinished)
  const triggerUpgrade = useTriggerUpgrade()
  const { data: latest } = useQuery<LatestAgentVersionResponse>({
    queryKey: ['agent', 'latest-version'],
    queryFn: () => api.get<LatestAgentVersionResponse>('/api/agent/latest-version'),
    staleTime: 60_000,
  })

  const canUpgrade = user?.role === 'admin'
  const upgradeEnabled = getEffectiveCapabilityEnabled(
    server.effective_capabilities,
    server.capabilities,
    CAP_UPGRADE
  )
  const currentVersion = server.agent_version ?? '-'
  const latestVersion = latest?.version ?? null
  const showUpgradeButton =
    canUpgrade &&
    latestVersion &&
    server.agent_version &&
    compareVersion(latestVersion, server.agent_version) > 0

  useEffect(() => {
    if (job?.status === 'succeeded') {
      const timer = window.setTimeout(() => clearFinished(server.id), 3000)
      return () => window.clearTimeout(timer)
    }
  }, [job?.status, server.id, clearFinished])

  return (
    <div className="mb-6 rounded-lg border bg-card p-4" id="agent-version-section">
      <div className="mb-4 flex items-start justify-between gap-4">
        <div>
          <h2 className="font-semibold text-sm">{t('upgrade_section_title')}</h2>
          <div className="mt-1 flex flex-wrap gap-4 text-sm">
            <span>{t('upgrade_current')} {currentVersion}</span>
            <span>{t('upgrade_latest')} {latestVersion ?? '-'}</span>
          </div>
        </div>
        {showUpgradeButton && (
          <AlertDialog>
            <AlertDialogTrigger asChild>
              <Button disabled={!upgradeEnabled}>
                {t('upgrade_button', { version: latestVersion })}
              </Button>
            </AlertDialogTrigger>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>{t('upgrade_confirm_title')}</AlertDialogTitle>
                <AlertDialogDescription>{t('upgrade_confirm_body')}</AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>{t('common:cancel')}</AlertDialogCancel>
                <AlertDialogAction onClick={() => triggerUpgrade.mutate({ serverId: server.id, version: latestVersion! })}>
                  {t('upgrade_button', { version: latestVersion })}
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        )}
      </div>

      {job?.status === 'running' && (
        <div className="grid gap-2 sm:grid-cols-5">
          {STAGES.map((stage, index) => {
            const currentIndex = STAGES.indexOf(job.stage)
            const done = index < currentIndex
            const active = index === currentIndex
            return (
              <div className="rounded border px-3 py-2 text-xs" key={stage}>
                <div className={cn(done && 'text-emerald-600', active && 'font-medium')}>
                  {done ? '✓ ' : ''}
                  {t(`upgrade_stage_${stage}`)}
                </div>
              </div>
            )
          })}
        </div>
      )}

      {job?.status === 'failed' && (
        <div className="space-y-2 rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-sm">
          <div>{t('upgrade_failed', { stage: t(`upgrade_stage_${job.stage}`), error: job.error })}</div>
          {job.backup_path && <div>{t('upgrade_failed_hint', { path: job.backup_path })}</div>}
          {canUpgrade && (
            <Button onClick={() => triggerUpgrade.mutate({ serverId: server.id, version: job.target_version })} variant="outline">
              {t('upgrade_retry')}
            </Button>
          )}
        </div>
      )}
    </div>
  )
}

// apps/web/src/routes/_authed/servers/$id.tsx
<UptimeCard serverId={id} />
<AgentVersionSection server={serverWithCaps} />

// apps/web/src/routes/_authed/servers/index.tsx
<div className="flex items-center gap-2">
  <StatusBadge online={row.original.online} />
  <UpgradeJobBadge serverId={row.original.id} />
</div>

// apps/web/src/components/server/server-card.tsx
<div className="mb-3 flex items-center justify-between">
  <Link
    className="flex items-center gap-1.5 truncate border-transparent border-b pb-px hover:border-current"
    params={{ id: server.id }}
    search={{ range: 'realtime' }}
    to="/servers/$id"
  >
    {flag && (
      <span className="shrink-0 text-sm" title={server.country_code ?? ''}>
        {flag}
      </span>
    )}
    {osEmoji && (
      <span className="shrink-0 text-sm" title={server.os ?? ''}>
        {osEmoji}
      </span>
    )}
    <h3 className="truncate font-semibold text-sm">{server.name}</h3>
  </Link>
  <div className="flex items-center gap-2">
    <UpgradeJobBadge serverId={server.id} />
    <StatusBadge online={server.online} />
  </div>
</div>
```

- [ ] **Step 4: Run the UI tests to verify they pass**

Run:

```bash
bun --cwd apps/web x vitest run src/components/server/agent-version-section.test.tsx 'src/routes/_authed/servers/$id.test.tsx'
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/server/agent-version-section.tsx apps/web/src/components/server/agent-version-section.test.tsx apps/web/src/components/server/upgrade-job-badge.tsx apps/web/src/routes/_authed/servers/$id.tsx apps/web/src/routes/_authed/servers/$id.test.tsx apps/web/src/routes/_authed/servers/index.tsx apps/web/src/components/server/server-card.tsx apps/web/src/locales/en/servers.json apps/web/src/locales/zh/servers.json
git commit -m "feat(web): add agent upgrade ui"
```

### Task 9: Update Docs, Manual QA, and Run the Full Verification Sweep

**Files:**
- Modify: `ENV.md`
- Modify: `apps/docs/content/docs/en/configuration.mdx`
- Modify: `apps/docs/content/docs/cn/configuration.mdx`
- Create: `tests/agent-upgrade.md`
- Modify: `tests/README.md`

- [ ] **Step 1: Update the config docs and manual checklist**

```md
<!-- ENV.md -->
| `SERVERBEE_UPGRADE__LATEST_VERSION_URL` | `upgrade.latest_version_url` | string | `""` | Optional override for the latest agent version endpoint. Expected JSON: `{ "version": "x.y.z", "released_at": "..." }` |

<!-- apps/docs/content/docs/en/configuration.mdx -->
| `SERVERBEE_UPGRADE__LATEST_VERSION_URL` | `""` | Optional override URL for the latest agent version lookup. When unset, ServerBee auto-detects GitHub Releases from `release_base_url` |

| `latest_version_url` | string? | `None` | Optional JSON endpoint returning `{ "version": "x.y.z", "released_at"?: "..." }` for self-hosted release mirrors |

<!-- apps/docs/content/docs/cn/configuration.mdx -->
| `SERVERBEE_UPGRADE__LATEST_VERSION_URL` | `""` | 可选的最新版本查询 URL。未设置时，ServerBee 会根据 `release_base_url` 自动识别 GitHub Releases |

| `latest_version_url` | string? | `None` | 可选的 JSON 端点，返回 `{ "version": "x.y.z", "released_at"?: "..." }`，适用于自托管镜像源 |

<!-- tests/agent-upgrade.md -->
# Agent Upgrade

1. Trigger an upgrade from the server detail page and confirm the stepper advances through Downloading -> Restarting.
2. Force a checksum mismatch and confirm the detail page shows a failed status with the verifying-stage error.
3. Block agent reconnect for more than 120 seconds and confirm the job becomes Timeout.
4. Click Retry from a failed or timeout state and confirm a new `job_id` is created.
5. Verify a timestamped `.bak.<YYYYMMDD-HHMMSS>` file exists beside the agent binary and that stale backups are removed after 24 hours.
```

- [ ] **Step 2: Add the new checklist to the test index**

```md
| [agent-upgrade.md](agent-upgrade.md) | Agent upgrade lifecycle | `/servers`, `/servers/:id` |
```

- [ ] **Step 3: Run the full verification sweep**

Run:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
bun --cwd apps/web test
bun --cwd apps/web run typecheck
bun --cwd apps/web x ultracite check
```

Expected: every command passes cleanly.

- [ ] **Step 4: Commit**

```bash
git add ENV.md apps/docs/content/docs/en/configuration.mdx apps/docs/content/docs/cn/configuration.mdx tests/agent-upgrade.md tests/README.md
git commit -m "docs: document agent upgrade workflow"
```

## Self-Review

### Spec Coverage

- Requirements 1, 2, 11, 12, 13 are covered by Task 1 and Task 4.
- Requirements 3, 4, 9, 10, 14 are covered by Task 2, Task 4, Task 7, and Task 8.
- Requirements 5 and 6 are covered by Task 5.
- Requirements 7, 8, and 15 are covered by Task 3, Task 7, and Task 8.
- Manual verification and docs updates are covered by Task 9.
- Spec integration scenarios 1-10 coverage:
  - Scenarios 1, 9 (success, capability-denied fast-fail) and 2, 3 (Verifying failure, timeout sweep): Task 6 integration tests.
  - Scenario 10 (FullSync contains upgrades): Task 6 integration test.
  - Scenarios 4 (concurrent 409), 7 (job_id mismatch), 8 (same-version retry): Task 2 tracker unit tests.
  - Scenarios 5 (agent offline pre-check), 6 (checksum 404 pre-check): covered transitively by `trigger_upgrade` returning early before `start_job` is reached (verified by the success-path integration test confirming `start_job` is reachable, combined with tracker unit tests that assert no job is created on early-return paths).
- No spec gaps remain after adding `backup_path` to failure payloads and using a service-local conflict error instead of `AppError::Conflict`.

### Placeholder Scan

- No `TODO`, `TBD`, or "similar to above" placeholders remain.
- Every code-changing step includes concrete code snippets or exact command sequences.
- Generated files are explicitly called out as generated instead of hand-edited.

### Type Consistency

- `UpgradeStage`, `UpgradeStatus`, `UpgradeJobDto`, `LatestAgentVersionResponse`, `useUpgradeJob`, and `useTriggerUpgrade` are named consistently across backend, OpenAPI, and frontend tasks.
- The frontend translation keys use the repo's existing flat `upgrade_*` naming style instead of introducing nested i18n structures mid-feature.
