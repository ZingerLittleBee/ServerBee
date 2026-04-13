# Memory & Frontend Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce memory overhead and improve frontend performance for 50+ server deployments.

**Architecture:** Seven independent changes across three layers — Server (Arc sharing, Docker cleanup), Agent (selective sysinfo refresh), Frontend (gcTime, content-visibility, memo, buffer optimization, query invalidation). Each change is independently mergeable.

**Tech Stack:** Rust (Axum, sea-orm, DashMap, sysinfo 0.33), React 19 (TanStack Query, Recharts, shadcn/ui)

**Spec:** `docs/superpowers/specs/2026-04-13-memory-and-frontend-optimization-design.md`

---

## File Map

| Change | Files Modified | Files Created |
|--------|---------------|---------------|
| S1 (Arc) | `crates/server/src/service/agent_manager.rs`, `crates/server/src/task/record_writer.rs`, `crates/server/src/router/ws/browser.rs`, `crates/server/src/router/api/status.rs` | — |
| S2 (Docker cleanup) | `crates/server/src/service/agent_manager.rs`, `crates/server/src/task/offline_checker.rs`, `crates/server/src/router/ws/agent.rs` | — |
| A1 (sysinfo) | `crates/agent/src/collector/mod.rs` | — |
| F1 (gcTime) | `apps/web/src/main.tsx` | — |
| F2 (content-visibility) | `apps/web/src/routes/_authed/servers/index.tsx` | — |
| F3 (memo + animation) | `apps/web/src/components/server/server-card.tsx` | — |
| F4 (buffer) | `apps/web/src/hooks/use-realtime-metrics.ts` | — |
| F5 (invalidation) | `apps/web/src/hooks/use-servers-ws.ts` | — |

---

## Task 1: S1 — Arc\<SystemReport\> in AgentManager

**Files:**
- Modify: `crates/server/src/service/agent_manager.rs:78-79,143-144,215-225`
- Modify: `crates/server/src/task/record_writer.rs:31,45`
- Modify: `crates/server/src/router/ws/browser.rs:265`
- Modify: `crates/server/src/router/api/status.rs:106`

- [ ] **Step 1: Update CachedReport to use Arc**

In `crates/server/src/service/agent_manager.rs`, add `use std::sync::Arc;` at the top (if not present), then change the `CachedReport` struct:

```rust
// Line 78: change
pub report: SystemReport,
// to:
pub report: Arc<SystemReport>,
```

- [ ] **Step 2: Wrap report in Arc in update_report**

In `update_report()` (line ~143), where `SystemReport` is stored into `CachedReport`:

```rust
// Find the line that creates CachedReport { report, received_at: now }
// Change `report` to `Arc::new(report)`
CachedReport { report: Arc::new(report), received_at: now }
```

- [ ] **Step 3: Update get_latest_report return type**

Change `get_latest_report` (line ~215-216):

```rust
// From:
pub fn get_latest_report(&self, server_id: &str) -> Option<SystemReport> {
    self.latest_reports.get(server_id).map(|r| r.report.clone())
}
// To:
pub fn get_latest_report(&self, server_id: &str) -> Option<Arc<SystemReport>> {
    self.latest_reports.get(server_id).map(|r| Arc::clone(&r.report))
}
```

- [ ] **Step 4: Update all_latest_reports return type**

Change `all_latest_reports` (line ~220-225):

```rust
// From:
pub fn all_latest_reports(&self) -> Vec<(String, SystemReport)> {
    self.latest_reports
        .iter()
        .map(|entry| (entry.key().clone(), entry.value().report.clone()))
        .collect()
}
// To:
pub fn all_latest_reports(&self) -> Vec<(String, Arc<SystemReport>)> {
    self.latest_reports
        .iter()
        .map(|entry| (entry.key().clone(), Arc::clone(&entry.value().report)))
        .collect()
}
```

- [ ] **Step 5: Adapt record_writer.rs caller**

In `crates/server/src/task/record_writer.rs`, the `reports` variable (line 31) now holds `Vec<(String, Arc<SystemReport>)>`. The loop at line 45 accesses `report` fields via auto-deref, so `report.net_in_transfer` etc. already work. No code changes needed in the loop body — just verify it compiles.

- [ ] **Step 6: Adapt browser.rs caller**

In `crates/server/src/router/ws/browser.rs`, line 265:

```rust
let report = state.agent_manager.get_latest_report(&server.id);
```

Now returns `Option<Arc<SystemReport>>`. The subsequent `if let Some(ref r) = report` blocks access fields via auto-deref (`r.cpu`, `r.mem_used`, etc.) — no changes needed.

- [ ] **Step 7: Adapt status.rs caller**

