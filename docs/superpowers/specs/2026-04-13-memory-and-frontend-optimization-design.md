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

- `update_report()` consumes `SystemReport` by value already, so `Arc::new(report)` has no extra overhead
- `get_latest_report()` returns `Option<Arc<SystemReport>>` (was `Option<SystemReport>`)
- `all_latest_reports()` returns `Vec<(String, Arc<SystemReport>)>` (was `Vec<(String, SystemReport)>`)
- Callers access fields via auto-deref (`report.cpu`, `report.mem_used`, etc.) — no business logic changes needed
- Two calls to `get_latest_report()` for the same server return `Arc` clones pointing to the same data (can verify with `Arc::ptr_eq`)

**Files**: `crates/server/src/service/agent_manager.rs`, `crates/server/src/task/record_writer.rs`, `crates/server/src/router/ws/browser.rs`

### S2. Docker Cache Cleanup on Agent Disconnect

**Problem**: `docker_containers`, `docker_stats`, `docker_info` DashMaps in AgentManager are NOT cleaned in `remove_connection()` — that method only removes from `connections`, `server_capabilities`, `agent_local_capabilities`, and `docker_log_sessions`. Docker cache cleanup relies on `handle_docker_unavailable()` being called separately in the agent WS handler. When the `offline_checker` detects a stale agent and calls `remove_connection()` directly, Docker caches for that server leak permanently.

**Change**: Add `clear_docker_caches(server_id)` call inside `remove_connection()`:

```rust
pub fn remove_connection(&self, server_id: &str) {
    self.connections.remove(server_id);
    self.server_capabilities.remove(server_id);
    self.agent_local_capabilities.remove(server_id);
    self.remove_docker_log_sessions_for_server(server_id);
    self.clear_docker_caches(server_id);  // NEW — ensures cleanup on all disconnect paths
    // ... broadcast ServerOffline
}
```

This consolidates all disconnect cleanup in one place instead of relying on the WS handler calling `clear_docker_caches` separately.

**Files**: `crates/server/src/service/agent_manager.rs` (`remove_connection` method)

---

## Agent Optimizations

### A1. Selective sysinfo Refresh

**Problem**: `collector/mod.rs:47` calls `self.sys.refresh_all()` every collection cycle. This refreshes everything including full process details (command line, environment, open files on some platforms), component temperatures, and disk metadata — most of which `collect()` doesn't use. The `sysinfo` crate's `refresh_all()` is documented as the heaviest refresh operation.

**What `collect()` actually needs from `System`**:
- CPU usage → `refresh_cpu_usage()`
- Memory/swap counters → `refresh_memory()`
- Process count → `refresh_processes()` (for `sys.processes().len()`)
- Disk used/total → **independent** — `disk.rs` uses `Disks::new_with_refreshed_list()` directly, not `System`
- Network → **independent** — uses `self.networks.refresh(true)` directly
- Temperature/GPU → **independent** — collected by dedicated functions

**Change**:

```rust
// Before:
self.sys.refresh_all();

// After:
self.sys.refresh_cpu_usage();
self.sys.refresh_memory();
self.sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
```

- `refresh_cpu_usage()`: only CPU utilization percentages
- `refresh_memory()`: only RAM/swap counters
- `refresh_processes(All, true)`: refreshes process list; `true` = remove dead processes from internal map to prevent unbounded growth on long-running agents
- Keep `System::new_all()` + `refresh_all()` in `Collector::new()` for initial full snapshot

**Files**: `crates/agent/src/collector/mod.rs` (line 47)

---

## Frontend Optimizations

### F1. Explicit gcTime on QueryClient (Code Hygiene)

**Problem**: `main.tsx` doesn't set `gcTime`. The default is 5 minutes, which is adequate. Making it explicit documents the intent and ensures the cache lifecycle is consciously controlled rather than relying on an implicit default.

**Change**:

```typescript
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 30_000, gcTime: 300_000, retry: 1 }
  }
})
```

**Note**: This is a code hygiene change, not a performance optimization. The value matches the current default.

**Files**: `apps/web/src/main.tsx`

### F2. Server Card Grid Virtual Scrolling

**Problem**: The servers index page renders all ServerCard components simultaneously. Each card contains Recharts mini-charts (BarChart for latency). At 50+ servers, this means 50+ chart instances mounted at once, each with its own SVG DOM subtree.

**Change**:

- Add `@tanstack/react-virtual` dependency
- In `servers/index.tsx` grid view, wrap the card list with `useVirtualizer` to only render visible cards
- Only enable virtualization when the server count exceeds a threshold (e.g., 20+) to avoid virtualizer overhead for small deployments
- Table view (DataTable) is left as-is — row rendering at 50 items is acceptable
- File browser and Docker container lists are not changed — not bottlenecks at current scale

**Files**: `apps/web/src/routes/_authed/servers/index.tsx`, `apps/web/package.json`

### F3. memo(ServerCard) + Disable Chart Animation

