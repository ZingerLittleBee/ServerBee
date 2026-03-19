# ServerBee Feature Design: Batch 1 & Batch 2

> Date: 2026-03-19
> Status: Approved

## Overview

Two batches of features to close the gap with competing probe/monitoring applications (Nezha, Uptime Kuma, Beszel, ServerStatus-Rust).

**Batch 1 (Core Competitiveness):**
1. Service Monitor — SSL, DNS, HTTP keyword, TCP port, WHOIS (Server-side execution)
2. Traffic Statistics Cycle Management — Global traffic page + Server Detail traffic tab
3. IP Change Notification — Server passive + Agent active detection

**Batch 2 (User Experience):**
4. Disk I/O Monitoring — Per-disk read/write speed collection
5. Tri-network Ping + Traceroute — Preset CT/CU/CM targets + on-demand traceroute
6. Multi-theme + Custom Branding — 8 preset themes + logo/title/favicon customization
7. Status Page Enhancement — Multiple pages, incidents, maintenance windows, uptime history
8. Mobile Responsive + PWA — Responsive layout + installable PWA

---

## 1. Service Monitor (Server-Side Monitoring Engine)

### 1.1 Design Decisions

- **Execution location**: Server-side only (not via Agent). SSL/DNS/HTTP/TCP/WHOIS checks do not require Agent presence; centralizing on the Server simplifies architecture.
- **Separate from Ping system**: New `service_monitor` / `service_monitor_record` tables, independent of Agent-side `ping_task` / `ping_record`. Different lifecycle (no WS dispatch, no capability checks).
- **Five monitor types**: SSL certificate, DNS records, HTTP(S) keyword, TCP port, domain WHOIS/expiry.

### 1.2 Data Model

**`service_monitor` table:**

| Column | Type | Description |
|--------|------|-------------|
| id | String (UUID) | PK |
| name | String | User-defined name |
| monitor_type | String | `ssl` / `dns` / `http_keyword` / `tcp` / `whois` |
| target | String | Domain/URL/IP:Port |
| interval | i32 | Check interval in seconds, default 300 (5 min) |
| config_json | String (JSON) | Type-specific configuration (see below) |
| notification_group_id | Option\<String\> | Notification group for alerts |
| retry_count | i32 | Consecutive failures before alerting, default 1 |
| enabled | bool | — |
| created_at | DateTime\<Utc\> | — |
| updated_at | DateTime\<Utc\> | — |

**`config_json` per type:**

```json
// SSL
{ "warning_days": 14, "critical_days": 7 }

// DNS
{ "record_type": "A", "expected_values": ["1.2.3.4"], "nameserver": null }

// HTTP Keyword
{ "method": "GET", "keyword": "OK", "keyword_exists": true,
  "expected_status": [200], "headers": {}, "body": null, "timeout": 10 }

// TCP
{ "timeout": 10 }

// WHOIS
{ "warning_days": 30, "critical_days": 7 }
```

**`service_monitor_record` table:**

| Column | Type | Description |
|--------|------|-------------|
| id | i64 | PK auto increment |
| monitor_id | String | FK -> service_monitor |
| success | bool | Check passed |
| latency | Option\<f64\> | Response time ms |
| detail_json | String (JSON) | Type-specific result details |
| error | Option\<String\> | Failure reason |
| time | DateTime\<Utc\> | — |

**`detail_json` per type:**

```json
// SSL
{ "issuer": "Let's Encrypt", "subject": "*.example.com",
  "not_before": "...", "not_after": "...", "days_remaining": 45,
  "fingerprint": "sha256:..." }

// DNS
{ "record_type": "A", "values": ["1.2.3.4", "5.6.7.8"],
  "nameserver": "8.8.8.8", "changed": false }

// HTTP Keyword
{ "status_code": 200, "keyword_found": true, "response_time_ms": 123 }

// TCP
{ "connected": true }

// WHOIS
{ "registrar": "...", "expiry_date": "...", "days_remaining": 120 }
```

### 1.3 Server-Side Execution Engine

New file: `crates/server/src/task/service_monitor_checker.rs`

- Background task ticks every **10 seconds**, checks which monitors are due.
- Maintains in-memory `next_check_at: HashMap<monitor_id, Instant>` schedule.
- Executes due monitors concurrently via `tokio::spawn`.
- Writes results to `service_monitor_record`.
- On failure: increments consecutive failure counter; alerts only after `retry_count` consecutive failures.
- On recovery (success after failures): resets counter, optionally sends recovery notification.

**Checker trait and implementations:**

