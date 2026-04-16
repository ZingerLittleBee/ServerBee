# Agent Recovery Merge Design

**Date:** 2026-04-16
**Status:** Draft
**Scope:** Admin-driven recovery of a reinstalled agent by merging a new temporary server record back into the original offline server record

## Problem

The current registration model can reuse a server row only when the agent still has a valid token or when the machine fingerprint remains stable. After a true system reinstall, the old token is often gone and the machine fingerprint may change, so the server creates a new temporary server record instead of reconnecting to the original one.

That creates two operational problems:

1. The original server record keeps the historical charts, alerts, tasks, billing metadata, and dashboard references, but it stays offline.
2. The newly registered server record becomes the live agent identity, but it starts with little or no history and is not the record users want to keep.

The desired recovery flow is:

- The admin starts from the old offline server record.
- The admin picks a newly registered online temporary server record.
- The system rebinds the live agent to the old server identity.
- The system merges the temporary server's history into the old server record.
- Overlapping time ranges prefer the temporary server's data.
- The temporary server record is deleted after recovery completes.

This is a targeted recovery flow only. It is not a general-purpose "merge any two servers" feature.

## Goals

- Preserve the original `server_id` as the long-term identity.
- Restore the live agent onto the original server record without requiring manual input on the agent.
- Merge historical data from the temporary server into the original server.
- Treat overlapping time ranges as `source wins`.
- Keep user-managed server configuration on the original record.
- Replace runtime system fields on the original record with the recovered agent's latest values.
- Automatically remove the temporary server record after successful recovery.
- Make the workflow explicit, auditable, and retryable.

## Non-Goals

- No attempt to fill monitoring gaps during the reinstall window.
- No support for arbitrary record-to-record merge in v1.
- No attempt to reverse the full workflow after the recovered agent has successfully rebound.
- No new permanent "installation identity" entity in v1.
- No merge behavior for data that is not keyed by `server_id` and is not semantically tied to one server record, such as `service_monitor_record`.

## User Workflow

### Entry Point

The recovery action appears only on a server detail page for a server that is currently offline.

Button label:

- `claim and merge new agent`

The action is admin-only.

### Candidate Selection

The action opens a dialog showing candidate temporary server records. Candidates must satisfy all of the following:

- currently online
- not equal to the target server
- not already participating in another recovery job
- still plausibly a temporary auto-registered record

Candidate ranking is recommendation-only. The admin must still explicitly confirm the selected source.

Recommended ranking signals:

- same or similar `last_remote_addr`
- matching `cpu_arch`
- matching `os`
- matching `virtualization`
- close `agent_version`
- close `created_at`
- `target` went offline before `source` was created
- matching `mem_total`
- matching `disk_total`
- matching `cpu_cores`
- matching `country_code` and `region`

The dialog should show a short explanation for why a candidate was recommended.

### Confirmation

Before execution, the dialog shows a summary:

- keep the old server record
- move the live agent identity onto the old server
- merge history from the temporary record
- when timestamps overlap, the temporary record wins
- delete the temporary record after success

### Result States

- On success: the original server becomes online again and the temporary server disappears.
- On failure before rebind: the temporary server remains unchanged and the admin can retry.
- On failure after rebind but before cleanup: the original server remains the live identity, the temporary server remains present, and the admin can retry completion.

## Terminology

- `target`: the original offline server record that will be kept
- `source`: the newly registered online temporary server record that will be absorbed and deleted

## Architecture

The recovery feature is implemented as a staged server-side recovery merge job.

High-level flow:

1. Validate `target` and `source`.
2. Rebind the live agent from `source` identity to `target` identity.
3. Wait for the agent to reconnect as `target`.
4. Freeze writes for both `target` and `source`.
5. Merge `source` history into `target`.
6. Update runtime fields on `target`.
7. Delete `source`.
8. Unfreeze writes and mark the job complete.

The key design choice is to split "future writes go to the right identity" from "past writes are merged." The system must not start deleting or migrating `source` history until the agent has actually rebound onto `target`.

## Components

### 1. Recovery Merge Job Tracker

Add a server-local tracker for recovery jobs, similar in spirit to the upgrade job tracker.

Tracked fields:

- `job_id`
- `target_server_id`
- `source_server_id`
- `status`
- `stage`
- `started_at`
- `updated_at`
- `error`
- per-stage checkpoint metadata

Suggested stages:

- `validating`
- `rebinding`
- `awaiting_target_online`
- `freezing_writes`
- `merging_history`
- `finalizing`
- `succeeded`
- `failed`

The tracker provides:

- protection against concurrent recovery jobs involving the same server
- visible progress for the frontend
- a retry boundary after partial completion

### 2. Agent Rebind Protocol

Add a dedicated protocol message that instructs a connected agent currently identified as `source` to switch to `target`.

New server-to-agent message:

- `ServerMessage::RebindIdentity { job_id, target_server_id, token }`

New agent-to-server messages:

- `AgentMessage::RebindIdentityAck { job_id }`
- `AgentMessage::RebindIdentityFailed { job_id, error }`

Agent behavior:

1. Receive `RebindIdentity`.
2. Persist the new token locally.
3. Acknowledge success or failure.
4. Disconnect.
5. Reconnect using the new token, which now authenticates as `target`.

The target server row receives a newly generated token. The source row keeps its existing token until final cleanup so that failure before the rebind is easy to reason about.

### 3. Write Freeze Guard

The system needs an explicit in-memory recovery lock for `target` and `source` during merge.

Reason:

- `records` are persisted asynchronously by `record_writer`
- `ping_records`, `task_results`, and `network_probe_record` are persisted directly from the WebSocket handler
- `traffic_hourly` and `traffic_state` are updated continuously

Without a write freeze, merge results could be invalidated by concurrent writes after the merge has already decided which side wins.

The guard should:

- block or drop writes for both `target` and `source` during `freezing_writes`, `merging_history`, and `finalizing`
- make the skip explicit in logs
- be lifted immediately after the job completes or fails

This intentionally allows a small monitoring gap during the merge window. That is acceptable because gap filling is out of scope and already accepted by the product requirements.

## Data Model Semantics

### Canonical Identity

The final canonical identity is always `target.server_id`.

After the recovery:

- all future agent writes use `target.server_id`
- all kept history belongs to `target.server_id`
- `source.server_id` no longer exists

### Server Row Field Policy

On `servers(target)`, keep the original user-managed fields:

- `name`
- `group_id`
- `weight`
- `hidden`
- `remark`
- `public_remark`
- `price`
- `billing_cycle`
- `currency`
- `expired_at`
- `traffic_limit`
- `traffic_limit_type`
- `billing_start_day`
- `capabilities`

On `servers(target)`, replace runtime fields from `source`:

- `cpu_name`
- `cpu_cores`
- `cpu_arch`
- `os`
- `kernel_version`
- `mem_total`
- `swap_total`
- `disk_total`
- `ipv4`
- `ipv6`
- `region`
- `country_code`
- `virtualization`
- `agent_version`
- `protocol_version`
- `features`
- `last_remote_addr`
- `fingerprint`

`server_tags` remain those of `target`.

## History Merge Rules

The merge logic is table-specific.

### Category A: Keep Target Configuration, Drop Source Configuration

These tables or fields are treated as target-owned configuration and are not merged from source:

- `servers` user-managed fields listed above
- `server_tag`
- `network_probe_config`
- any `server_ids_json` references already pointing at `target`

Source-owned values in this category are discarded when `source` is deleted.

### Category B: Raw Time-Series Tables

For raw tables without a natural uniqueness key, merge by replacing the target's overlapping time window with source data.

Algorithm per table:

1. Read the source time range: `min_ts` and `max_ts`.
2. Delete target rows whose timestamps fall in `[min_ts, max_ts]`.
3. Rewrite all source rows to `target.server_id`.
4. Delete the original source rows if they were not already moved by update.

This gives exact `source wins` behavior over the source's active time window.

Apply this policy to:

- `records`
- `gpu_record`
- `ping_record`
- `task_result`
- `network_probe_record`
- `docker_event`

Field-specific time keys:

- `records.time`
- `gpu_record.time`
- `ping_record.time`
- `task_result.finished_at`
- `network_probe_record.timestamp`
- `docker_event.timestamp`

Notes:

- `task_result` overlap uses `finished_at`; no attempt is made to semantically deduplicate by command.
- `docker_event` overlap uses event timestamp and still follows `source wins`.

### Category C: Aggregated or Unique-Key Tables

For tables with a uniqueness key or a natural aggregate bucket, merge by key with strict `source wins`.

Algorithm per table:

1. For each source row, compute the target conflict key.
2. Delete any target row with the same key.
3. Rewrite the source row to `target.server_id`.

Apply this policy to:

- `records_hourly` with key `(server_id, time)`
- `network_probe_record_hourly` with key `(server_id, target_id, hour)`
- `traffic_hourly` with key `(server_id, hour)`
- `traffic_daily` with key `(server_id, date)`
- `uptime_daily` with key `(server_id, date)`
- `alert_state` with key `(rule_id, server_id)`
- `traffic_state` with key `server_id`

Special notes:

- `alert_state`: when both sides exist for the same `(rule_id, server_id)` pair, keep the source row and delete the target row.
- `traffic_state`: always take the source row because it is the live baseline for future traffic deltas.

### Category D: Not Merged

Do not merge:

- `service_monitor_record`

Reason:

- It is keyed by `monitor_id`, not `server_id`.
- It does not represent per-server ownership in the way the recovery feature needs.

## Recovery Job Flow

### Stage 1: Validating

Checks:

- `target` exists
- `source` exists
- `target` is offline
- `source` is online
- neither record is already in another recovery job
- `source` still looks like a temporary auto-registered node

If any check fails, the job fails without side effects.

