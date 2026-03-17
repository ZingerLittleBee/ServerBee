# Design: Monthly Traffic Statistics + Scheduled Tasks

**Date:** 2026-03-17
**Status:** Draft
**Scope:** Two features — monthly traffic statistics with quota alerts, and scheduled (cron) tasks with retry

---

## 1. Monthly Traffic Statistics

### 1.1 Problem

The Agent reports cumulative `net_in_transfer` / `net_out_transfer` values from the OS. These values reset to zero on Agent/system restart. The current alert system calculates cycle traffic via `last_record - first_record` in the `records` table, which produces incorrect results across restarts. There is no dedicated traffic tracking, no monthly usage visualization, and no prediction of whether usage will exceed quota.

### 1.2 Data Storage

Three new tables in a single migration (two traffic tables + one state table).

**`traffic_hourly`** — fine-grained, short retention:

| Column | Type | Notes |
|--------|------|-------|
| id | BIGINT PK AUTO_INCREMENT | |
| server_id | STRING NOT NULL FK -> servers ON DELETE CASCADE | |
| hour | TIMESTAMP NOT NULL | Truncated to hour |
| bytes_in | BIGINT NOT NULL DEFAULT 0 | Inbound delta bytes for this hour |
| bytes_out | BIGINT NOT NULL DEFAULT 0 | Outbound delta bytes for this hour |
| | UNIQUE | (server_id, hour) |

**`traffic_daily`** — aggregated, long retention:

| Column | Type | Notes |
|--------|------|-------|
| id | BIGINT PK AUTO_INCREMENT | |
| server_id | STRING NOT NULL FK -> servers ON DELETE CASCADE | |
| date | DATE NOT NULL | Calendar date |
| bytes_in | BIGINT NOT NULL DEFAULT 0 | Inbound delta bytes for this day |
| bytes_out | BIGINT NOT NULL DEFAULT 0 | Outbound delta bytes for this day |
| | UNIQUE | (server_id, date) |

**`traffic_state`** — persists last-seen cumulative counters per server (for cache recovery on Server restart):

| Column | Type | Notes |
|--------|------|-------|
| server_id | STRING PK FK -> servers ON DELETE CASCADE | One row per server |
| last_in | BIGINT NOT NULL DEFAULT 0 | Last seen net_in_transfer |
| last_out | BIGINT NOT NULL DEFAULT 0 | Last seen net_out_transfer |
| updated_at | TIMESTAMP NOT NULL | When last updated |

### 1.3 Delta Calculation

Performed in the existing `record_writer` task (runs every 60 seconds) alongside record persistence.

**State:** A `previous_transfer_cache: HashMap<ServerId, (i64, i64)>` held in memory, keyed by server_id, storing the last seen `(net_in_transfer, net_out_transfer)`.

**On Server startup:** Initialize cache from the `traffic_state` table (not `records`). This table is always kept in sync and is not subject to retention cleanup.

**Restart detection:** Each direction is checked independently. If `net_in_transfer < prev_in`, only the inbound direction is treated as restarted (raw value used as delta); outbound is computed normally, and vice versa. This avoids over-counting one direction when only the other direction's counter resets (e.g., from network interface changes).

Note: `net_in_transfer` / `net_out_transfer` are `i64` (64-bit). Overflow (18 exabytes) is not a realistic concern.

**Per report cycle:**

```
for (server_id, report) in latest_reports:
    if let Some((prev_in, prev_out)) = cache.get(server_id):
        // Per-direction independent restart detection
        delta_in  = if report.net_in_transfer  >= prev_in:
                        report.net_in_transfer  - prev_in   // Normal
                    else:
                        report.net_in_transfer              // This direction restarted

        delta_out = if report.net_out_transfer >= prev_out:
                        report.net_out_transfer - prev_out  // Normal
                    else:
                        report.net_out_transfer             // This direction restarted

        // Skip zero deltas (defensive)
        if delta_in > 0 || delta_out > 0:
            UPSERT traffic_hourly (server_id, current_hour):
                SET bytes_in = bytes_in + delta_in, bytes_out = bytes_out + delta_out
                // Uses SQLite native: INSERT ... ON CONFLICT(server_id, hour) DO UPDATE

    cache.insert(server_id, (report.net_in_transfer, report.net_out_transfer))
    UPSERT traffic_state (server_id): SET last_in = report.net_in_transfer,
                                          last_out = report.net_out_transfer,
                                          updated_at = now
```

