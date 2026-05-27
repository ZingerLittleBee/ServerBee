---
title: Gauge Widget Visual Redesign
date: 2026-05-27
status: Draft
branch: marseille-v1
related:
  - apps/web/src/components/dashboard/widgets/gauge.tsx
  - apps/web/src/lib/widget-types.ts (GaugeConfig — unchanged)
  - apps/web/src/lib/widget-helpers.ts (METRIC_LABELS, extractLiveMetric)
---

# Gauge Widget Visual Redesign

## Goal

Replace the current recharts-based `GaugeWidget` with a hand-rolled SVG gauge that matches the reference iOS-style design: a near-full-circle ring with a two-stop gradient stroke, rounded end caps, two decorative "ball" knobs at the arc endpoints, a metric-aware icon above a colored label, a large value, and a muted subtitle. Threshold-based severity coloring is preserved (it carries operational meaning), but rendered as a gradient between two related theme hues for visual interest.

The widget's public surface (`GaugeConfig`, the metrics it accepts, its grid sizing constraints, its place in the dashboard registry) is unchanged. This is a pure visual and rendering-stack refactor.

## Non-goals

- New config fields — `GaugeConfig` stays exactly as it is.
- Animation — initial implementation is static, matching current behavior (`isAnimationActive={false}`).
- Trend indicators / ETAs / delta arrows in the subtitle — server name remains the subtitle.
- Multi-metric or stacked rings — single-metric, single-server, same as today.
- New widget category, new picker entry, or new tests of the dashboard grid.

These keep the change scoped to a single file rewrite plus a focused vitest.

---

## 1. Visual Anatomy

```
        ┌──────────────────────────┐
        │                          │
        │           ◯ ──── ◯       │   ← gradient arc (270°)
        │       ╱             ╲    │     with two ball end-caps
        │      ╱   [icon]      ╲   │
        │     │   Capacity      │  │   ← icon + label (gradient-start color)
        │     │                 │  │
        │     │     68.0%       │  │   ← big value (foreground)
        │     │                 │  │
        │      ╲   server-01   ╱   │   ← subtitle (muted)
        │       ╲             ╱    │
        │          ╲ ─── ╱         │
        │                          │
        └──────────────────────────┘
```

Stack (centered, vertical):

1. **Icon** — lucide-react, ~14–16px at default size, colored to match the gradient's start stop.
2. **Label** — small (`text-xs`/`text-sm`), same color as the icon.
3. **Value** — large numeric, foreground color, `text-2xl` to `text-4xl` depending on container.
4. **Subtitle** — `truncate text-center text-muted-foreground text-xs`, server name (preserved from current behavior).

Ring:

- Sweep **270°** with the gap centered at the top (start angle `135°`, end angle `45°` measured clockwise from 12 o'clock — equivalent to the reference image's gap orientation).
- Two SVG `<path>` elements:
  - **Track**: full 270° sweep, `stroke=var(--color-muted)`, low opacity.
  - **Progress**: from start angle to `start + (value/max) * 270°`, `stroke=url(#gauge-gradient-{id})`.
- `stroke-linecap="round"` on both.
- Two `<circle>` end-caps placed at the progress arc's start and end. Each is a white circle with a thin colored inner dot — the inner dot's color is sampled from the gradient at that endpoint.

## 2. Color Strategy

Threshold-based severity is **preserved**, but each state pairs two related theme hues into a gradient. All colors come from existing `--chart-*` CSS variables so theme switching keeps working automatically.

| Value range | Gradient start → end       | Semantic       |
| ----------- | -------------------------- | -------------- |
| `< 70%`     | `--chart-1` → `--chart-2`  | Normal (cool)  |
| `70%–<90%`  | `--chart-3` → `--chart-5`  | Warning (warm) |
| `>= 90%`    | `--chart-4` → `--chart-3`  | Critical (hot) |

