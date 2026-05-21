# Security Events: SSH Login / Brute Force / Port Scan

- **Date**: 2026-05-21
- **Status**: Draft (pending implementation plan)
- **Scope**: Linux agents only

## 1. Goal

Detect and report security-relevant events from the host where the agent runs, surface them in the web UI in real time, and let users wire them into the existing alert / notification pipeline. First wave of events:

- **SSH login (success)** — every successful authentication, with a `first_seen` flag indicating "this (user, source_ip) is new for this server".
- **SSH brute force** — agent-side sliding-window detection over failed authentication attempts.
- **Port scan** — agent-side sliding-window detection over distinct destination ports touched by the same source IP.

Non-goals (v1):
- macOS / Windows support.
- Active blocking (no firewall rule injection, no fail2ban replacement).
- Cross-server correlation (kept as an explicit phase-2 evolution path).
- Compact statistical rollups (protocol leaves room; not implemented in v1).

## 2. Architecture

```
                ┌──────────────────────────────────────────────┐
   Linux        │ Agent (crates/agent/src/security/)           │
   VPS          │                                              │
                │  ┌─────────────────┐   ┌─────────────────┐   │
   sshd ───────►│  │ JournalWatcher  │   │ ConntrackWatch  │◄──┼── netlink
   journal      │  │ (journalctl -f) │   │ (nfnetlink)     │   │
                │  └────────┬────────┘   └────────┬────────┘   │
                │           ▼                     ▼            │
                │  ┌──────────────────────────────────────┐    │
                │  │  Detector (sliding-window engine)    │    │
                │  │  ├ SshAuthDetector                   │    │
                │  │  │   • emit ssh_login (always)       │    │
                │  │  │   • emit ssh_brute_force (≥thr)   │    │
                │  │  │   • track (user, src_ip) → first  │    │
                │  │  └ PortScanDetector                  │    │
                │  │      • emit port_scan (≥thr)         │    │
                │  └──────────────┬───────────────────────┘    │
                │                 ▼                            │
                │       AgentMessage::SecurityEvent            │
                └───────────────┬──────────────────────────────┘
                                │ WebSocket (JSON)
                                ▼
                ┌─────────────────────────────────────────────┐
   Server       │  router/ws/agent  →  service::security      │
                │            │                                │
                │            ▼                                │
                │   ┌──────────────────┐                      │
                │   │ security_event   │ sea-orm entity       │
                │   │ table            │                      │
                │   └────────┬─────────┘                      │
                │            │                                │
                │            ├──► broadcast::Sender ──► Browser WS
                │            │                                │
                │            └──► alert_trigger (event-driven)│
                │                  • ssh_brute_force_detected │
                │                  • ssh_new_ip_login         │
                │                  • port_scan_detected       │
                │                  └──► notification          │
                └─────────────────────────────────────────────┘
```

Key properties:
- Events are pushed in real time, not bundled into the 60s `Report` envelope.
- Agent buffers events in a bounded in-memory queue when WS is down; oldest entries are dropped first.
- Detection and aggregation live in the agent for v1. Server is a sink + dispatcher. Protocol is shaped so phase-2 can add `SecurityRollup` without breaking v1.

## 3. Decisions Locked In

| Dimension | Decision | Rationale |
|---|---|---|
| Platforms | Linux only | 99% VPS install base; macOS/Windows detection has very different mechanics |
| SSH source | systemd-journal primary, `/var/log/auth.log` fallback | Every modern distro ships systemd; file fallback covers exotic containers |
| Port scan source | nfnetlink_conntrack primary, kernel firewall log supplementary | Zero-config detection of SYN floods to distinct ports; firewall log adds visibility into blocked traffic |
| Aggregation tier | Agent-local in v1, two-tier architecture | Keep raw event volume off the wire; protocol leaves room for server-side rollups |
| Login success reporting | All successful logins + `first_seen` boolean | Full audit trail; server decides whether to alert |
| Capability flag | New `CAP_SECURITY_EVENTS` (bit 8 = 256), included in `CAP_DEFAULT` | Opt-out for privacy-sensitive deployments; default-on for VPS users. **SSH detection works with the default `CAP_NET_RAW` privilege the systemd unit already grants; port-scan detection requires the agent operator to opt in via `agent.toml` AND grant `CAP_NET_ADMIN` to the unit, so default-on does not imply a privilege expansion.** |
| Alert integration | Reuse existing `alert_rules.rules_json` (`Vec<AlertRuleItem>`); add new `rule_type` values plus an optional typed `security` field on `AlertRuleItem` | Avoids inventing a parallel rule schema; mirrors how new rule types have historically been added |
| Implementation flavor | Hybrid — `journalctl` subprocess + Rust netlink for conntrack | `journalctl` is universally available; `conntrack-tools` often is not |

## 4. Data Model

New table `security_event`:

```rust
// crates/server/src/entity/security_event.rs
pub struct Model {
    pub id: String,                     // UUID v4 string, matches `Uuid::new_v4().to_string()`
                                        // pattern used across the codebase (e.g. alert.rs:284)
    pub server_id: String,              // FK → server.id (String per project convention)
    pub event_type: String,             // "ssh_login" | "ssh_brute_force" | "port_scan"
    pub severity: String,               // "info" | "low" | "medium" | "high" | "critical"
    pub source_ip: String,              // IPv4/IPv6, fully expanded for v6
    pub source_port: Option<i32>,       // ssh_login only
    pub username: Option<String>,
    pub started_at: DateTimeUtc,        // event window start
    pub ended_at: DateTimeUtc,          // ssh_login: == started_at; aggregated: window close
    pub first_seen: bool,               // ssh_login only: (server, user, src_ip) new locally
    pub detector_source: String,        // "journal" | "auth_log" | "conntrack" | "firewall_log"
    pub evidence: String,               // JSON-encoded; sqlite has no native JSON column type
    pub created_at: DateTimeUtc,        // server insert time
}
```

Indexes:

- `(server_id, created_at DESC)` — primary query path
- `(source_ip, created_at DESC)` — phase-2 cross-server correlation
- `(event_type, created_at DESC)` — type filter
- `(server_id, event_type, source_ip, started_at)` — alert dedupe lookups

Evidence JSON schemas:

```jsonc
// ssh_login
{ "auth_method": "publickey" | "password" | "keyboard-interactive" }

// ssh_brute_force
{
  "failed_count": 47,
  "distinct_users": 12,
  "sample_users": ["root", "admin", "ubuntu", "test", "git"],  // cap 10
  "invalid_user_count": 8,
  "window_seconds": 60,
  "threshold": 10
}

// port_scan
{
  "distinct_ports": 134,
  "sample_ports": [22, 80, 443, 3306, 5432, 6379, 8080, 9000, 27017, 11211],  // cap 20
  "total_attempts": 287,
  "window_seconds": 30,
  "threshold": 20,
  "blocked_count": 134  // 0 if firewall_log not contributing
}
```

Retention: reuse existing cleanup task. New config field `retention.security_event_days` on `RetentionConfig` (default 30), following the existing naming pattern (`records_days`, `audit_logs_days`, `service_monitor_days`, …). Env override: `SERVERBEE_RETENTION__SECURITY_EVENT_DAYS`.

Recovery: `security_event` is added to `recovery_merge::merge_server_history_on_connection` so that when a server identity is rebound, security history follows. Concretely:

- `RecoveryLockService` is **not** consulted on `security_event` inserts. The agent has no ack/backpressure protocol — if the server silently dropped writes during freeze, events would be lost forever. Allowing the writes lets the row land under the source identity; `merge_server_history_on_connection` then reconciles it onto the target identity when recovery completes (same model used for `records` and `network_probe_record`).
- The freeze remains in force for the tables that *need* the guard (record aggregation, traffic counters): writes that depend on prior state must be paused while history is being merged. `security_event` is append-only with no derived state, so it is safe to write during freeze.
- This is the deliberate carve-out, documented in `service::security` source.

Migrations:

1. `m20260521_001_create_security_event.rs` — creates the table.
2. `m20260521_002_extend_alert_state_event_key.rs` — adds `alert_states.event_key TEXT NOT NULL DEFAULT ''`, drops the existing unique index `idx_alert_states_rule_id_server_id` (`crates/server/src/migration/m20260312_000001_init.rs:579`), and recreates it as `idx_alert_states_rule_id_server_id_event_key` covering `(rule_id, server_id, event_key)`. We use `NOT NULL DEFAULT ''` because SQLite's UNIQUE treats multiple NULLs as distinct, which would bypass dedupe — legacy metric rules pass `event_key=""`, security rules pass the source IP.
3. `m20260521_003_backfill_capability_default.rs` — `UPDATE servers SET capabilities = capabilities | 256 WHERE capabilities = 60` (the previous default). This is **scoped to rows still on the old default** so administrators who explicitly set a custom mask (lower or higher) are not silently expanded. Operators who want security events on a customized server enable it from the UI like any other capability.

Each `up()` only; `down()` returns `Ok(())` per project convention.

**Note on column naming**: the actual column is `servers.capabilities` (`i32`), not `effective_capabilities`. The "effective" mask is composed at runtime by `AgentManager::get_effective_capabilities(server_id)` (`crates/server/src/service/agent_manager.rs:500`), which ANDs the server-side configured mask with the agent's locally advertised mask. Spec snippets that check capability membership go through that helper.

## 5. Protocol (crates/common)

### 5.1 AgentMessage

