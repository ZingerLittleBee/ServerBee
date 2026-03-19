# P12: Disk I/O Monitoring Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Capture per-disk read/write throughput from agents, persist it alongside existing server records, and render historical disk I/O charts on the server detail page.

**Architecture:** Extend `SystemReport` with an optional `disk_io` payload. The agent collector keeps per-device byte counters and computes deltas on each collection cycle; the server stores raw and hourly disk I/O snapshots in JSON columns on `records` and `records_hourly`; the frontend parses `disk_io_json` into merged and per-disk chart series rendered by a dedicated chart component. Keep P12 historical-only in the UI; the realtime range should hide the disk I/O section until `ServerStatus`/WebSocket support is intentionally added.

**Tech Stack:** Rust (serde, sea-orm, sea-orm-migration, tokio, sysinfo, platform-specific collectors), React (TanStack Query/Router, Recharts, i18next)

**Spec:** `docs/superpowers/specs/2026-03-19-batch1-batch2-features-design.md` Section 4

**Implementation decisions:**
- Use a new migration file, `crates/server/src/migration/m20260319_000008_disk_io_records.rs`, instead of editing `m20260319_000007_service_monitor.rs`.
- Reserve `disk_io = None` / SQL `NULL` for old agents or unsupported collectors; use `Some(vec![])` / JSON `[]` for first-sample baseline or “no physical disks after filtering”.
- Ship a Linux production collector in P12 and keep macOS/Windows compile-safe with explicit `None` fallbacks unless product scope expands.
- Document `GET /api/servers/{id}/records` with the raw-record schema because raw and hourly JSON payloads remain field-compatible.
- Fix the existing server-detail range selection bug while touching the page so `1h`, `6h`, `24h`, `7d`, and `30d` stay distinct.

---

## File Structure

### Modified Files (Common)
- `crates/common/src/types.rs` — add `DiskIo` and extend `SystemReport`
- `crates/common/src/protocol.rs` — add report serde compatibility tests for `disk_io`

### New / Modified Files (Agent)
- Create: `crates/agent/src/collector/disk_io.rs` — platform-specific disk counter reader + delta calculator + device filtering
- Modify: `crates/agent/src/collector/mod.rs` — register the collector, store previous disk counters, populate `SystemReport.disk_io`
- Modify: `crates/agent/src/collector/tests.rs` — baseline / non-negative output coverage
- Modify: `crates/agent/Cargo.toml` — only if macOS/Windows support needs extra crates
- Modify: `Cargo.lock` — only if dependencies change

### New / Modified Files (Server)
- Create: `crates/server/src/migration/m20260319_000008_disk_io_records.rs` — add `disk_io_json` columns to `records` and `records_hourly`
- Modify: `crates/server/src/migration/mod.rs` — register the new migration
- Modify: `crates/server/src/entity/record.rs` — add `disk_io_json` + `utoipa::ToSchema`
- Modify: `crates/server/src/entity/record_hourly.rs` — add `disk_io_json` + `utoipa::ToSchema`
- Modify: `crates/server/src/service/record.rs` — serialize raw disk I/O, aggregate hourly per-disk averages, extend tests
- Modify: `crates/server/src/router/api/server.rs` — add response schema for `/api/servers/{id}/records`
- Modify: `crates/server/src/openapi.rs` — register record schemas so generated TS types stay in sync
- Modify: `crates/server/tests/integration.rs` — assert record API returns `disk_io_json`

