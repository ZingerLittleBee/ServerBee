# Metric Card Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a new `metric-card` dashboard widget that shows current value, 1h delta, sparkline, and 24h peak/avg sub-stats for CPU / memory / network / disk I/O on a single server.

**Architecture:** New React widget under `apps/web/src/components/dashboard/widgets/metric-card/`. Data comes from existing `useServerRecords(server_id, 24, '5m')` history endpoint plus live `ServerMetrics` ticks spliced onto the tail. Peak / avg / 1h-delta derived in a dedicated hook. Wired into the existing widget renderer / picker / config-dialog system. No backend changes.

**Tech Stack:** React 19, TanStack Query, recharts `<Area>`, shadcn/ui primitives, vitest, react-i18next, Tailwind v4.

**Spec:** `docs/superpowers/specs/2026-05-27-metric-card-widget-design.md`

---

## File Structure

**Create:**
- `apps/web/src/components/dashboard/widgets/metric-card/metric-card-config.ts` — per-metric accent / icon / formatter map
- `apps/web/src/components/dashboard/widgets/metric-card/metric-card-header.tsx`
- `apps/web/src/components/dashboard/widgets/metric-card/metric-card-value.tsx`
- `apps/web/src/components/dashboard/widgets/metric-card/metric-card-sparkline.tsx`
- `apps/web/src/components/dashboard/widgets/metric-card/metric-card-stats.tsx`
- `apps/web/src/components/dashboard/widgets/metric-card.tsx` — entry component
- `apps/web/src/components/dashboard/widgets/metric-card.test.tsx`
- `apps/web/src/hooks/use-metric-series.ts` — derives sparkline + peak/avg/delta
- `apps/web/src/hooks/use-metric-series.test.ts`

**Modify:**
- `apps/web/src/lib/widget-types.ts` — add `MetricCardConfig`, register in `WIDGET_TYPES`, add to `WidgetConfig` union
- `apps/web/src/lib/widget-helpers.ts` — add `network` / `disk_io` cases to `extractLiveMetric` and `extractRecordMetric`
- `apps/web/src/components/dashboard/widget-renderer.tsx` — add `case 'metric-card'`
- `apps/web/src/components/dashboard/widget-picker.tsx` — register icon
- `apps/web/src/components/dashboard/widget-config-dialog.tsx` — add `MetricCardForm`
- `apps/web/src/locales/en/dashboard.json` — `widgetPicker.types.metric-card.*` + `metricCard.*` keys
- `apps/web/src/locales/zh/dashboard.json` — same keys (Chinese)

---

## Task 1: Extend widget-helpers with network / disk_io metrics

**Files:**
- Modify: `apps/web/src/lib/widget-helpers.ts`

The metric-card widget needs `network` (in+out throughput) and `disk_io` (read+write throughput) as first-class metric keys. The codebase already has `bandwidth` as an alias on live data and `net_in_speed` / `net_out_speed` on records; for disk I/O we need `disk_read_speed` + `disk_write_speed` from the record. We add these so downstream code (and any future widget) can share them.

- [ ] **Step 1: Inspect the record schema**

Run: `grep -n "disk_read_speed\|disk_write_speed\|net_in_speed\|net_out_speed" apps/web/src/lib/api-schema.ts`
Expected: confirm `ServerMetricRecord` includes those four fields. If `disk_read_speed` / `disk_write_speed` are absent, check `apps/web/src/lib/disk-io.ts` to see how `buildMergedDiskIoSeries` reads them and use the same field names.

- [ ] **Step 2: Add `network` and `disk_io` to `extractLiveMetric`**

In `apps/web/src/lib/widget-helpers.ts`, extend the switch in `extractLiveMetric`:

```ts
case 'network':
case 'bandwidth':
  return server.net_in_speed + server.net_out_speed
case 'disk_io':
  return (server.disk_read_speed ?? 0) + (server.disk_write_speed ?? 0)
```

Place the `'network'` case alongside `'bandwidth'` so both keys behave identically. The `disk_io` case uses nullish coalescing because older agents may omit those fields.

- [ ] **Step 3: Add `network` and `disk_io` to `extractRecordMetric`**

Extend the switch in `extractRecordMetric`:

```ts
case 'network':
  return record.net_in_speed + record.net_out_speed
case 'disk_io':
  return (record.disk_read_speed ?? 0) + (record.disk_write_speed ?? 0)
```

- [ ] **Step 4: Add labels**

Extend `METRIC_LABELS` with:

```ts
network: 'Network',
disk_io: 'Disk I/O'
```

- [ ] **Step 5: Run type check**