The `traffic_state` UPSERT runs every 60 seconds (same as record_writer), ensuring the cache can always be recovered after Server restart with minimal data loss (at most 60 seconds of traffic).

### 1.4 Aggregation and Cleanup

**Aggregator task** (runs hourly, existing task extended):
- For each calendar day (in the **configured timezone**, not UTC) that has `traffic_hourly` data: SUM all hourly rows whose UTC `hour` falls within that local day, then INSERT OR REPLACE into `traffic_daily` (full-day total, not incremental). This is idempotent — safe if the aggregator runs multiple times for the same day.
- `traffic_daily.date` stores the **local date** in the configured timezone (e.g., if timezone=Asia/Shanghai, "2026-03-17" means 2026-03-17 00:00 CST to 2026-03-17 23:59 CST, which is 2026-03-16T16:00Z to 2026-03-17T15:59Z in UTC).
- Processes both yesterday and today (local timezone) to handle midnight boundary correctly.
- This runs alongside existing `records` hourly aggregation.
- The `traffic_hourly.hour` column continues to store UTC timestamps. Only the daily bucketing and billing cycle calculations use the configured timezone.

**Cleanup task** (runs hourly, existing task extended):
- Delete `traffic_hourly` rows older than `retention.traffic_hourly_days` (default: 7).
- Delete `traffic_daily` rows older than `retention.traffic_daily_days` (default: 400).

**New retention config fields:**

| Config key | Env var | Default | Purpose |
|------------|---------|---------|---------|
| `retention.traffic_hourly_days` | `SERVERBEE_RETENTION__TRAFFIC_HOURLY_DAYS` | 7 | Hourly traffic data retention |
| `retention.traffic_daily_days` | `SERVERBEE_RETENTION__TRAFFIC_DAILY_DAYS` | 400 | Daily traffic data retention (covers yearly billing) |

### 1.5 Billing Cycle Calculation

**New field on `servers` table:**

| Column | Type | Notes |
|--------|------|-------|
| `billing_start_day` | INTEGER NULL | Day of month (1-28) for cycle reset. NULL = natural month (1st) |

**Cycle range logic:**

```rust
fn get_cycle_range(billing_cycle: &str, billing_start_day: Option<i32>, today: NaiveDate)
    -> (NaiveDate, NaiveDate)
{
    let anchor = billing_start_day.unwrap_or(1); // Default: natural month

    match billing_cycle {
        "monthly" => {
            // Find current cycle start: most recent anchor day <= today
            // Cycle end: day before next anchor day
            // Example: anchor=15, today=Mar 20 -> (Mar 15, Apr 14)
            // Example: anchor=1 (natural), today=Mar 20 -> (Mar 1, Mar 31)
        }
        "quarterly" => {
            // Same logic but 3-month periods from anchor
        }
        "yearly" => {
            // Same logic but 12-month periods from anchor
        }
    }
}
```

For `billing_start_day > 28`: not allowed (avoids Feb edge cases). Validated on input. DDL includes `CHECK(billing_start_day BETWEEN 1 AND 28 OR billing_start_day IS NULL)` for defense-in-depth.

For unrecognized `billing_cycle` values: fall back to `"monthly"` behavior.

### 1.6 Traffic Query API

**`GET /api/servers/{id}/traffic`** — protected, any authenticated user.

Query params:
- `cycle_start` (optional, DATE): Start of a specific billing cycle to query. If omitted, returns the current cycle. Allows querying historical cycles for month-over-month comparison.

Response:

```json
{
  "cycle_start": "2026-03-01",
  "cycle_end": "2026-03-31",
  "bytes_in": 53687091200,
  "bytes_out": 10737418240,
  "bytes_total": 64424509440,
  "traffic_limit": 107374182400,
  "traffic_limit_type": "sum",
  "usage_percent": 60.0,
  "prediction": {
    "estimated_total": 95539077530,
    "estimated_percent": 89.0,
    "will_exceed": false
  },
  "daily": [
    { "date": "2026-03-01", "bytes_in": 1717986918, "bytes_out": 343597383 }
  ],
  "hourly": [
    { "hour": "2026-03-17T00:00:00Z", "bytes_in": 71582788, "bytes_out": 14316557 }
  ]
}
```

