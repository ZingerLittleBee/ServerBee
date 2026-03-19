# P10: Traffic Statistics Cycle Management Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a global traffic overview page and per-server traffic tab showing billing cycle usage, daily trends, ranking, and historical comparison — all built on the existing traffic data pipeline.

**Architecture:** No backend data pipeline changes needed (traffic_hourly/daily already accumulating). Add 3 new API endpoints + 2 frontend pages. Pure read-only feature on existing data.

**Tech Stack:** Rust (Axum, sea-orm raw SQL), React (TanStack Router/Query, shadcn/ui, Recharts)

**Spec:** `docs/superpowers/specs/2026-03-19-batch1-batch2-features-design.md` Section 2

---

## File Structure

### New Files
- `apps/web/src/routes/_authed/traffic/index.tsx` — Global traffic overview page

### Modified Files (Rust)
- `crates/server/src/service/traffic.rs` — Add `overview()`, `overview_daily()`, `cycle_history()` methods
- `crates/server/src/router/api/traffic.rs` — Add 3 new endpoints
- `crates/server/src/openapi.rs` — Register new endpoints + schemas + tag

### Modified Files (Frontend)
- `apps/web/src/routes/_authed/servers/$id.tsx` — Add "Traffic" tab (or create `$serverId/traffic.tsx`)
- `apps/web/src/components/layout/sidebar.tsx` — Add "Traffic" entry

---

### Task 1: Traffic Overview API

**Files:**
- Modify: `crates/server/src/service/traffic.rs`
- Modify: `crates/server/src/router/api/traffic.rs`

- [ ] **Step 1: Add `overview()` method to TrafficService**

Query all servers, for each compute current billing cycle usage using existing `query_cycle_traffic()` and `get_cycle_range()`. Return a `Vec<ServerTrafficOverview>`:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ServerTrafficOverview {
    pub server_id: String,
    pub name: String,
    pub cycle_in: i64,
    pub cycle_out: i64,
    pub traffic_limit: Option<i64>,
    pub billing_cycle: Option<String>,
    pub percent_used: Option<f64>,
    pub days_remaining: i64,
}
```

- [ ] **Step 2: Add `overview_daily()` method to TrafficService**

Global daily aggregation: `SELECT date, SUM(bytes_in), SUM(bytes_out) FROM traffic_daily WHERE date >= ? GROUP BY date ORDER BY date`. Return `Vec<DailyTraffic>` (type already exists).

- [ ] **Step 3: Add `cycle_history()` method to TrafficService**

For a given server, compute the last N billing cycles using `get_cycle_range()` iteratively, query `traffic_daily` for each range. Return `Vec<CycleTraffic>`:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CycleTraffic {
    pub period: String,        // "2026-02"
    pub start: String,
    pub end: String,
    pub bytes_in: i64,
    pub bytes_out: i64,
}
```

- [ ] **Step 4: Add API endpoints**

In `crates/server/src/router/api/traffic.rs`, add handlers:
- `GET /api/traffic/overview` -> `get_traffic_overview`
- `GET /api/traffic/overview/daily?days=30` -> `get_traffic_overview_daily`
- `GET /api/traffic/:server_id/cycle?history=6` -> `get_traffic_cycle`

Register in the existing `read_router()`.

- [ ] **Step 5: Register in openapi.rs**

Add paths, schemas, and a `"traffic"` tag.

- [ ] **Step 6: Write tests**

Test `overview()` returns correct structure, `overview_daily()` returns aggregated data.

- [ ] **Step 7: Run tests**

Run: `cargo test -p serverbee-server -- traffic`

- [ ] **Step 8: Commit**

```bash
git add crates/server/src/service/traffic.rs crates/server/src/router/api/traffic.rs crates/server/src/openapi.rs
git commit -m "feat(server): add traffic overview, daily trend, and cycle history API"
```

---

### Task 2: Frontend — Global Traffic Page

**Files:**
- Create: `apps/web/src/routes/_authed/traffic/index.tsx`
- Modify: `apps/web/src/components/layout/sidebar.tsx`

- [ ] **Step 1: Create the traffic overview page**

Page structure:
- **Stat cards row**: Total inbound/outbound (sum of all servers this cycle), highest-usage server name, count of servers > 80% usage
- **Server traffic ranking table**: Columns — server name, cycle in, cycle out, total, limit, usage % (Progress component from shadcn), days remaining. Sortable. Use TanStack Query `useQuery` for `/api/traffic/overview`.
- **Global trend chart**: Recharts `AreaChart` with `bytes_in` / `bytes_out` series, last 30 days. Use `useQuery` for `/api/traffic/overview/daily?days=30`. Format bytes with existing `formatBytes()` utility.

- [ ] **Step 2: Add sidebar entry**

Add "Traffic" link with `BarChart3` icon from lucide-react, as a first-level sidebar entry.

- [ ] **Step 3: Verify frontend**

Run: `cd apps/web && bun run typecheck && bun run build`

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/
git commit -m "feat(web): add global traffic overview page with ranking and trend chart"
```

---

### Task 3: Frontend — Server Detail Traffic Tab

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`

- [ ] **Step 1: Add "Traffic" tab to server detail**

Content:
- **Cycle overview card**: Progress ring (used/limit), start/end dates, inbound/outbound values. Query `/api/traffic/:server_id/cycle`.
- **Daily trend chart**: Recharts `BarChart` (stacked in/out), with time range selector (7d/30d/90d). Query existing `/api/traffic/:server_id/daily?from=&to=`.
- **Historical cycle comparison**: Horizontal bar chart, last 6 cycles. Data from `/api/traffic/:server_id/cycle?history=6`.

Only render the Traffic tab if server has `billing_cycle` configured (otherwise show a hint to configure billing in server edit dialog).

- [ ] **Step 2: Verify frontend**

Run: `cd apps/web && bun run typecheck && bun x ultracite check && bun run build`

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/servers/
git commit -m "feat(web): add traffic tab to server detail with cycle overview and trends"
```

---

### Task 4: Final Verification

- [ ] **Step 1: Run all tests**

Run: `cargo test --workspace && cd apps/web && bun run test --run`

- [ ] **Step 2: Run clippy + lint**

Run: `cargo clippy --workspace -- -D warnings && cd apps/web && bun x ultracite check`

- [ ] **Step 3: Update docs**

Update TESTING.md and PROGRESS.md with P10 completion.

- [ ] **Step 4: Commit**

```bash
git add .
git commit -m "docs: update TESTING.md and PROGRESS.md for P10 traffic statistics"
```