```rust
pub enum AgentMessage {
    // ... existing variants
    SecurityEvent(SecurityEventPayload),
}

pub struct SecurityEventPayload {
    pub event_type: SecurityEventType,
    pub severity: Severity,
    pub source_ip: String,
    pub source_port: Option<u16>,
    pub username: Option<String>,
    pub started_at: i64,          // unix seconds, UTC
    pub ended_at: i64,
    pub first_seen: bool,
    pub detector_source: DetectorSource,
    pub evidence: SecurityEvidence,
}

pub enum SecurityEventType { SshLogin, SshBruteForce, PortScan }
pub enum Severity { Info, Low, Medium, High, Critical }
pub enum DetectorSource { Journal, AuthLog, Conntrack, FirewallLog }

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SecurityEvidence {
    SshLogin { auth_method: SshAuthMethod },
    SshBruteForce {
        failed_count: u32,
        distinct_users: u32,
        sample_users: Vec<String>,
        invalid_user_count: u32,
        window_seconds: u32,
        threshold: u32,
    },
    PortScan {
        distinct_ports: u32,
        sample_ports: Vec<u16>,
        total_attempts: u32,
        window_seconds: u32,
        threshold: u32,
        blocked_count: u32,
    },
}
```

### 5.2 ServerMessage (config push)

**v1 deliberately does not add a `SecurityConfigSync` variant.** Detection thresholds live entirely in the agent's local `agent.toml` (see §6.6) with built-in defaults. We considered a server → agent push channel, but shipping it cleanly requires server-side persistence (new table or `app_config` rows), CRUD API, UI, and propagation on rule edit. That is more surface area than the v1 problem (alerting on detected attacks) justifies.

Phase 2 will add `ServerMessage::SecurityConfigSync` together with the server-side config store. The protocol additions are listed in §12 so detectors can be designed to read configuration through an `Arc<RwLock<SecurityConfig>>` from day one, making the future push wire-up a drop-in.

### 5.3 BrowserMessage

```rust
pub enum BrowserMessage {
    // ... existing variants
    SecurityEvent(SecurityEventBroadcast),
}

pub struct SecurityEventBroadcast {
    pub server_id: String,
    pub event_id: String,
    pub event: SecurityEventPayload,
}
```

Broadcast after successful DB insert.

### 5.4 Capability bits

```rust
// crates/common/src/constants.rs (existing file)
pub const CAP_SECURITY_EVENTS: u32 = 1 << 8;       // 256
pub const CAP_VALID_MASK: u32 = 0b1_1111_1111;     // bits 0-8 (9 bits total)
pub const CAP_DEFAULT: u32 =
    CAP_UPGRADE | CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP | CAP_SECURITY_EVENTS;  // 60 + 256 = 316
```

The existing `CAP_DEFAULT` is 60 (includes `CAP_UPGRADE`), so the new default is 316. The `CapabilityDescriptor` registry (`crates/common/src/constants.rs:129-178`) gains a new entry, and `parse_cap_token` / `Display` impls are updated.

Backfill (`m20260521_003_backfill_capability_default.rs`): runs `UPDATE servers SET capabilities = capabilities | 256 WHERE capabilities = 60`. The `= 60` predicate scopes the change to rows that were still on the previous default — administrators who set a customized mask (whether stricter or wider) are not silently flipped. Without backfill, freshly upgraded installs whose server rows were written under the old default would silently keep security events disabled.

Defence in depth:
- Server rejects `SecurityEvent` when `agent_manager.get_effective_capabilities(server_id)` lacks `CAP_SECURITY_EVENTS` (logged to `audit_log`). If the agent connection is gone we fall back to reading `servers.capabilities` directly.
- Agent self-disables the watcher if its local effective capability mask lacks the bit.

## 6. Agent Implementation

Directory layout:

```
crates/agent/src/security/
├── mod.rs                — SecurityManager (life-cycle, config sync)
├── journal_watcher.rs    — journalctl subprocess wrapper + line parsing
├── conntrack_watcher.rs  — nfnetlink_conntrack subscription
├── ssh_parser.rs         — sshd log line → AuthAttempt
├── ssh_detector.rs       — sliding window → SshBruteForce / SshLogin
├── scan_detector.rs      — sliding window → PortScan
└── first_seen_store.rs   — persistent (user, source_ip) set
```

### 6.1 SecurityManager

```rust
pub struct SecurityManager {
    tx: mpsc::Sender<AgentMessage>,        // → Reporter
    config: Arc<RwLock<SecurityConfig>>,   // loaded once from agent.toml; the RwLock
                                           // shape lets phase-2 SecurityConfigSync
                                           // swap thresholds without restarting watchers
    journal_handle: Option<JoinHandle<()>>,
    conntrack_handle: Option<JoinHandle<()>>,
}
```

Start conditions: local capability includes `CAP_SECURITY_EVENTS` AND `target_os = "linux"`.

### 6.2 JournalWatcher

Two independent `journalctl` streams (kernel and sshd traverse different filters; ORing them in a single invocation is fragile across distros):