```rust
trait ServiceChecker {
    async fn check(&self, target: &str, config: &serde_json::Value) -> CheckResult;
}

struct CheckResult {
    success: bool,
    latency: Option<f64>,
    detail: serde_json::Value,
    error: Option<String>,
}

struct SslChecker;          // rustls + x509-parser: connect, extract cert info
struct DnsChecker;          // hickory-resolver: query record_type, compare expected
struct HttpKeywordChecker;  // reqwest: send request, check status + keyword
struct TcpChecker;          // tokio::net::TcpStream::connect with timeout
struct WhoisChecker;        // whois crate: query domain, parse expiry
```

### 1.4 API Endpoints

```
GET    /api/service-monitors              List all (supports ?type= filter)
POST   /api/service-monitors              Create
GET    /api/service-monitors/:id          Detail (includes latest check result)
PUT    /api/service-monitors/:id          Update
DELETE /api/service-monitors/:id          Delete
GET    /api/service-monitors/:id/records  History (?from=&to=&limit=)
POST   /api/service-monitors/:id/check    Trigger immediate check
```

All endpoints require session/API key auth. Admin only for create/update/delete.

### 1.5 Frontend

- New page: `_authed/settings/service-monitors.tsx` — CRUD management (list + create/edit dialog)
- New page: `_authed/service-monitors/$id.tsx` — Detail page (uptime %, response time chart, history table, SSL/DNS detail card)
- Sidebar: new "Service Monitor" entry

### 1.6 Data Retention

- `service_monitor_record` retained for 30 days (configurable via `SERVERBEE_RETENTION__SERVICE_MONITOR_DAYS`)
- `cleanup` task extended to purge expired records

### 1.7 New Dependencies

- `x509-parser` — SSL certificate parsing
- `hickory-resolver` — DNS resolution (formerly `trust-dns-resolver`, renamed to hickory-dns)
- `whois-rust` — WHOIS query (fallback: shell `whois` command for TLDs the crate cannot parse)
- `reqwest` — already in use (for HTTP keyword checks)

### 1.8 Indexes

- `service_monitor`: INDEX on `enabled` (background task queries enabled monitors every 10s)
- `service_monitor_record`: INDEX on `(monitor_id, time)` for history queries and retention cleanup

### 1.9 Concurrency Control

The execution engine uses `tokio::sync::Semaphore` with max **20 concurrent checks** to prevent resource exhaustion when many monitors become due simultaneously. Monitors that cannot acquire a permit are deferred to the next tick (10s later).

### 1.10 Retention Config

Add `service_monitor_days: u32` (default 30) to `RetentionConfig` in `crates/server/src/config.rs`. Env var: `SERVERBEE_RETENTION__SERVICE_MONITOR_DAYS`.

### 1.11 OpenAPI

All new endpoints annotated with `#[utoipa::path]`. All new DTOs derive `ToSchema`. Service Monitor endpoints registered under a new `service-monitors` tag in `ApiDoc`.

---

## 2. Traffic Statistics Cycle Management

### 2.1 Existing Infrastructure (No Changes Needed)

- `traffic_hourly` / `traffic_daily` tables — continuously accumulating
- `traffic_state` table — OS counter checkpointing
- `record_writer` — computes delta every 60s -> `traffic_hourly`
- `aggregator` — rolls up hourly -> daily every 3600s
- `get_cycle_range()` — supports monthly/quarterly/yearly with `billing_start_day`
- Alert rules — `transfer_in_cycle` / `transfer_out_cycle` / `transfer_all_cycle` already working
- Server entity — `billing_cycle`, `billing_start_day`, `traffic_limit`, `traffic_limit_type` fields exist

### 2.2 New/Enhanced API Endpoints

All traffic endpoints are grouped under `/api/traffic/` prefix (distinct from the existing `/api/servers/{id}/traffic` endpoint which remains unchanged for backward compatibility).

```
GET /api/traffic/overview
```
Returns all servers' current billing cycle usage:
```json
{
  "servers": [{
    "server_id": "...",
    "name": "...",
    "cycle_in": 123456789,
    "cycle_out": 987654321,
    "traffic_limit": 1099511627776,
    "billing_cycle": "monthly",
    "percent_used": 45.2,
    "days_remaining": 12
  }]
}
```

```
GET /api/traffic/:server_id/cycle
```
Returns current + historical cycle data:
```json
{
  "current": {
    "start": "2026-03-01", "end": "2026-03-31",
    "bytes_in": 123, "bytes_out": 456, "limit": 1000, "percent": 57.9
  },
  "history": [
    { "period": "2026-02", "bytes_in": 100, "bytes_out": 200 },
    { "period": "2026-01", "bytes_in": 150, "bytes_out": 250 }
  ]
}
```

