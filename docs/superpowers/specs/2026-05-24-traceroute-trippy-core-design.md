# Traceroute via trippy-core — Design

**Status:** Design
**Author:** ZingerLittleBee
**Date:** 2026-05-24
**Branch:** montreal

## Background

The current traceroute implementation in the ServerBee agent shells out to the system `traceroute` binary on Linux/macOS (`mtr` fallback) and `tracert` on Windows, then parses stdout with regex. Source: `crates/agent/src/reporter.rs:1356-1500`.

Pain points:

- **External dependency**: most minimal Linux images (Alpine, distroless, several cloud minimal images) do not ship `traceroute`/`mtr`. Users hit `traceroute not installed` errors with no in-product guidance.
- **Limited data**: stdout parsing yields only `{hop, ip, hostname, rtt1, rtt2, rtt3, asn}`. Loss%, jitter, stddev, ECMP multi-IP, ICMP packet type, NAT/MPLS extensions — all unavailable.
- **Inconsistent across platforms**: GNU traceroute / busybox traceroute / mtr / Windows tracert each have distinct output formats; the agent's `parse_traceroute_line` is a best-effort regex that breaks silently on variants.
- **No history**: results live in an in-memory DashMap that vanishes on agent restart. Users cannot compare a run today vs. last week.
- **One-shot result**: the agent blocks up to 60s and posts a single `TracerouteResult` message. There is no incremental progress feedback while the user waits.

## Goals

