# Network Quality Dashboard Widgets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add three built-in dashboard widgets — `network-latency`, `network-quality`, `network-overview` — that surface the existing network-probe (tri-network ping) data.

**Architecture:** Frontend-only. Three independent built-in widgets follow the existing 13-widget convention (one widget = one visual form). A shared `useNetworkChartRecords` hook is extracted from the network detail page so the latency widget and the detail page reuse the same records+realtime merge logic. No backend, protocol, or migration changes.

**Tech Stack:** React 19, TanStack Query, Recharts, react-i18next, Vitest + Testing Library, Biome/Ultracite.

---

## File Structure

**Create:**
- `apps/web/src/lib/network-chart-records.ts` — pure merge/dedupe function `mergeNetworkChartRecords`
- `apps/web/src/lib/network-chart-records.test.ts` — unit tests for the pure function
- `apps/web/src/hooks/use-network-chart-records.ts` — hook wrapping the pure function + data hooks
- `apps/web/src/components/dashboard/widgets/network-quality.tsx` — single-server summary card
- `apps/web/src/components/dashboard/widgets/network-quality.test.tsx`
- `apps/web/src/components/dashboard/widgets/network-latency-widget.tsx` — single-server latency chart
- `apps/web/src/components/dashboard/widgets/network-latency-widget.test.tsx`
- `apps/web/src/components/dashboard/widgets/network-overview-widget.tsx` — multi-server table
- `apps/web/src/components/dashboard/widgets/network-overview-widget.test.tsx`

**Modify:**
- `apps/web/src/lib/widget-types.ts` — 3 `WIDGET_TYPES` entries, 3 config interfaces, union extension
- `apps/web/src/components/dashboard/widget-render-dependencies.ts` — 3 scope cases
- `apps/web/src/components/dashboard/widget-renderer.tsx` — 3 imports + 3 switch cases
- `apps/web/src/components/dashboard/widget-config-dialog.tsx` — 3 forms + dispatch
- `apps/web/src/components/dashboard/widget-config-dialog.test.tsx` — 3 dispatch test cases
- `apps/web/src/components/dashboard/widget-picker.tsx` — 3 `WIDGET_ICONS` entries
- `apps/web/src/routes/_authed/network/$serverId.tsx` — refactor inline merge to use the new hook
- `apps/web/src/locales/en/dashboard.json` — picker + widget strings
- `apps/web/src/locales/zh/dashboard.json` — picker + widget strings

---

## Task 1: Extract pure network-chart-records merge function

**Files:**
- Create: `apps/web/src/lib/network-chart-records.ts`
- Test: `apps/web/src/lib/network-chart-records.test.ts`

This extracts the realtime/seed merge logic currently inlined in `$serverId.tsx:728-767` into a pure, testable function.

- [ ] **Step 1: Write the failing test**

Create `apps/web/src/lib/network-chart-records.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { mergeNetworkChartRecords } from './network-chart-records'
import type { NetworkProbeRecord, NetworkProbeResultData } from './network-types'

const seed: NetworkProbeRecord[] = [
  {
    id: 1,
    server_id: 'srv-1',
    target_id: 't-1',
    timestamp: '2026-05-29T10:00:00.000Z',
    avg_latency: 20,
    min_latency: 18,
    max_latency: 25,
    packet_loss: 0,
    packet_sent: 10,
    packet_received: 10
  }
]

const realtime: Record<string, NetworkProbeResultData[]> = {
  't-1': [
    {
      target_id: 't-1',
      timestamp: '2026-05-29T10:01:00.000Z',
      avg_latency: 22,
      min_latency: 19,
      max_latency: 28,
      packet_loss: 0,
      packet_sent: 10,
      packet_received: 10
    }
  ]
}

describe('mergeNetworkChartRecords', () => {
  it('returns historical records unchanged when not realtime', () => {
    const result = mergeNetworkChartRecords({ isRealtime: false, historical: seed, seed: [], realtime: {}, serverId: 'srv-1' })
    expect(result).toEqual(seed)
  })

  it('flattens realtime map and merges with seed in realtime mode', () => {
    const result = mergeNetworkChartRecords({ isRealtime: true, historical: [], seed, realtime, serverId: 'srv-1' })
    expect(result).toHaveLength(2)
    expect(result.map((r) => r.timestamp)).toEqual([
      '2026-05-29T10:00:00.000Z',
      '2026-05-29T10:01:00.000Z'
    ])
  })

  it('dedupes by target_id + timestamp keeping the latest entry', () => {
    const dupRealtime: Record<string, NetworkProbeResultData[]> = {
      't-1': [
        { target_id: 't-1', timestamp: '2026-05-29T10:00:00.000Z', avg_latency: 99, min_latency: 99, max_latency: 99, packet_loss: 0, packet_sent: 10, packet_received: 10 }
      ]
    }
    const result = mergeNetworkChartRecords({ isRealtime: true, historical: [], seed, realtime: dupRealtime, serverId: 'srv-1' })
    expect(result).toHaveLength(1)
    expect(result[0].avg_latency).toBe(99)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/web && bun run test -- network-chart-records`
Expected: FAIL — `mergeNetworkChartRecords` is not defined / module not found.

- [ ] **Step 3: Write minimal implementation**

Create `apps/web/src/lib/network-chart-records.ts`:

```ts
import type { NetworkProbeRecord, NetworkProbeResultData } from './network-types'

interface MergeArgs {
  historical: NetworkProbeRecord[]
  isRealtime: boolean
  realtime: Record<string, NetworkProbeResultData[]>
  seed: NetworkProbeRecord[]
  serverId: string
}

// Combine the 1h "seed" snapshot with live realtime points in realtime mode, or
// return historical records as-is otherwise. Realtime points override seed points
// at the same (target_id, timestamp) bucket. Mirrors the logic previously inlined
// in the network detail page.
export function mergeNetworkChartRecords({ historical, isRealtime, realtime, seed, serverId }: MergeArgs): NetworkProbeRecord[] {
  if (!isRealtime) {
    return historical
  }

  const realtimeFlat: NetworkProbeRecord[] = []
  for (const [targetId, points] of Object.entries(realtime)) {
    for (const point of points) {
      realtimeFlat.push({
        id: 0,
        server_id: serverId,
        target_id: targetId,
        timestamp: point.timestamp,
        avg_latency: point.avg_latency,
        min_latency: point.min_latency,
        max_latency: point.max_latency,
        packet_loss: point.packet_loss,
        packet_sent: point.packet_sent,
        packet_received: point.packet_received
      })
    }
  }

  const merged = [...seed, ...realtimeFlat]
  const seen = new Set<string>()
  const deduped: NetworkProbeRecord[] = []
  for (let i = merged.length - 1; i >= 0; i--) {
    const r = merged[i]
    const key = `${r.target_id}:${r.timestamp}`
    if (!seen.has(key)) {
      seen.add(key)
      deduped.push(r)
    }
  }
  deduped.reverse()
  return deduped
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd apps/web && bun run test -- network-chart-records`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/lib/network-chart-records.ts apps/web/src/lib/network-chart-records.test.ts
git commit -m "feat(web): extract pure network chart records merge function"
```

---

## Task 2: Create useNetworkChartRecords hook and refactor detail page

**Files:**
- Create: `apps/web/src/hooks/use-network-chart-records.ts`
- Modify: `apps/web/src/routes/_authed/network/$serverId.tsx:728-767`

- [ ] **Step 1: Write the hook**

Create `apps/web/src/hooks/use-network-chart-records.ts`:

```ts
import { useMemo } from 'react'
import { useNetworkRecords } from '@/hooks/use-network-api'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import { mergeNetworkChartRecords } from '@/lib/network-chart-records'
import type { NetworkProbeRecord } from '@/lib/network-types'

// `hours === 0` means realtime. Returns a record series ready for LatencyChart,
// combining historical OR (seed + live) data depending on the range.
export function useNetworkChartRecords(serverId: string, hours: number): NetworkProbeRecord[] {
  const isRealtime = hours === 0
  const { data: historical } = useNetworkRecords(serverId, hours, { enabled: !isRealtime && serverId.length > 0 })
  const { data: seed } = useNetworkRecords(serverId, 1, { enabled: isRealtime && serverId.length > 0 })
  const { data: realtime } = useNetworkRealtime(serverId)

  return useMemo(
    () =>
      mergeNetworkChartRecords({
        historical: historical ?? [],
        isRealtime,
        realtime,
        seed: seed ?? [],
        serverId
      }),
    [historical, isRealtime, realtime, seed, serverId]
  )
}
```

- [ ] **Step 2: Refactor the detail page to use the hook**

In `apps/web/src/routes/_authed/network/$serverId.tsx`, replace the `historicalRecords` / `seedRecords` / `realtimeData` declarations (lines ~649-653) and the `records` useMemo (lines ~728-767) with the hook. Keep `isRealtime` and `hours` as-is.

Remove these lines:

```tsx
  const { data: historicalRecords } = useNetworkRecords(serverId, hours, { enabled: !isRealtime })
  // Fetch last 10 min of data as seed for realtime chart (immediate data on first load)
  const { data: seedRecords } = useNetworkRecords(serverId, 1, { enabled: isRealtime })
```

and

```tsx
  const { data: realtimeData } = useNetworkRealtime(serverId)
```

(Leave the `useNetworkRecords` / `useNetworkRealtime` imports only if still used elsewhere; they are not after this change — remove them from the import block in lines 32-44 if unused. `useNetworkRecords` is no longer used; `useNetworkRealtime` is no longer used.)

Replace the entire `const records: NetworkProbeRecord[] = useMemo(() => { ... }, [...])` block (lines ~728-767) with:

```tsx
  const records = useNetworkChartRecords(serverId, isRealtime ? 0 : hours)
```

Add the import near the other hook imports:

```tsx
import { useNetworkChartRecords } from '@/hooks/use-network-chart-records'
```

- [ ] **Step 3: Run the existing detail-page test + typecheck**

Run: `cd apps/web && bun run test -- network/\\$server-id && bun run typecheck`
Expected: PASS, no type errors. (The detail-page test `routes/_authed/network/$server-id.test.tsx` exercises the chart; behavior is unchanged.)

- [ ] **Step 4: Lint**

Run: `cd apps/web && bun x ultracite check src/hooks/use-network-chart-records.ts src/routes/_authed/network/\\$serverId.tsx`
Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/hooks/use-network-chart-records.ts apps/web/src/routes/_authed/network/\$serverId.tsx
git commit -m "refactor(web): reuse shared network chart records hook in detail page"
```

---

## Task 3: Register widget types, scopes, and picker icons

