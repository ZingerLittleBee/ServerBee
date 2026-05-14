# ServerCard 双列布局重设计 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 ServerCard 重构为响应式双列布局 (2×2 RingChart + 左右并排方块栅格 + tag chips),并让外层 grid 按宽度自适应排列。

**Architecture:** 在现有 `server-card.tsx` 内部重排布局,新增内部组件 `RingMetric` / `NetworkSquareGrid` / `NetworkMetricHeader` / `TagChips`,把网络方块栅格用 `ResizeObserver` 实现动态点数。把 `getCombinedBarColor` 拆成 `getLatencySquareColor` / `getLossSquareColor` 两个纯函数,删除 SeverityBar 在 ServerCard 中的使用(组件文件保留)。

**Tech Stack:** React 19 + TanStack Router / Query + Tailwind CSS v4 + shadcn/ui + Vitest + Testing Library + Biome (Ultracite)

**Spec:** `docs/superpowers/specs/2026-05-15-server-card-2col-redesign-design.md`

---

## File Structure

**Create:**
- `apps/web/src/components/server/network-square-grid.tsx` — 单个方块栅格组件 (latency 或 loss),内部用 `ResizeObserver` 计算可显示方块数
- `apps/web/src/components/server/network-square-grid.test.tsx` — 测试 ResizeObserver + slice 行为
- `apps/web/src/components/server/tag-chips.tsx` — 渲染 `server.tags` 为彩色 chip,空数组不渲染
- `apps/web/src/components/server/tag-chips.test.tsx`

**Modify:**
- `apps/web/src/lib/network-latency-constants.ts` — 新增 `getLatencySquareColor` / `getLossSquareColor`
- `apps/web/src/lib/network-latency-constants.test.ts` — 测试新函数
- `apps/web/src/components/server/server-card-network-data.ts` — `MAX_TREND_POINTS` 12→30
- `apps/web/src/components/server/server-card-network-data.test.ts` — 长度断言 12→30
- `apps/web/src/components/ui/ring-chart.tsx` — 新增 `compact?: boolean` prop
- `apps/web/src/components/server/server-card.tsx` — 整体重排,内部新增 `RingMetric` / `NetworkMetricHeader`
- `apps/web/src/components/server/server-card.test.tsx` — 删除/更新依赖 SeverityBar 的断言,新增 2×2 + tags + 方块栅格的断言
- `apps/web/src/components/dashboard/widgets/server-cards.tsx` — `gridTemplateColumns` 改 `auto-fill, minmax(320px, 1fr)`,忽略 `config.columns`
- `apps/web/src/test/setup.ts` — 注册 ResizeObserver polyfill (供 jsdom)
- `apps/web/src/locales/{zh,en}/servers.json` — 新增 `card_latency` key (现有只有 `card_packet_loss`)

**Not deleted:** `apps/web/src/components/server/severity-bar.tsx` 保留(本次只让 ServerCard 不再 import,组件文件留作未来清理任务)

---

## Task 1: 新增 ResizeObserver polyfill 到测试 setup

**Files:**
- Modify: `apps/web/src/test/setup.ts`

- [ ] **Step 1: Run existing test suite to confirm baseline**

Run: `cd apps/web && bun run test -- --run`
Expected: All tests pass (record any pre-existing failures to ignore later).

- [ ] **Step 2: Add ResizeObserver polyfill**

Replace contents of `apps/web/src/test/setup.ts`:

```ts
import '@testing-library/jest-dom'

class ResizeObserverMock {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

if (typeof globalThis.ResizeObserver === 'undefined') {
  // @ts-expect-error jsdom does not implement ResizeObserver
  globalThis.ResizeObserver = ResizeObserverMock
}
```

- [ ] **Step 3: Run tests to confirm no regression**

Run: `cd apps/web && bun run test -- --run`
Expected: Same pass/fail count as Step 1.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/test/setup.ts
git commit -m "test(web): add ResizeObserver polyfill for jsdom"
```

---

## Task 2: 拆分 latency/loss square color helpers

**Files:**
- Modify: `apps/web/src/lib/network-latency-constants.ts`
- Test: `apps/web/src/lib/network-latency-constants.test.ts`

- [ ] **Step 1: Write failing tests for getLatencySquareColor**

Append to `apps/web/src/lib/network-latency-constants.test.ts` (inside the existing `describe('network-latency-constants', ...)` block):

```ts
describe('getLatencySquareColor', () => {
  it('returns muted for null latency', () => {
    expect(getLatencySquareColor({ latencyMs: null, lossRatio: 0 })).toBe(LATENCY_UNKNOWN_BAR_COLOR)
  })

  it('returns failed color when loss indicates probe failure', () => {
    expect(getLatencySquareColor({ latencyMs: 40, lossRatio: 1 })).toBe(LATENCY_FAILED_BAR_COLOR)
    expect(getLatencySquareColor({ latencyMs: null, lossRatio: 1 })).toBe(LATENCY_FAILED_BAR_COLOR)
  })

  it('returns healthy color below threshold', () => {
    expect(getLatencySquareColor({ latencyMs: 50, lossRatio: 0 })).toBe(LATENCY_HEALTHY_BAR_COLOR)
    expect(getLatencySquareColor({ latencyMs: 299, lossRatio: 0 })).toBe(LATENCY_HEALTHY_BAR_COLOR)
  })

  it('returns warning color at or above threshold', () => {
    expect(getLatencySquareColor({ latencyMs: 300, lossRatio: 0 })).toBe(LATENCY_WARNING_BAR_COLOR)
    expect(getLatencySquareColor({ latencyMs: 500, lossRatio: 0 })).toBe(LATENCY_WARNING_BAR_COLOR)
  })
})