1. Eliminate the external `traceroute`/`mtr` dependency by embedding [`trippy-core`](https://crates.io/crates/trippy-core) into the agent.
2. Surface richer per-hop diagnostics: loss%, best/avg/worst RTT, jitter, standard deviation, ECMP multi-IP responses.
3. Stream incremental updates round by round so the UI fills hops progressively.
4. Persist completed traceroute results in SQLite so users can view history, delete individual entries, or clear the entire history.
5. Allow the user to choose ICMP / UDP / TCP probe protocol from the dialog.
6. Keep wire compatibility with old agents during the upgrade window; rely on the existing agent self-upgrade mechanism.

## Non-Goals

- TCP traceroute with a custom destination port (defaults only).
- Configurable round count / per-probe timeout (fixed defaults).
- Cross-server fleet-wide traceroute orchestration.
- Auto-cleanup / retention policy for traceroute history (manual delete only in this iteration).
- Per-user attribution of who triggered a trace (records are shared across all users of the server).

## Architecture

```
Browser                Server                                    Agent
                       (AppState, agent_manager, broadcast_tx)   (reporter + new traceroute module)

  POST /api/servers/{id}/traceroute
    { target, protocol }                              ─────►
                                                              ServerMessage::Traceroute {
                                                                request_id, target, max_hops, protocol
                                                              }
   ◄─ 200 { request_id }                                                        ─────────► validate caps + target
                                                                                            │
                                                                                            spawn_blocking(trippy_core::Tracer::run_with)
                                                                                            │
   WS /api/ws/servers                                          ◄────────────────  AgentMessage::TracerouteRoundUpdate {
   ◄── BrowserMessage::TracerouteUpdate {                                            request_id, round, total_rounds,
          server_id, request_id, hops (enriched),              │                     hops, completed
          round, total_rounds, completed                       │                  }   (one per round)
       }                                                       │
   ◄── (next round update)                                     │ TracerouteEnricher.enrich(&mut hops)
   ◄── (… completed=true)                                      │   ↳ ASN from MMDB, hostname via cached PTR
                                                               │ agent_manager.update_traceroute_round(...)
                                                               │   ↳ in-memory cache (for polling fallback)
                                                               │   ↳ on completed=true, INSERT row into traceroute_record
                                                               │ browser_tx.send(BrowserMessage::TracerouteUpdate)

  GET /api/servers/{id}/traceroute/{request_id}     ─────────►
   (fallback for WS disconnect / page refresh)                 ← snapshot from in-memory cache
                                                                  (or DB if completed and evicted)

  GET /api/servers/{id}/traceroute                  ─────────►
                                                                ← Vec<TracerouteRecordSummary> from DB, recent first
  DELETE /api/servers/{id}/traceroute/{request_id}  ─────────►
                                                                ← admin only; deletes one DB row
  DELETE /api/servers/{id}/traceroute               ─────────►
                                                                ← admin only; deletes all rows for this server
```

Streaming is delivered via the existing `broadcast::Sender<BrowserMessage>` channel that already fans out server updates and ping data; HTTP polling on `GET .../traceroute/{request_id}` is kept as a reconnect / refresh fallback.

## Wire Protocol Changes (`crates/common`)

### `TracerouteHop` extension (`types.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct TracerouteHop {
    pub hop: u8,

    // --- Legacy fields filled by the old shell-based agent only ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    pub hostname: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt1: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt2: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtt3: Option<f64>,
    pub asn: Option<String>,

    // --- New fields populated by the trippy-core agent ---
    /// All IPs that responded for this TTL (ECMP). Empty when no response yet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ips: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_sent: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_recv: Option<u32>,
    /// Packet loss as percentage 0.0–100.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loss_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worst_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avg_ms: Option<f64>,
    /// RTT standard deviation across all received probes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stddev_ms: Option<f64>,
    /// Round-trip jitter (difference vs. previous probe).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jitter_ms: Option<f64>,
}
```

Frontend rendering rule: when `ips` is non-empty, treat the hop as new-schema and use the trippy fields; otherwise fall back to `ip` and `rtt1/2/3`.

### `ServerMessage::Traceroute` adds optional protocol

```rust
Traceroute {
    request_id: String,
    target: String,
    max_hops: u8,
    /// "icmp" | "udp" | "tcp"; None defaults to ICMP. Old agents ignore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    protocol: Option<String>,
},
```

### New streaming variant on `AgentMessage`

```rust
TracerouteResult {       // still emitted by old agents; new agents never emit this
    request_id, target, hops, completed, error
},
/// New agent: one update per probe round. `hops` is the FULL accumulated
/// state, not a delta. `completed=true` marks the final update.
TracerouteRoundUpdate {
    request_id: String,
    target: String,
    round: u32,
    total_rounds: u32,
    hops: Vec<TracerouteHop>,
    completed: bool,
    error: Option<String>,
},
```

Full-state-per-round avoids server-side merge logic. Bandwidth budget: a hop serializes to roughly 200 bytes, 30 hops × 5 rounds ≈ 30 KB total.

### New `BrowserMessage` variant

```rust
TracerouteUpdate {
    server_id: String,
    request_id: String,
    target: String,
    round: u32,
    total_rounds: u32,
    /// Server-side enriched (ASN + hostname filled in).
    hops: Vec<TracerouteHop>,
    completed: bool,
    error: Option<String>,
},
```

### Protocol unit tests

`crates/common/src/protocol.rs#tests` adds:

- `test_traceroute_server_message_with_protocol_round_trip`
- `test_traceroute_round_update_round_trip` (intermediate `completed=false` and final `completed=true`)
- `test_browser_message_traceroute_update_round_trip`
- `test_traceroute_hop_legacy_fields_skipped_when_none` (ensures new-field defaults do not pollute JSON emitted by old agents)

## Agent (`crates/agent`)

### Dependency

```toml
# crates/agent/Cargo.toml
trippy-core = "0.13"
```

