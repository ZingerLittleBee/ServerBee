---
title: Dashboard Widget Sizing Strategy
date: 2026-05-28
status: Draft
branch: lisbon
related:
  - apps/web/src/lib/widget-types.ts
  - apps/web/src/components/dashboard/dashboard-grid.tsx
  - apps/web/src/components/dashboard/dashboard-layout.ts
  - node_modules/react-grid-layout/dist/core.d.ts (LayoutConstraint API, subpath import)
---

# Dashboard Widget Sizing Strategy

## Goal

Formalize how widgets behave in the dashboard grid (free resize, fixed size, aspect-locked square, content-driven height) into a single explicit `sizing` field on each widget type. Replace the scattered ad-hoc markers (`SQUARE_TYPES`, `AUTO_HEIGHT_TYPES`, hand-rolled `visualSquareHFine` inlined in three places, `interactionStateRef` branching, post-hoc layout overrides in `updateLiveLayout` / `commitLayoutChange`) with a two-layer model:

- **Render-time normalization** (`normalizeRenderItem`) runs in `baseLayout` to set each item's fine-unit h / minH / maxH at idle.
- **Resize-time enforcement** uses RGL v2's `LayoutConstraint` API to keep the cell consistent during drag (no override race with react-resizable).

The immediate trigger is the gauge widget — its aspect-locked sizing currently fights RGL's resize pipeline because we manipulate h *after* `onResize` fires. The new model lets RGL own the resize math via `aspectRatio(1)`, and we own the idle math via `normalizeRenderItem`, with both layers producing the same pixel-square fine h.

Generalizing the model also addresses smaller issues: scattered strategy markers across three files; no documentation for "how to add a widget"; no compile-time enforcement that new widgets pick a strategy.

## Non-goals

- Backend / database changes. `sizing` stays a client-side static config.
- Migration scripts. Out-of-range widgets are clamped silently at render time by the existing `Math.max/Math.min` calls in `widgetsToLayout`.
- iOS-style multi-tier widget UIs (independent small/medium/large layouts). Container queries cover 80% of internal adaptation; aspect-locked widgets handle the remaining 20% via `aspect-square`.
- Backwards-incompatible bounds changes (e.g., capping markdown width). Existing `minW/maxW/minH/maxH` on each widget definition stay as-is.
- A `useWidgetSize` hook for JS-driven internal layout switching. CSS container queries are the chosen pattern; revisit when a real widget needs more.

---

## 1. Strategy model

Add a discriminated union to `widget-types.ts`:

```ts
export type SizingStrategy =
  | { kind: 'free' }
  | { kind: 'fixed' }
  | { kind: 'aspect-square'; tiers: number[] }
  | { kind: 'content-height' }

export interface WidgetTypeDefinition {
  // existing fields …
  sizing: SizingStrategy
}
```

The field is **required** on every widget type. The compiler refuses to accept a new widget that doesn't pick a strategy.

### Strategy assignments

| Widget          | Strategy                                              |
| --------------- | ----------------------------------------------------- |
| stat-number     | `{ kind: 'fixed' }`                                   |
| metric-card     | `{ kind: 'free' }`                                    |
| server-cards    | `{ kind: 'free' }`                                    |
| gauge           | `{ kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }`   |
| line-chart      | `{ kind: 'free' }`                                    |
| multi-line      | `{ kind: 'free' }`                                    |
| top-n           | `{ kind: 'content-height' }`                          |
| alert-list      | `{ kind: 'free' }`                                    |
| service-status  | `{ kind: 'free' }`                                    |
| traffic-bar     | `{ kind: 'free' }`                                    |
| disk-io         | `{ kind: 'free' }`                                    |
| server-map      | `{ kind: 'free' }`                                    |
| markdown        | `{ kind: 'free' }` (existing bounds unchanged)        |
| uptime-timeline | `{ kind: 'free' }`                                    |

The `tiers` array for `aspect-square` lists the legal coarse-grid sizes (w × w). For gauge: `[2, 3, 4, 5, 6]` — every integer between minW (2) and maxW (6).

