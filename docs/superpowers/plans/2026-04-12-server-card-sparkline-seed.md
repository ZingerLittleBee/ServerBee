# Server Card Sparkline Seed Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the `/servers` list-page sparkline bars render 30 points of historical data immediately on mount instead of filling from zero via WebSocket.

**Architecture:** Backend extends `GET /api/network-probes/overview` response with two 30-element sparkline arrays per server, computed via a batch SQL with window functions + Rust gap-fill post-processing. Frontend replaces the WS-only `useNetworkRealtime` hook with a pure `useMemo` over the already-cached overview data. No WS overlay, no new endpoints, no migrations.

**Tech Stack:** Rust (sea-orm raw SQL, `FromQueryResult`), React (TanStack Query `useNetworkOverview`, `useMemo`), Vitest

**Spec:** `docs/superpowers/specs/2026-04-12-server-card-latency-sparkline-seed-design.md`

**Important naming note:** The backend Rust struct is `ServerOverview` (at `crates/server/src/service/network_probe.rs:92`). The frontend TS interface is `NetworkServerSummary` (at `apps/web/src/lib/network-types.ts:44`). They serialize to the same JSON shape. This plan uses the correct name for each language.

---

### Task 1: Backend — SparklineBundle struct + query_sparklines method

**Files:**
- Modify: `crates/server/src/service/network_probe.rs`

This task adds the core SQL query and gap-fill post-processing. The method is tested in Task 2 and wired into the overview handler in Task 3.

- [ ] **Step 1: Add the SPARKLINE_LENGTH constant and SparklineBundle struct**

Add near the top of the file (after the existing struct definitions, around line 100):

```rust
pub const SPARKLINE_LENGTH: usize = 30;

#[derive(Debug, Clone)]
pub struct SparklineBundle {
    pub latency: Vec<Option<f64>>,
    pub loss: Vec<Option<f64>>,
}
```

- [ ] **Step 2: Add the raw SQL result row type**

Add near the other `FromQueryResult` structs (after `LatestRecordRow` around line 130):

```rust
#[derive(Debug, FromQueryResult)]
struct SparklineRow {
    pub server_id: String,
    pub bucket_ts: i64,
    pub latency: Option<f64>,
    pub loss: Option<f64>,
}
```

- [ ] **Step 3: Implement query_sparklines method**

Add this method to the `impl NetworkProbeService` block. Place it after the existing `get_overview` method (after line ~791):

