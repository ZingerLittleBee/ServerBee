# Traceroute via trippy-core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the shell-based traceroute path in ServerBee's agent with an embedded trippy-core implementation that streams per-round updates, persists completed runs in SQLite, lets the user pick ICMP/UDP/TCP, and surfaces richer per-hop diagnostics (loss%, jitter, stddev, ECMP multi-IP).

**Architecture:** Agent uses `trippy_core::Tracer::run_with` on a `spawn_blocking` thread with a privileged → unprivileged fallback; emits `AgentMessage::TracerouteRoundUpdate` once per round. Server caches request metadata at POST time (`TracerouteRequestMeta`), enriches hops with cached PTR lookups, persists on `completed=true` into a new `traceroute_record` table, and fans out `BrowserMessage::TracerouteUpdate` over the existing browser WebSocket. Frontend dialog gains a protocol dropdown, a 10-column streaming hop table, and a history list with admin-gated delete/clear.

**Tech Stack:** Rust (workspace: `common`, `server`, `agent`), trippy-core 0.13, sea-orm + SQLite, axum, dns_lookup, React 19 + TanStack Query + shadcn/ui.

**Spec:** `docs/superpowers/specs/2026-05-24-traceroute-trippy-core-design.md`

---

## File Map

**`crates/common/src/types.rs`** — Extend `TracerouteHop` with `ips`, `total_sent/recv`, `loss_pct`, `best/worst/avg_ms`, `stddev_ms`, `jitter_ms` (all Option / default empty).

**`crates/common/src/protocol.rs`** — Add `TraceProtocol` + `RecordedProtocol` enums. Add optional `protocol: Option<TraceProtocol>` field to `ServerMessage::Traceroute`. Add `AgentMessage::TracerouteRoundUpdate` variant. Add `BrowserMessage::TracerouteUpdate` variant carrying protocol + started_at. Round-trip tests.

**`crates/server/src/migration/m20260524_000032_create_traceroute_record.rs`** *(new)* — sea-orm migration creating the `traceroute_record` table with CHECK constraint on protocol.

**`crates/server/src/migration/mod.rs`** — Register new migration.

**`crates/server/src/entity/traceroute_record.rs`** *(new)* — sea-orm entity with typed `protocol` column mapping to `RecordedProtocol`.

**`crates/server/src/entity/mod.rs`** — Register entity module.

**`crates/server/Cargo.toml`** — Add `dns_lookup = "2"`.

**`crates/server/src/service/traceroute_enrich.rs`** *(new)* — `TracerouteEnricher` with PTR LRU cache + TTL.

**`crates/server/src/service/traceroute.rs`** *(new)* — DB CRUD: list / get_detail / delete / delete_all / insert_completed.

**`crates/server/src/service/agent_manager.rs`** — Replace `TracerouteResultData` shape; introduce `TracerouteRequestMeta`. Modify `insert_traceroute_placeholder` signature. Add `update_traceroute_round`. `get_traceroute_result` returns the new snapshot shape.

**`crates/server/src/service/mod.rs`** — Register `traceroute` and `traceroute_enrich` modules.

**`crates/server/src/state.rs`** — Add `traceroute_enricher: TracerouteEnricher` field, construct in `AppState::new`.

**`crates/server/src/router/ws/agent.rs`** — Add `AgentMessage::TracerouteRoundUpdate` handler that enriches, calls `update_traceroute_round`, broadcasts. Adapt the existing `TracerouteResult` arm to call the same code path with `round=1, total_rounds=1, completed=true, protocol="legacy"`.

**`crates/server/src/router/api/traceroute.rs`** — Replace `TracerouteResultResponse` with `TracerouteSnapshotResponse`. POST handler takes optional `protocol: Option<TraceProtocol>`. Add GET list (`/api/servers/{id}/traceroute`), DELETE one, DELETE all.

**`crates/server/src/router/api/mod.rs`** — Mount new DELETE routes under the admin-only write router; ensure new GET list is on the authenticated read router.

**`crates/server/src/openapi.rs`** — Register new endpoint handlers and DTOs.

**`crates/agent/Cargo.toml`** — Add `trippy-core = "0.13"`.

**`crates/agent/src/traceroute.rs`** *(new)* — Module containing `TraceProtocol -> trippy_core::Protocol` conversion, `port_direction_for`, `is_valid_traceroute_target`, `resolve_target`, `spawn_traceroute`, `is_privilege_error`, `platform_guidance`.

**`crates/agent/src/lib.rs`** *(or `main.rs` mod tree)* — Register the new `traceroute` module.

**`crates/agent/src/reporter.rs`** — Replace `ServerMessage::Traceroute` match arm with a call to `traceroute::spawn_traceroute`. Delete `execute_traceroute`, `parse_traceroute_output`, `parse_traceroute_line`. Move `is_valid_traceroute_target` (and its tests) to the new module.

**`apps/web/src/lib/network-types.ts`** — Extend `TracerouteHop` and `TracerouteResult`; add `TracerouteRecordSummary` and `RecordedProtocol` union.

**`apps/web/src/hooks/use-network-api.ts`** — Update `useStartTraceroute` to send `{ target, protocol }`. Add `useTracerouteHistory`, `useTracerouteRecord`, `useDeleteTraceroute`, `useClearTracerouteHistory`. Add `isNewSchema(hop)` helper.

**`apps/web/src/hooks/use-traceroute-stream.ts`** *(new)* — Subscribes to the existing browser WS dispatcher for `traceroute_update` messages filtered by `(serverId, requestId)`.

**`apps/web/src/hooks/use-servers-ws.ts`** — Add a `traceroute_update` branch to the in-process dispatcher so the new hook can subscribe.

**`apps/web/src/routes/_authed/network/$serverId.tsx`** — Refactor `TracerouteContent` to: protocol dropdown next to input; 10-column streaming hop table with loose `!= null` rendering; history list section; admin-gated Run form / delete / clear; state lifted to page (run vs. selected record exclusivity).

**`apps/docs/content/docs/cn/monitoring.mdx`** + **`apps/docs/content/docs/en/monitoring.mdx`** — Update Traceroute subsection: protocol selector, history feature, privilege matrix, remove the "install traceroute" note.

**`tests/traceroute.md`** *(new)* — Manual E2E checklist covering all three protocols, privilege fallback, history persistence, admin gating.

---

## Phase 1 — Wire Protocol (`crates/common`)

### Task 1: Add `TraceProtocol` + `RecordedProtocol` enums

**Files:**
- Modify: `crates/common/src/protocol.rs` (top-level enum definitions, near existing types)

- [ ] **Step 1: Write the failing tests**

Add to `crates/common/src/protocol.rs#tests`:

```rust
#[test]
fn test_trace_protocol_serializes_lowercase() {
    assert_eq!(serde_json::to_string(&TraceProtocol::Icmp).unwrap(), "\"icmp\"");
    assert_eq!(serde_json::to_string(&TraceProtocol::Udp).unwrap(), "\"udp\"");
    assert_eq!(serde_json::to_string(&TraceProtocol::Tcp).unwrap(), "\"tcp\"");
}

#[test]
fn test_trace_protocol_rejects_unknown_value() {
    let err = serde_json::from_str::<TraceProtocol>("\"banana\"").unwrap_err();
    assert!(err.to_string().contains("unknown variant"));
}

#[test]
fn test_trace_protocol_rejects_legacy_value() {
    // Legacy is a DB/read sentinel, not a probe-mode value the agent accepts.
    assert!(serde_json::from_str::<TraceProtocol>("\"legacy\"").is_err());
}

#[test]
fn test_recorded_protocol_serializes_lowercase_including_legacy() {
    assert_eq!(serde_json::to_string(&RecordedProtocol::Icmp).unwrap(), "\"icmp\"");
    assert_eq!(serde_json::to_string(&RecordedProtocol::Legacy).unwrap(), "\"legacy\"");
}

#[test]
fn test_recorded_protocol_from_trace_protocol() {
    assert_eq!(RecordedProtocol::from(TraceProtocol::Icmp), RecordedProtocol::Icmp);
    assert_eq!(RecordedProtocol::from(TraceProtocol::Udp),  RecordedProtocol::Udp);
    assert_eq!(RecordedProtocol::from(TraceProtocol::Tcp),  RecordedProtocol::Tcp);
}
```

- [ ] **Step 2: Run to confirm failures**

Run: `cargo test -p serverbee-common test_trace_protocol -- --nocapture`
Expected: compile errors ("cannot find type `TraceProtocol` in this scope").

- [ ] **Step 3: Implement the enums**

Add near the top of `crates/common/src/protocol.rs` (after the imports, before existing `ServerMessage`):

```rust
/// Strict input protocol enum used on `ServerMessage::Traceroute.protocol`
/// and on the server's POST request DTO. Only the three values the user can
/// pick are accepted; legacy is NOT part of this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum TraceProtocol {
    Icmp,
    Udp,
    Tcp,
}

/// Persisted/read protocol enum. Extends `TraceProtocol` with `Legacy` for
/// records normalized from pre-trippy agents whose actual probe mode is
/// unknown (Unix `traceroute` defaults to UDP, `mtr` is ICMP, Windows
/// `tracert` is ICMP — the legacy agent does not report which ran).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum RecordedProtocol {
    Icmp,
    Udp,
    Tcp,
    Legacy,
}

impl From<TraceProtocol> for RecordedProtocol {
    fn from(p: TraceProtocol) -> Self {
        match p {
            TraceProtocol::Icmp => Self::Icmp,
            TraceProtocol::Udp => Self::Udp,
            TraceProtocol::Tcp => Self::Tcp,
        }
    }
}
```

- [ ] **Step 4: Run to verify passing**

Run: `cargo test -p serverbee-common test_trace_protocol test_recorded_protocol -- --nocapture`
Expected: 5 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/common/src/protocol.rs
git commit -m "feat(common): add TraceProtocol and RecordedProtocol enums"
```

---

### Task 2: Extend `TracerouteHop` with trippy-core fields

**Files:**
- Modify: `crates/common/src/types.rs:108-116`

- [ ] **Step 1: Write the failing tests**

Add to `crates/common/src/protocol.rs#tests` (which already imports `TracerouteHop`):

```rust
#[test]
fn test_traceroute_hop_legacy_fields_skipped_when_none() {
    // A new-schema hop (filled by trippy) should NOT carry stale rtt1/2/3
    // keys in its JSON. Round-trip a hop with new fields populated but
    // legacy fields None and assert the serialized form has no rtt* / ip
    // keys (they are skip_serializing_if Option::is_none).
    let hop = TracerouteHop {
        hop: 1,
        ip: None,
        hostname: Some("router.local".into()),
        rtt1: None, rtt2: None, rtt3: None,
        asn: None,
        ips: vec!["10.0.0.1".into()],
        total_sent: Some(5),
        total_recv: Some(5),
        loss_pct: Some(0.0),
        best_ms: Some(1.1),
        worst_ms: Some(1.5),
        avg_ms: Some(1.3),
        stddev_ms: Some(0.15),
        jitter_ms: Some(0.05),
    };
    let json = serde_json::to_string(&hop).unwrap();
    assert!(!json.contains("\"rtt1\""), "got: {json}");
    assert!(!json.contains("\"ip\":"), "got: {json}");
    assert!(json.contains("\"ips\":[\"10.0.0.1\"]"));
    assert!(json.contains("\"loss_pct\":0.0"));
}

#[test]
fn test_traceroute_hop_new_schema_fields_skipped_when_default() {
    // A legacy-schema hop emitted by an old agent should NOT carry empty
    // ips: [] or null new-schema fields in JSON. Round-trip a legacy hop
    // and assert ips / total_sent etc. are absent.
    let hop = TracerouteHop {
        hop: 2,
        ip: Some("8.8.8.8".into()),
        hostname: Some("dns.google".into()),
        rtt1: Some(12.0), rtt2: Some(11.8), rtt3: Some(12.3),
        asn: Some("AS15169".into()),
        ips: vec![],
        total_sent: None, total_recv: None,
        loss_pct: None,
        best_ms: None, worst_ms: None, avg_ms: None,
        stddev_ms: None, jitter_ms: None,
    };
    let json = serde_json::to_string(&hop).unwrap();
    assert!(!json.contains("\"ips\":"),       "got: {json}");
    assert!(!json.contains("\"total_sent\""), "got: {json}");
    assert!(!json.contains("\"loss_pct\""),   "got: {json}");
    assert!(json.contains("\"rtt1\":12.0"));
}
```

- [ ] **Step 2: Run to confirm failures**

Run: `cargo test -p serverbee-common test_traceroute_hop -- --nocapture`
Expected: compile errors for unknown fields `ips`, `total_sent`, etc.

- [ ] **Step 3: Modify the struct**

Replace the body of `pub struct TracerouteHop` at `crates/common/src/types.rs:108` with:

```rust
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

- [ ] **Step 4: Run all common tests**

Run: `cargo test -p serverbee-common`
Expected: all pre-existing tests still pass; new `test_traceroute_hop_*` tests pass. Pre-existing tests like `test_traceroute_result_round_trip` may need their constructor calls updated — if a test fails compiling because it builds `TracerouteHop { hop, ip, hostname, rtt1, rtt2, rtt3, asn }` positionally or struct-literally without the new fields, update those test fixtures to add `..Default::default()`. But `TracerouteHop` does not yet derive `Default`. The simpler fix: do **not** derive `Default`; instead, update each pre-existing test to spell out the new fields explicitly (`ips: vec![], total_sent: None, ...`).

Search for failing test fixtures with: `grep -rn "TracerouteHop {" crates/`. Patch each fixture to add the new fields with default values (`ips: vec![]`, all `Option` fields = `None`).

- [ ] **Step 5: Commit**

```bash
git add crates/common/src/types.rs crates/common/src/protocol.rs
git commit -m "feat(common): extend TracerouteHop with trippy-core fields"
```

---

### Task 3: Add `protocol` field to `ServerMessage::Traceroute`

**Files:**
- Modify: `crates/common/src/protocol.rs:520-524`

- [ ] **Step 1: Write the failing test**

Add to `crates/common/src/protocol.rs#tests`:

```rust
#[test]
fn test_traceroute_server_message_with_protocol_round_trip() {
    let msg = ServerMessage::Traceroute {
        request_id: "rid-1".into(),
        target: "1.1.1.1".into(),
        max_hops: 30,
        protocol: Some(TraceProtocol::Udp),
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"protocol\":\"udp\""));
    let parsed: ServerMessage = serde_json::from_str(&json).unwrap();
    match parsed {
        ServerMessage::Traceroute { protocol, .. } => assert_eq!(protocol, Some(TraceProtocol::Udp)),
        _ => panic!("Expected Traceroute"),
    }
}

#[test]
fn test_traceroute_server_message_protocol_omitted_when_none() {
    // Old agents will see absent key and default to ICMP via existing behavior.
    let msg = ServerMessage::Traceroute {
        request_id: "rid-2".into(),
        target: "8.8.8.8".into(),
        max_hops: 30,
        protocol: None,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(!json.contains("\"protocol\""), "got: {json}");
}
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test -p serverbee-common test_traceroute_server_message_with_protocol -- --nocapture`
Expected: compile error (unknown field `protocol`).

- [ ] **Step 3: Add the field**

In `crates/common/src/protocol.rs:520`, modify the `Traceroute` variant:

```rust
    Traceroute {
        request_id: String,
        target: String,
        max_hops: u8,
        /// Strict enum; defaults to ICMP behavior when missing for old-agent
        /// compatibility.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        protocol: Option<TraceProtocol>,
    },
```

Update the existing `test_traceroute_server_message_round_trip` (in the same file) to construct with `protocol: None` so it still compiles.

- [ ] **Step 4: Run tests**

Run: `cargo test -p serverbee-common test_traceroute_server_message -- --nocapture`
Expected: 3 tests pass (old round-trip + 2 new).

- [ ] **Step 5: Commit**

```bash
git add crates/common/src/protocol.rs
git commit -m "feat(common): carry protocol on ServerMessage::Traceroute"
```

---

### Task 4: Add `AgentMessage::TracerouteRoundUpdate` variant

**Files:**
- Modify: `crates/common/src/protocol.rs` (in `AgentMessage` enum, after `TracerouteResult`)

- [ ] **Step 1: Write the failing test**

Add to `crates/common/src/protocol.rs#tests`:

```rust
#[test]
fn test_traceroute_round_update_round_trip_intermediate() {
    use crate::types::TracerouteHop;
    let msg = AgentMessage::TracerouteRoundUpdate {
        request_id: "rid-3".into(),
        target: "1.1.1.1".into(),
        round: 2,
        total_rounds: 5,
        hops: vec![TracerouteHop {
            hop: 1, ip: None, hostname: None,
            rtt1: None, rtt2: None, rtt3: None, asn: None,
            ips: vec!["10.0.0.1".into()],
            total_sent: Some(2), total_recv: Some(2),
            loss_pct: Some(0.0),
            best_ms: Some(1.0), worst_ms: Some(1.2), avg_ms: Some(1.1),
            stddev_ms: Some(0.1), jitter_ms: Some(0.05),
        }],
        completed: false,
        error: None,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"traceroute_round_update\""));
    let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentMessage::TracerouteRoundUpdate { round, total_rounds, completed, hops, .. } => {
            assert_eq!(round, 2);
            assert_eq!(total_rounds, 5);
            assert!(!completed);
            assert_eq!(hops.len(), 1);
        }
        _ => panic!("Expected TracerouteRoundUpdate"),
    }
}

#[test]
fn test_traceroute_round_update_terminal_error() {
    let msg = AgentMessage::TracerouteRoundUpdate {
        request_id: "rid-4".into(),
        target: "1.1.1.1".into(),
        round: 0,
        total_rounds: 0,
        hops: vec![],
        completed: true,
        error: Some("Traceroute requires elevated privileges".into()),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
    match parsed {
        AgentMessage::TracerouteRoundUpdate { completed, error, .. } => {
            assert!(completed);
            assert!(error.as_deref().unwrap().contains("privileges"));
        }
        _ => panic!("Expected TracerouteRoundUpdate"),
    }
}
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test -p serverbee-common test_traceroute_round_update -- --nocapture`
Expected: compile error (no variant `TracerouteRoundUpdate`).

- [ ] **Step 3: Add the variant**

In `crates/common/src/protocol.rs`, find the existing `TracerouteResult { ... }` variant in the `AgentMessage` enum (around line 351) and add this new variant right after it:

```rust
    /// Streamed by new (trippy-core) agents. One message per probe round.
    /// `hops` is the FULL accumulated state after this round, not a delta.
    /// `completed=true` marks the final update for `request_id`.
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

- [ ] **Step 4: Run tests**

Run: `cargo test -p serverbee-common test_traceroute_round_update -- --nocapture`
Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/common/src/protocol.rs
git commit -m "feat(common): add AgentMessage::TracerouteRoundUpdate variant"
```

---

### Task 5: Add `BrowserMessage::TracerouteUpdate` variant

**Files:**
- Modify: `crates/common/src/protocol.rs` (in `BrowserMessage` enum, near other domain updates like `NetworkProbeUpdate`)

- [ ] **Step 1: Write the failing test**

Add to `crates/common/src/protocol.rs#tests`:

```rust
#[test]
fn test_browser_message_traceroute_update_round_trip() {
    use crate::types::TracerouteHop;
    let msg = BrowserMessage::TracerouteUpdate {
        server_id: "srv-1".into(),
        request_id: "rid-5".into(),
        target: "1.1.1.1".into(),
        protocol: RecordedProtocol::Tcp,
        started_at: 1_716_500_000_000,
        round: 1,
        total_rounds: 5,
        hops: vec![TracerouteHop {
            hop: 1, ip: None, hostname: Some("hop1.example".into()),
            rtt1: None, rtt2: None, rtt3: None, asn: None,
            ips: vec!["10.0.0.1".into()],
            total_sent: Some(1), total_recv: Some(1),
            loss_pct: Some(0.0),
            best_ms: Some(1.0), worst_ms: Some(1.0), avg_ms: Some(1.0),
            stddev_ms: Some(0.0), jitter_ms: Some(0.0),
        }],
        completed: false,
        error: None,
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"traceroute_update\""));
    assert!(json.contains("\"protocol\":\"tcp\""));
    let parsed: BrowserMessage = serde_json::from_str(&json).unwrap();
    match parsed {
        BrowserMessage::TracerouteUpdate { protocol, started_at, .. } => {
            assert_eq!(protocol, RecordedProtocol::Tcp);
            assert_eq!(started_at, 1_716_500_000_000);
        }
        _ => panic!("Expected TracerouteUpdate"),
    }
}
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test -p serverbee-common test_browser_message_traceroute_update -- --nocapture`
Expected: compile error.

- [ ] **Step 3: Add the variant**

In `BrowserMessage` (around line 620 near `NetworkProbeUpdate`), add:

```rust
    TracerouteUpdate {
        server_id: String,
        request_id: String,
        target: String,
        /// From the server-side TracerouteRequestMeta cache so any browser
        /// (not only the originator) and reconnecting clients render the
        /// correct label without an extra GET round-trip.
        protocol: RecordedProtocol,
        started_at: i64,
        round: u32,
        total_rounds: u32,
        /// Server-side enriched (hostname filled in; ASN deferred).
        hops: Vec<TracerouteHop>,
        completed: bool,
        error: Option<String>,
    },
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p serverbee-common`
Expected: all common tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/common/src/protocol.rs
git commit -m "feat(common): add BrowserMessage::TracerouteUpdate variant"
```

---

## Phase 2 — Server DB (Migration + Entity)

### Task 6: Create the `traceroute_record` migration

**Files:**
- Create: `crates/server/src/migration/m20260524_000032_create_traceroute_record.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Create the migration file**

```rust
// crates/server/src/migration/m20260524_000032_create_traceroute_record.rs
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260524_000032_create_traceroute_record"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS traceroute_record (
                id TEXT PRIMARY KEY NOT NULL,
                server_id TEXT NOT NULL,
                target TEXT NOT NULL,
                protocol TEXT NOT NULL
                    CHECK (protocol IN ('icmp', 'udp', 'tcp', 'legacy')),
                started_at INTEGER NOT NULL,
                completed_at INTEGER,
                total_rounds INTEGER NOT NULL,
                completed_rounds INTEGER NOT NULL,
                hops_json TEXT NOT NULL,
                error TEXT,
                FOREIGN KEY (server_id) REFERENCES server(id) ON DELETE CASCADE
            )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_traceroute_record_server_started
                ON traceroute_record(server_id, started_at DESC)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

- [ ] **Step 2: Register in `mod.rs`**

In `crates/server/src/migration/mod.rs`, add the `mod` declaration alphabetically/numerically and append to the `Vec<Box<dyn MigrationTrait>>`:

```rust
mod m20260524_000032_create_traceroute_record;
// ...
            Box::new(m20260524_000032_create_traceroute_record::Migration),
```

- [ ] **Step 3: Build and run server-side compile check**

Run: `cargo build -p serverbee-server`
Expected: compiles.

- [ ] **Step 4: Run server tests (migrations execute against ephemeral SQLite in test setup)**

Run: `cargo test -p serverbee-server --test integration -- --skip large_` (any quick test that touches `Database`); if your integration test suite doesn't run the migration explicitly, simply `cargo build` is sufficient verification at this step.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/migration/m20260524_000032_create_traceroute_record.rs crates/server/src/migration/mod.rs
git commit -m "feat(server): add traceroute_record migration"
```

---

### Task 7: Create the `traceroute_record` entity

**Files:**
- Create: `crates/server/src/entity/traceroute_record.rs`
- Modify: `crates/server/src/entity/mod.rs`

- [ ] **Step 1: Create the entity**

```rust
// crates/server/src/entity/traceroute_record.rs
use sea_orm::entity::prelude::*;
use serverbee_common::protocol::RecordedProtocol;

#[derive(
    Clone,
    Debug,
    PartialEq,
    DeriveEntityModel,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    utoipa::ToSchema,
)]
#[sea_orm(table_name = "traceroute_record")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub server_id: String,
    pub target: String,
    /// Stored as the lowercase string form of `RecordedProtocol`. We expose
    /// it as `String` at the column level (sea-orm) and convert via
    /// `RecordedProtocol::try_from(s.as_str())` in the service layer.
    pub protocol: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub total_rounds: i32,
    pub completed_rounds: i32,
    /// Full hops Vec serialized as JSON.
    pub hops_json: String,
    pub error: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    pub fn protocol_enum(&self) -> RecordedProtocol {
        match self.protocol.as_str() {
            "icmp" => RecordedProtocol::Icmp,
            "udp"  => RecordedProtocol::Udp,
            "tcp"  => RecordedProtocol::Tcp,
            _      => RecordedProtocol::Legacy, // permissive read: covers "legacy" and any unknown value
        }
    }
}

pub fn protocol_to_str(p: RecordedProtocol) -> &'static str {
    match p {
        RecordedProtocol::Icmp => "icmp",
        RecordedProtocol::Udp  => "udp",
        RecordedProtocol::Tcp  => "tcp",
        RecordedProtocol::Legacy => "legacy",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_enum_roundtrip() {
        for p in [RecordedProtocol::Icmp, RecordedProtocol::Udp, RecordedProtocol::Tcp, RecordedProtocol::Legacy] {
            let s = protocol_to_str(p);
            let m = Model {
                id: "x".into(), server_id: "s".into(), target: "t".into(),
                protocol: s.to_string(), started_at: 0, completed_at: None,
                total_rounds: 1, completed_rounds: 1, hops_json: "[]".into(), error: None,
            };
            assert_eq!(m.protocol_enum(), p);
        }
    }

    #[test]
    fn test_unknown_protocol_string_maps_to_legacy() {
        let m = Model {
            id: "x".into(), server_id: "s".into(), target: "t".into(),
            protocol: "bogus".into(), started_at: 0, completed_at: None,
            total_rounds: 1, completed_rounds: 1, hops_json: "[]".into(), error: None,
        };
        assert_eq!(m.protocol_enum(), RecordedProtocol::Legacy);
    }
}
```

- [ ] **Step 2: Register in `entity/mod.rs`**

Add `pub mod traceroute_record;` alphabetically alongside other modules.

- [ ] **Step 3: Build and run entity tests**