### New / Modified Files (Frontend)
- Create: `apps/web/src/lib/disk-io.ts` — parse `disk_io_json`, merge totals, build chart series
- Create: `apps/web/src/lib/disk-io.test.ts` — pure helper coverage
- Create: `apps/web/src/components/server/disk-io-chart.tsx` — merged/per-disk historical chart UI
- Create: `apps/web/src/components/server/disk-io-chart.test.tsx` — chart rendering coverage
- Modify: `apps/web/src/hooks/use-api.ts` — replace the hand-written record type with the generated one
- Modify: `apps/web/src/hooks/use-api.test.tsx` — include `disk_io_json` in fixture coverage
- Modify: `apps/web/src/lib/api-schema.ts` — re-export generated server-record schema as `ServerMetricRecord`
- Modify: `apps/web/src/lib/api-types.ts` — generated file after OpenAPI refresh
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx` — fix range keys, parse disk I/O, mount the new chart component
- Modify: `apps/web/src/locales/en/servers.json` — disk I/O labels
- Modify: `apps/web/src/locales/zh/servers.json` — disk I/O labels
- No changes in `apps/web/src/hooks/use-realtime-metrics.ts` or `apps/web/src/hooks/use-servers-ws.ts` for P12 historical-only scope

### Verification / Doc Sync
- Modify: `TESTING.md` — keep counts and coverage notes aligned with new tests

---

## Chunk 1: Wire Format + Agent Collection

### Task 1: Extend Shared Report Types

**Files:**
- Modify: `crates/common/src/types.rs`
- Modify: `crates/common/src/protocol.rs`

- [ ] **Step 1: Add the shared `DiskIo` type**

In `crates/common/src/types.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct DiskIo {
    pub name: String,
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
}
```

- [ ] **Step 2: Extend `SystemReport` for backward-compatible transport**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemReport {
    // ...existing fields...
    #[serde(default)]
    pub disk_io: Option<Vec<DiskIo>>,
    pub temperature: Option<f64>,
    pub gpu: Option<GpuReport>,
}
```

- [ ] **Step 3: Add serde compatibility tests**

Cover both cases:
- `SystemReport` deserializes when `disk_io` is missing (`None`)
- `AgentMessage::Report` round-trips when `disk_io` is present

Example assertion using a full legacy payload (without `disk_io`):

```rust
let legacy = serde_json::json!({
    "cpu": 1.0,
    "mem_used": 0,
    "swap_used": 0,
    "disk_used": 0,
    "net_in_speed": 0,
    "net_out_speed": 0,
    "net_in_transfer": 0,
    "net_out_transfer": 0,
    "load1": 0.0,
    "load5": 0.0,
    "load15": 0.0,
    "tcp_conn": 0,
    "udp_conn": 0,
    "process_count": 0,
    "uptime": 0,
    "temperature": null,
    "gpu": null
});
let report: SystemReport = serde_json::from_value(legacy).unwrap();
assert!(report.disk_io.is_none());
```

- [ ] **Step 4: Verify the common crate**

Run: `cargo test -p serverbee-common`

- [ ] **Step 5: Commit**

```bash
git add crates/common/src/types.rs crates/common/src/protocol.rs
git commit -m "feat(common): add disk I/O metrics to SystemReport"
```

---

### Task 2: Implement the Agent Disk I/O Collector

**Files:**
- Create: `crates/agent/src/collector/disk_io.rs`
- Modify: `crates/agent/src/collector/mod.rs`
- Modify: `crates/agent/src/collector/tests.rs`
- Modify: `crates/agent/Cargo.toml` (only if needed)

- [ ] **Step 1: Create a collector module with platform-gated counter readers**

Structure `crates/agent/src/collector/disk_io.rs` around a small internal API:

```rust
#[derive(Clone, Debug, Default)]
pub(crate) struct DiskCounters {
    read_bytes: u64,
    write_bytes: u64,
}

pub fn collect(
    elapsed_secs: f64,
    previous: &mut std::collections::HashMap<String, DiskCounters>,
) -> Option<Vec<DiskIo>> {
    // read counters -> filter physical devices -> compute deltas -> return sorted Vec<DiskIo>
}
```

Implementation notes:
- Linux: parse `/proc/diskstats`, convert sector deltas to bytes with `* 512`
- Filter to whole physical disks only (exclude loop/dm/ram/sr devices and partitions such as `sda1` / `nvme0n1p1`, preferably by checking `/sys/block` membership)
- Sort output by disk name for deterministic tests
- macOS / Windows: keep separate `#[cfg]` readers inside this file that return `None` for now, with clear TODOs if native support is added later

- [ ] **Step 2: Wire collector state into `Collector`**

In `crates/agent/src/collector/mod.rs`, add previous disk counters and populate `SystemReport.disk_io`:

```rust
pub struct Collector {
    // ...existing fields...
    prev_disk_io: std::collections::HashMap<String, disk_io::DiskCounters>,
}

let disk_io = disk_io::collect(elapsed, &mut self.prev_disk_io);

SystemReport {
    // ...existing fields...
    disk_io,
    temperature,
    gpu: ...,
}
```

- [ ] **Step 3: Preserve the baseline semantics**

First sample rules:
- if the collector has no previous counters yet, return `Some(vec![])`
- if the platform reader is unavailable or intentionally unsupported, return `None`

