# Metric Card Widget — Design

**Date:** 2026-05-27
**Status:** Draft
**Surface:** `apps/web` dashboard

## 1. Motivation

The dashboard currently offers a thin `stat-number` widget (one value + an icon) and a much larger `line-chart` widget (full trend chart). There is no middle-weight widget that shows current load **plus** a short-term trend **plus** a long-term reference at a glance.

This spec adds `metric-card`, a square-ish info-dense card inspired by the iOS Bitcoin widget: header, large current value, short-term delta, sparkline, and two long-term sub-stats. Four metrics are supported on a single widget type, configurable per instance: CPU, memory, network throughput, and disk I/O throughput.

## 2. Scope

**In scope**

- A new dashboard widget type `metric-card` registered in `WIDGET_TYPES`.
- One widget instance binds to a single server and a single metric.
- Four metrics: `cpu`, `memory`, `network`, `disk_io`.
- 24-hour history-derived peak and average sub-stats.
- 1-hour delta indicator below the main value.
- Sparkline rendered from the same 24h history series, with the most recent live tick spliced in for liveness.

**Out of scope**

- Aggregating across all servers (`stat-number` already covers fleet-wide summaries).
- Configurable sparkline window or interval (fixed at 24h / 5m bucketing so all derived stats line up).
- Per-card alert thresholds or threshold lines (line-chart widget territory).
- Backend changes — all required endpoints already exist.

## 3. Visual Design

Default grid footprint: **4 columns × 4 rows** on the 12-col dashboard. Min 3×3, max 6×6.

```
┌──────────────────────────────────┐
│ [icon] CPU            · server-A │   header
│                                  │
│  45.2%                           │   primary value (large)
│  ▲ +2.1pp · past 1h              │   delta (small)
│                                  │
│       ╱╲    ╱─╲                  │   sparkline area
│  ────╯  ╲──╯   ╲────             │
│                                  │
│ ┌────────────┬────────────┐      │   sub-cards
│ │ 24H PEAK   │ 24H AVG    │      │
│ │  78.4%     │  41.7%     │      │
│ └────────────┴────────────┘      │
└──────────────────────────────────┘
```

**Sections (top to bottom):**

1. **Header** — metric icon (lucide: `Cpu` / `MemoryStick` / `Network` / `HardDriveDownload`), metric label, server name on the right.
2. **Primary value** — current reading, large display font.
3. **Delta line** — `▲/▼ value · past 1h`, color-coded.
4. **Sparkline** — recharts `<Area>` filling the metric color at low opacity, ~60px tall in the default size.
5. **Sub-cards** — two flex-equal rounded surfaces with uppercase caption + value: `24H PEAK`, `24H AVG`.

**Color tokens** (Tailwind / shadcn theme):

| Metric | Accent token |
|---|---|
| cpu | `--chart-4` (existing CPU accent) |
| memory | `--chart-3` |
| network | `--chart-1` |
| disk_io | `--chart-2` |

**Delta colors:**

- `cpu` / `memory` — semantic: up = `text-destructive`, down = `text-emerald-500` (rising utilization is a stress signal).
- `network` / `disk_io` — neutral: always `text-muted-foreground`, just an arrow for direction (throughput swings are neither good nor bad).

## 4. Data Model

### Per-metric formatting

| Metric | Value source (live) | Value source (history) | Display |
|---|---|---|---|
| `cpu` | `server.cpu` | `record.cpu` | `45.2%` |
| `memory` | `mem_used / mem_total * 100` | `record.mem_used / server.mem_total * 100` | `67.0%` |
| `network` | `net_in_speed + net_out_speed` | `record.net_in_speed + record.net_out_speed` | `124 MB/s` (via `formatSpeed`) |
| `disk_io` | `disk_read_speed + disk_write_speed` | from `buildMergedDiskIoSeries` summed | `34 MB/s` |

Reuse `extractLiveMetric` / `extractRecordMetric` in `lib/widget-helpers.ts`; extend both with `network` (alias of existing `bandwidth`) and `disk_io` cases.

### Delta calculation

```
delta = current_value - value_1h_ago
```

- For `cpu` / `memory` the unit is `pp` (percentage points): `▲ +2.1pp`.
- For `network` / `disk_io` the unit is relative percent: `▲ +18%`.
- If the 1h-ago sample is missing (server only just came online), show `—` instead of a delta.

### Peak / Avg

Computed over the same 24h history series used for the sparkline:

- `peak = max(series.value)`
- `avg = sum(series.value) / series.length`

When the series is empty (server offline / no data) show `—` for both.

## 5. Data Flow

```
useServerRecords(server_id, 24, '5m')      ← 24h history, refetch every 5 min
        │
        ▼
useMetricSeries(records, metric, server)   ← derives sparkline points,
        │                                    peak, avg, 1h-ago value
        ▼
MetricCardWidget renders                    ← splices the live tick from
                                              useServersWs onto the tail
```

