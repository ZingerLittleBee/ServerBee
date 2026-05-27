# Dashboard Widget Sizing Strategy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace scattered widget sizing markers (`SQUARE_TYPES`, `AUTO_HEIGHT_TYPES`, inline `visualSquareHFine`, `interactionStateRef`) with an explicit `sizing` field on each widget type, backed by a two-layer dispatcher: render-time `normalizeRenderItem` for idle h, and RGL v2 `LayoutConstraint`s for resize-time enforcement.

**Architecture:** Two layers. Layer A (`normalizeRenderItem`) runs in `baseLayout` to set fine-unit h / minH / maxH for aspect-square and content-height widgets at idle. Layer B attaches `LayoutConstraint`s to each `LayoutItem` so RGL's resize pipeline (`applySizeConstraints`) keeps the cell consistent during drag. Persistence and unit boundaries are explicit: only `baseLayout` does `h *= SCALE` (coarse → fine) and only `commitLayoutChange` does `Math.round(h / SCALE)` (fine → coarse).

**Tech Stack:** TypeScript, React 19, react-grid-layout v2.2.2 (subpath `react-grid-layout/core` for `aspectRatio` / `LayoutConstraint`), Vitest, Tailwind v4 container queries.

**Spec:** `docs/superpowers/specs/2026-05-28-dashboard-widget-sizing-strategy-design.md`

---

## Task 1: Extract grid constants into a shared module

**Files:**
- Create: `apps/web/src/components/dashboard/grid-constants.ts`
- Modify: `apps/web/src/components/dashboard/dashboard-grid.tsx` (replace local constant declarations with imports)

Pure refactor — no behavior change. We need these constants from two places (`dashboard-grid.tsx` and `sizing-strategies/normalize.ts`); moving them avoids a future circular import.

- [ ] **Step 1: Create `apps/web/src/components/dashboard/grid-constants.ts`**

```ts
// Grid layout primitives shared between dashboard-grid.tsx and sizing-strategies/.
// Persisted (coarse) grid rows are split into SCALE finer rows so content-sized
// widgets quantize to ~ROW_HEIGHT px instead of a whole coarse row.
// Invariant for pixel-identical legacy widgets: legacy per-row step
// (80 + 16) must equal SCALE * (ROW_HEIGHT + MARGIN_Y) → 4 * (8 + 16) = 96.
export const COLS = 12
export const SCALE = 4
export const ROW_HEIGHT = 8
export const MARGIN: readonly [number, number] = [16, 16]
export const MARGIN_X = MARGIN[0]
export const MARGIN_Y = MARGIN[1]
```

- [ ] **Step 2: Update `apps/web/src/components/dashboard/dashboard-grid.tsx`**

Find the existing constants (around lines 46-57) and replace with an import. Delete the local declarations:

```ts
// REMOVE these local declarations (lines ~46-57):
// const COLS = 12
// const SCALE = 4
// const ROW_HEIGHT = 8
// const MARGIN: [number, number] = [16, 16]
// const MARGIN_Y = MARGIN[1]

// ADD this import near the other local imports (after './widget-renderer'):
import { COLS, MARGIN, MARGIN_Y, ROW_HEIGHT, SCALE } from './grid-constants'
```

Keep `MOBILE_ROW_PX` and `MOBILE_BREAKPOINT` local — they're not needed by sizing-strategies.

- [ ] **Step 3: Run typecheck**

Run: `bun run typecheck`
Expected: passes with no errors

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/components/dashboard/grid-constants.ts apps/web/src/components/dashboard/dashboard-grid.tsx
git commit -m "refactor(web): extract dashboard grid constants for sharing"
```

---

## Task 2: Add `SizingStrategy` type and `sizing` field to every widget definition

**Files:**
- Modify: `apps/web/src/lib/widget-types.ts`

Type-system change. Adding `sizing: SizingStrategy` as a required field forces every widget entry to declare a strategy at compile time. No runtime change yet — nothing reads this field until Task 9.

- [ ] **Step 1: Add the type and field at the top of `widget-types.ts`**

After the `WidgetCategory` type and before `WidgetTypeDefinition` (around line 1):

```ts
export type WidgetCategory = 'Real-time' | 'Charts' | 'Status'

export type SizingStrategy =
  | { kind: 'free' }
  | { kind: 'fixed' }
  | { kind: 'aspect-square'; tiers: readonly number[] }
  | { kind: 'content-height' }

export interface WidgetTypeDefinition {
  category: WidgetCategory
  defaultH: number
  defaultW: number
  id: string
  label: string
  maxH?: number
  maxW?: number
  minH: number
  minW: number
  sizing: SizingStrategy
}
```

- [ ] **Step 2: Add `sizing` to every entry in `WIDGET_TYPES`**

Map each entry per the spec. The spec assigns:
- `gauge` → `{ kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }`
- `top-n` → `{ kind: 'content-height' }`
- `stat-number` → `{ kind: 'fixed' }`
- every other widget → `{ kind: 'free' }`

For each `WIDGET_TYPES` entry, add `sizing: ...` as the last field. Example for the first three:

```ts
export const WIDGET_TYPES = [
  {
    id: 'stat-number',
    label: 'Stat Number',
    category: 'Real-time',
    defaultW: 2,
    defaultH: 1,
    minW: 2,
    minH: 1,
    maxW: 2,
    maxH: 1,
    sizing: { kind: 'fixed' }
  },
  {
    id: 'metric-card',
    label: 'Metric Card',
    category: 'Real-time',
    defaultW: 4,
    defaultH: 4,
    minW: 3,
    minH: 3,
    maxW: 6,
    maxH: 6,
    sizing: { kind: 'free' }
  },
  {
    id: 'server-cards',
    label: 'Server Cards',
    category: 'Real-time',
    defaultW: 12,
    defaultH: 6,
    minW: 4,
    minH: 3,
    sizing: { kind: 'free' }
  },
  {
    id: 'gauge',
    label: 'Gauge',
    category: 'Real-time',
    defaultW: 2,
    defaultH: 2,
    minW: 2,
    minH: 2,
    maxW: 6,
    maxH: 6,
    sizing: { kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }
  },
  // … etc for every remaining widget
]
```

For the remaining 10 widgets (`line-chart`, `multi-line`, `top-n`, `alert-list`, `service-status`, `traffic-bar`, `disk-io`, `server-map`, `markdown`, `uptime-timeline`): add `sizing: { kind: 'top-n' is content-height; everything else is free }`.

- [ ] **Step 3: Run typecheck**

Run: `bun run typecheck`
Expected: passes. If any widget is missing `sizing`, TS will fail with "Property 'sizing' is missing".

- [ ] **Step 4: Run existing frontend tests**

Run: `cd apps/web && bun run test`
Expected: passes. No tests touch `sizing` yet; this confirms we didn't break anything.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/lib/widget-types.ts
git commit -m "feat(web): declare sizing strategy on every widget type"
```

