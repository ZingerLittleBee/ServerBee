# Server Card Grid Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace progress bars with ring charts, add uptime-style bar charts for network quality, and pack all 9 metrics into the server card.

**Architecture:** Two new UI primitives (`RingChart`, `UptimeBar`) plus a rewrite of `ServerCard` layout. Data layer (`useNetworkRealtime`, `useServersWs`) unchanged. i18n keys added for new short labels.

**Tech Stack:** React 19, SVG (ring charts), Vitest + @testing-library/react, i18next

**Spec:** `docs/superpowers/specs/2026-03-30-server-card-grid-redesign.md`

---

### File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `apps/web/src/components/ui/ring-chart.tsx` | Create | SVG donut ring chart component |
| `apps/web/src/components/ui/ring-chart.test.tsx` | Create | Unit tests for RingChart |
| `apps/web/src/components/ui/uptime-bar.tsx` | Create | Vertical bar chart component (uptime-style) |
| `apps/web/src/components/ui/uptime-bar.test.tsx` | Create | Unit tests for UptimeBar |
| `apps/web/src/components/server/server-card.tsx` | Rewrite | New card layout with rings + bars |
| `apps/web/src/components/server/server-card.test.tsx` | Create | Unit tests for ServerCard |
| `apps/web/src/locales/en/servers.json` | Modify | Add `card_net_in_speed`, `card_net_out_speed` |
| `apps/web/src/locales/zh/servers.json` | Modify | Add `card_net_in_speed`, `card_net_out_speed` |

---

### Task 1: Add i18n Keys

**Files:**
- Modify: `apps/web/src/locales/en/servers.json`
- Modify: `apps/web/src/locales/zh/servers.json`

- [ ] **Step 1: Add English keys**

Add after the `"card_udp": "UDP"` line in `apps/web/src/locales/en/servers.json`:

```json
"card_net_in_speed": "↓ In",
"card_net_out_speed": "↑ Out",
```

- [ ] **Step 2: Add Chinese keys**

Add after the `"card_udp": "UDP"` line in `apps/web/src/locales/zh/servers.json`:

```json
"card_net_in_speed": "↓ 入站",
"card_net_out_speed": "↑ 出站",
```

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/locales/en/servers.json apps/web/src/locales/zh/servers.json
git commit -m "feat(web): add i18n keys for server card network speed labels"
```

---

### Task 2: Create RingChart Component (TDD)

**Files:**
- Create: `apps/web/src/components/ui/ring-chart.test.tsx`
- Create: `apps/web/src/components/ui/ring-chart.tsx`

- [ ] **Step 1: Write failing tests**

Create `apps/web/src/components/ui/ring-chart.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { RingChart } from './ring-chart'