```rust
/// Batch-query sparkline data for all given server_ids.
/// Returns a HashMap of server_id → SparklineBundle (30 points, gap-filled).
pub async fn query_sparklines(
    db: &DatabaseConnection,
    server_ids: &[String],
    bucket_seconds: i64,
) -> Result<HashMap<String, SparklineBundle>, AppError> {
    if server_ids.is_empty() {
        return Ok(HashMap::new());
    }

    // Time bound: only scan records within the sparkline window + 1 bucket margin
    let window_seconds = (SPARKLINE_LENGTH as i64 + 1) * bucket_seconds;
    let cutoff = Utc::now() - Duration::seconds(window_seconds);
    let cutoff_str = cutoff.to_rfc3339();

    // Build parameterized IN clause: (?, ?, ...)
    let placeholders: Vec<&str> = server_ids.iter().map(|_| "?").collect();
    let in_clause = placeholders.join(", ");

    let sql = format!(
        "WITH agg AS ( \
            SELECT \
                server_id, \
                (CAST(strftime('%s', timestamp) AS INTEGER) / ?1) * ?1 AS bucket_ts, \
                AVG(avg_latency) AS latency, \
                AVG(packet_loss) AS loss, \
                ROW_NUMBER() OVER ( \
                    PARTITION BY server_id \
                    ORDER BY (CAST(strftime('%s', timestamp) AS INTEGER) / ?1) * ?1 DESC \
                ) AS rn \
            FROM network_probe_record \
            WHERE server_id IN ({in_clause}) AND timestamp >= ?2 \
            GROUP BY server_id, bucket_ts \
        ) \
        SELECT server_id, bucket_ts, latency, loss \
        FROM agg \
        WHERE rn <= {limit} \
        ORDER BY server_id, bucket_ts ASC",
        in_clause = in_clause,
        limit = SPARKLINE_LENGTH,
    );

    // Build parameter values: ?1 = bucket_seconds, ?2 = cutoff, then server_ids
    let mut params: Vec<Value> = Vec::with_capacity(2 + server_ids.len());
    params.push(Value::BigInt(Some(bucket_seconds)));
    params.push(Value::from(cutoff_str));
    for sid in server_ids {
        params.push(Value::from(sid.clone()));
    }

    let stmt = Statement::from_sql_and_values(DatabaseBackend::Sqlite, &sql, params);
    let rows = SparklineRow::find_by_statement(stmt).all(db).await?;

    // Index rows by (server_id, bucket_ts) for O(1) lookup during gap-fill
    let mut row_map: HashMap<(String, i64), (Option<f64>, Option<f64>)> = HashMap::new();
    let mut latest_by_server: HashMap<String, i64> = HashMap::new();
    for row in &rows {
        row_map.insert(
            (row.server_id.clone(), row.bucket_ts),
            (row.latency, row.loss),
        );
        let entry = latest_by_server
            .entry(row.server_id.clone())
            .or_insert(row.bucket_ts);
        if row.bucket_ts > *entry {
            *entry = row.bucket_ts;
        }
    }

    // Gap-fill: build continuous 30-bucket sequence per server
    let now_ts = Utc::now().timestamp();
    let now_bucket = (now_ts / bucket_seconds) * bucket_seconds;

    let mut result: HashMap<String, SparklineBundle> = HashMap::new();
    for sid in server_ids {
        let latest_bucket = latest_by_server
            .get(sid)
            .copied()
            .unwrap_or(now_bucket);

        let mut latency_vec = Vec::with_capacity(SPARKLINE_LENGTH);
        let mut loss_vec = Vec::with_capacity(SPARKLINE_LENGTH);

        for i in 0..SPARKLINE_LENGTH {
            let bucket = latest_bucket
                - ((SPARKLINE_LENGTH - 1 - i) as i64) * bucket_seconds;
            if let Some((lat, loss)) = row_map.get(&(sid.clone(), bucket)) {
                latency_vec.push(*lat);
                loss_vec.push(*loss);
            } else {
                latency_vec.push(None);
                loss_vec.push(None);
            }
        }

        result.insert(
            sid.clone(),
            SparklineBundle {
                latency: latency_vec,
                loss: loss_vec,
            },
        );
    }

    Ok(result)
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p serverbee-server`
Expected: compiles with no errors (may have unused warnings — fine for now)

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/network_probe.rs
git commit -m "feat(server): add query_sparklines batch SQL with gap-fill"
```

---

### Task 2: Backend — Tests for query_sparklines

**Files:**
- Modify: `crates/server/src/service/network_probe.rs` (test module at line ~1062)

- [ ] **Step 1: Write tests for query_sparklines**

Add to the existing `#[cfg(test)] mod tests` block. These tests need a real SQLite database. Follow the pattern of existing tests in the module that use `crate::test_utils::setup_test_db` (check if that exists, or set up an in-memory DB using the migration runner).

First, check what test infrastructure exists:

```bash
cargo test -p serverbee-server -- --list 2>&1 | grep network_probe | head -20
```

Then add the tests. The tests insert rows into `network_probe_record` and call `query_sparklines`. Here's the test code to add:

```rust
#[tokio::test]
async fn test_sparkline_empty_server_ids() {
    let db = setup_test_db().await;
    let result = NetworkProbeService::query_sparklines(&db, &[], 60).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_sparkline_single_server_dense() {
    let db = setup_test_db().await;
    let sid = "srv-1";
    let now = Utc::now();
    let bucket_seconds = 60i64;

    // Insert 30 records, one per bucket
    for i in 0..30 {
        let ts = now - Duration::seconds((29 - i) * bucket_seconds);
        insert_probe_record(&db, sid, "tgt-1", Some(10.0 + i as f64), 0.01, ts).await;
    }

    let result = NetworkProbeService::query_sparklines(&db, &[sid.to_string()], bucket_seconds)
        .await
        .unwrap();
    let bundle = result.get(sid).expect("should have server");
    assert_eq!(bundle.latency.len(), SPARKLINE_LENGTH);
    assert_eq!(bundle.loss.len(), SPARKLINE_LENGTH);
    // All 30 should be non-null
    assert!(bundle.latency.iter().all(|v| v.is_some()));
    assert!(bundle.loss.iter().all(|v| v.is_some()));
}

#[tokio::test]
async fn test_sparkline_multi_target_avg() {
    let db = setup_test_db().await;
    let sid = "srv-1";
    let now = Utc::now();
    let bucket_seconds = 60i64;
    // Align timestamp to bucket boundary
    let ts = now - Duration::seconds(now.timestamp() % bucket_seconds);

    // 3 targets in the same bucket
    insert_probe_record(&db, sid, "tgt-1", Some(100.0), 0.0, ts).await;
    insert_probe_record(&db, sid, "tgt-2", Some(200.0), 0.1, ts).await;
    insert_probe_record(&db, sid, "tgt-3", Some(300.0), 0.2, ts).await;

    let result = NetworkProbeService::query_sparklines(&db, &[sid.to_string()], bucket_seconds)
        .await
        .unwrap();
    let bundle = result.get(sid).unwrap();
    // Last element should be the averaged bucket
    let last_lat = bundle.latency.last().unwrap().unwrap();
    let last_loss = bundle.loss.last().unwrap().unwrap();
    assert!((last_lat - 200.0).abs() < 0.01);
    assert!((last_loss - 0.1).abs() < 0.01);
}

#[tokio::test]
async fn test_sparkline_sparse_data() {
    let db = setup_test_db().await;
    let sid = "srv-1";
    let now = Utc::now();
    let bucket_seconds = 60i64;

    // Only 5 records in recent buckets
    for i in 0..5 {
        let ts = now - Duration::seconds((4 - i) * bucket_seconds);
        insert_probe_record(&db, sid, "tgt-1", Some(50.0), 0.02, ts).await;
    }

    let result = NetworkProbeService::query_sparklines(&db, &[sid.to_string()], bucket_seconds)
        .await
        .unwrap();
    let bundle = result.get(sid).unwrap();
    assert_eq!(bundle.latency.len(), 30);
    // First 25 should be null (gap-filled)
    let null_count = bundle.latency.iter().filter(|v| v.is_none()).count();
    assert!(null_count >= 25);
    // Last 5 should be non-null
    let real_count = bundle.latency.iter().filter(|v| v.is_some()).count();
    assert_eq!(real_count, 5);
}

#[tokio::test]
async fn test_sparkline_gap_fill_continuity() {
    let db = setup_test_db().await;
    let sid = "srv-1";
    let now = Utc::now();
    let bucket_seconds = 60i64;
    let base = now - Duration::seconds(now.timestamp() % bucket_seconds);

    // Insert data for buckets 0,1 and 4,5 — skip 2,3
    for i in [0i64, 1, 4, 5] {
        let ts = base - Duration::seconds((5 - i) * bucket_seconds);
        insert_probe_record(&db, sid, "tgt-1", Some(10.0 * i as f64), 0.0, ts).await;
    }

    let result = NetworkProbeService::query_sparklines(&db, &[sid.to_string()], bucket_seconds)
        .await
        .unwrap();
    let bundle = result.get(sid).unwrap();
    // The last 6 positions should be: [data, data, null, null, data, data]
    let tail: Vec<bool> = bundle.latency.iter().rev().take(6).rev().map(|v| v.is_some()).collect();
    assert_eq!(tail, vec![true, true, false, false, true, true]);
}

#[tokio::test]
async fn test_sparkline_null_latency() {
    let db = setup_test_db().await;
    let sid = "srv-1";
    let now = Utc::now();
    // Insert one record with null avg_latency (100% packet loss)
    insert_probe_record(&db, sid, "tgt-1", None, 1.0, now).await;

    let result = NetworkProbeService::query_sparklines(&db, &[sid.to_string()], 60)
        .await
        .unwrap();
    let bundle = result.get(sid).unwrap();
    // Last position: latency = None, loss = Some(1.0)
    assert!(bundle.latency.last().unwrap().is_none());
    assert!(bundle.loss.last().unwrap().is_some());
    let loss = bundle.loss.last().unwrap().unwrap();
    assert!((loss - 1.0).abs() < 0.01);
}

#[tokio::test]
async fn test_sparkline_batch_multiple_servers() {
    let db = setup_test_db().await;
    let now = Utc::now();
    let bucket_seconds = 60i64;

    for sid in ["srv-1", "srv-2", "srv-3"] {
        insert_probe_record(&db, sid, "tgt-1", Some(42.0), 0.0, now).await;
    }

    let ids: Vec<String> = vec!["srv-1", "srv-2", "srv-3"]
        .into_iter()
        .map(String::from)
        .collect();
    let result = NetworkProbeService::query_sparklines(&db, &ids, bucket_seconds)
        .await
        .unwrap();
    assert_eq!(result.len(), 3);
    for sid in &ids {
        let bundle = result.get(sid).unwrap();
        assert_eq!(bundle.latency.len(), 30);
        assert_eq!(bundle.loss.len(), 30);
    }
}

#[tokio::test]
async fn test_sparkline_adaptive_bucket_size() {
    let db = setup_test_db().await;
    let sid = "srv-1";
    let now = Utc::now();
    let bucket_seconds = 120i64;
    let base = now - Duration::seconds(now.timestamp() % bucket_seconds);

    // Two records 60s apart should fall in the SAME 120s bucket
    insert_probe_record(&db, sid, "tgt-1", Some(100.0), 0.0, base).await;
    insert_probe_record(&db, sid, "tgt-1", Some(200.0), 0.1, base + Duration::seconds(60)).await;

    let result = NetworkProbeService::query_sparklines(&db, &[sid.to_string()], bucket_seconds)
        .await
        .unwrap();
    let bundle = result.get(sid).unwrap();
    // Both should be averaged into one bucket
    let real_values: Vec<f64> = bundle.latency.iter().filter_map(|v| *v).collect();
    assert_eq!(real_values.len(), 1);
    assert!((real_values[0] - 150.0).abs() < 0.01);
}
```