---

## Task 3: Implement `nearestTier` helper with tests

**Files:**
- Create: `apps/web/src/components/dashboard/sizing-strategies/index.ts`
- Create: `apps/web/src/components/dashboard/sizing-strategies/index.test.ts`

Smallest building block — a pure function used by aspect-square's `snapOnRelease` and by `widgetsToLayout`'s clamp pass.

- [ ] **Step 1: Write the failing test**

Create `apps/web/src/components/dashboard/sizing-strategies/index.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { nearestTier } from './index'

describe('nearestTier', () => {
  const tiers = [2, 3, 4, 5, 6] as const

  it('returns the exact tier when value matches', () => {
    expect(nearestTier(3, tiers)).toBe(3)
  })

  it('rounds down when between two tiers, closer to lower', () => {
    expect(nearestTier(3.4, tiers)).toBe(3)
  })

  it('rounds up when between two tiers, closer to upper', () => {
    expect(nearestTier(3.6, tiers)).toBe(4)
  })

  it('picks the first tier on exact ties (conservative)', () => {
    expect(nearestTier(3.5, [3, 4])).toBe(3)
    expect(nearestTier(4.5, tiers)).toBe(4)
  })

  it('clamps to minimum tier for values below the range', () => {
    expect(nearestTier(0, tiers)).toBe(2)
    expect(nearestTier(-5, tiers)).toBe(2)
  })

  it('clamps to maximum tier for values above the range', () => {
    expect(nearestTier(100, tiers)).toBe(6)
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd apps/web && bun run test sizing-strategies/index.test.ts`
Expected: FAIL with "Cannot find module './index'" (or similar)

- [ ] **Step 3: Create the implementation**

Create `apps/web/src/components/dashboard/sizing-strategies/index.ts`:

```ts
// Returns the tier closest to `value`. Ties pick the first (smaller) tier —
// conservative bias avoids accidental upgrades when the user releases mid-drag.
export function nearestTier(value: number, tiers: readonly number[]): number {
  return tiers.reduce(
    (best, t) => (Math.abs(t - value) < Math.abs(best - value) ? t : best),
    tiers[0]
  )
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd apps/web && bun run test sizing-strategies/index.test.ts`
Expected: PASS, 6 tests

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/dashboard/sizing-strategies/index.ts apps/web/src/components/dashboard/sizing-strategies/index.test.ts
git commit -m "feat(web): add nearestTier helper for sizing strategies"
```

---

## Task 4: Implement `applyCoarsePatch` helper with tests

**Files:**
- Modify: `apps/web/src/components/dashboard/sizing-strategies/index.ts` (add `applyCoarsePatch`)
- Modify: `apps/web/src/components/dashboard/sizing-strategies/index.test.ts` (add tests)

`applyCoarsePatch` converts a `SnapPatch` (coarse units) into a layout item update in fine units. Used by `commitLayoutChange` after `snapOnRelease`.

- [ ] **Step 1: Add failing tests**

Append to `apps/web/src/components/dashboard/sizing-strategies/index.test.ts`:

```ts
import { applyCoarsePatch, type SnapPatch } from './index'
import type { LayoutItem } from 'react-grid-layout'