---

## 2. Two layers, two unit contracts

This was the source of bugs in the previous draft. Pin both layers down:

### Layer A: render-time normalization (`normalizeRenderItem`)

```ts
// sizing-strategies/normalize.ts
import { SCALE, COLS, MARGIN, ROW_HEIGHT, MARGIN_Y } from '@/components/dashboard/grid-constants'
import type { LayoutItem } from 'react-grid-layout'

export interface NormalizeContext {
  containerWidth: number
  autoMeasuredFineH?: number   // set per-item by AutoHeightItem / ResizeObserver
}

// Operates on a LayoutItem whose h is already in FINE units (after baseLayout's h *= SCALE).
// Returns a new item with strategy-specific h / minH / maxH applied.
export function normalizeRenderItem(
  item: LayoutItem,
  strategy: SizingStrategy,
  ctx: NormalizeContext
): LayoutItem {
  switch (strategy.kind) {
    case 'aspect-square': {
      const h = pixelSquareFineH(item.w, ctx.containerWidth)
      const minH = pixelSquareFineH(item.minW ?? 2, ctx.containerWidth)
      const maxH = item.maxW !== undefined ? pixelSquareFineH(item.maxW, ctx.containerWidth) : undefined
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

// Same formula RGL's aspectRatio(1) uses internally — see chunk-XYPIYYYQ.mjs:61.
function pixelSquareFineH(wCoarse: number, containerWidth: number): number {
  if (containerWidth <= 0) return wCoarse * SCALE
  const colStepPx = (containerWidth + MARGIN[0]) / COLS
  const fineRowStepPx = ROW_HEIGHT + MARGIN_Y
  return Math.max(1, Math.round((wCoarse * colStepPx) / fineRowStepPx))
}
```

This is called once per item in `baseLayout`, after the existing `h *= SCALE` and `minH *= SCALE` / `maxH *= SCALE` conversion. The function operates **purely in fine units** for h, minH, maxH.

### Layer B: resize-time constraints (`applyStrategy`)

```ts
// sizing-strategies/index.ts
import { aspectRatio } from 'react-grid-layout/core'
import type { LayoutConstraint, ResizeHandleAxis } from 'react-grid-layout/core'

export interface StrategyDescriptor {
  // Attached to LayoutItem.constraints. Applied by RGL inside applySizeConstraints
  // during the resize pipeline. NOT applied at layout sync / idle render.
  constraints: LayoutConstraint[]
  resizeHandles?: ResizeHandleAxis[]
  isResizable: boolean
}

export function applyStrategy(strategy: SizingStrategy, measuredFineH?: number): StrategyDescriptor {
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

function lockHeight(fineH: number): LayoutConstraint {
  return {
    name: `lockHeight(${fineH})`,
    constrainSize(_item, w) {
      return { w, h: fineH }
    }
  }
}
```

The `constraints` array is attached to each `LayoutItem` in `baseLayout`. RGL invokes them inside `applySizeConstraints` during resize (verified in `react-grid-layout@2.2.2/dist/chunk-XM2M6TC6.mjs:356`).

**Important**: RGL does not apply constraints at layout sync time (`synchronizeLayoutWithChildren` at `chunk-XM2M6TC6.mjs:550` just clones / corrects bounds / compacts). That's why Layer A is necessary — without `normalizeRenderItem`, gauge would render with `h = w * SCALE` (rectangular) at mount and remain rectangular until the user resizes it. The two layers split idle h (Layer A) from drag-time h (Layer B), each in its proper place.

### Unit contract per pipeline stage

