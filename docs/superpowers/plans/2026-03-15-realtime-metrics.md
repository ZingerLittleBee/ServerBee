# Real-time Metrics Chart Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a real-time mode to the server detail page that accumulates live WebSocket data in a ring buffer and displays it in charts.

**Architecture:** Frontend-only change. A new `useRealtimeMetrics` hook subscribes to TanStack Query cache changes on the `['servers']` key, deduplicates via `last_active`, and accumulates data points in a ring buffer (10 min / ~200 points). The server detail page adds a "Real-time" button (default selected) and conditionally sources chart data from the hook or the existing REST API.

**Tech Stack:** React 19, TanStack Query, Recharts, Vitest

**Spec:** `docs/superpowers/specs/2026-03-15-realtime-metrics-design.md`

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `apps/web/src/hooks/use-realtime-metrics.ts` | Hook: subscribe to `['servers']` cache, deduplicate via `last_active`, accumulate ring buffer, seed on mount |
| Create | `apps/web/src/hooks/use-realtime-metrics.test.ts` | Unit tests for the hook's pure helper + integration tests via renderHook |
| Modify | `apps/web/src/hooks/use-api.ts` | Add optional `enabled` parameter to `useServerRecords` |
| Modify | `apps/web/src/hooks/use-api.test.tsx` | Add test for `useServerRecords` with `enabled: false` |
| Modify | `apps/web/src/routes/_authed/servers/$id.tsx` | Add Real-time button, wire up hook, conditional chart data source, realtime formatTime |
| Modify | `README.md` | Add real-time metrics to feature list |
| Modify | `README.zh-CN.md` | Add real-time metrics to feature list (Chinese) |
| Modify | `CHANGELOG.md` | Add v0.2.0 entry |

---

## Task 1: Create `useRealtimeMetrics` hook

**Files:**
- Create: `apps/web/src/hooks/use-realtime-metrics.ts`
- Create: `apps/web/src/hooks/use-realtime-metrics.test.ts`

### Step 1.1: Write the pure helper function and its tests

The hook internally needs a pure function `toRealtimeDataPoint` that converts a `ServerMetrics` object to a `RealtimeDataPoint`. Write this function and export it for testing first.

- [ ] **Write test file** `apps/web/src/hooks/use-realtime-metrics.test.ts`:

```typescript
import { describe, expect, it } from 'vitest'
import type { ServerMetrics } from './use-servers-ws'
import { toRealtimeDataPoint } from './use-realtime-metrics'

function makeMetrics(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 's1',
    name: 'Test',
    online: true,
    last_active: 1710500000,
    cpu: 50,
    mem_used: 8_000_000_000,
    mem_total: 16_000_000_000,
    swap_used: 0,
    swap_total: 4_000_000_000,
    disk_used: 100_000_000_000,
    disk_total: 500_000_000_000,
    net_in_speed: 1000,
    net_out_speed: 500,
    net_in_transfer: 10_000,
    net_out_transfer: 5000,
    load1: 1.5,
    load5: 1.2,
    load15: 1.0,
    tcp_conn: 100,
    udp_conn: 10,
    process_count: 200,
    uptime: 3600,
    cpu_name: 'Intel i7',
    os: 'Linux',
    region: 'US-East',
    country_code: 'US',
    group_id: 'g1',
    ...overrides
  }
}

describe('toRealtimeDataPoint', () => {
  it('converts ServerMetrics to RealtimeDataPoint with correct percentages', () => {
    const metrics = makeMetrics({ cpu: 75, mem_used: 4_000_000_000, mem_total: 16_000_000_000, disk_used: 250_000_000_000, disk_total: 500_000_000_000, last_active: 1710500000 })
    const point = toRealtimeDataPoint(metrics)
    expect(point.cpu).toBe(75)
    expect(point.memory_pct).toBe(25) // 4/16 * 100
    expect(point.disk_pct).toBe(50) // 250/500 * 100
    expect(point.timestamp).toBe(new Date(1710500000 * 1000).toISOString())
  })

  it('handles zero mem_total without division by zero', () => {
    const metrics = makeMetrics({ mem_total: 0, mem_used: 1000 })
    const point = toRealtimeDataPoint(metrics)
    expect(point.memory_pct).toBe(0)
  })

  it('handles zero disk_total without division by zero', () => {
    const metrics = makeMetrics({ disk_total: 0, disk_used: 1000 })
    const point = toRealtimeDataPoint(metrics)
    expect(point.disk_pct).toBe(0)
  })

  it('maps all metric fields correctly', () => {
    const metrics = makeMetrics({
      net_in_speed: 2000,
      net_out_speed: 800,
      net_in_transfer: 50_000,
      net_out_transfer: 25_000,
      load1: 2.5,
      load5: 1.8,
      load15: 1.1
    })
    const point = toRealtimeDataPoint(metrics)
    expect(point.net_in_speed).toBe(2000)
    expect(point.net_out_speed).toBe(800)
    expect(point.net_in_transfer).toBe(50_000)
    expect(point.net_out_transfer).toBe(25_000)
    expect(point.load1).toBe(2.5)
    expect(point.load5).toBe(1.8)
    expect(point.load15).toBe(1.1)
  })
})
```

