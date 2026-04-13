# Memory & Frontend Optimization Design

**Date**: 2026-04-13
**Status**: Draft
**Scope**: Server + Agent memory optimization, Frontend performance & logic improvements
**Approach**: Targeted structural improvements (方案 B)

## Context

ServerBee is a VPS monitoring system (Rust server + agent, React SPA). Current deployment targets 50+ servers. This is a preventive optimization effort — no acute symptoms, but the codebase has patterns that will become bottlenecks at scale. The goal is moderate refactoring on hot paths with independently testable changes.

## Constraints

- Each change must be independently mergeable and testable
- No architectural redesigns (e.g., broadcast channel overhaul, streaming file transfer)
- Behavior must remain identical — pure performance/memory improvements
- No new dependencies except `@tanstack/react-virtual` for frontend virtualization

---

## Server Optimizations

### S1. AgentManager — Arc\<SystemReport\> Shared Ownership

**Problem**: `latest_reports: DashMap<String, CachedReport>` stores `SystemReport` by value. Every call to `get_latest_report()` and `all_latest_reports()` performs a full deep clone of `SystemReport` (which contains `Vec<DiskIo>`, `Option<GpuReport>` with `Vec<GpuInfo>`). Hot callers:

- `record_writer.rs`: `all_latest_reports()` every 60s — clones N reports
- `browser.rs`: `build_full_sync()` calls `get_latest_report()` per server on every browser connect and lag resync
- Alert evaluator: periodic lookups

At 50 servers, this is 50+ unnecessary deep copies per minute.

**Change**:

```rust
// CachedReport field changes from:
pub report: SystemReport
// to:
pub report: Arc<SystemReport>
```

- `update_report()` wraps incoming report in `Arc::new(report)`
- `get_latest_report()` returns `Option<Arc<SystemReport>>` (was `Option<SystemReport>`)
- `all_latest_reports()` returns `Vec<(String, Arc<SystemReport>)>` (was `Vec<(String, SystemReport)>`)
- Callers dereference via `&*arc` or auto-deref — no business logic changes needed

**Files**: `crates/server/src/service/agent_manager.rs`, `crates/server/src/task/record_writer.rs`, `crates/server/src/router/ws/browser.rs`

### S2. Docker Cache Cleanup for Offline Agents

**Problem**: `docker_containers`, `docker_stats`, `docker_info` DashMaps in AgentManager are cleaned on agent disconnect (`handle_disconnect`), but if an agent reconnects without going through the full disconnect flow, stale entries persist. More importantly, during normal operation there is no eviction — data grows with each Docker update.

**Change**: In the `offline_checker` task (runs every 10s), after checking agent connectivity, add a sweep that removes Docker cache entries for servers no longer in `connections`:

```rust
// In offline_checker, after existing offline checks:
let connected: HashSet<&str> = self.connections.iter().map(|e| e.key().as_str()).collect();
self.docker_containers.retain(|k, _| connected.contains(k.as_str()));
self.docker_stats.retain(|k, _| connected.contains(k.as_str()));
self.docker_info.retain(|k, _| connected.contains(k.as_str()));
```

**Files**: `crates/server/src/service/agent_manager.rs` (add `cleanup_docker_caches` method), `crates/server/src/task/offline_checker.rs` (call it)

### S3. AlertStateManager Key Type Optimization

**Problem**: AlertStateManager uses `(String, String)` as DashMap key (rule_id and server_id). Every `is_triggered()` / `get_info()` / `trigger()` call converts `i32` rule_id to `String` via `.to_string()`, allocating on the heap.

**Change**: Change key type from `(String, String)` to `(i32, String)`:

- `rule_id` is always a database integer — pass it as `i32` directly
- `server_id` remains `String` (UUID format)
- Update all call sites to pass `i32` for rule_id instead of `&str`

**Files**: `crates/server/src/service/alert.rs` (AlertStateManager struct + methods), callers in `crates/server/src/task/alert_evaluator.rs`, `crates/server/src/service/check_event_rules.rs`

---

## Agent Optimizations

### A1. Selective sysinfo Refresh

**Problem**: `collector/mod.rs:47` calls `self.sys.refresh_all()` every collection cycle. This refreshes everything including full process details (command line, environment, open files on some platforms), component temperatures, and disk metadata — most of which `collect()` doesn't use. The `sysinfo` crate's `refresh_all()` is documented as the heaviest refresh operation.

**Change**:

```rust
// Before:
self.sys.refresh_all();

// After:
self.sys.refresh_cpu_usage();
self.sys.refresh_memory();
self.sys.refresh_processes(sysinfo::ProcessesToUpdate::All, false);
```

- `refresh_cpu_usage()`: only CPU utilization percentages
- `refresh_memory()`: only RAM/swap counters
- `refresh_processes(All, false)`: refreshes process list with basic info only (`false` = skip task/children details)
- Network is already handled by separate `self.networks.refresh(true)`
- Temperature and GPU are collected by their own dedicated functions
- Keep `System::new_all()` + `refresh_all()` in `Collector::new()` for initial full snapshot

**Files**: `crates/agent/src/collector/mod.rs` (line 47)

### A2. prev_disk_io Stale Entry Cleanup

**Problem**: `prev_disk_io: HashMap<String, DiskCounters>` in `Collector` accumulates entries for every disk device ever seen. Removed/unmounted devices leave orphan entries that never get cleaned up.

**Change**: After computing disk I/O deltas in `disk_io::collect()`, retain only devices present in the current cycle:

```rust
let current_devices: HashSet<&str> = current_stats.keys().map(|k| k.as_str()).collect();
prev.retain(|k, _| current_devices.contains(k.as_str()));
```

**Files**: `crates/agent/src/collector/disk_io.rs`

### A3. Docker Filter Constants via LazyLock

