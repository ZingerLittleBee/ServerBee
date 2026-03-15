# Network Quality Monitoring Design

## Overview

Add a network quality monitoring subsystem to ServerBee. Each VPS can probe multiple ISP/cloud provider targets (e.g. Shanghai Telecom, Beijing Unicom, Cloudflare) on a configurable interval, collecting latency statistics (avg/min/max) and packet loss rate. Results are displayed in a dedicated frontend section with an overview page and per-VPS detail page featuring multi-line latency charts.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Integration strategy | Independent subsystem + shared probe utils | Clean module boundaries, no impact on existing ping system, reuse ICMP/TCP/HTTP probe logic |
| ISP target organization | Provider x City | Most common in VPS monitoring tools (Nezha-style), users care about specific ISP + region combinations |
| Probing method | Agent-side batch probing | Agent sends N packets per round, computes aggregated stats locally, reports single result. Reduces WS traffic and server computation |
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
| host | TEXT NOT NULL | Probe address (IP or domain) |
| probe_type | TEXT NOT NULL | "icmp" / "tcp" / "http" |
| port | INTEGER NULL | Port for TCP/HTTP probes |
| is_builtin | BOOLEAN NOT NULL DEFAULT false | true = builtin (not deletable), false = user-created |
| created_at | DATETIME NOT NULL | |

### `network_probe_config` — Per-VPS Probe Configuration

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT PK | UUID |
| server_id | TEXT NOT NULL FK → servers | |
| target_id | TEXT NOT NULL FK → network_probe_target | |
| enabled | BOOLEAN NOT NULL DEFAULT true | |
| created_at | DATETIME NOT NULL | |

Unique constraint: `(server_id, target_id)`

### `network_probe_setting` — Global Probe Settings

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT PK | |
| interval | INTEGER NOT NULL DEFAULT 60 | Probe interval in seconds (30-600) |
| packet_count | INTEGER NOT NULL DEFAULT 10 | Packets per round (5-20) |
| default_target_ids | TEXT NOT NULL DEFAULT '[]' | JSON array of target IDs auto-assigned to new VPS |
| created_at | DATETIME NOT NULL | |
| updated_at | DATETIME NOT NULL | |

### `network_probe_record` — Probe Records (per-round aggregated)

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT PK | UUID |
| server_id | TEXT NOT NULL FK → servers | |
| target_id | TEXT NOT NULL FK → network_probe_target | |
| avg_latency | REAL NOT NULL | Average latency in ms |
| min_latency | REAL NOT NULL | Minimum latency in ms |
| max_latency | REAL NOT NULL | Maximum latency in ms |
| packet_loss | REAL NOT NULL | Packet loss rate 0.0-1.0 |
| packet_sent | INTEGER NOT NULL | Packets sent |
| packet_received | INTEGER NOT NULL | Packets received |
| timestamp | DATETIME NOT NULL | Probe time |

Index: `(server_id, target_id, timestamp)`

### `network_probe_record_hourly` — Hourly Aggregation

| Column | Type | Description |
|--------|------|-------------|
| id | TEXT PK | |
| server_id | TEXT NOT NULL FK → servers | |
| target_id | TEXT NOT NULL FK → network_probe_target | |
| avg_latency | REAL NOT NULL | Hourly average latency |
| min_latency | REAL NOT NULL | Hourly minimum latency |
| max_latency | REAL NOT NULL | Hourly maximum latency |
| avg_packet_loss | REAL NOT NULL | Hourly average packet loss |
| sample_count | INTEGER NOT NULL | Number of raw records aggregated |
| hour | DATETIME NOT NULL | Hour timestamp |

Index: `(server_id, target_id, hour)`

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
    pub host: String,
    pub probe_type: String,  // "icmp" / "tcp" / "http"
    pub port: Option<u16>,
}

pub struct NetworkProbeResultData {
    pub target_id: String,
    pub avg_latency: f64,
    pub min_latency: f64,
    pub max_latency: f64,
    pub packet_loss: f64,
    pub packet_sent: u32,
    pub packet_received: u32,
    pub timestamp: DateTime<Utc>,
}
```

### ServerMessage (Server → Agent)

New variant:

```rust
NetworkProbeSync {
    targets: Vec<NetworkProbeTarget>,
    interval: u32,
    packet_count: u32,
}
```

Sent on: agent connect (after Welcome), config change (settings update or per-VPS target change).

### AgentMessage (Agent → Server)

New variant:

```rust
NetworkProbeResult(NetworkProbeResultData)
```

Sent after each probe round completes for each target.

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

pub async fn probe_icmp(host: &str, timeout: Duration) -> ProbeResult;
pub async fn probe_tcp(host: &str, port: u16, timeout: Duration) -> ProbeResult;
pub async fn probe_http(url: &str, timeout: Duration) -> ProbeResult;
```

Refactor existing `pinger.rs` to call these shared functions. No behavior change.

### NetworkProber (`crates/agent/src/network_prober.rs`)

**Responsibilities:**
- Manage concurrent probe tasks per target (HashMap of JoinHandle, same pattern as PingManager)
- `sync(targets, interval, packet_count)` — reconcile running tasks with server config
- `stop_all()` — abort all tasks on shutdown

**Per-target task loop:**
1. Wait `interval` seconds
2. Send `packet_count` probes sequentially (calling `probe_utils` functions)
3. Compute aggregated stats: avg/min/max latency, packet_loss = 1 - (received / sent)
4. Send `NetworkProbeResultData` via mpsc channel to Reporter

**No capability gating needed** — network probing is outbound-only with no security implications.

### Reporter Integration (`crates/agent/src/reporter.rs`)