describe('RingChart', () => {
  it('renders percentage text', () => {
    render(<RingChart color="#3b82f6" label="CPU" value={72.3} />)
    expect(screen.getByText('72.3%')).toBeDefined()
  })

  it('renders label', () => {
    render(<RingChart color="#3b82f6" label="CPU" value={50} />)
    expect(screen.getByText('CPU')).toBeDefined()
  })

  it('renders SVG with accessible role and label', () => {
    render(<RingChart color="#3b82f6" label="MEM" value={85} />)
    const svg = screen.getByRole('img')
    expect(svg.getAttribute('aria-label')).toBe('MEM 85.0%')
  })

  it('clamps value to 0-100 range', () => {
    const { rerender } = render(<RingChart color="#3b82f6" label="CPU" value={150} />)
    expect(screen.getByText('100.0%')).toBeDefined()

    rerender(<RingChart color="#3b82f6" label="CPU" value={-10} />)
    expect(screen.getByText('0.0%')).toBeDefined()
  })

  it('accepts custom size', () => {
    const { container } = render(<RingChart color="#3b82f6" label="CPU" size={40} value={50} />)
    const wrapper = container.firstElementChild as HTMLElement
    expect(wrapper.style.width).toBe('40px')
  })

  it('renders two circles (background track + foreground arc)', () => {
    const { container } = render(<RingChart color="#3b82f6" label="CPU" value={50} />)
    const circles = container.querySelectorAll('circle')
    expect(circles.length).toBe(2)
  })

  it('applies color to foreground circle stroke', () => {
    const { container } = render(<RingChart color="var(--color-chart-1)" label="CPU" value={50} />)
    const circles = container.querySelectorAll('circle')
    const foreground = circles[1]
    expect(foreground.getAttribute('stroke')).toBe('var(--color-chart-1)')
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/web && bunx vitest run src/components/ui/ring-chart.test.tsx`

Expected: FAIL — module `./ring-chart` not found

- [ ] **Step 3: Implement RingChart**

Create `apps/web/src/components/ui/ring-chart.tsx`:

```tsx
interface RingChartProps {
  color: string
  label: string
  size?: number
  strokeWidth?: number
  value: number
}

const VIEWBOX = 36
const DEFAULT_SIZE = 56
const DEFAULT_STROKE = 3.5

export function RingChart({ value, size = DEFAULT_SIZE, strokeWidth = DEFAULT_STROKE, color, label }: RingChartProps) {
  const clamped = Math.min(100, Math.max(0, value))
  const radius = (VIEWBOX - strokeWidth) / 2
  const circumference = 2 * Math.PI * radius
  const dashArray = `${(clamped / 100) * circumference} ${circumference}`

  return (
    <div style={{ width: size }}>
      <div style={{ position: 'relative', width: size, height: size }}>
        <svg
          aria-label={`${label} ${clamped.toFixed(1)}%`}
          role="img"
          style={{ transform: 'rotate(-90deg)' }}
          viewBox={`0 0 ${VIEWBOX} ${VIEWBOX}`}
        >
          <circle
            cx={VIEWBOX / 2}
            cy={VIEWBOX / 2}
            fill="none"
            r={radius}
            stroke="rgba(128,128,128,0.15)"
            strokeWidth={strokeWidth}
          />
          <circle
            cx={VIEWBOX / 2}
            cy={VIEWBOX / 2}
            fill="none"
            r={radius}
            stroke={color}
            strokeDasharray={dashArray}
            strokeLinecap="round"
            strokeWidth={strokeWidth}
          />
        </svg>
        <div
          style={{
            position: 'absolute',
            inset: 0,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontSize: '11px',
            fontWeight: 700
          }}
        >
          {clamped.toFixed(1)}%
        </div>
      </div>
      <div className="mt-0.5 text-center text-[10px] text-muted-foreground">
        {label}
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/web && bunx vitest run src/components/ui/ring-chart.test.tsx`

Expected: all 7 tests PASS

- [ ] **Step 5: Lint check**

Run: `cd apps/web && bun x ultracite check src/components/ui/ring-chart.tsx src/components/ui/ring-chart.test.tsx`

Fix any issues with `bun x ultracite fix ...` if needed.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/ui/ring-chart.tsx apps/web/src/components/ui/ring-chart.test.tsx
git commit -m "feat(web): add RingChart SVG donut component"
```

---

### Task 3: Create UptimeBar Component (TDD)

**Files:**
- Create: `apps/web/src/components/ui/uptime-bar.test.tsx`
- Create: `apps/web/src/components/ui/uptime-bar.tsx`

- [ ] **Step 1: Write failing tests**

Create `apps/web/src/components/ui/uptime-bar.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { UptimeBar } from './uptime-bar'

const greenColor = (v: number | null) => (v == null || v >= 100 ? '#ef4444' : v >= 50 ? '#f59e0b' : '#10b981')

describe('UptimeBar', () => {
  it('renders one bar per data point', () => {
    const { container } = render(<UptimeBar data={[10, 20, 30]} getColor={greenColor} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    expect(bars.length).toBe(3)
  })

  it('renders nothing when data is empty', () => {
    const { container } = render(<UptimeBar data={[]} getColor={greenColor} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    expect(bars.length).toBe(0)
  })

  it('applies color from getColor callback', () => {
    const { container } = render(<UptimeBar data={[10, 80, null]} getColor={greenColor} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    expect((bars[0] as HTMLElement).style.backgroundColor).toBe('#10b981')
    expect((bars[1] as HTMLElement).style.backgroundColor).toBe('#f59e0b')
    expect((bars[2] as HTMLElement).style.backgroundColor).toBe('#ef4444')
  })

  it('renders null values at 100% height', () => {
    const { container } = render(<UptimeBar data={[50, null]} getColor={greenColor} maxValue={100} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    expect((bars[1] as HTMLElement).style.height).toBe('100%')
  })

  it('scales bar heights relative to maxValue', () => {
    const { container } = render(<UptimeBar data={[50, 100]} getColor={greenColor} maxValue={100} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    expect((bars[0] as HTMLElement).style.height).toBe('50%')
    expect((bars[1] as HTMLElement).style.height).toBe('100%')
  })

  it('uses data max when maxValue not provided', () => {
    const { container } = render(<UptimeBar data={[25, 50]} getColor={greenColor} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    // 25/50 = 50%, 50/50 = 100%
    expect((bars[0] as HTMLElement).style.height).toBe('50%')
    expect((bars[1] as HTMLElement).style.height).toBe('100%')
  })

  it('enforces minimum 10% height for non-null non-zero values', () => {
    const { container } = render(<UptimeBar data={[1, 100]} getColor={greenColor} maxValue={100} />)
    const bars = container.querySelectorAll('[data-testid="uptime-bar-item"]')
    // 1/100 = 1%, but min is 10%
    expect((bars[0] as HTMLElement).style.height).toBe('10%')
  })

  it('has accessible label', () => {
    render(<UptimeBar ariaLabel="Latency trend" data={[10, 20]} getColor={greenColor} />)
    expect(screen.getByLabelText('Latency trend')).toBeDefined()
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/web && bunx vitest run src/components/ui/uptime-bar.test.tsx`

Expected: FAIL — module `./uptime-bar` not found

- [ ] **Step 3: Implement UptimeBar**

Create `apps/web/src/components/ui/uptime-bar.tsx`:

```tsx
interface UptimeBarProps {
  ariaLabel?: string
  data: (number | null)[]
  getColor: (value: number | null) => string
  height?: number
  maxValue?: number
}

const MIN_HEIGHT_PCT = 10

export function UptimeBar({ data, height = 16, getColor, maxValue, ariaLabel }: UptimeBarProps) {
  const effectiveMax =
    maxValue ?? data.reduce<number>((max, v) => (v != null && v > max ? v : max), 0)

  function barHeight(value: number | null): string {
    if (value == null) {
      return '100%'
    }
    if (effectiveMax <= 0) {
      return `${MIN_HEIGHT_PCT}%`
    }
    const pct = (value / effectiveMax) * 100
    if (value > 0 && pct < MIN_HEIGHT_PCT) {
      return `${MIN_HEIGHT_PCT}%`
    }
    return `${Math.min(100, pct)}%`
  }

  return (
    <div
      aria-label={ariaLabel}
      style={{ display: 'flex', gap: '2px', height, alignItems: 'flex-end' }}
    >
      {data.map((value, i) => (
        <div
          data-testid="uptime-bar-item"
          key={i}
          style={{
            flex: 1,
            borderRadius: '2px',
            backgroundColor: getColor(value),
            height: barHeight(value)
          }}
        />
      ))}
    </div>
  )
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/web && bunx vitest run src/components/ui/uptime-bar.test.tsx`

Expected: all 8 tests PASS

- [ ] **Step 5: Lint check**

Run: `cd apps/web && bun x ultracite check src/components/ui/uptime-bar.tsx src/components/ui/uptime-bar.test.tsx`

Fix any issues if needed.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/ui/uptime-bar.tsx apps/web/src/components/ui/uptime-bar.test.tsx
git commit -m "feat(web): add UptimeBar vertical bar chart component"
```

---

### Task 4: Rewrite ServerCard Layout

**Files:**
- Modify: `apps/web/src/components/server/server-card.tsx`

**Context:** The current file is 202 lines. The entire file will be rewritten with the new layout. The `ProgressBar` local component is removed. Helper functions (`osIcon`, `getBarColor`, `formatLoad`, `formatLatency`, `formatPacketLoss`, `getLatencyColorClass`, `getLossColorClass`) are kept or adapted. `RingChart` and `UptimeBar` are imported from the new components.

- [ ] **Step 1: Rewrite server-card.tsx**

Replace the entire content of `apps/web/src/components/server/server-card.tsx` with:

```tsx
import { Link } from '@tanstack/react-router'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { RingChart } from '@/components/ui/ring-chart'
import { UptimeBar } from '@/components/ui/uptime-bar'
import { CompactMetric } from '@/components/server/compact-metric'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'
import { StatusBadge } from './status-badge'

interface ServerCardProps {
  server: ServerMetrics
}

function osIcon(os: string | null): string {
  if (!os) return ''
  const lower = os.toLowerCase()
  if (lower.includes('ubuntu') || lower.includes('debian') || lower.includes('linux')) return '🐧'
  if (lower.includes('windows')) return '🪟'
  if (lower.includes('macos') || lower.includes('darwin')) return '🍎'
  if (lower.includes('freebsd') || lower.includes('openbsd')) return '😈'
  return ''
}

function getRingColor(pct: number, brandColor: string): string {
  if (pct > 90) return '#ef4444'
  if (pct > 70) return '#f59e0b'
  return brandColor
}

function getLatencyColor(ms: number | null): string {
  if (ms == null) return '#ef4444'
  if (ms < 50) return '#10b981'
  if (ms < 100) return '#f59e0b'
  return '#ef4444'
}

function getLossColor(loss: number | null): string {
  if (loss == null) return '#ef4444'
  if (loss < 1) return '#10b981'
  if (loss < 5) return '#f59e0b'
  return '#ef4444'
}

function getLatencyTextClass(ms: number | null): string {
  if (ms == null) return 'text-muted-foreground'
  if (ms < 50) return 'text-emerald-600 dark:text-emerald-400'
  if (ms < 100) return 'text-amber-600 dark:text-amber-400'
  return 'text-red-600 dark:text-red-400'
}

function getLossTextClass(loss: number): string {
  if (loss < 1) return 'text-emerald-600 dark:text-emerald-400'
  if (loss < 5) return 'text-amber-600 dark:text-amber-400'
  return 'text-red-600 dark:text-red-400'
}

function formatLatency(ms: number | null): string {
  if (ms == null) return '-'
  return `${ms.toFixed(0)}ms`
}

function formatPacketLoss(loss: number): string {
  return `${loss.toFixed(1)}%`
}

function formatLoad(load: number): string {
  return load.toFixed(2)
}

export function ServerCard({ server }: ServerCardProps) {
  const { t } = useTranslation(['servers'])
  const { data: networkData } = useNetworkRealtime(server.id)

  const memoryPct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  const diskPct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
  const swapPct = server.swap_total > 0 ? (server.swap_used / server.swap_total) * 100 : 0
  const flag = countryCodeToFlag(server.country_code)
  const osEmoji = osIcon(server.os)

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

  return (
    <Link
      className="group flex h-full flex-col rounded-lg border bg-card p-4 shadow-sm transition-colors hover:bg-accent/50"
      params={{ id: server.id }}
      search={{ range: 'realtime' }}
      to="/servers/$id"
    >
      {/* Header */}
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center gap-1.5 truncate">
          {flag && (
            <span className="shrink-0 text-sm" title={server.country_code ?? ''}>
              {flag}
            </span>
          )}
          {osEmoji && (
            <span className="shrink-0 text-sm" title={server.os ?? ''}>
              {osEmoji}
            </span>
          )}
          <h3 className="truncate font-semibold text-sm">{server.name}</h3>
        </div>
        <StatusBadge online={server.online} />
      </div>

      {/* Ring Charts */}
      <div className="mb-3 flex justify-around">
        <RingChart color={getRingColor(server.cpu, 'var(--color-chart-1)')} label={t('col_cpu')} value={server.cpu} />
        <RingChart color={getRingColor(memoryPct, 'var(--color-chart-2)')} label={t('col_memory')} value={memoryPct} />
        <RingChart color={getRingColor(diskPct, 'var(--color-chart-3)')} label={t('col_disk')} value={diskPct} />
      </div>

      {/* System Metrics Row */}
      <div className="mb-1.5 grid grid-cols-5 gap-1 rounded-lg bg-muted/40 px-2 py-1.5">
        <CompactMetric className="items-center" label={t('card_load')} value={formatLoad(server.load1)} />
        <CompactMetric className="items-center" label={t('card_processes')} value={server.process_count} />
        <CompactMetric className="items-center" label={t('card_tcp')} value={server.tcp_conn} />
        <CompactMetric className="items-center" label={t('card_udp')} value={server.udp_conn} />
        <CompactMetric className="items-center" label={t('card_swap')} value={`${swapPct.toFixed(0)}%`} />
      </div>

      {/* Network Metrics Row */}
      <div className="mb-3 grid grid-cols-4 gap-1 rounded-lg bg-muted/40 px-2 py-1.5">
        <CompactMetric className="items-center" label={t('card_net_in_speed')} value={formatSpeed(server.net_in_speed)} />
        <CompactMetric className="items-center" label={t('card_net_out_speed')} value={formatSpeed(server.net_out_speed)} />
        <CompactMetric className="items-center" label={t('card_net_total')} value={formatBytes(server.net_in_transfer + server.net_out_transfer)} />
        <CompactMetric className="items-center" label={t('col_uptime')} value={formatUptime(server.uptime)} />
      </div>

      {/* Network Quality */}
      {latencyData.length > 0 && (
        <div className="mt-auto border-t pt-3">
          <div className="mb-2">
            <div className="mb-1 flex items-center justify-between">
              <span className="text-[10px] text-muted-foreground">{t('card_latency')}</span>
              <span className={`font-medium text-xs ${getLatencyTextClass(avgLatency)}`}>
                {formatLatency(avgLatency)}
              </span>
            </div>
            <UptimeBar ariaLabel="Latency trend" data={latencyData} getColor={getLatencyColor} />
          </div>
          <div>
            <div className="mb-1 flex items-center justify-between">
              <span className="text-[10px] text-muted-foreground">{t('card_packet_loss')}</span>
              <span className={`font-medium text-xs ${getLossTextClass(avgLoss)}`}>
                {formatPacketLoss(avgLoss)}
              </span>
            </div>
            <UptimeBar ariaLabel="Packet loss trend" data={lossData} getColor={getLossColor} />
          </div>
        </div>
      )}
    </Link>
  )
}
```

- [ ] **Step 2: Lint check**

Run: `cd apps/web && bun x ultracite check src/components/server/server-card.tsx`

Fix any issues if needed.

- [ ] **Step 3: TypeScript check**

Run: `cd apps/web && bun run typecheck`

Expected: No errors related to server-card, ring-chart, or uptime-bar files.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/components/server/server-card.tsx
git commit -m "feat(web): rewrite server card with ring charts and uptime bars"
```

---

### Task 5: Add ServerCard Tests

**Files:**
- Create: `apps/web/src/components/server/server-card.test.tsx`

**Context:** The ServerCard depends on `@tanstack/react-router` (Link), `react-i18next`, `useNetworkRealtime`. These must be mocked. Follow the mocking pattern from `traffic-card.test.tsx`.

- [ ] **Step 1: Write ServerCard tests**

Create `apps/web/src/components/server/server-card.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { ServerCard } from './server-card'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key
  })
}))

vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, ...props }: { children?: React.ReactNode; [k: string]: unknown }) => (
    <a data-testid="server-link" href={`/servers/${props.params && (props.params as { id: string }).id}`}>
      {children}
    </a>
  )
}))

const mockNetworkData = vi.fn()
vi.mock('@/hooks/use-network-realtime', () => ({
  useNetworkRealtime: (...args: unknown[]) => mockNetworkData(...args)
}))

function makeServer(overrides: Partial<Parameters<typeof ServerCard>[0]['server']> = {}) {
  return {
    id: 'srv-1',
    name: 'test-server',
    online: true,
    country_code: 'US',
    os: 'Ubuntu 22.04',
    cpu: 72,
    cpu_name: 'Intel i7',
    mem_used: 4_294_967_296,
    mem_total: 8_589_934_592,
    disk_used: 21_474_836_480,
    disk_total: 53_687_091_200,
    swap_used: 536_870_912,
    swap_total: 2_147_483_648,
    load1: 0.72,
    load5: 0.65,
    load15: 0.58,
    process_count: 142,
    tcp_conn: 38,
    udp_conn: 12,
    uptime: 1_987_200,
    net_in_speed: 12_900_000,
    net_out_speed: 4_300_000,
    net_in_transfer: 1_099_511_627_776,
    net_out_transfer: 549_755_813_888,
    region: null,
    group_id: null,
    last_active: Date.now(),
    ...overrides
  }
}

describe('ServerCard', () => {
  beforeEach(() => {
    mockNetworkData.mockReturnValue({ data: {} })
  })

  it('renders server name', () => {
    render(<ServerCard server={makeServer()} />)
    expect(screen.getByText('test-server')).toBeDefined()
  })

  it('renders three ring charts with CPU, Memory, Disk labels', () => {
    render(<ServerCard server={makeServer()} />)
    expect(screen.getByText('col_cpu')).toBeDefined()
    expect(screen.getByText('col_memory')).toBeDefined()
    expect(screen.getByText('col_disk')).toBeDefined()
  })

  it('renders system metrics row', () => {
    render(<ServerCard server={makeServer()} />)
    expect(screen.getByText('card_load')).toBeDefined()
    expect(screen.getByText('card_processes')).toBeDefined()
    expect(screen.getByText('card_tcp')).toBeDefined()
    expect(screen.getByText('card_udp')).toBeDefined()
    expect(screen.getByText('card_swap')).toBeDefined()
  })

  it('renders network metrics row', () => {
    render(<ServerCard server={makeServer()} />)
    expect(screen.getByText('card_net_in_speed')).toBeDefined()
    expect(screen.getByText('card_net_out_speed')).toBeDefined()
    expect(screen.getByText('card_net_total')).toBeDefined()
    expect(screen.getByText('col_uptime')).toBeDefined()
  })

  it('does not render network quality section when no data', () => {
    render(<ServerCard server={makeServer()} />)
    expect(screen.queryByLabelText('Latency trend')).toBeNull()
  })

  it('renders network quality bars when probe data exists', () => {
    mockNetworkData.mockReturnValue({
      data: {
        'target-1': [
          { target_id: 'target-1', avg_latency: 32, packet_loss: 0.002, packet_sent: 4, packet_received: 4, min_latency: 28, max_latency: 36, timestamp: '2026-03-31T10:00:00Z' },
          { target_id: 'target-1', avg_latency: 45, packet_loss: 0.0, packet_sent: 4, packet_received: 4, min_latency: 40, max_latency: 50, timestamp: '2026-03-31T10:01:00Z' }
        ]
      }
    })
    render(<ServerCard server={makeServer()} />)
    expect(screen.getByLabelText('Latency trend')).toBeDefined()
    expect(screen.getByLabelText('Packet loss trend')).toBeDefined()
  })

  it('sorts network data chronologically across targets', () => {
    mockNetworkData.mockReturnValue({
      data: {
        'target-a': [
          { target_id: 'target-a', avg_latency: 100, packet_loss: 0, packet_sent: 4, packet_received: 4, min_latency: 90, max_latency: 110, timestamp: '2026-03-31T10:02:00Z' }
        ],
        'target-b': [
          { target_id: 'target-b', avg_latency: 20, packet_loss: 0, packet_sent: 4, packet_received: 4, min_latency: 18, max_latency: 22, timestamp: '2026-03-31T10:01:00Z' }
        ]
      }
    })
    render(<ServerCard server={makeServer()} />)
    // Both targets should produce 2 bars
    const bars = screen.getByLabelText('Latency trend').querySelectorAll('[data-testid="uptime-bar-item"]')
    expect(bars.length).toBe(2)
  })

  it('handles null avg_latency as probe failure', () => {
    mockNetworkData.mockReturnValue({
      data: {
        'target-1': [
          { target_id: 'target-1', avg_latency: null, packet_loss: 1.0, packet_sent: 4, packet_received: 0, min_latency: null, max_latency: null, timestamp: '2026-03-31T10:00:00Z' }
        ]
      }
    })
    render(<ServerCard server={makeServer()} />)
    // Avg latency should display "-"
    expect(screen.getByText('-')).toBeDefined()
  })

  it('renders StatusBadge', () => {
    render(<ServerCard server={makeServer({ online: false })} />)
    expect(screen.getByText('offline')).toBeDefined()
  })
})
```

- [ ] **Step 2: Run tests**

Run: `cd apps/web && bunx vitest run src/components/server/server-card.test.tsx`

Expected: all tests PASS

- [ ] **Step 3: Lint check**

Run: `cd apps/web && bun x ultracite check src/components/server/server-card.test.tsx`

- [ ] **Step 4: Run full test suite**

Run: `cd apps/web && bun run test`

Expected: all 121+ tests PASS (existing tests unaffected)

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/server/server-card.test.tsx
git commit -m "test(web): add server card component tests"
```

---

### Task 6: Visual Verification & Cleanup

**Files:**
- No new files

- [ ] **Step 1: Build frontend**

Run: `cd apps/web && bun run build`

Expected: Build succeeds with no errors.

- [ ] **Step 2: Run full lint + typecheck**

Run: `cd apps/web && bun x ultracite check && bun run typecheck`

Expected: No warnings or errors.

- [ ] **Step 3: Run all tests**

Run: `cd apps/web && bun run test`

Expected: All tests pass.

- [ ] **Step 4: Commit if any fixes were needed**

Only if previous steps required fixes:

```bash
git add -u
git commit -m "fix(web): address lint and build issues in server card redesign"
```