That keeps new-agent “baseline not ready” distinct from “field absent / old agent”.

- [ ] **Step 4: Add targeted tests**

At minimum cover:
- first call returns empty data instead of bogus throughput
- second call never returns negative throughput
- device filtering excludes virtual disks
- output is deterministic by disk name

Prefer fixture-driven parser/helper tests (and `#[cfg(target_os = "linux")]` only where unavoidable) so `cargo test -p serverbee-agent collector` stays green on macOS/Windows even though those runtime collectors return `None` in P12.

- [ ] **Step 5: Verify the agent crate**

Run: `cargo test -p serverbee-agent collector`

- [ ] **Step 6: Commit**

```bash
git add crates/agent/src/collector crates/agent/Cargo.toml Cargo.lock
git commit -m "feat(agent): collect per-disk read and write throughput"
```

---

## Chunk 2: Persistence + API Surface

### Task 3: Add Database Columns and Entity Fields

**Files:**
- Create: `crates/server/src/migration/m20260319_000008_disk_io_records.rs`
- Modify: `crates/server/src/migration/mod.rs`
- Modify: `crates/server/src/entity/record.rs`
- Modify: `crates/server/src/entity/record_hourly.rs`

- [ ] **Step 1: Create a new up-only migration**

In `crates/server/src/migration/m20260319_000008_disk_io_records.rs`:

```rust
manager
    .alter_table(
        Table::alter()
            .table(Alias::new("records"))
            .add_column(ColumnDef::new(Alias::new("disk_io_json")).text().null())
            .to_owned(),
    )
    .await?;

manager
    .alter_table(
        Table::alter()
            .table(Alias::new("records_hourly"))
            .add_column(ColumnDef::new(Alias::new("disk_io_json")).text().null())
            .to_owned(),
    )
    .await?;
```

- [ ] **Step 2: Register the migration**

Add the new module import and `Box::new(...)` entry in `crates/server/src/migration/mod.rs` after `m20260319_000007_service_monitor`.

- [ ] **Step 3: Extend both record entities**

In `crates/server/src/entity/record.rs` and `record_hourly.rs`:

```rust
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = ServerRecord)]
pub struct Model {
    // ...existing fields...
    #[schema(value_type = String, format = DateTime)]
    pub time: DateTimeUtc,
    pub gpu_usage: Option<f64>,
    pub disk_io_json: Option<String>,
}
```

Mirror the same pattern in `record_hourly.rs` with a distinct schema alias (for example `ServerRecordHourly`) and the same explicit `#[schema(value_type = String, format = DateTime)]` annotation on `time` so generated OpenAPI names stay stable.

- [ ] **Step 4: Verify schema + entity compilation**

Run: `cargo check -p serverbee-server`

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/migration crates/server/src/entity/record.rs crates/server/src/entity/record_hourly.rs
git commit -m "feat(server): add disk_io_json columns to metric records"
```

---

### Task 4: Persist Raw Disk I/O and Aggregate Hourly Rollups

**Files:**
- Modify: `crates/server/src/service/record.rs`

- [ ] **Step 1: Serialize disk I/O when saving raw records**

In `RecordService::save_report()`:

```rust
let disk_io_json = report
    .disk_io
    .as_ref()
    .map(serde_json::to_string)
    .transpose()
    .map_err(|e| AppError::Internal(format!("disk_io serialize error: {e}")))?;

let new_record = record::ActiveModel {
    // ...existing fields...
    disk_io_json: Set(disk_io_json),
};
```

- [ ] **Step 2: Add helpers for parse / aggregate / serialize**

Keep the JSON logic inside `record.rs` helpers, for example:

```rust
fn parse_disk_io_json(raw: Option<&str>) -> Vec<DiskIo> { ... }
fn aggregate_disk_io(records: &[&record::Model]) -> Option<String> { ... }
```

Rules:
- skip malformed rows with a warning instead of failing the entire hourly job
- group by `disk.name`
- average `read_bytes_per_sec` and `write_bytes_per_sec` per disk
- preserve deterministic ordering by disk name before serializing
- if every raw row in the bucket has `disk_io_json = NULL`, store hourly `NULL`
- if at least one raw row has non-null disk I/O payload but the aggregated result is empty, store JSON `[]`

- [ ] **Step 3: Write hourly JSON into `records_hourly`**

Extend `RecordService::aggregate_hourly()` so the hourly active model also sets:

```rust
disk_io_json: Set(aggregate_disk_io(server_records)),
```

- [ ] **Step 4: Expand record service tests**

Add DB-backed assertions for:
- `save_report()` writes `disk_io_json`
- `query_history()` surfaces the field unchanged
- `aggregate_hourly()` averages per-disk read/write values correctly

- [ ] **Step 5: Verify the record service**

Run: `cargo test -p serverbee-server record`

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/service/record.rs
git commit -m "feat(server): persist and aggregate disk I/O metrics"
```

