---
title: Dashboard Widget Sizing Strategy
date: 2026-05-28
status: Draft
branch: lisbon
related:
  - apps/web/src/lib/widget-types.ts
  - apps/web/src/components/dashboard/dashboard-grid.tsx
  - apps/web/src/components/dashboard/dashboard-layout.ts
---

# Dashboard Widget Sizing Strategy

## Goal

Formalize how widgets behave in the dashboard grid (free resize, fixed size, aspect-locked square, content-driven height) into a single explicit `sizing` field on each widget type. Replace the scattered ad-hoc markers (`SQUARE_TYPES`, `AUTO_HEIGHT_TYPES`, `isResizable` derivation) with a typed dispatcher.

The immediate trigger is the gauge widget — its aspect-locked sizing is implemented through a fragile stack of overrides spread across `dashboard-grid.tsx`, `dashboard-layout.ts`, and `gauge.tsx` that has broken resize behavior multiple times. Generalizing the model also addresses several smaller issues (missing `maxW` on `markdown`/`server-cards`, no documentation for "how to add a widget", no helper for internal UI adaptation).

## Non-goals

- Backend / database changes. `sizing` stays a client-side static config.
- Migration scripts. Out-of-range widgets are clamped silently at render time.
- iOS-style multi-tier widget UIs (small / medium / large with independent layouts). Container queries cover 80% of internal adaptation; the remaining 20% (aspect-locked widgets) are handled by the `aspect-square` strategy.
- Changes to how the grid stores `grid_x / grid_y / grid_w / grid_h` per widget.

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
| server-cards    | `{ kind: 'free' }` (maxW=12, maxH=12 added)           |
| gauge           | `{ kind: 'aspect-square', tiers: [2, 3, 4, 5, 6] }`   |
| line-chart      | `{ kind: 'free' }`                                    |
| multi-line      | `{ kind: 'free' }`                                    |
| top-n           | `{ kind: 'content-height' }`                          |
| alert-list      | `{ kind: 'free' }`                                    |
| service-status  | `{ kind: 'free' }`                                    |
| traffic-bar     | `{ kind: 'free' }`                                    |
| disk-io         | `{ kind: 'free' }`                                    |
| server-map      | `{ kind: 'free' }`                                    |
| markdown        | `{ kind: 'free' }` (maxW=8, maxH=8 added)             |
| uptime-timeline | `{ kind: 'free' }`                                    |

---

## 2. Strategy dispatcher

A new directory `apps/web/src/components/dashboard/sizing-strategies/` owns the per-strategy logic. `dashboard-grid.tsx` does not branch on widget type any more — it asks the dispatcher.

### Directory layout

```
sizing-strategies/
├── index.ts            // exports applyStrategy, snapOnRelease, types
├── free.ts
├── fixed.ts
├── aspect-square.ts
└── content-height.ts
```

### Dispatcher API

```ts
export type InteractionState = 'idle' | 'dragging' | 'resizing'

export interface StrategyContext {
  containerWidth: number
  interactionState: InteractionState
  autoMeasuredFineH?: number   // content-height: VisibilityGate measurement
}

export interface StrategyResult {
  h: number                                // fine units
  minH: number                             // fine units
  maxH: number | undefined                 // fine units
  resizeHandles?: ResizeHandleAxis[]       // undefined = RGL default
  isResizable: boolean
}

export function applyStrategy(
  item: LayoutItem,
  strategy: SizingStrategy,
  ctx: StrategyContext
): StrategyResult

export function snapOnRelease(
  item: LayoutItem,
  strategy: SizingStrategy,
  ctx: StrategyContext
): LayoutItem
```

`applyStrategy` is called in three places:

- `baseLayout` (idle): `interactionState='idle'`
- `updateLiveLayout` (during drag/resize): `interactionState='dragging' | 'resizing'`
- `commitLayoutChange` (release): once with `'idle'` after `snapOnRelease`

### `dashboard-grid.tsx` after refactor (sketch)

```ts
const baseLayout = useMemo(() => {
  const layout = widgetsToLayout(widgets)
  for (const item of layout) {
    item.y *= SCALE
    const strategy = getStrategy(item.widgetType)
    const r = applyStrategy(item, strategy, {
      containerWidth: width,
      interactionState: 'idle',
      autoMeasuredFineH: autoUnits[item.i]
    })
    item.h = r.h
    item.minH = r.minH
    item.maxH = r.maxH
    if (r.resizeHandles) item.resizeHandles = r.resizeHandles
    if (!r.isResizable) item.isResizable = false
  }
  return deoverlapLayout(layout)
}, [widgets, autoUnits, width])
```