- `bytes_in` / `bytes_out` / `bytes_total`: Current cycle total, computed as `SUM(traffic_daily)` for completed local days + `SUM(traffic_hourly)` for today (local timezone). This ensures no lag from unaggregated data — same strategy as the billing alert query.
- `daily`: All days in the current cycle (from `traffic_daily`). Today's entry may be partial until the next aggregator run.
- `hourly`: Last 7 days only (from `traffic_hourly`).
- `usage_percent`: Based on `traffic_limit_type` — `sum` uses total, `up` uses bytes_out, `down` uses bytes_in.
- If `traffic_limit` is NULL, `usage_percent` and `prediction` are omitted.

### 1.7 Prediction Algorithm

```
days_elapsed = today - cycle_start + 1
recent_days = min(7, days_elapsed)
recent_sum = SUM(traffic_daily for completed local days in range)
           + SUM(traffic_hourly for today, local timezone)  // today not yet in traffic_daily
daily_avg = recent_sum / recent_days
days_remaining = cycle_end - today
estimated_total = current_total + daily_avg * days_remaining
will_exceed = traffic_limit IS NOT NULL AND estimated_total > effective_limit
```

`current_total` and `recent_sum` both use the same data source strategy as `bytes_total` in the traffic API: `traffic_daily` for completed local days + `traffic_hourly` for today. This ensures prediction, API totals, and alerts are always consistent.

`effective_limit` respects `traffic_limit_type`: for `up`/`down`, only the corresponding direction is compared.

Uses last 7 days (not full cycle) to better reflect recent usage trends.

Prediction is only computed when `days_elapsed >= 3` to avoid misleading results early in the cycle. When `days_elapsed < 3`, the `prediction` field is omitted from the response.

### 1.8 Alert System Fix

Existing `transfer_in_cycle`, `transfer_out_cycle`, `transfer_all_cycle` alert types in `AlertService::check_transfer_cycle` are refactored:

**Before:** Query `records` table, compute `last.net_in_transfer - first.net_in_transfer`. Uses fixed durations (hour=1h, day=24h, month=30d).

**After:** Query `traffic_hourly` table (not `traffic_daily`), compute `SUM(bytes_in)` / `SUM(bytes_out)` for the configured cycle range.

**Preserving existing semantics:** The existing `cycle_interval` field (`hour`/`day`/`week`/`month`/`year`) is preserved and continues to work as before — these are computed as relative time windows from `now`. A new `cycle_interval` value `"billing"` is added, which derives the cycle range from the server's `billing_cycle` + `billing_start_day` config (uses `get_cycle_range()`).

**Why `traffic_hourly` instead of `traffic_daily`:** The alert evaluator runs every 60 seconds. Querying `traffic_hourly` (updated every 60s by `record_writer`) gives near-real-time accuracy. Querying `traffic_daily` would introduce up to 1-hour lag since daily data is only aggregated hourly. For the `"billing"` cycle interval (which can span months), the query combines `SUM(traffic_daily)` for completed days + `SUM(traffic_hourly)` for today, avoiding both large query ranges and stale data.

This fixes:
- Agent restart causing incorrect delta calculation (traffic tables use correct deltas).
- Existing hour/day/week alerts continue working with same semantics but accurate data.
- New `"billing"` option enables alerts tied to actual VPS billing cycles.

### 1.9 Frontend — BillingInfoBar Enhancement

When `traffic_limit` is set, add a progress bar after the existing traffic limit text:

```
[Traffic: 60.0GB / 100GB [========--] 60%]
```

- Color thresholds: 0-70% green, 70-90% yellow, 90%+ red.
- If prediction `will_exceed`, a dashed marker overlays the predicted endpoint on the bar.
- Without `traffic_limit`: show used amount only, no bar.
- `traffic_limit_type` of `up`/`down`: bar shows only the relevant direction.

### 1.10 Frontend — Traffic Detail Card

Collapsible card below existing metrics charts on server detail page.