```
GET /api/traffic/:server_id/daily?from=&to=
```
Returns daily breakdown:
```json
{ "days": [{ "date": "2026-03-01", "bytes_in": 100, "bytes_out": 200 }] }
```

### 2.3 Frontend — Global Traffic Page

New route: `_authed/traffic/index.tsx`, sidebar first-level entry "Traffic".

**Page structure:**
- **Stat cards row** — Total inbound/outbound this cycle, highest-usage server, count of servers approaching limit
- **Server traffic ranking table** — Columns: server name, cycle inbound, outbound, total, limit, usage % (progress bar), days remaining. Sortable by usage/percentage.
- **Global trend chart** — All servers combined daily inbound/outbound area chart (last 30 days)

### 2.4 Frontend — Server Detail Traffic Tab

New "Traffic" tab in `servers/$id.tsx`:

- **Cycle overview card** — Current cycle start/end dates, used/total progress ring, inbound/outbound values
- **Daily trend chart** — Stacked bar chart (inbound/outbound), time range selector (7d/30d/90d)
- **Historical cycle comparison** — Last 6 cycles horizontal bar chart comparison

---

## 3. IP Change Notification

### 3.1 Detection Mechanisms

**Server-side passive detection (on connect):**
- When Agent WS connects, `agent.rs` handler already extracts `remote_addr` from `ConnectInfo`
- Store `remote_addr` in a new `servers.last_remote_addr` column (this is the TCP socket IP, which may be a NAT/proxy IP — distinct from `servers.ipv4` which is the Agent's self-reported local NIC IP)
- On each Agent connect: compare current `remote_addr` with stored `last_remote_addr`. If different, update `last_remote_addr`, write audit log, trigger notification
- Note: `remote_addr` vs `ipv4` are semantically different. `remote_addr` = external-facing IP (NAT gateway, public IP). `ipv4` = Agent's local NIC IP. Both are tracked independently.

**Agent-side active detection (during long connection):**
- Agent checks IP via two methods:
  1. **Local NIC enumeration** (`sysinfo::Networks`) — detects local interface IP changes
  2. **Optional external IP check** — if `config.check_external_ip = true` (default false), Agent queries `https://api.ipify.org` to detect public IP changes (useful for cloud VPS where local NIC shows private IP that never changes while public IP may be reassigned)
- Runs every 5 minutes. Compares against cached IP from previous check.
- On change: sends `IpChanged` message to Server

### 3.2 Protocol Extension

```rust
// AgentMessage — new variant
IpChanged {
    ipv4: Option<String>,
    ipv6: Option<String>,
    interfaces: Vec<NetworkInterface>,
}

// New type in types.rs
pub struct NetworkInterface {
    pub name: String,        // eth0, ens3, etc.
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
}
```

Server processing on `IpChanged`:
1. Update `servers.ipv4` / `servers.ipv6`
2. Re-run GeoIP resolution (region/country_code may change)
3. Write audit log entry `"ip_changed"`
4. Trigger notification via alert system

### 3.3 Alert Integration

Reuse existing alert system with new rule type:

```rust
// AlertRuleItem new rule_type value
"ip_changed"  // Triggers on any IP change; no threshold evaluation needed
```

This is an **event-driven rule** — it bypasses the normal poll-based `alert_evaluator` cycle. Instead, the IP change detection code (in `router/ws/agent.rs`) directly calls a new `AlertService::check_event_rules(server_id, event_type)` method:

```rust
impl AlertService {
    /// Check event-driven rules (ip_changed, etc.) — called from WS handler, not from alert_evaluator poll loop.
    pub async fn check_event_rules(&self, server_id: &str, event_type: &str) {
        // 1. Query enabled rules where rules_json contains an item with rule_type == event_type
        // 2. Check cover_type/server_ids to see if this server is covered
        // 3. If matched, fire notification via NotificationService::send_group
        // 4. Record alert_state (reuse existing AlertStateManager for dedup/once/always logic)
    }
}
```

This keeps the existing `evaluate_all()` poll loop unchanged (it skips `ip_changed` rules), while event-driven rules get immediate dispatch. The `AlertRuleItem` schema is reused with `min`/`max`/`duration` fields left as `None` for event rules.

Users create an `ip_changed` alert rule in the existing alert management UI, selecting notification group and server coverage (all/include/exclude).

### 3.4 Browser Push

```rust
// BrowserMessage — new variant
ServerIpChanged {
    server_id: String,
    old_ipv4: Option<String>,
    new_ipv4: Option<String>,
    old_ipv6: Option<String>,
    new_ipv6: Option<String>,
}
```

Frontend Dashboard shows a brief IP change indicator on the affected Server Card.

### 3.5 Changes Summary

| Layer | File | Change |
|-------|------|--------|
| Common | `protocol.rs` | New `IpChanged` variant + `NetworkInterface` type |
| Common | `types.rs` | New `NetworkInterface` struct |
| Agent | `reporter.rs` | New 5-min IP check timer, cached IP list comparison |
| Server | `router/ws/agent.rs` | Handle `IpChanged`: update DB + GeoIP + audit + notify |
| Server | `service/alert.rs` | New `ip_changed` rule type (event-driven, not threshold) |
| Server | Agent connect handler | Compare `remote_addr` vs stored `last_remote_addr` on connect |
| Server | migration | Add `last_remote_addr` column to `servers` table |
| Frontend | Alert management page | `rule_type` dropdown adds "IP Changed" option |

---

## 4. Disk I/O Monitoring

### 4.1 Agent-Side Collection

New file: `crates/agent/src/collector/disk_io.rs`

```rust
pub struct DiskIo {
    pub name: String,               // "sda", "nvme0n1", etc.
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
}
```

**Collection method by platform:**
- **Linux**: Read `/proc/diskstats`, compute delta between two samples (sectors read/written * 512 / elapsed_secs)
- **macOS**: `IOKit` framework or `iostat` command parsing
- **Windows**: WMI `Win32_PerfFormattedData_PerfDisk_PhysicalDisk`

**Filtering**: Only physical disks (exclude loop, dm, ram, sr virtual devices) via name prefix filtering.

**Sampling**: Each `Collector::collect()` call computes delta from previous sample. First call returns empty (no baseline).

### 4.2 Protocol Extension

```rust
// SystemReport — new field
#[serde(default)]
pub disk_io: Option<Vec<DiskIo>>,  // Option + #[serde(default)] for backward compatibility with old agents
```

The `#[serde(default)]` annotation is required so that old agents sending `Report` messages without the `disk_io` field will deserialize successfully (the field defaults to `None`).

### 4.3 Server-Side Storage

Extend existing `records` and `records_hourly` tables (no new table):

```sql
ALTER TABLE records ADD COLUMN disk_io_json TEXT;
ALTER TABLE records_hourly ADD COLUMN disk_io_json TEXT;
```

Rationale: Disk I/O is same-sampling-period as CPU/memory/network. JSON column avoids one-to-many join while supporting per-disk breakdown. Hourly aggregation computes per-disk averages.

Included in migration `m20260319_000001_service_monitor.rs` (up-only).

### 4.4 Frontend

**Server Detail page** — new "Disk I/O" chart section:
- One line chart group per disk (read speed + write speed)
- Optional merged view (all disks summed)
- Time range linked with other charts (raw/hourly)
- Uses existing `formatSpeed()` utility (bytes/s -> KB/s/MB/s)

**Dashboard Server Card**: No disk I/O display (information density already high).

---

## 5. Tri-Network Ping + Traceroute

### 5.1 Reuse Existing Network Probe System

The existing `network_probe_target` / `network_probe_record` / `NetworkProbeSync` infrastructure is fully reusable. Tri-network ping = preset targets with `provider` field set to CT/CU/CM.

### 5.2 Preset Tri-Network Targets

Seeded on first startup (admin can add/remove via existing Network Probe management page):

```
CT (China Telecom):
  Beijing    219.141.136.10   icmp
  Shanghai   202.96.209.133   icmp
  Guangzhou  58.60.188.222    icmp

CU (China Unicom):
  Beijing    202.106.50.1     icmp
  Shanghai   210.22.97.1      icmp
  Guangzhou  221.5.88.88      icmp

CM (China Mobile):
  Beijing    221.179.155.161  icmp
  Shanghai   211.136.112.200  icmp
  Guangzhou  120.196.165.24   icmp
```

Existing `network_probe_target` table already has `provider` and `location` columns — no schema change needed.

### 5.3 Frontend Enhancement — Network Page

Enhance existing `_authed/network/index.tsx` and `_authed/network/$serverId.tsx`:

- **Grouped by provider view** — CT/CU/CM three columns, each showing latency and packet loss to that carrier's nodes
- **Comparison mode** — Select multiple servers, side-by-side latency comparison to same target

### 5.4 Traceroute (On-Demand)

**Protocol extension:**

```rust
// ServerMessage — new variant
Traceroute {
    request_id: String,
    target: String,
    max_hops: u8,   // default 30
}

// AgentMessage — new variant
TracerouteResult {
    request_id: String,
    target: String,
    hops: Vec<TracerouteHop>,
    completed: bool,
    error: Option<String>,  // e.g., "traceroute not installed", "permission denied"
}

// New type
pub struct TracerouteHop {
    pub hop: u8,
    pub ip: Option<String>,
    pub hostname: Option<String>,
    pub rtt1: Option<f64>,   // ms
    pub rtt2: Option<f64>,
    pub rtt3: Option<f64>,
    pub asn: Option<String>,
}
```

**Agent implementation:**
- Linux/macOS: Execute system `traceroute` command (or `mtr -r -c 3`), parse output
- Windows: Execute `tracert` command, parse output
- Requires `CAP_PING_ICMP` capability (no new capability bit needed — traceroute is an extension of ICMP probing)
- **Input validation**: Target must match `^[a-zA-Z0-9.\-:]+$` regex (domain or IP only) to prevent command injection. Reject any input containing shell metacharacters.
- **Graceful fallback**: If `traceroute`/`mtr`/`tracert` is not installed, return `TracerouteResult` with `completed: true`, empty `hops`, and an error message in a new optional `error: Option<String>` field on `TracerouteResult`.
- **Privilege note**: On some systems traceroute requires root. If execution fails with permission error, report the error gracefully rather than failing silently.

**Server API:**

```
POST /api/servers/:id/traceroute     Trigger traceroute
  body: { "target": "1.2.3.4" }
  returns: { "request_id": "..." }

GET  /api/servers/:id/traceroute/:request_id   Get result
  returns: TracerouteResult (poll until completed=true)
```

**No persistence** — results stored in `AgentManager.pending_requests` (existing mechanism), 60s TTL.

**Frontend:**
- "Traceroute" button on Server Detail or Network detail page
- Input target address, click execute, display hop-by-hop result table
- Color-coded latency: <50ms green, <100ms yellow, >100ms red, timeout gray

---

## 6. Multi-Theme + Custom Branding

### 6.1 Preset Themes

8 themes, each with light and dark variants:

| Theme | Style | Primary Colors |
|-------|-------|---------------|
| Default | Current default | Blue/Gray |
| Tokyo Night | Popular dev theme | Purple-blue/Deep blue |
| Nord | Arctic tones | Ice blue/Snow white |
| Catppuccin | Warm pastel | Pink-purple/Cream |
| Dracula | Classic dark | Purple/Teal |
| One Dark | Atom-style | Blue/Orange |
| Solarized | Eye-friendly classic | Cyan/Yellow |
| Rose Pine | Elegant low-saturation | Rose/Gold |

### 6.2 Implementation — CSS Variables

Zero runtime overhead approach:

```
apps/web/src/themes/
  index.ts              — Theme registry + type definitions
  default.css           — Current theme (unchanged)
  tokyo-night.css       — :root[data-theme="tokyo-night"] { ... }
  nord.css
  catppuccin.css
  dracula.css
  one-dark.css
  solarized.css
  rose-pine.css
```

**Lazy loading**: Only the selected theme's CSS file is loaded via dynamic `<link>` injection. When the user switches themes, the old `<link>` is replaced with the new one. The `default.css` is always bundled (no extra load). This avoids loading all 8 theme files upfront.

Each theme file overrides all shadcn/ui CSS variables (`--background`, `--foreground`, `--primary`, `--card`, `--border`, etc.) in both light and dark variants:

```css
:root[data-theme="nord"] {
  --background: 0 0% 97%;
  --foreground: 220 16% 22%;
  --primary: 213 32% 52%;
}
:root[data-theme="nord"].dark {
  --background: 220 16% 22%;
  --foreground: 219 28% 88%;
}
```

### 6.3 ThemeProvider Extension

```typescript
// Existing: theme = 'dark' | 'light' | 'system'
// New: colorTheme = 'default' | 'tokyo-night' | 'nord' | ...

// New localStorage key: "color-theme"
// Application: document.documentElement.setAttribute('data-theme', colorTheme)

// useTheme() hook returns:
{ theme, setTheme, colorTheme, setColorTheme }
```

### 6.4 Custom Branding

**Storage**: Reuse existing `config` table (key-value):

```
brand.logo_url       — Custom logo URL (or base64 data URL)
brand.site_title     — Site title (default "ServerBee")
brand.favicon_url    — Custom favicon
brand.footer_text    — Footer text (optional)
```

**Logo upload**: `POST /api/settings/brand/logo` — upload image, store to `data/brand/` directory, serve via `/api/brand/logo` static route. Limit 512KB, formats **PNG/ICO only** (SVG excluded due to XSS risk from embedded `<script>` tags and event handlers).

**API:**

```
GET  /api/settings/brand         Get brand configuration
PUT  /api/settings/brand         Update brand configuration (admin only)
POST /api/settings/brand/logo    Upload logo file
```

**Frontend application:**
- Sidebar header: replace default logo with `brand.logo_url`, title with `brand.site_title`
- `<head>`: dynamically replace favicon
- Status page: also uses brand configuration
- Settings page: new "Branding" section (logo upload + title input + preview)

### 6.5 Theme Selection UI

Settings page new "Appearance" section:
- Theme grid: each theme shows a color preview card (4-5 primary color swatches), current selection highlighted
- Click to switch with instant preview
- Dark/Light/System toggle remains in header (unchanged)

---

## 7. Status Page Enhancement

### 7.1 Multiple Status Pages

**`status_page` table:**

| Column | Type | Description |
|--------|------|-------------|
| id | String (UUID) | PK |
| title | String | Page title |
| slug | String | URL path, unique (e.g., `asia`, `global`) |
| description | Option\<String\> | Page description |
| server_ids_json | String | Included servers JSON array |
| group_by_server_group | bool | Group by server groups, default true |
| show_values | bool | Show metric values, default true |
| custom_css | Option\<String\> | Optional custom styling (sanitized: only allow safe CSS properties, strip `url()`, `expression()`, `javascript:`, event handlers) |
| enabled | bool | — |
| created_at | DateTime\<Utc\> | — |
| updated_at | DateTime\<Utc\> | — |

**Routes:**
- `/status` — Default status page (backward compatible, shows all non-hidden servers)
- `/status/:slug` — Custom status page

### 7.2 Incidents

**`incident` table:**

| Column | Type | Description |
|--------|------|-------------|
| id | String (UUID) | PK |
| title | String | Incident title |
| status | String | `investigating` / `identified` / `monitoring` / `resolved` |
| severity | String | `minor` / `major` / `critical` |
| server_ids_json | Option\<String\> | Associated servers |
| status_page_ids_json | Option\<String\> | Display on which pages (null = all) |
| created_at | DateTime\<Utc\> | — |
| updated_at | DateTime\<Utc\> | — |
| resolved_at | Option\<DateTime\<Utc\>\> | — |

**`incident_update` table:**

| Column | Type | Description |
|--------|------|-------------|
| id | String (UUID) | PK |
| incident_id | String | FK -> incident |
| status | String | Status at time of update |
| message | String | Update content (supports Markdown) |
| created_at | DateTime\<Utc\> | — |

Workflow: admin creates incident -> adds updates ("Identified root cause", "Fix deployed"...) -> marks resolved.

### 7.3 Maintenance Windows

**`maintenance` table:**

| Column | Type | Description |
|--------|------|-------------|
| id | String (UUID) | PK |
| title | String | Maintenance title |
| description | Option\<String\> | Maintenance details |
| start_at | DateTime\<Utc\> | Planned start |
| end_at | DateTime\<Utc\> | Planned end |
| server_ids_json | Option\<String\> | Associated servers |
| status_page_ids_json | Option\<String\> | Display on which pages |
| active | bool | Whether effective |
| created_at | DateTime\<Utc\> | — |
| updated_at | DateTime\<Utc\> | — |

**Alert integration**: `alert_evaluator` checks if a server is within an active maintenance window before evaluating rules. If so, skip alerting to avoid false positives during planned maintenance.

```rust
fn is_in_maintenance(server_id: &str, now: DateTime<Utc>) -> bool {
    // Query active=true AND start_at <= now <= end_at AND server_ids contains server_id
}
```

### 7.4 Uptime History

**`uptime_daily` table (generated by aggregator):**

UNIQUE constraint on `(server_id, date)` for upsert (`INSERT ... ON CONFLICT DO UPDATE`).

| Column | Type | Description |
|--------|------|-------------|
| id | i64 | PK |
| server_id | String | FK |
| date | NaiveDate | Date (UNIQUE with server_id) |
| total_minutes | i32 | Total minutes in day (1440 or partial for first/last day) |
| online_minutes | i32 | Online minutes |
| downtime_incidents | i32 | Number of downtime events |

`aggregator` task extended: hourly check past 24h record existence, update `uptime_daily`.

**Frontend display:**
- **Uptime percentage** — Past 30/90 day availability number (e.g., 99.95%)
- **Uptime timeline bar** — 90-day horizontal bar (like GitHub status), one segment per day: green=100%, yellow=<100%, red=downtime, gray=no data
- Hover shows that day's specific online duration and downtime events

### 7.5 API Endpoints

```
# Status page management (admin)
GET    /api/status-pages                    List all status pages
POST   /api/status-pages                    Create
PUT    /api/status-pages/:id                Update
DELETE /api/status-pages/:id                Delete

# Public access (no auth)
GET    /api/status/:slug                    Get status page data (servers + incidents + maintenance + uptime)

# Incident management (admin)
GET    /api/incidents                       List incidents
POST   /api/incidents                       Create incident
PUT    /api/incidents/:id                   Update incident status
DELETE /api/incidents/:id                   Delete
POST   /api/incidents/:id/updates           Add incident update

# Maintenance management (admin)
GET    /api/maintenances                    List maintenance windows
POST   /api/maintenances                    Create
PUT    /api/maintenances/:id                Update
DELETE /api/maintenances/:id                Delete
```

### 7.6 Frontend Pages

**Public status page (`/status/:slug`):**
- Top: Brand logo + title + global status indicator (All Systems Operational / Partial Outage / Major Outage)
- Active incidents: Unresolved incident cards + progress update timeline
- Planned maintenance: Upcoming maintenance notices
- Server list: One row per server, 90-day uptime timeline bar + availability percentage
- Historical incidents: Last 7 days resolved incidents (collapsible)

**Admin page (`_authed/settings/status-pages.tsx`):**
- Status page list + create/edit dialog
- Incidents tab (list + create + add updates)
- Maintenance tab (list + create/edit)

---

## 8. Mobile Responsive + PWA

### 8.1 Responsive Breakpoints (Tailwind Defaults)

| Breakpoint | Width | Scenario |
|------------|-------|----------|
| `sm` | >= 640px | Large phone landscape |
| `md` | >= 768px | Tablet portrait |
| `lg` | >= 1024px | Tablet landscape / small laptop |
| `xl` | >= 1280px | Desktop (current design) |

### 8.2 Core Responsive Changes

**Sidebar -> Mobile drawer:**
- `lg`+: Current fixed sidebar unchanged
- Below `lg`: Sidebar hidden, hamburger menu button in Header left. Click opens shadcn/ui `Sheet` component (slides from left).
- Drawer content identical to Sidebar.

**Data tables -> Card lists:**
- Server list, alert rules, notification channels, etc. switch to stacked card view below `md`
- Each list page provides `<TableView>` and `<CardView>` components, viewport-switched
- Cards retain key info (name, status, core metrics); action buttons collapse to `...` dropdown

**Dashboard card grid:**
- `xl`: 4 columns -> `lg`: 3 -> `md`: 2 -> `sm`: 1
- StatCard row below `sm`: 2x2 grid or horizontal scroll

**Charts:**
- Recharts `<ResponsiveContainer>` already in use — no changes needed
- Chart tooltips on mobile: tap-triggered (not hover)

**Server Detail:**
- Metric cards from horizontal row to vertical stack
- Tab bar supports horizontal scroll (when many tabs)

**Dialogs:**
- Below `sm`: full-screen Sheet (slides from bottom) instead of centered modal
- shadcn/ui Dialog CSS override: `max-width: 100vw; height: 100vh` on small screens

### 8.3 PWA Configuration

**manifest.json:**

```json
{
  "name": "ServerBee",
  "short_name": "ServerBee",
  "description": "Server Monitoring Dashboard",
  "start_url": "/",
  "display": "standalone",
  "background_color": "#0a0a0a",
  "theme_color": "#f59e0b",
  "icons": [
    { "src": "/pwa-192.png", "sizes": "192x192", "type": "image/png" },
    { "src": "/pwa-512.png", "sizes": "512x512", "type": "image/png" },
    { "src": "/pwa-maskable-512.png", "sizes": "512x512", "type": "image/png", "purpose": "maskable" }
  ]
}
```

**Service Worker (vite-plugin-pwa):**

```typescript
VitePWA({
  registerType: 'autoUpdate',
  workbox: {
    globPatterns: ['**/*.{js,css,html,woff2,png,svg}'],
    navigateFallback: '/index.html',
    runtimeCaching: [
      { urlPattern: /^\/api\//, handler: 'NetworkOnly' },
      { urlPattern: /^\/pwa-/, handler: 'CacheFirst' },
    ],
  },
})
```

**Strategy:**
- App Shell (HTML/JS/CSS): precached, loads offline (shows page skeleton)
- API data: always network (monitoring data must be real-time); offline shows "waiting for connection" placeholder
- WebSocket: existing auto-reconnect with exponential backoff handles disconnections

### 8.4 Web Push Notifications

**Not implemented in this batch.** Rationale:
- Requires VAPID key management + push server infrastructure
- Existing alert notification channels (Telegram/Webhook/Email/Bark) already cover mobile
- Low ROI for this iteration
- Can be added later without architectural changes

### 8.5 OpenAPI

All new API endpoints (service monitors, traffic, status pages, incidents, maintenances, brand) annotated with `#[utoipa::path]`. All new DTOs derive `ToSchema`. New tags registered in `ApiDoc`: `service-monitors`, `traffic`, `status-pages`, `incidents`, `maintenances`, `brand`.

### 8.6 New Dependencies

- `vite-plugin-pwa` — PWA manifest generation + Service Worker

### 8.7 Changes Summary

| Change | Files |
|--------|-------|
| Sidebar responsive | `components/layout/sidebar.tsx` — Sheet drawer mode |
| Header responsive | `components/layout/header.tsx` — Hamburger menu button |
| Table/Card toggle | List pages — new CardView components |
| Dashboard grid | `_authed/index.tsx` — responsive grid columns |
| Dialog fullscreen | `components/ui/` — small-screen Dialog style override |
| PWA config | `vite.config.ts` + `public/manifest.json` + icon assets |
| SW registration | `main.tsx` — register Service Worker |
| Viewport meta | `index.html` — ensure `<meta name="viewport">` is correct |

---

## Database Migration Summary

Split into **3 migration files** (one per major feature group) to reduce risk:

- `m20260319_000001_service_monitor.rs` — service_monitor + service_monitor_record tables, disk_io_json columns, last_remote_addr column
- `m20260319_000002_status_page.rs` — status_page + incident + incident_update + maintenance + uptime_daily tables
- `m20260319_000003_seed_network_targets.rs` — Seed tri-network probe targets

### New Tables (7)
- `service_monitor` — Service monitor definitions
- `service_monitor_record` — Service monitor check results
- `status_page` — Multiple status page definitions
- `incident` — Incident tracking
- `incident_update` — Incident progress updates
- `maintenance` — Maintenance window definitions
- `uptime_daily` — Daily uptime aggregation

### Altered Tables (3)
- `records` — Add `disk_io_json TEXT`
- `records_hourly` — Add `disk_io_json TEXT`
- `servers` — Add `last_remote_addr TEXT`

### Seed Data
- 9 preset tri-network probe targets (CT/CU/CM x Beijing/Shanghai/Guangzhou)

---

## Protocol Changes Summary

### AgentMessage — New Variants
- `IpChanged { ipv4, ipv6, interfaces: Vec<NetworkInterface> }`
- `TracerouteResult { request_id, target, hops: Vec<TracerouteHop>, completed }`

### ServerMessage — New Variants
- `Traceroute { request_id, target, max_hops }`

### BrowserMessage — New Variants
- `ServerIpChanged { server_id, old_ipv4, new_ipv4, old_ipv6, new_ipv6 }`

### New Types
- `NetworkInterface { name, ipv4: Vec<String>, ipv6: Vec<String> }`
- `TracerouteHop { hop, ip, hostname, rtt1, rtt2, rtt3, asn }`
- `TracerouteResult` updated with `error: Option<String>` field
- `DiskIo { name, read_bytes_per_sec, write_bytes_per_sec }`

### Modified Types
- `SystemReport` — Add `#[serde(default)] disk_io: Option<Vec<DiskIo>>`

---

## New Frontend Routes Summary

| Route | Auth | Description |
|-------|------|-------------|
| `_authed/settings/service-monitors.tsx` | Admin | Service monitor CRUD |
| `_authed/service-monitors/$id.tsx` | Auth | Service monitor detail |
| `_authed/traffic/index.tsx` | Auth | Global traffic overview |
| `_authed/settings/status-pages.tsx` | Admin | Status page + incident + maintenance management |
| `/status/:slug` | Public | Custom status page |

### Enhanced Existing Routes
| Route | Changes |
|-------|---------|
| `_authed/servers/$id.tsx` | New Traffic tab, Disk I/O charts, Traceroute button |
| `_authed/network/index.tsx` | Tri-network grouped view |
| `_authed/network/$serverId.tsx` | Provider grouping, comparison mode |
| `_authed/settings/alerts.tsx` | New `ip_changed` rule type |
| All list pages | Table/Card responsive toggle |
| Layout components | Sidebar drawer, Header hamburger, Dialog fullscreen |

---

## Implementation Order

### Batch 1 (Core Competitiveness)
1. **P9: Service Monitor** — migration + entities + service + checkers + API + frontend
2. **P10: Traffic Statistics** — API endpoints + global traffic page + Server Detail traffic tab
3. **P11: IP Change Notification** — protocol + agent detection + server handling + alert integration

### Batch 2 (User Experience)
4. **P12: Disk I/O Monitoring** — agent collector + protocol + migration + frontend charts
5. **P13: Tri-Network Ping + Traceroute** — seed targets + frontend enhancement + traceroute protocol + agent executor
6. **P14: Multi-Theme + Branding** — CSS themes + ThemeProvider extension + brand API + settings UI
7. **P15: Status Page Enhancement** — migration + entities + services + API + public/admin frontend
8. **P16: Mobile Responsive + PWA** — responsive CSS + layout changes + PWA configuration
