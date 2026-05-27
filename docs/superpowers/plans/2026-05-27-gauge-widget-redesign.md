# Gauge Widget Visual Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite the Dashboard Gauge widget as a hand-rolled SVG ring with a two-stop gradient, end-cap "ball" knobs, and a metric-aware icon — matching the iOS-style reference in `docs/superpowers/specs/2026-05-27-gauge-widget-redesign-design.md`.

**Architecture:** Single-file rewrite of `apps/web/src/components/dashboard/widgets/gauge.tsx`. Replace `recharts` (`RadialBarChart`) with native SVG (`<path>` arcs + `<linearGradient>` + `<circle>` end caps). Public surface (`GaugeConfig`, `GaugeWidget` signature, dashboard registration) is unchanged. Threshold severity coloring is preserved but rendered as gradients between two `--chart-*` theme variables. A new vitest spec verifies render contract.

**Tech Stack:** React 19, TypeScript, Tailwind v4 (`@container` queries), `lucide-react` (already a dep), vitest + `@testing-library/react`. **Removes** the `recharts` import from this file only — other widgets keep using it.

---

## File Structure

| File                                                              | Responsibility                                                              |
| ----------------------------------------------------------------- | --------------------------------------------------------------------------- |
| `apps/web/src/components/dashboard/widgets/gauge.tsx`             | Rewritten widget: SVG ring + responsive text overlay                        |
| `apps/web/src/components/dashboard/widgets/gauge.test.tsx`        | **New** — vitest covering empty state, label/value, thresholds, clamp, zero |
| (`apps/web/src/lib/widget-types.ts`)                              | Untouched — `GaugeConfig` is unchanged                                      |
| (`apps/web/src/lib/widget-helpers.ts`)                            | Untouched — keeps providing `extractLiveMetric` and `METRIC_LABELS`         |
| (`apps/web/src/components/dashboard/widget-renderer.tsx`)         | Untouched — already routes `gauge` → `GaugeWidget`                          |

Everything lives in one component file. We don't extract helpers into separate modules — they're file-local utilities that aren't reused.

---

## Task 1: Add failing tests for the redesigned gauge

**Files:**
- Create: `apps/web/src/components/dashboard/widgets/gauge.test.tsx`

These tests describe the *target* render contract. They will fail against the current `recharts`-based implementation (no `data-testid` hooks, no `<linearGradient>`, no end-cap circles). The next task makes them pass.

- [ ] **Step 1: Create the test file**

Path: `apps/web/src/components/dashboard/widgets/gauge.test.tsx`

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { GaugeWidget } from './gauge'

function makeServer(id: string, overrides: Partial<ServerMetrics> = {}): ServerMetrics {
  return {
    id,
    name: `Server ${id}`,
    online: true,
    cpu: 50,
    mem_used: 4_000_000_000,
    mem_total: 8_000_000_000,
    swap_used: 0,
    swap_total: 0,
    disk_used: 20_000_000_000,
    disk_total: 40_000_000_000,
    disk_read_bytes_per_sec: 0,
    disk_write_bytes_per_sec: 0,
    net_in_speed: 1024,
    net_out_speed: 2048,
    net_in_transfer: 1,
    net_out_transfer: 1,
    load1: 0.5,
    load5: 0.4,
    load15: 0.3,
    tcp_conn: 10,
    udp_conn: 5,
    process_count: 100,
    uptime: 86_400,
    country_code: 'US',
    os: 'Linux',
    cpu_name: 'Test CPU',
    last_active: Date.now(),
    region: null,
    group_id: null,
    ...overrides
  }
}

function getStops(container: HTMLElement): { start: string | null; end: string | null } {
  const gradient = container.querySelector('[data-testid="gauge-gradient"]')
  if (!gradient) {
    return { start: null, end: null }
  }
  const stops = gradient.querySelectorAll('stop')
  return {
    start: stops[0]?.getAttribute('stop-color') ?? null,
    end: stops[1]?.getAttribute('stop-color') ?? null
  }
}