**Collapsed:** Single line summary — "This cycle: 60.0GB / 100GB - Est. month-end: 89.0GB"

**Expanded:**
- **Daily bar chart** (Recharts BarChart): X=date, Y=bytes. Stacked in/out bars. Dashed horizontal reference line at quota.
- **Hourly line chart** (last 7 days, day selector): Shows intra-day traffic peaks.
- **Prediction indicator**: Dashed line extending to month-end with estimated value.
- **Cycle info**: Start/end dates, reset mode (natural month vs billing day).

### 1.11 Frontend — Edit Dialog Extension

In `server-edit-dialog.tsx`, billing section adds:
- `billing_start_day`: Number input (1-28), placeholder "Leave empty for natural month (1st)".

---

## 2. Scheduled Tasks

### 2.1 Problem

The current task system only supports one-shot remote command execution. Users need recurring tasks (backups, log cleanup, cert renewal scripts) with cron scheduling and failure retry.

### 2.2 Data Model Changes

**`tasks` table — new columns** (same migration as traffic tables):

| Column | Type | Notes |
|--------|------|-------|
| `task_type` | STRING NOT NULL DEFAULT 'oneshot' | `oneshot` or `scheduled` |
| `name` | STRING NULL | Human-readable name (required for scheduled) |
| `cron_expression` | STRING NULL | Standard 5-field cron, e.g. `0 3 * * *` |
| `enabled` | BOOLEAN NOT NULL DEFAULT true | Toggle for scheduled tasks |
| `timeout` | INTEGER NULL | Execution timeout in seconds (existing param, now persisted) |
| `retry_count` | INTEGER NOT NULL DEFAULT 0 | Max retry attempts (0 = no retry) |
| `retry_interval` | INTEGER NOT NULL DEFAULT 60 | Seconds between retries |
| `last_run_at` | TIMESTAMP NULL | Last execution time |
| `next_run_at` | TIMESTAMP NULL | Next scheduled execution time |

**`task_results` table — new columns:**

| Column | Type | Notes |
|--------|------|-------|
| `run_id` | STRING NULL | UUID grouping all results from one cron trigger (NULL for oneshot tasks) |
| `attempt` | INTEGER NOT NULL DEFAULT 1 | 1 = first execution, 2+ = retry |
| `started_at` | TIMESTAMP NULL | When execution began (duration = finished_at - started_at) |

`run_id` groups results across servers and retries for a single cron trigger. The frontend uses it to display "Trigger time" (from the earliest `started_at` in a run group), "Duration", and "FAIL (2/3)" summaries.

### 2.3 Scheduler (New Background Task: `task_scheduler`)

**Architecture:** A new `TaskScheduler` struct is added to `AppState`:

```rust
// In state.rs:
pub struct AppState {
    // ... existing fields ...
    pub task_scheduler: Arc<TaskScheduler>,
}

// In service/task_scheduler.rs (or task/task_scheduler.rs):
pub struct TaskScheduler {
    scheduler: tokio::sync::RwLock<JobScheduler>,
    job_map: DashMap<String, JobUuid>,                        // task_id -> job UUID
    active_runs: DashMap<String, (String, CancellationToken)>, // task_id -> (run_id, cancel_token)
}

impl TaskScheduler {
    pub async fn add_job(&self, task: &task::Model) -> Result<(), AppError>;
    pub async fn remove_job(&self, task_id: &str) -> Result<(), AppError>;  // cancels active run + removes job
    pub async fn update_job(&self, task: &task::Model) -> Result<(), AppError>;
    pub async fn disable_job(&self, task_id: &str) -> Result<(), AppError>; // cancels active run + removes job
    pub fn is_running(&self, task_id: &str) -> bool;           // check if a run is active
    pub fn cancel_active_run(&self, task_id: &str);            // cancel + remove from active_runs
}
```

This ensures CRUD routes (which have `Arc<AppState>`) can directly call `state.task_scheduler.add_job()` etc. for immediate synchronization.

Registered in `main.rs` as the 7th background task (for the initial startup load + scheduler tick loop).

**On startup:**
1. Load all `tasks` where `task_type = 'scheduled' AND enabled = true`.
2. For each task, register a Job in `tokio-cron-scheduler`'s `JobScheduler`.
3. Store mappings in the `job_map: DashMap<task_id, JobUuid>`.

