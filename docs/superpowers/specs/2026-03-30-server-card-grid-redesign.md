# Server Card Grid Redesign

## Goal

Refactor the server list grid card (`server-card.tsx`) to replace monotonous progress bars with ring charts, increase information density by showing all available metrics, and replace sparkline charts with uptime-style vertical bar charts for network quality visualization.

## Current State

The existing `ServerCard` component displays:
- Header: country flag + OS icon + server name + online/offline badge
- 3 horizontal progress bars: CPU, Memory, Disk
- 4 compact metrics: Load, Process count, TCP, UDP
- Conditional swap bar + uptime text
- Bottom section: network speeds + total traffic + sparkline charts for latency/packet loss

Problems:
- Three progress bars look monotonous and dominate the card
- Information density is low — swap and uptime are awkwardly placed
- Network quality sparklines are small and easy to miss

## New Design

### Card Structure (top to bottom)

#### 1. Header (unchanged)
- Country flag emoji + OS icon emoji + server name (truncated) + `StatusBadge`
- Same layout and styling as current

#### 2. Ring Charts Row
- Three SVG donut/ring charts displayed horizontally with `justify-around`
- Each ring: ~56px diameter (shrinks to 40px when card width < 200px), 3.5px stroke, percentage centered inside, label below
- Metrics: **CPU** / **MEM** / **DISK**
- Color logic (stroke color uses CSS custom properties, not Tailwind classes, because SVG `stroke` requires actual color values):
  - `>90%`: `#ef4444` (red)
  - `>70%`: `#f59e0b` (amber)
  - `<=70%`: `var(--color-chart-1)` for CPU, `var(--color-chart-2)` for MEM, `var(--color-chart-3)` for DISK
- SVG approach: two `<circle>` elements — background track + foreground arc via `stroke-dasharray`

#### 3. System Metrics Row (5 columns)
- Grid: `grid-cols-5`, light muted background, rounded corners
- Columns: **Load** | **Proc** | **TCP** | **UDP** | **Swap**
- Each cell: 9px label on top, 12px bold value below, center-aligned
- Swap shows percentage (e.g., "24%"), displays "0%" when swap_total is 0
- i18n keys: `card_load`, `card_processes`, `card_tcp`, `card_udp`, `card_swap` (all exist)

#### 4. Network Metrics Row (4 columns)
- Grid: `grid-cols-4`, same background style as row above, slight gap between rows
- Columns: **↓ In** | **↑ Out** | **Traffic** | **Uptime**
- Speed uses `formatSpeed()`, traffic uses `formatBytes()`, uptime uses `formatUptime()`
- i18n keys: new `card_net_in_speed` / `card_net_out_speed` (short labels for card context), reuse `card_net_total`, `col_uptime`

#### 5. Network Quality Section
- Separated by a top border line
- Two sub-sections: **Latency** and **Packet Loss**
- Each sub-section:
  - Header line: label (left) + current average value (right, color-coded)
  - Below: uptime-style bar chart (20 vertical bars)
- Conditional: only rendered when `latencyData.length > 0` (same as current card behavior)

### Network Data Merging Rules

The `useNetworkRealtime` hook returns `{ [targetId: string]: NetworkProbeResultData[] }`. To produce the 20-bar chart:

1. **Merge**: `Object.values(data).flat()` to get all results across targets
2. **Sort**: by `timestamp` ascending (chronological order, not target-order-dependent)
3. **Slice**: take the last 20 entries
4. **Extract latency**: `result.avg_latency` — if `null` (probe failure/timeout), treat as a sentinel value
5. **Extract packet loss**: `result.packet_loss * 100` (convert from 0-1 to percentage)
6. **Null latency handling in UptimeBar**: when `avg_latency === null`, render the bar at full height with the red failure color (`#ef4444`), representing a probe timeout/failure
7. **Average calculation**: for the displayed average value, exclude null entries from the mean. If all entries are null, display "-"

### New Component: `RingChart`

Location: `apps/web/src/components/ui/ring-chart.tsx`