### Stage 2: Rebinding

1. Generate a new token for `target`.
2. Persist the new token hash and prefix on `target`.
3. Send `RebindIdentity` to the currently connected `source` agent.
4. Wait for `RebindIdentityAck`.

If the agent reports failure, the job fails here and no history is merged.

### Stage 3: Awaiting Target Online

Wait for the recovered agent to reconnect as `target`.

Success condition:

- `target` becomes the current online connection

Failure condition:

- timeout

Timeout does not roll back to the old identity. The job simply fails before merge and keeps `source` untouched.

### Stage 4: Freezing Writes

Enable recovery locks for both `target` and `source`.

This must happen only after `target` is already online under the recovered identity, because the freeze may cause some writes to be skipped.

### Stage 5: Merging History

Execute the table-group merge in bounded transactions.

Recommended groups:

- group 1: `records`, `gpu_record`, `docker_event`
- group 2: `records_hourly`, `uptime_daily`, `traffic_hourly`, `traffic_daily`, `traffic_state`
- group 3: `ping_record`, `task_result`, `network_probe_record`, `network_probe_record_hourly`
- group 4: `alert_state`

Each group:

- runs in its own DB transaction
- records a completed checkpoint before the next group starts

### Stage 6: Finalizing

1. Update `servers(target)` runtime fields from `source`.
2. Delete remaining source-owned rows that were not already moved.
3. Delete the `source` server row.
4. Clear job locks.
5. Write audit log entries.

### Stage 7: Terminal State

- `succeeded`
- `failed`

## Failure Handling

### Failure Before Target Rebind Succeeds

If the job fails before `target` reconnects:

- do not merge history
- do not delete source
- do not freeze writes
- mark the job failed

This keeps retry semantics simple.

### Failure After Target Rebind Succeeds

If the job fails after `target` is already online:

- keep `target` as the live identity
- keep `source` present
- keep job checkpoints
- allow retry from the first incomplete merge stage

The system does not attempt a full rollback after the live identity has already switched. That would be more fragile than completing the merge forward.

### Failure During Final Cleanup

If all history has been merged but deleting `source` fails:

- leave the source row present
- mark the job failed in `finalizing`
- allow a retry that only runs the remaining cleanup steps

## Transaction Strategy

Do not use one global transaction for the entire recovery flow.

Reasons:

- the workflow includes WebSocket disconnect and reconnect
- SQLite lock duration would be too large
- a late failure would waste all merge work

Instead:

- use short transactions for validation-side DB writes
- use no transaction during the async rebind wait
- use one transaction per merge table group
- use one short transaction for final cleanup

This provides clear checkpoints and safe retries.

## API and UI

### API

Suggested endpoints:

- `GET /api/servers/{target_id}/recovery-candidates`
- `POST /api/servers/{target_id}/recover-merge`
- `GET /api/servers/recovery-jobs/{job_id}`

`POST /recover-merge` request body:

```json
{
  "source_server_id": "..."
}
```

Response:

```json
{
  "data": {
    "job_id": "...",
    "status": "running",
    "stage": "validating"
  }
}
```

### UI

On the target server detail page:

- admin-only button: `claim and merge new agent`
- candidate list dialog with match explanations
- confirmation dialog with irreversible-effect summary

During execution:

- show recovery stage on the target page
- show source as `recovery in progress`

On success:

- refresh both list and detail views
- target remains
- source disappears

On failure:

- show stage-specific error
- offer retry

## Audit Logging

Write explicit audit entries for:

- recovery started
- source selected
- rebind succeeded or failed
- merge succeeded or failed
- source deleted

Recommended detail payload:

- `job_id`
- `target_server_id`
- `source_server_id`
- `stage`
- `error`

## Testing Strategy

### Backend Integration Tests

Must cover:

1. successful end-to-end recovery
2. rebind failure before merge
3. timeout waiting for target online
4. failure during one merge group with retryable checkpoint state
5. successful retry after partial failure
6. `source wins` for each unique-key table
7. raw time-window replacement for each raw history table
8. write-freeze behavior during merge
9. final cleanup deleting the source record

### Agent Tests

Must cover:

1. receiving `RebindIdentity`
2. persisting the new token
3. reporting ack and failure
4. reconnecting with the new identity

### Frontend Tests

Must cover:

1. candidate ranking and rendering
2. confirmation summary
3. progress state rendering
4. error state and retry action

## Rollout

Recommended rollout order:

1. backend job tracker and protocol
2. agent rebind support
3. write-freeze guards
4. merge engine and tests
5. UI workflow
6. documentation

## Open Tradeoffs

- The merge window intentionally drops some live writes due to the recovery lock. This is acceptable because monitoring-gap repair is out of scope.
- The design chooses forward completion over full rollback after live identity rebind. This reduces failure complexity and matches the operational priority of restoring the server under the original identity.
- The design does not try to infer recovery automatically. Admin confirmation remains mandatory to avoid silent mis-merges.