The label, icon, and end-cap inner dots use the **gradient start color** (single solid pick, not the gradient itself) so they remain readable on top of the dark card.

Rationale for the thresholds: matches the existing `getGaugeColor` ranges so no operational tuning is lost. The new piece is just rendering as a gradient pair instead of a single solid color.

## 3. Component Structure

Single file: `apps/web/src/components/dashboard/widgets/gauge.tsx`.

No sub-component extraction unless the file exceeds ~180 lines after rewrite — currently it's ~80 lines, the rewrite is expected around ~140–160 lines.

### Exports

`GaugeWidget(props: { config: GaugeConfig; servers: ServerMetrics[] }): JSX.Element` — same signature as today.

### Internal helpers (file-local, not exported)

- `getGaugeGradient(value: number): { start: string; end: string }` — returns CSS variables for the two stops based on threshold.
- `getMetricIcon(metric: string): LucideIcon` — maps `cpu/memory/disk/swap/load*/net_*/bandwidth` to lucide icons, default `Gauge`.
- `polarToCartesian(cx, cy, r, angleDeg): { x, y }` — for end-cap positioning and arc-path generation.
- `arcPath(cx, cy, r, startAngle, endAngle): string` — returns an SVG `d` attribute for a circular arc between the two angles.

### Icon mapping

| Metric                                | Icon          |
| ------------------------------------- | ------------- |
| `cpu`                                 | `Cpu`         |
| `memory`, `swap`                      | `MemoryStick` |
| `disk`                                | `HardDrive`   |
| `load1`, `load5`, `load15`            | `Activity`    |
| `net_in`, `net_out`, `bandwidth`      | `Network`     |
| default                               | `Gauge`       |

All from `lucide-react`, already a dependency.

### SVG structure (simplified)