Note: you'll need a helper function `insert_probe_record`. Check if one exists in the test module; if not, add:

```rust
async fn insert_probe_record(
    db: &DatabaseConnection,
    server_id: &str,
    target_id: &str,
    avg_latency: Option<f64>,
    packet_loss: f64,
    timestamp: DateTime<Utc>,
) {
    use crate::entity::network_probe_record;
    network_probe_record::ActiveModel {
        id: NotSet,
        server_id: Set(server_id.to_string()),
        target_id: Set(target_id.to_string()),
        avg_latency: Set(avg_latency),
        min_latency: Set(avg_latency),
        max_latency: Set(avg_latency),
        packet_loss: Set(packet_loss),
        packet_sent: Set(10),
        packet_received: Set(if packet_loss >= 1.0 { 0 } else { 10 }),
        timestamp: Set(timestamp),
    }
    .insert(db)
    .await
    .unwrap();
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test -p serverbee-server test_sparkline -- --nocapture`
Expected: all 8 tests pass. If setup_test_db doesn't exist, adapt to use whatever test DB pattern the file already has.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/service/network_probe.rs
git commit -m "test(server): add sparkline query tests (8 cases)"
```

---

### Task 3: Backend — Enrich overview handler with sparkline data

**Files:**
- Modify: `crates/server/src/service/network_probe.rs` (struct `ServerOverview` at line ~92, method `get_overview` at line ~670)

- [ ] **Step 1: Add sparkline fields to ServerOverview struct**

At `crates/server/src/service/network_probe.rs:92`, add two fields to the existing `ServerOverview` struct:

```rust
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ServerOverview {
    pub server_id: String,
    pub server_name: String,
    pub online: bool,
    pub last_probe_at: Option<String>,
    pub targets: Vec<TargetSummary>,
    pub anomaly_count: i64,
    // new
    pub latency_sparkline: Vec<Option<f64>>,
    pub loss_sparkline: Vec<Option<f64>>,
}
```

- [ ] **Step 2: Call query_sparklines inside get_overview**

In the `get_overview` method (~line 670), after the line that collects `server_ids`, add the sparkline query. Then when building each `ServerOverview`, populate the new fields from the sparkline map.

Find the section where `ServerOverview` instances are constructed (near the end of `get_overview`). Add before the loop:

```rust
// Fetch sparkline data
let setting = Self::get_setting(db).await.unwrap_or_default();
let bucket_seconds = std::cmp::max(60, setting.interval as i64);
let sparklines = Self::query_sparklines(db, &server_ids, bucket_seconds)
    .await
    .unwrap_or_default();