- [ ] **Write implementation** `apps/web/src/hooks/use-realtime-metrics.ts`:

```typescript
import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import type { ServerMetrics } from './use-servers-ws'

export interface RealtimeDataPoint {
  cpu: number
  disk_pct: number
  load1: number
  load5: number
  load15: number
  memory_pct: number
  net_in_speed: number
  net_in_transfer: number
  net_out_speed: number
  net_out_transfer: number
  timestamp: string
}

const MAX_BUFFER_SIZE = 200
const TRIM_THRESHOLD = 250

export function toRealtimeDataPoint(metrics: ServerMetrics): RealtimeDataPoint {
  return {
    timestamp: new Date(metrics.last_active * 1000).toISOString(),
    cpu: metrics.cpu,
    memory_pct: metrics.mem_total > 0 ? (metrics.mem_used / metrics.mem_total) * 100 : 0,
    disk_pct: metrics.disk_total > 0 ? (metrics.disk_used / metrics.disk_total) * 100 : 0,
    net_in_speed: metrics.net_in_speed,
    net_out_speed: metrics.net_out_speed,
    net_in_transfer: metrics.net_in_transfer,
    net_out_transfer: metrics.net_out_transfer,
    load1: metrics.load1,
    load5: metrics.load5,
    load15: metrics.load15
  }
}

export function useRealtimeMetrics(serverId: string): RealtimeDataPoint[] {
  const queryClient = useQueryClient()
  const bufferRef = useRef<RealtimeDataPoint[]>([])
  const lastActiveRef = useRef<number>(0)
  const [, setTick] = useState(0)

  useEffect(() => {
    // Seed from current cache snapshot
    const cached = queryClient.getQueryData<ServerMetrics[]>(['servers'])
    const current = cached?.find((s) => s.id === serverId)
    if (current && current.online && current.last_active > 0) {
      bufferRef.current = [toRealtimeDataPoint(current)]
      lastActiveRef.current = current.last_active
      setTick((t) => t + 1)
    }

    const unsubscribe = queryClient.getQueryCache().subscribe((event) => {
      if (event.type !== 'updated' || event.query.queryHash !== '["servers"]') {
        return
      }

      const servers = queryClient.getQueryData<ServerMetrics[]>(['servers'])
      const server = servers?.find((s) => s.id === serverId)
      if (!server || server.last_active <= lastActiveRef.current) {
        return
      }

      lastActiveRef.current = server.last_active
      bufferRef.current.push(toRealtimeDataPoint(server))

      if (bufferRef.current.length > TRIM_THRESHOLD) {
        bufferRef.current = bufferRef.current.slice(-MAX_BUFFER_SIZE)
      }

      setTick((t) => t + 1)
    })

    return () => {
      unsubscribe()
      bufferRef.current = []
      lastActiveRef.current = 0
    }
  }, [queryClient, serverId])

  return bufferRef.current
}
```

- [ ] **Run tests to verify they pass**

Run: `cd apps/web && bun run test -- --run use-realtime-metrics`
Expected: All 4 tests PASS

- [ ] **Commit**

```bash
git add apps/web/src/hooks/use-realtime-metrics.ts apps/web/src/hooks/use-realtime-metrics.test.ts
git commit -m "feat: add useRealtimeMetrics hook with ring buffer and deduplication"
```

---

## Task 2: Extend `useServerRecords` with `enabled` option

**Files:**
- Modify: `apps/web/src/hooks/use-api.ts`
- Modify: `apps/web/src/hooks/use-api.test.tsx`