describe('getLossSquareColor', () => {
  it('returns muted for null loss', () => {
    expect(getLossSquareColor(null)).toBe(LATENCY_UNKNOWN_BAR_COLOR)
  })

  it('returns healthy when loss is below warning threshold', () => {
    expect(getLossSquareColor(0)).toBe(LATENCY_HEALTHY_BAR_COLOR)
    expect(getLossSquareColor(0.009)).toBe(LATENCY_HEALTHY_BAR_COLOR)
  })

  it('returns warning between warning and severe thresholds', () => {
    expect(getLossSquareColor(0.01)).toBe(LATENCY_WARNING_BAR_COLOR)
    expect(getLossSquareColor(0.049)).toBe(LATENCY_WARNING_BAR_COLOR)
  })

  it('returns failed at or above severe threshold', () => {
    expect(getLossSquareColor(0.05)).toBe(LATENCY_FAILED_BAR_COLOR)
    expect(getLossSquareColor(1)).toBe(LATENCY_FAILED_BAR_COLOR)
  })
})
```

Also extend the top imports in that file to include the new functions:

```ts
import {
  getCombinedBarColor,
  getCombinedSeverity,
  getLatencySquareColor,
  getLossSquareColor,
  LATENCY_FAILED_BAR_COLOR,
  LATENCY_HEALTHY_BAR_COLOR,
  LATENCY_UNKNOWN_BAR_COLOR,
  LATENCY_WARNING_BAR_COLOR,
  // ...keep existing imports
} from './network-latency-constants'
```

(If existing imports already cover the color constants, just add the two new function names.)

- [ ] **Step 2: Run tests to verify failure**

Run: `cd apps/web && bun run test -- --run src/lib/network-latency-constants.test.ts`
Expected: FAIL with `getLatencySquareColor is not a function` (or import error).

- [ ] **Step 3: Implement the new helpers**

Append to `apps/web/src/lib/network-latency-constants.ts`:

```ts
export function getLatencySquareColor({ latencyMs, lossRatio }: CombinedSeverityInput): string {
  if (lossRatio != null && lossRatio >= NETWORK_FAILURE_PACKET_LOSS_RATIO) {
    return LATENCY_FAILED_BAR_COLOR
  }
  if (latencyMs == null) {
    return LATENCY_UNKNOWN_BAR_COLOR
  }
  if (latencyMs < LATENCY_HEALTHY_THRESHOLD_MS) {
    return LATENCY_HEALTHY_BAR_COLOR
  }
  return LATENCY_WARNING_BAR_COLOR
}

export function getLossSquareColor(lossRatio: number | null | undefined): string {
  if (lossRatio == null) {
    return LATENCY_UNKNOWN_BAR_COLOR
  }
  if (lossRatio < LOSS_WARNING_THRESHOLD_RATIO) {
    return LATENCY_HEALTHY_BAR_COLOR
  }
  if (lossRatio < LOSS_SEVERE_THRESHOLD_RATIO) {
    return LATENCY_WARNING_BAR_COLOR
  }
  return LATENCY_FAILED_BAR_COLOR
}
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cd apps/web && bun run test -- --run src/lib/network-latency-constants.test.ts`
Expected: PASS (all original + 8 new test cases).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/lib/network-latency-constants.ts apps/web/src/lib/network-latency-constants.test.ts
git commit -m "feat(web): add latency/loss square color helpers"
```

---

## Task 3: 放宽 MAX_TREND_POINTS 至 30

**Files:**
- Modify: `apps/web/src/components/server/server-card-network-data.ts:3`
- Test: `apps/web/src/components/server/server-card-network-data.test.ts`

- [ ] **Step 1: Update existing test expectations from 12 to 30**

In `apps/web/src/components/server/server-card-network-data.test.ts`, replace every occurrence of `toHaveLength(12)` with `toHaveLength(30)`. Five lines should change:

```ts
// lines 95, 167, 205, 208 (and any others — search the file)
expect(state.latencyPoints).toHaveLength(30)
expect(state.lossPoints).toHaveLength(30)
```

Verify by running: `grep -n "toHaveLength(12)" apps/web/src/components/server/server-card-network-data.test.ts` — expected: no matches.

- [ ] **Step 2: Run tests to verify failure**

Run: `cd apps/web && bun run test -- --run src/components/server/server-card-network-data.test.ts`
Expected: FAIL — actual length is 12, expected 30.

- [ ] **Step 3: Update the constant**

In `apps/web/src/components/server/server-card-network-data.ts:3`:

```ts
const MAX_TREND_POINTS = 30
```

- [ ] **Step 4: Run tests to verify pass**

Run: `cd apps/web && bun run test -- --run src/components/server/server-card-network-data.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/server/server-card-network-data.ts apps/web/src/components/server/server-card-network-data.test.ts
git commit -m "refactor(web): widen ServerCard trend window from 12 to 30 points"
```

---

## Task 4: 给 RingChart 增加 compact 模式

**Files:**
- Modify: `apps/web/src/components/ui/ring-chart.tsx`

- [ ] **Step 1: Add compact prop**

Replace `apps/web/src/components/ui/ring-chart.tsx`:

```tsx
interface RingChartProps {
  color: string
  label: string
  size?: number
  strokeWidth?: number
  value: number
  compact?: boolean
}

const VIEWBOX = 36
const DEFAULT_SIZE = 56
const DEFAULT_STROKE = 3.5
const COMPACT_SIZE = 32
const COMPACT_STROKE = 4

export function RingChart({
  value,
  size,
  strokeWidth,
  color,
  label,
  compact = false
}: RingChartProps) {
  const resolvedSize = size ?? (compact ? COMPACT_SIZE : DEFAULT_SIZE)
  const resolvedStroke = strokeWidth ?? (compact ? COMPACT_STROKE : DEFAULT_STROKE)
  const clamped = Math.min(100, Math.max(0, value))
  const radius = (VIEWBOX - resolvedStroke) / 2
  const circumference = 2 * Math.PI * radius
  const dashArray = `${(clamped / 100) * circumference} ${circumference}`
  const labelFontSize = compact ? '9px' : '11px'

  return (
    <div style={{ width: resolvedSize }}>
      <div style={{ position: 'relative', width: resolvedSize, height: resolvedSize }}>
        <svg
          aria-label={`${label} ${clamped.toFixed(1)}%`}
          height={resolvedSize}
          role="img"
          style={{ transform: 'rotate(-90deg)' }}
          viewBox={`0 0 ${VIEWBOX} ${VIEWBOX}`}
          width={resolvedSize}
        >
          <circle
            cx={VIEWBOX / 2}
            cy={VIEWBOX / 2}
            fill="none"
            r={radius}
            stroke="rgba(128,128,128,0.15)"
            strokeWidth={resolvedStroke}
          />
          <circle
            cx={VIEWBOX / 2}
            cy={VIEWBOX / 2}
            fill="none"
            r={radius}
            stroke={color}
            strokeDasharray={dashArray}
            strokeLinecap="round"
            strokeWidth={resolvedStroke}
          />
        </svg>
        <div
          style={{
            position: 'absolute',
            inset: 0,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            fontSize: labelFontSize,
            fontWeight: 700
          }}
        >
          {clamped.toFixed(0)}%
        </div>
      </div>
      {!compact && <div className="mt-0.5 text-center text-[10px] text-muted-foreground">{label}</div>}
    </div>
  )
}
```