```

Then in each `ServerOverview` construction, add:

```rust
let sparkline = sparklines.get(&server_id).cloned().unwrap_or_else(|| {
    SparklineBundle {
        latency: vec![None; SPARKLINE_LENGTH],
        loss: vec![None; SPARKLINE_LENGTH],
    }
});
// ... in the struct literal:
latency_sparkline: sparkline.latency,
loss_sparkline: sparkline.loss,
```

- [ ] **Step 3: Verify compilation and existing tests still pass**

Run: `cargo test -p serverbee-server -- --nocapture 2>&1 | tail -5`
Expected: all tests pass (including the new sparkline tests from Task 2)

- [ ] **Step 4: Verify the endpoint manually (optional, if server is running)**

Run: `curl -s http://localhost:9527/api/network-probes/overview -H "X-API-Key: ..." | jq '.[0] | {latency_sparkline, loss_sparkline}'`
Expected: two 30-element arrays with numbers or nulls

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/network_probe.rs
git commit -m "feat(server): enrich overview response with sparkline arrays"
```

---

### Task 4: Frontend — sparkline.ts module + types + tests

**Files:**
- Modify: `apps/web/src/lib/network-types.ts`
- Create: `apps/web/src/lib/sparkline.ts`
- Create: `apps/web/src/lib/sparkline.test.ts`

- [ ] **Step 1: Add sparkline fields to NetworkServerSummary**

In `apps/web/src/lib/network-types.ts`, add two fields to `NetworkServerSummary` (after line 51):

```ts
export interface NetworkServerSummary {
  anomaly_count: number
  last_probe_at: string | null
  online: boolean
  server_id: string
  server_name: string
  targets: NetworkTargetSummary[]
  latency_sparkline: (number | null)[]
  loss_sparkline: (number | null)[]
}
```

- [ ] **Step 2: Create sparkline.ts**

Create `apps/web/src/lib/sparkline.ts`:

```ts
import type { NetworkServerSummary } from './network-types'

export const SPARKLINE_LENGTH = 30

export interface SparklinePoint {
  latency: number | null
  loss: number | null
}

export function seedFromSummary(summary: NetworkServerSummary): SparklinePoint[] {
  const points: SparklinePoint[] = []
  for (let i = 0; i < SPARKLINE_LENGTH; i++) {
    points.push({
      latency: summary.latency_sparkline[i] ?? null,
      loss: summary.loss_sparkline[i] ?? null,
    })
  }
  return points
}

export function toBarData(points: SparklinePoint[], pick: 'latency' | 'lossPercent'): (number | null)[] {
  return points.map((p) => {
    if (pick === 'lossPercent') {
      return p.loss != null ? p.loss * 100 : null
    }
    return p.latency
  })
}

export function summaryStats(points: SparklinePoint[]): {
  avgLatency: number | null
  avgLoss: number | null
} {
  const validLatencies = points.map((p) => p.latency).filter((v): v is number => v != null)
  const validLosses = points.map((p) => p.loss).filter((v): v is number => v != null)

  const avgLatency =
    validLatencies.length > 0 ? validLatencies.reduce((a, b) => a + b, 0) / validLatencies.length : null

  const avgLoss = validLosses.length > 0 ? validLosses.reduce((a, b) => a + b, 0) / validLosses.length : null

  return { avgLatency, avgLoss }
}
```

- [ ] **Step 3: Write tests**

Create `apps/web/src/lib/sparkline.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import type { NetworkServerSummary } from './network-types'
import { SPARKLINE_LENGTH, seedFromSummary, summaryStats, toBarData } from './sparkline'