---

### Task 5: Expose the Record Schema to OpenAPI and Generated TS Types

**Files:**
- Modify: `crates/server/src/router/api/server.rs`
- Modify: `crates/server/src/openapi.rs`
- Modify: `apps/web/src/lib/api-schema.ts`
- Modify: `apps/web/src/lib/api-types.ts`
- Modify: `apps/web/src/hooks/use-api.ts`
- Modify: `apps/web/src/hooks/use-api.test.tsx`

- [ ] **Step 1: Document `/api/servers/{id}/records` with a concrete body schema**

In `crates/server/src/router/api/server.rs`, update the `#[utoipa::path]` for `get_records`:

```rust
responses(
    (status = 200, description = "Server metric records", body = Vec<crate::entity::record::Model>),
)
```

Use the raw record model as the response schema because raw and hourly payloads serialize to the same JSON shape.

- [ ] **Step 2: Register the record models in OpenAPI**

Add `crate::entity::record::Model` and `crate::entity::record_hourly::Model` to the schema list in `crates/server/src/openapi.rs` so the generator emits them reliably.

- [ ] **Step 3: Regenerate frontend API types**

Run from `apps/web`:

```bash
bun run generate:api-types
```

- [ ] **Step 4: Re-export and use the generated record type**

In `apps/web/src/lib/api-schema.ts`, re-export the generated schema for `crate::entity::record::Model` under a stable local alias such as `ServerMetricRecord`, then update `apps/web/src/hooks/use-api.ts` to remove the hand-written `ServerRecord` interface and use that alias instead.

- [ ] **Step 5: Update hook tests for the new field**

Add `disk_io_json` to the mock record payload and keep the existing query-key coverage intact.

- [ ] **Step 6: Verify server + frontend type generation**

Run:
- `cargo test -p serverbee-server --test integration test_agent_register_connect_report`
- `cd apps/web && bun run test -- src/hooks/use-api.test.tsx`

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/router/api/server.rs crates/server/src/openapi.rs apps/web/src/lib/api-schema.ts apps/web/src/lib/api-types.ts apps/web/src/hooks/use-api.ts apps/web/src/hooks/use-api.test.tsx
git commit -m "feat(api): document server metric records with disk I/O schema"
```

---

## Chunk 3: Frontend Charts + Final Verification

### Task 6: Build Disk I/O Parsing Helpers and Chart UI

**Files:**
- Create: `apps/web/src/lib/disk-io.ts`
- Create: `apps/web/src/lib/disk-io.test.ts`
- Create: `apps/web/src/components/server/disk-io-chart.tsx`
- Create: `apps/web/src/components/server/disk-io-chart.test.tsx`

- [ ] **Step 1: Create a pure helper for parsing and chart shaping**

In `apps/web/src/lib/disk-io.ts`, add functions like:

```ts
export interface DiskIoSample {
  name: string
  read_bytes_per_sec: number
  write_bytes_per_sec: number
}

export function parseDiskIoJson(raw: string | null | undefined): DiskIoSample[] { ... }
export function buildMergedDiskIoSeries(records: ServerMetricRecord[]): Record<string, unknown>[] { ... }
export function buildPerDiskSeries(records: ServerMetricRecord[]): Array<{ disk: string; points: Record<string, unknown>[] }> { ... }
```

Rules:
- invalid JSON returns an empty array
- merged view sums all disks per timestamp
- per-disk view preserves one card per disk

- [ ] **Step 2: Add helper tests before the chart component**

Cover:
- `null` / invalid JSON handling
- merged totals across multiple disks
- missing disks on some timestamps
- stable disk ordering

- [ ] **Step 3: Build a dedicated chart component**

In `apps/web/src/components/server/disk-io-chart.tsx`:
- use a card-like wrapper consistent with `MetricsChart` / `TrafficCard`
- add tabs or a small toggle for `Merged` vs `Per Disk`
- use `formatSpeed()` for Y-axis and tooltip formatting
- render read/write as two series per chart
- return `null` when there is no historical disk I/O data

Prefer a dedicated component instead of expanding `MetricsChart`, which is single-series today.

- [ ] **Step 4: Add component coverage**

Verify:
- merged view renders two series
- per-disk view renders one chart group per disk
- empty data path returns `null`

- [ ] **Step 5: Verify focused frontend tests**

Run:

```bash
cd apps/web && bun run test -- src/lib/disk-io.test.ts src/components/server/disk-io-chart.test.tsx
```

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/lib/disk-io.ts apps/web/src/lib/disk-io.test.ts apps/web/src/components/server/disk-io-chart.tsx apps/web/src/components/server/disk-io-chart.test.tsx
git commit -m "feat(web): add disk I/O chart helpers and component"
```

