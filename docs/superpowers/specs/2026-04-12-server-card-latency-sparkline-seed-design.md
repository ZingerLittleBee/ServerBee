# Server Card Latency Sparkline: Seed + Realtime Merge

- **Date**: 2026-04-12
- **Status**: Draft (brainstorming complete, awaiting user review)
- **Scope**: `/servers` list page — `ServerCard` latency/loss sparkline
- **Related**: `2026-03-15-network-quality-monitoring-design.md`, `2026-03-16-network-probe-target-refactor-design.md`

## 1. Problem

On the `/servers` list page each `ServerCard` renders a small latency/loss trend bar at the bottom (`apps/web/src/components/server/server-card.tsx:197-217`). The current implementation is purely live: `useNetworkRealtime(server.id)` starts from empty state `{}` on every mount and only appends data from the `network-probe-update` WebSocket event. Consequences:

1. On first visit, the bar is empty and fills up one point at a time. With the default 60s probe interval, reaching the 20-point `slice(-20)` ceiling takes ~20 minutes.
2. Navigating away and back resets the state. Mobile users who switch routes lose all accumulated points.
3. The current "average" numbers displayed above each bar (`avgLatency`, `avgLoss` in `server-card.tsx:120-134`) are computed from `Object.values(networkData).flat().sort().slice(-20)` — an interleaved mix of all probe targets. In multi-target setups (e.g. CT/CU/CM + international) the weighting is unpredictable: each target's sample count depends on agent timing, drop-rate, and the order WS events arrive.

The database already persists the complete history (`network_probe_record` table, 7-day retention) — the problem is purely that the list page does not consume it.

## 2. Goals

- On mount, each `ServerCard` shows **30 historical data points** sourced from the database, so the user sees meaningful trend immediately.
- WebSocket new points append to the rolling 30-point window in real time.
- Multi-target values are folded into a single per-timestamp value using **arithmetic average across targets** (the "B" aggregation strategy chosen during brainstorming).
- The two summary numbers above each bar (`avgLatency`, `avgLoss`) re-derive from the same 30-point window — no more mixed-weight "lucky dip" statistics.
- Bucket size adapts to the configured probe `interval` (`max(60, interval)` seconds), so the 30-point window maps to a **contiguous** wall-clock window of `30 × bucket_seconds`. At 60s interval this is 30 minutes; at 120s it is 60 minutes. Gaps (agent offline) appear as null bars, not compressed-away time.
- Point count is fixed at 30 (not width-responsive) — at the 240px card width the bar width works out to ~6px + 2px gap, stable across grid/list layouts.

## 3. Non-Goals (YAGNI)

- The `/network/$serverId` detail page is not touched. It already uses a seed-from-REST + WS-merge pattern (`apps/web/src/routes/_authed/network/$serverId.tsx:288-392`) and is considered correct.
- No hover popover on the card (that was Direction 3 during brainstorming — deferred).
- No multi-line rendering (one `<Line>` per target) on the card. The card stays single-series.
- No `primary target` configuration on `network_probe_config`. Folding happens purely server-side via SQL `AVG`.
- No changes to probe mechanics, intervals, retention, or existing REST endpoints other than extending `GET /api/network-probes/overview`.
- No new caching layer. The batch SQL below completes in milliseconds on SQLite WAL and runs once per `overview` refetch (60s).
- No change to the existing `useNetworkRealtime` hook. It continues to power the detail page.
- No Rust→TS type codegen. Types are kept in sync manually.
- No snapshot tests (data-driven colors are snapshot-hostile).

## 4. Current State

### 4.1 Relevant files

- `apps/web/src/components/server/server-card.tsx` — card layout + latency/loss UI
- `apps/web/src/hooks/use-network-realtime.ts` — WS-only state, used by both card and detail page
- `apps/web/src/hooks/use-network-api.ts` — React Query hooks (`useNetworkOverview`, `useNetworkRecords`, ...)
- `apps/web/src/lib/network-types.ts` — frontend DTOs mirroring backend shapes
- `apps/web/src/components/ui/uptime-bar.tsx` — the flex-row bar renderer
- `crates/server/src/service/network_probe.rs` — `NetworkProbeService` with summary/record queries
- `crates/server/src/router/api/network_probe.rs` — REST endpoints including `/api/network-probes/overview`
- `crates/server/src/entity/network_probe_record.rs` — raw records entity (7-day retention)
- `crates/server/src/entity/network_probe_record_hourly.rs` — hourly aggregates (90-day retention, not used by this feature)