Add 6th channel to `tokio::select!` loop:
- Receive `NetworkProbeResultData` from network_prober → serialize as `AgentMessage::NetworkProbeResult` → send to WebSocket
- Handle `ServerMessage::NetworkProbeSync` → call `network_prober.sync()`

## Server Implementation

### NetworkProbeService (`crates/server/src/service/network_probe.rs`)

**Target management:**
- `list_targets() -> Vec<Target>` — all targets (builtin + custom)
- `create_target(input) -> Target` — create custom target
- `update_target(id, input) -> Target` — update custom target (builtin targets immutable)
- `delete_target(id)` — delete custom target (cascade delete config + records)

**Configuration:**
- `get_setting() -> Setting` — global probe settings
- `update_setting(input)` — update settings; push `NetworkProbeSync` to all online agents
- `get_server_targets(server_id) -> Vec<Target>` — targets assigned to a VPS
- `set_server_targets(server_id, target_ids)` — replace VPS target assignments; push `NetworkProbeSync` to agent if online
- `apply_defaults(server_id)` — assign default targets to newly registered VPS

**Records:**
- `save_result(server_id, result)` — insert into `network_probe_record`
- `query_records(server_id, target_id?, time_range) -> Vec<Record>` — smart interval selection: < 1 day raw, 1-30 days hourly, > 30 days hourly
- `get_server_summary(server_id) -> Summary` — current latency, packet loss, availability per target
- `get_overview() -> Vec<ServerNetworkSummary>` — all VPS network summaries for overview page
- `get_anomalies(server_id) -> Vec<Anomaly>` — targets with high latency (> 200ms) or high packet loss (> 10%)

**Aggregation (called by background tasks):**
- `aggregate_hourly()` — raw records → hourly aggregates
- `cleanup_old_records()` — delete records older than retention period

### API Routes (`crates/server/src/router/api/network_probe.rs`)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/network-probes/targets` | read | List all probe targets |
| POST | `/api/network-probes/targets` | admin | Create custom target |
| PUT | `/api/network-probes/targets/{id}` | admin | Update target |
| DELETE | `/api/network-probes/targets/{id}` | admin | Delete target |
| GET | `/api/network-probes/setting` | admin | Get global settings |
| PUT | `/api/network-probes/setting` | admin | Update global settings |
| GET | `/api/network-probes/servers/{id}/targets` | read | Get VPS probe targets |
| PUT | `/api/network-probes/servers/{id}/targets` | admin | Set VPS probe targets |
| GET | `/api/network-probes/servers/{id}/records` | read | Query probe records (params: target_id?, hours) |
| GET | `/api/network-probes/servers/{id}/summary` | read | VPS network summary |
| GET | `/api/network-probes/overview` | read | All VPS network overview |

### WebSocket Integration

**Agent WS handler (`ws/agent.rs`):**
- After Welcome + existing sync messages, also send `NetworkProbeSync` with the agent's configured targets
- On `AgentMessage::NetworkProbeResult`: save to DB, broadcast `BrowserMessage::NetworkProbeUpdate`

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

These reuse existing alert rule model (`alert_rule` table) and notification dispatch. No new tables needed. The `alert_rule.metric` field accepts the new metric type strings.

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
  - Current average latency
  - Current packet loss rate
  - Eye icon toggle to show/hide line on chart
  - Cards grouped by provider
- **Multi-line area chart** (Recharts):
  - X-axis: time, Y-axis: latency (ms)
  - One line per target, colors match card labels
  - Hover tooltip: timestamp + each target's latency value
  - Realtime mode: data pushed from WebSocket into ring buffer (reuse `use-realtime-metrics` pattern, 200-point buffer)
  - Historical mode: data from `GET /api/network-probes/servers/{id}/records`
- **Bottom stats bar**: overall average latency | availability % (1 - avg packet loss) | target count n/n
- **Anomaly summary table** (below chart):
  - Lists anomalous periods in selected time range
  - Columns: time | target | type (high latency / high packet loss / unreachable) | value
- **Admin actions**: "Manage Targets" button → dialog to add/remove targets for this VPS

### Settings Page (`/settings/network-probes`)

Two tabs:

**Tab 1: Target Management**
- Table of all targets (builtin marked with lock icon, custom editable/deletable)
- "Add Target" button → dialog: name, provider, location, host, probe_type, port
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
- `useNetworkRecords(serverId, targetId?, timeRange)` — historical records
- `useNetworkTargets()` — target list
- `useNetworkSetting()` — global settings
- Mutation hooks for CRUD operations

### i18n

Add `network` namespace to translation files (`locales/en/network.json`, `locales/cn/network.json`) covering:
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

Thresholds are hardcoded constants (not user-configurable) to keep complexity low. If needed later, they can be moved to `network_probe_setting`.

The overview API and summary API include anomaly flags in their responses. The frontend uses these to render anomaly banners and badges.

## Data Retention

New constants in `crates/common/src/constants.rs`:

```rust
pub const NETWORK_PROBE_RETENTION_DAYS: u32 = 7;
pub const NETWORK_PROBE_HOURLY_RETENTION_DAYS: u32 = 90;
```

Cleanup runs as part of existing `cleanup` background task.

## Data Export

Detail page supports CSV export of current time range data (frontend-only, generated in browser via Blob). No backend API needed.

## Not In Scope (YAGNI)

- Traceroute / MTR — high complexity and agent resource cost
- Cross-target comparison analytics
- Persistent anomaly event log table — dynamic computation is sufficient
- Target health checking (validating if builtin IPs are still responsive)
- Configurable anomaly thresholds — hardcoded is sufficient for MVP