describe('applyCoarsePatch', () => {
  const baseItem: LayoutItem = {
    i: 'gauge-1',
    x: 2,
    y: 0,
    w: 2,
    h: 8        // fine units (= 2 coarse)
  }

  it('returns item unchanged when patch is empty', () => {
    const result = applyCoarsePatch(baseItem, {})
    expect(result.w).toBe(2)
    expect(result.h).toBe(8)
  })

  it('applies coarse w directly (w lives in coarse units throughout)', () => {
    const patch: SnapPatch = { w: 3 }
    const result = applyCoarsePatch(baseItem, patch)
    expect(result.w).toBe(3)
    expect(result.h).toBe(8)  // unchanged
  })

  it('converts coarse h to fine by multiplying by SCALE (4)', () => {
    const patch: SnapPatch = { h: 3 }
    const result = applyCoarsePatch(baseItem, patch)
    expect(result.w).toBe(2)
    expect(result.h).toBe(12)  // 3 * 4 = 12 fine
  })

  it('applies both w and h', () => {
    const patch: SnapPatch = { w: 4, h: 4 }
    const result = applyCoarsePatch(baseItem, patch)
    expect(result.w).toBe(4)
    expect(result.h).toBe(16)
  })

  it('does not mutate the input item', () => {
    const patch: SnapPatch = { w: 5, h: 5 }
    applyCoarsePatch(baseItem, patch)
    expect(baseItem.w).toBe(2)
    expect(baseItem.h).toBe(8)
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/web && bun run test sizing-strategies/index.test.ts`
Expected: FAIL with "applyCoarsePatch is not defined" or "SnapPatch is not exported"

- [ ] **Step 3: Add the implementation**

Add to `apps/web/src/components/dashboard/sizing-strategies/index.ts`:

```ts
import type { LayoutItem } from 'react-grid-layout'
import { SCALE } from '../grid-constants'

// A coarse-unit override returned by `snapOnRelease`. The grid layer applies
// it to fine-unit layout items via `applyCoarsePatch`.
export interface SnapPatch {
  w?: number   // coarse units
  h?: number   // coarse units (will be multiplied by SCALE when applied)
}

export function applyCoarsePatch(item: LayoutItem, patch: SnapPatch): LayoutItem {
  return {
    ...item,
    w: patch.w ?? item.w,
    h: patch.h !== undefined ? patch.h * SCALE : item.h
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/web && bun run test sizing-strategies/index.test.ts`
Expected: PASS, all 11 tests (6 nearestTier + 5 applyCoarsePatch)

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/dashboard/sizing-strategies/index.ts apps/web/src/components/dashboard/sizing-strategies/index.test.ts
git commit -m "feat(web): add applyCoarsePatch helper for sizing strategies"
```

---

## Task 5: Implement `pixelSquareFineH` and `normalizeRenderItem` (Layer A)

**Files:**
- Create: `apps/web/src/components/dashboard/sizing-strategies/normalize.ts`
- Create: `apps/web/src/components/dashboard/sizing-strategies/normalize.test.ts`

Layer A computes idle-time h / minH / maxH for `aspect-square` (pixel-square) and `content-height` (locked to measurement) widgets.

- [ ] **Step 1: Write failing tests**

Create `apps/web/src/components/dashboard/sizing-strategies/normalize.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import type { LayoutItem } from 'react-grid-layout'
import type { SizingStrategy } from '@/lib/widget-types'
import { normalizeRenderItem, pixelSquareFineH } from './normalize'

const W_AT_1004 = 1004  // a representative container width

describe('pixelSquareFineH', () => {
  it('returns wCoarse * SCALE when containerWidth is non-positive', () => {
    expect(pixelSquareFineH(2, 0)).toBe(8)
    expect(pixelSquareFineH(3, -100)).toBe(12)
  })

  it('returns pixel-square fine h at typical container width', () => {
    // At 1004px: colStepPx = (1004+16)/12 = 85; fineRowStepPx = 8+16 = 24
    // pixelSquareFineH(2) = round(2 * 85 / 24) = round(7.083) = 7
    expect(pixelSquareFineH(2, W_AT_1004)).toBe(7)
    // pixelSquareFineH(3) = round(3 * 85 / 24) = round(10.625) = 11
    expect(pixelSquareFineH(3, W_AT_1004)).toBe(11)
    // pixelSquareFineH(6) = round(6 * 85 / 24) = round(21.25) = 21
    expect(pixelSquareFineH(6, W_AT_1004)).toBe(21)
  })

  it('never returns less than 1', () => {
    expect(pixelSquareFineH(0, W_AT_1004)).toBe(1)
  })
})

describe('normalizeRenderItem', () => {
  const baseItem: LayoutItem = {
    i: 'widget-1',
    x: 0,
    y: 0,
    w: 2,
    h: 8,           // fine units
    minW: 2,
    minH: 8,
    maxW: 6,
    maxH: 24
  }

  it('returns aspect-square h derived from w', () => {
    const strategy: SizingStrategy = { kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }
    const result = normalizeRenderItem(baseItem, strategy, { containerWidth: W_AT_1004 })
    expect(result.h).toBe(7)              // pixelSquareFineH(2, 1004)
    expect(result.minH).toBe(7)           // pixelSquareFineH(minW=2, 1004)
    expect(result.maxH).toBe(21)          // pixelSquareFineH(maxW=6, 1004)
  })

  it('aspect-square leaves w/x/y/minW/maxW unchanged', () => {
    const strategy: SizingStrategy = { kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }
    const result = normalizeRenderItem(baseItem, strategy, { containerWidth: W_AT_1004 })
    expect(result.w).toBe(2)
    expect(result.x).toBe(0)
    expect(result.minW).toBe(2)
    expect(result.maxW).toBe(6)
  })

  it('content-height uses measured fine h when present', () => {
    const strategy: SizingStrategy = { kind: 'content-height' }
    const result = normalizeRenderItem(baseItem, strategy, {
      containerWidth: W_AT_1004,
      autoMeasuredFineH: 11
    })
    expect(result.h).toBe(11)
    expect(result.minH).toBe(11)
    expect(result.maxH).toBe(11)
  })

  it('content-height returns item unchanged when measurement is missing', () => {
    const strategy: SizingStrategy = { kind: 'content-height' }
    const result = normalizeRenderItem(baseItem, strategy, { containerWidth: W_AT_1004 })
    expect(result.h).toBe(8)              // unchanged
    expect(result.minH).toBe(8)
    expect(result.maxH).toBe(24)
  })

  it('free returns item unchanged', () => {
    const strategy: SizingStrategy = { kind: 'free' }
    const result = normalizeRenderItem(baseItem, strategy, { containerWidth: W_AT_1004 })
    expect(result.h).toBe(8)
    expect(result.minH).toBe(8)
    expect(result.maxH).toBe(24)
  })

  it('fixed returns item unchanged', () => {
    const strategy: SizingStrategy = { kind: 'fixed' }
    const result = normalizeRenderItem(baseItem, strategy, { containerWidth: W_AT_1004 })
    expect(result.h).toBe(8)
    expect(result.minH).toBe(8)
    expect(result.maxH).toBe(24)
  })

  it('aspect-square handles undefined maxW gracefully', () => {
    const strategy: SizingStrategy = { kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }
    const itemNoMaxW: LayoutItem = { ...baseItem, maxW: undefined, maxH: undefined }
    const result = normalizeRenderItem(itemNoMaxW, strategy, { containerWidth: W_AT_1004 })
    expect(result.maxH).toBeUndefined()
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/web && bun run test sizing-strategies/normalize.test.ts`
Expected: FAIL with "Cannot find module './normalize'"

- [ ] **Step 3: Create the implementation**

Create `apps/web/src/components/dashboard/sizing-strategies/normalize.ts`:

```ts
import type { LayoutItem } from 'react-grid-layout'
import type { SizingStrategy } from '@/lib/widget-types'
import { COLS, MARGIN_X, MARGIN_Y, ROW_HEIGHT, SCALE } from '../grid-constants'

export interface NormalizeContext {
  containerWidth: number
  autoMeasuredFineH?: number   // populated for content-height widgets via ResizeObserver
}

// Same formula RGL's aspectRatio(1) uses internally (see
// react-grid-layout@2.2.2/dist/chunk-XYPIYYYQ.mjs:61). Replicated here so we can
// apply it at idle render time — RGL constraints only run during resize.
export function pixelSquareFineH(wCoarse: number, containerWidth: number): number {
  if (containerWidth <= 0) {
    return wCoarse * SCALE
  }
  const colStepPx = (containerWidth + MARGIN_X) / COLS
  const fineRowStepPx = ROW_HEIGHT + MARGIN_Y
  return Math.max(1, Math.round((wCoarse * colStepPx) / fineRowStepPx))
}

// Applies strategy-specific overrides to a fine-unit layout item.
// Operates entirely in fine units (caller has already done `h *= SCALE`).
// Returns a NEW item — never mutates the input.
export function normalizeRenderItem(
  item: LayoutItem,
  strategy: SizingStrategy,
  ctx: NormalizeContext
): LayoutItem {
  switch (strategy.kind) {
    case 'aspect-square': {
      const h = pixelSquareFineH(item.w, ctx.containerWidth)
      const minH = pixelSquareFineH(item.minW ?? 2, ctx.containerWidth)
      const maxH =
        item.maxW !== undefined
          ? pixelSquareFineH(item.maxW, ctx.containerWidth)
          : undefined
      return { ...item, h, minH, maxH }
    }
    case 'content-height': {
      const measured = ctx.autoMeasuredFineH
      if (measured !== undefined) {
        return { ...item, h: measured, minH: measured, maxH: measured }
      }
      return item
    }
    case 'fixed':
    case 'free':
      return item
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/web && bun run test sizing-strategies/normalize.test.ts`
Expected: PASS, all 11 tests

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/dashboard/sizing-strategies/normalize.ts apps/web/src/components/dashboard/sizing-strategies/normalize.test.ts
git commit -m "feat(web): add normalizeRenderItem for idle widget sizing (layer A)"
```

---

## Task 6: Implement `applyStrategy` (Layer B) with `lockHeight` constraint

**Files:**
- Modify: `apps/web/src/components/dashboard/sizing-strategies/index.ts`
- Modify: `apps/web/src/components/dashboard/sizing-strategies/index.test.ts`

Layer B returns `LayoutConstraint`s + resize handle config that attaches per-item. RGL's pipeline applies them during resize.

- [ ] **Step 1: Verify the subpath export resolves**

Run: `node -e "console.log(Object.keys(require('react-grid-layout/core')).filter(k => k === 'aspectRatio' || k === 'snapToGrid').sort())"`
Expected: `[ 'aspectRatio' ]` (snapToGrid not used by us but proves the path resolves; aspectRatio is the one we need)

If the output is empty or the require fails, run `cd apps/web && bun install` first and re-check.

- [ ] **Step 2: Append failing tests**

Append to `apps/web/src/components/dashboard/sizing-strategies/index.test.ts`:

```ts
import { applyStrategy } from './index'
import type { SizingStrategy } from '@/lib/widget-types'

describe('applyStrategy', () => {
  it('free returns no constraints and undefined resize handles', () => {
    const strategy: SizingStrategy = { kind: 'free' }
    const desc = applyStrategy(strategy)
    expect(desc.constraints).toEqual([])
    expect(desc.resizeHandles).toBeUndefined()
    expect(desc.isResizable).toBe(true)
  })

  it('fixed returns no constraints, empty resize handles, isResizable false', () => {
    const strategy: SizingStrategy = { kind: 'fixed' }
    const desc = applyStrategy(strategy)
    expect(desc.constraints).toEqual([])
    expect(desc.resizeHandles).toEqual([])
    expect(desc.isResizable).toBe(false)
  })

  it('aspect-square returns aspectRatio(1) constraint and SE handle', () => {
    const strategy: SizingStrategy = { kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }
    const desc = applyStrategy(strategy)
    expect(desc.constraints).toHaveLength(1)
    expect(desc.constraints[0].name).toBe('aspectRatio(1)')
    expect(desc.resizeHandles).toEqual(['se'])
    expect(desc.isResizable).toBe(true)
  })

  it('content-height returns lockHeight constraint when measured', () => {
    const strategy: SizingStrategy = { kind: 'content-height' }
    const desc = applyStrategy(strategy, 11)
    expect(desc.constraints).toHaveLength(1)
    expect(desc.constraints[0].name).toBe('lockHeight(11)')
    expect(desc.resizeHandles).toEqual(['e'])
    expect(desc.isResizable).toBe(true)
  })

  it('content-height returns no constraint when unmeasured', () => {
    const strategy: SizingStrategy = { kind: 'content-height' }
    const desc = applyStrategy(strategy)
    expect(desc.constraints).toEqual([])
    expect(desc.resizeHandles).toEqual(['e'])
    expect(desc.isResizable).toBe(true)
  })

  it('lockHeight constraint locks h to the measured value', () => {
    const strategy: SizingStrategy = { kind: 'content-height' }
    const desc = applyStrategy(strategy, 11)
    const constraint = desc.constraints[0]
    // Call the constraint directly — params: item, w, h, handle, context
    const result = constraint.constrainSize?.(
      { i: 'x', x: 0, y: 0, w: 4, h: 99 },
      4,
      99,
      'e',
      { cols: 12, containerWidth: 1000, maxRows: Infinity, rowHeight: 8, margin: [16, 16], layout: [] } as any
    )
    expect(result).toEqual({ w: 4, h: 11 })
  })
})
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd apps/web && bun run test sizing-strategies/index.test.ts`
Expected: FAIL with "applyStrategy is not defined" or similar

- [ ] **Step 4: Add the implementation**

Append to `apps/web/src/components/dashboard/sizing-strategies/index.ts`:

```ts
import { aspectRatio } from 'react-grid-layout/core'
import type { LayoutConstraint, ResizeHandleAxis } from 'react-grid-layout/core'
import type { SizingStrategy } from '@/lib/widget-types'

export interface StrategyDescriptor {
  // Constraints attached to LayoutItem.constraints. Applied by RGL inside
  // applySizeConstraints during the resize pipeline. NOT applied at idle
  // layout sync — use `normalizeRenderItem` for idle h.
  constraints: LayoutConstraint[]

  // Which resize handles to render. undefined = RGL default (all 8 corners/edges).
  resizeHandles?: ResizeHandleAxis[]

  // Whether the item participates in resize at all.
  isResizable: boolean
}

function lockHeight(fineH: number): LayoutConstraint {
  return {
    name: `lockHeight(${fineH})`,
    constrainSize(_item, w) {
      return { w, h: fineH }
    }
  }
}

export function applyStrategy(
  strategy: SizingStrategy,
  measuredFineH?: number
): StrategyDescriptor {
  switch (strategy.kind) {
    case 'free':
      return { constraints: [], resizeHandles: undefined, isResizable: true }
    case 'fixed':
      return { constraints: [], resizeHandles: [], isResizable: false }
    case 'aspect-square':
      return { constraints: [aspectRatio(1)], resizeHandles: ['se'], isResizable: true }
    case 'content-height':
      return {
        constraints: measuredFineH !== undefined ? [lockHeight(measuredFineH)] : [],
        resizeHandles: ['e'],
        isResizable: true
      }
  }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd apps/web && bun run test sizing-strategies/index.test.ts`
Expected: PASS, all 17 tests (6 nearestTier + 5 applyCoarsePatch + 6 applyStrategy)

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/dashboard/sizing-strategies/index.ts apps/web/src/components/dashboard/sizing-strategies/index.test.ts
git commit -m "feat(web): add applyStrategy with RGL constraints (layer B)"
```

---

## Task 7: Implement `snapOnRelease`

**Files:**
- Modify: `apps/web/src/components/dashboard/sizing-strategies/index.ts`
- Modify: `apps/web/src/components/dashboard/sizing-strategies/index.test.ts`

`snapOnRelease` produces a coarse-unit `SnapPatch` for aspect-square widgets at the end of a resize. Other strategies return an empty patch.

- [ ] **Step 1: Append failing tests**

Append to `apps/web/src/components/dashboard/sizing-strategies/index.test.ts`:

```ts
import { snapOnRelease } from './index'

describe('snapOnRelease', () => {
  const baseItem: LayoutItem = {
    i: 'gauge-1',
    x: 0,
    y: 0,
    w: 3,
    h: 11      // fine units
  }

  it('aspect-square snaps w to the nearest tier and returns coarse h = w', () => {
    const strategy: SizingStrategy = { kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }
    const result = snapOnRelease({ ...baseItem, w: 4 }, strategy, { containerWidth: 1004 })
    expect(result).toEqual({ w: 4, h: 4 })
  })

  it('aspect-square snaps non-tier w to nearest', () => {
    const strategy: SizingStrategy = { kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }
    // base.w shouldn't be fractional in practice, but commitLayoutChange
    // may receive a w just above a tier boundary during dragging — verify snap.
    const result = snapOnRelease({ ...baseItem, w: 3 }, strategy, { containerWidth: 1004 })
    expect(result).toEqual({ w: 3, h: 3 })
  })

  it('free returns empty patch', () => {
    const strategy: SizingStrategy = { kind: 'free' }
    expect(snapOnRelease(baseItem, strategy, { containerWidth: 1004 })).toEqual({})
  })

  it('fixed returns empty patch', () => {
    const strategy: SizingStrategy = { kind: 'fixed' }
    expect(snapOnRelease(baseItem, strategy, { containerWidth: 1004 })).toEqual({})
  })

  it('content-height returns empty patch', () => {
    const strategy: SizingStrategy = { kind: 'content-height' }
    expect(snapOnRelease(baseItem, strategy, { containerWidth: 1004 })).toEqual({})
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/web && bun run test sizing-strategies/index.test.ts`
Expected: FAIL with "snapOnRelease is not defined"

- [ ] **Step 3: Add the implementation**

Append to `apps/web/src/components/dashboard/sizing-strategies/index.ts`:

```ts
export interface SnapContext {
  containerWidth: number
}

export function snapOnRelease(
  item: LayoutItem,
  strategy: SizingStrategy,
  _ctx: SnapContext
): SnapPatch {
  switch (strategy.kind) {
    case 'aspect-square': {
      const tier = nearestTier(item.w, strategy.tiers)
      return { w: tier, h: tier }
    }
    case 'free':
    case 'fixed':
    case 'content-height':
      return {}
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/web && bun run test sizing-strategies/index.test.ts`
Expected: PASS, all 22 tests

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/dashboard/sizing-strategies/index.ts apps/web/src/components/dashboard/sizing-strategies/index.test.ts
git commit -m "feat(web): add snapOnRelease for aspect-square tier snap"
```

---

## Task 8: Add aspect-square clamp pass in `widgetsToLayout`

**Files:**
- Modify: `apps/web/src/components/dashboard/dashboard-layout.ts`
- Create: `apps/web/src/components/dashboard/dashboard-layout.test.ts`

When old data has `grid_w !== grid_h` for an aspect-square widget, snap to the nearest tier on first load. No DB write — render-time only.

- [ ] **Step 1: Write failing test**

Create `apps/web/src/components/dashboard/dashboard-layout.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import type { DashboardWidget } from '@/lib/widget-types'
import { widgetsToLayout } from './dashboard-layout'

function makeWidget(overrides: Partial<DashboardWidget>): DashboardWidget {
  return {
    id: 'w1',
    dashboard_id: 'd1',
    widget_type: 'gauge',
    grid_x: 0,
    grid_y: 0,
    grid_w: 2,
    grid_h: 2,
    sort_order: 0,
    title: null,
    config_json: '{}',
    created_at: '2026-05-28T00:00:00Z',
    ...overrides
  }
}

describe('widgetsToLayout', () => {
  describe('aspect-square clamp', () => {
    it('snaps a non-square gauge to the nearest tier (uses max of w/h)', () => {
      // grid_w=3, grid_h=5 → max=5 → nearest tier in [2,3,4,5,6] is 5
      const widget = makeWidget({ grid_w: 3, grid_h: 5 })
      const [item] = widgetsToLayout([widget])
      expect(item.w).toBe(5)
      expect(item.h).toBe(5)
    })

    it('preserves a gauge already at a legal tier', () => {
      const widget = makeWidget({ grid_w: 4, grid_h: 4 })
      const [item] = widgetsToLayout([widget])
      expect(item.w).toBe(4)
      expect(item.h).toBe(4)
    })

    it('clamps an oversized gauge to maxW tier', () => {
      // grid_w=10 → clamped to maxW=6 by existing logic → tier=6
      const widget = makeWidget({ grid_w: 10, grid_h: 10 })
      const [item] = widgetsToLayout([widget])
      expect(item.w).toBe(6)
      expect(item.h).toBe(6)
    })

    it('does not snap non-aspect-square widgets', () => {
      // metric-card is free; should keep grid_w/h as-is (within min/max)
      const widget = makeWidget({ widget_type: 'metric-card', grid_w: 4, grid_h: 3 })
      const [item] = widgetsToLayout([widget])
      expect(item.w).toBe(4)
      expect(item.h).toBe(3)
    })
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/web && bun run test dashboard-layout.test.ts`
Expected: FAIL — the gauge snap won't happen yet

- [ ] **Step 3: Update `widgetsToLayout` in `dashboard-layout.ts`**

Read the file to find the existing clamp logic (around line 36-44). Add the aspect-square snap after the clamp. Also remove the `if (maxW === minW && maxH === minH) isResizable = false` block at the bottom (Task 9 picks that up via the `fixed` strategy).

The full updated function looks like:

```ts
import type { Layout, LayoutItem } from 'react-grid-layout'
import { type DashboardWidget, WIDGET_TYPES, type WidgetTypeDefinition } from '@/lib/widget-types'
import { nearestTier } from './sizing-strategies'

// (existing code stays)

const WIDGET_TYPE_MAP = new Map<string, WidgetTypeDefinition>(
  WIDGET_TYPES.map((widget) => [widget.id, widget])
)

function getSizeConstraints(widgetType: string) {
  const definition = WIDGET_TYPE_MAP.get(widgetType)
  return {
    minW: definition?.minW ?? 2,
    minH: definition?.minH ?? 2,
    maxW: definition?.maxW,
    maxH: definition?.maxH
  }
}

function isWidgetStatic(configJson: string): boolean {
  try {
    const config = JSON.parse(configJson)
    return config?.is_static === true
  } catch {
    return false
  }
}

export function widgetsToLayout(widgets: DashboardWidget[]): Layout {
  return widgets.map((widget) => {
    const { minW, minH, maxW, maxH } = getSizeConstraints(widget.widget_type)
    let w = Math.max(widget.grid_w, minW)
    let h = Math.max(widget.grid_h, minH)
    if (maxW !== undefined) w = Math.min(w, maxW)
    if (maxH !== undefined) h = Math.min(h, maxH)

    // Aspect-square widgets persist as w_coarse === h_coarse. If historical data
    // somehow has them out of sync (or grid_w not at a legal tier), snap here so
    // the cell renders square on first load. Next user drag/resize writes back.
    const sizing = WIDGET_TYPE_MAP.get(widget.widget_type)?.sizing
    if (sizing?.kind === 'aspect-square') {
      const tier = nearestTier(Math.max(w, h), sizing.tiers)
      w = tier
      h = tier
    }

    const item: LayoutItem = {
      i: widget.id,
      x: widget.grid_x,
      y: widget.grid_y,
      w,
      h,
      minW,
      minH
    }
    if (maxW !== undefined) item.maxW = maxW
    if (maxH !== undefined) item.maxH = maxH
    if (isWidgetStatic(widget.config_json)) {
      item.static = true
    }
    return item
  })
}
```

Note: the old `if (maxW !== undefined && maxH !== undefined && minW === maxW && minH === maxH) item.isResizable = false` is **removed**. Task 9 sets `isResizable: false` via the `fixed` strategy descriptor.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/web && bun run test dashboard-layout.test.ts`
Expected: PASS, 4 tests

- [ ] **Step 5: Run typecheck and all tests**

Run: `bun run typecheck && cd apps/web && bun run test`
Expected: passes. The deleted `isResizable` block isn't covered by tests yet — Task 9 picks it up via strategy.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/dashboard/dashboard-layout.ts apps/web/src/components/dashboard/dashboard-layout.test.ts
git commit -m "feat(web): snap aspect-square widgets to nearest tier in widgetsToLayout"
```

---

## Task 9: Refactor `dashboard-grid.tsx` to use the strategy dispatcher

**Files:**
- Modify: `apps/web/src/components/dashboard/dashboard-grid.tsx`

This is the central refactor. We delete `SQUARE_TYPES`, `AUTO_HEIGHT_TYPES`, `squareIdSet`, `autoIdSet`, `visualSquareHFine`, `interactionStateRef`, and the per-strategy branching in `baseLayout` / `updateLiveLayout` / `commitLayoutChange`. We replace them with a `widgetById` map + `getStrategy` lookup + `normalizeRenderItem` + `applyStrategy` + `snapOnRelease` + `applyCoarsePatch`.

No unit tests for this file — it's a thin orchestrator over the strategy modules (which are tested) and RGL itself. We verify manually in Task 10 via `make dev-demo`.

- [ ] **Step 1: Read the current file to anchor the edits**

Run: `wc -l apps/web/src/components/dashboard/dashboard-grid.tsx`
Note the current line count. Expect around 460 lines.

- [ ] **Step 2: Delete the strategy markers and `visualSquareHFine`**

In `apps/web/src/components/dashboard/dashboard-grid.tsx`:

1. Delete lines containing `AUTO_HEIGHT_TYPES` (the constant declaration around line 60-61) and `SQUARE_TYPES` (around line 63-65).
2. Delete the `visualSquareHFine` `useCallback` block (around lines 197-212).
3. Delete the `squareIdSet` `useMemo` (lines 192-195).
4. Delete `autoIdSet` `useMemo` (around lines 244-247).
5. Delete `interactionStateRef` declaration and assignment (around lines 256-260).

(You'll add their replacements in the next steps. The file may not compile in between — that's fine, we'll get it back together in Step 5.)

- [ ] **Step 3: Add strategy lookup infrastructure**

After the `useContainerWidth` hook call (around line 183), add the widget map and strategy lookup. Also import the needed helpers.

Top of file, add to imports:

```ts
import { WIDGET_TYPES } from '@/lib/widget-types'
import type { SizingStrategy, WidgetTypeDefinition } from '@/lib/widget-types'
import { applyStrategy, applyCoarsePatch, snapOnRelease } from './sizing-strategies'
import { normalizeRenderItem } from './sizing-strategies/normalize'
```

Add a module-level map (outside the component, near other module-level constants like `COLS`):

```ts
const WIDGET_TYPE_MAP = new Map<string, WidgetTypeDefinition>(
  WIDGET_TYPES.map((widget) => [widget.id, widget])
)
```

Inside the component, after `const [autoUnits, setAutoUnits] = useState<...>({})`:

```ts
const widgetById = useMemo(
  () => new Map(widgets.map((w) => [w.id, w])),
  [widgets]
)

const getStrategy = useCallback(
  (itemId: string): SizingStrategy => {
    const widget = widgetById.get(itemId)
    if (!widget) return { kind: 'free' }
    const def = WIDGET_TYPE_MAP.get(widget.widget_type)
    return def?.sizing ?? { kind: 'free' }
  },
  [widgetById]
)
```

- [ ] **Step 4: Rewrite `baseLayout`**

Replace the entire `baseLayout` `useMemo` block with this implementation:

```ts
const baseLayout = useMemo(() => {
  const layout = widgetsToLayout(widgets)
  for (const item of layout) {
    item.y *= SCALE
    item.h *= SCALE
    if (item.minH !== undefined) item.minH *= SCALE
    if (item.maxH !== undefined) item.maxH *= SCALE

    const strategy = getStrategy(item.i)
    const measured = autoUnits[item.i]

    // Layer A: idle h / minH / maxH per strategy
    const normalized = normalizeRenderItem(item, strategy, {
      containerWidth: width,
      autoMeasuredFineH: measured
    })
    item.h = normalized.h
    item.minH = normalized.minH
    item.maxH = normalized.maxH

    // Layer B: resize-time constraints, handles, resizability
    const desc = applyStrategy(strategy, measured)
    if (desc.constraints.length > 0) item.constraints = desc.constraints
    if (desc.resizeHandles) item.resizeHandles = desc.resizeHandles
    if (!desc.isResizable) item.isResizable = false
  }
  return deoverlapLayout(layout)
}, [widgets, autoUnits, width, getStrategy])
```

- [ ] **Step 5: Rewrite `updateLiveLayout`**

Find the existing `updateLiveLayout` `useCallback` and replace its body. The new version uses strategy to decide whether to snap h to coarse multiples (only `free` and `fixed` need this; aspect-square and content-height own their fine h):

```ts
const updateLiveLayout = useCallback(
  (nextLayout: Layout) => {
    const snapped = nextLayout.map((item) => {
      const strategy = getStrategy(item.i)
      const base = {
        ...item,
        y: Math.round(item.y / SCALE) * SCALE
      }
      // Snap h to SCALE multiples only for strategies that operate at coarse h.
      // aspect-square: h is fine pixel-square (constraints handle resize); leave it.
      // content-height: h locked to measurement; leave it.
      if (strategy.kind === 'free' || strategy.kind === 'fixed') {
        base.h = Math.max(SCALE, Math.round(item.h / SCALE) * SCALE)
      }
      return base
    })
    setLiveLayout(deoverlapLayout(snapped))
  },
  [getStrategy]
)
```

- [ ] **Step 6: Rewrite `commitLayoutChange`**

Replace its body:

```ts
const commitLayoutChange = useCallback(
  (finalLayout: Layout) => {
    setInteractionState('idle')

    // Per-strategy: snap to coarse boundaries (free/fixed), then apply
    // snapOnRelease's coarse SnapPatch via applyCoarsePatch, then re-normalize
    // so the live layout matches what the next baseLayout render will produce.
    const snapped = finalLayout.map((item) => {
      const strategy = getStrategy(item.i)
      const measured = autoUnits[item.i]

      let base = {
        ...item,
        y: Math.round(item.y / SCALE) * SCALE
      }
      if (strategy.kind === 'free' || strategy.kind === 'fixed') {
        base.h = Math.max(SCALE, Math.round(item.h / SCALE) * SCALE)
      }

      const snap = snapOnRelease(base, strategy, { containerWidth: width })
      base = applyCoarsePatch(base, snap)
      return normalizeRenderItem(base, strategy, {
        containerWidth: width,
        autoMeasuredFineH: measured
      })
    })
    const resolved = deoverlapLayout(snapped)
    setLiveLayout(resolved)

    const coarseLayout = resolved.map((item) => ({
      ...item,
      y: Math.round(item.y / SCALE),
      h: Math.round(item.h / SCALE)
    }))
    const patch = layoutToPatch(coarseLayout, widgets)
    if (patch.length > 0) {
      onLayoutChange(patch)
    }
  },
  [autoUnits, getStrategy, onLayoutChange, widgets, width]
)
```

- [ ] **Step 7: Run typecheck**

Run: `bun run typecheck`
Expected: passes. If any references to `SQUARE_TYPES`, `AUTO_HEIGHT_TYPES`, `visualSquareHFine`, `squareIdSet`, `autoIdSet`, or `interactionStateRef` remain, TS will flag them.

- [ ] **Step 8: Run all frontend tests**

Run: `cd apps/web && bun run test`
Expected: passes. Existing widget tests should not be affected.

- [ ] **Step 9: Run lint**

Run: `bun x ultracite check`
Expected: passes. If imports got out of order or there are unused imports left over from the deletions, this catches them.

- [ ] **Step 10: Commit**

```bash
git add apps/web/src/components/dashboard/dashboard-grid.tsx
git commit -m "refactor(web): route dashboard grid sizing through strategy dispatcher

Replace SQUARE_TYPES / AUTO_HEIGHT_TYPES / visualSquareHFine /
interactionStateRef with widgetById + getStrategy + normalizeRenderItem
(layer A, idle) + applyStrategy (layer B, RGL constraints). Per-strategy
h snap in updateLiveLayout/commitLayoutChange replaces the inline
overrides. snapOnRelease + applyCoarsePatch land aspect-square tier
snaps on resize end."
```

---

## Task 10: Manual verification with `make dev-demo`

**Files:**
- None directly modified. We just exercise the UI.

The strategy infrastructure has unit tests, but the integration with RGL needs a real browser. `make dev-demo` (admin/admin123, demo data) is the fastest way.

- [ ] **Step 1: Start the demo environment**

Run: `make dev-demo`

Wait for the script to print `Demo data is ready` and the Vite dev server URL. Open the URL in a browser. Log in as `admin / admin123`.

- [ ] **Step 2: Verify gauge renders pixel-square on mount**

Navigate to a dashboard that has at least one gauge widget (the demo dashboards should). Without interacting, measure the gauge's cell rectangle. It should be visually pixel-square (1:1).

If the cell is rectangular (taller than wide), `normalizeRenderItem` is not being applied — re-check Task 9 Step 4.

- [ ] **Step 3: Verify gauge resize feels right**

Click "Edit" to enter edit mode. Drag a gauge's SE handle:
- During the drag, the cell stays pixel-square (RGL `aspectRatio(1)` enforces this).
- The cell follows the cursor in whichever axis is larger.
- Drag right enough → w advances by one tier (visually).
- Release → cell snaps to the nearest tier (2/3/4/5/6 coarse, depending on cursor).

If the cell goes rectangular during drag, the `applyStrategy` constraints aren't being attached to `item.constraints` — re-check Task 9 Step 4.

- [ ] **Step 4: Verify gauge persistence**

After resizing and releasing, exit edit mode. Reload the page. The gauge should come back at the same tier you released. Confirms `commitLayoutChange` is producing a correct coarse patch via `snapOnRelease` → `layoutToPatch`.

- [ ] **Step 5: Verify content-height widget (top-n)**

Navigate to a dashboard with a Top-N widget. In edit mode, only the east (`e`) handle should be visible. Width resize works; height is locked to content. Confirms `applyContentHeight` with measured value attached the `lockHeight` constraint and `resizeHandles: ['e']`.

- [ ] **Step 6: Verify fixed widget (stat-number)**

Navigate to a dashboard with a Stat Number widget. In edit mode, no resize handles should appear. Confirms `applyStrategy({kind:'fixed'})` set `isResizable: false`.

- [ ] **Step 7: Verify free widgets (line-chart, etc.)**

Resize a line-chart widget. All 8 handles should be available. Resize works without issues. h snaps to coarse multiples on release (look for clean integer pixel values, not random fine offsets).

- [ ] **Step 8: Check browser console for errors**

Open DevTools → Console. There should be no errors. In particular, no "Maximum update depth exceeded" warnings (that was the symptom from the previous implementation).

- [ ] **Step 9: Stop the demo**

Press Ctrl-C in the terminal running `make dev-demo`. The cleanup hook stops the server cleanly.

- [ ] **Step 10: Take note of any visual issues**

If anything looks off (visual jump on release, jitter during drag, missing handles, unexpected behavior), make a note. These should not be present, but if they are, fix in a follow-up commit before continuing to Task 11.

No commit for this task — it's verification only.

---

## Task 11: Add contributor checklist

**Files:**
- Create: `docs/dashboard-widget-checklist.md`

A short reference for anyone adding a new widget type. Lives at repo root level under `docs/` (alongside `ENV.md`, project-level docs).

- [ ] **Step 1: Create the file**

```markdown
# Adding a New Dashboard Widget

This checklist captures the steps to add a new widget type to the dashboard.

## Required
1. **`apps/web/src/lib/widget-types.ts`** — add a `WidgetTypeDefinition` entry. The `sizing` field is required (TypeScript will fail to compile if you omit it).
2. **`apps/web/src/components/dashboard/widget-renderer.tsx`** — add a `switch` case for the new `widget_type`.
3. **`apps/web/src/components/dashboard/widget-picker.tsx`** — register the picker entry (icon + i18n label key).
4. **`apps/web/src/components/dashboard/widget-config-dialog.tsx`** — add a config form if the widget needs configuration.
5. **`apps/web/src/locales/{en,zh}/dashboard.json`** — i18n strings for the picker entry and any in-widget text.

## Optional
- Widget component: `apps/web/src/components/dashboard/widgets/<name>.tsx`
- Vitest coverage: `apps/web/src/components/dashboard/widgets/<name>.test.tsx`

## Picking a `sizing` strategy

| When the widget is …                                  | Use                                              |
| ----------------------------------------------------- | ------------------------------------------------ |
| A chart, table, or generic free-sized panel           | `{ kind: 'free' }`                               |
| A single fixed-size readout (one viable footprint)    | `{ kind: 'fixed' }` (set minW=maxW, minH=maxH)   |
| A radial / gauge / aspect-locked visual               | `{ kind: 'aspect-square', tiers: [...] }`        |
| A list/table that should hug its measured content     | `{ kind: 'content-height' }`                     |

For `aspect-square`, `tiers` is the list of legal coarse-grid sizes (w × w). For gauge today: `[2, 3, 4, 5, 6]` — every integer between minW (2) and maxW (6).

## Related

- Architecture: `docs/superpowers/specs/2026-05-28-dashboard-widget-sizing-strategy-design.md`
- Strategy dispatcher: `apps/web/src/components/dashboard/sizing-strategies/`
```

- [ ] **Step 2: Commit**

```bash
git add docs/dashboard-widget-checklist.md
git commit -m "docs: add new-widget contributor checklist"
```

---

## Task 12: Add manual verification checklist

**Files:**
- Create: `tests/manual/dashboard-widget-sizing.md`

The project keeps manual verification scripts in `tests/manual/`. This one documents what to check whenever the sizing/dispatcher logic changes.

- [ ] **Step 1: Verify `tests/manual/` exists**

Run: `ls tests/manual | head -5`
Expected: a list of existing manual test files. If the directory doesn't exist, run `mkdir -p tests/manual`.

- [ ] **Step 2: Create the file**

```markdown
# Dashboard Widget Sizing — Manual Verification

Run this checklist after touching `apps/web/src/components/dashboard/dashboard-grid.tsx`,
`apps/web/src/components/dashboard/dashboard-layout.ts`, or anything in
`apps/web/src/components/dashboard/sizing-strategies/`.

## Setup

Run `make dev-demo` (admin/admin123). Open the printed URL.

## Aspect-square (gauge)

- [ ] On page load (no interaction), gauge cells are visually pixel-square (1:1).
- [ ] In edit mode, the gauge's SE handle is visible. No other handles.
- [ ] Dragging the SE handle keeps the cell visually pixel-square throughout. Cell follows whichever cursor axis (x or y) moved more.
- [ ] Releasing snaps the cell to the nearest tier in `[2, 3, 4, 5, 6]` (coarse grid units).
- [ ] After resizing and reloading the page, the gauge is at the same tier — persistence works.
- [ ] Window resize / sidebar toggle: gauge scales visually but `grid_w` doesn't change.
- [ ] Two adjacent gauges: dragging into the neighbor blocks growth (existing collision behavior).

## Content-height (top-n)

- [ ] In edit mode, only the east (`e`) handle is visible on Top-N.
- [ ] Width is resizable; height tracks content measurement.

## Fixed (stat-number)

- [ ] No resize handles on Stat Number widgets in edit mode.

## Free (line-chart, multi-line, traffic-bar, disk-io, alert-list, server-cards, metric-card, …)

- [ ] All 8 handles visible in edit mode.
- [ ] Resize works smoothly.
- [ ] Released height is a clean coarse multiple — no fractional pixel offsets.

## Regression checks

- [ ] Browser console has no errors (especially no "Maximum update depth exceeded").
- [ ] Layout doesn't jitter or flicker during drag/resize.
- [ ] Layout doesn't shift visibly on release of a resize that didn't change size.
```

- [ ] **Step 3: Commit**

```bash
git add tests/manual/dashboard-widget-sizing.md
git commit -m "docs: add manual verification checklist for widget sizing"
```

---

## Task 13: Final cleanup pass

**Files:**
- None expected to change.

Sweep for leftover dead code or stale comments missed by the refactor.

- [ ] **Step 1: Search for stale identifiers**

Run: `grep -rn "SQUARE_TYPES\|AUTO_HEIGHT_TYPES\|visualSquareHFine\|squareIdSet\|autoIdSet\|interactionStateRef" apps/web/src docs/ --include="*.ts" --include="*.tsx"`
Expected: matches only in the spec (`docs/superpowers/specs/2026-05-28-dashboard-widget-sizing-strategy-design.md`) describing the deletion. No runtime code references.

If any other matches appear, remove them.

- [ ] **Step 2: Run full typecheck + tests + lint**

Run: `bun run typecheck && cd apps/web && bun run test && cd ../.. && bun x ultracite check`
Expected: all pass.

- [ ] **Step 3: Verify no commit hooks complain**

Run: `git status`
Expected: clean (no remaining modifications).

- [ ] **Step 4: Stop the brainstorm server if still running**

Run: `ls .superpowers/brainstorm/ 2>/dev/null && bash /Users/zingerbee/.claude/plugins/cache/claude-plugins-official/superpowers/5.0.7/skills/brainstorming/scripts/stop-server.sh $(ls -d .superpowers/brainstorm/*/ | head -1) 2>/dev/null || echo 'no brainstorm server running'`
Expected: either stops cleanly or "no brainstorm server running".

No additional commit for this task.

---

## Done

The full sequence (Tasks 1–13) produces:

- Explicit `sizing` field on every widget type (Task 2).
- Two-layer strategy infrastructure with unit tests (Tasks 3–7).
- Layered behavior in `widgetsToLayout` (Task 8) and `dashboard-grid.tsx` (Task 9).
- Manual verification done in a real browser (Task 10).
- Contributor docs and manual checklist (Tasks 11–12).
- Cleanup pass (Task 13).

Commits along the way produce a clean history where each step is reviewable.