**Files:**
- Modify: `apps/web/src/lib/widget-types.ts`
- Modify: `apps/web/src/components/dashboard/widget-render-dependencies.ts:44-70`
- Modify: `apps/web/src/components/dashboard/widget-picker.tsx:35-50`

- [ ] **Step 1: Add WIDGET_TYPES entries**

In `apps/web/src/lib/widget-types.ts`, inside the `WIDGET_TYPES` array (before the closing `] as const satisfies ...`), add after the `uptime-timeline` entry:

```ts
  ,
  {
    id: 'network-latency',
    label: 'Network Latency',
    category: 'Charts',
    defaultW: 6,
    defaultH: 4,
    minW: 4,
    minH: 3,
    maxW: 12,
    maxH: 8,
    sizing: { kind: 'free' }
  },
  {
    id: 'network-quality',
    label: 'Network Quality',
    category: 'Real-time',
    defaultW: 4,
    defaultH: 4,
    minW: 3,
    minH: 3,
    maxW: 8,
    maxH: 8,
    sizing: { kind: 'free' }
  },
  {
    id: 'network-overview',
    label: 'Network Overview',
    category: 'Status',
    defaultW: 8,
    defaultH: 5,
    minW: 4,
    minH: 3,
    maxW: 12,
    maxH: 8,
    sizing: { kind: 'free' }
  }
```

(Note: the existing array's last element `uptime-timeline` has no trailing comma; the leading `,` above attaches the new entries. Verify the final array is syntactically valid.)

- [ ] **Step 2: Add config interfaces and extend the union**

In `apps/web/src/lib/widget-types.ts`, after the `UptimeTimelineConfig` interface, add:

```ts
export interface NetworkLatencyConfig {
  hours?: number // 0 means realtime
  server_id: string
}

export interface NetworkQualityConfig {
  server_id: string
}

export interface NetworkOverviewConfig {
  server_ids?: string[]
}
```

Then extend the `WidgetConfig` union — change:

```ts
  | UptimeTimelineConfig
```

to:

```ts
  | UptimeTimelineConfig
  | NetworkLatencyConfig
  | NetworkQualityConfig
  | NetworkOverviewConfig
```

- [ ] **Step 3: Add render-dependency scopes**

In `apps/web/src/components/dashboard/widget-render-dependencies.ts`, inside `getWidgetServerScope`'s switch, add cases before the `default`:

```ts
    case 'network-latency':
    case 'network-quality':
      return singleServerScope(config.server_id, 'name')
    case 'network-overview':
      return selectedServerScope(config.server_ids, 'name')
```

- [ ] **Step 4: Add picker icons**

In `apps/web/src/components/dashboard/widget-picker.tsx`, add to the `WIDGET_ICONS` record (the `Network`, `Gauge`, `Globe` icons are already imported):

```ts
  'network-latency': LineChart,
  'network-quality': Gauge,
  'network-overview': Network
```

(Add a comma after the existing `'uptime-timeline': Activity` entry first.)

- [ ] **Step 5: Typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: PASS. (The renderer's switch is not yet exhaustive-checked at compile time — it has a `default`, so no error for the missing cases yet.)

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/lib/widget-types.ts apps/web/src/components/dashboard/widget-render-dependencies.ts apps/web/src/components/dashboard/widget-picker.tsx
git commit -m "feat(web): register network quality widget types and picker icons"
```

---

## Task 4: Network Quality widget (single-server summary card)

**Files:**
- Create: `apps/web/src/components/dashboard/widgets/network-quality.tsx`
- Test: `apps/web/src/components/dashboard/widgets/network-quality.test.tsx`
- Modify: `apps/web/src/components/dashboard/widget-renderer.tsx`

- [ ] **Step 1: Write the failing test**

Create `apps/web/src/components/dashboard/widgets/network-quality.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { NetworkServerSummary } from '@/lib/network-types'
import { NetworkQualityWidget } from './network-quality'

const summaryMock = vi.fn<() => { data: NetworkServerSummary | undefined; isLoading: boolean }>()

vi.mock('@/hooks/use-network-api', () => ({
  useNetworkServerSummary: () => summaryMock()
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (_k: string, fallback?: string) => fallback ?? _k })
}))

vi.mock('@/components/ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children?: ReactNode }) => <div>{children}</div>
}))

const baseSummary: NetworkServerSummary = {
  server_id: 'srv-1',
  server_name: 'Server 1',
  online: true,
  last_probe_at: '2026-05-29T10:00:00.000Z',
  anomaly_count: 0,
  latency_sparkline: [],
  loss_sparkline: [],
  targets: [
    { target_id: 't-1', target_name: 'China Telecom', provider: 'ct', avg_latency: 23.1, min_latency: 20, max_latency: 30, packet_loss: 0, availability: 100 },
    { target_id: 't-2', target_name: 'International', provider: 'international', avg_latency: 142.3, min_latency: 130, max_latency: 160, packet_loss: 0.015, availability: 98 }
  ]
}

