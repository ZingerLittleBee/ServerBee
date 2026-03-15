# Network Quality Monitoring Design

## Overview

Add a network quality monitoring subsystem to ServerBee. Each VPS can probe multiple ISP/cloud provider targets (e.g. Shanghai Telecom, Beijing Unicom, Cloudflare) on a configurable interval, collecting latency statistics (avg/min/max) and packet loss rate. Results are displayed in a dedicated frontend section with an overview page and per-VPS detail page featuring multi-line latency charts.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Integration strategy | Independent subsystem + shared probe utils | Clean module boundaries, no impact on existing ping system, reuse ICMP/TCP/HTTP probe logic |
| ISP target organization | Provider x City | Most common in VPS monitoring tools (Nezha-style), users care about specific ISP + region combinations |
| Probing method | Agent-side batch probing | Agent sends N packets per round via `ping -c N`, computes aggregated stats locally, reports single result. Reduces WS traffic and server computation |
| Configuration model | Two-level (global defaults + per-VPS override) | Global defaults for convenience, per-VPS override for flexibility |
| Probe frequency | User-configurable (default 60s interval, 10 packets/round) | Different users have different real-time and accuracy needs |
| UI layout | Card list overview + multi-line chart detail page | Overview for quick scanning, detail page inspired by Nezha-style network monitoring |

## Data Model

### `network_probe_target` — Probe Targets (builtin + custom)

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT PK | UUID |
| name | TEXT NOT NULL | Display name, e.g. "Shanghai Telecom" |
| provider | TEXT NOT NULL | ISP/provider, e.g. "Telecom", "Unicom", "Mobile", "Cloudflare" |
| location | TEXT NOT NULL | Region, e.g. "Shanghai", "Beijing", "US" |
| target | TEXT NOT NULL | Probe address: IP for ICMP, `host:port` for TCP, full URL for HTTP |
| probe_type | TEXT NOT NULL | "icmp" / "tcp" / "http" |
| is_builtin | BOOLEAN NOT NULL DEFAULT false | true = builtin (not deletable), false = user-created |
| created_at | DATETIME NOT NULL | |
| updated_at | DATETIME NOT NULL | |

### `network_probe_config` — Per-VPS Probe Configuration

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT PK | UUID |
| server_id | TEXT NOT NULL FK → servers | |
| target_id | TEXT NOT NULL FK → network_probe_target | |
| created_at | DATETIME NOT NULL | |

Unique constraint: `(server_id, target_id)`. Row existence means enabled; `set_server_targets` is a full replacement (delete all + insert new).

Foreign key `server_id` uses `on_delete(Cascade)` — when a server is deleted, all its `network_probe_config` rows are automatically removed (consistent with `server_tags` FK behavior).

Max 20 targets per VPS to prevent configuration amplification.

### Global Probe Settings — stored in `config` table

Use existing `ConfigService::get_typed/set_typed` with key `"network_probe_setting"`. No dedicated table.

```rust
struct NetworkProbeSetting {
    interval: u32,              // Probe interval in seconds (30-600), default 60
    packet_count: u32,          // Packets per round (5-20), default 10
    default_target_ids: Vec<String>,  // Target IDs auto-assigned to new VPS
}
```

### `network_probe_record` — Probe Records (per-round aggregated)

| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | rowid (auto-increment) |
| server_id | TEXT NOT NULL FK → servers | |
| target_id | TEXT NOT NULL FK → network_probe_target | |
| avg_latency | REAL NULL | Average latency in ms. NULL when packet_received == 0 (100% loss) |
| min_latency | REAL NULL | Minimum latency in ms. NULL when packet_received == 0 |
| max_latency | REAL NULL | Maximum latency in ms. NULL when packet_received == 0 |
| packet_loss | REAL NOT NULL | Packet loss rate 0.0-1.0 |
| packet_sent | INTEGER NOT NULL | Packets sent |
| packet_received | INTEGER NOT NULL | Packets received |
| timestamp | DATETIME NOT NULL | Probe time (also used as retention cutoff reference) |

Index: `(server_id, target_id, timestamp)`

Foreign key `server_id` uses `on_delete(Cascade)` — server deletion cascades to its probe records.

Note: for TCP/HTTP probes, `packet_sent`/`packet_received`/`packet_loss` represent probe attempt counts, not network packets. This is a common convention in monitoring tools.

### `network_probe_record_hourly` — Hourly Aggregation

| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | rowid (auto-increment) |
| server_id | TEXT NOT NULL FK → servers | |
| target_id | TEXT NOT NULL FK → network_probe_target | |
| avg_latency | REAL NULL | Hourly average latency. NULL if all samples had 100% loss |
| min_latency | REAL NULL | Hourly minimum latency |
| max_latency | REAL NULL | Hourly maximum latency |
| avg_packet_loss | REAL NOT NULL | Hourly average packet loss |
| sample_count | INTEGER NOT NULL | Number of raw records aggregated |
| hour | DATETIME NOT NULL | Hour timestamp |

Unique constraint: `(server_id, target_id, hour)` — ensures idempotent aggregation.

Foreign key `server_id` uses `on_delete(Cascade)` — server deletion cascades to its hourly records.

Aggregation note: `AVG(avg_latency)` naturally ignores NULL rows (SQL standard behavior), so hourly averages only reflect rounds where at least one packet was received. If all raw records in an hour have NULL latency (100% loss throughout), the hourly `avg_latency` is also NULL.

### Builtin Probe Targets (seed data)

```
China Telecom:  Shanghai (61.129.2.3), Beijing (106.37.67.29), Guangzhou (14.215.116.1)
China Unicom:   Shanghai (210.22.84.3), Beijing (202.106.50.1), Guangzhou (221.5.88.88)
China Mobile:   Shanghai (117.131.19.23), Beijing (221.179.155.161), Guangzhou (120.196.165.24)
International:  Cloudflare (1.1.1.1), Google (8.8.8.8), AWS Tokyo (13.112.63.251)
```

All builtin targets use `probe_type: "icmp"`, `is_builtin: true`.

## Protocol Messages

### Common DTOs (`crates/common/src/types.rs`)

```rust
pub struct NetworkProbeTarget {
    pub target_id: String,
    pub name: String,
    pub target: String,       // IP for ICMP, host:port for TCP, URL for HTTP
    pub probe_type: String,   // "icmp" / "tcp" / "http"
}

pub struct NetworkProbeResultData {
    pub target_id: String,
    pub avg_latency: Option<f64>,  // None when packet_received == 0
    pub min_latency: Option<f64>,
    pub max_latency: Option<f64>,
    pub packet_loss: f64,
    pub packet_sent: u32,
    pub packet_received: u32,
    pub timestamp: DateTime<Utc>,
}

/// Anomaly info returned in summary/overview APIs
pub struct NetworkProbeAnomaly {
    pub timestamp: DateTime<Utc>,
    pub target_id: String,
    pub target_name: String,
    pub anomaly_type: String,  // "high_latency" / "very_high_latency" / "high_packet_loss" / "very_high_packet_loss" / "unreachable"
    pub value: f64,
}
```

Note: `NetworkProbeTarget` is the agent-facing wire type (minimal fields for probing). REST API responses use a server-side DTO that includes all DB fields (`provider`, `location`, `is_builtin`, `created_at`, `updated_at`).

### ServerMessage (Server → Agent)

New variant:

```rust
NetworkProbeSync {
    targets: Vec<NetworkProbeTarget>,
    interval: u32,
    packet_count: u32,
}
```

Sent on: agent connect (after Welcome), config change (settings update or per-VPS target change). An empty `targets` array signals the agent to stop all network probe tasks (same semantics as `PingTasksSync` with empty tasks).

### AgentMessage (Agent → Server)

New variant:

```rust
NetworkProbeResults {
    results: Vec<NetworkProbeResultData>,
}
```

Sent once per probe round, batching all target results into a single message. Uses named-field style consistent with `PingTasksSync { tasks }` and `BrowserMessage::NetworkProbeUpdate`.

### BrowserMessage (Server → Browser)

New variant:

```rust
NetworkProbeUpdate {
    server_id: String,
    results: Vec<NetworkProbeResultData>,
}
```

Broadcast when probe results arrive from an agent.

## Agent Implementation

### Shared Probe Utils (`crates/agent/src/probe_utils.rs`)

Extract existing probe logic from `pinger.rs` into standalone functions:

```rust
pub struct ProbeResult {
    pub success: bool,
    pub latency_ms: f64,
    pub error: Option<String>,
}

/// Single-packet probe (used by existing PingManager)
pub async fn probe_icmp(host: &str, timeout: Duration) -> ProbeResult;
pub async fn probe_tcp(host: &str, port: u16, timeout: Duration) -> ProbeResult;
pub async fn probe_http(url: &str, timeout: Duration) -> ProbeResult;

/// Batch ICMP probe using `ping -c N`. Returns per-packet results parsed from
/// the summary line (min/avg/max/mdev) and packet loss from statistics line.
/// Much more efficient than forking N separate processes.
pub struct BatchIcmpResult {
    pub avg_latency: Option<f64>,  // None if 100% loss
    pub min_latency: Option<f64>,
    pub max_latency: Option<f64>,
    pub packet_loss: f64,
    pub packet_sent: u32,
    pub packet_received: u32,
}

pub async fn probe_icmp_batch(host: &str, count: u32, timeout: Duration) -> BatchIcmpResult;
```