- [ ] **Add test for disabled query** in `apps/web/src/hooks/use-api.test.tsx`:

Add this test inside the existing `describe('useServerRecords', ...)` block, after the existing tests:

```typescript
it('does not fetch when enabled is false', async () => {
  const { result } = renderHook(() => useServerRecords('srv-1', 1, '5m', { enabled: false }), {
    wrapper: createWrapper()
  })

  await waitFor(() => {
    expect(result.current.fetchStatus).toBe('idle')
  })

  expect(globalThis.fetch).not.toHaveBeenCalled()
})
```

- [ ] **Modify `useServerRecords`** in `apps/web/src/hooks/use-api.ts`:

Change the function signature and `enabled` field:

```typescript
export function useServerRecords(id: string, hours: number, interval: string, options?: { enabled?: boolean }) {
  return useQuery<ServerRecord[]>({
    queryKey: ['servers', id, 'records', hours, interval],
    queryFn: () => {
      const now = new Date()
      const from = new Date(now.getTime() - hours * 3600 * 1000).toISOString()
      const to = now.toISOString()
      return api.get<ServerRecord[]>(
        `/api/servers/${id}/records?from=${encodeURIComponent(from)}&to=${encodeURIComponent(to)}&interval=${encodeURIComponent(interval)}`
      )
    },
    enabled: id.length > 0 && (options?.enabled ?? true),
    refetchInterval: 60_000
  })
}
```

- [ ] **Run tests to verify all pass**

Run: `cd apps/web && bun run test -- --run use-api`
Expected: All 5 tests PASS (3 existing + 1 new)

- [ ] **Commit**

```bash
git add apps/web/src/hooks/use-api.ts apps/web/src/hooks/use-api.test.tsx
git commit -m "feat: add enabled option to useServerRecords hook"
```

---

## Task 3: Integrate real-time mode into server detail page

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`

- [ ] **Add import for the new hook** at top of file:

```typescript
import { useRealtimeMetrics } from '@/hooks/use-realtime-metrics'
```

- [ ] **Prepend real-time entry to `TIME_RANGES` and update default**:

Replace the existing `TIME_RANGES` array and `selectedRange` default:

```typescript
const TIME_RANGES: TimeRange[] = [
  { label: 'Real-time', hours: 0, interval: 'realtime' },
  { label: '1h', hours: 1, interval: 'raw' },
  { label: '6h', hours: 6, interval: 'raw' },
  { label: '24h', hours: 24, interval: 'raw' },
  { label: '7d', hours: 168, interval: 'hourly' },
  { label: '30d', hours: 720, interval: 'hourly' }
]
```

In `ServerDetailPage`, change:
```typescript
const [selectedRange, setSelectedRange] = useState(0)
```

- [ ] **Wire up the hook and conditional data sourcing**:

In `ServerDetailPage`, after the existing `useServer` call:

1. Derive `isRealtime`:
```typescript
const isRealtime = range.interval === 'realtime'
```

2. Call the new hook (always called, React rules of hooks):
```typescript
const realtimeData = useRealtimeMetrics(id)
```

3. Pass `enabled` to `useServerRecords`:
```typescript
const { data: records } = useServerRecords(id, range.hours, range.interval, { enabled: !isRealtime })
```

4. Add `enabled: !isRealtime` to the `gpuRecords` query:
```typescript
const { data: gpuRecords } = useQuery<GpuRecordAggregated[]>({
  // ... existing queryKey, queryFn ...
  enabled: id.length > 0 && !isRealtime,
  refetchInterval: 60_000
})
```

5. Replace the existing `chartData` useMemo to handle both modes:
```typescript
const chartData = useMemo(() => {
  if (isRealtime) {
    return realtimeData
  }
  if (!records) {
    return []
  }
  return records.map((r) => ({
    timestamp: r.time,
    cpu: r.cpu,
    memory_pct: server?.mem_total ? (r.mem_used / server.mem_total) * 100 : 0,
    disk_pct: server?.disk_total ? (r.disk_used / server.disk_total) * 100 : 0,
    net_in_speed: r.net_in_speed,
    net_out_speed: r.net_out_speed,
    net_in_transfer: r.net_in_transfer,
    net_out_transfer: r.net_out_transfer,
    load1: r.load1,
    load5: r.load5,
    load15: r.load15,
    temperature: r.temperature
  }))
}, [isRealtime, realtimeData, records, server])
```

- [ ] **Add realtime `formatTime` and pass to charts**:

After the `chartData` useMemo, add:

```typescript
const realtimeFormatTime = useMemo(() => {
  if (!isRealtime) {
    return undefined
  }
  const firstTimestamp = realtimeData.length > 0 ? realtimeData[0].timestamp : ''
  return (time: string) => {
    if (time === firstTimestamp) {
      return new Date(time).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
    }
    const d = new Date(time)
    return `${String(d.getMinutes()).padStart(2, '0')}:${String(d.getSeconds()).padStart(2, '0')}`
  }
}, [isRealtime, realtimeData])
```

Then update the `hasTemperature` and `hasGpu` conditions to account for realtime mode:

```typescript
const hasTemperature = !isRealtime && chartData.some((d) => (d as Record<string, unknown>).temperature != null && ((d as Record<string, unknown>).temperature as number) > 0)
const hasGpu = !isRealtime && gpuChartData.length > 0
```

- [ ] **Pass `formatTime` to all MetricsChart components**:

For each `<MetricsChart>` in the render, add the `formatTime` prop conditionally. Example for CPU:

```tsx
<MetricsChart
  color="var(--color-chart-1)"
  data={chartData}
  dataKey="cpu"
  formatTime={realtimeFormatTime}
  title="CPU Usage"
  unit="%"