Run: `cd apps/web && bun run typecheck`
Expected: no errors. If `ServerMetrics` is missing `disk_read_speed` / `disk_write_speed`, add the optional fields to its type (`apps/web/src/hooks/use-servers-ws.ts`) but DO NOT change the WS payload — they were already being sent by the agent for the disk-io widget, this is just declaring them in the TS type.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/lib/widget-helpers.ts apps/web/src/hooks/use-servers-ws.ts
git commit -m "feat(web): add network and disk_io metric extractors"
```

---

## Task 2: Register widget type and config schema

**Files:**
- Modify: `apps/web/src/lib/widget-types.ts`

- [ ] **Step 1: Add `MetricCardConfig` interface**

In `apps/web/src/lib/widget-types.ts` (after `StatNumberConfig` so related types are co-located), add:

```ts
export type MetricCardMetric = 'cpu' | 'memory' | 'network' | 'disk_io'

export interface MetricCardConfig {
  metric: MetricCardMetric
  server_id: string
  label?: string
}
```

- [ ] **Step 2: Register `metric-card` in `WIDGET_TYPES`**

Add an entry (preserving the existing `as const satisfies` shape):

```ts
{
  id: 'metric-card',
  label: 'Metric Card',
  category: 'Real-time',
  defaultW: 4,
  defaultH: 4,
  minW: 3,
  minH: 3,
  maxW: 6,
  maxH: 6
}
```

Place it right after the `stat-number` entry so picker ordering puts the richer card next to its lighter sibling.

- [ ] **Step 3: Add to `WidgetConfig` union**

Update the `WidgetConfig` union to include `| MetricCardConfig`.

- [ ] **Step 4: Run type check**

Run: `cd apps/web && bun run typecheck`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/lib/widget-types.ts
git commit -m "feat(web): register metric-card widget type"
```

---

## Task 3: Per-metric configuration map

**Files:**
- Create: `apps/web/src/components/dashboard/widgets/metric-card/metric-card-config.ts`

This module owns everything that varies per metric: icon, accent color token, value formatter, delta unit, and whether deltas use semantic (high=bad) or neutral coloring. Keeping it isolated means rendering components stay metric-agnostic.

- [ ] **Step 1: Create the file**

```ts
import { Cpu, HardDriveDownload, MemoryStick, Network } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { formatSpeed } from '@/lib/utils'
import type { MetricCardMetric } from '@/lib/widget-types'

export type DeltaUnit = 'pp' | 'percent'
export type DeltaTone = 'semantic' | 'neutral'

export interface MetricCardSpec {
  icon: LucideIcon
  accent: string // CSS variable name e.g. '--chart-4'
  formatValue: (n: number) => string
  deltaUnit: DeltaUnit
  deltaTone: DeltaTone
  labelKey: string // i18n key under 'metricCard.metric.*'
}

const formatPercent = (n: number) => `${n.toFixed(1)}%`

export const METRIC_CARD_SPECS: Record<MetricCardMetric, MetricCardSpec> = {
  cpu: {
    icon: Cpu,
    accent: '--chart-4',
    formatValue: formatPercent,
    deltaUnit: 'pp',
    deltaTone: 'semantic',
    labelKey: 'metricCard.metric.cpu'
  },
  memory: {
    icon: MemoryStick,
    accent: '--chart-3',
    formatValue: formatPercent,
    deltaUnit: 'pp',
    deltaTone: 'semantic',
    labelKey: 'metricCard.metric.memory'
  },
  network: {
    icon: Network,
    accent: '--chart-1',
    formatValue: formatSpeed,
    deltaUnit: 'percent',
    deltaTone: 'neutral',
    labelKey: 'metricCard.metric.network'
  },
  disk_io: {
    icon: HardDriveDownload,
    accent: '--chart-2',
    formatValue: formatSpeed,
    deltaUnit: 'percent',
    deltaTone: 'neutral',
    labelKey: 'metricCard.metric.diskIo'
  }
}
```

Verify `formatSpeed` exists in `lib/utils.ts` (it is used by `disk-io.tsx`). If absent, fall back to `formatBytes` and append `/s`.

- [ ] **Step 2: Run type check**

Run: `cd apps/web && bun run typecheck`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/dashboard/widgets/metric-card/metric-card-config.ts
git commit -m "feat(web): metric-card per-metric spec map"
```

---

## Task 4: `use-metric-series` hook (TDD)

**Files:**
- Create: `apps/web/src/hooks/use-metric-series.ts`
- Test: `apps/web/src/hooks/use-metric-series.test.ts`

Pure derivation hook. Takes records + live server + metric; returns `{ points, current, peak, avg, oneHourDelta }` where:

- `points`: `Array<{ t: number; v: number }>` sorted by `t`, with the most recent live tick appended if newer than the last record
- `current`: the most recent value (live tick if available, else last record)
- `peak`: `Math.max(points.value)` or `null` for empty series
- `avg`: arithmetic mean or `null`
- `oneHourDelta`: `current - value_at(now - 1h)` or `null` if no sample is older than 55 minutes

- [ ] **Step 1: Write the failing tests**

Create `apps/web/src/hooks/use-metric-series.test.ts`:

```ts
import { renderHook } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { ServerMetricRecord } from '@/lib/api-schema'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { useMetricSeries } from './use-metric-series'