Refactor existing `pinger.rs` to call the single-packet `probe_icmp`/`probe_tcp`/`probe_http` functions. No behavior change.

For network quality monitoring:
- ICMP: use `probe_icmp_batch` (runs `ping -c N` once, parses summary line for min/avg/max/loss)
- TCP/HTTP: run `probe_tcp`/`probe_http` N times sequentially, compute stats from individual results

### NetworkProber (`crates/agent/src/network_prober.rs`)

**Responsibilities:**
- Manage concurrent probe tasks per target (HashMap of JoinHandle, same pattern as PingManager)
- `sync(targets, interval, packet_count, capabilities)` — reconcile running tasks with server config; filter targets by capability (see Capability Gating below)
- `stop_all()` — abort all tasks on shutdown

**Per-target task loop:**
1. On first iteration, add random initial jitter (0 to `interval` seconds) to prevent all agents from probing simultaneously after receiving `NetworkProbeSync`
2. Wait `interval` seconds (using `tokio::time::interval` with `MissedTickBehavior::Skip` to prevent round overlap when probes exceed interval duration)
3. For ICMP: call `probe_icmp_batch(host, packet_count)`. For TCP/HTTP: run `probe_tcp`/`probe_http` sequentially `packet_count` times, compute avg/min/max/loss
4. When `packet_received == 0`, set avg/min/max_latency to `None`
5. Send `NetworkProbeResultData` via mpsc channel to Reporter

All targets run their probe rounds independently. The Reporter collects results arriving within a short time window and batches them into a single `AgentMessage::NetworkProbeResults(Vec<...>)` message.

**Capability Gating:** Network probes reuse existing capability bits. `sync()` filters targets by `probe_type`:
- `"icmp"` → requires `CAP_PING_ICMP`
- `"tcp"` → requires `CAP_PING_TCP`
- `"http"` → requires `CAP_PING_HTTP`

Targets whose capability is disabled are silently skipped (not started). When capabilities change via `CapabilitiesSync`, the prober re-syncs to start/stop affected targets.

### Reporter Integration (`crates/agent/src/reporter.rs`)

Add 6th channel to `tokio::select!` loop:
- Receive `NetworkProbeResultData` from network_prober → collect into batch → serialize as `AgentMessage::NetworkProbeResults` → send to WebSocket
- Handle `ServerMessage::NetworkProbeSync` → call `network_prober.sync()`
- Handle `ServerMessage::CapabilitiesSync` → also re-sync network_prober with updated capabilities

## Server Implementation

### NetworkProbeService (`crates/server/src/service/network_probe.rs`)

**Target management:**
- `list_targets() -> Vec<Target>` — all targets (builtin + custom)
- `create_target(input) -> Target` — create custom target
- `update_target(id, input) -> Target` — update custom target (builtin targets immutable)
- `delete_target(id)` — delete custom target (cascade delete config + records). After deletion, push updated `NetworkProbeSync` to all online agents that had this target assigned. Also remove the target ID from `default_target_ids` in the global setting to prevent dangling references.

**Configuration:**
- `get_setting() -> NetworkProbeSetting` — read from `ConfigService::get_typed("network_probe_setting")`, return defaults if not set
- `update_setting(input)` — write via `ConfigService::set_typed`; push `NetworkProbeSync` to all online agents
- `get_server_targets(server_id) -> Vec<Target>` — targets assigned to a VPS
- `set_server_targets(server_id, target_ids)` — replace VPS target assignments (max 20). Target assignments are always persisted to `network_probe_config` regardless of agent online status. If the agent is online, push `NetworkProbeSync` immediately; if offline, the agent receives correct config on next connection (via the Welcome flow).
- `apply_defaults(server_id)` — assign default targets to newly registered VPS