Run: `cargo test -p serverbee-server entity::traceroute_record -- --nocapture`
Expected: 2 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/entity/traceroute_record.rs crates/server/src/entity/mod.rs
git commit -m "feat(server): add traceroute_record sea-orm entity"
```

---

## Phase 3 — Server Service Layer

### Task 8: Add `dns_lookup` dependency and create `TracerouteEnricher` scaffold

**Files:**
- Modify: `crates/server/Cargo.toml`
- Create: `crates/server/src/service/traceroute_enrich.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Add the dependency**

In `crates/server/Cargo.toml` under `[dependencies]`, add:

```toml
dns-lookup = "2"
```

- [ ] **Step 2: Create the enricher with PTR cache + tests**

```rust
// crates/server/src/service/traceroute_enrich.rs
use dashmap::DashMap;
use serverbee_common::types::TracerouteHop;
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PTR_CACHE_TTL: Duration = Duration::from_secs(3600);
const PTR_CACHE_MAX: usize = 4096;

#[derive(Clone, Default)]
pub struct TracerouteEnricher {
    /// IpAddr -> (hostname_or_none, inserted_at). `None` means "we tried and got nothing"
    /// — we still cache the negative result to avoid hammering DNS.
    ptr_cache: Arc<DashMap<IpAddr, (Option<String>, Instant)>>,
}

impl TracerouteEnricher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fill in `hostname` for every hop with an IP. Best-effort; failures
    /// leave the field at whatever value the agent sent. ASN is deferred —
    /// existing GeoIP DB is country-only, IP quality data is per-agent, not
    /// per arbitrary hop IP.
    pub async fn enrich(&self, hops: &mut [TracerouteHop]) {
        // Walk hops, prefer `ips[0]` (new schema) over `ip` (legacy).
        for hop in hops.iter_mut() {
            if hop.hostname.is_some() {
                continue;
            }
            let ip_str = hop.ips.first().cloned().or_else(|| hop.ip.clone());
            let Some(s) = ip_str else { continue };
            let Ok(ip) = IpAddr::from_str(&s) else { continue };
            hop.hostname = self.ptr_lookup(ip).await;
        }
    }

    async fn ptr_lookup(&self, ip: IpAddr) -> Option<String> {
        let now = Instant::now();
        if let Some(entry) = self.ptr_cache.get(&ip) {
            let (cached, inserted_at) = entry.value();
            if now.duration_since(*inserted_at) < PTR_CACHE_TTL {
                return cached.clone();
            }
        }
        // Miss or expired — look up.
        let result = tokio::task::spawn_blocking(move || dns_lookup::lookup_addr(&ip).ok())
            .await
            .ok()
            .flatten();
        // Evict oldest if over cap (cheap, not strict LRU — accept some churn).
        if self.ptr_cache.len() >= PTR_CACHE_MAX {
            // Drop ~1/16 of entries by scanning and removing the oldest we see.
            let limit = PTR_CACHE_MAX / 16;
            let mut victims: Vec<IpAddr> = self
                .ptr_cache
                .iter()
                .map(|e| (*e.key(), e.value().1))
                .collect::<Vec<_>>()
                .into_iter()
                .take(limit)
                .map(|(k, _)| k)
                .collect();
            // Sort by inserted_at ASC and drop the oldest.
            victims.sort();
            for v in victims {
                self.ptr_cache.remove(&v);
            }
        }
        self.ptr_cache.insert(ip, (result.clone(), now));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::types::TracerouteHop;

    fn hop_with_ip(s: &str) -> TracerouteHop {
        TracerouteHop {
            hop: 1, ip: Some(s.into()), hostname: None,
            rtt1: None, rtt2: None, rtt3: None, asn: None,
            ips: vec![], total_sent: None, total_recv: None, loss_pct: None,
            best_ms: None, worst_ms: None, avg_ms: None, stddev_ms: None, jitter_ms: None,
        }
    }

    #[tokio::test]
    async fn test_enrich_leaves_asn_field_none() {
        // Regression guard: a future ASN feature must not silently drop the field.
        let e = TracerouteEnricher::new();
        let mut hops = vec![hop_with_ip("127.0.0.1")];
        e.enrich(&mut hops).await;
        assert!(hops[0].asn.is_none(), "ASN must remain None in this iteration");
    }

    #[tokio::test]
    async fn test_ptr_cache_hit_does_not_relookup() {
        let e = TracerouteEnricher::new();
        let ip: IpAddr = "203.0.113.1".parse().unwrap();
        // Pre-populate with a fake answer
        e.ptr_cache.insert(ip, (Some("fake.example".into()), Instant::now()));
        let v = e.ptr_lookup(ip).await;
        assert_eq!(v.as_deref(), Some("fake.example"));
    }

    #[tokio::test]
    async fn test_ptr_cache_expired_entry_is_refreshed() {
        let e = TracerouteEnricher::new();
        let ip: IpAddr = "203.0.113.2".parse().unwrap();
        // Insert an expired entry (inserted 2 hours ago) with a sentinel value
        let stale_time = Instant::now() - Duration::from_secs(7200);
        e.ptr_cache.insert(ip, (Some("stale.example".into()), stale_time));
        // After lookup, real DNS will likely return None for TEST-NET-3 → entry replaced
        let _ = e.ptr_lookup(ip).await;
        let (_, inserted_at) = e.ptr_cache.get(&ip).unwrap().value().clone();
        assert!(
            inserted_at > stale_time + Duration::from_secs(3600),
            "expected the cache entry to be refreshed"
        );
    }

    #[tokio::test]
    async fn test_enrich_handles_ipv6() {
        let e = TracerouteEnricher::new();
        let mut hops = vec![hop_with_ip("::1")];
        // Should not panic on IPv6 even if PTR lookup fails.
        e.enrich(&mut hops).await;
    }
}
```

- [ ] **Step 3: Register module**

Add `pub mod traceroute_enrich;` in `crates/server/src/service/mod.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p serverbee-server service::traceroute_enrich -- --nocapture`
Expected: 4 tests pass (PTR lookups against TEST-NET-3 / loopback should not panic).

- [ ] **Step 5: Commit**

```bash
git add crates/server/Cargo.toml crates/server/src/service/traceroute_enrich.rs crates/server/src/service/mod.rs Cargo.lock
git commit -m "feat(server): introduce TracerouteEnricher with PTR LRU cache"
```

---

### Task 9: Create `service/traceroute.rs` CRUD module