/>
```

Apply the same `formatTime={realtimeFormatTime}` prop to all MetricsChart instances (CPU, Memory, Disk, Network In, Network Out, Load, Temperature, GPU Usage, GPU Temperature). When `realtimeFormatTime` is `undefined` (history mode), the component falls back to its `defaultFormatTime`.

- [ ] **Run lint and typecheck**

Run: `cd apps/web && bun x ultracite check && bun run typecheck`
Expected: No errors

- [ ] **Commit**

```bash
git add apps/web/src/routes/_authed/servers/\$id.tsx
git commit -m "feat: add real-time mode to server detail page charts"
```

---

## Task 4: Update documentation

**Files:**
- Modify: `README.md`
- Modify: `README.zh-CN.md`
- Modify: `CHANGELOG.md`

- [ ] **Update README.md** — change line 11 from:

```markdown
- **Detailed Metrics** -- Historical charts (1h/6h/24h/7d/30d) for CPU, memory, disk, network, load, temperature, GPU
```

to:

```markdown
- **Detailed Metrics** -- Real-time streaming charts + historical views (1h/6h/24h/7d/30d) for CPU, memory, disk, network, load, temperature, GPU
```

- [ ] **Update README.zh-CN.md** — change line 11 from:

```markdown
- **详细指标** -- 历史图表 (1h/6h/24h/7d/30d)，涵盖 CPU、内存、磁盘、网络、负载、温度、GPU
```

to:

```markdown
- **详细指标** -- 实时流式图表 + 历史视图 (1h/6h/24h/7d/30d)，涵盖 CPU、内存、磁盘、网络、负载、温度、GPU
```

- [ ] **Add v0.2.0 to CHANGELOG.md** — insert after line 7 (before the `## [0.1.0]` section):

```markdown
## [0.2.0] - 2026-03-15

### Added

- **Real-time Metrics Charts** -- Server detail page now defaults to real-time mode, streaming live CPU, memory, disk, network, and load data from WebSocket updates at ~3s intervals. Data is accumulated in a 10-minute ring buffer (~200 data points). Users can switch between real-time and historical views (1h/6h/24h/7d/30d). Time axis shows `mm:ss` format with `HH:mm:ss` on the first data point.

```

- [ ] **Commit**

```bash
git add README.md README.zh-CN.md CHANGELOG.md
git commit -m "docs: add real-time metrics to README and CHANGELOG v0.2.0"
```

---

## Task 5: Final verification

- [ ] **Run all frontend tests**

Run: `cd apps/web && bun run test`
Expected: All tests pass (existing 72 + new tests)

- [ ] **Run typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: No errors

- [ ] **Run lint**

Run: `cd apps/web && bun x ultracite check`
Expected: No errors

- [ ] **Verify dev server compiles** (optional, if server is available)

Run: `cd apps/web && bun run build`
Expected: Build succeeds