describe('NetworkQualityWidget', () => {
  it('renders each target with latency and packet loss', () => {
    summaryMock.mockReturnValue({ data: baseSummary, isLoading: false })
    render(<NetworkQualityWidget config={{ server_id: 'srv-1' }} servers={[]} />)
    expect(screen.getByText('China Telecom')).toBeInTheDocument()
    expect(screen.getByText('International')).toBeInTheDocument()
    expect(screen.getByText('23.1 ms')).toBeInTheDocument()
  })

  it('renders empty state when there are no targets', () => {
    summaryMock.mockReturnValue({ data: { ...baseSummary, targets: [] }, isLoading: false })
    render(<NetworkQualityWidget config={{ server_id: 'srv-1' }} servers={[]} />)
    expect(screen.getByText(/no network probe data/i)).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/web && bun run test -- network-quality`
Expected: FAIL — module `./network-quality` not found.

- [ ] **Step 3: Write the widget**

Create `apps/web/src/components/dashboard/widgets/network-quality.tsx`:

```tsx
import { useTranslation } from 'react-i18next'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { useNetworkServerSummary } from '@/hooks/use-network-api'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { CHART_COLORS } from '@/lib/chart-colors'
import { formatLatency, formatPacketLoss, getLossTextClassName } from '@/lib/network-types'
import type { NetworkQualityConfig } from '@/lib/widget-types'

interface NetworkQualityWidgetProps {
  config: NetworkQualityConfig
  servers: ServerMetrics[]
}

export function NetworkQualityWidget({ config }: NetworkQualityWidgetProps) {
  const { t } = useTranslation('dashboard')
  const serverId = config.server_id ?? ''
  const { data: summary, isLoading } = useNetworkServerSummary(serverId)

  if (isLoading) {
    return (
      <div className="flex h-full flex-col gap-2 rounded-lg border bg-card p-4">
        <Skeleton className="h-4 w-32" />
        <Skeleton className="flex-1" />
      </div>
    )
  }

  const targets = summary?.targets ?? []

  if (targets.length === 0) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-1 font-semibold text-sm">{t('widgets.networkQuality.title', 'Network Quality')}</h3>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-sm">
          {t('widgets.networkQuality.empty.noData', 'No network probe data available')}
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <div className="mb-2">
        <h3 className="font-semibold text-sm">{t('widgets.networkQuality.title', 'Network Quality')}</h3>
        <p className="text-muted-foreground text-xs">{summary?.server_name}</p>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <ul className="space-y-1.5 pr-2">
          {targets.map((target, i) => (
            <li className="flex items-center gap-3 rounded-md border px-3 py-2" key={target.target_id}>
              <span
                aria-hidden="true"
                className="size-2.5 shrink-0 rounded-full"
                style={{ backgroundColor: CHART_COLORS[i % CHART_COLORS.length] }}
              />
              <span className="min-w-0 flex-1 truncate font-medium text-sm">{target.target_name}</span>
              <span className="font-mono text-sm tabular-nums">{formatLatency(target.avg_latency)}</span>
              <span className={`font-mono text-xs tabular-nums ${getLossTextClassName(target.packet_loss)}`}>
                {formatPacketLoss(target.packet_loss)}
              </span>
            </li>
          ))}
        </ul>
      </ScrollArea>
    </div>
  )
}
```

- [ ] **Step 4: Wire into the renderer**

In `apps/web/src/components/dashboard/widget-renderer.tsx`:

Add the import (alphabetically near the other widget imports):

```tsx
import { NetworkQualityWidget } from './widgets/network-quality'
```

Add the config type to the type-only import block from `@/lib/widget-types`:

```tsx
  NetworkQualityConfig,
```

Add the switch case (before `default`):

```tsx
    case 'network-quality':
      return <NetworkQualityWidget config={config as unknown as NetworkQualityConfig} servers={servers} />
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd apps/web && bun run test -- network-quality`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/dashboard/widgets/network-quality.tsx apps/web/src/components/dashboard/widgets/network-quality.test.tsx apps/web/src/components/dashboard/widget-renderer.tsx
git commit -m "feat(web): add network quality summary widget"
```

---

## Task 5: Network Latency widget (single-server chart)

**Files:**
- Create: `apps/web/src/components/dashboard/widgets/network-latency-widget.tsx`
- Test: `apps/web/src/components/dashboard/widgets/network-latency-widget.test.tsx`
- Modify: `apps/web/src/components/dashboard/widget-renderer.tsx`

- [ ] **Step 1: Write the failing test**

Create `apps/web/src/components/dashboard/widgets/network-latency-widget.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { NetworkProbeRecord, NetworkServerSummary } from '@/lib/network-types'
import { NetworkLatencyWidget } from './network-latency-widget'

const recordsMock = vi.fn<() => NetworkProbeRecord[]>()
const summaryMock = vi.fn<() => { data: NetworkServerSummary | undefined }>()

vi.mock('@/hooks/use-network-chart-records', () => ({
  useNetworkChartRecords: () => recordsMock()
}))

vi.mock('@/hooks/use-network-api', () => ({
  useNetworkServerSummary: () => summaryMock()
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (_k: string, fallback?: string) => fallback ?? _k })
}))

// LatencyChart is exercised in its own context; stub it so this test focuses on the widget shell.
vi.mock('@/components/network/latency-chart', () => ({
  LatencyChart: ({ records }: { records: NetworkProbeRecord[] }) => (
    <div data-testid="latency-chart">{records.length} points</div>
  )
}))

const summary: NetworkServerSummary = {
  server_id: 'srv-1',
  server_name: 'Server 1',
  online: true,
  last_probe_at: null,
  anomaly_count: 0,
  latency_sparkline: [],
  loss_sparkline: [],
  targets: [{ target_id: 't-1', target_name: 'China Telecom', provider: 'ct', avg_latency: 20, min_latency: 18, max_latency: 25, packet_loss: 0, availability: 100 }]
}

describe('NetworkLatencyWidget', () => {
  it('renders the latency chart with merged records', () => {
    summaryMock.mockReturnValue({ data: summary })
    recordsMock.mockReturnValue([
      { id: 1, server_id: 'srv-1', target_id: 't-1', timestamp: '2026-05-29T10:00:00.000Z', avg_latency: 20, min_latency: 18, max_latency: 25, packet_loss: 0, packet_sent: 10, packet_received: 10 }
    ])
    render(<NetworkLatencyWidget config={{ server_id: 'srv-1', hours: 24 }} servers={[]} />)
    expect(screen.getByTestId('latency-chart')).toHaveTextContent('1 points')
  })

  it('renders empty state when there are no records', () => {
    summaryMock.mockReturnValue({ data: summary })
    recordsMock.mockReturnValue([])
    render(<NetworkLatencyWidget config={{ server_id: 'srv-1', hours: 24 }} servers={[]} />)
    expect(screen.getByText(/no network probe data/i)).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/web && bun run test -- network-latency-widget`
Expected: FAIL — module `./network-latency-widget` not found.

- [ ] **Step 3: Write the widget**

Create `apps/web/src/components/dashboard/widgets/network-latency-widget.tsx`:

```tsx
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { LatencyChart } from '@/components/network/latency-chart'
import { useNetworkServerSummary } from '@/hooks/use-network-api'
import { useNetworkChartRecords } from '@/hooks/use-network-chart-records'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { CHART_COLORS } from '@/lib/chart-colors'
import type { NetworkLatencyConfig } from '@/lib/widget-types'

interface NetworkLatencyWidgetProps {
  config: NetworkLatencyConfig
  servers: ServerMetrics[]
}

export function NetworkLatencyWidget({ config }: NetworkLatencyWidgetProps) {
  const { t } = useTranslation('dashboard')
  const serverId = config.server_id ?? ''
  const hours = config.hours ?? 24
  const isRealtime = hours === 0

  const records = useNetworkChartRecords(serverId, hours)
  const { data: summary } = useNetworkServerSummary(serverId)

  const chartTargets = useMemo(
    () =>
      (summary?.targets ?? []).map((target, i) => ({
        id: target.target_id,
        name: target.target_name,
        color: CHART_COLORS[i % CHART_COLORS.length],
        visible: true
      })),
    [summary]
  )

  if (records.length === 0) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-1 font-semibold text-sm">{t('widgets.networkLatency.title', 'Network Latency')}</h3>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-sm">
          {t('widgets.networkLatency.empty.noData', 'No network probe data available')}
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <div className="mb-2">
        <h3 className="font-semibold text-sm">{t('widgets.networkLatency.title', 'Network Latency')}</h3>
        <p className="text-muted-foreground text-xs">{summary?.server_name}</p>
      </div>
      <div className="min-h-0 flex-1">
        <LatencyChart hours={isRealtime ? 1 : hours} isRealtime={isRealtime} records={records} targets={chartTargets} />
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Wire into the renderer**

In `apps/web/src/components/dashboard/widget-renderer.tsx`:

Add the import:

```tsx
import { NetworkLatencyWidget } from './widgets/network-latency-widget'
```

Add to the type-only import block from `@/lib/widget-types`:

```tsx
  NetworkLatencyConfig,
```

Add the switch case (before `default`):

```tsx
    case 'network-latency':
      return <NetworkLatencyWidget config={config as unknown as NetworkLatencyConfig} servers={servers} />
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd apps/web && bun run test -- network-latency-widget`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/dashboard/widgets/network-latency-widget.tsx apps/web/src/components/dashboard/widgets/network-latency-widget.test.tsx apps/web/src/components/dashboard/widget-renderer.tsx
git commit -m "feat(web): add network latency chart widget"
```

---

## Task 6: Network Overview widget (multi-server table)

**Files:**
- Create: `apps/web/src/components/dashboard/widgets/network-overview-widget.tsx`
- Test: `apps/web/src/components/dashboard/widgets/network-overview-widget.test.tsx`
- Modify: `apps/web/src/components/dashboard/widget-renderer.tsx`

- [ ] **Step 1: Write the failing test**

Create `apps/web/src/components/dashboard/widgets/network-overview-widget.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { NetworkServerSummary } from '@/lib/network-types'
import { NetworkOverviewWidget } from './network-overview-widget'

const overviewMock = vi.fn<() => { data: NetworkServerSummary[]; isLoading: boolean }>()

vi.mock('@/hooks/use-network-api', () => ({
  useNetworkOverview: () => overviewMock()
}))

vi.mock('react-i18next', () => ({
  useTranslation: () => ({ t: (_k: string, fallback?: string) => fallback ?? _k })
}))

vi.mock('@/components/ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children?: ReactNode }) => <div>{children}</div>
}))

// Render TanStack Router Link as a plain anchor so the widget can be tested in isolation.
vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to, params }: { children?: ReactNode; to?: string; params?: Record<string, string> }) => (
    <a href={`${to}/${params?.serverId ?? ''}`}>{children}</a>
  )
}))

const summaries: NetworkServerSummary[] = [
  { server_id: 'srv-1', server_name: 'Server 1', online: true, last_probe_at: null, anomaly_count: 2, latency_sparkline: [10, 12], loss_sparkline: [0, 0], targets: [{ target_id: 't-1', target_name: 'CT', provider: 'ct', avg_latency: 20, min_latency: 18, max_latency: 25, packet_loss: 0.012, availability: 99 }] },
  { server_id: 'srv-2', server_name: 'Server 2', online: false, last_probe_at: null, anomaly_count: 0, latency_sparkline: [], loss_sparkline: [], targets: [] }
]

describe('NetworkOverviewWidget', () => {
  it('renders one row per server with a link to the network detail page', () => {
    overviewMock.mockReturnValue({ data: summaries, isLoading: false })
    render(<NetworkOverviewWidget config={{}} servers={[]} />)
    expect(screen.getByText('Server 1')).toBeInTheDocument()
    expect(screen.getByText('Server 2')).toBeInTheDocument()
    const link = screen.getByText('Server 1').closest('a')
    expect(link).toHaveAttribute('href', '/network/srv-1')
  })

  it('filters to configured server_ids', () => {
    overviewMock.mockReturnValue({ data: summaries, isLoading: false })
    render(<NetworkOverviewWidget config={{ server_ids: ['srv-2'] }} servers={[]} />)
    expect(screen.queryByText('Server 1')).not.toBeInTheDocument()
    expect(screen.getByText('Server 2')).toBeInTheDocument()
  })

  it('renders empty state when there is no data', () => {
    overviewMock.mockReturnValue({ data: [], isLoading: false })
    render(<NetworkOverviewWidget config={{}} servers={[]} />)
    expect(screen.getByText(/no network probe data/i)).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd apps/web && bun run test -- network-overview-widget`
Expected: FAIL — module `./network-overview-widget` not found.

- [ ] **Step 3: Write the widget**

Create `apps/web/src/components/dashboard/widgets/network-overview-widget.tsx`:

```tsx
import { Link } from '@tanstack/react-router'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { ScrollArea } from '@/components/ui/scroll-area'
import { Skeleton } from '@/components/ui/skeleton'
import { useNetworkOverview } from '@/hooks/use-network-api'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { formatLatency, type NetworkServerSummary } from '@/lib/network-types'
import type { NetworkOverviewConfig } from '@/lib/widget-types'

interface NetworkOverviewWidgetProps {
  config: NetworkOverviewConfig
  servers: ServerMetrics[]
}

// Average latency across a server's targets, ignoring targets with no reading.
function avgLatency(summary: NetworkServerSummary): number | null {
  const values = summary.targets.map((target) => target.avg_latency).filter((v): v is number => v != null)
  if (values.length === 0) {
    return null
  }
  return values.reduce((a, b) => a + b, 0) / values.length
}

export function NetworkOverviewWidget({ config }: NetworkOverviewWidgetProps) {
  const { t } = useTranslation('dashboard')
  const { data: overview = [], isLoading } = useNetworkOverview()

  const rows = useMemo(() => {
    const ids = config.server_ids
    if (!ids || ids.length === 0) {
      return overview
    }
    const allow = new Set(ids)
    return overview.filter((summary) => allow.has(summary.server_id))
  }, [overview, config.server_ids])

  if (isLoading) {
    return (
      <div className="flex h-full flex-col gap-2 rounded-lg border bg-card p-4">
        <Skeleton className="h-4 w-32" />
        <Skeleton className="flex-1" />
      </div>
    )
  }

  if (rows.length === 0) {
    return (
      <div className="flex h-full flex-col rounded-lg border bg-card p-4">
        <h3 className="mb-1 font-semibold text-sm">{t('widgets.networkOverview.title', 'Network Overview')}</h3>
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-sm">
          {t('widgets.networkOverview.empty.noData', 'No network probe data available')}
        </div>
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col rounded-lg border bg-card p-4">
      <h3 className="mb-2 font-semibold text-sm">{t('widgets.networkOverview.title', 'Network Overview')}</h3>
      <ScrollArea className="min-h-0 flex-1">
        <ul className="space-y-1 pr-2">
          {rows.map((summary) => {
            const latency = avgLatency(summary)
            return (
              <li key={summary.server_id}>
                <Link
                  className="flex items-center gap-3 rounded-md border px-3 py-2 transition-colors hover:bg-muted/50"
                  params={{ serverId: summary.server_id }}
                  to="/network/$serverId"
                >
                  <span
                    aria-hidden="true"
                    className={`size-2 shrink-0 rounded-full ${summary.online ? 'bg-emerald-500' : 'bg-muted-foreground/40'}`}
                  />
                  <span className="min-w-0 flex-1 truncate font-medium text-sm">{summary.server_name}</span>
                  <span className="font-mono text-sm tabular-nums">{formatLatency(latency)}</span>
                  {summary.anomaly_count > 0 && (
                    <span className="rounded-full bg-amber-100 px-2 py-0.5 text-amber-700 text-xs dark:bg-amber-900/30 dark:text-amber-400">
                      {summary.anomaly_count}
                    </span>
                  )}
                </Link>
              </li>
            )
          })}
        </ul>
      </ScrollArea>
    </div>
  )
}
```

- [ ] **Step 4: Wire into the renderer**

In `apps/web/src/components/dashboard/widget-renderer.tsx`:

Add the import:

```tsx
import { NetworkOverviewWidget } from './widgets/network-overview-widget'
```

Add to the type-only import block from `@/lib/widget-types`:

```tsx
  NetworkOverviewConfig,
```

Add the switch case (before `default`):

```tsx
    case 'network-overview':
      return <NetworkOverviewWidget config={config as unknown as NetworkOverviewConfig} servers={servers} />
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd apps/web && bun run test -- network-overview-widget`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/dashboard/widgets/network-overview-widget.tsx apps/web/src/components/dashboard/widgets/network-overview-widget.test.tsx apps/web/src/components/dashboard/widget-renderer.tsx
git commit -m "feat(web): add network overview widget"
```

---

## Task 7: Config dialog forms for the three widgets

**Files:**
- Modify: `apps/web/src/components/dashboard/widget-config-dialog.tsx`
- Modify: `apps/web/src/components/dashboard/widget-config-dialog.test.tsx`

The latency form needs a range select that includes a **Realtime** option (value `'0'`). Quality form picks a server only. Overview form is a server multi-select.

- [ ] **Step 1: Write the failing tests**

In `apps/web/src/components/dashboard/widget-config-dialog.test.tsx`, add to the `translations` map (inside the existing object):

```ts
  'widgets.common.placeholders.selectServer': 'Select server',
  'widgets.common.empty.noServers': 'No servers',
  'common.timeRange.realtime': 'Realtime',
  'common.timeRange.6hours': '6 hours',
  'common.timeRange.7days': '7 days',
```

Then add these test cases inside the top-level `describe('WidgetConfigDialog', ...)` block:

```tsx
  it('renders server + range (with realtime) for network-latency widget', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open
        servers={mockServers as never}
        widgetType="network-latency"
      />
    )

    expect(screen.getByText('Server')).toBeInTheDocument()
    expect(screen.getByText('Time Range')).toBeInTheDocument()
    expect(screen.getByText('Realtime')).toBeInTheDocument()
    expect(screen.getByText('Server 1')).toBeInTheDocument()
  })

  it('renders a server select for network-quality widget', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open
        servers={mockServers as never}
        widgetType="network-quality"
      />
    )

    expect(screen.getByText('Server')).toBeInTheDocument()
    expect(screen.getByText('Server 1')).toBeInTheDocument()
  })

  it('renders a server multi-select for network-overview widget', () => {
    render(
      <WidgetConfigDialog
        onOpenChange={noop}
        onSubmit={noop}
        open
        servers={mockServers as never}
        widgetType="network-overview"
      />
    )

    expect(screen.getByText('Servers')).toBeInTheDocument()
    expect(screen.getByText('Server 1')).toBeInTheDocument()
  })
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/web && bun run test -- widget-config-dialog`
Expected: FAIL — the new widget types fall through to no form; `'Realtime'` / `'Servers'` not found for these types.

- [ ] **Step 3: Add the forms and dispatch**

In `apps/web/src/components/dashboard/widget-config-dialog.tsx`:

Add to the type-only import from `@/lib/widget-types`:

```ts
  NetworkLatencyConfig,
  NetworkOverviewConfig,
  NetworkQualityConfig,