function record(time: string, cpu: number): ServerMetricRecord {
  return {
    time,
    cpu,
    mem_used: 0,
    disk_used: 0,
    load1: 0,
    load5: 0,
    load15: 0,
    net_in_speed: 0,
    net_out_speed: 0,
    disk_read_speed: 0,
    disk_write_speed: 0
  } as unknown as ServerMetricRecord
}

function server(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 's1',
    name: 'srv',
    online: true,
    cpu: 50,
    mem_used: 0,
    mem_total: 0,
    disk_used: 0,
    disk_total: 0,
    swap_used: 0,
    swap_total: 0,
    net_in_speed: 0,
    net_out_speed: 0,
    disk_read_speed: 0,
    disk_write_speed: 0,
    ...overrides
  } as unknown as ServerMetrics
}

describe('useMetricSeries', () => {
  it('returns null stats when records are empty', () => {
    const { result } = renderHook(() =>
      useMetricSeries({ records: [], server: server(), metric: 'cpu' })
    )
    expect(result.current.points).toHaveLength(1) // live tick still appended
    expect(result.current.peak).toBe(50)
    expect(result.current.avg).toBe(50)
    expect(result.current.oneHourDelta).toBeNull()
  })

  it('computes peak and avg from records + live tick', () => {
    const now = Date.now()
    const records = [
      record(new Date(now - 60 * 60_000).toISOString(), 20),
      record(new Date(now - 30 * 60_000).toISOString(), 60),
      record(new Date(now - 5 * 60_000).toISOString(), 40)
    ]
    const { result } = renderHook(() =>
      useMetricSeries({ records, server: server({ cpu: 80 }), metric: 'cpu' })
    )
    expect(result.current.current).toBe(80)
    expect(result.current.peak).toBe(80)
    expect(result.current.avg).toBeCloseTo((20 + 60 + 40 + 80) / 4)
  })

  it('computes 1h delta when a sample exists near 1h ago', () => {
    const now = Date.now()
    const records = [
      record(new Date(now - 62 * 60_000).toISOString(), 30),
      record(new Date(now - 1 * 60_000).toISOString(), 45)
    ]
    const { result } = renderHook(() =>
      useMetricSeries({ records, server: server({ cpu: 50 }), metric: 'cpu' })
    )
    expect(result.current.oneHourDelta).toBeCloseTo(50 - 30)
  })

  it('returns null delta when no sample is old enough', () => {
    const now = Date.now()
    const records = [record(new Date(now - 5 * 60_000).toISOString(), 30)]
    const { result } = renderHook(() =>
      useMetricSeries({ records, server: server({ cpu: 32 }), metric: 'cpu' })
    )
    expect(result.current.oneHourDelta).toBeNull()
  })

  it('aggregates network as in+out', () => {
    const { result } = renderHook(() =>
      useMetricSeries({
        records: [],
        server: server({ net_in_speed: 1000, net_out_speed: 2000 }),
        metric: 'network'
      })
    )
    expect(result.current.current).toBe(3000)
  })
})
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cd apps/web && bun run test use-metric-series`
Expected: FAIL with module-not-found for `./use-metric-series`.

- [ ] **Step 3: Implement the hook**

Create `apps/web/src/hooks/use-metric-series.ts`:

```ts
import { useMemo } from 'react'
import type { ServerMetricRecord } from '@/lib/api-schema'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { extractLiveMetric, extractRecordMetric } from '@/lib/widget-helpers'

export interface MetricSeriesPoint {
  t: number
  v: number
}

export interface MetricSeries {
  points: MetricSeriesPoint[]
  current: number
  peak: number | null
  avg: number | null
  oneHourDelta: number | null
}

interface Params {
  records: ServerMetricRecord[] | undefined
  server: ServerMetrics | undefined
  metric: string
}

const ONE_HOUR_MS = 60 * 60_000
const DELTA_WINDOW_MS = 5 * 60_000 // accept samples within ±5 min of the 1h mark