function makeSummary(
  latency: (number | null)[],
  loss: (number | null)[]
): NetworkServerSummary {
  // Pad to 30 if shorter
  while (latency.length < SPARKLINE_LENGTH) latency.unshift(null)
  while (loss.length < SPARKLINE_LENGTH) loss.unshift(null)
  return {
    anomaly_count: 0,
    last_probe_at: null,
    online: true,
    server_id: 'test',
    server_name: 'Test',
    targets: [],
    latency_sparkline: latency,
    loss_sparkline: loss,
  }
}

describe('seedFromSummary', () => {
  it('returns array of length 30', () => {
    const points = seedFromSummary(makeSummary([], []))
    expect(points).toHaveLength(30)
  })

  it('propagates nulls from front-padded arrays', () => {
    const points = seedFromSummary(makeSummary([null, null, 50, 100], [null, null, 0.01, 0.05]))
    expect(points[0].latency).toBeNull()
    expect(points[0].loss).toBeNull()
    expect(points[28].latency).toBe(50)
    expect(points[28].loss).toBe(0.01)
    expect(points[29].latency).toBe(100)
    expect(points[29].loss).toBe(0.05)
  })

  it('zips latency and loss at same positions', () => {
    const points = seedFromSummary(makeSummary([10, 20], [0.1, 0.2]))
    expect(points[28]).toEqual({ latency: 10, loss: 0.1 })
    expect(points[29]).toEqual({ latency: 20, loss: 0.2 })
  })
})

describe('toBarData', () => {
  it('extracts latency values preserving null', () => {
    const points = seedFromSummary(makeSummary([null, 50], [null, 0.01]))
    const data = toBarData(points, 'latency')
    expect(data[28]).toBeNull()
    expect(data[29]).toBe(50)
  })

  it('converts loss to percent', () => {
    const points = seedFromSummary(makeSummary([50], [0.05]))
    const data = toBarData(points, 'lossPercent')
    expect(data[29]).toBe(5)
  })

  it('preserves null in loss percent', () => {
    const points = seedFromSummary(makeSummary([50], [null]))
    const data = toBarData(points, 'lossPercent')
    expect(data[28]).toBeNull()
  })
})

describe('summaryStats', () => {
  it('averages non-null latencies only', () => {
    const points = seedFromSummary(makeSummary([100, 200], [0.1, 0.2]))
    const { avgLatency } = summaryStats(points)
    expect(avgLatency).toBe(150)
  })

  it('returns null avgLatency when all null', () => {
    const points = seedFromSummary(makeSummary([], []))
    const { avgLatency, avgLoss } = summaryStats(points)
    expect(avgLatency).toBeNull()
    expect(avgLoss).toBeNull()
  })

  it('returns null avgLoss when all loss values are null', () => {
    const lat = Array(30).fill(50)
    const loss: (number | null)[] = Array(30).fill(null)
    const points = seedFromSummary(makeSummary(lat, loss))
    const { avgLatency, avgLoss } = summaryStats(points)
    expect(avgLatency).toBe(50)
    expect(avgLoss).toBeNull()
  })
})
```

- [ ] **Step 4: Run tests**

Run: `cd apps/web && bun run test -- sparkline`
Expected: all tests pass

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/lib/network-types.ts apps/web/src/lib/sparkline.ts apps/web/src/lib/sparkline.test.ts
git commit -m "feat(web): add sparkline module with seedFromSummary, toBarData, summaryStats"
```

---

### Task 5: Frontend — ServerCard rewrite + UptimeBar null fix + test migration