**Job execution flow:**

**Overlap policy: skip if already running.** When a cron trigger fires or "Run Now" is invoked, the scheduler first checks `active_runs.contains_key(task_id)`. If a run is already active, the new trigger is silently skipped (logged at WARN level). This prevents resource waste, duplicate results, and cancellation token conflicts. Most cron schedulers use this policy by default.

```
Job triggers:
  0. If active_runs.contains_key(task_id): skip (log WARN "task still running, skipping")
  1. Load task from DB (get latest command, server_ids, timeout, retry config)
     // Command is captured at load time — if task is edited mid-retry, in-flight retries
     // continue with the original command.
  2. Generate run_id (UUID), create CancellationToken
     active_runs.insert(task_id, (run_id, token.clone()))
  3. Update last_run_at = now, compute and update next_run_at
  4. For each target server with CAP_EXEC:
     tokio::spawn an independent execution chain (parallel per server, non-blocking):
       a. Check agent online: if agent_manager.get_sender(server_id) is None:
            Write synthetic result (exit_code=-3, output="Server offline")
            continue to next server
       b. Send ServerMessage::Exec via agent_manager
          If send fails: write synthetic result (exit_code=-3, output="Dispatch failed")
          continue to next server
       c. Wait for TaskResult (with timeout)
          If oneshot channel times out or errors (agent disconnected, WS lost):
            Write synthetic result (exit_code=-4, output="No response within {timeout}s")
            // This counts as a failure — eligible for retry
       d. Write result to task_results (attempt=1)
       e. If exit_code != 0 AND retry_count > 0:
          for attempt in 2..=retry_count+1:
            // Check cancellation before each retry
            if cancellation_token.is_cancelled(): break
            sleep(retry_interval)
            if cancellation_token.is_cancelled(): break
            Re-send Exec, wait for result
            Write to task_results (attempt=N)
            if exit_code == 0: break
  5. For servers without CAP_EXEC: write synthetic result (exit_code=-2)
  6. After all spawned tasks complete (tokio::JoinSet):
     active_runs.remove(task_id)  // Mark run as finished, allow next trigger
```

**Exit code conventions for synthetic results:**
- `-1`: Execution timeout (agent-side)
- `-2`: Capability denied (CAP_EXEC not enabled)
- `-3`: Server offline or dispatch failed
- `-4`: Scheduler timeout — no response from agent (WS disconnect, message lost, or oneshot channel timeout)

Each server's execution+retry chain runs in its own `tokio::spawn` task, so:
- Multiple target servers execute concurrently.
- Retries do not block the cron scheduler's job thread.
- A maximum total duration guard applies: `timeout * (retry_count + 1) + retry_interval * retry_count`. If exceeded, remaining retries are abandoned.

**Cancellation:** `TaskScheduler` maintains `active_runs: DashMap<task_id, (run_id, CancellationToken)>` (using `tokio_util::sync::CancellationToken`). The token is created per-run (step 2 in the flow above) and stored in `active_runs`. Each spawned retry chain holds a clone of the token and checks `token.is_cancelled()` before each retry and before writing results. When all spawned tasks in a run complete, the entry is removed from `active_runs` (step 6). On delete/disable/update, CRUD routes call `cancel_active_run(task_id)` which cancels the token and removes the entry, stopping in-flight chains. This prevents orphaned retries and ensures overlap detection uses a clean state.

**Correlation ID design:** Each Exec sent by the scheduler uses a correlation ID as the `task_id` field in the Exec message. Format: `{scheduled_task_id}:{run_id}:{server_id}:{attempt}`. This ensures each (server, attempt) pair has a unique key in `AgentManager.pending_requests`, so multiple servers returning results concurrently do not collide.

The scheduler registers one oneshot channel per (server, attempt) combination. The agent WS handler in `router/ws/agent.rs` is modified for **both** `TaskResult` and `CapabilityDenied`:

- **TaskResult:** First tries `agent_manager.dispatch_pending_response(&result.task_id)`. If dispatched, the scheduler handles DB persistence. If not (one-shot task), existing code path saves directly.
- **CapabilityDenied:** The WS handler converts it to a synthetic `TaskResult` (exit_code=-2, output="Capability denied: exec"), then tries `dispatch_pending_response`. If dispatched, scheduler handles it. If no pending listener (one-shot task), the synthetic TaskResult is saved directly with exit_code=-2. **Note:** the existing direct-save path in `agent.rs` currently writes exit_code=-1 for CapabilityDenied — this is changed to exit_code=-2 for consistency. Exit code -2 always means "capability denied" regardless of task type.

This ensures the scheduler always receives a result for every dispatched Exec, whether the agent executes it, denies it, times out, or goes offline.

**Timeout and pending_requests lifetime:** The scheduler waits `task.timeout + 10s` for each Exec response. However, the existing `offline_checker` task cleans up pending requests older than 60 seconds (`cleanup_expired_requests(Duration::from_secs(60))`). This would prematurely remove the scheduler's oneshot channels for long-running tasks (default timeout=300s).

Fix: `pending_requests` value type is changed from `(oneshot::Sender, Instant)` to `(oneshot::Sender, Instant, Duration)`, where the third element is the per-entry TTL. When registering a pending request, the caller specifies the expected wait duration (file operations: 60s, scheduled tasks: `timeout + 10s`). `cleanup_expired_requests` checks each entry's own TTL (`now - created_at > ttl`) instead of using a global cutoff.

**Manual trigger (`POST /api/tasks/{id}/run`):** Fires one immediate execution without retry (ignores retry_count/retry_interval). Uses the same Exec dispatch path. Subject to the same overlap check: if the task is already running, returns **409 Conflict** with `{ "error": { "code": "TASK_ALREADY_RUNNING", "message": "Task is currently running, try again later" } }`. This allows the frontend to show a clear message to the user instead of silently skipping.

**CRUD synchronization:**
- Create scheduled task -> register new Job in scheduler.
- Update cron/enabled/command -> cancel in-flight retries, remove old Job, register new Job if enabled.
- Delete scheduled task -> cancel in-flight retries, remove Job, delete associated task_results.
- Disable -> cancel in-flight retries, remove Job from scheduler, keep DB record.
- Enable -> register Job in scheduler.

### 2.4 Result Retention

**New retention config:**

| Config key | Env var | Default |
|------------|---------|---------|
| `retention.task_results_days` | `SERVERBEE_RETENTION__TASK_RESULTS_DAYS` | 7 |

Cleanup task extended: `DELETE FROM task_results WHERE finished_at < now - task_results_days`. Applies to both oneshot and scheduled task results.

### 2.5 API Routes

| Method | Path | Description | Auth |
|--------|------|-------------|------|
| `GET /api/tasks` | **New** | List tasks, filter by `?type=scheduled` or `?type=oneshot` | admin |
| `POST /api/tasks` | **Modified** | Create task. If `task_type=scheduled`, registers cron job | admin |
| `GET /api/tasks/{id}` | Unchanged | Task detail | admin |
| `PUT /api/tasks/{id}` | **New** | Edit scheduled task (name, cron, command, servers, enabled, retry) | admin |
| `DELETE /api/tasks/{id}` | **New** | Delete task + remove scheduler job + cascade delete results | admin |
| `POST /api/tasks/{id}/run` | **New** | Manual trigger of a scheduled task (immediate one-off execution) | admin |
| `GET /api/tasks/{id}/results` | **Extended** | Paginated, includes `attempt` field. Query: `?page=1&per_page=20` | admin |

**POST /api/tasks request body (scheduled):**

```json
{
  "task_type": "scheduled",
  "name": "Daily log cleanup",
  "command": "find /var/log -name '*.log.gz' -mtime +30 -delete",
  "server_ids": ["srv-1", "srv-2"],
  "cron_expression": "0 3 * * *",
  "timeout": 300,
  "retry_count": 3,
  "retry_interval": 60
}
```

### 2.6 Frontend — Tasks Page Redesign

**Tab layout:**

```
[One-off Tasks]  [Scheduled Tasks]
```

**One-off Tasks tab:** Unchanged from current implementation. Same command input, server selection, result display.

**Scheduled Tasks tab — List view:**