Note: in compact mode the label is rendered by the parent (`RingMetric` in Task 7) and the inner text drops the decimal for legibility at 32px.

- [ ] **Step 2: Run typecheck and tests for files using RingChart**

Run: `cd apps/web && bun run test -- --run src/components/server`
Expected: PASS (RingChart props are backwards compatible).

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/ui/ring-chart.tsx
git commit -m "feat(web): add compact mode to RingChart"
```

---

## Task 5: 创建 NetworkSquareGrid 组件

**Files:**
- Create: `apps/web/src/components/server/network-square-grid.tsx`
- Test: `apps/web/src/components/server/network-square-grid.test.tsx`

- [ ] **Step 1: Write failing test**

Create `apps/web/src/components/server/network-square-grid.test.tsx`:

```tsx
import { render } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/components/ui/tooltip'
import type { ServerCardMetricPoint } from './server-card-network-data'
import { NetworkSquareGrid } from './network-square-grid'

vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key: string) => key
  })
}))

type ObserveCallback = (entries: Array<{ contentRect: { width: number } }>) => void

const observers: ObserveCallback[] = []

class TestResizeObserver {
  constructor(private cb: ObserveCallback) {
    observers.push(cb)
  }
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

function makePoint(value: number, lossRatio = 0): ServerCardMetricPoint {
  return {
    synthetic: false,
    targets: [
      {
        latency: value,
        lossRatio,
        targetId: 't1',
        targetName: 'Tokyo'
      }
    ],
    timestamp: new Date().toISOString(),
    value
  }
}

describe('NetworkSquareGrid', () => {
  afterEach(() => {
    observers.length = 0
  })

  it('renders no more squares than the container can fit', () => {
    // @ts-expect-error inject mock
    globalThis.ResizeObserver = TestResizeObserver

    const points = Array.from({ length: 30 }, (_, i) => makePoint(50 + i))

    const { container } = render(
      <TooltipProvider>
        <NetworkSquareGrid kind="latency" points={points} />
      </TooltipProvider>
    )

    // Simulate a container width of 80px → fits floor((80 + 2) / 8) = 10 squares.
    observers[0]?.([{ contentRect: { width: 80 } }])

    const squares = container.querySelectorAll('[data-testid="square"]')
    expect(squares.length).toBe(10)
  })

  it('renders at least one square even at zero width', () => {
    // @ts-expect-error inject mock
    globalThis.ResizeObserver = TestResizeObserver

    const points = [makePoint(50)]

    const { container } = render(
      <TooltipProvider>
        <NetworkSquareGrid kind="loss" points={points} />
      </TooltipProvider>
    )

    const squares = container.querySelectorAll('[data-testid="square"]')
    expect(squares.length).toBe(1)
  })
})
```

- [ ] **Step 2: Run test to verify failure**

Run: `cd apps/web && bun run test -- --run src/components/server/network-square-grid.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement component**

Create `apps/web/src/components/server/network-square-grid.tsx`:

```tsx
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import {
  getLatencySquareColor,
  getLossSquareColor,
  isLatencyFailure,
  LATENCY_UNKNOWN_BAR_COLOR
} from '@/lib/network-latency-constants'
import { latencyColorClass } from '@/lib/network-types'
import type { ServerCardMetricPoint } from './server-card-network-data'

const SQUARE_SIZE = 6
const SQUARE_GAP = 2
const STEP = SQUARE_SIZE + SQUARE_GAP

interface NetworkSquareGridProps {
  kind: 'latency' | 'loss'
  points: readonly ServerCardMetricPoint[]
}

function averageLossRatio(point: ServerCardMetricPoint): number | null {
  if (point.targets.length === 0) {
    return null
  }
  return point.targets.reduce((sum, target) => sum + target.lossRatio, 0) / point.targets.length
}

function getSquareColor(point: ServerCardMetricPoint, kind: 'latency' | 'loss'): string {
  if (point.synthetic) {
    return LATENCY_UNKNOWN_BAR_COLOR
  }
  if (kind === 'latency') {
    return getLatencySquareColor({ latencyMs: point.value, lossRatio: averageLossRatio(point) })
  }
  return getLossSquareColor(point.value)
}

function formatLatency(ms: number | null): string {
  if (ms == null) {
    return '-'
  }
  return `${ms.toFixed(0)}ms`
}

function formatPacketLoss(lossRatio: number | null): string {
  if (lossRatio == null) {
    return '-'
  }
  return `${(lossRatio * 100).toFixed(1)}%`
}

function formatTooltipLabel(point: ServerCardMetricPoint, t: (key: string) => string): string {
  if (point.synthetic) {
    return t('current_targets')
  }
  return new Date(point.timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false
  })
}

function getLossTextClassName(lossRatio: number | null): string {
  if (lossRatio == null) {
    return 'text-muted-foreground'
  }
  if (lossRatio < 0.01) {
    return 'text-emerald-600 dark:text-emerald-400'
  }
  if (lossRatio < 0.05) {
    return 'text-amber-600 dark:text-amber-400'
  }
  return 'text-red-600 dark:text-red-400'
}

function PointTooltip({ point, t }: { point: ServerCardMetricPoint; t: (key: string) => string }) {
  if (point.targets.length === 0) {
    return null
  }
  return (
    <div className="grid min-w-48 gap-1.5 text-xs">
      <div className="font-medium">{formatTooltipLabel(point, t)}</div>
      <div className="grid gap-1.5">
        {point.targets.map((target) => {
          const failed = isLatencyFailure(target.lossRatio)
          return (
            <div className="flex items-center justify-between gap-3" key={target.targetId}>
              <span className="truncate text-muted-foreground">{target.targetName}</span>
              <div className="flex gap-2 font-medium font-mono tabular-nums">
                <span className={latencyColorClass(target.latency, { failed })}>{formatLatency(target.latency)}</span>
                <span className={getLossTextClassName(target.lossRatio)}>{formatPacketLoss(target.lossRatio)}</span>
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

export function NetworkSquareGrid({ points, kind }: NetworkSquareGridProps) {
  const { t } = useTranslation(['servers'])
  const containerRef = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(0)

  useEffect(() => {
    const el = containerRef.current
    if (!el) {
      return
    }
    const observer = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width ?? 0
      setWidth(w)
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  const maxSquares = Math.max(1, Math.floor((width + SQUARE_GAP) / STEP))
  const visible = points.slice(-maxSquares)

  return (
    <div
      className="flex h-3 w-full items-end overflow-hidden"
      ref={containerRef}
      style={{ gap: `${SQUARE_GAP}px` }}
    >
      {visible.map((point, index) => (
        <Tooltip key={`${point.timestamp}-${index}`}>
          <TooltipTrigger asChild>
            <div
              className="flex-none rounded-[1px]"
              data-testid="square"
              style={{
                backgroundColor: getSquareColor(point, kind),
                height: `${SQUARE_SIZE}px`,
                width: `${SQUARE_SIZE}px`
              }}
            />
          </TooltipTrigger>
          <TooltipContent
            className="grid min-w-48 gap-1.5 rounded-lg border border-border/50 bg-background/95 px-3 py-2 text-xs shadow-xl backdrop-blur-sm"
            sideOffset={4}
          >
            <PointTooltip point={point} t={t} />
          </TooltipContent>
        </Tooltip>
      ))}
    </div>
  )
}
```

- [ ] **Step 4: Run test to verify pass**

Run: `cd apps/web && bun run test -- --run src/components/server/network-square-grid.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/server/network-square-grid.tsx apps/web/src/components/server/network-square-grid.test.tsx
git commit -m "feat(web): add NetworkSquareGrid with dynamic point fitting"
```

---

## Task 6: 创建 TagChips 组件

**Files:**
- Create: `apps/web/src/components/server/tag-chips.tsx`
- Test: `apps/web/src/components/server/tag-chips.test.tsx`

- [ ] **Step 1: Write failing test**

Create `apps/web/src/components/server/tag-chips.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { TagChips } from './tag-chips'

describe('TagChips', () => {
  it('renders nothing when tags is undefined', () => {
    const { container } = render(<TagChips tags={undefined} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders nothing when tags is empty', () => {
    const { container } = render(<TagChips tags={[]} />)
    expect(container.firstChild).toBeNull()
  })

  it('renders each tag as a chip', () => {
    render(<TagChips tags={['CN2 GIA', 'AS9929', 'CMI']} />)
    expect(screen.getByText('CN2 GIA')).toBeDefined()
    expect(screen.getByText('AS9929')).toBeDefined()
    expect(screen.getByText('CMI')).toBeDefined()
  })
})
```

- [ ] **Step 2: Run test to verify failure**

Run: `cd apps/web && bun run test -- --run src/components/server/tag-chips.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement component**

Create `apps/web/src/components/server/tag-chips.tsx`:

```tsx
interface TagChipsProps {
  tags: readonly string[] | undefined
}

export function TagChips({ tags }: TagChipsProps) {
  if (!tags || tags.length === 0) {
    return null
  }
  return (
    <div className="flex flex-wrap gap-1">
      {tags.map((tag) => (
        <span
          className="rounded-md border border-emerald-500/30 bg-emerald-500/5 px-1.5 py-0.5 text-[10px] text-emerald-700 dark:text-emerald-300"
          key={tag}
        >
          {tag}
        </span>
      ))}
    </div>
  )
}
```

- [ ] **Step 4: Run test to verify pass**

Run: `cd apps/web && bun run test -- --run src/components/server/tag-chips.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/server/tag-chips.tsx apps/web/src/components/server/tag-chips.test.tsx
git commit -m "feat(web): add TagChips for rendering server tags"
```

---

## Task 7: 新增 card_latency 翻译 key

**Files:**
- Modify: `apps/web/src/locales/zh/servers.json`
- Modify: `apps/web/src/locales/en/servers.json`

- [ ] **Step 1: Add `card_latency` to zh locale**

In `apps/web/src/locales/zh/servers.json`, add the key alphabetically near `card_packet_loss`:

```json
"card_latency": "延迟",
```

- [ ] **Step 2: Add `card_latency` to en locale**

In `apps/web/src/locales/en/servers.json`:

```json
"card_latency": "Latency",
```

- [ ] **Step 3: Verify JSON validity**

Run: `cd apps/web && bun run typecheck`
Expected: PASS (no JSON parse errors).

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/locales/zh/servers.json apps/web/src/locales/en/servers.json
git commit -m "i18n(web): add card_latency translation key"
```

---

## Task 8: 重构 ServerCard 主体布局

**Files:**
- Modify: `apps/web/src/components/server/server-card.tsx`
- Modify: `apps/web/src/components/server/server-card.test.tsx`

This task replaces the layout and removes SeverityBar/BarChart usage. Tests are updated alongside.

- [ ] **Step 1: Update tests for new structure**

Replace `apps/web/src/components/server/server-card.test.tsx` (overwriting the file). Keep the existing imports, mocks, and `makeServer` / `makeSummary` helpers, but adjust the test body. Below is the full new file content:

```tsx
import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { CostOverviewResponse, ServerCostOverview } from '@/lib/api-schema'
import type { NetworkServerSummary } from '@/lib/network-types'
import { TooltipProvider } from '@/components/ui/tooltip'
import { CostFootnote } from './cost-footnote'
import { ServerCard } from './server-card'

const REGEX_COST_PER_HOUR = /0\.01\/h/

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

const mockNetworkOverview = vi.fn()
const mockNetworkRealtime = vi.fn()
const mockTrafficOverview = vi.fn()
const mockCostOverview = vi.fn()
vi.mock('@/hooks/use-cost', () => ({
  useCostOverview: (...args: unknown[]) => mockCostOverview(...args)
}))
vi.mock('@/hooks/use-network-api', () => ({
  useNetworkOverview: (...args: unknown[]) => mockNetworkOverview(...args)
}))
vi.mock('@/hooks/use-network-realtime', () => ({
  useNetworkRealtime: (...args: unknown[]) => mockNetworkRealtime(...args)
}))
vi.mock('@/hooks/use-traffic-overview', () => ({
  useTrafficOverview: (...args: unknown[]) => mockTrafficOverview(...args)
}))

function renderCard(server: Parameters<typeof ServerCard>[0]['server']) {
  return render(
    <TooltipProvider>
      <ServerCard server={server} />
    </TooltipProvider>
  )
}

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
    disk_read_bytes_per_sec: 0,
    disk_write_bytes_per_sec: 0,
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
    tags: undefined,
    ...overrides
  }
}

function makeSummary(overrides: Partial<NetworkServerSummary> = {}): NetworkServerSummary {
  return {
    anomaly_count: 0,
    last_probe_at: null,
    latency_sparkline: [],
    loss_sparkline: [],
    online: true,
    server_id: 'srv-1',
    server_name: 'test-server',
    targets: [],
    ...overrides
  }
}

describe('ServerCard', () => {
  beforeEach(() => {
    mockNetworkOverview.mockReturnValue({ data: [] })
    mockNetworkRealtime.mockReturnValue({ data: {} })
    mockTrafficOverview.mockReturnValue({ data: [] })
    mockCostOverview.mockReturnValue({ data: { currencies: [], servers: [] } satisfies CostOverviewResponse })
  })

  it('renders server name', () => {
    renderCard(makeServer())
    expect(screen.getByText('test-server')).toBeDefined()
  })

  it('renders four ring charts with CPU, Memory, Disk, Traffic labels', () => {
    renderCard(makeServer())
    expect(screen.getByText('col_cpu')).toBeDefined()
    expect(screen.getByText('col_memory')).toBeDefined()
    expect(screen.getByText('col_disk')).toBeDefined()
    expect(screen.getByText('card_traffic_quota')).toBeDefined()
  })

  it('renders footnote secondary metrics', () => {
    renderCard(makeServer())
    expect(screen.getByText('col_uptime')).toBeDefined()
    expect(screen.getByText('card_swap')).toBeDefined()
    expect(screen.getByText('card_processes')).toBeDefined()
    expect(screen.getByText('card_tcp')).toBeDefined()
    expect(screen.getByText('card_udp')).toBeDefined()
  })

  it('renders compact cost footnote when cost overview is available', () => {
    mockCostOverview.mockReturnValue({
      data: {
        currencies: [],
        servers: [
          {
            configured: true,
            cost_per_hour: 0.01,
            currency: 'USD',
            name: 'test-server',
            server_id: 'srv-1',
            value_score: {
              confidence: 'high',
              grade: 'good',
              reasons: [],
              score: 82
            }
          }
        ]
      } satisfies CostOverviewResponse
    })

    renderCard(makeServer())

    expect(screen.getByText(REGEX_COST_PER_HOUR)).toBeDefined()
    expect(screen.getByText('cost_grade_good')).toBeDefined()
    expect(screen.queryByText('82')).toBeNull()
  })

  it('renders compact unconfigured cost footnote labels', () => {
    const missingPrice = {
      configured: false,
      invalid_reason: 'missing_price',
      name: 'test-server',
      server_id: 'srv-1'
    } satisfies ServerCostOverview
    const missingCycle = {
      configured: false,
      invalid_reason: 'missing_billing_cycle',
      name: 'test-server',
      server_id: 'srv-1'
    } satisfies ServerCostOverview
    const invalidPrice = {
      configured: false,
      invalid_reason: 'invalid_price',
      name: 'test-server',
      server_id: 'srv-1'
    } satisfies ServerCostOverview

    const { rerender } = render(<CostFootnote entry={missingPrice} />)
    expect(screen.getByText('cost_not_set')).toBeDefined()

    rerender(<CostFootnote entry={missingCycle} />)
    expect(screen.getByText('cost_price_only')).toBeDefined()

    rerender(<CostFootnote entry={invalidPrice} />)
    expect(screen.getByText('cost_invalid')).toBeDefined()
  })

  it('renders network and disk I/O rates with load trend', () => {
    renderCard(makeServer())
    expect(screen.getByText('card_net_in_speed')).toBeDefined()
    expect(screen.getByText('card_net_out_speed')).toBeDefined()
    expect(screen.getByLabelText('card_disk_read')).toBeDefined()
    expect(screen.getByLabelText('card_disk_write')).toBeDefined()
    expect(screen.getByText('card_load_trend')).toBeDefined()
  })

  it('does not render network quality section when no data', () => {
    renderCard(makeServer())
    expect(screen.queryByText('card_latency')).toBeNull()
    expect(screen.queryByText('card_packet_loss')).toBeNull()
  })

  it('renders latency and loss square grids when network data is present', () => {
    mockNetworkOverview.mockReturnValue({
      data: [
        makeSummary({
          targets: [
            {
              availability: 0.99,
              avg_latency: 40,
              max_latency: 45,
              min_latency: 35,
              packet_loss: 0.01,
              provider: 'ct',
              target_id: 'target-1',
              target_name: 'Shanghai Telecom'
            }
          ]
        })
      ]
    })

    renderCard(makeServer())

    expect(screen.getByText('card_latency')).toBeDefined()
    expect(screen.getByText('card_packet_loss')).toBeDefined()
  })

  it('renders tag chips when server.tags is non-empty', () => {
    renderCard(makeServer({ tags: ['CN2 GIA', 'AS9929'] }))
    expect(screen.getByText('CN2 GIA')).toBeDefined()
    expect(screen.getByText('AS9929')).toBeDefined()
  })

  it('does not render tag chip container when tags is empty', () => {
    renderCard(makeServer({ tags: [] }))
    expect(screen.queryByText('CN2 GIA')).toBeNull()
  })

  it('renders StatusBadge', () => {
    renderCard(makeServer({ online: false }))
    expect(screen.getByText('offline')).toBeDefined()
  })
})
```

- [ ] **Step 2: Run test to verify failure**

Run: `cd apps/web && bun run test -- --run src/components/server/server-card.test.tsx`
Expected: FAIL — old `ServerCard` does not have `card_latency` text, no tag chip support, etc.

- [ ] **Step 3: Replace ServerCard implementation**

Overwrite `apps/web/src/components/server/server-card.tsx`:

```tsx
import { Link } from '@tanstack/react-router'
import { memo, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { CompactMetric } from '@/components/server/compact-metric'
import { RingChart } from '@/components/ui/ring-chart'
import { useCostOverview } from '@/hooks/use-cost'
import { useNetworkOverview } from '@/hooks/use-network-api'
import { useNetworkRealtime } from '@/hooks/use-network-realtime'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { useTrafficOverview } from '@/hooks/use-traffic-overview'
import { isLatencyFailure } from '@/lib/network-latency-constants'
import { latencyColorClass } from '@/lib/network-types'
import { computeTrafficQuota } from '@/lib/traffic'
import { countryCodeToFlag, formatBytes, formatSpeed, formatUptime } from '@/lib/utils'
import { useUpgradeJobsStore } from '@/stores/upgrade-jobs-store'
import { CostFootnote } from './cost-footnote'
import { NetworkSquareGrid } from './network-square-grid'
import { buildServerCardNetworkState } from './server-card-network-data'
import { StatusBadge } from './status-badge'
import { TagChips } from './tag-chips'
import { UpgradeJobBadge } from './upgrade-job-badge'

interface ServerCardProps {
  server: ServerMetrics
}

function osIcon(os: string | null): string {
  if (!os) {
    return ''
  }
  const lower = os.toLowerCase()
  if (lower.includes('ubuntu') || lower.includes('debian') || lower.includes('linux')) {
    return '🐧'
  }
  if (lower.includes('windows')) {
    return '🪟'
  }
  if (lower.includes('macos') || lower.includes('darwin')) {
    return '🍎'
  }
  if (lower.includes('freebsd') || lower.includes('openbsd')) {
    return '😈'
  }
  return ''
}

function getRingColor(pct: number, brandColor: string): string {
  if (pct > 90) {
    return '#ef4444'
  }
  if (pct > 70) {
    return '#f59e0b'
  }
  return brandColor
}

function getLossTextClassName(lossRatio: number | null): string {
  if (lossRatio == null) {
    return 'text-muted-foreground'
  }
  if (lossRatio < 0.01) {
    return 'text-emerald-600 dark:text-emerald-400'
  }
  if (lossRatio < 0.05) {
    return 'text-amber-600 dark:text-amber-400'
  }
  return 'text-red-600 dark:text-red-400'
}

function formatLatency(ms: number | null): string {
  if (ms == null) {
    return '—'
  }
  return `${ms.toFixed(0)}`
}

function formatPacketLoss(lossRatio: number | null): string {
  if (lossRatio == null) {
    return '—'
  }
  return `${(lossRatio * 100).toFixed(1)}%`
}

function formatLoad(load: number): string {
  return load.toFixed(2)
}

function renderSpeedValue(bytesPerSec: number): React.ReactNode {
  if (bytesPerSec <= 0) {
    return '0'
  }
  const formatted = formatSpeed(bytesPerSec)
  const lastSpace = formatted.lastIndexOf(' ')
  if (lastSpace < 0) {
    return formatted
  }
  return (
    <>
      {formatted.slice(0, lastSpace)}
      <span className="ml-0.5 font-normal text-[10px] text-muted-foreground">{formatted.slice(lastSpace + 1)}</span>
    </>
  )
}

interface RingMetricProps {
  color: string
  label: string
  subText: React.ReactNode
  value: number
}

function RingMetric({ color, label, subText, value }: RingMetricProps) {
  return (
    <div className="flex items-center gap-2">
      <RingChart color={color} compact label={label} value={value} />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-[11px] text-muted-foreground">{label}</span>
        <span className="truncate text-[10px] text-muted-foreground tabular-nums">{subText}</span>
      </div>
    </div>
  )
}

const ServerCardInner = ({ server }: ServerCardProps) => {
  const { t } = useTranslation(['servers'])
  const { data: networkOverview = [] } = useNetworkOverview()
  const { data: realtimeData } = useNetworkRealtime(server.id)
  const { data: trafficOverview } = useTrafficOverview()
  const { data: costOverview } = useCostOverview()
  const upgradeJob = useUpgradeJobsStore((state) => state.jobs.get(server.id))

  const memoryPct = server.mem_total > 0 ? (server.mem_used / server.mem_total) * 100 : 0
  const diskPct = server.disk_total > 0 ? (server.disk_used / server.disk_total) * 100 : 0
  const swapPct = server.swap_total > 0 ? (server.swap_used / server.swap_total) * 100 : 0
  const flag = countryCodeToFlag(server.country_code)
  const osEmoji = osIcon(server.os)

  const networkSummary = networkOverview.find((entry) => entry.server_id === server.id)
  const { currentAvgLatency, currentAvgLossRatio, latencyPoints, lossPoints } = useMemo(
    () => buildServerCardNetworkState(networkSummary, realtimeData),
    [networkSummary, realtimeData]
  )

  const hasNetworkData = latencyPoints.length > 0

  const trafficEntry = trafficOverview?.find((entry) => entry.server_id === server.id)
  const {
    used: trafficUsed,
    limit: trafficLimit,
    pct: trafficRingPct
  } = computeTrafficQuota({
    entry: trafficEntry,
    netInTransfer: server.net_in_transfer,
    netOutTransfer: server.net_out_transfer
  })
  const trafficDaysRemaining = trafficEntry?.days_remaining ?? null
  const costEntry = costOverview?.servers.find((entry) => entry.server_id === server.id)

  return (
    <div className="flex w-full min-w-[320px] max-w-[480px] flex-col gap-2 rounded-lg border bg-card p-3 shadow-sm">
      <div className="flex items-center justify-between">
        <Link
          className="flex items-center gap-1 truncate border-transparent border-b pb-px hover:border-current"
          params={{ id: server.id }}
          search={{ range: 'realtime' }}
          to="/servers/$id"
        >
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
          <h3 className="truncate font-semibold text-[13px]">{server.name}</h3>
        </Link>
        <div className="flex items-center gap-1.5">
          <UpgradeJobBadge job={upgradeJob} />
          <StatusBadge online={server.online} />
        </div>
      </div>

      <div className="grid grid-cols-2 gap-x-3 gap-y-2">
        <RingMetric
          color={getRingColor(server.cpu, 'var(--color-chart-1)')}
          label={t('col_cpu')}
          subText={
            <>
              {t('card_load')} <span className="font-medium text-foreground">{formatLoad(server.load1)}</span>
            </>
          }
          value={server.cpu}
        />
        <RingMetric
          color={getRingColor(memoryPct, 'var(--color-chart-2)')}
          label={t('col_memory')}
          subText={
            <>
              <span className="font-medium text-foreground">{formatBytes(server.mem_used)}</span>
              <span className="mx-0.5">/</span>
              {formatBytes(server.mem_total)}
            </>
          }
          value={memoryPct}
        />
        <RingMetric
          color={getRingColor(diskPct, 'var(--color-chart-3)')}
          label={t('col_disk')}
          subText={
            <>
              <span className="font-medium text-foreground">{formatBytes(server.disk_used)}</span>
              <span className="mx-0.5">/</span>
              {formatBytes(server.disk_total)}
            </>
          }
          value={diskPct}
        />
        <RingMetric
          color={getRingColor(trafficRingPct, 'var(--color-chart-4)')}
          label={t('card_traffic_quota')}
          subText={
            <>
              <span className="font-medium text-foreground">{formatBytes(trafficUsed)}</span>
              <span className="mx-0.5">/</span>
              {formatBytes(trafficLimit)}
            </>
          }
          value={trafficRingPct}
        />
      </div>

      <div className="grid grid-cols-2 gap-x-3 gap-y-1 rounded-md bg-muted/40 px-2 py-1.5">
        <CompactMetric label={t('card_net_in_speed')} value={renderSpeedValue(server.net_in_speed)} />
        <CompactMetric label={t('card_net_out_speed')} value={renderSpeedValue(server.net_out_speed)} />
        <CompactMetric
          label={
            <span
              aria-label={t('card_disk_read')}
              className="inline-flex size-3 flex-none items-center justify-center rounded-sm bg-muted font-semibold text-[9px] text-foreground leading-none"
              role="img"
            >
              R
            </span>
          }
          value={renderSpeedValue(server.disk_read_bytes_per_sec)}
        />
        <CompactMetric
          label={
            <span
              aria-label={t('card_disk_write')}
              className="inline-flex size-3 flex-none items-center justify-center rounded-sm bg-muted font-semibold text-[9px] text-foreground leading-none"
              role="img"
            >
              W
            </span>
          }
          value={renderSpeedValue(server.disk_write_bytes_per_sec)}
        />
        <CompactMetric label={t('card_load_trend')} value={`${formatLoad(server.load5)}·${formatLoad(server.load15)}`} />
      </div>

      {hasNetworkData && (
        <div className="grid grid-cols-2 gap-x-3 gap-y-1">
          <div className="flex items-baseline justify-between">
            <span className="text-[11px] text-muted-foreground">{t('card_latency')}</span>
            <span
              className={`font-semibold text-xs tabular-nums ${latencyColorClass(currentAvgLatency, {
                failed: isLatencyFailure(currentAvgLossRatio)
              })}`}
            >
              {formatLatency(currentAvgLatency)}
              <span className="ml-0.5 font-medium text-[10px] text-muted-foreground">ms</span>
            </span>
          </div>
          <div className="flex items-baseline justify-between">
            <span className="text-[11px] text-muted-foreground">{t('card_packet_loss')}</span>
            <span className={`font-semibold text-xs tabular-nums ${getLossTextClassName(currentAvgLossRatio)}`}>
              {formatPacketLoss(currentAvgLossRatio)}
            </span>
          </div>
          <NetworkSquareGrid kind="latency" points={latencyPoints} />
          <NetworkSquareGrid kind="loss" points={lossPoints} />
        </div>
      )}

      <div className="flex flex-wrap items-center justify-center gap-x-3 gap-y-0.5 text-[10px] text-muted-foreground">
        <span>
          {t('col_uptime')}{' '}
          <span className="font-medium text-foreground tabular-nums">{formatUptime(server.uptime)}</span>
        </span>
        <span aria-hidden="true">·</span>
        <span>
          {t('card_swap')} <span className="font-medium text-foreground tabular-nums">{`${swapPct.toFixed(0)}%`}</span>
        </span>
        <span aria-hidden="true">·</span>
        <span>
          {t('card_processes')} <span className="font-medium text-foreground tabular-nums">{server.process_count}</span>
        </span>
        <span aria-hidden="true">·</span>
        <span>
          {t('card_tcp')} <span className="font-medium text-foreground tabular-nums">{server.tcp_conn}</span>
        </span>
        <span aria-hidden="true">·</span>
        <span>
          {t('card_udp')} <span className="font-medium text-foreground tabular-nums">{server.udp_conn}</span>
        </span>
        {trafficDaysRemaining != null && (
          <>
            <span aria-hidden="true">·</span>
            <span className="tabular-nums">{t('card_traffic_days_left', { count: trafficDaysRemaining })}</span>
          </>
        )}
        <CostFootnote entry={costEntry} />
      </div>

      <TagChips tags={server.tags} />
    </div>
  )
}

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
    a.swap_total === b.swap_total &&
    a.tags === b.tags
  )
})
```

Note: this removes the `SeverityBar`/`BarChart`/`recharts` imports. `figure[aria-label="common:a11y.latency_trend"]` is no longer rendered. The two old assertions in the previous test (`findHeroLatency`, `latency_trend` aria) are removed in the new test file in Step 1.

- [ ] **Step 4: Run ServerCard tests to verify pass**

Run: `cd apps/web && bun run test -- --run src/components/server/server-card.test.tsx`
Expected: PASS.

- [ ] **Step 5: Run the full ServerCard-related test suite**

Run: `cd apps/web && bun run test -- --run src/components/server`
Expected: PASS — including `network-square-grid.test.tsx`, `tag-chips.test.tsx`, `server-card-network-data.test.ts`.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/server/server-card.tsx apps/web/src/components/server/server-card.test.tsx
git commit -m "feat(web): redesign ServerCard with 2x2 rings and side-by-side network grids"
```

---

## Task 9: ServerCardsWidget 切换到响应式 grid

**Files:**
- Modify: `apps/web/src/components/dashboard/widgets/server-cards.tsx`

- [ ] **Step 1: Replace gridTemplateColumns logic**

Overwrite `apps/web/src/components/dashboard/widgets/server-cards.tsx`:

```tsx
import { useMemo } from 'react'
import { ServerCard } from '@/components/server/server-card'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { filterByIds } from '@/lib/widget-helpers'
import type { ServerCardsConfig } from '@/lib/widget-types'

interface ServerCardsWidgetProps {
  config: ServerCardsConfig
  servers: ServerMetrics[]
}

export function ServerCardsWidget({ config, servers }: ServerCardsWidgetProps) {
  const filtered = useMemo(() => filterByIds(servers, config.server_ids, (s) => s.id), [servers, config.server_ids])

  return (
    <div
      className="grid h-full content-start gap-4 overflow-auto"
      style={{
        gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))'
      }}
    >
      {filtered.map((server) => (
        <ServerCard key={server.id} server={server} />
      ))}
      {filtered.length === 0 && (
        <div className="col-span-full flex items-center justify-center py-8 text-muted-foreground text-sm">
          No servers to display
        </div>
      )}
    </div>
  )
}
```

Notes: `config.columns` is intentionally ignored — the card width range (320–480px enforced by ServerCard's own classes) now governs column count.

- [ ] **Step 2: Run widget tests**

Run: `cd apps/web && bun run test -- --run src/components/dashboard`
Expected: PASS (the existing widget-renderer test mocks `ServerCardsWidget`, so internal changes don't break it).

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/dashboard/widgets/server-cards.tsx
git commit -m "feat(web): make server-cards widget responsive via auto-fill grid"
```