| Stage                                  | w               | h               | Owner                                  |
| -------------------------------------- | --------------- | --------------- | -------------------------------------- |
| Persisted (`DashboardWidget`)          | coarse          | coarse          | DB / API                               |
| `widgetsToLayout` output               | coarse          | coarse          | `dashboard-layout.ts`                  |
| `baseLayout` after `h *= SCALE`        | coarse          | **fine** (raw)  | `dashboard-grid.tsx`                   |
| `baseLayout` after `normalizeRenderItem` | coarse        | **fine** (strategy-normalized) | Layer A         |
| `liveLayout` / RGL `layout` prop       | coarse          | fine            | renders                                |
| RGL `onResize` callback                | coarse          | fine (constrained by Layer B) | RGL pipeline       |
| `updateLiveLayout` after y/h snap      | coarse          | fine            | `dashboard-grid.tsx`                   |
| `snapOnRelease` output                 | coarse          | **coarse** (persistence patch) | Layer A's snap helper |
| Persisted again                        | coarse          | coarse          | back to DB                             |

The only fine ↔ coarse boundary crossings happen at `baseLayout` entry (`h *= SCALE`) and `commitLayoutChange` exit (`Math.round(h / SCALE)` plus `snapOnRelease` overrides for aspect-square).

---

## 3. Identifying a widget's strategy

`LayoutItem` (the type RGL knows about) only has `i / x / y / w / h / min/max / static / constraints`. To find a widget's strategy from a layout item, maintain a `widgetById` map in the grid component:

```ts
// dashboard-grid.tsx
const widgetById = useMemo(
  () => new Map(widgets.map((w) => [w.id, w])),
  [widgets]
)

function getStrategy(itemId: string): SizingStrategy {
  const widget = widgetById.get(itemId)
  if (!widget) return { kind: 'free' }
  const def = WIDGET_TYPE_MAP.get(widget.widget_type)
  return def?.sizing ?? { kind: 'free' }
}
```

`itemId` is `LayoutItem.i`, which `widgetsToLayout` sets to `widget.id`. We don't extend `LayoutItem` because RGL ships that type and a side map is one line.

---

## 4. `dashboard-grid.tsx` integration

The grid's responsibility:

1. Build `baseLayout` from `widgetsToLayout(widgets)`. Apply the existing `h *= SCALE` and other fine-unit conversions.
2. For each item: look up the strategy, run `normalizeRenderItem` for idle h/minH/maxH (Layer A), then attach `constraints` / `resizeHandles` / `isResizable` from `applyStrategy` for resize-time enforcement (Layer B).
3. `updateLiveLayout` snaps y to coarse multiples for all items, and h to coarse multiples **only for `free` and `fixed` strategies**. Aspect-square / content-height widgets keep their constraint-derived fine h to avoid jitter from rounding the pixel-square value to a coarse boundary.
4. `commitLayoutChange` runs `snapOnRelease` per item to produce the persistence patch (coarse units), and re-runs `normalizeRenderItem` on the live layout before `setLiveLayout` so the on-screen cell matches what the next render will produce (no visible jump on release).

Sketch:

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
    const normalized = normalizeRenderItem(item, strategy, { containerWidth: width, autoMeasuredFineH: measured })
    item.h = normalized.h
    item.minH = normalized.minH
    item.maxH = normalized.maxH

    const desc = applyStrategy(strategy, measured)
    item.constraints = desc.constraints
    if (desc.resizeHandles) item.resizeHandles = desc.resizeHandles
    if (!desc.isResizable) item.isResizable = false
  }
  return deoverlapLayout(layout)
}, [widgets, autoUnits, width])

const updateLiveLayout = useCallback((nextLayout) => {
  const snapped = nextLayout.map((item) => {
    const strategy = getStrategy(item.i)
    const base = { ...item, y: Math.round(item.y / SCALE) * SCALE }
    if (strategy.kind === 'free' || strategy.kind === 'fixed') {
      // Snap h to SCALE multiples for coarse-h widgets; prevents resize jitter
      // before the coarse persistence round-trip. Aspect-square / content-height
      // own their fine h (constraints / measurement) so we leave it alone.
      base.h = Math.max(SCALE, Math.round(item.h / SCALE) * SCALE)
    }
    return base
  })
  setLiveLayout(deoverlapLayout(snapped))
}, [/* deps */])