**Records:**
- `save_results(server_id, results: Vec<NetworkProbeResultData>)` — insert all results in a single DB transaction for efficiency
- `query_records(server_id, target_id?, from, to) -> Vec<Record>` — smart interval selection: < 1 day raw, 1-30 days hourly, > 30 days hourly
- `get_server_summary(server_id) -> Summary` — current latency, packet loss, availability per target; includes `last_probe_at` timestamp to distinguish stale data from live data
- `get_overview() -> Vec<ServerNetworkSummary>` — all VPS network summaries for overview page; includes `last_probe_at` per VPS
- `get_anomalies(server_id, from, to) -> Vec<Anomaly>` — targets with high latency (> 200ms) or high packet loss (> 10%) in the given time range

**Aggregation (called by background tasks):**
- `aggregate_hourly()` — raw records → hourly aggregates (idempotent via unique constraint on `(server_id, target_id, hour)`)
- `cleanup_old_records()` — delete records older than retention period

### API Routes (`crates/server/src/router/api/network_probe.rs`)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/network-probes/targets` | read | List all probe targets |
| POST | `/api/network-probes/targets` | admin | Create custom target |
| PUT | `/api/network-probes/targets/{id}` | admin | Update target |
| DELETE | `/api/network-probes/targets/{id}` | admin | Delete target |
| GET | `/api/network-probes/setting` | read | Get global settings |
| PUT | `/api/network-probes/setting` | admin | Update global settings |
| GET | `/api/network-probes/overview` | read | All VPS network overview |
| GET | `/api/servers/{id}/network-probes/targets` | read | Get VPS probe targets |
| PUT | `/api/servers/{id}/network-probes/targets` | admin | Set VPS probe targets |
| GET | `/api/servers/{id}/network-probes/records` | read | Query probe records (params: target_id?, from, to) |
| GET | `/api/servers/{id}/network-probes/summary` | read | VPS network summary |
| GET | `/api/servers/{id}/network-probes/anomalies` | read | VPS anomalies (params: from, to) |

Per-server endpoints are nested under `/api/servers/{id}/` consistent with existing routes (`/api/servers/{id}/records`, `/api/servers/{id}/gpu-records`). Global resources (targets, setting, overview) remain under `/api/network-probes/`.

### WebSocket Integration

**Agent WS handler (`ws/agent.rs`):**
- After Welcome + existing sync messages, also send `NetworkProbeSync` with the agent's configured targets
- On `AgentMessage::NetworkProbeResults`: save all results to DB in a single transaction, broadcast `BrowserMessage::NetworkProbeUpdate` with the batch

**Browser WS handler (`ws/browser.rs`):**
- `NetworkProbeUpdate` flows through existing `broadcast::Sender<BrowserMessage>` — no handler changes needed

### Background Task Extensions

- `aggregator`: add call to `network_probe_service.aggregate_hourly()`
- `cleanup`: add call to `network_probe_service.cleanup_old_records()`

### Agent Registration Hook

In `POST /api/agent/register` handler, after creating the server record, call `network_probe_service.apply_defaults(server_id)` to assign global default targets.

## Alert Integration

Extend `AlertService.evaluate_all()` with two new metric types:

- `network_latency` — triggers when a VPS's average latency to a target exceeds threshold (e.g. > 200ms for 70% of samples in 10-minute window)
- `network_packet_loss` — triggers when packet loss exceeds threshold (e.g. > 10%)

These reuse the existing alert rule model (`alert_rule` table with `rules_json` containing `Vec<AlertRuleItem>`) and notification dispatch. No new tables needed. Two new `rule_type` values (`network_latency`, `network_packet_loss`) are added to `AlertRuleItem`. The `check_server` function in `AlertService` gets new match arms that query `network_probe_record` for recent samples instead of the `record` table. Alert rules apply per-server (across all targets for that server); the highest latency / worst packet loss among all targets is used for threshold comparison.

## Frontend

### New Routes

| Path | File | Description |
|------|------|-------------|
| `/network` | `_authed/network/index.tsx` | Network quality overview |
| `/network/$serverId` | `_authed/network/$serverId.tsx` | VPS network quality detail |
| `/settings/network-probes` | `_authed/settings/network-probes.tsx` | Global probe settings (targets + config) |

Sidebar: add "Network" nav item below "Servers". Settings: add "Network Probes" item.

### Overview Page (`/network`)

- **Top stats bar**: total VPS count, online count, anomaly count (VPS with high latency or high packet loss)
- **Anomaly banner**: prominent alert if anomalies exist (e.g. "3 VPS have latency > 200ms to Shanghai Telecom")
- **VPS card list** (searchable, filterable by group):
  - Each card: VPS name, online status, average latency, availability %, target count, worst-line summary
  - Click → navigate to `/network/{serverId}`