describe('GaugeWidget', () => {
  it('renders the empty state when the configured server is not in the list', () => {
    render(<GaugeWidget config={{ metric: 'cpu', server_id: 'missing' }} servers={[makeServer('1')]} />)

    expect(screen.getByText('Server not found')).toBeInTheDocument()
    expect(screen.queryByTestId('gauge-svg')).not.toBeInTheDocument()
  })

  it('renders label, formatted value, and server-name subtitle', () => {
    const { container } = render(
      <GaugeWidget
        config={{ label: 'CPU Usage', metric: 'cpu', server_id: '1' }}
        servers={[makeServer('1', { cpu: 50 })]}
      />
    )

    expect(screen.getByTestId('gauge-label')).toHaveTextContent('CPU Usage')
    expect(screen.getByTestId('gauge-value')).toHaveTextContent('50.0%')
    expect(screen.getByTestId('gauge-subtitle')).toHaveTextContent('Server 1')
    expect(container.querySelector('[data-testid="gauge-svg"]')).not.toBeNull()
  })

  it('uses the normal-range gradient (chart-1 → chart-2) when value < 70', () => {
    const { container } = render(
      <GaugeWidget config={{ metric: 'cpu', server_id: '1' }} servers={[makeServer('1', { cpu: 50 })]} />
    )

    expect(getStops(container)).toEqual({
      start: 'var(--chart-1)',
      end: 'var(--chart-2)'
    })
  })

  it('uses the warning gradient (chart-3 → chart-5) when value is in [70, 90)', () => {
    const { container } = render(
      <GaugeWidget config={{ metric: 'cpu', server_id: '1' }} servers={[makeServer('1', { cpu: 75 })]} />
    )

    expect(getStops(container)).toEqual({
      start: 'var(--chart-3)',
      end: 'var(--chart-5)'
    })
  })

  it('uses the critical gradient (chart-4 → chart-3) when value >= 90', () => {
    const { container } = render(
      <GaugeWidget config={{ metric: 'cpu', server_id: '1' }} servers={[makeServer('1', { cpu: 95 })]} />
    )

    expect(getStops(container)).toEqual({
      start: 'var(--chart-4)',
      end: 'var(--chart-3)'
    })
  })

  it('clamps values above the configured max', () => {
    render(
      <GaugeWidget
        config={{ max: 80, metric: 'cpu', server_id: '1' }}
        servers={[makeServer('1', { cpu: 95 })]}
      />
    )

    expect(screen.getByTestId('gauge-value')).toHaveTextContent('80.0%')
  })

  it('hides the progress arc and end-cap balls when value is zero', () => {
    const { container } = render(
      <GaugeWidget config={{ metric: 'cpu', server_id: '1' }} servers={[makeServer('1', { cpu: 0 })]} />
    )

    expect(container.querySelector('[data-testid="gauge-progress"]')).toBeNull()
    expect(container.querySelector('[data-testid="gauge-endcaps"]')).toBeNull()
    // The track is still drawn.
    expect(container.querySelector('[data-testid="gauge-track"]')).not.toBeNull()
  })
})
```

- [ ] **Step 2: Run the test and confirm it fails**

Run from repo root:

```bash
cd apps/web && bun run test -- gauge.test.tsx --run
```

Expected: tests fail because the current `gauge.tsx` (recharts-based) doesn't expose any of these `data-testid` hooks and renders a different DOM. At minimum the empty-state and percentage tests may accidentally pass, but the gradient/zero/clamp/subtitle ones will fail.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/dashboard/widgets/gauge.test.tsx
git commit -m "test(web): add failing render contract for redesigned gauge widget"
```

---

## Task 2: Rewrite `gauge.tsx` as SVG-based, threshold-gradient gauge

**Files:**
- Modify (full rewrite): `apps/web/src/components/dashboard/widgets/gauge.tsx`

- [ ] **Step 1: Replace the file contents**

Path: `apps/web/src/components/dashboard/widgets/gauge.tsx`