**Problem**: `docker/containers.rs` builds a `ListContainersOptions` struct (containing a `HashMap` for filters) on every `list_containers()` call.

**Change**: Extract as a module-level `LazyLock` constant:

```rust
use std::sync::LazyLock;

static LIST_OPTS: LazyLock<ListContainersOptions<String>> = LazyLock::new(|| {
    ListContainersOptions { all: true, ..Default::default() }
});
```

Call site references `&*LIST_OPTS`.

**Files**: `crates/agent/src/docker/containers.rs`

---

## Frontend Optimizations

### F1. Explicit gcTime on QueryClient

**Problem**: `main.tsx` doesn't set `gcTime`. While the default is 5 minutes, making it explicit ensures predictable cache behavior and documents the intent. Combined with F2-F5, this ensures the full cache lifecycle is controlled.

**Change**:

```typescript
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 30_000, gcTime: 300_000, retry: 1 }
  }
})
```

**Files**: `apps/web/src/main.tsx`

### F2. Server Card Grid Virtual Scrolling

**Problem**: The servers index page renders all ServerCard components simultaneously. Each card contains Recharts mini-charts (BarChart for latency). At 50+ servers, this means 50+ chart instances mounted at once, each with its own SVG DOM subtree.

**Change**:

- Add `@tanstack/react-virtual` dependency
- In `servers/index.tsx` grid view, wrap the card list with `useVirtualizer` to only render visible cards
- Table view (DataTable) is left as-is — row rendering at 50 items is acceptable
- File browser and Docker container lists are not changed — not bottlenecks at current scale

**Files**: `apps/web/src/routes/_authed/servers/index.tsx`, `apps/web/package.json`

### F3. memo(ServerCard) + Disable Chart Animation

**Problem**:
1. `ServerCard` is not wrapped in `memo()` — every WS update to `['servers']` re-renders all cards
2. Recharts BarChart in cards uses default animation (800ms transitions) — 50+ simultaneous animations cause frame drops

**Change**:

- Wrap `ServerCard` export with `memo()`, comparing `server.id` + `server.last_active` for equality
- Add `isAnimationActive={false}` to BarChart components inside ServerCard
- Detail page charts (metrics-chart, disk-io-chart) keep animation — single-server view, good UX

```typescript
export const ServerCard = memo(ServerCardInner, (prev, next) =>
  prev.server.id === next.server.id && prev.server.last_active === next.server.last_active
)
```

**Files**: `apps/web/src/components/server/server-card.tsx`

### F4. useRealtimeMetrics Buffer — In-place Mutation

**Problem**: `use-realtime-metrics.ts:123` creates a new array on every data point: `[...bufferRef.current, point]`. At TRIM_THRESHOLD=250, this copies up to 250 object references per WS update per server.

**Change**:

```typescript
// Before:
bufferRef.current = [...bufferRef.current, point]
if (bufferRef.current.length > TRIM_THRESHOLD) {
  bufferRef.current = bufferRef.current.slice(-MAX_BUFFER_SIZE)
}

// After:
bufferRef.current.push(point)
if (bufferRef.current.length > TRIM_THRESHOLD) {
  bufferRef.current.splice(0, bufferRef.current.length - MAX_BUFFER_SIZE)
}
```

- `push()` appends in place — no array copy
- `splice()` removes from head in place — no new array allocation
- Rendering is driven by `setTick()`, not array identity, so behavior is unchanged

**Files**: `apps/web/src/hooks/use-realtime-metrics.ts`

### F5. Reduce Redundant Query Invalidation on WS Messages

**Problem**: `use-servers-ws.ts:223,235` calls `invalidateQueries({ queryKey: ['servers-list'] })` on `capabilities_changed` and `agent_info_updated` messages. This triggers a network refetch even though the data was already updated locally via `setQueryData`. The `['servers-list']` query (if it exists) fetches the same server list that was just updated.

**Change**:

- Replace `invalidateQueries` with `setQueryData` for `['servers-list']` — apply the same capability/protocol_version update locally
- If `['servers-list']` uses a different data shape, map the update accordingly

**Files**: `apps/web/src/hooks/use-servers-ws.ts`

---

## Testing Strategy

Each optimization is independently verifiable:

| Change | Verification |
|--------|-------------|
| S1 (Arc) | `cargo test -p serverbee-server` — all existing tests pass, type-check ensures correct usage |
| S2 (Docker cleanup) | Add unit test: insert Docker cache for fake server, call cleanup, verify removal |
| S3 (Alert key) | `cargo test` on alert module — existing tests cover state manager |
| A1 (sysinfo) | `cargo test -p serverbee-agent` — collector tests verify report structure unchanged |
| A2 (disk_io) | Add unit test: insert stale device, run collect, verify cleanup |
| A3 (Docker const) | Compile check — LazyLock is a drop-in replacement |
| F1 (gcTime) | Manual: open 10 server detail pages, navigate away, verify queries gc'd after 5min in React Query devtools |
| F2 (virtual) | Manual: load 50+ servers, verify smooth scrolling, measure DOM node count |
| F3 (memo) | React devtools Profiler: verify ServerCard skips re-render when server.last_active unchanged |
| F4 (buffer) | Existing `use-realtime-metrics` tests + manual: verify chart data continuity |
| F5 (invalidation) | Network tab: verify no redundant GET after capabilities_changed WS message |

## Non-Goals

- Broadcast channel redesign (single 256-capacity channel is adequate for current scale)
- Custom allocator (jemalloc/mimalloc — marginal benefit without measured fragmentation)
- Streaming file transfer (agent file manager — rare operation, not a memory hotspot)
- Frontend state management rewrite (current WS → setQueryData pattern works well)
- DataTable virtualization (50 rows is fine without it)