---

### Task 7: Integrate the Chart into the Server Detail Page

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`
- Modify: `apps/web/src/locales/en/servers.json`
- Modify: `apps/web/src/locales/zh/servers.json`

- [ ] **Step 1: Fix the range key bug while updating the page state**

Change the search param from ambiguous interval values to stable keys:

```ts
const TIME_RANGES = [
  { key: 'realtime', label: 'range_realtime', hours: 0, interval: 'realtime' },
  { key: '1h', label: 'range_1h', hours: 1, interval: 'raw' },
  { key: '6h', label: 'range_6h', hours: 6, interval: 'raw' },
  { key: '24h', label: 'range_24h', hours: 24, interval: 'raw' },
  { key: '7d', label: 'range_7d', hours: 168, interval: 'hourly' },
  { key: '30d', label: 'range_30d', hours: 720, interval: 'hourly' },
]
```

Update `validateSearch`, the selected-range lookup, and the range buttons to store `key` instead of `interval`.

- [ ] **Step 2: Build disk I/O datasets from record history**

Use the new helper functions in `apps/web/src/routes/_authed/servers/$id.tsx`:
- keep P12 historical-only (`isRealtime` => do not render the disk I/O section)
- derive merged and per-disk datasets from `records`
- mount `<DiskIoChart />` below the load/network charts and above `<TrafficCard />`

- [ ] **Step 3: Add i18n strings**

Add keys to both locale files:
- `chart_disk_io`
- `disk_io_merged`
- `disk_io_per_disk`
- `disk_io_read`
- `disk_io_write`

- [ ] **Step 4: Re-run focused page-adjacent tests**

Run:

```bash
cd apps/web && bun run test -- src/hooks/use-api.test.tsx src/lib/disk-io.test.ts src/components/server/disk-io-chart.test.tsx
cd apps/web && bun run typecheck
cd apps/web && bun x ultracite check
```

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed/servers/$id.tsx apps/web/src/locales/en/servers.json apps/web/src/locales/zh/servers.json
git commit -m "feat(web): surface historical disk I/O on server detail page"
```

---

### Task 8: End-to-End Verification and Test Guide Sync

**Files:**
- Modify: `crates/server/tests/integration.rs`
- Modify: `TESTING.md`

- [ ] **Step 1: Extend the integration flow with disk I/O data**

In `crates/server/tests/integration.rs`:
- send `disk_io` inside the WS `report` payload
- persist a raw record deterministically before the API assertion (either by calling `RecordService::save_report()` against the temp SQLite DB or by exposing a one-shot helper; do not rely on `record_writer`, because background tasks are not started in `start_test_server()`)
- query `/api/servers/{id}/records?from=...&to=...&interval=raw`
- assert the first record includes a non-null `disk_io_json` string and the expected disk names

- [ ] **Step 2: Update `TESTING.md` counts and coverage notes**

Keep the repo’s test-guide contract in sync with:
- new common tests
- new agent collector tests
- new server record tests / integration assertion
- new frontend helper/component tests

- [ ] **Step 3: Run the full validator set**

Run:

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cd apps/web && bun run test
cd apps/web && bun run typecheck
cd apps/web && bun x ultracite check
```

- [ ] **Step 4: Commit**

```bash
git add crates/server/tests/integration.rs TESTING.md
git commit -m "test: cover disk I/O metric flow end to end"
```

---

Plan complete and saved to `docs/superpowers/plans/2026-03-19-p12-disk-io-monitoring.md`. Ready to execute.