### 4.2 Observed multi-target behavior

In `server-card.tsx:120-134`:

```ts
const allResults = Object.values(networkData)
  .flat()
  .sort((a, b) => a.timestamp.localeCompare(b.timestamp))
  .slice(-20)
```

This flattens every target's recent results, sorts by timestamp, and takes the last 20. In a 3-target setup with stable upload this is "roughly 6–7 rounds × 3 targets = 18–20 mixed points", but when any target has gaps the weighting skews toward the survivor(s). The displayed `avgLatency` becomes an opaque function of timing noise rather than a clear metric.

### 4.3 Default probe config

From `crates/server/src/service/network_probe.rs:28-37`: default interval is 60 seconds, configurable range 30–600 seconds, default `packet_count = 10`.

## 5. High-Level Architecture

```
          ┌─────────────────────────────┐
          │ Agent (every interval)      │
          │ probe_once() → PingResult   │
          └──────────────┬──────────────┘
                         │ AgentMessage::PingResult
                         ▼
          ┌─────────────────────────────┐
          │ Server:                     │
          │   save_results() → INSERT   │
          │     network_probe_record    │
          │   broadcast::Sender         │
          │     NetworkProbeResult      │
          └──────┬──────────────┬───────┘
                 │ WS           │ (1) DB row persisted
                 ▼              │
  ┌──────────────────────────────┐
  │ Browser:                     │
  │   (A) GET /api/network-      │ ← mount & every 60s
  │       probes/overview        │
  │       → sparkline arrays     │
  │   (B) window event           │ ← continuous
  │       network-probe-update   │
  │                              │
  │   ServerCard state:          │
  │     SparklinePoint[]         │
  │     (length capped at 30)    │
  │     mergeSeed + append merge │
  └──────────────────────────────┘
```

Two data paths feed a single rolling 30-point array per server in the browser. The array is authoritative both when seeded from REST and when extended by WS — the merge rules in §7 guarantee consistency.

## 6. Backend Contract

### 6.1 DTO additions

`NetworkServerSummary` (currently defined in `crates/server/src/service/network_probe.rs`) gains three fields:

```rust
pub struct NetworkServerSummary {
    // existing
    pub server_id: String,
    pub server_name: String,
    pub online: bool,
    pub last_probe_at: Option<DateTime<Utc>>,
    pub anomaly_count: i64,
    pub targets: Vec<NetworkTargetSummary>,

    // new — both always present, length == SPARKLINE_LENGTH
    pub latency_sparkline: Vec<Option<f64>>,
    pub loss_sparkline: Vec<Option<f64>>,
}
```

A module-level constant `pub const SPARKLINE_LENGTH: usize = 30;` lives in `crates/server/src/service/network_probe.rs`. The frontend mirrors it in `apps/web/src/lib/sparkline.ts`.

Arrays are:
- **Always length 30.** Padded at the front with `None` when the database has fewer than 30 valid buckets for the server.
- **Ordered ascending by time** — index 0 is the oldest bucket, index 29 is the newest.

`sparkline_last_at` has been removed. It was needed for WS overlay staleness checks, which were dropped (see §8.1).

### 6.2 Service method

```rust
pub struct SparklineBundle {
    pub latency: Vec<Option<f64>>,
    pub loss: Vec<Option<f64>>,
}

impl NetworkProbeService {
    pub async fn query_sparklines(
        db: &DatabaseConnection,
        server_ids: &[String],
    ) -> Result<HashMap<String, SparklineBundle>, AppError> { ... }
}
```

Called from the existing `overview` handler (`crates/server/src/router/api/network_probe.rs`) once per request. The handler enriches each `NetworkServerSummary` with the corresponding `SparklineBundle`, falling back to all-`None` arrays when a server has no bundle in the map.

### 6.3 Bucket sizing (adaptive)