**Live tail splicing:** `useServersWs` already pushes `ServerMetrics` updates every second. The metric card appends `{ timestamp: now, value: extractLiveMetric(server, metric) }` to the series tail in memo'd derived state. This keeps the sparkline visibly moving without re-fetching history every second.

**Loading state:** while `useServerRecords` is loading, show skeleton placeholders for value, delta, sparkline, and sub-cards.

**Offline state:** if the bound server is `online === false`, dim the card and render `—` for current value/delta but keep peak/avg from history (since the last 24h may still contain data).

## 6. Configuration

### Type

```ts
export interface MetricCardConfig {
  metric: 'cpu' | 'memory' | 'network' | 'disk_io'
  server_id: string
  label?: string  // optional override of the default metric label
}
```

### Widget registration

Add to `WIDGET_TYPES` in `lib/widget-types.ts`:

```ts
{
  id: 'metric-card',
  label: 'Metric Card',
  category: 'Real-time',
  defaultW: 4, defaultH: 4,
  minW: 3, minH: 3,
  maxW: 6, maxH: 6
}
```

### Picker entry

Appears under the "Real-time" group in `widget-picker.tsx` with a thumbnail/preview matching the layout in §3.

### Config dialog

Extend `widget-config-dialog.tsx` with a new case for `metric-card`:

- **Server** — select existing servers, required.
- **Metric** — radio group of `cpu` / `memory` / `network` / `disk_io`, required, default `cpu`.
- **Label** — optional text input, placeholder is the localized metric label.

## 7. Component Structure

```
apps/web/src/components/dashboard/widgets/
  metric-card.tsx                ← MetricCardWidget (entrypoint)
  metric-card/
    metric-card-header.tsx       ← icon + label + server name
    metric-card-value.tsx        ← large value + delta line
    metric-card-sparkline.tsx    ← recharts Area, color-token driven
    metric-card-stats.tsx        ← 24h peak / avg sub-cards
    metric-card-config.ts        ← per-metric accent, icon, formatter map

apps/web/src/hooks/
  use-metric-series.ts           ← derives sparkline + peak/avg/1h-delta
                                   from useServerRecords + live tail

apps/web/src/components/dashboard/widgets/
  metric-card.test.tsx           ← snapshot + computed-value tests
```

Files inside `metric-card/` are co-located fragments small enough that each one has a single responsibility; they are not exported elsewhere.

## 8. Integration Points

| File | Change |
|---|---|
| `lib/widget-types.ts` | Add `MetricCardConfig` interface; register `metric-card` in `WIDGET_TYPES`; include in `WidgetConfig` union. |
| `lib/widget-helpers.ts` | Add `network` and `disk_io` cases to `extractLiveMetric` and `extractRecordMetric`; add their labels and unit hints. |
| `components/dashboard/widget-renderer.tsx` | Add `case 'metric-card'` rendering `<MetricCardWidget />`. |
| `components/dashboard/widget-picker.tsx` | Add picker entry under Real-time. |
| `components/dashboard/widget-config-dialog.tsx` | Add config form section for `metric-card`. |
| `apps/web/public/locales/{en,zh}/dashboard.json` | Add `metric_card.*` keys for labels, sub-card captions, delta suffixes. |
| `components/dashboard/dashboard-editor-view.test.tsx` | Add the new widget id to existing fixtures if they enumerate types. |

No backend or `crates/` changes. Existing `/api/records` (consumed by `useServerRecords`) provides all needed data.

## 9. Testing

Unit (`metric-card.test.tsx`):

- Renders header, value, delta, sparkline, and sub-cards for each of the four metrics.
- Computes `peak` and `avg` correctly from a stubbed records list.
- Computes 1h `delta` and chooses correct unit (`pp` for cpu/memory, `%` for network/disk_io).
- Shows skeleton during loading.
- Shows `—` placeholders when the server is offline and history is empty.

`use-metric-series.test.ts`:

- Splices a live tick onto the history tail.
- Handles empty / sparse records without crashing.

Manual visual verification (per the project convention for UI work): open the dashboard, add four `metric-card` instances (one per metric) bound to the test VPS, and confirm:

- Value updates every second.
- Sparkline scrolls.
- Peak/avg stay stable across renders.
- Delta direction matches recent load change.

## 10. Open Questions / Risks

- **Sparkline density at min size (3×3):** at the smallest allowed footprint the sub-cards may crowd the chart. Mitigation: collapse the two sub-cards to a single line of `peak · avg` text when computed inner height drops below a threshold.
- **`disk_io` history shape:** confirm during implementation that `buildMergedDiskIoSeries` is reusable as a single-series source (sum of read+write); if not, add a small `extractDiskIoTotal(record)` helper next to it.
- **Locale-specific unit formatting:** `formatSpeed` already handles `MB/s` style display; verify it matches the typography weight used by the primary value or override with a tabular-nums class.