Same shape in `updateLiveLayout` and `commitLayoutChange`, with `interactionState` reflecting the actual phase via `interactionStateRef.current`.

---

## 3. `aspect-square` resize interaction

This is the most subtle strategy and the one we've broken several times. Pin the details:

### Drag flow

1. **`onResizeStart`** → `setInteractionState('resizing')`. No special bookkeeping.

2. **`onResize` (fires every cursor mousemove)** → RGL hands us the cursor-derived `newW` (coarse), `newH` (fine). For `aspect-square`:
   - `targetCoarse = max(newW, ceil(newH / SCALE))` — take whichever axis the cursor drove further as the new edge.
   - `base.w = clamp(targetCoarse, minW, maxW)`.
   - `base.h = visualSquareHFine(base.w, ctx.containerWidth)` — pixel-square fine height for the new w.

   Both `w` and `h` are derived from `targetCoarse`. RGL's react-resizable still owns the cursor → delta computation (it does not read our overridden `h` for that), so its internal state remains consistent across frames.

3. **`onResizeStop`** → `commitLayoutChange` runs `snapOnRelease` first:
   - `targetTier = nearestTier(base.w, strategy.tiers)`.
   - Set `base.w = targetTier`, `base.h = visualSquareHFine(targetTier, ctx.containerWidth)`.
   - Persist `grid_w = grid_h = targetTier` (coarse units; `visualSquareHFine` applies again at next idle render).

### Idle h derivation (`visualSquareHFine`)

The 12-col grid step (≈ container_width / 12) is wider than the fine row step (24px), so `h_coarse = w_coarse` renders rectangular. Override at render time:

```ts
function visualSquareHFine(wCoarse: number, containerWidth: number): number {
  if (containerWidth <= 0) return wCoarse * SCALE  // pre-mount fallback
  const colStepPx = (containerWidth + MARGIN_X) / COLS
  const fineRowStepPx = ROW_HEIGHT + MARGIN_Y
  return Math.max(1, Math.round((wCoarse * colStepPx) / fineRowStepPx))
}
```

Move this helper into `sizing-strategies/aspect-square.ts`. It is not used by any other strategy.

### `nearestTier` helper

```ts
function nearestTier(value: number, tiers: number[]): number {
  return tiers.reduce((best, t) => (Math.abs(t - value) < Math.abs(best - value) ? t : best), tiers[0])
}
```

Tie-break (e.g., value `3.5` with tiers `[3, 4]`): the first tier wins (`3`), which is the conservative choice (avoid accidental upgrades).

### `aspect-square` `applyStrategy` cases

| ctx.interactionState | h                                                            | minH                       | maxH                       | resizeHandles |
| -------------------- | ------------------------------------------------------------ | -------------------------- | -------------------------- | ------------- |
| `idle`               | `visualSquareHFine(item.w, ctx.width)`                       | `visualSquareHFine(minW)`  | `visualSquareHFine(maxW)`  | `['se']`      |
| `resizing`           | `visualSquareHFine(base.w, ctx.width)` (after `w` is updated per the drag flow) | same as idle               | same as idle               | `['se']`      |
| `dragging`           | same as idle (resize and drag are exclusive)                 | same as idle               | same as idle               | `['se']`      |

### Edge cases

- **MaxW boundary**: cursor keeps moving but `targetCoarse` is clamped to `maxW`. Cell stops at the maxW × maxW visual.
- **Drag toward top-left**: RGL gives non-positive newW/newH. `targetCoarse` is computed as `max(...)`; if both axes shrink, it falls below `minW` and is clamped up to `minW`. Cell can't shrink below minW × minW.
- **`preventCollision` blocks growth**: RGL refuses the new `newW`, so the layout that reaches `onResize` already has the blocked `w`. `targetCoarse` reflects that. The cell stops growing on the blocked side; the user can still grow further only if the other axis is also unblocked (because `targetCoarse` uses the larger of the two).
- **Container width changes mid-render**: `visualSquareHFine` recomputes via the React tree. Cell visually scales with the container. `grid_w` does not change.

---

## 4. `content-height` strategy

Behaves exactly like today's `AUTO_HEIGHT_TYPES`:

```ts
export function applyContentHeight(item, ctx): StrategyResult {
  const measured = ctx.autoMeasuredFineH
  if (measured !== undefined) {
    return {
      h: measured,
      minH: measured,
      maxH: measured,
      resizeHandles: ['e'],
      isResizable: true
    }
  }
  const h = item.h * SCALE
  return {
    h,
    minH: h,
    maxH: h,
    resizeHandles: ['e'],
    isResizable: true
  }
}
```

`handleMeasure` (VisibilityGate callback) keeps writing into `autoUnits`. The strategy reads from `ctx.autoMeasuredFineH`.

---

## 5. `fixed` strategy

```ts
export function applyFixed(item, ctx): StrategyResult {
  const h = item.h * SCALE
  return { h, minH: h, maxH: h, resizeHandles: [], isResizable: false }
}
```

Replaces the `if (minW === maxW && minH === maxH) isResizable = false` block in `dashboard-layout.ts`. `stat-number` becomes the only `fixed` widget today.

---

## 6. `free` strategy

```ts
export function applyFree(item, ctx): StrategyResult {
  const minH = (item.minH ?? 1) * SCALE
  const maxH = item.maxH !== undefined ? item.maxH * SCALE : undefined
  return {
    h: item.h * SCALE,
    minH,
    maxH,
    resizeHandles: undefined,   // RGL default (all 8 corners/edges)
    isResizable: true
  }
}
```

This is the no-op baseline. Most widgets use it.

---

## 7. `widgetsToLayout` clamp pass

`dashboard-layout.ts` already clamps `grid_w` and `grid_h` to `[minW, maxW]` and `[minH, maxH]`. Extend it for `aspect-square`:

```ts
if (strategy.kind === 'aspect-square') {
  const tier = nearestTier(Math.max(w, h), strategy.tiers)
  w = h = tier
}
```

Result: any historical widget with `grid_w !== grid_h` (or with a `grid_w` not in the tiers list) is silently corrected on first render. Persistence happens lazily on the next drag/resize, so no upfront migration is needed.

---

## 8. `widget-types.ts` changes

Add bounds to the two unbounded widgets:

```diff
- { id: 'markdown', minW: 2, minH: 2 },
+ { id: 'markdown', minW: 2, minH: 2, maxW: 8, maxH: 8, sizing: { kind: 'free' } },

- { id: 'server-cards', defaultW: 12, defaultH: 6, minW: 4, minH: 3 },
+ { id: 'server-cards', defaultW: 12, defaultH: 6, minW: 4, minH: 3, maxW: 12, maxH: 12, sizing: { kind: 'free' } },
```

Add `sizing` to every entry.

---

## 9. Deletions

The refactor removes:

- `SQUARE_TYPES` constant in `dashboard-grid.tsx`
- `AUTO_HEIGHT_TYPES` constant in `dashboard-grid.tsx`
- `squareIdSet` and `autoIdSet` memos
- The inline `visualSquareHFine` and its 3-place override sites (moved into `sizing-strategies/aspect-square.ts`)
- The `if (minW === maxW && minH === maxH) item.isResizable = false` block in `dashboard-layout.ts`

`interactionStateRef` stays — multiple strategies use it.

---

## 10. Internal UI adaptation (P3)

Most widgets already adapt via CSS container queries (`@container/foo`). This stays the default approach.

For the rare case where JS-driven layout switching is needed (e.g., a chart wanting to hide its legend below 200px), add a small hook:

```ts
// apps/web/src/hooks/use-widget-size.ts
export type SizeBucket = 'xs' | 'sm' | 'md' | 'lg' | 'xl'

export interface WidgetSize {
  width: number
  height: number
  bucket: SizeBucket
}

export function useWidgetSize(
  ref: RefObject<HTMLElement>,
  buckets?: Record<SizeBucket, number>
): WidgetSize
```

Default bucket thresholds: `xs <160px, sm <240px, md <360px, lg <480px, xl ≥480px` (in widget width). Override via the `buckets` arg.

The hook is **opt-in**. No existing widget is migrated. JSDoc explains "use container queries first, this hook only when CSS isn't enough".

---

## 11. Documentation (P2)

New file `docs/dashboard-widget-checklist.md` (project doc, not a spec):

````markdown
# Adding a New Dashboard Widget

## Required
1. `widget-types.ts` — `WidgetTypeDefinition` entry with `sizing` set
2. `widget-renderer.tsx` — add the switch case
3. `widget-picker` — icon + i18n label
4. `widget-config-dialog.tsx` — config form (if any)
5. `locales/{en,zh}/dashboard.json` — i18n strings