```typescript
interface RingChartProps {
  value: number      // 0-100 percentage
  size?: number      // diameter in px, default 56
  strokeWidth?: number // default 3.5
  color: string      // CSS color value or custom property, e.g. 'var(--color-chart-1)' or '#ef4444'
  label: string      // text below ring
}
```

Implementation:
- SVG `viewBox="0 0 36 36"`, rotated -90deg for top start
- Background circle: `stroke` set to muted color (e.g., `rgba(128,128,128,0.15)`)
- Foreground circle: `stroke-dasharray` calculated as `(value / 100) * circumference`, remainder = `circumference`, with `stroke-linecap="round"`
- Center text: percentage with `font-semibold text-xs`
- Label below: `text-[10px] text-muted-foreground`
- The parent `ServerCard` computes the threshold-based color and passes it as a CSS color string

### New Component: `UptimeBar`

Location: `apps/web/src/components/ui/uptime-bar.tsx`

```typescript
interface UptimeBarProps {
  data: (number | null)[]  // array of values; null = probe failure
  height?: number          // container height, default 16
  getColor: (value: number | null) => string  // maps value to CSS color string
  maxValue?: number        // optional max for height scaling; if omitted, uses max of non-null data
}
```

Implementation:
- Flex container with `gap-0.5` (or 2px), items aligned to bottom (`items-end`)
- Each bar: `flex: 1`, `border-radius: 2px`, height proportional to value/maxValue
- Null values: rendered at 100% height with the color returned by `getColor(null)` (failure indicator)
- Minimum height for non-null non-zero values: 10% (so small values are still visible)
- Zero values: render at minimum height
- Color determined per-bar by `getColor` callback

### Color Thresholds

Latency:
- Green (`#10b981`): <50ms
- Amber (`#f59e0b`): <100ms
- Red (`#ef4444`): >=100ms or null (probe failure)

Packet Loss:
- Green (`#10b981`): <1%
- Amber (`#f59e0b`): <5%
- Red (`#ef4444`): >=5%

Ring charts:
- Brand color (CSS variable): <=70%
- Amber (`#f59e0b`): >70%
- Red (`#ef4444`): >90%

### Files Changed

| File | Change |
|------|--------|
| `apps/web/src/components/ui/ring-chart.tsx` | New — RingChart component |
| `apps/web/src/components/ui/uptime-bar.tsx` | New — UptimeBar component |
| `apps/web/src/components/server/server-card.tsx` | Rewrite — new layout using RingChart + UptimeBar |
| `apps/web/src/locales/en/servers.json` | Add `card_net_in_speed`, `card_net_out_speed` keys |
| `apps/web/src/locales/zh/servers.json` | Add `card_net_in_speed`, `card_net_out_speed` keys |

### Files NOT Changed

- `components/dashboard/widgets/server-cards.tsx` — uses `ServerCard`, no wrapper changes needed
- `compact-metric.tsx` — still used in the metrics rows
- `status-badge.tsx` — still used in header
- `sparkline.tsx` — still used in server detail page; not deleted
- `use-servers-ws.ts` — data layer unchanged
- `use-network-realtime.ts` — data layer unchanged
- Servers list page (`routes/_authed/servers/index.tsx`) — grid wrapper unchanged

### Responsive Behavior

The servers page grid (`sm:grid-cols-2 lg:grid-cols-3`) is unchanged. The card adapts to its container:

- **Normal width (>=240px)**: ring charts at 56px, 5-column + 4-column metrics grids
- **Narrow width (<200px)**: ring charts shrink to 40px diameter. This is relevant for the dashboard `ServerCardsWidget` where `columns` can be configured higher than 3, or when the dashboard panel itself is narrow.

The `RingChart` component accepts a `size` prop; `ServerCard` does not need to detect width — the dashboard widget's minimum practical column count (2-3) keeps cards above 200px in all realistic layouts.

### Accessibility

- Ring chart SVG includes `role="img"` and `aria-label` with metric name and value
- UptimeBar container includes `aria-label` describing the metric trend
- Color is never the sole indicator — values are always displayed as text alongside visual elements