---

## Task 10: 删除 ServerCard 中遗留的 severity bar 引用残骸

After Task 8, the `severity-bar.tsx` file still exists but is no longer imported anywhere except potentially leftover unused imports/tests. Confirm and clean up.

**Files:**
- Verify: `apps/web/src/components/server/severity-bar.tsx`

- [ ] **Step 1: Search for any remaining usage**

Run: `cd apps/web && grep -rn "severity-bar\|SeverityBar" src/`
Expected: Output should NOT contain any production imports. If the only matches are within `severity-bar.tsx` itself or test fixtures that are also unused, proceed; otherwise resolve before continuing.

- [ ] **Step 2: Leave the file in place (per spec)**

Per the spec ("SeverityBar 与 BarChart 退役: 不再被 ServerCard 引用。组件文件保留..."), we do not delete `severity-bar.tsx` in this plan. No code changes here. Skip the commit step for this task.

- [ ] **Step 3: (Optional) Note follow-up cleanup**

No action — this task is purely verification.

---

## Task 11: 端到端验证 (typecheck + lint + tests + visual)

**Files:** none

- [ ] **Step 1: TypeScript typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: 0 errors.

- [ ] **Step 2: Lint**

Run: `cd apps/web && bun x ultracite check`
Expected: 0 issues. If any auto-fixable warnings appear, run `bun x ultracite fix` and commit as `style(web): apply ultracite fixes`.

