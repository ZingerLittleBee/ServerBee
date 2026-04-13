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
- No new dependencies

---

## Server Optimizations

### S1. AgentManager — Arc\<SystemReport\> Shared Ownership

**Problem**: `latest_reports: DashMap<String, CachedReport>` stores `SystemReport` by value. Every call to `get_latest_report()` and `all_latest_reports()` performs a full deep clone of `SystemReport` (which contains `Vec<DiskIo>`, `Option<GpuReport>` with `Vec<GpuInfo>`). Hot callers:

- `record_writer.rs`: `all_latest_reports()` every 60s — clones N reports
- `browser.rs`: `build_full_sync()` calls `get_latest_report()` per server on every browser connect and lag resync
- `status.rs`: `get_latest_report()` for public status page rendering
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

**Files**: `crates/server/src/service/agent_manager.rs`, `crates/server/src/task/record_writer.rs`, `crates/server/src/router/ws/browser.rs`, `crates/server/src/router/api/status.rs`

### S2. Full Docker State Cleanup on Agent Disconnect

**Problem**: `remove_connection()` in AgentManager does NOT perform complete Docker cleanup. It cleans `connections`, `server_capabilities`, `agent_local_capabilities`, and `docker_log_sessions` — but NOT the three Docker cache maps (`docker_containers`, `docker_stats`, `docker_info`), not the `DockerViewerTracker`, and does not broadcast `DockerAvailabilityChanged` or update the server's `features`.

The full Docker unavailable cleanup lives in `handle_docker_unavailable()` (`router/ws/agent.rs:945`), which:
1. Calls `clear_docker_caches(server_id)` — clears the 3 cache maps
2. Calls `docker_viewers.remove_all_for_server(server_id)` — clears viewer tracking
3. Calls `remove_docker_log_sessions_for_server(server_id)` — clears log sessions
4. Removes `"docker"` from server features, persists to DB, updates cache
5. Broadcasts `DockerAvailabilityChanged { available: false }`

When the `offline_checker` detects a stale agent, it calls `check_offline()` → `remove_connection()` which skips steps 1, 2, 4, and 5. This leaks Docker caches and viewer tracker entries. When the agent reconnects (`agent.rs:437`), stale viewer tracker entries can cause it to resume Docker stats/events streaming to non-existent browser connections.

**Change**: Extract the Docker unavailable cleanup into a method on `AgentManager` (or a standalone helper that takes `&AppState`) so both `remove_connection()` and the WS handler share the same cleanup path:

```rust
// In AgentManager or as a standalone function:
pub async fn handle_agent_docker_cleanup(state: &AppState, server_id: &str) {
    state.agent_manager.clear_docker_caches(server_id);
    state.docker_viewers.remove_all_for_server(server_id);
    state.agent_manager.remove_docker_log_sessions_for_server(server_id);

    let mut features = state.agent_manager.get_features(server_id);
    if features.contains(&"docker".to_string()) {
        features.retain(|f| f != "docker");
        let _ = ServerService::update_features(&state.db, server_id, &features).await;
        state.agent_manager.update_features(server_id, features);
    }

    state.agent_manager.broadcast_browser(BrowserMessage::DockerAvailabilityChanged {
        server_id: server_id.to_string(),
        available: false,
    });
}
```

- `remove_connection()` is sync, but this cleanup requires DB access (async). Two options:
  - (a) Make `remove_connection()` async and call `handle_agent_docker_cleanup` inside it
  - (b) Keep `remove_connection()` sync; call `handle_agent_docker_cleanup` from `check_offline()` in `offline_checker.rs` after `remove_connection()` returns
- Option (b) is lower-risk since it doesn't change `remove_connection()`'s signature. The WS handler (`agent.rs`) replaces its inline `handle_docker_unavailable` with a call to the same shared helper.

**Files**: `crates/server/src/service/agent_manager.rs`, `crates/server/src/task/offline_checker.rs`, `crates/server/src/router/ws/agent.rs`

---

## Agent Optimizations

### A1. Selective sysinfo Refresh

**Problem**: `collector/mod.rs:47` calls `self.sys.refresh_all()` every collection cycle. This refreshes everything including full process details, component temperatures, and disk metadata — most of which `collect()` doesn't use.