const commitLayoutChange = useCallback((finalLayout) => {
  setInteractionState('idle')
  const snapped = finalLayout.map((item) => {
    const strategy = getStrategy(item.i)
    let base = { ...item, y: Math.round(item.y / SCALE) * SCALE }
    if (strategy.kind === 'free' || strategy.kind === 'fixed') {
      base.h = Math.max(SCALE, Math.round(item.h / SCALE) * SCALE)
    }
    // Apply tier snap for aspect-square (operates in coarse, returns a coarse patch).
    const snap = snapOnRelease(base, strategy, { containerWidth: width })

    // Re-normalize so the live layout matches the next baseLayout render.
    return normalizeRenderItem(applyCoarsePatch(base, snap), strategy, {
      containerWidth: width,
      autoMeasuredFineH: autoUnits[item.i]
    })
  })
  setLiveLayout(deoverlapLayout(snapped))
  const coarsePatch = snapped.map((item) => ({
    id: item.i,
    grid_x: item.x,
    grid_y: Math.round(item.y / SCALE),
    grid_w: item.w,
    grid_h: Math.round(item.h / SCALE)
  }))
  // dispatch persistence …
}, [/* deps */])
```

`applyCoarsePatch(item, patch)` is a tiny helper: takes `{ w?, h? }` in coarse units and applies them to a fine-layout item (with `h *= SCALE`). Lives in `sizing-strategies/normalize.ts`.

`SQUARE_TYPES` / `AUTO_HEIGHT_TYPES` / `interactionStateRef` / `visualSquareHFine` (the inline copy) all go away.

---

## 5. `snapOnRelease` — coarse persistence patch

Unit contract: returns coarse-unit `{ w?, h? }` override that the commit pipeline applies to the layout item.

```ts
// sizing-strategies/index.ts
export interface SnapPatch {
  w?: number   // coarse
  h?: number   // coarse
}

export function snapOnRelease(
  item: LayoutItem,    // fine-unit live item
  strategy: SizingStrategy,
  ctx: { containerWidth: number }
): SnapPatch {
  switch (strategy.kind) {
    case 'aspect-square': {
      const tier = nearestTier(item.w, strategy.tiers)
      return { w: tier, h: tier }   // coarse units
    }
    case 'free':
    case 'fixed':
    case 'content-height':
      return {}
  }
}

function nearestTier(value: number, tiers: number[]): number {
  return tiers.reduce(
    (best, t) => (Math.abs(t - value) < Math.abs(best - value) ? t : best),
    tiers[0]
  )
}
```

Tie-break (e.g., value `3.5` with tiers `[3, 4]`): first tier wins (`3`) — the conservative choice (avoid accidental upgrades).

`applyCoarsePatch` applies the snap to the fine-unit live item:

```ts
function applyCoarsePatch(item: LayoutItem, patch: SnapPatch): LayoutItem {
  return {
    ...item,
    w: patch.w ?? item.w,
    h: patch.h !== undefined ? patch.h * SCALE : item.h
  }
}
```

After this, `commitLayoutChange` re-runs `normalizeRenderItem` to bring the fine h back to the pixel-square value for the new w. The live layout and the next baseLayout agree.

---

## 6. `widgetsToLayout` clamp pass

The existing min/max clamp covers `grid_w` and `grid_h` going out of range. Extend it for `aspect-square` widgets so historical data with `grid_w !== grid_h` snaps to the nearest tier on first load:

```ts
import { WIDGET_TYPE_MAP } from '@/lib/widget-types'
import { nearestTier } from '@/components/dashboard/sizing-strategies'