- [ ] **Step 3: Full frontend test suite**

Run: `cd apps/web && bun run test -- --run`
Expected: All tests pass.

- [ ] **Step 4: Build sanity check**

Run: `cd apps/web && bun run build`
Expected: Build succeeds.

- [ ] **Step 5: Visual verification in dev server**

Run: `cd apps/web && bun run dev` (or `make web-dev-prod` for production data per CLAUDE.md).
Open the dashboard at `http://localhost:5173/` and verify in three viewports (1280, 1562, 1920):

  - The cards wrap into 3+ columns at 1562px (was 3 before, now ≥3 depending on width).
  - At narrow widths (≤640px), one column.
  - Each card stays within 320–480px.
  - 2×2 RingChart renders, latency/loss labels show.
  - Network square grids fill horizontally; resizing window changes the visible square count without overflow.
  - Servers with `tags` show colored chips below the misc row; servers without `tags` show no chip row.
  - Offline servers still show the offline badge correctly.

- [ ] **Step 6: Final commit (only if optional fix-up was needed)**

If `ultracite fix` produced changes, commit them. Otherwise nothing to commit here.

---

## Self-Review Notes

**Spec coverage check:**
- ✅ 双列 2×2 主指标 — Task 8 (`RingMetric` + grid-cols-2)
- ✅ 左右并排方块栅格 — Task 5 + Task 8
- ✅ 卡片 320–480px 宽度限制 — Task 8 (`min-w-[320px] max-w-[480px]`)
- ✅ `server.tags` 渲染 — Task 6 + Task 8
- ✅ 外层 grid auto-fill — Task 9
- ✅ `MAX_TREND_POINTS` 12→30 — Task 3
- ✅ 拆分 latency/loss color helpers — Task 2
- ✅ RingChart compact 模式 — Task 4
- ✅ ResizeObserver polyfill 测试 setup — Task 1
- ✅ `card_latency` i18n — Task 7
- ✅ SeverityBar 不再使用,文件保留 — Task 8 / Task 10
- ✅ Tests updated for new structure — Task 8 step 1
- ✅ Verification (typecheck / lint / build / visual) — Task 11

**Not in scope (per spec):**
- IPv4/IPv6 badge
- ISP tag 自动识别
- Widget config dialog columns 字段废弃
- 删除 SeverityBar 文件

No placeholders or TBDs. Every step has either exact code or an exact command with expected output.