| Name | Cron | Description | Servers | Last Run | Next Run | Status | Actions |
|------|------|-------------|---------|----------|----------|--------|---------|
| Log cleanup | `0 3 * * *` | Every day at 3:00 AM | srv-1, srv-2 | Mar 17 03:00 OK | Mar 18 03:00 | Enabled | Edit, Run Now, Delete |
| DB backup | `0 0 * * 0` | Every Sunday at midnight | srv-3 | Mar 16 00:00 FAIL (2/3) | Mar 23 00:00 | Enabled | Edit, Run Now, Delete |

- "Cron" column shows expression; "Description" column shows human-readable text (parsed client-side with `cronstrue` or similar library).
- "Status" column: toggle switch for enable/disable.
- "Last Run" shows the latest result status with color (green=success, red=fail with retry count).
- "Run Now" button: triggers `POST /api/tasks/{id}/run`, immediate execution.

**Create/Edit dialog:**
- Name (required)
- Cron expression input + human-readable preview (e.g., "Every day at 3:00 AM")
- Command (monospace textarea)
- Server multi-select (same component as one-off tasks, respects CAP_EXEC)
- Timeout (seconds, default 300)
- Retry count (0-10, default 0)
- Retry interval (seconds, default 60, shown only when retry_count > 0)

**Execution history (expandable per task):**
- Click a scheduled task row to expand history panel.
- Results are grouped by `run_id` (one group = one cron trigger).
- Group header: Trigger time (earliest `started_at` in group) | Overall status (all OK / N failed) | Total servers
- Expand group to see per-server results: Server | Exit code | Attempt | Duration (`finished_at - started_at`)
- Click a result row to view full output in a dialog.
- Pagination by run groups (20 groups per page).
- API: `GET /api/tasks/{id}/results?page=1&per_page=20` returns results ordered by `started_at DESC`, grouped by `run_id`.

---

## 3. Database Migration

A single migration `m20260317_000005_traffic_and_scheduled_tasks` covering all schema changes:

1. Create `traffic_hourly` table.
2. Create `traffic_daily` table.
3. Create `traffic_state` table.
4. Add `billing_start_day` column to `servers` table (with CHECK constraint).
5. Add columns to `tasks` table: `task_type`, `name`, `cron_expression`, `enabled`, `timeout`, `retry_count`, `retry_interval`, `last_run_at`, `next_run_at`.
6. Add `run_id`, `attempt`, `started_at` columns to `task_results` table.

---

## 4. Configuration Changes

New fields in `AppConfig`:

```toml
[retention]
traffic_hourly_days = 7
traffic_daily_days = 400
task_results_days = 7

[scheduler]
timezone = "UTC"   # IANA timezone for cron scheduling and billing cycle boundaries
                   # Examples: "UTC", "Asia/Shanghai", "America/New_York"
```

Environment variable mapping:
- Retention: `SERVERBEE_RETENTION__` prefix (existing pattern).
- Timezone: `SERVERBEE_SCHEDULER__TIMEZONE` (default: `"UTC"`).

**Timezone semantics:**
- **Cron scheduling:** `tokio-cron-scheduler` uses `chrono-tz` for timezone-aware scheduling. The configured timezone determines when "every day at 3:00 AM" fires.
- **Billing cycle boundaries:** `get_cycle_range()` computes dates in the configured timezone. A "natural month starting March 1st" means March 1st 00:00 in the configured timezone.
- **All timestamps in DB remain UTC.** Timezone conversion happens only at the boundary: cron evaluation and cycle range computation.

---

## 5. Files to Create or Modify