```tsx
<svg viewBox="0 0 100 100" className="h-full w-full">
  <defs>
    <linearGradient id={`gauge-gradient-${uid}`} x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stopColor={gradient.start} />
      <stop offset="100%" stopColor={gradient.end} />
    </linearGradient>
  </defs>
  {/* Track */}
  <path d={trackPath} stroke="var(--color-muted)" strokeOpacity={0.35} strokeWidth={STROKE} fill="none" strokeLinecap="round" />
  {/* Progress */}
  <path d={progressPath} stroke={`url(#gauge-gradient-${uid})`} strokeWidth={STROKE} fill="none" strokeLinecap="round" />
  {/* End-cap balls (only if value > 0) */}
  <circle cx={startCap.x} cy={startCap.y} r={BALL_R} fill="white" />
  <circle cx={startCap.x} cy={startCap.y} r={BALL_R_INNER} fill={gradient.start} />
  <circle cx={endCap.x}   cy={endCap.y}   r={BALL_R} fill="white" />
  <circle cx={endCap.x}   cy={endCap.y}   r={BALL_R_INNER} fill={gradient.end} />
</svg>
```

`uid` is a stable per-instance id from `useId()` so multiple gauges on one page don't collide on the gradient ref.

Constants (in viewBox units, 100×100):
- `RADIUS = 38`
- `STROKE = 8`
- `BALL_R = 5.5`
- `BALL_R_INNER = 2`
- Sweep: `startAngle = 135°`, `endAngle = 45°` (clockwise, 270° total)

The text stack (icon + label + value + subtitle) sits in a centered absolutely-positioned `<div>` inside the same flex column, layered above the SVG via `position: relative` on the wrapper. Text never lives inside the SVG — this lets Tailwind's responsive font sizing and `truncate` work as usual.

## 4. Responsive Sizing

The widget grid range is 2×2 → 6×6. The redesign must look good at all sizes.

Two parts:

- **SVG**: always fills its container via `viewBox=0 0 100 100` and `h-full w-full`. Scales linearly.
- **Text overlay**: Tailwind responsive classes keyed off container queries (Tailwind v4 supports `@container`).
  - Value: `text-2xl @md:text-3xl @lg:text-4xl`
  - Label: `text-xs @md:text-sm`
  - Icon: `h-3.5 w-3.5 @md:h-4 @md:w-4 @lg:h-5 @lg:w-5`
  - Subtitle: hidden below `@xs`; shown otherwise.

The wrapping card gets `@container/gauge` so we don't depend on viewport size — multiple gauges of different grid sizes can sit side-by-side and each scale independently.

If container queries turn out not to be wired up in the project's Tailwind config, fallback is to use `ResizeObserver` + a `useState` for a discrete size bucket (`sm | md | lg`). Decision made during implementation, not in this spec.

## 5. State Handling

Same as today:

- If `server` is not found in `servers`, render the existing "Server not found" empty state (border + muted text). No gauge ring drawn.
- `value` is clamped to `[0, max]`. If `value === 0`, skip rendering the progress path and the two end-cap balls — only the track is shown. This avoids a degenerate zero-length arc with two overlapping balls at the start angle.

## 6. Dependency Change

Remove the `recharts` imports from `gauge.tsx`:

```ts
// before
import { PolarAngleAxis, RadialBar, RadialBarChart, ResponsiveContainer } from 'recharts'
// after
import { useId, useMemo } from 'react'
import { Activity, Cpu, Gauge as GaugeIcon, HardDrive, MemoryStick, Network } from 'lucide-react'
```

Other widgets still use recharts (`line-chart-widget`, `multi-line`, `disk-io`, `traffic-bar`, etc.), so `recharts` stays in the project dependencies. We are only removing the use *from this one widget*.

## 7. Testing

Add `apps/web/src/components/dashboard/widgets/gauge.test.tsx` (vitest + RTL, same pattern as `stat-number.test.tsx`):

- Renders "Server not found" when `server_id` doesn't match any server.
- Renders the configured label and the server's name (subtitle).
- Renders the formatted percentage (`68.0%`) for a known metric value.
- Threshold transitions: with values `50 / 75 / 95`, the rendered SVG `<linearGradient>` has the expected `--chart-*` stop colors. Assert via `getByTestId('gauge-gradient')` and reading the `<stop>` children's `stopColor` attribute.
- Clamps values: when `extractLiveMetric` returns a value > `max`, the rendered text shows `max.toFixed(1)%`.
- Zero state: when `value === 0`, the progress `<path>` and end-cap `<circle>` group are absent.

The existing tests under `apps/web/src/components/dashboard/widgets/` use vitest + `@testing-library/react`. The new test file follows the same setup — no new harness needed.

Manual visual verification (per project policy on UI changes): start `bun run dev`, add a Gauge widget to a dashboard, eyeball it against the reference image, and re-check at min (2×2) and max (6×6) grid sizes. Note in the PR description if visual confirmation was done.

## 8. Out-of-scope follow-ups (deliberate)

- Animating value changes (`requestAnimationFrame` tween).
- Configurable gradient direction or angle.
- Showing a trend arrow / delta vs. 5-min ago.
- Letting users pick the icon per widget instance.
- Click-through to a detail view from the gauge.

These can each ship later as small, independent improvements. None of them affect the data model or the API surface.

## 9. File diff summary

| File                                                              | Change                                  |
| ----------------------------------------------------------------- | --------------------------------------- |
| `apps/web/src/components/dashboard/widgets/gauge.tsx`             | Rewritten (recharts → SVG)              |
| `apps/web/src/components/dashboard/widgets/gauge.test.tsx`        | **New** — vitest coverage               |
| `apps/web/src/lib/widget-types.ts`                                | Unchanged                               |
| `apps/web/src/lib/widget-helpers.ts`                              | Unchanged                               |
| `apps/web/src/components/dashboard/widget-renderer.tsx`           | Unchanged (still routes `gauge` → `GaugeWidget`) |

No backend changes. No migrations. No new env vars. No docs updates (CN/EN docs don't reference the gauge widget's visual style).