**What `collect()` actually needs from `System`**:
- CPU usage → `refresh_cpu_usage()`
- Memory/swap counters → `refresh_memory()`
- Process count → only `sys.processes().len()` — no per-process details needed
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
self.sys.refresh_processes_specifics(
    sysinfo::ProcessesToUpdate::All,
    true,  // remove dead processes to prevent unbounded growth
    sysinfo::ProcessRefreshKind::nothing(),  // only enumerate, skip per-process details
);
```

**Why `refresh_processes_specifics` instead of `refresh_processes`**: The default `refresh_processes()` delegates to `refresh_processes_specifics()` with `ProcessRefreshKind::nothing().with_memory().with_cpu().with_disk_usage().with_exe(OnlyIfNotSet)`. Since we only call `sys.processes().len()` for process count, we don't need any per-process details. `ProcessRefreshKind::nothing()` enumerates the process list (for `.len()`) and removes dead entries (when `remove_dead_processes=true`) without reading each process's memory/cpu/disk/exe.

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

### F2. Server Card Grid — content-visibility for Off-screen Cards

**Problem**: The servers index page renders all ServerCard components simultaneously. Each card contains Recharts mini-charts (BarChart for latency). At 50+ servers, 50+ chart SVG subtrees are painted and laid out even when off-screen.

**Why not TanStack Virtual**: The current grid is responsive (`sm:grid-cols-2 lg:grid-cols-3`) with variable-height cards (the network quality section at `server-card.tsx:236` is conditionally rendered). TanStack Virtual requires a fixed scroll container, explicit size estimates, and grid lanes handling — significant complexity for a case where `memo()` (F3) already eliminates most re-render overhead. The benefit doesn't justify the risk of layout regressions.

**Change**: Use CSS `content-visibility: auto` on card elements to let the browser skip layout and painting for off-screen cards:

```css
/* In the grid card wrapper */
.server-card {
  content-visibility: auto;
  contain-intrinsic-size: auto 280px;  /* estimated card height */
}
```

- `content-visibility: auto` tells the browser to skip rendering content that's off-screen, but render it normally once it scrolls into view
- `contain-intrinsic-size` provides a height placeholder so the scrollbar doesn't jump
- Works with the existing responsive grid layout without any structural changes
- Browser support: Chrome 85+, Edge 85+, Firefox 125+ — covers all modern browsers
- Combined with F3 (memo), this means off-screen cards aren't even laid out, and on-screen cards only re-render when their data changes

**Files**: `apps/web/src/routes/_authed/servers/index.tsx` (add className to card wrapper), `apps/web/src/index.css` or component styles

### F3. memo(ServerCard) + Disable Chart Animation

**Problem**:
1. `ServerCard` is not wrapped in `memo()` — every WS update to `['servers']` re-renders all cards
2. Recharts `<Bar>` elements in cards use default animation — 50+ simultaneous animations cause frame drops

**Change**:

- Wrap `ServerCard` export with `memo()`, comparing fields that drive visual output:
```typescript
export const ServerCard = memo(ServerCardInner, (prev, next) =>
  prev.server.id === next.server.id &&
  prev.server.online === next.server.online &&
  prev.server.last_active === next.server.last_active
)
```
- `last_active` changes whenever any metric changes (cpu, mem, net, etc.), so it's a reliable proxy for "data changed"
- `online` changes independently via `server_online/offline` WS messages (doesn't update `last_active`)
- `capabilities` and `features` are NOT included because the current `ServerCard` does not render them (verified: `server-card.tsx:162-296` renders status, metrics, and network data only). If capability/feature indicators are added to the card in the future, the comparator should be updated.

- Add `isAnimationActive={false}` to the two `<Bar>` elements inside ServerCard (`server-card.tsx:261` and `:285`), NOT on `<BarChart>`. In Recharts, animation is controlled per-shape (`<Bar>`, `<Line>`, `<Area>`), not on the chart container.
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
| S2 (Docker cleanup) | Add unit test: connect agent, populate Docker caches + viewer tracker, call `remove_connection` + Docker cleanup, verify all Docker state (caches, viewers, features) is cleared. |
| A1 (sysinfo) | `cargo test -p serverbee-agent` — collector tests verify report structure unchanged. Manual: run agent for 1h, confirm process map doesn't grow unbounded. |
| F1 (gcTime) | Code review only — value matches existing default. |
| F2 (content-visibility) | Manual: load 50+ servers, verify no layout shift or scrollbar jump. Chrome DevTools Performance tab: confirm off-screen cards are not in paint records. |
| F3 (memo) | React devtools Profiler: verify ServerCard skips re-render when `last_active` unchanged. Also verify card updates on online/offline transition. |
| F4 (buffer) | Existing `use-realtime-metrics` tests + manual: verify chart data continuity on long sessions. |
| F5 (invalidation) | Network tab: verify no redundant GET `/api/servers` after `capabilities_changed` WS message. Verify capabilities settings page reflects updated values. |

## Items Removed After Review

The following items were considered but removed during spec review:

- **S3 (AlertStateManager key type)**: Removed — `alert_rule.id` is a String (UUID), not an integer. The `.to_string()` allocation cost on UUID strings is negligible compared to the DB I/O in the same code paths.
- **A2 (prev_disk_io cleanup)**: Removed — `disk_io::collect()` already does `*previous = current` (line 29), which replaces the entire HashMap each cycle. Stale device entries are naturally removed.
- **A3 (Docker filter LazyLock)**: Removed — bollard's `list_containers()` takes `Option<ListContainersOptions<T>>` by value (ownership). A `LazyLock` constant would require `.clone()` on every call, providing no benefit.
- **F2 TanStack Virtual**: Replaced with `content-visibility: auto` CSS approach. TanStack Virtual requires explicit scroll container, size estimates, and grid lane handling — too much complexity for variable-height responsive cards. `content-visibility` achieves 80% of the rendering benefit with zero structural changes.

## Non-Goals

- Broadcast channel redesign (single 256-capacity channel is adequate for current scale)
- Custom allocator (jemalloc/mimalloc — marginal benefit without measured fragmentation)
- Streaming file transfer (agent file manager — rare operation, not a memory hotspot)
- Frontend state management rewrite (current WS → setQueryData pattern works well)
- DataTable virtualization (50 rows is fine without it)