```tsx
import { Activity, Cpu, Gauge as GaugeIcon, HardDrive, MemoryStick, Network } from 'lucide-react'
import { useId, useMemo } from 'react'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { extractLiveMetric, METRIC_LABELS } from '@/lib/widget-helpers'
import type { GaugeConfig } from '@/lib/widget-types'

interface GaugeWidgetProps {
  config: GaugeConfig
  servers: ServerMetrics[]
}

// SVG geometry constants (viewBox is 100x100)
const VIEWBOX = 100
const CENTER = VIEWBOX / 2
const RADIUS = 38
const STROKE = 8
const BALL_R = 5.5
const BALL_R_INNER = 2
// 270° sweep, gap centered at top. Angles in degrees, clockwise from 12 o'clock.
const START_ANGLE = 135
const SWEEP = 270

interface Gradient {
  end: string
  start: string
}

function getGaugeGradient(value: number): Gradient {
  if (value >= 90) {
    return { start: 'var(--chart-4)', end: 'var(--chart-3)' }
  }
  if (value >= 70) {
    return { start: 'var(--chart-3)', end: 'var(--chart-5)' }
  }
  return { start: 'var(--chart-1)', end: 'var(--chart-2)' }
}

function getMetricIcon(metric: string) {
  switch (metric) {
    case 'cpu':
      return Cpu
    case 'memory':
    case 'swap':
      return MemoryStick
    case 'disk':
      return HardDrive
    case 'load1':
    case 'load5':
    case 'load15':
      return Activity
    case 'net_in':
    case 'net_out':
    case 'bandwidth':
      return Network
    default:
      return GaugeIcon
  }
}

// Convert a polar coordinate (angle in degrees, clockwise from 12 o'clock) to cartesian SVG coords.
function polarToCartesian(cx: number, cy: number, r: number, angleDeg: number) {
  const angleRad = ((angleDeg - 90) * Math.PI) / 180
  return {
    x: cx + r * Math.cos(angleRad),
    y: cy + r * Math.sin(angleRad)
  }
}

// SVG arc path between two angles (clockwise from 12 o'clock). Sweeps the short or long way
// as needed so that any 0-360° span is drawable.
function arcPath(cx: number, cy: number, r: number, startAngle: number, endAngle: number): string {
  const start = polarToCartesian(cx, cy, r, startAngle)
  const end = polarToCartesian(cx, cy, r, endAngle)
  const span = ((endAngle - startAngle) % 360 + 360) % 360
  const largeArcFlag = span > 180 ? 1 : 0
  return `M ${start.x} ${start.y} A ${r} ${r} 0 ${largeArcFlag} 1 ${end.x} ${end.y}`
}

export function GaugeWidget({ config, servers }: GaugeWidgetProps) {
  const gradientId = useId()
  const server_id = config.server_id ?? ''
  const { metric } = config
  const max = config.max ?? 100

  const server = useMemo(() => servers.find((s) => s.id === server_id), [servers, server_id])

  const value = useMemo(() => {
    if (!server) {
      return 0
    }
    return Math.min(max, Math.max(0, extractLiveMetric(server, metric)))
  }, [server, metric, max])

  if (!server) {
    return (
      <div className="flex h-full items-center justify-center rounded-lg border bg-card text-muted-foreground text-sm">
        Server not found
      </div>
    )
  }

  const label = config.label ?? METRIC_LABELS[metric] ?? metric
  const gradient = getGaugeGradient(value)
  const Icon = getMetricIcon(metric)

  const progressSweep = max > 0 ? (value / max) * SWEEP : 0
  const trackEnd = START_ANGLE + SWEEP
  const progressEnd = START_ANGLE + progressSweep

  const trackPathD = arcPath(CENTER, CENTER, RADIUS, START_ANGLE, trackEnd)
  const progressPathD = arcPath(CENTER, CENTER, RADIUS, START_ANGLE, progressEnd)
  const startCap = polarToCartesian(CENTER, CENTER, RADIUS, START_ANGLE)
  const endCap = polarToCartesian(CENTER, CENTER, RADIUS, progressEnd)

  const showProgress = value > 0

  return (
    <div className="@container/gauge relative flex h-full flex-col items-center justify-center overflow-hidden rounded-lg border bg-card p-3">
      <svg
        aria-hidden="true"
        className="absolute inset-0 h-full w-full"
        data-testid="gauge-svg"
        viewBox={`0 0 ${VIEWBOX} ${VIEWBOX}`}
      >
        <defs>
          <linearGradient
            data-testid="gauge-gradient"
            id={gradientId}
            x1="0%"
            x2="100%"
            y1="0%"
            y2="100%"
          >
            <stop offset="0%" stopColor={gradient.start} />
            <stop offset="100%" stopColor={gradient.end} />
          </linearGradient>
        </defs>
        <path
          data-testid="gauge-track"
          d={trackPathD}
          fill="none"
          stroke="var(--color-muted)"
          strokeLinecap="round"
          strokeOpacity={0.35}
          strokeWidth={STROKE}
        />
        {showProgress && (
          <path
            data-testid="gauge-progress"
            d={progressPathD}
            fill="none"
            stroke={`url(#${gradientId})`}
            strokeLinecap="round"
            strokeWidth={STROKE}
          />
        )}
        {showProgress && (
          <g data-testid="gauge-endcaps">
            <circle cx={startCap.x} cy={startCap.y} fill="white" r={BALL_R} />
            <circle cx={startCap.x} cy={startCap.y} fill={gradient.start} r={BALL_R_INNER} />
            <circle cx={endCap.x} cy={endCap.y} fill="white" r={BALL_R} />
            <circle cx={endCap.x} cy={endCap.y} fill={gradient.end} r={BALL_R_INNER} />
          </g>
        )}
      </svg>

      <div className="relative z-10 flex flex-col items-center text-center">
        <Icon
          aria-hidden="true"
          className="h-3.5 w-3.5 @[10rem]:h-4 @[10rem]:w-4 @[14rem]:h-5 @[14rem]:w-5"
          style={{ color: gradient.start }}
        />
        <p
          className="mt-1 truncate font-medium text-xs @[10rem]:text-sm"
          data-testid="gauge-label"
          style={{ color: gradient.start }}
        >
          {label}
        </p>
        <p
          className="mt-1 font-bold text-2xl text-foreground tabular-nums @[10rem]:text-3xl @[14rem]:text-4xl"
          data-testid="gauge-value"
        >
          {value.toFixed(1)}%
        </p>
        <p
          className="mt-1 hidden max-w-[80%] truncate text-muted-foreground text-xs @[8rem]:block"
          data-testid="gauge-subtitle"
        >
          {server.name}
        </p>
      </div>
    </div>
  )
}
```

Notes for the engineer reviewing this code:

- `useId()` gives every gauge instance its own gradient id, so multiple gauges on the same dashboard don't collide on `url(#...)` refs.
- `@container/gauge` names the container. The text overlay then uses arbitrary container queries (`@[10rem]:`, `@[14rem]:`) instead of fixed Tailwind breakpoints so each gauge scales by its own grid size, not the viewport.
- The empty-state early-return happens *before* computing gradient/icon/paths — keeps the happy path clean and matches the existing pattern in this file.
- The SVG is absolutely positioned behind the text via `absolute inset-0`. The text wrapper uses `relative z-10` so it sits on top. The card itself is `flex flex-col items-center justify-center` to center the text vertically.

- [ ] **Step 2: Run the gauge tests; expect all to pass**

```bash
cd apps/web && bun run test -- gauge.test.tsx --run
```

Expected: all 7 tests pass.

- [ ] **Step 3: Run typecheck and the existing widget test suite**

```bash
bun run typecheck
cd apps/web && bun run test --run
```

Expected: typecheck passes; full vitest run is green. If any existing test fails, it's almost certainly because another file imports `recharts` differently — read the failure and fix only if it's caused by this change.

- [ ] **Step 4: Run lint and auto-format**

From repo root:

```bash
bun x ultracite check apps/web/src/components/dashboard/widgets/gauge.tsx apps/web/src/components/dashboard/widgets/gauge.test.tsx
```

If the check reports fixable issues:

```bash
bun x ultracite fix apps/web/src/components/dashboard/widgets/gauge.tsx apps/web/src/components/dashboard/widgets/gauge.test.tsx
```

Expected: clean (zero diagnostics) after the fix pass.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/dashboard/widgets/gauge.tsx apps/web/src/components/dashboard/widgets/gauge.test.tsx
git commit -m "feat(web): redesign gauge widget with svg ring and gradient stroke"
```

(The test file's intent is now satisfied — same commit folds in any reformatting from Step 4.)

---

## Task 3: Verify across the wider build and report visual-verification status

**Files:** none modified (verification + reporting only)

- [ ] **Step 1: Run the workspace-level checks**

From repo root:

```bash
bun run typecheck
cd apps/web && bun run test --run
bun x ultracite check
```

Expected: all clean. If `bun x ultracite check` flags unrelated files modified in earlier sessions, leave them alone — only fix files this plan touched.

- [ ] **Step 2: Note manual visual verification status**

Per project policy on UI changes, a real browser eyeballing pass is required before claiming completion. This environment may not have a browser. The executor must do *one* of the following and record the outcome in the final summary:

- **If a browser is available:** start `cd apps/web && bun run dev`, open a dashboard with a Gauge widget at min (2×2) and max (6×6) sizes, confirm against `docs/superpowers/specs/2026-05-27-gauge-widget-redesign-design.md` § 1. Take notes; capture a screenshot path if possible.
- **If no browser is available:** state this explicitly in the completion summary — "Visual verification not done; no browser tool in this environment." Do NOT claim the visual matches the reference without having seen it. This matches the user's standing preference (`feedback_visual_verification.md` in MEMORY.md).

- [ ] **Step 3: Confirm no `recharts` regression**

```bash
rg "recharts" apps/web/src/components/dashboard/widgets/gauge.tsx
```

Expected: zero matches (the import was removed).

```bash
rg "recharts" apps/web/src/
```

Expected: still present in other widget files (`line-chart-widget.tsx`, `multi-line.tsx`, `disk-io.tsx`, `traffic-bar.tsx`, etc.). The dependency stays in `package.json`.

- [ ] **Step 4: Final commit only if Task 3 produced changes**

Task 3 is verification-only. Do not create an empty commit. If the ultracite check in Step 1 produced fixes that weren't covered by Task 2's commit, stage and commit them:

```bash
git add -A
git commit -m "chore(web): apply ultracite formatting to gauge files"
```

Otherwise skip.

---

## Self-Review

**1. Spec coverage:**

| Spec section                          | Plan coverage                                                                  |
| ------------------------------------- | ------------------------------------------------------------------------------ |
| § 1 Visual anatomy (icon/label/value/subtitle) | Task 2 Step 1 — all four elements rendered with documented testids       |
| § 2 Threshold gradient table          | Task 1 Steps 3/4/5 (gradient stop assertions) + Task 2 `getGaugeGradient`      |
| § 3 Component structure / helpers     | Task 2 Step 1 — all helpers (`getGaugeGradient`, `getMetricIcon`, `polarToCartesian`, `arcPath`) defined |
| § 3 SVG structure                     | Task 2 Step 1 — matches the spec's simplified TSX example                      |
| § 4 Responsive sizing (container queries) | Task 2 Step 1 — `@container/gauge` + `@[10rem]/@[14rem]` arbitrary breakpoints |
| § 5 State handling (empty, clamp, zero) | Task 1 Steps 1/6/7 + Task 2 early-return and `showProgress` gate            |
| § 6 Dependency change (drop recharts) | Task 2 Step 1 (no recharts import) + Task 3 Step 3 (grep verification)        |
| § 7 Testing (6 cases)                 | Task 1 Step 1 — all six listed cases plus the label/value sanity test         |
| § 8 Out-of-scope                      | N/A — not implemented, as intended                                             |
| § 9 File diff summary                 | Plan's File Structure table matches exactly                                    |

No gaps.

**2. Placeholder scan:** No TBDs, no "handle errors appropriately", no "similar to above". All code blocks are complete.

**3. Type/name consistency:**
- `getGaugeGradient` returns `{ start, end }` — used as `gradient.start` / `gradient.end` everywhere. ✓
- `data-testid` strings (`gauge-svg`, `gauge-gradient`, `gauge-track`, `gauge-progress`, `gauge-endcaps`, `gauge-label`, `gauge-value`, `gauge-subtitle`) match between Task 1's assertions and Task 2's implementation. ✓
- `getStops` helper in the test file reads `'stop-color'` attribute, which is what React emits for `stopColor` in SVG. ✓
- `useId()` gradient id is referenced via `url(#${gradientId})` and as the `<linearGradient id={gradientId}>`. ✓
- `SWEEP = 270`, `START_ANGLE = 135` — `trackEnd = 405° = 45°` mod 360, which is the spec's stated end angle. ✓

All consistent.