**Problem**:
1. `ServerCard` is not wrapped in `memo()` — every WS update to `['servers']` re-renders all cards
2. Recharts BarChart in cards uses default animation (800ms transitions) — 50+ simultaneous animations cause frame drops

**Change**:

- Wrap `ServerCard` export with `memo()`, comparing key fields for equality:
```typescript
export const ServerCard = memo(ServerCardInner, (prev, next) =>
  prev.server.id === next.server.id &&
  prev.server.online === next.server.online &&
  prev.server.last_active === next.server.last_active &&
  prev.server.capabilities === next.server.capabilities &&
  prev.server.effective_capabilities === next.server.effective_capabilities &&
  prev.server.features === next.server.features
)
```
- Include `online`, `capabilities`, `effective_capabilities`, and `features` in the comparator because `server_online/offline`, `capabilities_changed`, and `docker_availability_changed` WS messages update these fields without changing `last_active`, and the card displays status badges and Docker/capability indicators
- Add `isAnimationActive={false}` to BarChart components inside ServerCard
- Detail page charts (metrics-chart, disk-io-chart) keep animation — single-server view, good UX

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

**Problem**: `use-servers-ws.ts:223,235` calls `invalidateQueries({ queryKey: ['servers-list'] })` on `capabilities_changed` and `agent_info_updated` messages. This triggers a network refetch of `/api/servers` even though the data was already updated locally via `setQueryData` on `['servers']` and `['servers', server_id]`.

The `['servers-list']` query fetches `ServerInfo[]` from `/api/servers` — a REST model that includes `capabilities` and `protocol_version` fields among others. These are the exact fields being updated by the WS messages.

**Change**: Replace `invalidateQueries` with targeted `setQueryData` updates:

```typescript
// For capabilities_changed (use Record<string, unknown> since no shared ServerInfo type exists):
queryClient.setQueryData<Record<string, unknown>[]>(['servers-list'], (prev) =>
  prev?.map((s) =>
    s.id === server_id
      ? { ...s, capabilities, agent_local_capabilities, effective_capabilities }
      : s
  )
)

// For agent_info_updated:
queryClient.setQueryData<Record<string, unknown>[]>(['servers-list'], (prev) =>
  prev?.map((s) =>
    s.id === server_id ? { ...s, protocol_version } : s
  )
)
```

**Note**: The `['servers-list']` query is typed as various local interfaces (`ServerInfo`, `Server`, `ServerResponse`) in different consumers, with no shared type. Use `Record<string, unknown>[]` for the updater to avoid coupling. Alternatively, the implementer may extract a shared type as a follow-up.

This eliminates the network round-trip while keeping all query caches consistent. The pattern matches the existing `setQueryData` usage for `['servers']` in the same handlers.

**Files**: `apps/web/src/hooks/use-servers-ws.ts`

---

## Testing Strategy

Each optimization is independently verifiable:

| Change | Verification |
|--------|-------------|
| S1 (Arc) | `cargo test -p serverbee-server` — all existing tests pass; type-check ensures correct usage. Optionally add a test that verifies two `get_latest_report` calls return `Arc::ptr_eq` pointers. |
| S2 (Docker cleanup) | Add unit test: connect agent, populate Docker caches, call `remove_connection`, verify Docker caches are cleared. |
| A1 (sysinfo) | `cargo test -p serverbee-agent` — collector tests verify report structure unchanged. Manual: run agent for 1h, confirm no process entry accumulation via memory profiling. |
| F1 (gcTime) | Code review only — value matches existing default. |
| F2 (virtual) | Manual: load 50+ servers, verify smooth scrolling, measure DOM node count before/after. |
| F3 (memo) | React devtools Profiler: verify ServerCard skips re-render when `last_active` unchanged. Also verify card updates when capabilities change. |
| F4 (buffer) | Existing `use-realtime-metrics` tests + manual: verify chart data continuity on long sessions. |
| F5 (invalidation) | Network tab: verify no redundant GET `/api/servers` after `capabilities_changed` WS message. Verify capabilities settings page reflects updated values. |

## Items Removed After Review

The following items were considered but removed during spec review:

- **S3 (AlertStateManager key type)**: Removed — `alert_rule.id` is a String (UUID), not an integer. The `.to_string()` allocation cost on UUID strings is negligible compared to the DB I/O in the same code paths.
- **A2 (prev_disk_io cleanup)**: Removed — `disk_io::collect()` already does `*previous = current` (line 29), which replaces the entire HashMap each cycle. Stale device entries are naturally removed.
- **A3 (Docker filter LazyLock)**: Removed — bollard's `list_containers()` takes `Option<ListContainersOptions<T>>` by value (ownership). A `LazyLock` constant would require `.clone()` on every call, providing no benefit.

## Non-Goals

- Broadcast channel redesign (single 256-capacity channel is adequate for current scale)
- Custom allocator (jemalloc/mimalloc — marginal benefit without measured fragmentation)
- Streaming file transfer (agent file manager — rare operation, not a memory hotspot)
- Frontend state management rewrite (current WS → setQueryData pattern works well)
- DataTable virtualization (50 rows is fine without it)
