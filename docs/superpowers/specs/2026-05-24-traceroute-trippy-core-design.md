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
   ◄── (… completed=true)                                      │   ↳ hostname via cached PTR (ASN deferred)
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

Frontend rendering rule: a hop is new-schema when `hop.total_sent != null` (the loose `!=` is intentional — `Option::None` in Rust with `skip_serializing_if = "Option::is_none"` is **omitted** from the JSON entirely, so in the browser the field is `undefined`, not `null`; loose `!=` catches both). `loss_pct != null` is an equivalent check. These fields are populated by trippy-core for every hop the tracer touched, even when there are zero responses (`ips: []`, 100% loss). Using `ips.length === 0` as the discriminator would mis-classify a fully-lost hop from a new agent as legacy. When new-schema, render from the trippy fields; otherwise fall back to `ip` and `rtt1/2/3`.

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
    /// Server-side enriched (hostname filled in; ASN is deferred — see Server section).
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

1. **Resolve** hostname to `IpAddr` (literal IP parses first; if it fails, fall back to `tokio::net::lookup_host((target, 0u16))` and take the first address). Resolution runs on tokio because `lookup_host` is async — perform it **before** entering `spawn_blocking`.
2. **Enter** `tokio::task::spawn_blocking` for the synchronous tracer lifecycle.
3. **Build + run with two-stage privilege fallback.** Both `Builder::build()` and `Tracer::run_with()` can fail with privilege errors — `build()` does config validation and may trigger a probe socket open early on some platforms, but actual `sendto`/`recvfrom` privilege errors typically surface inside `run_with`. The retry must wrap both:

   ```rust
   fn try_trace(addr, max_hops, proto, priv_mode) -> Result<(), trippy_core::Error> {
       let tracer = Builder::new(addr)
           .max_ttl(max_hops)
           .max_rounds(Some(DEFAULT_MAX_ROUNDS as usize))   // ← what stops run_with
           .min_round_duration(ROUND_INTERVAL)
           .max_round_duration(ROUND_INTERVAL * 2)
           .read_timeout(PROBE_TIMEOUT)
           .protocol(proto)
           .privilege_mode(priv_mode)
           .build()?;
       tracer.run_with(|round| { /* see step 4 */ })
   }

   match try_trace(addr, max_hops, proto, PrivilegeMode::Privileged) {
       Ok(()) => {},
       Err(e) if is_privilege_error(&e) => {
           match try_trace(addr, max_hops, proto, PrivilegeMode::Unprivileged) {
               Ok(()) => {},
               Err(e2) => emit_terminal_error(platform_guidance(e2)),
           }
       }
       Err(e) => emit_terminal_error(format!("{e}")),
   }
   ```

   Per [trippy-core 0.13's Error enum](https://docs.rs/trippy-core/0.13.0/trippy_core/enum.Error.html), the variants relevant to privilege failures are:

   ```rust
   pub enum Error {
       // ...
       IoError(std::io::Error),
       ProbeFailed(std::io::Error),
       PrivilegeError(trippy_privilege::Error),
       // ...
   }
   ```

   `is_privilege_error` matches:

   ```rust
   fn is_privilege_error(e: &trippy_core::Error) -> bool {
       use trippy_core::Error::*;
       use std::io::ErrorKind::PermissionDenied;
       match e {
           PrivilegeError(_) => true,
           IoError(io) | ProbeFailed(io) => io.kind() == PermissionDenied,
           _ => false,
       }
   }
   ```

   `trippy_privilege::Error` variants are inspected at implementation time and emitted via `Display` into the user-facing error string; no need to pattern-match its inner variants for the retry decision (any privilege-family error triggers the unprivileged retry).
4. **Stop condition lives on the builder**, not on the callback. `Tracer::run_with`'s callback is `Fn(&Round)` and **cannot signal stop**. Use `.max_rounds(Some(DEFAULT_MAX_ROUNDS as usize))` so the tracer exits after N rounds and `run_with` returns. The callback only emits messages.
5. **State accumulation via interior mutability.** Wrap `trippy_core::State` in `std::cell::RefCell<State>` (the closure runs on the single blocking thread, so `RefCell` is sufficient; no `Mutex` needed). Also wrap the round counter in `std::cell::Cell<u32>`. Each callback invocation:
   - `state.borrow_mut().update_from_round(round)`
   - Read `state.borrow().hops()` → snapshot into `Vec<TracerouteHop>`
   - Increment `round_no` via `Cell::set(round_no.get() + 1)`
   - Build a `TracerouteRoundUpdate` with `completed = (round_no.get() >= DEFAULT_MAX_ROUNDS)` (this flag only marks the message; trippy stops on its own when `max_rounds` is hit)
   - `tx.blocking_send(msg)` — `Sender::blocking_send` because we are inside `spawn_blocking`.
6. **Terminal error path.** Both `try_trace` failures emit one final `TracerouteRoundUpdate { round: 0, total_rounds: 0, completed: true, hops: vec![], error: Some(...) }` and return. The server then persists this row with the error string so the user sees it in history.

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

This iteration enriches **hostname only**. ASN enrichment is out of scope.

```rust
pub struct TracerouteEnricher {
    ptr_cache: Arc<DashMap<IpAddr, (Option<String>, Instant)>>,
}

impl TracerouteEnricher {
    pub fn new() -> Self;
    pub async fn enrich(&self, hops: &mut [TracerouteHop]);
    async fn ptr_lookup(&self, ip: IpAddr) -> Option<String>;
}
```

- **PTR lookup** uses `tokio::task::spawn_blocking + dns_lookup::lookup_addr` (or `getnameinfo` direct). LRU is approximated with a `DashMap` plus a 1 h TTL and a max cap of 4096 entries; eviction runs piggybacked on inserts (drop oldest when over cap). No extra crate.
- **ASN is deferred.** The existing `GeoIpService` is country/region only (loaded from `dbip-country-lite.mmdb`); it does not contain ASN data. The IP Quality service has ASN, but only for the agent's own outbound IP — it cannot answer ASN for arbitrary hop IPs along a traceroute path. Properly populating ASN requires either:
  - Adding a separate `dbip-asn-lite.mmdb` (or equivalent) plus a config key, loader, and downloader, or
  - Calling an external whois / Team Cymru DNS service per hop, with caching and rate limits.

  Both options are real work and out of scope for this iteration. The `asn` wire field stays in `TracerouteHop` (always `None` from the new agent path; old agents may have parsed it from `traceroute` stdout but it's unreliable). The UI shows `—` in the ASN column. A follow-up spec can add proper ASN lookup.

### `AppState` wiring

`crates/server/src/state.rs` gains `pub traceroute_enricher: TracerouteEnricher`. Constructed in `AppState::new` with `TracerouteEnricher::new()`.

### New table `traceroute_record`

```sql
CREATE TABLE traceroute_record (
    id               TEXT PRIMARY KEY,            -- = request_id (UUID)
    server_id        TEXT NOT NULL,
    target           TEXT NOT NULL,
    protocol         TEXT NOT NULL,               -- 'icmp' | 'udp' | 'tcp' | 'legacy'
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

The protocol is decided server-side at POST time and is **never** echoed back by the agent. `insert_traceroute_placeholder` is extended to cache the full request metadata; the WS handler later joins agent updates with this cached metadata when persisting.

```rust
pub struct TracerouteRequestMeta {
    pub server_id: String,
    pub target: String,
    pub protocol: String,        // 'icmp' | 'udp' | 'tcp'
    pub started_at: i64,         // unix ms, captured at POST
}

impl AgentManager {
    pub fn insert_traceroute_placeholder(
        &self,
        request_id: &str,
        meta: TracerouteRequestMeta,
    );

    /// Apply one round of trippy data to the in-memory cache, and on
    /// completed=true persist the run via the cached metadata.
    pub async fn update_traceroute_round(
        &self,
        db: &DatabaseConnection,
        request_id: &str,
        round: u32,
        total_rounds: u32,
        hops: Vec<TracerouteHop>,
        completed: bool,
        error: Option<String>,
    ) -> Result<(), AppError>;
}
```

Behavior:

1. Look up the cached `TracerouteRequestMeta` by `request_id`. If absent (placeholder expired or the agent sent a stale update), drop the message with a warning.
2. Update the in-memory cache slot for this request (target, protocol, latest hops, completed flag). The HTTP polling endpoint reads from this cache.
3. If `completed == true`, INSERT a row into `traceroute_record` using metadata + the final hops. Mid-rounds do not write.
4. Bookkeeping for cleanup (an existing background `cleanup_traceroute_results` evicts old in-memory entries).

Legacy `AgentMessage::TracerouteResult` from old agents is normalized into the same code path: the WS handler builds a synthetic `TracerouteRoundUpdate { round: 1, total_rounds: 1, completed: true }` and reuses the same enricher + persistence + broadcast path.

The persisted `protocol` for the legacy path is set to the sentinel string `"legacy"`, **not** the protocol the user picked and **not** a guessed value. Reasoning: old agents shell out to `traceroute -n -m N` (which is UDP by default on Unix), then fall back to `mtr` (ICMP), with `tracert` (ICMP) on Windows. The old agent doesn't report which of these actually ran, so the server cannot know — labeling it `"icmp"` would be wrong on every Linux/macOS host where `traceroute` is installed. The browser renders `"legacy"` records with a distinct chip and tooltip ("Traceroute from a pre-trippy agent; actual probe protocol unknown"). Once the agent self-upgrades, all new records carry the real selected protocol.

`TracerouteRequestMeta.protocol` in the cache keeps the user's requested value (for in-flight display). Only the persisted DB row uses the `"legacy"` sentinel when normalizing from `TracerouteResult`.

### New `service/traceroute.rs`

```rust
pub async fn list_records_for_server(
    db: &DatabaseConnection,
    server_id: &str,
    limit: u64,
    offset: u64,
) -> Result<Vec<TracerouteRecordSummary>, AppError>;

/// Both `server_id` and `request_id` are required so the WHERE clause
/// enforces the path-supplied server scope. Returns NotFound if the
/// record exists under a different server.
pub async fn get_record_detail(
    db: &DatabaseConnection,
    server_id: &str,
    request_id: &str,
) -> Result<TracerouteRecordDetail, AppError>;

pub async fn delete_record(
    db: &DatabaseConnection,
    server_id: &str,
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

All scoped reads/writes filter on `server_id` so a stray UUID from one server's history cannot be fetched or deleted via another server's route. Drift prevention, not anti-guessing security.

### API endpoints

| Method | Path | Behavior | Auth |
|--------|------|----------|------|
| `POST` | `/api/servers/{id}/traceroute` | Trigger a trace; returns `request_id`. Existing. | **Admin** (unchanged from current `write_router` placement under `require_admin`) |
| `GET` | `/api/servers/{id}/traceroute/{request_id}` | Latest snapshot (in-memory cache; falls back to DB if evicted). Existing endpoint — response shape **extended** to include `protocol`, `started_at`, `completed_at` so a record selected in the UI carries its provenance and the "legacy" tooltip is not lost when switching from history list to detail view. | Authenticated |
| `GET` | `/api/servers/{id}/traceroute` | **New**: list `TracerouteRecordSummary[]`, recent first, default limit 50. | Authenticated |
| `DELETE` | `/api/servers/{id}/traceroute/{request_id}` | **New**: delete one record. | Admin |
| `DELETE` | `/api/servers/{id}/traceroute` | **New**: clear all records for this server. | Admin |

POST stays admin-only because it causes the agent to send raw probe packets to an arbitrary target — that is an outbound side effect against external infrastructure and reasonably belongs under operator authority. Members can read history (read-only diagnostic forensics) and trigger no network activity. If we later want member-initiated traceroutes, that needs an explicit security review, a rate limit per (user, server), and audit logging — out of scope for this spec.

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
  - `test_ptr_cache_hit_does_not_relookup`
  - `test_ptr_cache_evicts_after_ttl`
  - `test_ptr_cache_evicts_oldest_when_at_cap`
  - `test_enrich_handles_ipv6`
  - `test_enrich_leaves_asn_field_none` (regression guard so a future ASN fix doesn't silently drop the field)
- `crates/server/tests/traceroute_persistence.rs` (integration):
  - POST → simulated agent RoundUpdate stream → GET list contains entry → GET detail → DELETE → list empty.
  - Member token attempting DELETE returns 403.
  - Legacy `TracerouteResult` from a mock old agent is normalized and persisted identically.

## Frontend (`apps/web`)

### Type updates (`lib/network-types.ts`)

```ts
// New fields are marked `?: T | null` because Rust serializes Option::None
// with `skip_serializing_if = "Option::is_none"`, so the key is OMITTED
// from old-agent JSON. Treating them as `T | null` only would let
// `total_sent === null` pass on a legacy hop (where the key is undefined).
// The API client may optionally normalize undefined to null at the boundary;
// either way, all consumers MUST use loose `value != null` checks.
export interface TracerouteHop {
  hop: number
  // legacy
  ip?: string | null
  hostname: string | null
  rtt1?: number | null
  rtt2?: number | null
  rtt3?: number | null
  asn: string | null
  // new (all optional; absent from old-agent payloads)
  ips?: string[]
  total_sent?: number | null
  total_recv?: number | null
  loss_pct?: number | null
  best_ms?: number | null
  worst_ms?: number | null
  avg_ms?: number | null
  stddev_ms?: number | null
  jitter_ms?: number | null
}

export interface TracerouteResult {
  target: string
  /** 'legacy' = run by a pre-trippy agent; actual probe protocol unknown */
  protocol: 'icmp' | 'udp' | 'tcp' | 'legacy'
  started_at: number
  completed_at: number | null
  hops: TracerouteHop[]
  completed: boolean
  error: string | null
  round?: number
  total_rounds?: number
}

export interface TracerouteRecordSummary {
  request_id: string
  target: string
  /** 'legacy' = run by a pre-trippy agent; actual probe protocol unknown */
  protocol: 'icmp' | 'udp' | 'tcp' | 'legacy'
  started_at: number
  completed_at: number | null
  hop_count: number
  has_error: boolean
}
```

The OpenAPI-generated `api-types.ts` follows the backend schema automatically.

### New hooks (`hooks/use-network-api.ts` + `hooks/use-traceroute-stream.ts`)

All hooks pass `serverId` so the underlying API calls hit the server-scoped routes; this matches the backend `service::traceroute` helpers, which take `server_id` alongside `request_id`.

- `useTracerouteHistory(serverId)` — TanStack Query, paginated. `GET /api/servers/{serverId}/traceroute`.
- `useTracerouteRecord(serverId, requestId)` — fetch one full record by id; enabled when `selectedRecordId != null`. `GET /api/servers/{serverId}/traceroute/{request_id}`. Returns the extended response shape (includes `protocol`, `started_at`, `completed_at`).
- `useDeleteTraceroute(serverId)` — mutation, takes `requestId` at call time, invalidates history.
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
- **Admin-only controls** (hidden for members via `useAuth().user.role !== 'admin'`):
  - The entire Run form (target input + protocol selector + Run button) — POST is admin-only on the server (matches existing `write_router` placement under `require_admin`); members would only hit a dead 403 path, so the form should not be rendered at all.
  - Per-row trash icons.
  - "Clear all" button.
- Members see the dialog title, the currently-selected record's hop table (or empty state), and the history list — strictly read-only. The header shows a small "read-only" note when admin controls are hidden so members understand why there's no Run form.
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

A helper `isNewSchema(hop)` returns `hop.total_sent != null` (loose `!=` so both missing JSON keys and explicit `null` are treated as legacy). Branch per-hop, not per-record (in theory all hops of a single response come from the same agent, but per-hop is the safer rule and free).

| Column | New schema (`isNewSchema === true`) | Legacy fallback |
|--------|------------------------------------|-----------------|
| Hop | `hop` | `hop` |
| IP | `ips[0]` + `+N` chip when `ips.length > 1`; tooltip shows the rest. When `ips.length === 0`, render `* * *` (no response). | `ip ?? '* * *'` |
| Host | `hostname ?? '—'` | same |
| ASN | `'—'` (deferred; see *Server* section) | `asn ?? '—'` (may be set by old shell-parsed agents but unreliable) |
| Loss% | `loss_pct?.toFixed(0) + '%'`; coloured via existing `getLossTextClassName` | computed from `rtt1/2/3` (count of nulls / 3) |
| Best | `best_ms` | `min(rtt1, rtt2, rtt3)` filtering nulls |
| Avg | `avg_ms`; coloured via existing `latencyColorClass` | `avg(rtt1, rtt2, rtt3)` filtering nulls |
| Worst | `worst_ms` | `max(rtt1, rtt2, rtt3)` filtering nulls |
| Jitter | `jitter_ms ?? '—'` | `—` (always unavailable) |
| StdDev | `stddev_ms ?? '—'` | `—` (always unavailable) |

Rows where new-schema `total_recv === 0`, or where legacy has all three RTTs null, render the row dimmed.

### Progress indicator

The dialog header shows `Round {{current}} / {{total}}` only while `completed === false`. The button still spins; the previous "Traceroute running…" centered placeholder is removed because the streaming table already provides progress feedback.

### Frontend tests

- `use-traceroute-stream.ts` — vitest with a stubbed BrowserMessage dispatcher, asserting requestId filtering and the completed transition.
- `TracerouteContent` rendering tests:
  - Legacy hops (only `rtt1/2/3`, `total_sent == null` — covers both omitted key and explicit null) render best / avg / worst correctly.
  - New-schema hops (`total_sent != null`) prefer the new fields.
  - **A new-schema hop with `ips: []` and 100% loss still renders as new-schema** (regression guard for the discriminator fix — must not silently fall back to legacy and lose `loss_pct` / `total_sent` display).
  - `ips.length > 1` renders the `+N` chip and tooltip lists the rest.
  - Empty history renders an empty-state message.
  - Clicking a history row swaps the displayed record.
  - Non-admin user does not see the Run form, delete buttons, or "Clear all" — only the selected record's table and the history list. The header shows the read-only note.
  - `error` non-null renders the error banner instead of the table body.

## Capabilities

`CAP_PING_ICMP` continues to gate all three probe protocols (ICMP/UDP/TCP). Rationale: traceroute is a diagnostic operation closer in nature to ping than to active probing, and the existing default capability set already includes this bit. Adding new capability bits would complicate UI defaults and migrate semantics for marginal benefit.

If we later want operators to disable traceroute independently of ping, a `CAP_TRACEROUTE` bit can be added without protocol-level changes — `is_valid_traceroute_target` already runs after the capability check.

## Privilege model

Per [Trippy's privilege guide](https://trippy.rs/guides/privileges/), `PrivilegeMode::Unprivileged` is currently supported on **macOS only**. Linux and Windows always require elevated privileges:

| Platform | Privileged path | Unprivileged path |
|----------|-----------------|-------------------|
| Linux | `CAP_NET_RAW` capability or root | **Not supported.** ICMP datagram sockets via `ping_group_range` are an ICMP-only escape hatch that does not implement Trippy's tracing semantics. |
| macOS | Root | Supported (ICMP datagram sockets) |
| Windows | Administrator | Not supported |

Fallback behavior:

1. The agent first attempts `Builder::build()` with `PrivilegeMode::Privileged`.
2. On any privilege-family error (`Error::PrivilegeError(_)` or `Error::IoError(io)` / `Error::ProbeFailed(io)` where `io.kind() == PermissionDenied` — see `is_privilege_error` in the *Agent* section), the agent retries once with `PrivilegeMode::Unprivileged`. This second attempt only succeeds on macOS in practice; on Linux/Windows it will also fail.
3. After both attempts fail, the agent emits one final `TracerouteRoundUpdate` with `completed: true` and an `error` string that carries platform-aware guidance:
   - Linux: `Traceroute requires elevated privileges. Run the agent as root, or grant CAP_NET_RAW: sudo setcap cap_net_raw+ep $(which serverbee-agent)`
   - Windows: `Traceroute requires Administrator privileges. Restart the agent as Administrator.`
   - macOS: same as Linux but with `sudo` only (setcap is not applicable).

**Implication for documentation**: embedding trippy-core removes the runtime dependency on `traceroute`/`mtr`/`tracert` binaries, but does **not** remove the raw-socket privilege requirement on Linux/Windows. The deployment story changes from "install traceroute + (most distros come with setuid)" to "the agent itself needs CAP_NET_RAW once" — a one-time setup that the install script can automate.

No restart needed when the capability is granted — the next traceroute request retries from scratch (each request builds its own tracer).

## Backward compatibility

Old agents (still running the shell implementation):

- Continue sending `AgentMessage::TracerouteResult` with `rtt1/2/3 / ip / hostname / asn` populated.
- Server handler normalizes the old payload into a synthetic `TracerouteRoundUpdate { round: 1, total_rounds: 1, completed: true }`, runs it through the same enricher + persistence + broadcast pipeline.
- Browser receives a single `TracerouteUpdate` for old agents and renders using the legacy field fallbacks in the table.
- History records from old agents are persisted identically; the new fields are simply absent in the JSON.

New agents always send `TracerouteRoundUpdate` and never `TracerouteResult`. The old variant remains in the protocol enum forever (or until a major-version protocol break).

The `protocol` field on `ServerMessage::Traceroute` is optional; old agents ignore it and run their platform's legacy shell implementation (which is **UDP** by default on Unix when `traceroute` is installed, **ICMP** when only `mtr` is available, and **ICMP** for Windows `tracert`). The server cannot infer which one actually ran, so persisted records from the legacy path use the `"legacy"` protocol sentinel rather than the user's requested value or a guess.

## Documentation

`apps/docs/content/docs/{cn,en}/monitoring.mdx` — Traceroute subsection:

- Add the ICMP / UDP / TCP protocol selector.
- Document the persistent history feature (default-on; admin can delete or clear).
- Remove the previous "Agent tries `traceroute` then `mtr`" note — the agent now uses an embedded library and no external binary is needed.
- Add a privilege block that states the actual platform matrix from the *Privilege model* section: Linux/Windows need root or `CAP_NET_RAW` (Administrator on Windows); macOS supports unprivileged mode. Include the `setcap` one-liner.
- Note: removing the binary dependency does **not** remove the raw-socket privilege requirement on Linux/Windows. This is the most likely source of post-upgrade tickets, so call it out clearly.

`apps/docs/content/docs/{cn,en}/admin.mdx` — Capabilities note: clarify that `CAP_PING_ICMP` continues to gate traceroute including TCP/UDP modes.

`apps/docs/content/docs/{cn,en}/configuration.mdx` — No changes (no new config keys).

## Testing strategy summary

| Layer | Tests |
|-------|-------|
| Wire (`common`) | Round-trip serialization for new message variants, legacy-field skip. |
| Agent | Pure functions (protocol parsing, target validation, hostname resolution). End-to-end trace not in CI (raw socket). |
| Server | sea-orm entity round-trip, service layer CRUD (server-scoped methods), enricher PTR cache TTL/eviction behavior, integration test for POST → stream → list → delete (admin) + member-token POST/DELETE → 403, legacy `TracerouteResult` normalization persists with protocol="legacy". |
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