## Optional
- Widget component: `apps/web/src/components/dashboard/widgets/<name>.tsx`
- Vitest test: `apps/web/src/components/dashboard/widgets/<name>.test.tsx`

## Picking a `sizing` strategy
- Most widgets → `{ kind: 'free' }`
- Single-value, one viable size → `{ kind: 'fixed' }`
- Radial / gauge / aspect-locked → `{ kind: 'aspect-square', tiers: [...] }`
- List / table with content-driven height → `{ kind: 'content-height' }`
````

---

## 12. Testing

### Unit tests (vitest)

- `sizing-strategies/aspect-square.test.ts`
  - drag mid-flight: `edge_fine = max(newW * SCALE, newH)`
  - `snapOnRelease`: `base.w` snaps to nearest tier in tiers list
  - clamps to `maxW` when cursor goes past
  - clamps to `minW` when cursor goes negative
  - persisted `grid_w === grid_h === tier`

- `sizing-strategies/content-height.test.ts`
  - `autoMeasuredFineH` takes priority over `item.h * SCALE`
  - `resizeHandles === ['e']`
  - `minH === maxH === h`

- `sizing-strategies/fixed.test.ts`
  - `isResizable === false`
  - `resizeHandles === []`

- `dashboard-layout.test.ts` (new)
  - `widgetsToLayout` clamps `grid_w` past `maxW` silently
  - `widgetsToLayout` snaps `aspect-square` widget with `grid_w !== grid_h` to the nearest tier

### Manual verification

New file `tests/manual/dashboard-widget-sizing.md`:

- Gauge resize: drag SE from 2×2 to 6×6, releases snap to tier
- Gauge resize: drag diagonally, cell stays square throughout
- Gauge: window resize scales cells, persisted size unchanged
- Top-N: cell height tracks content, only horizontal handle visible
- Stat-number: not resizable, no handles
- Markdown: cannot drag past width = 8 cols
- Server-cards: cannot drag past width = 12 cols

---

## 13. Risk assessment

| Risk                                                            | Mitigation                                                                                   |
| --------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| Existing gauge instances change visual size on first render     | Silent clamp + snap. No data loss. Users see a more-square cell — visual improvement, not loss. |
| Refactor touches the dashboard-grid core flow                   | `sizing-strategies/` is independent and unit-tested. `dashboard-grid` becomes a thin caller. |
| Adding `maxW` to `markdown` / `server-cards` shrinks rare wides | Clamp values (8 / 12) cover the practical range. The few that exceed are clamped once.       |
| TS discriminated union landing partially                        | `sizing` is required on `WidgetTypeDefinition`. Compile-time enforced.                       |

---

## 14. Out of scope (deliberate)

- iOS-style multi-tier widget UIs (separate small/medium/large layouts)
- Backend-side sizing override per widget instance
- Per-instance aspect-ratio override (e.g., a "2:1" gauge variant)
- Drag-time mid-snap (snap only happens on release)
- Animation on snap-to-tier (instant)

Each of these can ship later as an additional strategy or extension without breaking the model.

---

## 15. File diff summary

| File                                                            | Change                                                                |
| --------------------------------------------------------------- | --------------------------------------------------------------------- |
| `apps/web/src/lib/widget-types.ts`                              | Add `SizingStrategy` type and `sizing` field; bound markdown/server-cards |
| `apps/web/src/components/dashboard/sizing-strategies/`          | **New** directory: 4 strategy files + `index.ts`                      |
| `apps/web/src/components/dashboard/dashboard-grid.tsx`          | Delete `SQUARE_TYPES`/`AUTO_HEIGHT_TYPES`/`visualSquareHFine`; call `applyStrategy` |
| `apps/web/src/components/dashboard/dashboard-layout.ts`         | Delete inline `isResizable` check; add `aspect-square` clamp pass     |
| `apps/web/src/hooks/use-widget-size.ts`                         | **New** opt-in hook                                                   |
| `apps/web/src/components/dashboard/widgets/gauge.tsx`           | No code change (strategy moves out of gauge.tsx and into shared dispatcher) |
| `apps/web/src/components/dashboard/sizing-strategies/*.test.ts` | **New** vitest coverage                                               |
| `apps/web/src/components/dashboard/dashboard-layout.test.ts`    | **New** vitest coverage for clamp paths                               |
| `docs/dashboard-widget-checklist.md`                            | **New** contributor checklist                                         |
| `tests/manual/dashboard-widget-sizing.md`                       | **New** manual verification checklist                                 |

No backend changes. No DB migrations. No env vars.