1. **sshd stream** — `journalctl -f --output=json -n 0 SYSLOG_IDENTIFIER=sshd + _SYSTEMD_UNIT=ssh.service + _SYSTEMD_UNIT=sshd.service + _COMM=sshd` (the `+` is journalctl's OR operator). Covers Debian/Ubuntu (`ssh.service`), RHEL/Fedora/Alpine (`sshd.service`), and systems where only `SYSLOG_IDENTIFIER` is set reliably. Falls back to tailing `/var/log/auth.log` (Debian/Ubuntu) or `/var/log/secure` (RHEL) via the `notify` crate when `journalctl --version` is absent.
2. **kernel stream** — `journalctl -k -f --output=json -n 0`. Only started when port-scan detection is enabled (§6.3), because its sole purpose is the firewall-log enrichment of scan events. Lines matching `[UFW BLOCK]` / `iptables: ` / `nftables` prefixes go to `FirewallDrop`.

Each stream parses with `serde_json` into `JournalEntry`. sshd entries flow to `ssh_parser`, kernel firewall hits flow to `scan_detector` as auxiliary signal (sets `evidence.blocked_count`).

Regex set (ssh_parser):

- `Accepted (publickey|password|keyboard-interactive) for (\S+) from (\S+) port (\d+)` → success
- `Failed password for (?:invalid user )?(\S+) from (\S+) port (\d+)` → failure (`invalid user` flagged)
- `Invalid user (\S+) from (\S+) port (\d+)` → failure + invalid_user

Subprocess crash → exponential backoff 1s → 30s, matching the reporter style.

### 6.3 ConntrackWatcher

Dependencies: `netlink-sys` + `netlink-packet-netfilter` (or equivalent maintained crate).

```rust
let mut socket = NetlinkSocket::new(NETLINK_NETFILTER)?;
socket.bind(&SocketAddr::new(0, NF_NETLINK_CONNTRACK_NEW))?;
loop {
    let msg = socket.recv().await?;
    if let Ok(event) = parse_conntrack_new(&msg) {
        if event.protocol == IPPROTO_TCP && event.state == TCP_SYN_SENT {
            scan_detector_tx.send(ConntrackEvent { src_ip, dst_port, ts }).await?;
        }
    }
}
```

**Privilege model — explicit security decision**: subscribing to `NETLINK_NETFILTER` requires `CAP_NET_ADMIN`. The existing `deploy/serverbee-agent.service` only grants `CAP_NET_RAW`, so port-scan detection introduces a real privilege expansion. To avoid silently broadening privileges on upgrade:

- `agent.toml` field `security.port_scan.enabled` defaults to **`false`**.
- The shipped systemd unit is **unchanged** (still `CAP_NET_RAW` only). `SecurityManager` only starts `ConntrackWatcher` when `security.port_scan.enabled = true`.
- When the user opts in, the agent log emits a one-time `WARN` line linking to documentation: enabling port-scan detection requires the operator to edit the unit to add `CAP_NET_ADMIN` to `AmbientCapabilities` (or run the agent as root), then `systemctl daemon-reload && systemctl restart serverbee-agent`. Without that, netlink bind fails and the watcher stays down — partial degradation only, SSH detection still works.
- Documentation (Fumadocs `security-events` page) walks through the unit edit step.

If bind fails for any other reason (e.g., kernel module not loaded), scan detection disables but SSH detection continues — graceful degradation, never an all-or-nothing failure.

### 6.4 Detectors

**SshDetector**:
- State: `HashMap<src_ip, VecDeque<AuthAttempt>>`
- On each attempt, pop entries older than `window_seconds`
- Emit `ssh_brute_force` when `failed_count ≥ failed_threshold`. `distinct_users` is **not** a gate — single-user hammering (`ssh root@host` × 50) is the canonical brute-force pattern and must trigger. Instead, `distinct_users` raises severity:
  - `distinct_users == 1` → `severity = medium`
  - `distinct_users ∈ [2, 4]` → `severity = high`
  - `distinct_users ≥ 5` → `severity = critical` (clearly a credential-stuffing scanner)
- After emit, clear the IP's queue to avoid duplicate fires inside the same window.
- Success → check `first_seen_store` for `(user, src_ip)` → emit `ssh_login` immediately.
- Background sweep every 5 min: drop IPs idle > 10 min.

**PortScanDetector**:
- State per src_ip: `VecDeque<(ts, dst_port)>` (full event log within window) + `HashMap<dst_port, u32>` (active counts).
- On new event: push to deque; `*port_counts.entry(port).or_insert(0) += 1`.
- On window slide (cheapest: check head before each insert, plus a 5s background sweep): pop head entries where `ts < now - window_seconds`; for each popped `port`, decrement the count map and remove the key when count hits 0. `distinct_ports == port_counts.len()`.
- Emit `port_scan` when `distinct_ports ≥ threshold`; clear that IP's state post-emit.

### 6.5 FirstSeenStore

- File: `<agent_data_dir>/security/first_seen.json`
- Data: `HashMap<(username, source_ip), unix_ts>` (key uses `\x00` separator)
- Memory-backed reads; batched writes (every 10s or every 100 changes), atomic `tmp + rename`
- Load on startup; corrupted file → rename to `.corrupt-<ts>`, reset to empty
- Size cap: 10000 entries, LRU-evict to 8000 when full

### 6.6 Agent config

`agent.toml`:

```toml
[security]
enabled = true                            # gates the SecurityManager as a whole
data_dir = "/var/lib/serverbee/security"

[security.ssh]
window_seconds = 60
failed_threshold = 10
# distinct_users is not a gate; it raises severity. See §6.4.

[security.port_scan]
enabled = false                           # opt-in; requires CAP_NET_ADMIN, see §6.3
window_seconds = 30
distinct_port_threshold = 20
```

Precedence (v1): local `agent.toml` > built-in defaults. There is no server-side override path in v1 — see §5.2 and §12. When phase-2 adds `SecurityConfigSync`, `security.port_scan.enabled` will be excluded from the syncable set: it is a local privilege opt-in and must not be flippable from the control plane.

## 7. Server Implementation

Layout:

```
crates/server/src/
├── entity/security_event.rs           — entity
├── migration/m20260521_001_*.rs       — table creation
├── service/security.rs                — insert + broadcast + alert trigger
└── router/api/security.rs             — REST queries
```

### 7.1 WS entry point

```rust
// router/ws/agent.rs — inside handle_agent_message(state, server_id, msg)
AgentMessage::SecurityEvent(payload) => {
    // handle_agent_message only carries server_id (not the server model),
    // matching how other branches like PingResult / TerminalOutput operate.
    let caps = match state.agent_manager.get_effective_capabilities(server_id) {
        Some(c) => c,
        None => {
            // Agent has not yet sent SystemInfo on this connection. Cheap DB lookup
            // by primary key; cached server snapshots are not part of AppState.
            match server::Entity::find_by_id(server_id).one(&state.db).await {
                Ok(Some(s)) => u32::try_from(s.capabilities).unwrap_or(0),
                _ => 0,
            }
        }
    };
    if !has_capability(caps, CAP_SECURITY_EVENTS) {
        AuditLogService::write("security_event_denied", server_id, ...).await.ok();
        return;
    }
    state.security_service.record_event(server_id, payload).await.ok();
}
```

The `&server_id` is already a `&str` in the handler scope; no extra cloning is needed. The fallback DB query is rare in practice — `SystemInfo` arrives within the first few seconds of a new connection — so we don't pre-fetch the server model just to make the hot path branchless.

### 7.2 service::security

```rust
pub struct SecurityService {
    db: DatabaseConnection,
    browser_tx: broadcast::Sender<BrowserMessage>,
    alert_state_manager: Arc<AlertStateManager>,
    // Notifications are dispatched via NotificationService::send_group
    // (crates/server/src/service/notification.rs:323), called directly when a
    // rule fires — there is no separate dispatcher type in the codebase today.
}

impl SecurityService {
    pub async fn record_event(&self, server_id: &str, p: SecurityEventPayload) -> Result<String> {
        // 1. Validate (IP format, evidence shape, allowed event_type)
        // 2. Insert security_event row (UUID id)
        // 3. Broadcast BrowserMessage::SecurityEvent
        // 4. Evaluate matching alert rules inline (push-based, low-latency).
    }
}
```

### 7.3 Alert rule integration

The existing alert schema stores `Vec<AlertRuleItem>` JSON-encoded in `alert_rules.rules_json` (`crates/server/src/service/alert.rs:15`). We extend that struct rather than inventing a parallel table:

```rust
// crates/server/src/service/alert.rs
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AlertRuleItem {
    pub rule_type: String,
    #[serde(default)] pub min: Option<f64>,
    #[serde(default)] pub max: Option<f64>,
    #[serde(default)] pub duration: Option<u32>,
    #[serde(default)] pub cycle_interval: Option<String>,
    #[serde(default)] pub cycle_limit: Option<i64>,
    #[serde(default)] pub security: Option<SecurityRuleParams>,  // NEW
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SecurityRuleParams {
    #[serde(default)] pub min_failed_count: Option<u32>,        // ssh_brute_force_detected
    #[serde(default)] pub min_distinct_ports: Option<u32>,      // port_scan_detected
    #[serde(default)] pub exclude_users: Vec<String>,           // ssh_new_ip_login
    #[serde(default)] pub exclude_cidrs: Vec<String>,           // ssh_new_ip_login
    #[serde(default = "default_dedupe_secs")] pub dedupe_window_seconds: u32,
}

fn default_dedupe_secs() -> u32 { 600 }
```

New `rule_type` values: `ssh_brute_force_detected`, `ssh_new_ip_login` (fires only when `first_seen=true`), `port_scan_detected`. All existing rule_type strings remain untouched.

`EVENT_DRIVEN_RULE_TYPES` (`crates/server/src/service/alert.rs:15`) currently contains only `"ip_changed"`; we extend it to:

```rust
const EVENT_DRIVEN_RULE_TYPES: &[&str] =
    &["ip_changed", "ssh_brute_force_detected", "ssh_new_ip_login", "port_scan_detected"];
```

This is what the metric-based 60s `alert_evaluator` uses to skip event-driven rules (see `alert.rs:468-475`). Without extending it, the periodic evaluator would attempt to handle the new types as metrics and likely no-op them, but the safer contract is to keep the registry authoritative.

**Multi-item semantics**: `rules_json` allows multiple `AlertRuleItem`s per rule. The metric evaluator's `check_server` (`crates/server/src/service/alert.rs:531`) requires **all** items to match (AND semantics — `if !matched { return false }`). For event-driven security types, AND across mixed types is meaningless — a single inbound `SecurityEvent` can only satisfy one type. We therefore **forbid mixing security rule types with non-security types within one `alert_rule`**, and forbid more than one security item per rule. The validator in `service::alert` enforces this on `CreateAlertRule` / `UpdateAlertRule`. Rationale: each security preset becomes one focused rule (matches the three-card UI in §8.4) and dedupe semantics stay unambiguous.

**Dedupe model**: existing `AlertStateManager.triggered: DashMap<(String, String), TriggeredInfo>` keys by `(rule_id, server_id)`. The existing unique index `idx_alert_states_rule_id_server_id` (`crates/server/src/migration/m20260312_000001_init.rs:579`) enforces this at the DB level. For security rules, collapsing on `(rule_id, server_id)` would merge "IP A scanning" and "IP B scanning" into one alert state, defeating per-attacker visibility. Solution:

- Migration `m20260521_002_extend_alert_state_event_key.rs` adds `alert_states.event_key TEXT NOT NULL DEFAULT ''`, drops `idx_alert_states_rule_id_server_id`, and creates `idx_alert_states_rule_id_server_id_event_key UNIQUE(rule_id, server_id, event_key)`. `NOT NULL DEFAULT ''` is required because SQLite UNIQUE treats multiple NULLs as distinct — using NULL would bypass dedupe.
- `AlertStateManager.triggered` becomes `DashMap<(String, String, String), TriggeredInfo>` (third element = `event_key`; `""` for legacy metric-based rules, preserving their semantics; `source_ip` for security rules).
- All existing `is_triggered` / `get_info` / `mark_triggered` / `mark_resolved` helpers gain an `event_key: &str` parameter; call sites in the metric evaluator pass `""`, security call sites pass the source IP.

**Push-based trigger flow**:

1. `record_event` (after broadcast) loads `alert_rules` where `enabled=true` and `cover_type/server_ids_json` covers this `server_id`.
2. For each rule's `rules_json`, find the single security `AlertRuleItem` (validator guarantees ≤1), match by `rule_type`, apply `SecurityRuleParams` filter (`min_failed_count`, `min_distinct_ports`, `exclude_users`, `exclude_cidrs`).
3. On hit, mirror the order used by `handle_triggered` (`crates/server/src/service/alert.rs:574`):
   1. Read `alert_state_manager.get_info(&rule_id, &server_id, &source_ip)`.
   2. Compute `should_notify`: `info.is_none() || (now - info.last_notified_at) >= dedupe_window_seconds`.
   3. Call `alert_state_manager.mark_triggered(db, &rule_id, &server_id, &source_ip)` to bump count and update `last_notified_at`.
   4. If `should_notify`, dispatch via `NotificationService::send_group` (`crates/server/src/service/notification.rs:323`) using the rule's `notification_group_id`.

   Reversing read/mark would clobber `last_notified_at` and dedupe every notification away forever.

Relationship to the existing 60s `alert_evaluator`: complementary. Metric-based rules keep running on the 60s loop with `event_key=""`; security rules use push-based triggering with `event_key=source_ip` for low latency.

### 7.4 REST API

```
GET    /api/security/events?server_id=&event_type=&source_ip=&severity=
                            &since=&until=&cursor=&limit=  (default 50, max 200)
GET    /api/security/events/:id
GET    /api/security/stats?server_id=&since=&until=
                          &group_by=event_type|source_ip|day
DELETE /api/security/events/:id   (admin only)
```

- Reads on `read_router`; DELETE on `write_router` + `require_admin`
- `#[utoipa::path]` on every endpoint; DTOs `#[derive(ToSchema)]`
- Responses wrapped as `Json<ApiResponse<T>>` per project convention

### 7.5 Cleanup

No new background task. The existing `cleanup` task gains a step:

```rust
let cutoff = Utc::now() - Duration::days(config.retention.security_event_days as i64);
security_event::Entity::delete_many()
    .filter(security_event::Column::CreatedAt.lt(cutoff))
    .exec(&db).await?;
```

Config: new field `retention.security_event_days: u32` on `RetentionConfig` (`crates/server/src/config.rs:113`), default `30`, following the existing naming convention. Env override: `SERVERBEE_RETENTION__SECURITY_EVENT_DAYS`.

### 7.6 Recovery merge

`recovery_merge::merge_server_history_on_connection` (`crates/server/src/service/recovery_merge.rs:509`) gains a call:

```rust
Self::merge_raw_table_on_connection(
    db,
    "security_event",
    "created_at",
    target_server_id,
    source_server_id,
).await?;
```

This ensures that when a server identity is rebound (e.g., agent reinstall), its security event history follows. Because the merge covers this table, agent writes do not need to bypass `RecoveryLockService`: events written under the source identity during the freeze window will be reconciled to the target identity when recovery completes.

## 8. Frontend (apps/web)

### 8.1 Routes & navigation

New top-level sidebar entry **Security** (icon `ShieldAlert`).

```
apps/web/src/routes/_authed/security/
├── index.tsx       — global timeline across all servers
└── $serverId.tsx   — per-server detail
```

Server detail page (`_authed/servers/$id.tsx`) gains a "Security" tab with the last 50 events and a link to the full timeline.

### 8.2 Overview page layout

- Time range switcher (24h / 7d / 30d / custom)
- KPI cards: Brute Force / Port Scans / New IP Logins / Top Attacker IP
- Stacked bar chart (Recharts) over time, stacked by `event_type`
- Filter bar: server, event_type, severity, source_ip, first_seen toggle
- Data table (TanStack Table) with cursor pagination, wrapped in shadcn `<ScrollArea>` per project rule (no naked `overflow-auto`)

Interactions:
- Row click → Drawer with full evidence JSON and an external link to `https://www.virustotal.com/gui/ip-address/<ip>` (`target="_blank" rel="noopener"`)
- Clicking a `source_ip` cell injects it into the filter bar
- Type badges color-coded: brute_force = red, port_scan = orange, ssh_login = blue (with dot when `first_seen=true`)

### 8.3 Realtime updates

`use-servers-ws.ts` handles `security_event`:

```ts
case 'security_event': {
  queryClient.setQueryData<EventPage>(
    ['security', 'events', filterKey],
    (old) => old ? { ...old, items: [msg.event, ...old.items].slice(0, 200) } : old
  );
  queryClient.invalidateQueries({ queryKey: ['security', 'stats'] });
  if (msg.event.severity === 'high' || msg.event.severity === 'critical') {
    toast.warning(t('security.attack_detected', { ip: msg.event.source_ip }));
  }
}
```

### 8.4 Alert rule UI

`_authed/alerts/` gains three preset cards mirroring the existing `ip_changed` rule form: SSH Brute Force / SSH New IP Login / Port Scan Detected. Each card exposes the relevant `params` knobs + notification group selector.

### 8.5 Types & i18n

- DTOs flow into `api-types.gen.ts` via `bun run generate:api-types`
- WS message types are currently inlined in `apps/web/src/hooks/use-servers-ws.ts:55` (the `WsMessage` union). Add `{ type: 'security_event'; server_id: string; event_id: string; event: SecurityEventPayload }` as a new union arm there. No new types module is created — splitting that union out is a separate refactor and out of scope for this spec.
- `apps/web/src/locales/{en,zh}/security.json` — ~30 strings

## 9. Error Handling & Edge Cases

### 9.1 Agent failure modes

| Failure | Handling | User visibility |
|---|---|---|
| `journalctl` subprocess crashes | 1s → 30s exponential backoff restart, also attempt auth.log fallback | Agent log warn |
| Neither systemd nor auth.log present (minimal containers) | SshDetector disabled; PortScan continues | Agent log info |
| `CAP_NET_ADMIN` missing → netlink bind fails | ConntrackWatcher disabled; SSH continues | Agent log warn |
| `first_seen.json` parse failure | Rename to `.corrupt-<ts>`, rebuild empty | Agent log error |
| `first_seen.json` disk write failure | Memory state continues; 5min backoff on flush; sustained 24h failure escalates to error log | Agent log error |
| In-memory queue overflow (WS down) | Cap 1000 entries; drop oldest; on reconnect, batch send. No synthetic event is emitted (there's no matching `SecurityEventType` / evidence schema and we don't want to dilute attack signal). Record drop count to `agent log warn` + `tracing::counter!("security.events.dropped")`. | Agent log warn + metric |
| Detector state map blowup under DDoS | Per-detector map cap 10000 source IPs; LRU evict; if evicted IP already > 50% of threshold, force-emit | Internal metric |

Principle: partial degradation > full stop. SSH and Scan pipelines are independent.

### 9.2 Server failure modes

| Failure | Handling |
|---|---|
| `SecurityEvent` received without `CAP_SECURITY_EVENTS` | Silent drop + audit_log `security_event_denied` |
| Malformed `source_ip` | Reject before insert, agent WS warn-log, no DB row |
| Evidence JSON deserialize failure | Drop + audit_log `security_event_malformed` |
| `browser_tx.send()` errors (no subscribers) | Ignored (broadcast convention) |
| `alert_trigger` panics | `tokio::spawn` + `catch_unwind` isolates |
| DB write failure | Retry 3× (100ms, 500ms, 2s), then drop + metric `security_event_drop_total` |
| RecoveryLock freeze period | Deliberate carve-out: `security_event` writes proceed during freeze (append-only, no derived state). Rows landing under a source identity are reconciled by `recovery_merge` (see §7.6). |

### 9.3 Rate limit (anti-DoS)

- Per-agent: max 100 `SecurityEvent` per minute (DashMap sliding window)
- Overflow → drop + audit `security_event_rate_limited`
- Pure backstop; agent-side aggregation makes this effectively unreachable in normal operation
- Override: `SERVERBEE_SECURITY__MAX_EVENTS_PER_MINUTE`

### 9.4 IPv6 / private IPs

- Stored as strings (not BLOB) for LIKE search and CSV export
- IPv6 stored fully expanded to avoid `::1` vs `0:0:...:1` mismatches
- Private IPs (10/8, 172.16/12, 192.168/16, 127/8, fe80::/10) are not filtered out; LAN attacks are real. UI default filter has a "public IPs only" toggle, on by default.

### 9.5 Timezone

- Internal timestamps: UTC (`DateTime<Utc>`)
- API responses: ISO 8601 with `Z` suffix
- Frontend renders using `config.scheduler.timezone` or browser locale

## 10. Testing

### 10.1 Unit tests (Rust)

Agent:
- `ssh_parser` — table-driven across Debian/Ubuntu/RHEL/Alpine sshd output, IPv6, long usernames, special chars. Fixtures in `tests/fixtures/sshd_logs/*.txt`.
- `ssh_detector` — injected `Clock` trait to simulate time; verifies threshold trip on **single-user hammering** (one username × `failed_threshold` attempts must fire), severity escalation table (1 user → medium, 2-4 → high, ≥5 → critical), post-trip silence within window, scope of cleanup
- `scan_detector` — same shape; verifies distinct-port threshold, single-port repeat does not trip, sliding window correctness
- `first_seen_store` — load/save/corruption recovery/LRU eviction
- `journal_watcher` — mocked subprocess stdout; verifies fallback switch

Server:
- `record_event` happy path on in-memory sqlite — row persisted, broadcast sent, alert_trigger invoked
- Capability rejection path
- IP format validation
- Evidence deserialize failure

Target: ≥ 80% coverage on new modules (no enforced gate, matches project density).

### 10.2 Integration tests

`crates/server/tests/integration/security.rs`:
- Test server + mock agent WS sends `SecurityEvent`
- Assert: `GET /api/security/events` returns it
- Assert: broadcast channel receives `BrowserMessage::SecurityEvent`
- Assert: configured alert_rule fires + notification dispatched (mock dispatcher)
- Assert: capability-disabled scenario rejects with audit_log row

### 10.3 Manual E2E checklist

New `tests/security-events.md`:
- Lab VPS receives 15 wrong-password SSH attempts → UI shows brute_force within 90s
- `nmap -p 1-1000 <target>` triggers port_scan event
- Legitimate key login from a brand-new IP → ssh_login with `first_seen=true`, `ssh_new_ip_login` alert fires
- Disabling `CAP_SECURITY_EVENTS` on a server stops the agent watcher and UI events
- 10-minute agent offline during an attack → reconnect drains buffered events in order

### 10.4 Frontend tests

- `vitest`: `use-security-events` hook cache merge, filter logic, WS handler dispatch
- No full-page E2E (matches current web test density)

### 10.5 Benchmarks (criterion)

- `ssh_detector` ≥ 100k attempts/s
- `conntrack_watcher` sustains 10k events/s without drops (verified with `nft` injection)
- Server `record_event` P99 < 50ms on in-memory sqlite baseline

## 11. Rollout

Phase ordering (single PR may be too big; split into 3 PRs):

1. **Foundation PR** — protocol additions (capability bit, message variants), `security_event` entity + migration, capability default change. Backwards-compatible: old agents simply never send the new message.
2. **Agent PR** — `crates/agent/src/security/` module, config plumbing, integration with reporter.
3. **Server & UI PR** — `service::security`, REST API, alert rule kinds, frontend pages.

Documentation updates that ship alongside:
- `ENV.md` — `SERVERBEE_SECURITY__MAX_EVENTS_PER_MINUTE` and `SERVERBEE_RETENTION__SECURITY_EVENT_DAYS`
- `apps/docs/content/docs/{en,cn}/configuration.mdx` — same env block
- `apps/docs/content/docs/{en,cn}/` — new "Security Events" page covering detection mechanics, the explicit privilege model (default-on capability vs opt-in `CAP_NET_ADMIN` for conntrack), the systemd-unit edit for scan detection, and false-positive tuning
- `tests/security-events.md` — E2E checklist

## 12. Open Items for Phase 2

Out of scope for v1 but kept reachable:

- **`ServerMessage::SecurityConfigSync` + server-side config store** — push threshold overrides from the control plane. Requires: a new `security_config` table (or rows in existing `config`), CRUD REST endpoints under `/api/security/config`, a Settings page UI, and propagation hook in agent_manager when the server boots / config changes. `security.port_scan.enabled` is excluded from the syncable set.
- `SecurityRollup` agent message — periodic per-window compact stats (top-N offending IPs, blocked-conn counts).
- Cross-server correlation: detect IPs scanning multiple agents from the same fleet.
- IP reputation enrichment (ASN, country, abuse score).
- Notification suppression / event coalescing across rules.
- Whitelist management UI (per-IP / per-CIDR exemptions).