```

Add a network range options hook near `useRangeOptions` (includes realtime as `'0'`):

```ts
function useNetworkRangeOptions(t: (key: string) => string): { label: string; value: string }[] {
  return [
    { label: t('common.timeRange.realtime'), value: '0' },
    { label: t('common.timeRange.1hour'), value: '1' },
    { label: t('common.timeRange.6hours'), value: '6' },
    { label: t('common.timeRange.24hours'), value: '24' },
    { label: t('common.timeRange.7days'), value: '168' }
  ]
}
```

Add the three form components after `UptimeTimelineForm`:

```tsx
function NetworkLatencyForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<NetworkLatencyConfig>
  onChange: (c: Partial<NetworkLatencyConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  const NETWORK_RANGE_OPTIONS = useNetworkRangeOptions(t)
  return (
    <>
      <ServerSelect
        label={t('widgets.common.labels.server')}
        onChange={(v) => onChange({ ...config, server_id: v })}
        placeholder={t('widgets.common.placeholders.selectServer')}
        servers={servers}
        value={config.server_id ?? ''}
      />
      <div className="space-y-1.5">
        <Label>{t('widgets.common.labels.timeRange')}</Label>
        <Select
          items={NETWORK_RANGE_OPTIONS}
          onValueChange={(v) => v !== null && onChange({ ...config, hours: Number(v) })}
          value={String(config.hours ?? '24')}
        >
          <SelectTrigger className="w-full">
            <SelectValue placeholder={t('widgets.common.placeholders.selectRange')} />
          </SelectTrigger>
          <SelectContent>
            {NETWORK_RANGE_OPTIONS.map((r) => (
              <SelectItem key={r.value} value={r.value}>
                {r.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    </>
  )
}

function NetworkQualityForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<NetworkQualityConfig>
  onChange: (c: Partial<NetworkQualityConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  return (
    <ServerSelect
      label={t('widgets.common.labels.server')}
      onChange={(v) => onChange({ ...config, server_id: v })}
      placeholder={t('widgets.common.placeholders.selectServer')}
      servers={servers}
      value={config.server_id ?? ''}
    />
  )
}

function NetworkOverviewForm({
  config,
  servers,
  onChange,
  t
}: {
  config: Partial<NetworkOverviewConfig>
  onChange: (c: Partial<NetworkOverviewConfig>) => void
  servers: ServerMetrics[]
  t: (key: string) => string
}) {
  return (
    <ServerMultiSelect
      emptyMessage={t('widgets.common.empty.noServers')}
      label={t('widgets.common.labels.servers')}
      onChange={(ids) => onChange({ ...config, server_ids: ids })}
      selected={config.server_ids ?? []}
      servers={servers}
    />
  )
}
```

Add the dispatch entries inside the dialog body (after the `uptime-timeline` block, before the `isModule` block):

```tsx
          {widgetType === 'network-latency' && (
            <NetworkLatencyForm
              config={config as Partial<NetworkLatencyConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {widgetType === 'network-quality' && (
            <NetworkQualityForm
              config={config as Partial<NetworkQualityConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
          {widgetType === 'network-overview' && (
            <NetworkOverviewForm
              config={config as Partial<NetworkOverviewConfig>}
              onChange={setConfig}
              servers={servers}
              t={t}
            />
          )}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/web && bun run test -- widget-config-dialog`
Expected: PASS (all existing + 3 new cases).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/dashboard/widget-config-dialog.tsx apps/web/src/components/dashboard/widget-config-dialog.test.tsx
git commit -m "feat(web): add config forms for network quality widgets"
```

---

## Task 8: i18n strings (en + zh)

**Files:**
- Modify: `apps/web/src/locales/en/dashboard.json`
- Modify: `apps/web/src/locales/zh/dashboard.json`

The widgets use `t(key, fallback)` so missing keys won't crash, but real strings are required for production. Add picker entries, widget titles/empty states, and the `common.timeRange.realtime` key (other timeRange keys already exist).

- [ ] **Step 1: Add English strings**

In `apps/web/src/locales/en/dashboard.json`:

Under `widgetPicker.types`, add (after `uptime-timeline`):

```json
    "network-latency": { "label": "Network Latency", "description": "Latency over time to network probe targets" },
    "network-quality": { "label": "Network Quality", "description": "Current latency and packet loss per target" },
    "network-overview": { "label": "Network Overview", "description": "Network quality across servers" }
```

Under `widgets`, add (sibling of `diskIo`):

```json
    "networkLatency": { "title": "Network Latency", "empty": { "noData": "No network probe data available" } },
    "networkQuality": { "title": "Network Quality", "empty": { "noData": "No network probe data available" } },
    "networkOverview": { "title": "Network Overview", "empty": { "noData": "No network probe data available" } }
```

Under `common.timeRange`, add if missing:

```json
    "realtime": "Realtime"
```

- [ ] **Step 2: Add Chinese strings**

In `apps/web/src/locales/zh/dashboard.json`, mirror the same structure:

Under `widgetPicker.types`:

```json
    "network-latency": { "label": "网络延迟", "description": "对探测目标的延迟随时间变化" },
    "network-quality": { "label": "网络质量", "description": "各目标的当前延迟与丢包" },
    "network-overview": { "label": "网络总览", "description": "跨服务器的网络质量" }
```

Under `widgets`:

```json
    "networkLatency": { "title": "网络延迟", "empty": { "noData": "暂无网络探测数据" } },
    "networkQuality": { "title": "网络质量", "empty": { "noData": "暂无网络探测数据" } },
    "networkOverview": { "title": "网络总览", "empty": { "noData": "暂无网络探测数据" } }
```

Under `common.timeRange`, add if missing:

```json
    "realtime": "实时"
```

- [ ] **Step 3: Validate JSON + typecheck**

Run: `cd apps/web && python3 -c "import json; json.load(open('src/locales/en/dashboard.json')); json.load(open('src/locales/zh/dashboard.json')); print('valid')" && bun run typecheck`
Expected: prints `valid`, no type errors.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/locales/en/dashboard.json apps/web/src/locales/zh/dashboard.json
git commit -m "feat(web): add i18n strings for network quality widgets"
```

---

## Task 9: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full frontend test suite**

Run: `cd apps/web && bun run test`
Expected: PASS — all suites green, including the new network widget tests.

- [ ] **Step 2: Typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: no errors.

- [ ] **Step 3: Lint**

Run: `cd apps/web && bun x ultracite check`
Expected: no errors. If any auto-fixable issues exist, run `bun x ultracite fix` and re-run check, then amend the relevant commit.

- [ ] **Step 4: Build (confirms widgets compile into the bundle)**

Run: `cd apps/web && bun run build`
Expected: build succeeds.

- [ ] **Step 5: Final commit (only if lint/fix changed files)**

```bash
git add -A
git commit -m "chore(web): lint pass for network quality widgets"
```

(Skip if nothing changed in steps 1-4.)

---

## Notes for the implementer

- **Do not push.** The goal is to commit locally only.
- The `servers` prop is accepted by all three widgets for renderer/memoization uniformity even though the network widgets fetch their own data; this matches the existing widget signatures and the `areWidgetServerDependenciesEqual` contract.
- `getLossTextClassName` takes a loss **ratio** (0-1), which matches `NetworkTargetSummary.packet_loss`.
- `formatLatency` / `formatPacketLoss` already handle `null` and ratio→percent formatting.
- If `bun run test -- <pattern>` does not filter as expected in this repo's vitest config, fall back to `bun run test` and read the relevant suite output.