### New files:
- `crates/server/src/migration/m20260317_000005_traffic_and_scheduled_tasks.rs`
- `crates/server/src/entity/traffic_hourly.rs`
- `crates/server/src/entity/traffic_daily.rs`
- `crates/server/src/entity/traffic_state.rs`
- `crates/server/src/service/traffic.rs` — delta calculation, cycle logic, query, prediction
- `crates/server/src/service/task_scheduler.rs` — TaskScheduler struct (lives in service/ since it's shared via AppState)
- `crates/server/src/router/api/traffic.rs` — GET /api/servers/{id}/traffic
- `apps/web/src/hooks/use-traffic.ts` — React Query hooks for traffic API
- `apps/web/src/components/server/traffic-card.tsx` — collapsible traffic detail card
- `apps/web/src/components/server/traffic-progress.tsx` — BillingInfoBar progress bar
- `apps/web/src/components/task/scheduled-task-list.tsx` — scheduled tasks list
- `apps/web/src/components/task/scheduled-task-dialog.tsx` — create/edit dialog

### Modified files:
- `crates/server/src/migration/mod.rs` — register new migration
- `crates/server/src/entity/mod.rs` — register new entities
- `crates/server/src/entity/server.rs` — add `billing_start_day` field
- `crates/server/src/entity/task.rs` — add scheduled task fields
- `crates/server/src/entity/task_result.rs` — add `run_id`, `attempt`, `started_at` fields
- `crates/server/src/config.rs` — add retention fields + scheduler timezone
- `crates/server/src/state.rs` — add `task_scheduler: Arc<TaskScheduler>` to AppState
- `crates/server/src/main.rs` — create TaskScheduler, pass to AppState, spawn scheduler loop
- `crates/server/src/service/server.rs` — add `billing_start_day` to UpdateServerInput DTO
- `crates/server/src/task/record_writer.rs` — add delta calculation and traffic_hourly upsert
- `crates/server/src/task/aggregator.rs` — add traffic_hourly -> traffic_daily aggregation
- `crates/server/src/task/cleanup.rs` — add traffic + task_results cleanup
- `crates/server/src/service/mod.rs` — export new `traffic` and `task_scheduler` modules
- `crates/server/src/service/alert.rs` — refactor check_transfer_cycle to use traffic_hourly/daily
- `crates/server/src/service/agent_manager.rs` — pending_requests per-entry TTL (replace global 60s cutoff)
- `crates/server/src/task/offline_checker.rs` — cleanup_expired_requests uses per-entry TTL
- `crates/server/src/router/api/mod.rs` — register traffic and task routes
- `crates/server/src/router/ws/agent.rs` — TaskResult/CapabilityDenied dispatch + exit_code=-2 unification
- `crates/server/src/router/api/task.rs` — extend with PUT, DELETE, GET list, POST run, 409 on overlap
- `crates/server/src/openapi.rs` — register new traffic/task route schemas and DTOs
- `apps/web/src/routes/_authed/servers/$id.tsx` — BillingInfoBar enhancement + traffic card
- `apps/web/src/routes/_authed/settings/tasks.tsx` — tab layout + scheduled tasks UI
- `apps/web/src/components/server/server-edit-dialog.tsx` — billing_start_day field
- `apps/web/src/lib/api-schema.ts` — regenerated from OpenAPI
- `ENV.md` — document new env vars
- `apps/docs/content/docs/{en,cn}/configuration.mdx` — document new config fields
- `TESTING.md` — update test counts, add new test file locations, update verification checklist

---

## 6. Testing Strategy

### Rust unit tests:
- `service/traffic.rs`: delta calculation (normal, single-direction restart, both-direction restart, server restart recovery), cycle range computation (natural month, billing day, quarterly, yearly, edge cases like anchor=28 in Feb), prediction algorithm (including days_elapsed < 3 guard).
- `service/alert.rs`: updated transfer_cycle alert using traffic_hourly data; verify `"billing"` cycle_interval uses server config; verify existing hour/day/week intervals still work correctly.
- `service/task_scheduler.rs`: job registration, execution flow, retry logic (mock Exec/TaskResult), correlation ID uniqueness across servers, run_id grouping.
- Migration: verify all new columns and tables created correctly.

### Rust integration tests:
- Traffic flow: simulate Agent reports with single-direction restart (only in resets, out continues), verify per-direction delta correctness.
- Scheduled task: create, trigger against 2+ servers, verify run_id groups results correctly, verify retry produces multiple attempts.
- Scheduler CRUD sync: create/update/delete scheduled task via API, verify scheduler job is immediately added/updated/removed.
- API: traffic query returns correct cycle data; task CRUD operations; results pagination by run_id groups.

### Frontend vitest tests:
- `use-traffic.ts`: query hooks, data transformation.
- Traffic progress bar: color thresholds, limit type handling.
- Scheduled task dialog: form validation (cron expression, retry fields).
- Tab switching behavior.
- Execution history grouping by run_id.