export function useMetricSeries({ records, server, metric }: Params): MetricSeries {
  return useMemo(() => {
    const points: MetricSeriesPoint[] = []

    if (records) {
      for (const r of records) {
        const t = new Date(r.time).getTime()
        if (Number.isFinite(t)) {
          points.push({ t, v: extractRecordMetric(r, metric, server) })
        }
      }
    }

    points.sort((a, b) => a.t - b.t)

    const liveValue = server ? extractLiveMetric(server, metric) : 0
    const liveTick: MetricSeriesPoint = { t: Date.now(), v: liveValue }

    const last = points.at(-1)
    if (!last || liveTick.t > last.t) {
      points.push(liveTick)
    }

    if (points.length === 0) {
      return { points, current: 0, peak: null, avg: null, oneHourDelta: null }
    }

    const current = points.at(-1)?.v ?? 0
    let peak = points[0].v
    let sum = 0
    for (const p of points) {
      if (p.v > peak) peak = p.v
      sum += p.v
    }
    const avg = sum / points.length

    const target = liveTick.t - ONE_HOUR_MS
    let oneHourDelta: number | null = null
    let bestDist = Number.POSITIVE_INFINITY
    for (const p of points) {
      const dist = Math.abs(p.t - target)
      if (dist <= DELTA_WINDOW_MS && dist < bestDist) {
        bestDist = dist
        oneHourDelta = current - p.v
      }
    }

    return { points, current, peak, avg, oneHourDelta }
  }, [records, server, metric])
}
```

- [ ] **Step 4: Run tests to confirm they pass**

Run: `cd apps/web && bun run test use-metric-series`
Expected: PASS (5/5).

- [ ] **Step 5: Run lint + typecheck**

Run: `cd apps/web && bun run typecheck && bun x ultracite check src/hooks/use-metric-series.ts src/hooks/use-metric-series.test.ts`
Expected: no errors / warnings.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/hooks/use-metric-series.ts apps/web/src/hooks/use-metric-series.test.ts
git commit -m "feat(web): use-metric-series hook for metric-card"
```

---

## Task 5: Subcomponent — `MetricCardHeader`

**Files:**
- Create: `apps/web/src/components/dashboard/widgets/metric-card/metric-card-header.tsx`

Layout: icon chip (left) → metric label (left, bold) → server name (right, muted, truncated).

- [ ] **Step 1: Implement**

```tsx
import type { LucideIcon } from 'lucide-react'
import { cn } from '@/lib/utils'

interface MetricCardHeaderProps {
  Icon: LucideIcon
  label: string
  serverName: string
  accent: string
}

export function MetricCardHeader({ Icon, label, serverName, accent }: MetricCardHeaderProps) {
  return (
    <div className="flex items-center gap-2.5">
      <div
        className={cn('flex size-8 shrink-0 items-center justify-center rounded-lg')}
        style={{ backgroundColor: `color-mix(in oklab, var(${accent}) 18%, transparent)` }}
        data-testid="metric-card-icon"
      >
        <Icon className="size-4" style={{ color: `var(${accent})` }} />
      </div>
      <span className="font-semibold text-sm leading-tight">{label}</span>
      <span className="ml-auto truncate text-muted-foreground text-xs leading-tight">{serverName}</span>
    </div>
  )
}
```

- [ ] **Step 2: Typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/dashboard/widgets/metric-card/metric-card-header.tsx
git commit -m "feat(web): metric-card header subcomponent"
```

---

## Task 6: Subcomponent — `MetricCardValue`

**Files:**
- Create: `apps/web/src/components/dashboard/widgets/metric-card/metric-card-value.tsx`

Big formatted value + delta row. Delta row is `▲/▼ {magnitude}{unit} · {pastLabel}` or `—` if delta is null.

- [ ] **Step 1: Implement**

```tsx
import { TrendingDown, TrendingUp } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { DeltaTone, DeltaUnit } from './metric-card-config'

interface MetricCardValueProps {
  formattedValue: string
  delta: number | null
  deltaUnit: DeltaUnit
  deltaTone: DeltaTone
  pastLabel: string // localized e.g. "past 1h"
}

function formatDelta(delta: number, unit: DeltaUnit): string {
  const sign = delta >= 0 ? '+' : '−'
  const magnitude = Math.abs(delta)
  if (unit === 'pp') {
    return `${sign}${magnitude.toFixed(1)}pp`
  }
  return `${sign}${magnitude.toFixed(0)}%`
}

function deltaColor(delta: number, tone: DeltaTone): string {
  if (tone === 'neutral') return 'text-muted-foreground'
  if (delta === 0) return 'text-muted-foreground'
  return delta > 0 ? 'text-destructive' : 'text-emerald-500'
}

export function MetricCardValue({
  formattedValue,
  delta,
  deltaUnit,
  deltaTone,
  pastLabel
}: MetricCardValueProps) {
  const Trend = delta !== null && delta < 0 ? TrendingDown : TrendingUp
  return (
    <div className="space-y-0.5">
      <p
        className="truncate font-bold text-3xl leading-tight tracking-tight tabular-nums"
        data-testid="metric-card-value"
      >
        {formattedValue}
      </p>
      <p
        className={cn('flex items-center gap-1 text-xs', delta === null ? 'text-muted-foreground' : deltaColor(delta, deltaTone))}
        data-testid="metric-card-delta"
      >
        {delta === null ? (
          <span>—</span>
        ) : (
          <>
            <Trend className="size-3" />
            <span className="font-medium tabular-nums">{formatDelta(delta, deltaUnit)}</span>
          </>
        )}
        <span className="text-muted-foreground">· {pastLabel}</span>
      </p>
    </div>
  )
}
```

- [ ] **Step 2: Typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/dashboard/widgets/metric-card/metric-card-value.tsx
git commit -m "feat(web): metric-card value + delta subcomponent"
```