- **Data source**: `GET /api/network-probes/overview` + WebSocket `NetworkProbeUpdate` for real-time

### Detail Page (`/network/$serverId`)

Inspired by the user's reference screenshot (Nezha-style):

- **Header**: VPS name + online status badge + time range selector (realtime / 1h / 6h / 24h / 7d / 30d)
- **VPS info bar**: IPv4, IPv6, region, country, virtualization type (from existing `servers` table)
- **Target cards row**: one card per probe target showing:
  - Target name (color-coded label matching chart line)
  - Current average latency (or "N/A" when NULL / 100% loss)
  - Current packet loss rate
  - Eye icon toggle to show/hide line on chart
  - Cards grouped by provider
- **Multi-line area chart** (Recharts):
  - X-axis: time, Y-axis: latency (ms)
  - One line per target, colors match card labels
  - Data points with NULL latency (100% loss) render as gaps in the line
  - Hover tooltip: timestamp + each target's latency value
  - Realtime mode: data pushed from WebSocket into ring buffer (reuse `use-realtime-metrics` pattern, 200-point buffer)
  - Historical mode: data from `GET /api/servers/{id}/network-probes/records`
- **Bottom stats bar**: overall average latency | availability % (1 - avg packet loss) | target count n/n
- **Anomaly summary table** (below chart):
  - Lists anomalous periods in selected time range
  - Columns: time | target | type (high latency / high packet loss / unreachable) | value
  - Data from `GET /api/servers/{id}/network-probes/anomalies`
- **Admin actions**: "Manage Targets" button → dialog to add/remove targets for this VPS

### Settings Page (`/settings/network-probes`)

Two tabs:

**Tab 1: Target Management**
- Table of all targets (builtin marked with lock icon, custom editable/deletable)
- "Add Target" button → dialog: name, provider, location, target address, probe_type
- Builtin targets are read-only

**Tab 2: Global Settings**
- Probe interval: number input (30-600s, default 60)
- Packets per round: number input (5-20, default 10)
- Default targets: multi-select checkbox list of targets auto-assigned to new VPS

### New Hooks

**`use-network-probe-ws.ts`**:
- Listen for `NetworkProbeUpdate` messages from existing browser WebSocket
- Update React Query cache for affected server's network data
- Maintain realtime ring buffer per server (same pattern as `use-realtime-metrics`)

**API hooks (extend `use-api.ts`)**:
- `useNetworkOverview()` — overview data
- `useNetworkServerSummary(serverId)` — VPS network summary
- `useNetworkRecords(serverId, targetId?, from, to)` — historical records
- `useNetworkTargets()` — target list
- `useNetworkSetting()` — global settings
- Mutation hooks for CRUD operations

### i18n

Add `network` namespace to translation files (`locales/en/network.json`, `locales/zh/network.json`) covering:
- Page titles, stat labels, anomaly messages, form labels, tooltip text

## Anomaly Detection

Anomalies are computed dynamically (no separate storage):

| Anomaly Type | Condition | Severity |
|--------------|-----------|----------|
| High Latency | avg_latency > 200ms | warning |
| Very High Latency | avg_latency > 500ms | critical |
| High Packet Loss | packet_loss > 0.1 (10%) | warning |
| Very High Packet Loss | packet_loss > 0.5 (50%) | critical |
| Unreachable | packet_loss = 1.0 | critical |

Thresholds are hardcoded constants (not user-configurable) to keep complexity low. If needed later, they can be moved to the global probe setting.

The overview API and summary API include anomaly flags in their responses. The frontend uses these to render anomaly banners and badges.

## Data Retention

Add two new fields to `RetentionConfig` in `crates/server/src/config.rs` (consistent with existing retention configuration pattern):

```rust
// In RetentionConfig
pub network_probe_days: u32,         // default 7
pub network_probe_hourly_days: u32,  // default 90
```

Configurable via env vars: `SERVERBEE_RETENTION__NETWORK_PROBE_DAYS`, `SERVERBEE_RETENTION__NETWORK_PROBE_HOURLY_DAYS`.

Cleanup runs as part of existing `cleanup` background task.

## Data Export

Detail page supports CSV export of current time range data (frontend-only, generated in browser via Blob). No backend API needed.

## Not In Scope (YAGNI)

- Traceroute / MTR — high complexity and agent resource cost
- Cross-target comparison analytics
- Persistent anomaly event log table — dynamic computation is sufficient
- Target health checking (validating if builtin IPs are still responsive)
- Configurable anomaly thresholds — hardcoded is sufficient for MVP