In `crates/server/src/router/api/status.rs`, line 106:

```rust
state.agent_manager.get_latest_report(&s.id).map(|r| StatusMetrics { ... })
```

Now `r` is `Arc<SystemReport>`. Field access via auto-deref works unchanged.

- [ ] **Step 8: Fix existing tests**

In `agent_manager.rs` tests, `test_update_report_and_cache` (line ~628) does:
```rust
let cached = mgr.get_latest_report("s1").unwrap();
```
Now returns `Arc<SystemReport>`. Field access `cached.cpu` works via auto-deref. No test changes needed.

- [ ] **Step 9: Run tests**

Run: `cargo test -p serverbee-server`
Expected: All tests pass.

- [ ] **Step 10: Commit**

```bash
git add crates/server/src/service/agent_manager.rs crates/server/src/task/record_writer.rs crates/server/src/router/ws/browser.rs crates/server/src/router/api/status.rs
git commit -m "perf(server): use Arc<SystemReport> to eliminate clone overhead in AgentManager"
```

---

## Task 2: S2 — Full Docker State Cleanup on Agent Disconnect

**Files:**
- Modify: `crates/server/src/service/agent_manager.rs:131-140`
- Modify: `crates/server/src/task/offline_checker.rs:15-18`
- Modify: `crates/server/src/router/ws/agent.rs:945-965`

- [ ] **Step 1: Add docker cleanup to remove_connection**

In `crates/server/src/service/agent_manager.rs`, modify `remove_connection` (lines 131-140):

```rust
pub fn remove_connection(&self, server_id: &str) {
    self.connections.remove(server_id);
    self.server_capabilities.remove(server_id);
    self.agent_local_capabilities.remove(server_id);
    self.remove_docker_log_sessions_for_server(server_id);
    self.clear_docker_caches(server_id);  // ADD THIS LINE

    let _ = self.browser_tx.send(BrowserMessage::ServerOffline {
        server_id: server_id.to_string(),
    });
}
```

- [ ] **Step 2: Add async docker cleanup helper for offline path**

Add a new public async function in the same file (or as a free function in a shared module). Since `remove_connection` is sync and the Docker features/viewers cleanup needs `&AppState` (for DB and docker_viewers), add the async part to `offline_checker`:

In `crates/server/src/task/offline_checker.rs`, expand the offline loop:

```rust
use std::sync::Arc;
use std::time::Duration;

use crate::state::AppState;
use serverbee_common::protocol::BrowserMessage;

pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        interval.tick().await;

        let offline_ids = state.agent_manager.check_offline(30);
        for id in &offline_ids {
            tracing::info!("Agent {id} marked offline (no report for 30s)");
            // Full Docker cleanup for the offline agent
            cleanup_docker_for_server(&state, id).await;
        }

        state.agent_manager.cleanup_expired_requests();
        state.agent_manager.cleanup_traceroute_results();
    }
}

/// Clean up remaining Docker state for a server that went offline.
/// Complements `remove_connection()` which already handles:
///   - clear_docker_caches (containers, stats, info)
///   - remove_docker_log_sessions_for_server
/// This function handles the rest that `remove_connection` doesn't cover.
async fn cleanup_docker_for_server(state: &AppState, server_id: &str) {
    state.docker_viewers.remove_all_for_server(server_id);

    let mut features = state.agent_manager.get_features(server_id);
    features.retain(|feature| feature != "docker");
    let _ = crate::service::server::ServerService::update_features(
        &state.db, server_id, &features,
    )
    .await;
    state.agent_manager.update_features(server_id, features);

    state
        .agent_manager
        .broadcast_browser(BrowserMessage::DockerAvailabilityChanged {
            server_id: server_id.to_string(),
            available: false,
        });
}
```

- [ ] **Step 3: Refactor handle_docker_unavailable in agent.rs**

In `crates/server/src/router/ws/agent.rs`, simplify `handle_docker_unavailable` (lines 945-965) to reuse the same pattern. Since `remove_connection` already calls `clear_docker_caches`, and the WS handler calls `handle_docker_unavailable` for Docker-becomes-unavailable (not full disconnect), keep it as-is — it's a different code path (agent is still connected, just Docker became unavailable). No change needed here.

**Rationale**: `handle_docker_unavailable` runs when a connected agent reports Docker is no longer available. `remove_connection` + `cleanup_docker_for_server` runs when the agent disconnects entirely. Both paths now do full cleanup, but they're triggered differently. Don't merge them — they have different callers and contexts.

- [ ] **Step 4: Add test for Docker cache cleanup on disconnect**

In `agent_manager.rs` tests, add:

```rust
#[test]
fn test_remove_connection_clears_docker_caches() {
    let (mgr, _rx) = make_manager();
    let (tx, _) = mpsc::channel(1);
    mgr.add_connection("s1".into(), "Srv".into(), tx, test_addr());

    // Populate Docker caches
    mgr.update_docker_containers("s1", vec![]);
    mgr.update_docker_stats("s1", vec![]);

    // Disconnect
    mgr.remove_connection("s1");

    // Docker caches should be cleared
    assert!(mgr.get_docker_containers("s1").is_none());
    assert!(mgr.get_docker_stats("s1").is_none());
}
```

Note: If `update_docker_containers`/`get_docker_containers` methods don't exist with these exact names, use the actual method names from the codebase (likely direct DashMap insert/get via existing public methods).

- [ ] **Step 5: Run tests**

Run: `cargo test -p serverbee-server`
Expected: All tests pass including the new one.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/service/agent_manager.rs crates/server/src/task/offline_checker.rs
git commit -m "fix(server): ensure full Docker state cleanup on agent disconnect"
```

---

## Task 3: A1 — Selective sysinfo Refresh in Agent Collector

**Files:**
- Modify: `crates/agent/src/collector/mod.rs:47`

- [ ] **Step 1: Replace refresh_all with selective refresh**

In `crates/agent/src/collector/mod.rs`, change line 47:

```rust
// From:
self.sys.refresh_all();

// To:
self.sys.refresh_cpu_usage();
self.sys.refresh_memory();
self.sys.refresh_processes_specifics(
    sysinfo::ProcessesToUpdate::All,
    true,
    sysinfo::ProcessRefreshKind::nothing(),
);
```

Add `use sysinfo::ProcessRefreshKind;` to the imports if needed (or use fully-qualified path as shown).

- [ ] **Step 2: Run existing collector tests**

Run: `cargo test -p serverbee-agent -- collector`
Expected: All tests pass — `test_collect_returns_valid_report`, `test_cpu_usage_range`, `test_disk_used_le_total`, `test_memory_used_le_total`, etc.

- [ ] **Step 3: Verify system_info still works**

Run: `cargo test -p serverbee-agent -- test_system_info_populated`
Expected: PASS. `system_info()` uses `&self.sys` which was fully initialized in `new()`.

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/collector/mod.rs
git commit -m "perf(agent): use selective sysinfo refresh instead of refresh_all"
```

---

## Task 4: F1 — Explicit gcTime on QueryClient

**Files:**
- Modify: `apps/web/src/main.tsx:11`

- [ ] **Step 1: Add gcTime to QueryClient config**

In `apps/web/src/main.tsx`, change line 11:

```typescript
// From:
    queries: { staleTime: 30_000, retry: 1 }
// To:
    queries: { staleTime: 30_000, gcTime: 300_000, retry: 1 }
```

- [ ] **Step 2: Run frontend tests**

Run: `cd apps/web && bun run test`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/main.tsx
git commit -m "chore(web): add explicit gcTime to QueryClient defaults"
```

---

## Task 5: F2 — content-visibility for Off-screen Server Cards

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/index.tsx:364`

- [ ] **Step 1: Add content-visibility style to grid card wrapper**

In `apps/web/src/routes/_authed/servers/index.tsx`, change the grid section (lines 363-368):

```typescript
// From:
{servers.length > 0 && viewMode === 'grid' && (
  <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
    {filtered.map((server) => (
      <ServerCard key={server.id} server={server} />
    ))}
  </div>
)}

// To:
{servers.length > 0 && viewMode === 'grid' && (
  <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
    {filtered.map((server) => (
      <div className="[contain-intrinsic-size:auto_280px] [content-visibility:auto]" key={server.id}>
        <ServerCard server={server} />
      </div>
    ))}
  </div>
)}
```

Uses Tailwind arbitrary value syntax for the CSS properties. No new CSS file needed.

- [ ] **Step 2: Run frontend lint**