**Files:**
- Modify: `apps/web/src/components/server/server-card.tsx`
- Modify: `apps/web/src/components/ui/uptime-bar.tsx`
- Modify: `apps/web/src/components/server/server-card.test.tsx`
- Modify: `apps/web/src/components/ui/uptime-bar.test.tsx`

- [ ] **Step 1: Fix UptimeBar null rendering**

In `apps/web/src/components/ui/uptime-bar.tsx`, change the `barHeight` function (line 14-23). Currently `value == null` returns `'100%'`. Change to:

```ts
function barHeight(value: number | null): string {
  if (value == null) {
    return `${MIN_HEIGHT_PCT}%`
  }
  if (effectiveMax <= 0) {
    return `${MIN_HEIGHT_PCT}%`
  }
  const pct = (value / effectiveMax) * 100
  return `${Math.min(100, Math.max(MIN_HEIGHT_PCT, pct))}%`
}
```

- [ ] **Step 2: Update uptime-bar.test.tsx**

In `apps/web/src/components/ui/uptime-bar.test.tsx`, find the test at line 36-40 that asserts null renders at 100% height. Change the expected value:

```ts
it('renders null values at minimum height', () => {
  const { getAllByTestId } = render(<UptimeBar data={[null, 50]} getColor={greenColor} />)
  const bars = getAllByTestId('uptime-bar-item')
  expect(bars[0].style.height).toBe('10%')
})
```

- [ ] **Step 3: Rewrite ServerCard data consumption**

In `apps/web/src/components/server/server-card.tsx`:

**Replace imports** — remove `useNetworkRealtime`, add new imports:

```ts
// Remove this line:
// import { useNetworkRealtime } from '@/hooks/use-network-realtime'
// Add these:
import { useNetworkOverview } from '@/hooks/use-network-api'
import { seedFromSummary, summaryStats, toBarData } from '@/lib/sparkline'
```

**Replace the data computation block** (lines 112-134). Remove:

```ts
const { data: networkData } = useNetworkRealtime(server.id)

const { latencyData, lossData, avgLatency, avgLoss } = useMemo(() => {
  const allResults = Object.values(networkData)
    .flat()
    .sort((a, b) => a.timestamp.localeCompare(b.timestamp))
    .slice(-20)

  const latency = allResults.map((r) => r.avg_latency)
  const loss = allResults.map((r) => r.packet_loss * 100)

  const validLatencies = latency.filter((v): v is number => v != null)
  const avg = validLatencies.length > 0 ? validLatencies.reduce((a, b) => a + b, 0) / validLatencies.length : null
  const avgL = loss.length > 0 ? loss.reduce((a, b) => a + b, 0) / loss.length : 0

  return { latencyData: latency, lossData: loss, avgLatency: avg, avgLoss: avgL }
}, [networkData])
```

Replace with:

```ts
const { data: overview } = useNetworkOverview()
const points = useMemo(() => {
  const summary = overview?.find((s) => s.server_id === server.id)
  return summary ? seedFromSummary(summary) : []
}, [overview, server.id])
const latencyData = toBarData(points, 'latency')
const lossData = toBarData(points, 'lossPercent')
const { avgLatency, avgLoss } = summaryStats(points)
const hasAnyData = points.some((p) => p.latency != null || p.loss != null)
```

**Update the visibility condition** at line ~198. Replace:

```tsx
{latencyData.length > 0 && (
```

With:

```tsx
{hasAnyData && (
```

- [ ] **Step 4: Widen loss helper function signatures for null**

In `server-card.tsx`, update three functions:

`getLatencyColor` (line 46-57) — already handles null, returns `'#ef4444'`. Change the null case to return gray:

```ts
function getLatencyColor(ms: number | null): string {
  if (ms == null) {
    return '#d1d5db'
  }
  // ... rest unchanged
}
```

`getLossColor` (line 59-70) — same fix:

```ts
function getLossColor(loss: number | null): string {
  if (loss == null) {
    return '#d1d5db'
  }
  // ... rest unchanged
}
```

`getLossTextClass` (line 85-93) — widen parameter to `number | null`:

```ts
function getLossTextClass(loss: number | null): string {
  if (loss == null) {
    return 'text-muted-foreground'
  }
  // ... rest unchanged
}
```