// inside widgetsToLayout, after the existing min/max clamp:
const strategy = WIDGET_TYPE_MAP.get(widget.widget_type)?.sizing
if (strategy?.kind === 'aspect-square') {
  const tier = nearestTier(Math.max(w, h), strategy.tiers)
  w = h = tier
}
```

Render-time only; no DB write. Persistence happens lazily on the next drag/resize.

---

## 7. Deletions

The refactor removes:

- `SQUARE_TYPES` constant in `dashboard-grid.tsx`
- `AUTO_HEIGHT_TYPES` constant in `dashboard-grid.tsx`
- `squareIdSet` and `autoIdSet` memos
- `visualSquareHFine` and its three inline override sites (`baseLayout`, `updateLiveLayout`, `commitLayoutChange`)
- `interactionStateRef` ref and the resizing/idle branching it gated
- The `if (minW === maxW && minH === maxH) item.isResizable = false` block in `dashboard-layout.ts`
- Per-strategy `base.h` overrides in `updateLiveLayout` and `commitLayoutChange`

The `pixelSquareFineH` formula is preserved (renamed and moved) inside `sizing-strategies/normalize.ts` — it's still needed for Layer A, just centralized.

---

## 8. `widget-types.ts` changes

Add the `sizing` field to every entry. **No bounds changes** in this iteration — existing `minW/maxW/minH/maxH` stay exactly as they are today so no user's widgets shrink unexpectedly.

```diff
  {
    id: 'gauge',
    ...
+   sizing: { kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] },
  },
  {
    id: 'top-n',
    ...
+   sizing: { kind: 'content-height' },
  },
  {
    id: 'stat-number',
    ...
+   sizing: { kind: 'fixed' },
  },
  // every other widget:
+   sizing: { kind: 'free' },
```

Bounds tightening for `markdown` / `server-cards` is out of scope; ship it later if needed.

---

## 9. Testing

### Unit tests (vitest)

- `sizing-strategies/normalize.test.ts`
  - `normalizeRenderItem` aspect-square: returns `h = pixelSquareFineH(w, containerWidth)`
  - aspect-square: returns `minH = pixelSquareFineH(minW)`, `maxH = pixelSquareFineH(maxW)`
  - content-height: when `autoMeasuredFineH` present → h/minH/maxH all = measured
  - content-height: when measured undefined → returns item unchanged
  - fixed: returns item unchanged
  - free: returns item unchanged

- `sizing-strategies/index.test.ts`
  - `applyStrategy('aspect-square')` returns `constraints: [aspectRatio(1)]` and `resizeHandles: ['se']`
  - `applyStrategy('content-height', 11)` returns `[lockHeight(11)]` and `['e']`
  - `applyStrategy('fixed')` returns `isResizable: false`
  - `applyStrategy('free')` returns empty constraints, undefined handles (RGL default)
  - `snapOnRelease(aspect-square)` returns `{ w: tier, h: tier }` with `tier = nearestTier(item.w, tiers)`
  - `snapOnRelease` for other strategies returns `{}`

- `dashboard-layout.test.ts` (new)
  - `widgetsToLayout` snaps aspect-square widgets with `grid_w !== grid_h` to the nearest tier
  - `widgetsToLayout` preserves widgets at legal tiers

### Manual verification

New file `tests/manual/dashboard-widget-sizing.md`:

- Gauge resize: drag SE from 2×2 toward 6×6. The cell stays pixel-square throughout (RGL `aspectRatio(1)` constraint enforces it during drag). On release, the size snaps to the nearest tier in `[2, 3, 4, 5, 6]`.
- Gauge mounts pixel-square (Layer A) — verify on page load before any user interaction.
- Gauge with two side-by-side gauges: `preventCollision` blocks growth into the neighbor (existing RGL behavior).
- Container width changes (sidebar collapse / window resize): cells visually scale, persisted `grid_w/h` unchanged.
- Top-N: cell height tracks content via `autoMeasuredFineH`; only `e` handle visible.
- Stat-number: not resizable, no handles.
- Free widgets (line-chart, etc.): resize feels unchanged from today (h snaps to coarse multiples on release).

---

## 10. Documentation

New file `docs/dashboard-widget-checklist.md`:

````markdown
# Adding a New Dashboard Widget

## Required
1. `widget-types.ts` — `WidgetTypeDefinition` entry with `sizing` set (TS enforces this)
2. `widget-renderer.tsx` — add the switch case
3. `widget-picker` — icon + i18n label
4. `widget-config-dialog.tsx` — config form (if any)
5. `locales/{en,zh}/dashboard.json` — i18n strings

## Optional
- Widget component: `apps/web/src/components/dashboard/widgets/<name>.tsx`
- Vitest test: `apps/web/src/components/dashboard/widgets/<name>.test.tsx`

## Picking a `sizing` strategy
- Most widgets → `{ kind: 'free' }`
- Single-value, one viable size → `{ kind: 'fixed' }` (also set minW=maxW, minH=maxH)
- Radial / gauge / aspect-locked → `{ kind: 'aspect-square', tiers: [...] }`
- List / table with content-driven height → `{ kind: 'content-height' }`
````

---

## 11. Risk assessment

| Risk                                                                | Mitigation                                                                              |
| ------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Existing gauge instances with `grid_w !== grid_h` change shape      | `widgetsToLayout` snaps to nearest tier on first load. No data loss; cell looks more square — visual improvement. |
| Refactor touches dashboard-grid core flow                           | `sizing-strategies/` is independent and unit-tested. `dashboard-grid` becomes a thin caller. |
| RGL `aspectRatio(1)` drifts from `pixelSquareFineH`                 | We use the same formula in both. Test `pixelSquareFineH(w, width)` against `aspectRatio(1).constrainSize` output for representative widths. |
| `react-grid-layout/core` subpath not exported in some build configs | Verified in v2.2.2 `package.json` exports. Add a type-only import test to catch regressions on upgrade. |
| Tier snap mid-drag jumps                                            | Snap runs only in `commitLayoutChange`, not `updateLiveLayout`. Drag feels free; release snaps. |
| Free-widget resize jitter from removing the h-snap                  | Kept: `updateLiveLayout` still snaps h to SCALE multiples for `free` and `fixed`. Only aspect-square / content-height bypass it. |

---

## 12. Out of scope (deliberate)

- iOS-style multi-tier widget UIs (separate small/medium/large layouts)
- Backend-side sizing override per widget instance
- Per-instance aspect-ratio override (e.g., a 2:1 variant of gauge)
- Drag-time mid-snap for aspect-square (snap only happens on release)
- Animated snap-to-tier transition (instant)
- A `useWidgetSize` hook (CSS container queries cover current widgets)
- Tightening `markdown` / `server-cards` bounds (separate PR with migration discussion)

---

## 13. File diff summary

| File                                                            | Change                                                                              |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| `apps/web/src/lib/widget-types.ts`                              | Add `SizingStrategy` union and the required `sizing` field on every entry           |
| `apps/web/src/components/dashboard/sizing-strategies/`          | **New** directory: `normalize.ts` (Layer A) + `index.ts` (Layer B, exports `applyStrategy` / `snapOnRelease` / `nearestTier` / `applyCoarsePatch`) |
| `apps/web/src/components/dashboard/sizing-strategies/normalize.test.ts` | **New** vitest for Layer A                                                  |
| `apps/web/src/components/dashboard/sizing-strategies/index.test.ts`     | **New** vitest for Layer B                                                  |
| `apps/web/src/components/dashboard/dashboard-grid.tsx`          | Delete `SQUARE_TYPES`/`AUTO_HEIGHT_TYPES`/`visualSquareHFine`/`interactionStateRef`; add `widgetById` map + Layer A normalize + Layer B constraints attachment; keep h snap in `updateLiveLayout` for `free` / `fixed` |
| `apps/web/src/components/dashboard/dashboard-layout.ts`         | Delete inline `isResizable` check; add aspect-square `nearestTier` snap in `widgetsToLayout`        |
| `apps/web/src/components/dashboard/dashboard-layout.test.ts`    | **New** vitest                                                                       |
| `apps/web/src/components/dashboard/widgets/gauge.tsx`           | No change (strategy lives in the dispatcher)                                        |
| `docs/dashboard-widget-checklist.md`                            | **New** contributor checklist                                                       |
| `tests/manual/dashboard-widget-sizing.md`                       | **New** manual verification checklist                                               |

No backend changes. No DB migrations. No env vars. No bounds changes to existing widget definitions.