Run: `cd apps/web && bun x ultracite check`
Expected: No new errors.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/servers/index.tsx
git commit -m "perf(web): add content-visibility:auto to server card grid"
```

---

## Task 6: F3 — memo(ServerCard) + Disable Bar Animation

**Files:**
- Modify: `apps/web/src/components/server/server-card.tsx:1-2,162,261,285`

- [ ] **Step 1: Add memo import**

In `apps/web/src/components/server/server-card.tsx`, add `memo` to the React import:

```typescript
// Line 2: change
import { type ComponentProps, useMemo } from 'react'
// to:
import { type ComponentProps, memo, useMemo } from 'react'
```

- [ ] **Step 2: Rename and wrap with memo**

Change the function declaration (line 162):

```typescript
// From:
export function ServerCard({ server }: ServerCardProps) {

// To:
const ServerCardInner = ({ server }: ServerCardProps) => {
```

Then at the end of the file (after the closing `}` of ServerCardInner), add:

```typescript
export const ServerCard = memo(ServerCardInner, (prev, next) => {
  const a = prev.server
  const b = next.server
  return (
    a.id === b.id &&
    a.online === b.online &&
    a.last_active === b.last_active &&
    a.name === b.name &&
    a.country_code === b.country_code &&
    a.os === b.os &&
    a.mem_total === b.mem_total &&
    a.disk_total === b.disk_total &&
    a.swap_total === b.swap_total
  )
})
```

- [ ] **Step 3: Disable animation on both Bar elements**

At line ~261 (first `<Bar>`):

```typescript
// From:
<Bar dataKey="value" radius={2}>
// To:
<Bar dataKey="value" isAnimationActive={false} radius={2}>
```

At line ~285 (second `<Bar>`):

```typescript
// From:
<Bar dataKey={() => 1} radius={1}>
// To:
<Bar dataKey={() => 1} isAnimationActive={false} radius={1}>
```

- [ ] **Step 4: Run frontend tests**

Run: `cd apps/web && bun run test`
Expected: All tests pass. If `server-card.test.tsx` imports `ServerCard` by name, the named export still works.

- [ ] **Step 5: Run lint**

Run: `cd apps/web && bun x ultracite check`
Expected: No errors.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/server/server-card.tsx
git commit -m "perf(web): memo ServerCard and disable Bar animation"
```

---

## Task 7: F4 — In-place Buffer Mutation in useRealtimeMetrics

**Files:**
- Modify: `apps/web/src/hooks/use-realtime-metrics.ts:123-127`

- [ ] **Step 1: Replace spread with push/splice**

In `apps/web/src/hooks/use-realtime-metrics.ts`, change lines 123-127:

```typescript
// From:
      bufferRef.current = [...bufferRef.current, point]

      if (bufferRef.current.length > TRIM_THRESHOLD) {
        bufferRef.current = bufferRef.current.slice(-MAX_BUFFER_SIZE)
      }

// To:
      bufferRef.current.push(point)

      if (bufferRef.current.length > TRIM_THRESHOLD) {
        bufferRef.current.splice(0, bufferRef.current.length - MAX_BUFFER_SIZE)
      }
```

- [ ] **Step 2: Run frontend tests**

Run: `cd apps/web && bun run test`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/hooks/use-realtime-metrics.ts
git commit -m "perf(web): use in-place buffer mutation in useRealtimeMetrics"
```

---

## Task 8: F5 — Replace invalidateQueries with setQueryData

**Files:**
- Modify: `apps/web/src/hooks/use-servers-ws.ts:223,235`

- [ ] **Step 1: Replace invalidateQueries for capabilities_changed**

In `apps/web/src/hooks/use-servers-ws.ts`, change line 223:

```typescript
// From:
    queryClient.invalidateQueries({ queryKey: ['servers-list'] })

// To:
    queryClient.setQueryData<Record<string, unknown>[]>(['servers-list'], (prev) =>
      prev?.map((s) =>
        s.id === server_id
          ? { ...s, capabilities, agent_local_capabilities: agent_local_capabilities ?? null, effective_capabilities: effective_capabilities ?? null }
          : s
      )
    )
```

- [ ] **Step 2: Replace invalidateQueries for agent_info_updated**

Change line 235:

```typescript
// From:
    queryClient.invalidateQueries({ queryKey: ['servers-list'] })

// To:
    queryClient.setQueryData<Record<string, unknown>[]>(['servers-list'], (prev) =>
      prev?.map((s) => (s.id === server_id ? { ...s, protocol_version } : s))
    )
```

- [ ] **Step 3: Run frontend tests**

Run: `cd apps/web && bun run test`
Expected: All tests pass.

- [ ] **Step 4: Run lint**

Run: `cd apps/web && bun x ultracite check`
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/hooks/use-servers-ws.ts
git commit -m "perf(web): replace invalidateQueries with setQueryData for servers-list"
```

---

## Final Verification

- [ ] **Step 1: Full Rust test suite**

Run: `cargo test --workspace`
Expected: All tests pass.

- [ ] **Step 2: Full frontend test suite**

Run: `cd apps/web && bun run test`
Expected: All tests pass.

- [ ] **Step 3: Frontend lint + typecheck**

Run: `cd apps/web && bun x ultracite check && bun run typecheck`
Expected: No errors.

- [ ] **Step 4: Build check**

Run: `cargo build --workspace && cd apps/web && bun run build`
Expected: Clean build with no warnings.