`formatPacketLoss` (line 102-104) — widen parameter to `number | null`:

```ts
function formatPacketLoss(loss: number | null): string {
  if (loss == null) {
    return '-'
  }
  return `${loss.toFixed(1)}%`
}
```

- [ ] **Step 5: Update server-card.test.tsx**

Replace the mock setup. Currently (line 5-22) it mocks `useNetworkRealtime`. Change to mock `useNetworkOverview` from `@/hooks/use-network-api`:

```ts
vi.mock('@/hooks/use-network-api', () => ({
  useNetworkOverview: vi.fn(() => ({
    data: [
      {
        server_id: 'server-1',
        server_name: 'Test Server',
        online: true,
        last_probe_at: null,
        anomaly_count: 0,
        targets: [],
        latency_sparkline: Array(30).fill(null),
        loss_sparkline: Array(30).fill(null),
      },
    ],
  })),
}))
```

Remove the old `useNetworkRealtime` mock entirely.

Update the test that checks network quality rendering (line 96+). Since the top-level mock returns all-null sparklines, override it per-test using `vi.mocked`:

```ts
import { useNetworkOverview } from '@/hooks/use-network-api'

it('renders network quality bars when sparkline data exists', () => {
  vi.mocked(useNetworkOverview).mockReturnValue({
    data: [
      {
        server_id: 'server-1',
        server_name: 'Test Server',
        online: true,
        last_probe_at: null,
        anomaly_count: 0,
        targets: [],
        latency_sparkline: [...Array(28).fill(null), 32, 45],
        loss_sparkline: [...Array(28).fill(null), 0.002, 0.001],
      },
    ],
  } as ReturnType<typeof useNetworkOverview>)

  const { container } = render(<ServerCard server={makeServer()} />)
  const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
  expect(bars.length).toBeGreaterThan(0)
})
```

Delete the test that checks chronological sort (line 128+). This behavior no longer applies — the backend handles ordering. The sparkline arrives pre-ordered from the overview endpoint.

- [ ] **Step 6: Run all frontend tests**

Run: `cd apps/web && bun run test`
Expected: all tests pass

- [ ] **Step 7: Run linting and type checks**

Run: `cd apps/web && bun x ultracite check && bun run typecheck`
Expected: zero errors

- [ ] **Step 8: Commit**

```bash
git add apps/web/src/components/server/server-card.tsx \
       apps/web/src/components/ui/uptime-bar.tsx \
       apps/web/src/components/server/server-card.test.tsx \
       apps/web/src/components/ui/uptime-bar.test.tsx
git commit -m "feat(web): replace WS-only sparkline with overview seed

- ServerCard now consumes latency/loss from useNetworkOverview
- Remove useNetworkRealtime dependency from list page
- Fix UptimeBar null rendering (gray at 10% instead of red at 100%)
- Widen loss helper signatures for null
- Migrate existing tests to new mock shape"
```

---

### Task 6: Final verification

**Files:** none (verification only)

- [ ] **Step 1: Full Rust test suite**

Run: `cargo test --workspace`
Expected: all tests pass, including the 8 new sparkline tests

- [ ] **Step 2: Full frontend test suite**

Run: `cd apps/web && bun run test`
Expected: all tests pass (existing + new sparkline + updated server-card + updated uptime-bar)

- [ ] **Step 3: Clippy clean**

Run: `cargo clippy --workspace -- -D warnings`
Expected: zero warnings

- [ ] **Step 4: Ultracite + TypeScript clean**

Run: `cd apps/web && bun x ultracite check && bun run typecheck`
Expected: zero errors

- [ ] **Step 5: Manual smoke test (if dev server available)**

Start: `cargo run -p serverbee-server` + `cd apps/web && bun run dev`

1. Open `/servers?view=grid` — every card should show 30 sparkline bars immediately
2. Wait 60s — bars should shift (overview refetch)
3. Navigate to a server detail page and back — sparkline still populated
4. Check a server with multiple targets — displayed avgLatency should be the average, not a random one