Reading `NetworkProbeSetting::interval` from `network_probe_config` (the global setting table) before running the SQL. Bucket size in seconds is:

```
bucket_seconds = max(60, setting.interval)
```

The floor of 60 ensures that the 30-bucket window covers at least 30 minutes even when users drop the interval to 30s (where each bucket would fold in ~2 rounds). When the interval is larger, the bucket widens to match so every bucket gets at least one data point under normal conditions.

### 6.4 Batch SQL

One statement handles all requested `server_ids`. A CTE groups raw records into `bucket_seconds` buckets and averages multi-target values per bucket, then a `ROW_NUMBER()` window partitions by `server_id` to keep only the most recent 30 buckets per server:

```sql
WITH agg AS (
  SELECT
    server_id,
    (CAST(strftime('%s', timestamp) AS INTEGER) / ?1) * ?1 AS bucket_ts,
    AVG(avg_latency) AS latency,
    AVG(packet_loss) AS loss,
    ROW_NUMBER() OVER (
      PARTITION BY server_id
      ORDER BY (CAST(strftime('%s', timestamp) AS INTEGER) / ?1) * ?1 DESC
    ) AS rn
  FROM network_probe_record
  WHERE server_id IN rarray(?2)
  GROUP BY server_id, bucket_ts
)
SELECT server_id, bucket_ts, latency, loss
FROM agg
WHERE rn <= 30
ORDER BY server_id, bucket_ts ASC;
```