---

## Task 7: Subcomponent — `MetricCardSparkline`

**Files:**
- Create: `apps/web/src/components/dashboard/widgets/metric-card/metric-card-sparkline.tsx`

Recharts `<AreaChart>` with no axes, no grid, no tooltip — pure decorative trend. Color via CSS var.

- [ ] **Step 1: Implement**

```tsx
import { useId } from 'react'
import { Area, AreaChart, ResponsiveContainer } from 'recharts'
import type { MetricSeriesPoint } from '@/hooks/use-metric-series'

interface MetricCardSparklineProps {
  points: MetricSeriesPoint[]
  accent: string
}

export function MetricCardSparkline({ points, accent }: MetricCardSparklineProps) {
  const gradientId = useId()
  const color = `var(${accent})`

  if (points.length < 2) {
    return <div className="h-full w-full" data-testid="metric-card-sparkline-empty" />
  }

  return (
    <ResponsiveContainer data-testid="metric-card-sparkline" height="100%" width="100%">
      <AreaChart data={points} margin={{ top: 2, right: 0, bottom: 0, left: 0 }}>
        <defs>
          <linearGradient id={gradientId} x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor={color} stopOpacity={0.35} />
            <stop offset="100%" stopColor={color} stopOpacity={0} />
          </linearGradient>
        </defs>
        <Area
          dataKey="v"
          fill={`url(#${gradientId})`}
          isAnimationActive={false}
          stroke={color}
          strokeWidth={1.5}
          type="monotone"
        />
      </AreaChart>
    </ResponsiveContainer>
  )
}
```

- [ ] **Step 2: Typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/dashboard/widgets/metric-card/metric-card-sparkline.tsx
git commit -m "feat(web): metric-card sparkline subcomponent"
```

---

## Task 8: Subcomponent — `MetricCardStats`

**Files:**
- Create: `apps/web/src/components/dashboard/widgets/metric-card/metric-card-stats.tsx`

Two equal rounded surfaces side by side. Each shows caption + value. Falls back to `—` for null.

- [ ] **Step 1: Implement**

```tsx
interface StatProps {
  caption: string
  value: string
}

function Stat({ caption, value }: StatProps) {
  return (
    <div className="flex-1 rounded-md border bg-muted/40 px-2.5 py-1.5">
      <p className="font-medium text-[0.625rem] text-muted-foreground uppercase tracking-[0.12em]">{caption}</p>
      <p className="font-semibold text-sm tabular-nums leading-tight" data-testid={`metric-card-stat-${caption}`}>
        {value}
      </p>
    </div>
  )
}

interface MetricCardStatsProps {
  peakCaption: string
  avgCaption: string
  peak: string
  avg: string
}

export function MetricCardStats({ peakCaption, avgCaption, peak, avg }: MetricCardStatsProps) {
  return (
    <div className="flex gap-2">
      <Stat caption={peakCaption} value={peak} />
      <Stat caption={avgCaption} value={avg} />
    </div>
  )
}
```

- [ ] **Step 2: Typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/dashboard/widgets/metric-card/metric-card-stats.tsx
git commit -m "feat(web): metric-card stats subcomponent"
```

---

## Task 9: Top-level `MetricCardWidget` (TDD)

**Files:**
- Create: `apps/web/src/components/dashboard/widgets/metric-card.tsx`
- Test: `apps/web/src/components/dashboard/widgets/metric-card.test.tsx`

Composes the four subcomponents, wires data via `useServerRecords` + `useMetricSeries`, handles loading / offline states.

- [ ] **Step 1: Write tests**

Create `apps/web/src/components/dashboard/widgets/metric-card.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { MetricCardWidget } from './metric-card'

const translations: Record<string, string> = {
  'metricCard.metric.cpu': 'CPU',
  'metricCard.metric.memory': 'Memory',
  'metricCard.metric.network': 'Network',
  'metricCard.metric.diskIo': 'Disk I/O',
  'metricCard.past1h': 'past 1h',
  'metricCard.peak': '24H PEAK',
  'metricCard.avg': '24H AVG',
  'metricCard.unknownServer': 'Unknown server'
}

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => translations[key] ?? key
  })
}))

vi.mock('@/hooks/use-api', () => ({
  useServerRecords: () => ({ data: [], isLoading: false })
}))