Binary size impact measured in isolation: roughly +200–300 KB after stripping (Rust dead-code elimination is aggressive; many of trippy-core's deps such as `socket2`, `parking_lot`, `nix` are already pulled in by the existing tokio / dashmap stack).

### New module `crates/agent/src/traceroute.rs`

`reporter.rs` already exceeds 1900 lines. Traceroute logic moves into a dedicated module to keep `reporter.rs` focused on protocol dispatch and to make the trippy integration unit-testable.

```rust
const DEFAULT_MAX_ROUNDS: u32 = 5;
const ROUND_INTERVAL: Duration = Duration::from_millis(1000);
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

pub fn parse_protocol(s: Option<&str>) -> trippy_core::Protocol {
    match s.unwrap_or("icmp").to_ascii_lowercase().as_str() {
        "udp" => Protocol::Udp,
        "tcp" => Protocol::Tcp,
        _    => Protocol::Icmp,
    }
}

pub fn is_valid_traceroute_target(target: &str) -> bool { ... } // moved from reporter.rs

async fn resolve(target: &str) -> Result<IpAddr, String> { ... } // tokio::net::lookup_host

pub fn spawn_traceroute(
    request_id: String,
    target: String,
    max_hops: u8,
    protocol: Option<String>,
    tx: tokio::sync::mpsc::Sender<AgentMessage>,
) { ... }
```

Inside `spawn_traceroute`:

1. Resolve hostname to `IpAddr` (literal IP passes through).
2. `tokio::task::spawn_blocking` to host the synchronous `Tracer::run_with` loop.
3. Build the tracer with `PrivilegeMode::Privileged` first; on failure (typically `Error::InsufficientPrivileges`), rebuild once with `PrivilegeMode::Unprivileged`. Two failures send a `TracerouteRoundUpdate { completed: true, error: Some(...) }` carrying an installation/setcap hint.
4. Maintain a `trippy_core::State` outside the round callback so each callback can call `state.update_from_round(round)` and then snapshot `state.hops()` → `Vec<TracerouteHop>`.
5. Send one `TracerouteRoundUpdate` per round. Mark the last round as `completed=true`.
6. Use `Sender::blocking_send` (we are inside `spawn_blocking`, no `.await` allowed).

### `reporter.rs` changes

Replace the `ServerMessage::Traceroute` arm:

```rust
ServerMessage::Traceroute { request_id, target, max_hops, protocol } => {
    if !has_capability(caps, CAP_PING_ICMP) { /* existing CapabilityDenied path */ }
    if !traceroute::is_valid_traceroute_target(&target) { /* existing invalid-target path */ }
    traceroute::spawn_traceroute(request_id, target, max_hops, protocol, cmd_result_tx.clone());
    Ok(ServerMessageOutcome::Continue)
}
```

Delete `execute_traceroute`, `parse_traceroute_output`, `parse_traceroute_line`. Move `is_valid_traceroute_target` plus its `#[test]` block to `traceroute.rs`.

### Agent tests

`crates/agent/src/traceroute.rs#tests`:

- `test_parse_protocol_defaults_to_icmp`
- `test_parse_protocol_case_insensitive`
- `test_parse_protocol_invalid_falls_back_to_icmp`
- `test_is_valid_traceroute_target` (migrated suite)
- `test_resolve_literal_ipv4` / `test_resolve_literal_ipv6` / `test_resolve_invalid_hostname_returns_err`

The end-to-end tracer integration is not covered by automated tests because raw socket access is unavailable in CI. A manual checklist lives in `tests/traceroute.md` (to be created).

## Server (`crates/server`)

### New module `service/traceroute_enrich.rs`

```rust
pub struct TracerouteEnricher {
    mmdb: Option<Arc<maxminddb::Reader<Vec<u8>>>>,
    ptr_cache: Arc<DashMap<IpAddr, (Option<String>, Instant)>>,
}

impl TracerouteEnricher {
    pub fn new(mmdb: Option<Arc<maxminddb::Reader<Vec<u8>>>>) -> Self;
    pub async fn enrich(&self, hops: &mut [TracerouteHop]);
    async fn ptr_lookup(&self, ip: IpAddr) -> Option<String>;
    fn asn_lookup(&self, ip: IpAddr) -> Option<String>;
}
```

- ASN read uses the same MMDB Arc already loaded in `AppState` for the IP Quality feature (see `geoip.mmdb_path` in `apps/docs/content/docs/{cn,en}/configuration.mdx`).
- PTR lookup uses `tokio::task::spawn_blocking + dns_lookup::lookup_addr` (or `getnameinfo` direct). LRU is approximated with a `DashMap` plus a 1 h TTL and a max cap of 4096 entries; eviction runs piggybacked on inserts (drop oldest when over cap). No extra crate.

### `AppState` wiring

`crates/server/src/state.rs` gains `pub traceroute_enricher: TracerouteEnricher`. Constructed in `AppState::new` using the existing MMDB Arc.

### New table `traceroute_record`

```sql
CREATE TABLE traceroute_record (
    id               TEXT PRIMARY KEY,            -- = request_id (UUID)
    server_id        TEXT NOT NULL,
    target           TEXT NOT NULL,
    protocol         TEXT NOT NULL,               -- 'icmp' | 'udp' | 'tcp'
    started_at       INTEGER NOT NULL,            -- unix ms
    completed_at     INTEGER,                     -- NULL when interrupted
    total_rounds     INTEGER NOT NULL,
    completed_rounds INTEGER NOT NULL,
    hops_json        TEXT NOT NULL,               -- full enriched hops
    error            TEXT,
    FOREIGN KEY (server_id) REFERENCES server(id) ON DELETE CASCADE
);

CREATE INDEX idx_traceroute_record_server_started
    ON traceroute_record(server_id, started_at DESC);
```

Persistence decisions:

- `hops_json` stores the full hop array as JSON. The hop count per record is ≤ 30, hops are always read together, and no per-hop query is needed. A normalized `traceroute_hop` child table would be over-engineered.
- Only rows where the trace ran to completion (or completed with an error) are inserted. In-progress traces live only in the in-memory cache. Closing and reopening the dialog still streams from the cache; only a server restart or agent disconnect drops an in-flight trace.
- `ON DELETE CASCADE` from `server(id)` keeps trace history consistent with server deletions.

sea-orm entity: `crates/server/src/entity/traceroute_record.rs`. Migration: `crates/server/src/migration/m20260524_000001_create_traceroute_record.rs` (final filename will follow the next available numeric suffix in that directory; convention is `mYYYYMMDD_NNNNNN_<name>`). Per CLAUDE.md, only `up()` is implemented; `down()` returns `Ok(())`.

### `agent_manager` changes

`update_traceroute_round` becomes the single entry point invoked by the WS agent handler:

```rust
pub async fn update_traceroute_round(
    &self,
    db: &DatabaseConnection,
    request_id: &str,
    server_id: &str,
    target: &str,
    protocol: &str,
    round: u32,
    total_rounds: u32,
    hops: Vec<TracerouteHop>,
    completed: bool,
    error: Option<String>,
) -> Result<(), AppError>;
```

Behavior:

1. Update the in-memory cache (the HTTP polling endpoint reads this).
2. If `completed == true`, INSERT a row into `traceroute_record`. Mid-rounds do not write.
3. Bookkeeping for cleanup (an existing background `cleanup_traceroute_results` evicts old in-memory entries).

Legacy `AgentMessage::TracerouteResult` from old agents is normalized into the same code path: the WS handler builds a synthetic `TracerouteRoundUpdate` with `round = 1, total_rounds = 1, completed = true` and reuses the same enricher + persistence + broadcast path. The browser only needs to handle one BrowserMessage variant.

### New `service/traceroute.rs`

```rust
pub async fn list_records_for_server(
    db: &DatabaseConnection,
    server_id: &str,
    limit: u64,
    offset: u64,
) -> Result<Vec<TracerouteRecordSummary>, AppError>;

pub async fn get_record_detail(
    db: &DatabaseConnection,
    request_id: &str,
) -> Result<TracerouteRecordDetail, AppError>;

pub async fn delete_record(
    db: &DatabaseConnection,
    request_id: &str,
) -> Result<(), AppError>;

pub async fn delete_records_for_server(
    db: &DatabaseConnection,
    server_id: &str,
) -> Result<u64, AppError>;

pub async fn insert_completed_record(
    db: &DatabaseConnection,
    record: NewTracerouteRecord,
) -> Result<(), AppError>;
```

### API endpoints

| Method | Path | Behavior | Auth |
|--------|------|----------|------|
| `POST` | `/api/servers/{id}/traceroute` | Trigger a trace; returns `request_id`. Existing. | Authenticated |
| `GET` | `/api/servers/{id}/traceroute/{request_id}` | Latest snapshot (in-memory cache; falls back to DB if evicted). Existing endpoint, response shape unchanged. | Authenticated |
| `GET` | `/api/servers/{id}/traceroute` | **New**: list `TracerouteRecordSummary[]`, recent first, default limit 50. | Authenticated |
| `DELETE` | `/api/servers/{id}/traceroute/{request_id}` | **New**: delete one record. | Admin |
| `DELETE` | `/api/servers/{id}/traceroute` | **New**: clear all records for this server. | Admin |

DTOs:

```rust
#[derive(Serialize, ToSchema)]
pub struct TracerouteRecordSummary {
    pub request_id: String,
    pub target: String,
    pub protocol: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub hop_count: u32,
    pub has_error: bool,
}

#[derive(Serialize, ToSchema)]
pub struct TracerouteRecordDetail {
    pub request_id: String,
    pub target: String,
    pub protocol: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub hops: Vec<TracerouteHop>,
    pub error: Option<String>,
}
```

The list endpoint returns summaries only; clients fetch full hops via the existing `GET .../traceroute/{request_id}` endpoint when a record is selected.

### Server tests

- `entity/traceroute_record.rs#tests` — JSON serialization round-trip.
- `service/traceroute.rs#tests` — list / get_detail / delete / delete_all using a SeaORM in-memory test DB.
- `service/traceroute_enrich.rs#tests`:
  - `test_enrich_fills_asn_from_mmdb` (fixture MMDB)
  - `test_enrich_skips_when_mmdb_missing`
  - `test_ptr_cache_hit_does_not_relookup`
  - `test_ptr_cache_evicts_after_ttl`
  - `test_enrich_handles_ipv6`
- `crates/server/tests/traceroute_persistence.rs` (integration):
  - POST → simulated agent RoundUpdate stream → GET list contains entry → GET detail → DELETE → list empty.
  - Member token attempting DELETE returns 403.
  - Legacy `TracerouteResult` from a mock old agent is normalized and persisted identically.

## Frontend (`apps/web`)

### Type updates (`lib/network-types.ts`)

```ts
export interface TracerouteHop {
  hop: number
  // legacy
  ip: string | null
  hostname: string | null
  rtt1: number | null
  rtt2: number | null
  rtt3: number | null
  asn: string | null
  // new
  ips: string[]
  total_sent: number | null
  total_recv: number | null
  loss_pct: number | null
  best_ms: number | null
  worst_ms: number | null
  avg_ms: number | null
  stddev_ms: number | null
  jitter_ms: number | null
}

export interface TracerouteResult {
  target: string
  hops: TracerouteHop[]
  completed: boolean
  error: string | null
  round?: number
  total_rounds?: number
}

export interface TracerouteRecordSummary {
  request_id: string
  target: string
  protocol: 'icmp' | 'udp' | 'tcp'
  started_at: number
  completed_at: number | null
  hop_count: number
  has_error: boolean
}
```

The OpenAPI-generated `api-types.ts` follows the backend schema automatically.

### New hooks (`hooks/use-network-api.ts` + `hooks/use-traceroute-stream.ts`)

- `useTracerouteHistory(serverId)` — TanStack Query, paginated.
- `useTracerouteRecord(requestId)` — fetch one full record by id; enabled when `selectedRecordId` is set.
- `useDeleteTraceroute(serverId)` — mutation, invalidates history.
- `useClearTracerouteHistory(serverId)` — mutation, invalidates history.
- `useTracerouteStream(serverId, requestId)` — subscribes to the existing servers WS dispatcher, listens for `traceroute_update` messages filtered by both `server_id` and `request_id`. The WS connection itself is the one already opened in the layout for server updates; this hook only adds a new message type to the in-process dispatcher.

### Dialog layout (`apps/web/src/routes/_authed/network/$serverId.tsx`)

```
┌─ Traceroute ──────────────────────────────────────────────┐
│ [target_______________] [ICMP ▾] [Run]                   │
├───────────────────────────────────────────────────────────┤
│ ── Current / Selected ──                                  │
│ Target: 1.1.1.1   Protocol: ICMP   Round 3 / 5 ⏳         │
│ ┌─────────────────────────────────────────────────────┐  │
│ │Hop│IP │Host│ASN │Loss%│Best│Avg│Worst│Jit │StdDev│  │
│ │ 1 │…  │…   │…   │ 0%  │1.2 │1.5│ 2.1 │0.3 │ 0.2  │  │
│ └─────────────────────────────────────────────────────┘  │
├───────────────────────────────────────────────────────────┤
│ ── History (5) ──                       [Clear all]      │
│ • 1.1.1.1    icmp   10 hops   2 h ago    ✓   [🗑]        │
│ • google.com tcp     8 hops   yesterday  ✓   [🗑]        │
│ • 8.8.8.8    icmp    —        3 d ago    ✗   [🗑]        │
└───────────────────────────────────────────────────────────┘
```

Behavior:

- Default display shows the most recent completed trace (if any) so users see context immediately.
- "Run" starts a new stream; the current section switches to live updates.
- Clicking a history row fetches that record's detail and shows it.
- "Clear all" / per-row trash icons require admin role; they are hidden for member users (`useAuth().user.role`).
- Mutations invalidate the history query, so new completed traces appear immediately.

Active stream vs. selected record state lives in the parent route component:

```tsx
const [traceTarget, setTraceTarget] = useState('')
const [traceProtocol, setTraceProtocol] = useState<'icmp' | 'udp' | 'tcp'>('icmp')
const [traceRequestId, setTraceRequestId] = useState<string | null>(null)
const [selectedRecordId, setSelectedRecordId] = useState<string | null>(null)
// Mutually exclusive: starting a new run clears selectedRecordId;
// picking a history row clears traceRequestId.
```

This keeps the trace running when the user closes the dialog, and lets them reopen to keep watching.

### Table rendering rules

| Column | Source |
|--------|--------|
| Hop | `hop` |
| IP | `ips[0]` + `+N` chip when `ips.length > 1`; tooltip shows the rest. Fallback to `ip ?? '* * *'`. |
| Host | `hostname ?? '—'` |
| ASN | `asn ?? '—'` |
| Loss% | `loss_pct ?? compute_from(rtt1/2/3)` ; coloured via existing `getLossTextClassName` |
| Best | `best_ms ?? min(rtt1,rtt2,rtt3)` |
| Avg | `avg_ms ?? avg(rtt1,rtt2,rtt3)` ; coloured via existing `latencyColorClass` |
| Worst | `worst_ms ?? max(rtt1,rtt2,rtt3)` |
| Jitter | `jitter_ms ?? '—'` |
| StdDev | `stddev_ms ?? '—'` |

Rows where `total_recv === 0` or all RTTs are null render dimmed with `* * *` in the IP column.

### Progress indicator

The dialog header shows `Round {{current}} / {{total}}` only while `completed === false`. The button still spins; the previous "Traceroute running…" centered placeholder is removed because the streaming table already provides progress feedback.

### Frontend tests

- `use-traceroute-stream.ts` — vitest with a stubbed BrowserMessage dispatcher, asserting requestId filtering and the completed transition.
- `TracerouteContent` rendering tests:
  - Legacy hops (only `rtt1/2/3`) render best / avg / worst correctly.
  - New-schema hops prefer the new fields.
  - `ips.length > 1` renders the `+N` chip.
  - Empty history renders an empty-state message.
  - Clicking a history row swaps the displayed record.
  - Non-admin user does not see delete buttons.
  - `error` non-null renders the error banner instead of the table body.

## Capabilities

`CAP_PING_ICMP` continues to gate all three probe protocols (ICMP/UDP/TCP). Rationale: traceroute is a diagnostic operation closer in nature to ping than to active probing, and the existing default capability set already includes this bit. Adding new capability bits would complicate UI defaults and migrate semantics for marginal benefit.

If we later want operators to disable traceroute independently of ping, a `CAP_TRACEROUTE` bit can be added without protocol-level changes — `is_valid_traceroute_target` already runs after the capability check.

## Privilege model

trippy-core attempts `PrivilegeMode::Privileged` first (raw ICMP/UDP/TCP sockets via `CAP_NET_RAW` or setuid). On `Error::InsufficientPrivileges`, the agent rebuilds the tracer with `PrivilegeMode::Unprivileged` (Linux ICMP datagram sockets via `net.ipv4.ping_group_range`, macOS equivalent). Two failures send back a final `TracerouteRoundUpdate` whose `error` field carries platform-specific guidance (e.g., `Run agent as root or apply: sudo setcap cap_net_raw+ep $(which serverbee-agent)` on Linux).

No restart needed when the capability is granted — the next traceroute request retries from scratch.

## Backward compatibility

Old agents (still running the shell implementation):

- Continue sending `AgentMessage::TracerouteResult` with `rtt1/2/3 / ip / hostname / asn` populated.
- Server handler normalizes the old payload into a synthetic `TracerouteRoundUpdate { round: 1, total_rounds: 1, completed: true }`, runs it through the same enricher + persistence + broadcast pipeline.
- Browser receives a single `TracerouteUpdate` for old agents and renders using the legacy field fallbacks in the table.
- History records from old agents are persisted identically; the new fields are simply absent in the JSON.

New agents always send `TracerouteRoundUpdate` and never `TracerouteResult`. The old variant remains in the protocol enum forever (or until a major-version protocol break).

The `protocol` field on `ServerMessage::Traceroute` is optional; old agents ignore it and default to ICMP, which matches their current behavior.

## Documentation

`apps/docs/content/docs/{cn,en}/monitoring.mdx` — Traceroute subsection:

- Add the ICMP / UDP / TCP protocol selector.
- Document the persistent history feature (default-on; admin can delete or clear).
- Remove the previous note that traceroute / mtr must be installed on the agent.
- Add a small troubleshooting block on raw socket privileges (setcap / root) for hosts where unprivileged mode is not available.

`apps/docs/content/docs/{cn,en}/admin.mdx` — Capabilities note: clarify that `CAP_PING_ICMP` continues to gate traceroute including TCP/UDP modes.

`apps/docs/content/docs/{cn,en}/configuration.mdx` — No changes (no new config keys).

## Testing strategy summary

| Layer | Tests |
|-------|-------|
| Wire (`common`) | Round-trip serialization for new message variants, legacy-field skip. |
| Agent | Pure functions (protocol parsing, target validation, hostname resolution). End-to-end trace not in CI (raw socket). |
| Server | sea-orm entity round-trip, service layer CRUD, enricher with MMDB fixture, integration test for POST → stream → list → delete → 403 for member, legacy `TracerouteResult` normalization. |
| Frontend | `useTracerouteStream` dispatcher, `TracerouteContent` rendering with both schemas, role-based delete button visibility. |
| Manual | `tests/traceroute.md` checklist: privileged + unprivileged modes, all three protocols, history persistence across server restart, capability denied, invalid target. |

## Migration & rollout

1. Land protocol changes (`common`) — purely additive, deploy in any order.
2. Land server changes (migration + endpoints + enricher + history). Old agents continue to work via the normalization path; history records start accruing for traces from any agent version.
3. Land agent changes (trippy-core swap). Agents auto-upgrade per the existing pinned-source mechanism.
4. Land frontend changes; the UI gracefully renders both legacy and new-schema hops, so it can ship before all agents have upgraded.

No DB backfill needed. The migration creates an empty `traceroute_record` table.

## Open questions

None.

## References

- trippy-core crate: `https://crates.io/crates/trippy-core` (v0.13.0, Apache-2.0, ~290 KB / 9800 LOC of Rust).
- Project: `https://github.com/fujiapple852/trippy`.
- Current shell-based implementation: `crates/agent/src/reporter.rs:1356-1500`.
- Current API: `crates/server/src/router/api/traceroute.rs`.
- Existing browser WS dispatcher pattern: `apps/web/src/hooks/use-servers-ws.ts`.
- Capability constants: `crates/common/src/constants.rs`.