Parameters:
- `?1` — `bucket_seconds` (i64). Used three times to keep the window function expression identical to the projection.
- `?2` — `server_ids` (carrier for sea-orm's `IN` binding; concrete binding style follows existing patterns in the same service module).

Notes:
- `AVG()` in SQLite ignores `NULL`, so partially-null `avg_latency` rows across targets are handled correctly: if all targets are null in a bucket, `latency = NULL`; otherwise the average is over non-null values.
- `packet_loss` is `f64 NOT NULL` in schema, so `AVG()` on it is always defined — but the bucket value is wrapped in `Option` anyway for symmetry with latency.
- SQLite requires 3.25+ for `ROW_NUMBER() OVER (PARTITION BY ...)`. This is standard in all supported build targets.
- The `rarray` / `IN` syntax is illustrative; the real implementation follows whatever idiom already exists in `network_probe.rs` for batch `IN` clauses (`sea_query::Expr::is_in(...)`).

After the SQL returns, Rust post-processing builds a **continuous** 30-bucket timeline per server to ensure gaps (agent offline, network outage) are visible as null bars rather than silently compressed:

1. For each `server_id`, determine `latest_bucket` = the maximum `bucket_ts` from the SQL results for that server, or `floor(now / bucket_seconds) * bucket_seconds` if no rows.
2. Generate the expected bucket sequence: `[latest_bucket - 29 * bucket_seconds, latest_bucket - 28 * bucket_seconds, ..., latest_bucket]`.
3. For each expected bucket, look up the SQL result (a `HashMap<(server_id, bucket_ts), (latency, loss)>` built from the query). Present → use values. Absent → `(None, None)`.

This guarantees the returned arrays always have **exactly 30 elements** covering a contiguous `30 × bucket_seconds` wall-clock window. A 5-minute agent outage at 60s buckets produces 5 consecutive gray bars, not a compressed timeline that hides the gap.

### 6.5 Endpoint

**No new endpoint.** `GET /api/network-probes/overview` is extended in place. Response body grows by approximately `2 × 30 × 8 bytes × N_servers ≈ 480 bytes × N`; for 50 servers that is ~24 KB uncompressed / ~5 KB gzipped. Acceptable.

`utoipa::ToSchema` picks up the new fields automatically. Swagger UI reflects them on next build.

### 6.6 Auth & permissions

Unchanged. `overview` is already reachable by both admin and member roles.

## 7. Frontend Contract

### 7.1 Types

`apps/web/src/lib/network-types.ts` — add the three new fields to `NetworkServerSummary`:

```ts
export interface NetworkServerSummary {
  anomaly_count: number
  last_probe_at: string | null
  online: boolean
  server_id: string
  server_name: string
  targets: NetworkTargetSummary[]
  // new
  latency_sparkline: (number | null)[]
  loss_sparkline: (number | null)[]
}
```

### 7.2 New shared module `apps/web/src/lib/sparkline.ts`

```ts
import type { NetworkServerSummary } from './network-types'

export const SPARKLINE_LENGTH = 30

export interface SparklinePoint {
  latency: number | null      // ms
  loss: number | null         // 0..1 (raw, not percent)
}

export function seedFromSummary(summary: NetworkServerSummary): SparklinePoint[]
export function toBarData(
  points: SparklinePoint[],
  pick: 'latency' | 'lossPercent'
): (number | null)[]
export function summaryStats(points: SparklinePoint[]): {
  avgLatency: number | null
  avgLoss: number | null
}
```

Three functions, no state, no timestamps, no WS logic.

Semantics:

- **`seedFromSummary`** zips the two parallel `Option`-arrays from the backend into 30 `SparklinePoint` objects. Front-padded empty slots have `{ latency: null, loss: null }`.

- **`toBarData`** adapts to `UptimeBar`'s expected shape. For `'lossPercent'` it maps `loss != null ? loss * 100 : null`. Null entries pass through as null.

- **`summaryStats`** computes arithmetic means ignoring null entries. If every `latency` is null, `avgLatency = null`. If every `loss` is null, `avgLoss = null`. This replaces the current `server-card.tsx:131` behavior of returning `0` for empty loss (which was ambiguous with "no loss observed").

**No WS overlay.** The sparkline updates every 60s via `useNetworkOverview`'s `refetchInterval`. This is a deliberate simplification: mixing REST seed + WS realtime on a bucketed timeline with multiple unsynchronized clocks (overview refetch, wall-clock buckets, WS event timing, agent jitter) produced escalating complexity across three review rounds. For a list card whose job is "scan 50 servers at a glance," 60s freshness is sufficient. The detail page (`/network/$serverId`) already provides per-target real-time charts for users who need live observation.

### 7.3 New hook `useServerSparkline`

Lives next to `server-card.tsx` (e.g. `apps/web/src/hooks/use-server-sparkline.ts`):

```ts
export function useServerSparkline(serverId: string): SparklinePoint[] {
  const { data: overview } = useNetworkOverview()
  return useMemo(() => {
    const summary = overview?.find(s => s.server_id === serverId)
    return summary ? seedFromSummary(summary) : []
  }, [overview, serverId])
}
```

Six lines. No state, no refs, no effects, no event listeners.

`useNetworkOverview` is already React Query–cached and shared across all `ServerCard` instances (one HTTP request per 60s, not one per card). Each card's `useMemo` extracts its server's sparkline from the shared cache — O(N) find, trivially fast.

The existing `useNetworkRealtime` is untouched — the detail page continues to use it for per-target real-time charts.

### 7.4 `ServerCard` rewrite

Replace the current `useMemo` block at `server-card.tsx:120-134` with:

```ts
const points = useServerSparkline(server.id)
const latencyData = toBarData(points, 'latency')
const lossData = toBarData(points, 'lossPercent')
const { avgLatency, avgLoss } = summaryStats(points)
```

Because `summaryStats` now returns `avgLoss: number | null` (instead of the current `number`), the helper functions in `server-card.tsx` also need their signatures widened for symmetry with `avgLatency`'s existing null path:

- `getLossColor(loss: number | null)` — handle null by returning the neutral gray (same as the latency helper).
- `getLossTextClass(loss: number | null)` — return `text-muted-foreground` on null, matching `getLatencyTextClass`.
- `formatPacketLoss(loss: number | null)` — return `'-'` on null, matching `formatLatency`.

No new functions are added; only three existing ones widen their parameter type.

The visibility condition at `server-card.tsx:198` (`latencyData.length > 0 &&`) becomes:

```ts
const hasAnyData = points.some(p => p.latency != null || p.loss != null)
// ...
{hasAnyData && (
  <div className="mt-auto border-t pt-3">
    ...
  </div>
)}
```

No other structural changes to the JSX.

### 7.5 `UptimeBar` null semantics

Current behavior (`apps/web/src/components/ui/uptime-bar.tsx:14-23`) renders `value == null` at 100% height. This is ambiguous with "maximum value". Change:

- `barHeight(null)` returns `${MIN_HEIGHT_PCT}%` (i.e. 10%) — same as "empty bucket".
- The ServerCard-side color callbacks `getLatencyColor` / `getLossColor` (currently return `#ef4444` for null → red) change to return a neutral gray:
  - Light mode: `#d1d5db`
  - Dark mode: `#4b5563`

Gray communicates "no data for this bucket" rather than "severe latency", which matters for sparse-data scenarios (sparkline front-padding, early startup, stretched buckets on large intervals). The color callbacks can branch on `value == null` first before applying the existing thresholds.

### 7.6 Files touched

Minimal footprint — six files + test updates:

| File | Change |
|---|---|
| `crates/server/src/service/network_probe.rs` | add `SPARKLINE_LENGTH` const, `SparklineBundle` struct, `query_sparklines` method with gap-fill, new fields on `NetworkServerSummary` |
| `crates/server/src/router/api/network_probe.rs` | `overview` handler calls `query_sparklines`, enriches summaries |
| `apps/web/src/lib/network-types.ts` | add three new fields to `NetworkServerSummary` interface |
| `apps/web/src/lib/sparkline.ts` | **new file** — `seedFromSummary`, `toBarData`, `summaryStats` (3 pure functions, ~40 lines) |
| `apps/web/src/components/server/server-card.tsx` | swap `useNetworkRealtime` + `useMemo(flat+sort+slice)` for inline `useMemo` over `useNetworkOverview` + `seedFromSummary` + helpers |
| `apps/web/src/components/ui/uptime-bar.tsx` | `barHeight(null)` fix (§7.5) |
| `apps/web/src/components/server/server-card.test.tsx` | migrate mocks + assertions (§9.4) |
| `apps/web/src/components/ui/uptime-bar.test.tsx` | update null height assertion (§9.4) |

Note: the 6-line hook from §7.3 can be inlined directly in `server-card.tsx` as a `useMemo`. No separate hook file needed.

## 8. Data Flow

Two clocks feed one state:

- **REST (overview)**: fires on mount and every `refetchInterval: 60_000` thereafter. Each response is a snapshot of the database as of some server-side read time.
- **WS (network-probe-update)**: fires whenever the agent's reporter flushes a batch of probe results and the server persists + broadcasts them. A batch may contain results from 1, 2, or N targets (see §8.2 for the confirmed pipeline). Typical end-to-end latency: tens to hundreds of milliseconds.

### 8.1 Why no WS overlay

Three rounds of review revealed that mixing REST seed + WS realtime on a bucketed timeline with multiple unsynchronized clocks (overview refetch interval, wall-clock bucket grid, WS event timing, agent per-target jitter) produces cascading complexity:

- WS batches don't correspond to probe rounds (per-target jitter at `crates/agent/src/network_prober.rs:146` + opportunistic reporter batching at `crates/agent/src/reporter.rs:293`)
- Overview refetch and bucket grid are not aligned → buffer spans multiple buckets
- `sparkline_last_at` is bucket-aligned but WS timestamps are raw → staleness comparison across different granularities
- Tentative buckets can overlap with the seed's last bucket → need bucket identity for position-mapped merge
- Interval changes create mixed-grid states between old-bucket base and new-bucket tentative

Each fix introduced new state; each new state opened new inconsistency windows. The incremental UX value (sub-60s freshness on a list card) did not justify the complexity.

The detail page (`/network/$serverId`) already provides per-target real-time charts with seed+WS merge — that model works there because it serves one server at a time, uses per-target multi-line Recharts (not single-series bucketed bars), and doesn't need cross-server batch queries.

## 9. Error Handling & Edge Cases

### 9.1 Enumerated cases

| Case | Behavior |
|---|---|
| Server has no probe records at all | Backend returns 30×`None`. Frontend hides the sparkline block via `hasAnyData === false`, identical to today's `latencyData.length > 0` gate. |
| Server has 1–5 recent points | Backend returns 25+ front-padded nulls and 1–5 real values. Frontend renders full-width 30-bar row with gray front, colored tail. `summaryStats` divides only by non-null count. |
| Probe round with 100% packet loss on every target | `avg_latency = null` for each target → bucket's `latency = null` (gray bar); `loss = 1.0` for each → bucket's `loss = 1.0` (full-red bar). Two bars decouple cleanly. |
| Server offline | Sparkline still shows frozen history. `StatusBadge` already signals offline state above. No extra handling. |
| REST overview request fails (network / 5xx) | React Query's error state; `useMemo` returns `[]` (or previous cached data if React Query returns stale). No crash. |
| User changes probe interval mid-session | On the next overview refetch (≤ 60s) the backend recomputes with the new bucket size and returns a fresh 30-point array. User sees a one-time "redistribution" of points. Acceptable. |
| Many incoming targets with wildly different latencies | Backend SQL `AVG` smooths them. Detail page remains the place to see per-target spread. |

### 9.2 Backend tests (new, in `crates/server/src/service/network_probe.rs` test module or adjacent)

1. **Single server, single target, 30 dense buckets**: insert 30 rows spaced `interval` apart → expect length-30 array, ascending, no nulls, ordered correctly.
2. **Single server, multi-target same bucket**: insert 3 rows with identical second-precision timestamps but different `avg_latency` → expect that bucket's value == `(a + b + c) / 3`.
3. **Single server, 5 sparse points**: only 5 rows exist → expect first 25 entries null, last 5 entries real.
4. **Batch across 3 servers**: insert distinct data for each → expect `HashMap` with 3 entries, each independently limited to 30 rows via `ROW_NUMBER()`.
5. **Adaptive bucket size**: set `NetworkProbeSetting.interval = 120` → verify SQL uses `bucket_seconds = 120` and a 240s span folds two rounds into one bucket when their aligned second is the same.
6. **Empty `server_ids`**: returns empty `HashMap` without running SQL.
7. **`avg_latency = NULL` handling**: insert a row where `avg_latency` is null → verify `SELECT AVG(avg_latency)` skips it; if every target in the bucket is null, bucket's latency is null.
8. **Gap-fill continuity**: insert data for buckets T, T+1, T+4, T+5 (skipping T+2 and T+3) → verify the returned array contains null entries at the positions corresponding to T+2 and T+3 instead of compressing the timeline.

### 9.3 Frontend tests (new, `apps/web/src/lib/sparkline.test.ts`)

1. **`seedFromSummary` shape**: length 30, nulls propagate, no timestamp field in output.
2. **`seedFromSummary` with mixed data**: `latency_sparkline = [null, null, 50, 100]` (padded to 30), `loss_sparkline = [null, null, 0.01, 0.05]` → correct zip, nulls in same positions.
3. **`summaryStats` null exclusion**: points with 10 real + 20 null latencies → `avgLatency` divides only by 10; all-null latency → `avgLatency = null`; all-null loss → `avgLoss = null`.
4. **`toBarData` mapping**: `'lossPercent'` multiplies 0..1 values by 100 and preserves null.

### 9.4 Existing test migration (breaking changes)

The following existing tests will fail after this feature lands and must be updated as part of the PR:

1. **`apps/web/src/components/server/server-card.test.tsx:19`** — mocks `useNetworkRealtime`. Change to mock `useNetworkOverview` returning a `NetworkServerSummary[]` with `latency_sparkline` / `loss_sparkline` fields. The card no longer calls `useNetworkRealtime`.
2. **`apps/web/src/components/server/server-card.test.tsx:128`** — asserts the old cross-target `flat().sort().slice(-20)` behavior. Rewrite to assert that bars render from the overview sparkline data (already folded by the backend).
3. **`apps/web/src/components/ui/uptime-bar.test.tsx:36`** — asserts `null` renders at 100% height. Change to assert `null` renders at `MIN_HEIGHT_PCT` (10%) per §7.5.

### 9.5 Manual verification checklist (add to `tests/servers/`)

1. Open `/servers?view=grid` in a fresh tab (cleared storage). Every card shows 30 sparkline bars immediately — no empty-then-fill animation.
2. Wait 60s (one overview refetch cycle). Verify sparkline updates (rightmost bar shifts).
3. Click into a server detail page and navigate back. Sparkline still shows data (React Query cache).
4. In settings, change probe interval to 120s. Within 60s the sparkline re-buckets (30 bars now cover 60 minutes).
6. For a server with 3 targets (CT/CU/CM), verify displayed `avgLatency` is close to the true average of all three, and that `loss_sparkline` reflects the averaged packet loss across targets.
7. Force one target to 100% loss; verify that bucket's loss bar reads ~0.33 (one of three targets at 1.0).
8. Remove all targets from a server; next refetch shows sparkline disappear (`hasAnyData = false`).

### 9.6 Performance & N+1

- **Sparkline query**: single batch SQL with `ROW_NUMBER() OVER (PARTITION BY server_id ...)`, estimated ~1500 rows for 50 servers × ≤30 buckets, < 5 ms on SQLite WAL. Followed by O(N×30) in-memory gap-fill — negligible.
- **Total `overview` endpoint cost**: the sparkline batch SQL is added to the existing per-server queries that `overview` already executes (notably per-server anomaly counts at `crates/server/src/service/network_probe.rs:778,917`). The endpoint is **not** single-query and was never single-query. Batching the anomaly counts is a worthwhile separate optimization but is out of scope for this feature.
- `useNetworkOverview` is React Query–deduplicated across all `ServerCard` instances — one HTTP request per 60s regardless of card count.
- No WS event listeners on the list page. Zero per-card overhead beyond the shared `useNetworkOverview` query.

## 10. Open Questions & Assumptions to Verify

1. **`network_probe_record.timestamp` column type**: assumed `DATETIME` stored in ISO-8601 or Unix-epoch format compatible with `strftime('%s', ...)`. The query above uses `strftime('%s', timestamp)`. If the column is already an integer Unix-epoch, the cast simplifies. To be confirmed in `migration/m20260315_000004_network_probe.rs`.
2. **sea-orm / sea-query access to window functions**: `ROW_NUMBER() OVER (PARTITION BY ...)` may need raw-SQL execution if the builder doesn't support it directly. Acceptable to use `Statement::from_sql_and_values` for this one query.
3. **`IN` clause parameter style**: confirmed existing patterns in `network_probe.rs` for multi-id queries to follow the same idiom.

All three are implementation-time confirmations, not design alternatives. None of them should change the shape of this spec.

## 11. Out of Scope (Deferred Ideas)

These were discussed during brainstorming and explicitly deferred:

- **WS realtime overlay** on the list card sparkline. Explored extensively during design; dropped after three review rounds revealed cascading clock-alignment complexity (WS batch granularity, overview/bucket grid misalignment, stale-seed detection across different timestamp granularities, interval-change mixed grids). See §8.1 for the full rationale. If sub-60s freshness on the list card becomes a real user request, revisit with a centralized event store rather than per-card buffers.
- **Hover/click popover** showing per-target multi-line Recharts (Komari pattern). Could layer on top of this design later without data-model changes.
- **Primary-target selection** via a new `is_primary` field on `network_probe_config`. Would replace the `AVG` aggregation with `WHERE is_primary`. Schema migration required.
- **Worst-target aggregation** (`MAX` instead of `AVG`). User chose `AVG` during brainstorming; switching is a one-line SQL change if revisited.
- **Width-responsive point count**. Discussed but dropped in favor of fixed 30 for cross-layout visual consistency.
- **Multi-series overlay on the card** (one colored bar row per target). Considered too busy for list-card density.
- **Target-count badge on the card**. Would have paired well with `MAX` aggregation to signal "this is folded"; with `AVG` it adds little.
- **Tooltip on bars**. Card is a `<Link>`; nested tooltip UX is awkward. Users click through to the detail page for values.

## 12. Implementation Notes

- The feature is a single PR, mostly additive. No migrations. No breaking API changes.
- Clippy-clean, ultracite-clean, typecheck-clean expected.
- Order of work: backend SQL + service + gap-fill → API handler → frontend types → `sparkline.ts` + tests → hook → `ServerCard` swap → existing test migration → `UptimeBar` null fix → manual checklist.
- Rollback: revert the PR. No data to clean up.