function makeServer(overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id: 's1',
    name: 'web-1',
    online: true,
    cpu: 42.5,
    mem_used: 4_000_000_000,
    mem_total: 8_000_000_000,
    disk_used: 0,
    disk_total: 0,
    swap_used: 0,
    swap_total: 0,
    net_in_speed: 0,
    net_out_speed: 0,
    disk_read_speed: 0,
    disk_write_speed: 0,
    ...overrides
  } as unknown as ServerMetrics
}

function wrap(node: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return <QueryClientProvider client={qc}>{node}</QueryClientProvider>
}

describe('MetricCardWidget', () => {
  it('renders the CPU value', () => {
    render(
      wrap(
        <MetricCardWidget
          config={{ metric: 'cpu', server_id: 's1' }}
          servers={[makeServer()]}
        />
      )
    )
    expect(screen.getByTestId('metric-card-value')).toHaveTextContent('42.5%')
  })

  it('shows unknown server placeholder when server_id is missing', () => {
    render(
      wrap(
        <MetricCardWidget
          config={{ metric: 'cpu', server_id: 'missing' }}
          servers={[makeServer()]}
        />
      )
    )
    expect(screen.getByText('Unknown server')).toBeInTheDocument()
  })

  it('renders dash for delta when no history is available', () => {
    render(
      wrap(
        <MetricCardWidget
          config={{ metric: 'cpu', server_id: 's1' }}
          servers={[makeServer()]}
        />
      )
    )
    expect(screen.getByTestId('metric-card-delta')).toHaveTextContent('—')
  })

  it('uses the custom label override', () => {
    render(
      wrap(
        <MetricCardWidget
          config={{ metric: 'memory', server_id: 's1', label: 'RAM Pressure' }}
          servers={[makeServer()]}
        />
      )
    )
    expect(screen.getByText('RAM Pressure')).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run tests to confirm they fail**

Run: `cd apps/web && bun run test metric-card.test`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement the widget**

Create `apps/web/src/components/dashboard/widgets/metric-card.tsx`:

```tsx
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useServerRecords } from '@/hooks/use-api'
import { useMetricSeries } from '@/hooks/use-metric-series'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { cn } from '@/lib/utils'
import type { MetricCardConfig } from '@/lib/widget-types'
import { METRIC_CARD_SPECS } from './metric-card/metric-card-config'
import { MetricCardHeader } from './metric-card/metric-card-header'
import { MetricCardSparkline } from './metric-card/metric-card-sparkline'
import { MetricCardStats } from './metric-card/metric-card-stats'
import { MetricCardValue } from './metric-card/metric-card-value'

interface MetricCardWidgetProps {
  config: MetricCardConfig
  servers: ServerMetrics[]
}

const HISTORY_HOURS = 24
const HISTORY_INTERVAL = '5m'

export function MetricCardWidget({ config, servers }: MetricCardWidgetProps) {
  const { t } = useTranslation('dashboard')
  const spec = METRIC_CARD_SPECS[config.metric]
  const server = useMemo(() => servers.find((s) => s.id === config.server_id), [servers, config.server_id])

  const { data: records } = useServerRecords(config.server_id, HISTORY_HOURS, HISTORY_INTERVAL, {
    enabled: Boolean(config.server_id) && Boolean(server)
  })

  const series = useMetricSeries({ records, server, metric: config.metric })

  if (!server) {
    return (
      <div
        className="flex h-full items-center justify-center rounded-xl border bg-card text-muted-foreground text-sm"
        data-testid="metric-card-missing-server"
      >
        {t('metricCard.unknownServer')}
      </div>
    )
  }

  const label = config.label ?? t(spec.labelKey)
  const formattedValue = spec.formatValue(series.current)
  const formattedPeak = series.peak === null ? '—' : spec.formatValue(series.peak)
  const formattedAvg = series.avg === null ? '—' : spec.formatValue(series.avg)

  const dimmed = !server.online

  return (
    <div
      className={cn(
        'flex h-full min-w-0 flex-col gap-3 overflow-hidden rounded-xl border bg-card p-3 shadow-sm',
        dimmed && 'opacity-70'
      )}
      data-metric={config.metric}
      data-testid="metric-card-widget"
    >
      <MetricCardHeader
        Icon={spec.icon}
        accent={spec.accent}
        label={label}
        serverName={server.name}
      />
      <MetricCardValue
        delta={server.online ? series.oneHourDelta : null}
        deltaTone={spec.deltaTone}
        deltaUnit={spec.deltaUnit}
        formattedValue={dimmed ? '—' : formattedValue}
        pastLabel={t('metricCard.past1h')}
      />
      <div className="min-h-0 flex-1">
        <MetricCardSparkline accent={spec.accent} points={series.points} />
      </div>
      <MetricCardStats
        avg={formattedAvg}
        avgCaption={t('metricCard.avg')}
        peak={formattedPeak}
        peakCaption={t('metricCard.peak')}
      />
    </div>
  )
}
```

- [ ] **Step 4: Run tests to confirm they pass**

Run: `cd apps/web && bun run test metric-card.test`
Expected: PASS (4/4).

- [ ] **Step 5: Typecheck + lint**

Run: `cd apps/web && bun run typecheck && bun x ultracite check src/components/dashboard/widgets/metric-card.tsx src/components/dashboard/widgets/metric-card.test.tsx src/components/dashboard/widgets/metric-card/`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/dashboard/widgets/metric-card.tsx apps/web/src/components/dashboard/widgets/metric-card.test.tsx apps/web/src/components/dashboard/widgets/metric-card/
git commit -m "feat(web): MetricCardWidget composing subcomponents"
```

---

## Task 10: Wire into widget-renderer

**Files:**
- Modify: `apps/web/src/components/dashboard/widget-renderer.tsx`

- [ ] **Step 1: Add import**

At the top of `widget-renderer.tsx` next to other widget imports:

```ts
import { MetricCardWidget } from './widgets/metric-card'
```

And add `MetricCardConfig` to the existing `widget-types` type import.

- [ ] **Step 2: Add switch case**

After the `'stat-number'` case (around line 87), add:

```tsx
case 'metric-card':
  return <MetricCardWidget config={config as unknown as MetricCardConfig} servers={servers} />
```

- [ ] **Step 3: Typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/components/dashboard/widget-renderer.tsx
git commit -m "feat(web): render metric-card widget"
```

---

## Task 11: Register icon in widget-picker

**Files:**
- Modify: `apps/web/src/components/dashboard/widget-picker.tsx`

- [ ] **Step 1: Import the Cpu icon (or reuse existing)**

In `widget-picker.tsx` add `Cpu` to the lucide import.

- [ ] **Step 2: Register icon**

In `WIDGET_ICONS`, add:

```ts
'metric-card': Cpu,
```

- [ ] **Step 3: Typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/components/dashboard/widget-picker.tsx
git commit -m "feat(web): metric-card icon in widget picker"
```

---

## Task 12: Config dialog form

**Files:**
- Modify: `apps/web/src/components/dashboard/widget-config-dialog.tsx`

The dialog uses a per-widget subform component (`StatNumberForm`, `GaugeForm`, etc.). Add `MetricCardForm` following the same pattern.

- [ ] **Step 1: Add import**

Add `MetricCardConfig`, `MetricCardMetric` to the existing `widget-types` import.

- [ ] **Step 2: Add metric option helper**

Near the other metric-options helpers (around line 70):

```ts
function useMetricCardMetrics(t: (key: string) => string): { label: string; value: MetricCardMetric }[] {
  return [
    { label: t('common.metrics.cpu'), value: 'cpu' },
    { label: t('common.metrics.memory'), value: 'memory' },
    { label: t('common.metrics.network'), value: 'network' },
    { label: t('common.metrics.diskIo'), value: 'disk_io' }
  ]
}
```

- [ ] **Step 3: Add `MetricCardForm` component**

Define a new form component near the other `*Form` definitions in the file. It mirrors `GaugeForm`: server select + metric select + optional label.

```tsx
interface MetricCardFormProps {
  config: Partial<MetricCardConfig>
  onChange: (next: WidgetConfig) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}

function MetricCardForm({ config, onChange, servers, t }: MetricCardFormProps) {
  const metrics = useMetricCardMetrics(t)
  const metric = (config.metric ?? 'cpu') as MetricCardMetric
  const serverId = config.server_id ?? ''
  const label = config.label ?? ''

  return (
    <>
      <div className="space-y-1.5">
        <Label>{t('dialogs.widgetConfig.fields.server')}</Label>
        <Select onValueChange={(value) => onChange({ ...config, metric, server_id: value, label })} value={serverId}>
          <SelectTrigger>
            <SelectValue placeholder={t('dialogs.widgetConfig.placeholders.selectServer')} />
          </SelectTrigger>
          <SelectContent>
            {servers.map((s) => (
              <SelectItem key={s.id} value={s.id}>{s.name}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="space-y-1.5">
        <Label>{t('dialogs.widgetConfig.fields.metric')}</Label>
        <Select
          onValueChange={(value) => onChange({ ...config, metric: value as MetricCardMetric, server_id: serverId, label })}
          value={metric}
        >
          <SelectTrigger><SelectValue /></SelectTrigger>
          <SelectContent>
            {metrics.map((m) => (
              <SelectItem key={m.value} value={m.value}>{m.label}</SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      <div className="space-y-1.5">
        <Label>{t('dialogs.widgetConfig.fields.label')}</Label>
        <Input
          onChange={(e) => onChange({ ...config, metric, server_id: serverId, label: e.target.value })}
          placeholder={t('dialogs.widgetConfig.placeholders.optionalLabel')}
          value={label}
        />
      </div>
    </>
  )
}
```

- [ ] **Step 4: Render in the dialog body**

After the `stat-number` form block (around line 678), add:

```tsx
{widgetType === 'metric-card' && (
  <MetricCardForm
    config={config as Partial<MetricCardConfig>}
    onChange={setConfig}
    servers={servers}
    t={t}
  />
)}
```

- [ ] **Step 5: Add `metric-card` to `i18n` common metrics if missing**

Verify `common.metrics.diskIo` and `common.metrics.network` exist in `apps/web/src/locales/{en,zh}/common.json` (or wherever `common.metrics.*` resolves). If absent, add:

```jsonc
// en/common.json
"network": "Network",
"diskIo": "Disk I/O"
// zh/common.json
"network": "网络",
"diskIo": "磁盘 I/O"
```

Also ensure `dialogs.widgetConfig.fields.metric`, `.label`, `.server`, `.placeholders.selectServer`, `.placeholders.optionalLabel` exist — they are reused by other widget forms, so they should already be present.

- [ ] **Step 6: Typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add apps/web/src/components/dashboard/widget-config-dialog.tsx apps/web/src/locales/
git commit -m "feat(web): metric-card config form"
```

---

## Task 13: i18n strings

**Files:**
- Modify: `apps/web/src/locales/en/dashboard.json`
- Modify: `apps/web/src/locales/zh/dashboard.json`

- [ ] **Step 1: Add English keys**

Inside `apps/web/src/locales/en/dashboard.json`, add at the top level:

```jsonc
"metricCard": {
  "metric": {
    "cpu": "CPU",
    "memory": "Memory",
    "network": "Network",
    "diskIo": "Disk I/O"
  },
  "past1h": "past 1h",
  "peak": "24H PEAK",
  "avg": "24H AVG",
  "unknownServer": "Unknown server"
}
```

And inside the existing `widgetPicker.types` object, add:

```jsonc
"metric-card": {
  "label": "Metric Card",
  "description": "Current value + 1h delta + 24h sparkline & stats"
}
```

- [ ] **Step 2: Add Chinese keys (mirror structure)**

In `apps/web/src/locales/zh/dashboard.json`:

```jsonc
"metricCard": {
  "metric": {
    "cpu": "CPU",
    "memory": "内存",
    "network": "网络",
    "diskIo": "磁盘 I/O"
  },
  "past1h": "过去 1 小时",
  "peak": "24H 峰值",
  "avg": "24H 均值",
  "unknownServer": "未知服务器"
}
```

And under `widgetPicker.types`:

```jsonc
"metric-card": {
  "label": "指标卡片",
  "description": "当前值 + 1h 变化 + 24h sparkline 与峰值/均值"
}
```

- [ ] **Step 3: Typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: no errors (JSON locale files do not type-check, but renderer references will).

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/locales/en/dashboard.json apps/web/src/locales/zh/dashboard.json
git commit -m "feat(web): i18n strings for metric-card widget"
```

---

## Task 14: Full repo verification

- [ ] **Step 1: Run web tests**

Run: `cd apps/web && bun run test`
Expected: all tests pass, including the new `use-metric-series` and `metric-card` suites.

- [ ] **Step 2: Run typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: no errors.

- [ ] **Step 3: Run lint**

Run: `cd apps/web && bun x ultracite check`
Expected: no errors. If fixable issues exist run `bun x ultracite fix` and re-run check.

- [ ] **Step 4: Visual verification check**

Per project preference [[feedback_visual_verification]]: this is a UI feature. Honestly report that visual verification was not performed in this environment if no browser tooling is available. Otherwise, run `make web-dev-prod` (or `bun run dev` from `apps/web`), open the dashboard editor, add four `metric-card` widgets (one per metric) bound to a test server, and confirm:

- Value updates every second.
- Sparkline scrolls without flicker.
- 24h peak/avg stay stable as live ticks arrive.
- Delta sign and color match (rising CPU = red, falling = green; network = neutral).
- Switching metric in the config dialog updates the icon, accent, and formatting.

- [ ] **Step 5: Commit any lint-fix-only diffs**

If `ultracite fix` changed anything, commit it:

```bash
git add -A
git commit -m "chore(web): lint fixes for metric-card"
```

(Skip this step if there are no changes.)

---

## Notes on style

- Default to no comments. The subcomponent split, file names, and identifier names should carry the WHAT.
- Follow [[feedback_no_claude_attribution]]: no "Generated with Claude" or co-author lines on any commit.
- Honor [[feedback_git_push]]: commit locally only — do NOT push.
- Don't widen API surfaces beyond what the spec requires. The hook returns exactly the four derived values; the widget exposes one config shape.