**Files:**
- Create: `crates/server/src/service/traceroute.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Implement service functions**

```rust
// crates/server/src/service/traceroute.rs
use crate::entity::traceroute_record::{self, Model};
use crate::error::AppError;
use sea_orm::*;
use serverbee_common::protocol::RecordedProtocol;
use serverbee_common::types::TracerouteHop;

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct TracerouteRecordSummary {
    pub request_id: String,
    pub target: String,
    pub protocol: RecordedProtocol,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub hop_count: u32,
    pub has_error: bool,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct TracerouteSnapshotResponse {
    pub request_id: String,
    pub target: String,
    pub protocol: RecordedProtocol,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub round: u32,
    pub total_rounds: u32,
    pub completed: bool,
    pub hops: Vec<TracerouteHop>,
    pub error: Option<String>,
}

pub struct NewTracerouteRecord {
    pub id: String,
    pub server_id: String,
    pub target: String,
    pub protocol: RecordedProtocol,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub total_rounds: u32,
    pub completed_rounds: u32,
    pub hops: Vec<TracerouteHop>,
    pub error: Option<String>,
}

pub async fn list_records_for_server(
    db: &DatabaseConnection,
    server_id: &str,
    limit: u64,
    offset: u64,
) -> Result<Vec<TracerouteRecordSummary>, AppError> {
    let rows = traceroute_record::Entity::find()
        .filter(traceroute_record::Column::ServerId.eq(server_id))
        .order_by_desc(traceroute_record::Column::StartedAt)
        .limit(limit)
        .offset(offset)
        .all(db)
        .await
        .map_err(|e| AppError::Internal(format!("DB list: {e}")))?;
    Ok(rows
        .into_iter()
        .map(|m| {
            let hop_count =
                serde_json::from_str::<Vec<TracerouteHop>>(&m.hops_json)
                    .map(|h| h.len() as u32)
                    .unwrap_or(0);
            TracerouteRecordSummary {
                request_id: m.id.clone(),
                target: m.target.clone(),
                protocol: m.protocol_enum(),
                started_at: m.started_at,
                completed_at: m.completed_at,
                hop_count,
                has_error: m.error.is_some(),
            }
        })
        .collect())
}

pub async fn get_record_snapshot(
    db: &DatabaseConnection,
    server_id: &str,
    request_id: &str,
) -> Result<Option<TracerouteSnapshotResponse>, AppError> {
    let row = traceroute_record::Entity::find_by_id(request_id.to_string())
        .filter(traceroute_record::Column::ServerId.eq(server_id))
        .one(db)
        .await
        .map_err(|e| AppError::Internal(format!("DB get: {e}")))?;
    Ok(row.map(|m| model_to_snapshot(&m)))
}

pub fn model_to_snapshot(m: &Model) -> TracerouteSnapshotResponse {
    let hops: Vec<TracerouteHop> =
        serde_json::from_str(&m.hops_json).unwrap_or_default();
    TracerouteSnapshotResponse {
        request_id: m.id.clone(),
        target: m.target.clone(),
        protocol: m.protocol_enum(),
        started_at: m.started_at,
        completed_at: m.completed_at,
        round: m.completed_rounds as u32,
        total_rounds: m.total_rounds as u32,
        completed: m.completed_at.is_some(),
        hops,
        error: m.error.clone(),
    }
}

pub async fn delete_record(
    db: &DatabaseConnection,
    server_id: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let res = traceroute_record::Entity::delete_many()
        .filter(traceroute_record::Column::Id.eq(request_id))
        .filter(traceroute_record::Column::ServerId.eq(server_id))
        .exec(db)
        .await
        .map_err(|e| AppError::Internal(format!("DB delete: {e}")))?;
    if res.rows_affected == 0 {
        return Err(AppError::NotFound(format!(
            "Traceroute record {request_id} not found for server {server_id}"
        )));
    }
    Ok(())
}

pub async fn delete_records_for_server(
    db: &DatabaseConnection,
    server_id: &str,
) -> Result<u64, AppError> {
    let res = traceroute_record::Entity::delete_many()
        .filter(traceroute_record::Column::ServerId.eq(server_id))
        .exec(db)
        .await
        .map_err(|e| AppError::Internal(format!("DB clear: {e}")))?;
    Ok(res.rows_affected)
}

pub async fn insert_completed_record(
    db: &DatabaseConnection,
    record: NewTracerouteRecord,
) -> Result<(), AppError> {
    let hops_json = serde_json::to_string(&record.hops)
        .map_err(|e| AppError::Internal(format!("JSON encode hops: {e}")))?;
    let am = traceroute_record::ActiveModel {
        id: Set(record.id),
        server_id: Set(record.server_id),
        target: Set(record.target),
        protocol: Set(traceroute_record::protocol_to_str(record.protocol).to_string()),
        started_at: Set(record.started_at),
        completed_at: Set(record.completed_at),
        total_rounds: Set(record.total_rounds as i32),
        completed_rounds: Set(record.completed_rounds as i32),
        hops_json: Set(hops_json),
        error: Set(record.error),
    };
    traceroute_record::Entity::insert(am)
        .exec(db)
        .await
        .map_err(|e| AppError::Internal(format!("DB insert: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{Database, DbBackend, Schema};

    async fn fresh_db() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let schema = Schema::new(DbBackend::Sqlite);
        let stmt = schema.create_table_from_entity(traceroute_record::Entity);
        db.execute(db.get_database_backend().build(&stmt)).await.unwrap();
        db
    }

    fn new_record(server_id: &str, request_id: &str, target: &str) -> NewTracerouteRecord {
        NewTracerouteRecord {
            id: request_id.into(),
            server_id: server_id.into(),
            target: target.into(),
            protocol: RecordedProtocol::Icmp,
            started_at: 1_716_500_000_000,
            completed_at: Some(1_716_500_010_000),
            total_rounds: 5,
            completed_rounds: 5,
            hops: vec![],
            error: None,
        }
    }

    #[tokio::test]
    async fn test_insert_and_list_filters_by_server_id() {
        let db = fresh_db().await;
        insert_completed_record(&db, new_record("s1", "r1", "1.1.1.1")).await.unwrap();
        insert_completed_record(&db, new_record("s2", "r2", "8.8.8.8")).await.unwrap();
        let only_s1 = list_records_for_server(&db, "s1", 50, 0).await.unwrap();
        assert_eq!(only_s1.len(), 1);
        assert_eq!(only_s1[0].request_id, "r1");
    }

    #[tokio::test]
    async fn test_get_record_snapshot_rejects_cross_server() {
        let db = fresh_db().await;
        insert_completed_record(&db, new_record("s1", "r1", "1.1.1.1")).await.unwrap();
        let none = get_record_snapshot(&db, "s2", "r1").await.unwrap();
        assert!(none.is_none(), "must not return a record under wrong server scope");
    }

    #[tokio::test]
    async fn test_delete_record_rejects_cross_server() {
        let db = fresh_db().await;
        insert_completed_record(&db, new_record("s1", "r1", "1.1.1.1")).await.unwrap();
        let err = delete_record(&db, "s2", "r1").await.unwrap_err();
        match err {
            AppError::NotFound(_) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
        // Original row still present
        assert!(get_record_snapshot(&db, "s1", "r1").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_delete_records_for_server_returns_count() {
        let db = fresh_db().await;
        insert_completed_record(&db, new_record("s1", "r1", "1.1.1.1")).await.unwrap();
        insert_completed_record(&db, new_record("s1", "r2", "8.8.8.8")).await.unwrap();
        insert_completed_record(&db, new_record("s2", "r3", "9.9.9.9")).await.unwrap();
        let n = delete_records_for_server(&db, "s1").await.unwrap();
        assert_eq!(n, 2);
        let remaining = list_records_for_server(&db, "s1", 50, 0).await.unwrap();
        assert!(remaining.is_empty());
    }
}
```

- [ ] **Step 2: Register module**

In `crates/server/src/service/mod.rs`: `pub mod traceroute;`

- [ ] **Step 3: Run tests**

Run: `cargo test -p serverbee-server service::traceroute -- --nocapture`
Expected: 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/traceroute.rs crates/server/src/service/mod.rs
git commit -m "feat(server): add traceroute service CRUD with server-scoped guards"
```

---

## Phase 4 — `AppState` + `agent_manager` refactor

### Task 10: Replace `TracerouteResultData` cache shape with `TracerouteRequestMeta`

**Files:**
- Modify: `crates/server/src/service/agent_manager.rs:39-50` (data types), `:615-654` (methods)

- [ ] **Step 1: Write the failing tests**

Add to `crates/server/src/service/agent_manager.rs#tests` (or create the test module if absent). For agent_manager, behavior tests typically need a real `AgentManager` instance — the existing code likely has none, so add a `#[cfg(test)] mod tests {}` block at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::protocol::RecordedProtocol;
    use tokio::sync::broadcast;

    fn make_manager() -> AgentManager {
        let (tx, _rx) = broadcast::channel(16);
        AgentManager::new(tx)
    }

    #[test]
    fn test_insert_placeholder_then_get_returns_meta() {
        let m = make_manager();
        m.insert_traceroute_placeholder(
            "rid",
            TracerouteRequestMeta {
                server_id: "s".into(),
                target: "1.1.1.1".into(),
                protocol: RecordedProtocol::Udp,
                started_at: 1_716_500_000_000,
            },
        );
        let snap = m.get_traceroute_snapshot("rid").expect("snapshot present");
        assert_eq!(snap.server_id, "s");
        assert_eq!(snap.target, "1.1.1.1");
        assert_eq!(snap.protocol, RecordedProtocol::Udp);
        assert_eq!(snap.started_at, 1_716_500_000_000);
        assert!(snap.hops.is_empty());
        assert!(!snap.completed);
    }

    #[test]
    fn test_update_round_overwrites_hops_and_marks_completed() {
        use serverbee_common::types::TracerouteHop;
        let m = make_manager();
        m.insert_traceroute_placeholder(
            "rid",
            TracerouteRequestMeta {
                server_id: "s".into(),
                target: "1.1.1.1".into(),
                protocol: RecordedProtocol::Icmp,
                started_at: 0,
            },
        );
        let hop = TracerouteHop {
            hop: 1, ip: None, hostname: None,
            rtt1: None, rtt2: None, rtt3: None, asn: None,
            ips: vec!["10.0.0.1".into()],
            total_sent: Some(2), total_recv: Some(2), loss_pct: Some(0.0),
            best_ms: Some(1.0), worst_ms: Some(1.0), avg_ms: Some(1.0),
            stddev_ms: Some(0.0), jitter_ms: Some(0.0),
        };
        m.update_traceroute_round("rid", 1, 5, vec![hop.clone()], false, None);
        m.update_traceroute_round("rid", 5, 5, vec![hop.clone()], true, None);
        let snap = m.get_traceroute_snapshot("rid").unwrap();
        assert_eq!(snap.round, 5);
        assert_eq!(snap.total_rounds, 5);
        assert!(snap.completed);
        assert_eq!(snap.hops.len(), 1);
    }

    #[test]
    fn test_update_with_missing_meta_is_dropped_silently() {
        let m = make_manager();
        // No placeholder inserted → update should be a no-op (and not panic).
        m.update_traceroute_round("ghost", 1, 1, vec![], true, None);
        assert!(m.get_traceroute_snapshot("ghost").is_none());
    }
}
```

- [ ] **Step 2: Replace data types and methods**

Replace `TracerouteResultData`, `TracerouteResultEntry`, and the four `*_traceroute_*` methods (around lines 39-50 and 612-661) with:

```rust
use serverbee_common::protocol::RecordedProtocol;

#[derive(Clone, Debug)]
pub struct TracerouteRequestMeta {
    pub server_id: String,
    pub target: String,
    pub protocol: RecordedProtocol,
    pub started_at: i64,
}

#[derive(Clone, Debug)]
pub struct TracerouteSnapshot {
    pub server_id: String,
    pub target: String,
    pub protocol: RecordedProtocol,
    pub started_at: i64,
    pub round: u32,
    pub total_rounds: u32,
    pub completed: bool,
    pub hops: Vec<TracerouteHop>,
    pub error: Option<String>,
}

struct TracerouteCacheEntry {
    meta: TracerouteRequestMeta,
    round: u32,
    total_rounds: u32,
    hops: Vec<TracerouteHop>,
    completed: bool,
    error: Option<String>,
    created_at: Instant,
    completed_at: Option<Instant>,
}
```

…and replace the field `traceroute_results: DashMap<String, TracerouteResultEntry>` in `AgentManager` with `traceroute_results: DashMap<String, TracerouteCacheEntry>`.

Then the methods:

```rust
// --- Traceroute cache ---

pub fn insert_traceroute_placeholder(
    &self,
    request_id: &str,
    meta: TracerouteRequestMeta,
) {
    self.traceroute_results.insert(
        request_id.to_string(),
        TracerouteCacheEntry {
            meta,
            round: 0,
            total_rounds: 0,
            hops: vec![],
            completed: false,
            error: None,
            created_at: Instant::now(),
            completed_at: None,
        },
    );
}

/// Apply one round of trippy data to the cache. Drops the message if no
/// placeholder exists (e.g., cache evicted or stale agent reply).
/// Returns the snapshot AFTER applying, or None if dropped.
pub fn update_traceroute_round(
    &self,
    request_id: &str,
    round: u32,
    total_rounds: u32,
    hops: Vec<TracerouteHop>,
    completed: bool,
    error: Option<String>,
) -> Option<TracerouteSnapshot> {
    let mut entry = self.traceroute_results.get_mut(request_id)?;
    entry.round = round;
    entry.total_rounds = total_rounds;
    entry.hops = hops;
    entry.completed = completed;
    entry.error = error;
    if completed && entry.completed_at.is_none() {
        entry.completed_at = Some(Instant::now());
    }
    Some(TracerouteSnapshot {
        server_id: entry.meta.server_id.clone(),
        target: entry.meta.target.clone(),
        protocol: entry.meta.protocol,
        started_at: entry.meta.started_at,
        round: entry.round,
        total_rounds: entry.total_rounds,
        completed: entry.completed,
        hops: entry.hops.clone(),
        error: entry.error.clone(),
    })
}

pub fn get_traceroute_snapshot(&self, request_id: &str) -> Option<TracerouteSnapshot> {
    self.traceroute_results.get(request_id).map(|e| TracerouteSnapshot {
        server_id: e.meta.server_id.clone(),
        target: e.meta.target.clone(),
        protocol: e.meta.protocol,
        started_at: e.meta.started_at,
        round: e.round,
        total_rounds: e.total_rounds,
        completed: e.completed,
        hops: e.hops.clone(),
        error: e.error.clone(),
    })
}

pub fn get_traceroute_meta(&self, request_id: &str) -> Option<TracerouteRequestMeta> {
    self.traceroute_results.get(request_id).map(|e| e.meta.clone())
}

pub fn set_traceroute_meta_protocol(&self, request_id: &str, protocol: RecordedProtocol) {
    if let Some(mut entry) = self.traceroute_results.get_mut(request_id) {
        entry.meta.protocol = protocol;
    }
}

/// Evict cache entries 120s after `completed_at` (or 120s after creation
/// for stuck/never-completed traces).
pub fn cleanup_traceroute_results(&self) {
    let now = Instant::now();
    self.traceroute_results.retain(|_, entry| {
        let anchor = entry.completed_at.unwrap_or(entry.created_at);
        now.duration_since(anchor).as_secs() < 120
    });
}
```

- [ ] **Step 3: Update call sites**

Compile errors will surface every old caller. They are: the POST handler (`router/api/traceroute.rs::trigger_traceroute`), the WS handler (`router/ws/agent.rs::TracerouteResult` arm), the existing GET handler. **Don't** fix them in this task — Tasks 11, 12, 13 do. Use `#[allow(dead_code)]` if needed to keep the build green is **not** acceptable here; instead, **expect** the next tasks to address compile failures.

Actually you can't commit a non-compiling tree. So: in this task only, also **stub out** the old methods that callers reference:

- Keep `update_traceroute_result`, `get_traceroute_result` as thin shims that adapt to / return errors so callers still compile. Mark them `#[deprecated]`. Tasks 11-13 will delete them.

Add this temporary shim block right after the new methods:

```rust
// --- TEMPORARY SHIMS (removed by Tasks 11-13) ---

#[deprecated(note = "use TracerouteRequestMeta + insert_traceroute_placeholder(meta)")]
#[allow(dead_code)]
pub fn insert_traceroute_placeholder_legacy(
    &self,
    request_id: &str,
    server_id: &str,
    target: &str,
) {
    self.insert_traceroute_placeholder(
        request_id,
        TracerouteRequestMeta {
            server_id: server_id.to_string(),
            target: target.to_string(),
            protocol: RecordedProtocol::Icmp,
            started_at: chrono::Utc::now().timestamp_millis(),
        },
    );
}

/// Returns the snapshot in the legacy 4-field tuple shape so the current
/// router code still compiles before Tasks 11-13 land.
#[deprecated(note = "use get_traceroute_snapshot")]
#[allow(dead_code)]
pub fn get_traceroute_result(
    &self,
    request_id: &str,
) -> Option<(String, TracerouteResultData)> {
    let s = self.get_traceroute_snapshot(request_id)?;
    Some((
        s.server_id.clone(),
        TracerouteResultData {
            target: s.target,
            hops: s.hops,
            completed: s.completed,
            error: s.error,
        },
    ))
}

#[deprecated(note = "use update_traceroute_round")]
#[allow(dead_code)]
pub fn update_traceroute_result(&self, request_id: &str, result: TracerouteResultData) {
    let _ = self.update_traceroute_round(
        request_id,
        1,
        1,
        result.hops,
        result.completed,
        result.error,
    );
}

#[deprecated]
#[allow(dead_code)]
pub struct TracerouteResultData {
    pub target: String,
    pub hops: Vec<TracerouteHop>,
    pub completed: bool,
    pub error: Option<String>,
}
```

Adjust the existing `insert_traceroute_placeholder` call in `router/api/traceroute.rs` to use the `_legacy` shim with one find-and-replace.

- [ ] **Step 4: Run tests**

Run: `cargo build -p serverbee-server` (must compile).
Then: `cargo test -p serverbee-server service::agent_manager -- --nocapture`
Expected: 3 new tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/agent_manager.rs crates/server/src/router/api/traceroute.rs
git commit -m "feat(server): replace traceroute cache with TracerouteRequestMeta"
```

---

### Task 11: Wire `TracerouteEnricher` into `AppState`

**Files:**
- Modify: `crates/server/src/state.rs`

- [ ] **Step 1: Add the field**

In `crates/server/src/state.rs`, add `pub traceroute_enricher: crate::service::traceroute_enrich::TracerouteEnricher,` to the struct. In the `new()` / construction site initialize it with `TracerouteEnricher::new()`.

- [ ] **Step 2: Build**

Run: `cargo build -p serverbee-server`
Expected: compiles.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/state.rs
git commit -m "feat(server): expose TracerouteEnricher on AppState"
```

---

## Phase 5 — WS handler

### Task 12: Handle `AgentMessage::TracerouteRoundUpdate` in the WS agent handler

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs:1302-1321` (existing `TracerouteResult` arm and a new arm right after)

- [ ] **Step 1: Add the new arm**

Right after the existing `AgentMessage::TracerouteResult { ... }` block, add:

```rust
AgentMessage::TracerouteRoundUpdate {
    request_id,
    target,
    round,
    total_rounds,
    mut hops,
    completed,
    error,
} => {
    tracing::debug!(
        "Received TracerouteRoundUpdate from {server_id} \
        (request_id={request_id}, round={round}/{total_rounds}, completed={completed})"
    );

    // Server-side enrich (hostname only this iteration)
    state.traceroute_enricher.enrich(&mut hops).await;

    // Update in-memory cache
    let Some(snapshot) = state.agent_manager.update_traceroute_round(
        &request_id,
        round,
        total_rounds,
        hops.clone(),
        completed,
        error.clone(),
    ) else {
        tracing::warn!(
            "Dropping TracerouteRoundUpdate {request_id}: no cached placeholder"
        );
        return;
    };

    // On completion, persist a DB row
    if completed {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let new_record = crate::service::traceroute::NewTracerouteRecord {
            id: request_id.clone(),
            server_id: snapshot.server_id.clone(),
            target: snapshot.target.clone(),
            protocol: snapshot.protocol,
            started_at: snapshot.started_at,
            completed_at: Some(now_ms),
            total_rounds: snapshot.total_rounds,
            completed_rounds: snapshot.round,
            hops: snapshot.hops.clone(),
            error: snapshot.error.clone(),
        };
        if let Err(e) = crate::service::traceroute::insert_completed_record(&state.db, new_record).await {
            tracing::warn!("Failed to persist traceroute record {request_id}: {e:?}");
        }
    }

    // Broadcast to subscribed browsers
    let _ = state.browser_tx.send(serverbee_common::protocol::BrowserMessage::TracerouteUpdate {
        server_id: snapshot.server_id.clone(),
        request_id: request_id.clone(),
        target: snapshot.target.clone(),
        protocol: snapshot.protocol,
        started_at: snapshot.started_at,
        round: snapshot.round,
        total_rounds: snapshot.total_rounds,
        hops: snapshot.hops,
        completed: snapshot.completed,
        error: snapshot.error,
    });
}
```

- [ ] **Step 2: Adapt the existing `TracerouteResult` arm to use the same pipeline**

Replace the body of the `TracerouteResult` arm (above the new arm) with:

```rust
AgentMessage::TracerouteResult {
    request_id,
    target,
    hops,
    completed,
    error,
} => {
    tracing::info!(
        "Received legacy TracerouteResult from {server_id} (request_id={request_id})"
    );
    // Legacy agent does not report which probe protocol actually ran (UDP
    // for Unix `traceroute`, ICMP for `mtr` / Windows `tracert`). Persist
    // with the "legacy" sentinel.
    state.agent_manager.set_traceroute_meta_protocol(
        &request_id,
        serverbee_common::protocol::RecordedProtocol::Legacy,
    );
    // Re-dispatch into the new pipeline as a single-round update.
    let synthetic = AgentMessage::TracerouteRoundUpdate {
        request_id, target, round: 1, total_rounds: 1, hops, completed, error,
    };
    // Recurse into the new arm via direct call (extract the body of the
    // new arm into a helper function or use a labelled goto via match).
    handle_traceroute_round_update(&state, &server_id, synthetic).await;
    return;
}
```

…and extract the body of the new arm above into a small helper `async fn handle_traceroute_round_update(state: &Arc<AppState>, server_id: &str, msg: AgentMessage)` either in the same file (private fn at the bottom) or in a sibling module. Use that helper from BOTH the `TracerouteResult` arm and the `TracerouteRoundUpdate` arm.

- [ ] **Step 3: Build**

Run: `cargo build -p serverbee-server`
Expected: compiles.

- [ ] **Step 4: Run tests**

Run: `cargo test -p serverbee-server -- --skip large_`
Expected: all tests still pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/ws/agent.rs
git commit -m "feat(server): handle TracerouteRoundUpdate + legacy normalization"
```

---

## Phase 6 — Server REST endpoints

### Task 13: Replace POST/GET DTOs with strict protocol enum and snapshot response

**Files:**
- Modify: `crates/server/src/router/api/traceroute.rs` (full file rewrite is reasonable)
- Modify: `crates/server/src/router/api/mod.rs` (mount new routes)
- Modify: `crates/server/src/openapi.rs` (register new endpoints + DTOs)

- [ ] **Step 1: Rewrite `router/api/traceroute.rs`**

Replace the entire file with:

```rust
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use uuid::Uuid;

use crate::error::{ApiResponse, AppError, ok};
use crate::service::traceroute::{
    self, TracerouteRecordSummary, TracerouteSnapshotResponse,
};
use crate::state::AppState;
use serverbee_common::protocol::{RecordedProtocol, ServerMessage, TraceProtocol};

const MAX_HOPS: u8 = 30;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TriggerTracerouteRequest {
    /// Target host or IP (e.g. "1.2.3.4" or "example.com").
    pub target: String,
    /// One of `icmp` | `udp` | `tcp`. Missing → defaults to `icmp`.
    #[serde(default)]
    pub protocol: Option<TraceProtocol>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct TriggerTracerouteResponse {
    pub request_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
}

fn default_limit() -> u64 { 50 }

// ---------- Routers ----------

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/servers/{id}/traceroute/{request_id}",
            get(get_traceroute_snapshot),
        )
        .route("/servers/{id}/traceroute", get(list_traceroute_records))
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/servers/{id}/traceroute", post(trigger_traceroute))
        .route(
            "/servers/{id}/traceroute/{request_id}",
            delete(delete_traceroute_record),
        )
        .route("/servers/{id}/traceroute", delete(clear_traceroute_history))
}

// ---------- Handlers ----------

#[utoipa::path(
    post, path = "/api/servers/{id}/traceroute", tag = "traceroute",
    params(("id" = String, Path, description = "Server ID")),
    request_body = TriggerTracerouteRequest,
    responses(
        (status = 200, body = TriggerTracerouteResponse),
        (status = 404), (status = 422),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn trigger_traceroute(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Json(input): Json<TriggerTracerouteRequest>,
) -> Result<Json<ApiResponse<TriggerTracerouteResponse>>, AppError> {
    if input.target.is_empty()
        || !input.target.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':')
    {
        return Err(AppError::Validation(
            "Invalid target: only alphanumeric characters, dots, hyphens, and colons are allowed".to_string(),
        ));
    }
    let tx = state.agent_manager.get_sender(&server_id)
        .ok_or_else(|| AppError::NotFound(format!("Server {server_id} is not online")))?;
    let request_id = Uuid::new_v4().to_string();
    let protocol = input.protocol.unwrap_or(TraceProtocol::Icmp);

    state.agent_manager.insert_traceroute_placeholder(
        &request_id,
        crate::service::agent_manager::TracerouteRequestMeta {
            server_id: server_id.clone(),
            target: input.target.clone(),
            protocol: RecordedProtocol::from(protocol),
            started_at: chrono::Utc::now().timestamp_millis(),
        },
    );

    let msg = ServerMessage::Traceroute {
        request_id: request_id.clone(),
        target: input.target,
        max_hops: MAX_HOPS,
        protocol: Some(protocol),
    };
    tx.send(msg).await
        .map_err(|_| AppError::Internal("Failed to send traceroute command to agent".to_string()))?;

    ok(TriggerTracerouteResponse { request_id })
}

#[utoipa::path(
    get, path = "/api/servers/{id}/traceroute/{request_id}", tag = "traceroute",
    params(("id" = String, Path), ("request_id" = String, Path)),
    responses((status = 200, body = TracerouteSnapshotResponse), (status = 404)),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_traceroute_snapshot(
    State(state): State<Arc<AppState>>,
    Path((server_id, request_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<TracerouteSnapshotResponse>>, AppError> {
    // 1. In-memory cache (live or recently-completed)
    if let Some(snap) = state.agent_manager.get_traceroute_snapshot(&request_id)
        && snap.server_id == server_id
    {
        return ok(TracerouteSnapshotResponse {
            request_id,
            target: snap.target,
            protocol: snap.protocol,
            started_at: snap.started_at,
            completed_at: if snap.completed { Some(chrono::Utc::now().timestamp_millis()) } else { None },
            round: snap.round,
            total_rounds: snap.total_rounds,
            completed: snap.completed,
            hops: snap.hops,
            error: snap.error,
        });
    }
    // 2. DB fallback
    if let Some(snap) = traceroute::get_record_snapshot(&state.db, &server_id, &request_id).await? {
        return ok(snap);
    }
    Err(AppError::NotFound(format!("Traceroute {request_id} not found")))
}

#[utoipa::path(
    get, path = "/api/servers/{id}/traceroute", tag = "traceroute",
    params(("id" = String, Path)),
    responses((status = 200, body = Vec<TracerouteRecordSummary>)),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list_traceroute_records(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Query(q): Query<ListQuery>,
) -> Result<Json<ApiResponse<Vec<TracerouteRecordSummary>>>, AppError> {
    let rows = traceroute::list_records_for_server(&state.db, &server_id, q.limit, q.offset).await?;
    ok(rows)
}

#[utoipa::path(
    delete, path = "/api/servers/{id}/traceroute/{request_id}", tag = "traceroute",
    params(("id" = String, Path), ("request_id" = String, Path)),
    responses((status = 204), (status = 404)),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn delete_traceroute_record(
    State(state): State<Arc<AppState>>,
    Path((server_id, request_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    traceroute::delete_record(&state.db, &server_id, &request_id).await?;
    ok(())
}

#[utoipa::path(
    delete, path = "/api/servers/{id}/traceroute", tag = "traceroute",
    params(("id" = String, Path)),
    responses((status = 200, body = ClearedResponse)),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn clear_traceroute_history(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> Result<Json<ApiResponse<ClearedResponse>>, AppError> {
    let deleted = traceroute::delete_records_for_server(&state.db, &server_id).await?;
    ok(ClearedResponse { deleted })
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct ClearedResponse {
    pub deleted: u64,
}
```

- [ ] **Step 2: Update OpenAPI registration**

In `crates/server/src/openapi.rs` (replace the two existing traceroute path references and add the new ones):

```rust
        crate::router::api::traceroute::trigger_traceroute,
        crate::router::api::traceroute::get_traceroute_snapshot,
        crate::router::api::traceroute::list_traceroute_records,
        crate::router::api::traceroute::delete_traceroute_record,
        crate::router::api::traceroute::clear_traceroute_history,
```

…and in the `components(schemas(...))` block:

```rust
        crate::router::api::traceroute::TriggerTracerouteRequest,
        crate::router::api::traceroute::TriggerTracerouteResponse,
        crate::router::api::traceroute::ClearedResponse,
        crate::service::traceroute::TracerouteRecordSummary,
        crate::service::traceroute::TracerouteSnapshotResponse,
        serverbee_common::types::TracerouteHop,
        serverbee_common::protocol::TraceProtocol,
        serverbee_common::protocol::RecordedProtocol,
```

(Remove the obsolete `TracerouteResultResponse`.)

- [ ] **Step 3: Verify router mounting**

`crates/server/src/router/api/mod.rs` already calls `traceroute::write_router()` under the admin gate (line 98) and `traceroute::read_router()` under the authenticated read group (line 68). The rewritten module exports the same two functions with the new routes; no changes needed there as long as the function names match.

- [ ] **Step 4: Build and run tests**

Run: `cargo build -p serverbee-server && cargo test -p serverbee-server`
Expected: compiles and all tests pass (the legacy DTO shims in agent_manager are now unreferenced but still allowed via `#[allow(dead_code)]`).

- [ ] **Step 5: Clean up the temporary shims**

Delete the `// --- TEMPORARY SHIMS ---` block from `agent_manager.rs` (the four `_legacy` / deprecated helpers). Build again to confirm nothing else still uses them.

Run: `cargo build -p serverbee-server`
Expected: compiles cleanly.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/router/api/traceroute.rs crates/server/src/openapi.rs crates/server/src/service/agent_manager.rs
git commit -m "feat(server): expose traceroute snapshot/history/delete REST endpoints"
```

---

## Phase 7 — Agent: trippy-core integration

### Task 14: Add `trippy-core` dependency and create the new module

**Files:**
- Modify: `crates/agent/Cargo.toml`
- Create: `crates/agent/src/traceroute.rs`
- Modify: `crates/agent/src/main.rs` or `crates/agent/src/lib.rs` (whichever declares `mod` items)

- [ ] **Step 1: Add the dependency**

In `crates/agent/Cargo.toml` under `[dependencies]`:

```toml
trippy-core = "0.13"
```

- [ ] **Step 2: Create the module scaffold**

```rust
// crates/agent/src/traceroute.rs
use std::cell::{Cell, RefCell};
use std::net::IpAddr;
use std::str::FromStr;
use std::time::Duration;

use serverbee_common::protocol::{AgentMessage, TraceProtocol};
use serverbee_common::types::TracerouteHop;
use tokio::sync::mpsc;
use trippy_core::{
    Builder, Port, PortDirection, PrivilegeMode, Protocol,
};

pub const DEFAULT_MAX_ROUNDS: u32 = 5;
pub const ROUND_INTERVAL: Duration = Duration::from_millis(1000);
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);
pub const UDP_DEFAULT_DEST_PORT: u16 = 33_434;
pub const TCP_DEFAULT_DEST_PORT: u16 = 80;

pub fn port_direction_for(proto: Protocol) -> PortDirection {
    match proto {
        Protocol::Icmp => PortDirection::None,
        Protocol::Udp  => PortDirection::FixedDest(Port(UDP_DEFAULT_DEST_PORT)),
        Protocol::Tcp  => PortDirection::FixedDest(Port(TCP_DEFAULT_DEST_PORT)),
    }
}

pub fn trippy_protocol_from(p: TraceProtocol) -> Protocol {
    match p {
        TraceProtocol::Icmp => Protocol::Icmp,
        TraceProtocol::Udp  => Protocol::Udp,
        TraceProtocol::Tcp  => Protocol::Tcp,
    }
}

/// Validate that a traceroute target contains only safe characters (domain or IP).
pub fn is_valid_traceroute_target(target: &str) -> bool {
    !target.is_empty()
        && target.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':')
}

/// Resolve a literal IP or hostname. `lookup_host` requires `host:port`, so
/// we tack on a dummy port and take the first address.
pub async fn resolve_target(target: &str) -> Result<IpAddr, String> {
    if let Ok(ip) = IpAddr::from_str(target) {
        return Ok(ip);
    }
    let mut iter = tokio::net::lookup_host((target, 0u16))
        .await
        .map_err(|e| format!("DNS resolution failed: {e}"))?;
    iter.next()
        .map(|sa| sa.ip())
        .ok_or_else(|| format!("DNS resolution returned no addresses for {target}"))
}

pub fn is_privilege_error(e: &trippy_core::Error) -> bool {
    use std::io::ErrorKind::PermissionDenied;
    use trippy_core::Error::*;
    match e {
        PrivilegeError(_) => true,
        IoError(io) | ProbeFailed(io) => io.kind() == PermissionDenied,
        _ => false,
    }
}

pub fn platform_guidance() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "Traceroute requires elevated privileges. Run the agent as root, or grant CAP_NET_RAW: \
         sudo setcap cap_net_raw+ep $(which serverbee-agent)"
    }
    #[cfg(target_os = "macos")]
    {
        "Traceroute requires elevated privileges. Run the agent as root (sudo)."
    }
    #[cfg(target_os = "windows")]
    {
        "Traceroute requires Administrator privileges. Restart the agent as Administrator."
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "Traceroute requires elevated privileges. Run the agent as a privileged user."
    }
}

fn try_trace<F>(
    addr: IpAddr,
    max_hops: u8,
    proto: Protocol,
    priv_mode: PrivilegeMode,
    callback: F,
) -> Result<(), trippy_core::Error>
where
    F: Fn(&trippy_core::Round<'_>),
{
    let tracer = Builder::new(addr)
        .max_ttl(max_hops)
        .max_rounds(Some(DEFAULT_MAX_ROUNDS as usize))
        .min_round_duration(ROUND_INTERVAL)
        .max_round_duration(ROUND_INTERVAL * 2)
        .read_timeout(PROBE_TIMEOUT)
        .protocol(proto)
        .port_direction(port_direction_for(proto))
        .privilege_mode(priv_mode)
        .build()?;
    tracer.run_with(callback)
}

fn hops_from_state(state: &trippy_core::State) -> Vec<TracerouteHop> {
    state.hops().iter().map(|h| TracerouteHop {
        hop: h.ttl(),
        // Legacy fields left None — server discriminates via total_sent.
        ip: None,
        hostname: None,
        rtt1: None, rtt2: None, rtt3: None,
        asn: None,
        ips: h.addrs().map(|a| a.to_string()).collect(),
        total_sent: Some(h.total_sent() as u32),
        total_recv: Some(h.total_recv() as u32),
        loss_pct: Some(h.loss_pct()),
        best_ms: h.best_ms(),
        worst_ms: h.worst_ms(),
        avg_ms: Some(h.avg_ms()),
        stddev_ms: Some(h.stddev_ms()),
        jitter_ms: h.jitter_ms(),
    }).collect()
}

pub fn spawn_traceroute(
    request_id: String,
    target: String,
    max_hops: u8,
    protocol: TraceProtocol,
    tx: mpsc::Sender<AgentMessage>,
) {
    tokio::spawn(async move {
        let addr = match resolve_target(&target).await {
            Ok(a) => a,
            Err(e) => {
                let _ = tx.send(AgentMessage::TracerouteRoundUpdate {
                    request_id, target,
                    round: 0, total_rounds: 0, hops: vec![],
                    completed: true,
                    error: Some(format!("DNS error: {e}")),
                }).await;
                return;
            }
        };
        let proto = trippy_protocol_from(protocol);
        let request_id_inner = request_id.clone();
        let target_inner = target.clone();
        let tx_inner = tx.clone();

        let result = tokio::task::spawn_blocking(move || -> Result<(), String> {
            let state = RefCell::new(trippy_core::State::default());
            let round_no = Cell::new(0u32);
            let callback = |round: &trippy_core::Round<'_>| {
                state.borrow_mut().update_from_round(round);
                round_no.set(round_no.get() + 1);
                let n = round_no.get();
                let completed = n >= DEFAULT_MAX_ROUNDS;
                let hops = hops_from_state(&state.borrow());
                let _ = tx_inner.blocking_send(AgentMessage::TracerouteRoundUpdate {
                    request_id: request_id_inner.clone(),
                    target: target_inner.clone(),
                    round: n,
                    total_rounds: DEFAULT_MAX_ROUNDS,
                    hops,
                    completed,
                    error: None,
                });
            };

            // Try privileged → fallback to unprivileged on privilege errors.
            match try_trace(addr, max_hops, proto, PrivilegeMode::Privileged, &callback) {
                Ok(()) => Ok(()),
                Err(e) if is_privilege_error(&e) => {
                    tracing::info!("traceroute privileged failed ({e}); retrying unprivileged");
                    match try_trace(addr, max_hops, proto, PrivilegeMode::Unprivileged, &callback) {
                        Ok(()) => Ok(()),
                        Err(e2) => Err(format!("{}: {e2}", platform_guidance())),
                    }
                }
                Err(e) => Err(format!("Tracer error: {e}")),
            }
        }).await;

        // Emit terminal error if the blocking task crashed or returned Err.
        match result {
            Ok(Ok(())) => {} // success — final round message already sent with completed=true
            Ok(Err(msg)) => {
                let _ = tx.send(AgentMessage::TracerouteRoundUpdate {
                    request_id, target,
                    round: 0, total_rounds: 0, hops: vec![],
                    completed: true,
                    error: Some(msg),
                }).await;
            }
            Err(join_err) => {
                let _ = tx.send(AgentMessage::TracerouteRoundUpdate {
                    request_id, target,
                    round: 0, total_rounds: 0, hops: vec![],
                    completed: true,
                    error: Some(format!("Tracer task panicked: {join_err}")),
                }).await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use trippy_core::{PortDirection, Protocol};

    #[test]
    fn test_is_valid_traceroute_target() {
        assert!(is_valid_traceroute_target("8.8.8.8"));
        assert!(is_valid_traceroute_target("google.com"));
        assert!(is_valid_traceroute_target("sub.example.com"));
        assert!(is_valid_traceroute_target("2001:db8::1"));
        assert!(is_valid_traceroute_target("my-server.example.com"));
        assert!(!is_valid_traceroute_target(""));
        assert!(!is_valid_traceroute_target("8.8.8.8; rm -rf /"));
        assert!(!is_valid_traceroute_target("$(whoami)"));
        assert!(!is_valid_traceroute_target("foo bar"));
    }

    #[test]
    fn test_port_direction_for_icmp_is_none() {
        assert!(matches!(port_direction_for(Protocol::Icmp), PortDirection::None));
    }

    #[test]
    fn test_port_direction_for_udp_is_fixed_dest_33434() {
        match port_direction_for(Protocol::Udp) {
            PortDirection::FixedDest(Port(p)) => assert_eq!(p, 33_434),
            other => panic!("expected FixedDest(33434), got {other:?}"),
        }
    }

    #[test]
    fn test_port_direction_for_tcp_is_fixed_dest_80() {
        match port_direction_for(Protocol::Tcp) {
            PortDirection::FixedDest(Port(p)) => assert_eq!(p, 80),
            other => panic!("expected FixedDest(80), got {other:?}"),
        }
    }

    #[test]
    fn test_builder_builds_for_all_three_protocols() {
        // Regression guard against trippy's BadConfig when UDP/TCP are paired
        // with PortDirection::None. We only build (no run) so this doesn't
        // require raw socket privileges in CI.
        let addr: IpAddr = "1.1.1.1".parse().unwrap();
        for proto in [Protocol::Icmp, Protocol::Udp, Protocol::Tcp] {
            let res = Builder::new(addr)
                .max_ttl(5)
                .max_rounds(Some(1))
                .protocol(proto)
                .port_direction(port_direction_for(proto))
                .privilege_mode(PrivilegeMode::Privileged)
                .build();
            assert!(res.is_ok(), "build failed for {proto:?}: {:?}", res.err());
        }
    }

    #[tokio::test]
    async fn test_resolve_literal_ipv4() {
        assert_eq!(resolve_target("8.8.8.8").await.unwrap(), "8.8.8.8".parse::<IpAddr>().unwrap());
    }

    #[tokio::test]
    async fn test_resolve_literal_ipv6() {
        assert_eq!(resolve_target("::1").await.unwrap(), "::1".parse::<IpAddr>().unwrap());
    }

    #[tokio::test]
    async fn test_resolve_invalid_hostname_returns_err() {
        let result = resolve_target("definitely-not-a-real-tld.invalid").await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Register the module**

In `crates/agent/src/main.rs` (or wherever modules are declared), add `mod traceroute;`.

- [ ] **Step 4: Build and run unit tests**

Run: `cargo build -p serverbee-agent && cargo test -p serverbee-agent traceroute -- --nocapture`
Expected: build succeeds; tests pass. The `test_resolve_invalid_hostname_returns_err` test depends on `.invalid` not resolving — this is the standard reserved TLD per RFC 2606.

- [ ] **Step 5: Commit**

```bash
git add crates/agent/Cargo.toml crates/agent/src/traceroute.rs crates/agent/src/main.rs Cargo.lock
git commit -m "feat(agent): introduce trippy-core traceroute module"
```

---

### Task 15: Replace the old shell traceroute in `reporter.rs`

**Files:**
- Modify: `crates/agent/src/reporter.rs` (delete the old shell impl; redirect the message handler to the new module)

- [ ] **Step 1: Update the `ServerMessage::Traceroute` arm**

Find the existing arm (around `reporter.rs:826-855`) and replace its body with:

```rust
ServerMessage::Traceroute { request_id, target, max_hops, protocol } => {
    let caps = capabilities.load(Ordering::SeqCst);
    if !has_capability(caps, CAP_PING_ICMP) {
        // Existing CapabilityDenied path — keep as-is. Reuse whatever
        // helper currently constructs the denied message.
        let denied = build_capability_denied_message(
            request_id.clone(),
            CAP_PING_ICMP,
            CapabilityDeniedReason::Disabled,
        );
        let tx = cmd_result_tx.clone();
        tokio::spawn(async move { let _ = tx.send(denied).await; });
        return Ok(ServerMessageOutcome::Continue);
    }
    if !crate::traceroute::is_valid_traceroute_target(&target) {
        tracing::warn!("Traceroute rejected: invalid target '{target}' (request_id={request_id})");
        let tx = cmd_result_tx.clone();
        let request_id_c = request_id.clone();
        let target_c = target.clone();
        tokio::spawn(async move {
            let _ = tx.send(AgentMessage::TracerouteRoundUpdate {
                request_id: request_id_c,
                target: target_c,
                round: 0, total_rounds: 0, hops: vec![],
                completed: true,
                error: Some("Invalid target: must be a domain or IP address".into()),
            }).await;
        });
        return Ok(ServerMessageOutcome::Continue);
    }
    let proto = protocol.unwrap_or(serverbee_common::protocol::TraceProtocol::Icmp);
    tracing::info!(
        "Executing traceroute to {target} (max_hops={max_hops}, request_id={request_id}, protocol={proto:?})"
    );
    crate::traceroute::spawn_traceroute(
        request_id,
        target,
        max_hops,
        proto,
        cmd_result_tx.clone(),
    );
    Ok(ServerMessageOutcome::Continue)
}
```

- [ ] **Step 2: Delete the old functions**

Delete from `reporter.rs`:
- `execute_traceroute` (the entire async function)
- `parse_traceroute_output` and `parse_traceroute_line`
- The legacy `is_valid_traceroute_target` (it's now in the new module)
- The unit tests for those functions (`test_is_valid_traceroute_target`, any `test_parse_traceroute*` tests) — they are migrated / superseded

- [ ] **Step 3: Build and test**

Run: `cargo build -p serverbee-agent && cargo test -p serverbee-agent`
Expected: compiles; all remaining tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/reporter.rs
git commit -m "feat(agent): replace shell traceroute with trippy-core dispatch"
```

---

## Phase 8 — Frontend types + hooks

### Task 16: Update `network-types.ts`

**Files:**
- Modify: `apps/web/src/lib/network-types.ts:87-102` (TracerouteHop / TracerouteResult)

- [ ] **Step 1: Replace the types and add new ones**

Replace the existing `TracerouteHop` and `TracerouteResult` blocks with:

```ts
export type RecordedProtocol = 'icmp' | 'udp' | 'tcp' | 'legacy'
export type TraceProtocol = 'icmp' | 'udp' | 'tcp'

// Rust serializes Option::None with skip_serializing_if = "Option::is_none",
// so old-agent JSON OMITS the new keys entirely. In JS that means
// `total_sent === undefined`, not null. All discriminator checks MUST use
// loose `value != null` to catch both undefined and null.
export interface TracerouteHop {
  hop: number
  // Legacy fields (filled only by old shell-based agents)
  ip?: string | null
  hostname: string | null
  rtt1?: number | null
  rtt2?: number | null
  rtt3?: number | null
  asn: string | null
  // New fields (filled by trippy-core agent); absent from old-agent payloads
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
  request_id: string
  target: string
  /** 'legacy' = run by a pre-trippy agent; actual probe protocol unknown */
  protocol: RecordedProtocol
  started_at: number
  completed_at: number | null
  round: number
  total_rounds: number
  hops: TracerouteHop[]
  completed: boolean
  error: string | null
}

export interface TracerouteRecordSummary {
  request_id: string
  target: string
  protocol: RecordedProtocol
  started_at: number
  completed_at: number | null
  hop_count: number
  has_error: boolean
}

export interface TracerouteResponse {
  request_id: string
}

export function isNewSchemaHop(hop: TracerouteHop): boolean {
  return hop.total_sent != null
}
```

- [ ] **Step 2: Find and fix any consumers that used the old shape**

Run: `bun run typecheck` from `apps/web` (or repo root) — fix any TS errors caused by callers that destructured the old `TracerouteResult` shape. Likely call sites:
- `apps/web/src/routes/_authed/network/$serverId.tsx::TracerouteContent` — will be fully rewritten in Task 19, but make sure the file still compiles after this task by either adjusting access patterns or temporarily using `??` defaults.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/lib/network-types.ts
git commit -m "feat(web): extend Traceroute TS types with trippy fields"
```

---

### Task 17: Update / add traceroute hooks

**Files:**
- Modify: `apps/web/src/hooks/use-network-api.ts:138-151` (existing hooks)
- Create: `apps/web/src/hooks/use-traceroute-stream.ts`
- Modify: `apps/web/src/hooks/use-servers-ws.ts` (add `traceroute_update` dispatcher branch — see step 3 below)

- [ ] **Step 1: Replace the existing two hooks and add four new ones**

Replace the existing `useStartTraceroute` / `useTracerouteResult` blocks with:

```ts
export function useStartTraceroute(serverId: string) {
  return useMutation({
    mutationFn: (input: { target: string; protocol: TraceProtocol }) =>
      api.post<TracerouteResponse>(`/api/servers/${serverId}/traceroute`, input),
  })
}

export function useTracerouteRecord(serverId: string, requestId: string | null) {
  return useQuery<TracerouteResult>({
    queryKey: ['servers', serverId, 'traceroute', requestId],
    queryFn: () => api.get(`/api/servers/${serverId}/traceroute/${requestId}`),
    enabled: !!requestId,
    refetchInterval: (query) => (query.state.data?.completed ? false : 2000),
  })
}

export function useTracerouteHistory(serverId: string) {
  return useQuery<TracerouteRecordSummary[]>({
    queryKey: ['servers', serverId, 'traceroute-history'],
    queryFn: () => api.get(`/api/servers/${serverId}/traceroute?limit=50`),
  })
}

export function useDeleteTraceroute(serverId: string) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (requestId: string) =>
      api.delete(`/api/servers/${serverId}/traceroute/${requestId}`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['servers', serverId, 'traceroute-history'] })
    },
  })
}

export function useClearTracerouteHistory(serverId: string) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => api.delete(`/api/servers/${serverId}/traceroute`),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['servers', serverId, 'traceroute-history'] })
    },
  })
}
```

Make sure imports at the top of the file include `TraceProtocol`, `TracerouteResult`, `TracerouteResponse`, `TracerouteRecordSummary`, and `useQueryClient`.

- [ ] **Step 2: Create the WS stream hook**

```ts
// apps/web/src/hooks/use-traceroute-stream.ts
import { useEffect, useState } from 'react'
import type { TracerouteHop, RecordedProtocol } from '@/lib/network-types'
import { subscribeBrowserMessage } from '@/hooks/use-servers-ws'

export interface TracerouteStreamState {
  request_id: string
  target: string
  protocol: RecordedProtocol
  started_at: number
  round: number
  total_rounds: number
  hops: TracerouteHop[]
  completed: boolean
  error: string | null
}

export function useTracerouteStream(
  serverId: string,
  requestId: string | null,
): TracerouteStreamState | null {
  const [data, setData] = useState<TracerouteStreamState | null>(null)

  useEffect(() => {
    setData(null)
    if (!requestId) return
    return subscribeBrowserMessage('traceroute_update', (msg: any) => {
      if (msg.server_id !== serverId || msg.request_id !== requestId) return
      setData({
        request_id: msg.request_id,
        target: msg.target,
        protocol: msg.protocol,
        started_at: msg.started_at,
        round: msg.round,
        total_rounds: msg.total_rounds,
        hops: msg.hops,
        completed: msg.completed,
        error: msg.error ?? null,
      })
    })
  }, [serverId, requestId])

  return data
}
```

- [ ] **Step 3: Wire the dispatcher**

In `apps/web/src/hooks/use-servers-ws.ts`, locate the central WS message dispatcher. Add (if not present) a generic `subscribeBrowserMessage(type, handler)` function that maintains a `Map<type, Set<handler>>` and is invoked from the main `onmessage` switch. If a similar primitive already exists under a different name (e.g., the existing code already routes `network_update` to other hooks), reuse the same pattern. **Critical:** keep the existing message routing for `update`, `full_sync`, `network_probe_update`, etc., untouched — only add a new `case 'traceroute_update':` branch.

Pseudo-pattern (adapt to the actual file structure):

```ts
// inside the message switch
case 'traceroute_update':
  dispatchToSubscribers('traceroute_update', payload)
  break
```

If `use-servers-ws.ts` has no general-purpose dispatcher, introduce a minimal one:

```ts
type Handler = (msg: any) => void
const subscribers = new Map<string, Set<Handler>>()

export function subscribeBrowserMessage(type: string, handler: Handler) {
  let set = subscribers.get(type)
  if (!set) { set = new Set(); subscribers.set(type, set) }
  set.add(handler)
  return () => { set!.delete(handler) }
}

function dispatchToSubscribers(type: string, msg: any) {
  subscribers.get(type)?.forEach((h) => h(msg))
}
```

- [ ] **Step 4: Typecheck**

Run: `bun run typecheck`
Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/hooks/use-network-api.ts apps/web/src/hooks/use-traceroute-stream.ts apps/web/src/hooks/use-servers-ws.ts
git commit -m "feat(web): add traceroute hooks (history, stream, delete, clear)"
```

---

## Phase 9 — Frontend dialog UI

### Task 18: Add protocol dropdown to the Run form + lift state to page

**Files:**
- Modify: `apps/web/src/routes/_authed/network/$serverId.tsx` (TracerouteContent + NetworkDetailPage)

- [ ] **Step 1: Lift trace state and add protocol state at page level**

Inside `NetworkDetailPage`, add:

```tsx
const [traceTarget, setTraceTarget] = useState('')
const [traceProtocol, setTraceProtocol] = useState<TraceProtocol>('icmp')
const [traceRequestId, setTraceRequestId] = useState<string | null>(null)
const [selectedRecordId, setSelectedRecordId] = useState<string | null>(null)
```

Pass these (with setters) into `<TracerouteContent />`.

- [ ] **Step 2: Add the Select to the form**

In `TracerouteContent`, replace the `<div className="flex gap-2"> Input, Button </div>` block with:

```tsx
{isAdmin && (
  <div className="flex gap-2">
    <Input
      disabled={isRunning || startTraceroute.isPending}
      onChange={(e) => setTarget(e.target.value)}
      onKeyDown={handleKeyDown}
      placeholder={t('traceroute_target')}
      value={target}
    />
    <Select value={protocol} onValueChange={(v) => setProtocol(v as TraceProtocol)}>
      <SelectTrigger className="w-24">
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="icmp">ICMP</SelectItem>
        <SelectItem value="udp">UDP</SelectItem>
        <SelectItem value="tcp">TCP</SelectItem>
      </SelectContent>
    </Select>
    <Button disabled={!target.trim() || isRunning || startTraceroute.isPending} onClick={handleRun} size="sm">
      {isRunning || startTraceroute.isPending ? (
        <Loader2 aria-hidden="true" className="mr-1 size-4 animate-spin" />
      ) : (
        <Play aria-hidden="true" className="mr-1 size-4" />
      )}
      {isRunning ? t('traceroute_running') : t('run_traceroute')}
    </Button>
  </div>
)}
{!isAdmin && (
  <p className="text-muted-foreground text-xs">{t('traceroute_readonly_note')}</p>
)}
```

`isAdmin` is read from `useAuth()` at the top of `TracerouteContent`. The handler `handleRun` now calls `startTraceroute.mutate({ target: target.trim(), protocol }, { onSuccess: (data) => setTraceRequestId(data.request_id) })` and clears `selectedRecordId`.

Add the Select import: `import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'`. Add the `TraceProtocol` import from `@/lib/network-types`. Add i18n keys `traceroute_readonly_note` (cn: `仅可查看历史，触发追踪需管理员权限`; en: `View-only. Triggering a trace requires admin.`) to the `network` namespace.

- [ ] **Step 3: Build, typecheck, and visually verify**

Run: `bun run typecheck`
Then start dev: `make web-dev-prod` (or `cd apps/web && bun run dev`) and visually confirm in browser that:
- Admin sees Run form with the protocol Select.
- Member account (use a member key via cookie swap if available) does not see the form, but sees the readonly note.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/network/\$serverId.tsx apps/web/src/locales
git commit -m "feat(web): protocol dropdown and admin gating in traceroute dialog"
```

---

### Task 19: New 10-column streaming table with `!= null` discriminator

**Files:**
- Modify: `apps/web/src/routes/_authed/network/$serverId.tsx` (TracerouteContent, hop table)

- [ ] **Step 1: Replace the hop table body**

Replace the existing Hop / IP / Host / RTT1/2/3 / ASN table inside `TracerouteContent` with:

```tsx
{result && result.hops.length > 0 && (
  <div className="max-h-[60vh] overflow-auto rounded-md border">
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead className="w-12">{t('hop')}</TableHead>
          <TableHead>{t('ip_address')}</TableHead>
          <TableHead>{t('hostname')}</TableHead>
          <TableHead>{t('asn')}</TableHead>
          <TableHead className="text-right">{t('loss_pct')}</TableHead>
          <TableHead className="text-right">{t('best')}</TableHead>
          <TableHead className="text-right">{t('avg')}</TableHead>
          <TableHead className="text-right">{t('worst')}</TableHead>
          <TableHead className="text-right">{t('jitter')}</TableHead>
          <TableHead className="text-right">{t('stddev')}</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {result.hops.map((hop) => <HopRow key={hop.hop} hop={hop} />)}
      </TableBody>
    </Table>
  </div>
)}
```

Add a private `HopRow` component in the same file:

```tsx
function HopRow({ hop }: { hop: TracerouteHop }) {
  const isNew = isNewSchemaHop(hop)
  const primaryIp = isNew
    ? hop.ips?.[0] ?? null
    : hop.ip ?? null
  const extraIpCount = isNew && (hop.ips?.length ?? 0) > 1 ? (hop.ips!.length - 1) : 0
  const dimmed = isNew
    ? (hop.total_recv ?? 0) === 0
    : hop.rtt1 == null && hop.rtt2 == null && hop.rtt3 == null

  const legacyRtts = [hop.rtt1, hop.rtt2, hop.rtt3].filter((v): v is number => v != null)
  const lossPct = isNew
    ? hop.loss_pct ?? null
    : (legacyRtts.length === 0 ? 100 : ((3 - legacyRtts.length) / 3) * 100)
  const bestMs = isNew ? hop.best_ms ?? null : legacyRtts.length ? Math.min(...legacyRtts) : null
  const avgMs = isNew ? hop.avg_ms ?? null : legacyRtts.length ? legacyRtts.reduce((a, b) => a + b, 0) / legacyRtts.length : null
  const worstMs = isNew ? hop.worst_ms ?? null : legacyRtts.length ? Math.max(...legacyRtts) : null

  return (
    <TableRow className={cn(dimmed && 'opacity-50')}>
      <TableCell className="font-mono">{hop.hop}</TableCell>
      <TableCell className="font-mono">
        {primaryIp ?? '* * *'}
        {extraIpCount > 0 && (
          <Tooltip>
            <TooltipTrigger asChild>
              <Badge className="ml-1" variant="secondary">+{extraIpCount}</Badge>
            </TooltipTrigger>
            <TooltipContent>{hop.ips!.slice(1).join(', ')}</TooltipContent>
          </Tooltip>
        )}
      </TableCell>
      <TableCell className="text-muted-foreground max-w-[200px] truncate">{hop.hostname ?? '—'}</TableCell>
      <TableCell className="text-muted-foreground">{hop.asn ?? '—'}</TableCell>
      <TableCell className={cn('text-right font-mono', getLossTextClassName(lossPct == null ? null : lossPct / 100))}>
        {lossPct == null ? '—' : `${lossPct.toFixed(0)}%`}
      </TableCell>
      <TableCell className="text-right font-mono">{bestMs == null ? '—' : `${bestMs.toFixed(1)}`}</TableCell>
      <TableCell className={cn('text-right font-mono', latencyColorClass(avgMs, { failed: dimmed }))}>
        {avgMs == null ? '—' : `${avgMs.toFixed(1)}`}
      </TableCell>
      <TableCell className="text-right font-mono">{worstMs == null ? '—' : `${worstMs.toFixed(1)}`}</TableCell>
      <TableCell className="text-right font-mono">{hop.jitter_ms == null ? '—' : `${hop.jitter_ms.toFixed(2)}`}</TableCell>
      <TableCell className="text-right font-mono">{hop.stddev_ms == null ? '—' : `${hop.stddev_ms.toFixed(2)}`}</TableCell>
    </TableRow>
  )
}
```

Add imports as needed: `Tooltip`, `TooltipTrigger`, `TooltipContent` from `@/components/ui/tooltip`; `Badge` from `@/components/ui/badge`; `getLossTextClassName` and `latencyColorClass` from `@/lib/network-types`; `cn` from `@/lib/utils`; `isNewSchemaHop` and `TracerouteHop` from `@/lib/network-types`.

Wire the new `useTracerouteStream` hook into `TracerouteContent`:

```tsx
const stream = useTracerouteStream(serverId, traceRequestId)
const { data: polled } = useTracerouteRecord(serverId, selectedRecordId ?? (stream?.completed ? null : traceRequestId))
const result = selectedRecordId ? polled ?? null : (stream ?? polled ?? null)
```

The `selectedRecordId` represents the user clicking a history record; when set, show that record's snapshot. When unset and a trace is running, show the streaming state. The HTTP `useTracerouteRecord` is the WS-disconnect fallback.

Add a small round progress indicator in the header area inside the dialog body:

```tsx
{stream && !stream.completed && (
  <span className="text-muted-foreground text-xs tabular-nums">
    {t('round_progress', { current: stream.round, total: stream.total_rounds })}
  </span>
)}
```

i18n keys to add to `network` namespace (cn / en):
- `loss_pct`: `丢包率` / `Loss%`
- `best`: `最优` / `Best`
- `avg`: `平均` / `Avg`
- `worst`: `最差` / `Worst`
- `jitter`: `抖动` / `Jitter`
- `stddev`: `标准差` / `StdDev`
- `round_progress`: `第 {{current}} 轮 / 共 {{total}} 轮` / `Round {{current}} / {{total}}`

- [ ] **Step 2: Typecheck**

Run: `bun run typecheck`
Expected: passes.

- [ ] **Step 3: Visual verify in browser**

Start dev server. Open the network detail dialog and trigger a trace; verify the table renders 10 columns and updates round by round.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/network/\$serverId.tsx apps/web/src/locales
git commit -m "feat(web): 10-column streaming hop table with loose null discriminator"
```

---

### Task 20: History list section with admin-gated delete and clear

**Files:**
- Modify: `apps/web/src/routes/_authed/network/$serverId.tsx`

- [ ] **Step 1: Add the history list UI**

Below the hop table in `TracerouteContent`, add:

```tsx
<div className="mt-4 border-t pt-4">
  <div className="mb-2 flex items-center justify-between">
    <h3 className="text-sm font-medium">{t('history')} ({history?.length ?? 0})</h3>
    {isAdmin && (history?.length ?? 0) > 0 && (
      <Button size="sm" variant="ghost" onClick={() => {
        if (window.confirm(t('clear_all_confirm', { count: history!.length }))) {
          clearMutation.mutate()
        }
      }}>
        {t('clear_all')}
      </Button>
    )}
  </div>
  {history?.length === 0 && (
    <p className="text-muted-foreground text-sm">{t('history_empty')}</p>
  )}
  <div className="max-h-64 overflow-auto">
    <ul className="space-y-1">
      {history?.map((r) => (
        <li
          key={r.request_id}
          className={cn(
            'flex items-center gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-muted/40 cursor-pointer',
            selectedRecordId === r.request_id && 'bg-muted',
          )}
          onClick={() => setSelectedRecordId(r.request_id)}
        >
          <span className="font-mono flex-1 truncate">{r.target}</span>
          <Badge variant={r.protocol === 'legacy' ? 'outline' : 'secondary'}>
            {r.protocol === 'legacy' ? (
              <Tooltip>
                <TooltipTrigger asChild><span>legacy</span></TooltipTrigger>
                <TooltipContent>{t('legacy_record_tooltip')}</TooltipContent>
              </Tooltip>
            ) : r.protocol.toUpperCase()}
          </Badge>
          <span className="text-muted-foreground text-xs">{r.hop_count} hops</span>
          <span className="text-muted-foreground text-xs">{formatRelativeTime(r.started_at)}</span>
          {r.has_error ? <X className="size-3 text-destructive" /> : <Check className="size-3 text-emerald-500" />}
          {isAdmin && (
            <Button
              size="icon"
              variant="ghost"
              onClick={(e) => { e.stopPropagation(); deleteMutation.mutate(r.request_id) }}
              aria-label={t('delete')}
            >
              <Trash2 className="size-4" />
            </Button>
          )}
        </li>
      ))}
    </ul>
  </div>
</div>
```

Add at the top of `TracerouteContent`:

```tsx
const { data: history } = useTracerouteHistory(serverId)
const deleteMutation = useDeleteTraceroute(serverId)
const clearMutation = useClearTracerouteHistory(serverId)
```

Add `formatRelativeTime` helper (or import from a shared util if one exists):

```tsx
function formatRelativeTime(unixMs: number): string {
  const diff = Date.now() - unixMs
  if (diff < 60_000) return 'just now'
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`
  return `${Math.floor(diff / 86_400_000)}d ago`
}
```

i18n additions (network namespace):
- `history` / `历史记录`
- `clear_all` / `清空全部`
- `clear_all_confirm` / `将删除 {{count}} 条历史记录，确定？` / `Delete {{count}} history record(s)?`
- `history_empty` / `暂无历史` / `No history yet.`
- `legacy_record_tooltip` / `由旧版 Agent 生成，实际协议未知` / `Recorded by a pre-trippy agent; actual probe protocol unknown.`
- `delete` / `删除` / `Delete`

Lucide imports: `Trash2`, `Check`, `X`.

- [ ] **Step 2: Verify mutually-exclusive state**

When `handleRun` fires, it should set `traceRequestId` and clear `selectedRecordId`. When a history item is clicked it sets `selectedRecordId` and clears `traceRequestId`. Confirm both are wired in the parent component.

- [ ] **Step 3: Typecheck and visual verify**

Run: `bun run typecheck`
Then start dev server and exercise:
- Run a trace → completes → history list grows by one
- Click an older entry → table swaps to show that record
- Delete one → confirms via window prompt → list shrinks
- Clear all → list empties
- Member account → history list is read-only; no trash icons, no clear button

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/network/\$serverId.tsx apps/web/src/locales
git commit -m "feat(web): traceroute history list with admin delete and clear"
```

---

## Phase 10 — Docs + Manual Checklist

### Task 21: Update documentation

**Files:**
- Modify: `apps/docs/content/docs/cn/monitoring.mdx:408-424` (Traceroute subsection)
- Modify: `apps/docs/content/docs/en/monitoring.mdx` (matching subsection)

- [ ] **Step 1: Rewrite the cn Traceroute subsection**

Replace the existing `### Traceroute` block in `apps/docs/content/docs/cn/monitoring.mdx` with:

```mdx
### Traceroute

网络详情页可以从选中的 Agent 对目标主机或 IP 发起 Traceroute，用于排查路由变化和丢包问题。

- 需要 Agent 的 effective `CAP_PING_ICMP` 能力。
- 目标只允许字母、数字、点、连字符和冒号。
- Dialog 提供 ICMP / UDP / TCP 三种探测协议下拉选择。
- 默认跑 5 轮，每轮一次增量更新通过 WebSocket 推送到浏览器，表格实时填充：
  Hop / IP / 主机名 / ASN / 丢包率 / Best / Avg / Worst / Jitter / StdDev。
- ECMP 多路径时同一 TTL 会有多个 IP，第一行显示主 IP，悬停 `+N` chip 查看其他 IP。
- 已完成的 Traceroute 自动保存到本地 SQLite，可以在 dialog 历史区点击切换查看；
  管理员可以删除单条或一键清空。
- POST 触发需要管理员权限；只读用户能看到历史，但不能发起新 trace 或删除。

#### 权限

ServerBee Agent 现在内嵌 [trippy-core](https://crates.io/crates/trippy-core) 直接发起 raw ICMP/UDP/TCP 包，
**不再依赖系统的 `traceroute` / `mtr` 二进制**。但 raw socket 仍需要操作系统级权限：

| 平台 | 要求 |
|------|------|
| Linux | `CAP_NET_RAW` 或以 root 运行。一次性配置：`sudo setcap cap_net_raw+ep $(which serverbee-agent)` |
| macOS | 以 root 运行 (sudo)，或部分场景下可走 unprivileged ICMP datagram socket |
| Windows | 以 Administrator 身份启动 Agent |

权限不足时 Traceroute 会立刻返回带平台对应安装提示的错误消息，不需要重启 Agent。

#### API 流程

```http
POST   /api/servers/{id}/traceroute          { target, protocol? }   → { request_id }
GET    /api/servers/{id}/traceroute/{rid}    → 最新快照（含 protocol/started_at/round/total_rounds 等完整字段）
GET    /api/servers/{id}/traceroute          → 该服务器的历史记录摘要列表，按时间倒序
DELETE /api/servers/{id}/traceroute/{rid}    → 删除单条（admin）
DELETE /api/servers/{id}/traceroute          → 清空全部（admin）
```

WebSocket 推送 `BrowserMessage::TracerouteUpdate` 携带每轮增量结果。客户端在断开重连时可通过 GET 单条接口拉取最新累计快照。
```

- [ ] **Step 2: Mirror the change in en**

Apply equivalent English content to `apps/docs/content/docs/en/monitoring.mdx`.

- [ ] **Step 3: Build docs typecheck**

Run: `bun --filter @serverbee/docs types:check`
Expected: passes.

- [ ] **Step 4: Commit**

```bash
git add apps/docs/content/docs/cn/monitoring.mdx apps/docs/content/docs/en/monitoring.mdx
git commit -m "docs: traceroute now uses embedded trippy-core with history + protocol"
```

---

### Task 22: Add manual E2E checklist

**Files:**
- Create: `tests/traceroute.md`

- [ ] **Step 1: Create the checklist**

```markdown
# Traceroute Manual E2E Checklist

Run through this list after deploying changes to the agent + server. Repeat once per supported platform if possible.

## Setup

- Agent built from this branch deployed to a test VPS (Linux + macOS recommended).
- Either run agent as root, or apply: `sudo setcap cap_net_raw+ep $(which serverbee-agent)`.
- Server running with the new migration applied.
- Two browser sessions: one admin, one member.

## Happy path (ICMP)

- [ ] Admin: open network detail page → click "路由追踪" → enter `1.1.1.1` → protocol ICMP → Run
- [ ] Verify the table fills hop-by-hop within ~5 seconds; round counter goes 1→5
- [ ] Final hop shows non-zero `total_recv`, valid `avg_ms`, hostname populated where reverse DNS works
- [ ] After completion, a new entry appears in the history list with `icmp` chip
- [ ] Close dialog and reopen → newest record is the one we just ran

## UDP

- [ ] Admin: trigger trace to `1.1.1.1` with protocol UDP
- [ ] Verify it completes without `BadConfig` (regression for the PortDirection fix)
- [ ] Hops mostly populated; some intermediate hops may show ICMP TimeExceeded fine

## TCP

- [ ] Admin: trigger trace to `1.1.1.1` with protocol TCP
- [ ] Verify completion; route may differ from ICMP due to load balancers

## Privilege fallback

- [ ] Linux: stop the agent, remove CAP_NET_RAW (`sudo setcap -r $(which serverbee-agent)`),
      restart agent as non-root
- [ ] Trigger any trace; verify the error toast contains the setcap one-liner
- [ ] Re-apply setcap; verify next trace succeeds without restarting the agent

## ECMP / multi-IP

- [ ] Trace to a target known to use ECMP (e.g., `cloudflare.com`)
- [ ] Some hops show `+N` chip; hover reveals the alternate IPs

## History + admin gating

- [ ] Member account: open the dialog
- [ ] Verify no Run form is visible; instead see the read-only note
- [ ] History list is visible; clicking a row shows the snapshot in the table
- [ ] No trash icons, no Clear all button
- [ ] Admin: delete a single record → list shrinks
- [ ] Admin: Clear all → confirm dialog → list empties

## WebSocket reconnect / refresh

- [ ] Admin: trigger a long trace (e.g., target with high TTL like a route to Australia from US)
- [ ] Mid-flight, hard-refresh the page → reopen the dialog → snapshot continues to update via the GET fallback
- [ ] Confirm completion still records to history

## Stale-meta drop

- [ ] Trigger a trace and immediately delete the cache by restarting the server (history is preserved in DB)
- [ ] Verify the running trace no longer streams to the UI (cache evicted) but completed result lands in DB

## Capability denied

- [ ] On the server side, disable `CAP_PING_ICMP` for the test server
- [ ] Trigger a trace → result shows the capability-denied error immediately, no agent activity
```

- [ ] **Step 2: Commit**

```bash
git add tests/traceroute.md
git commit -m "docs: add traceroute manual E2E checklist"
```

---

## Self-Review

After implementing all tasks, the implementer should run:

```bash
cargo build --workspace
cargo clippy --workspace -- -D warnings
cargo test --workspace
cd apps/web && bun run typecheck && bun x ultracite check
cd apps/docs && bun run types:check
```

Then run through `tests/traceroute.md` on a real Linux VPS to verify the privilege flow and the streaming UI under realistic latency.
