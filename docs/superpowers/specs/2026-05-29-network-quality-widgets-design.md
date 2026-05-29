# Network Quality Dashboard Widgets — Design

Date: 2026-05-29

## Goal

Surface the existing network-probe (tri-network ping) data as dashboard widgets so
users can compose latency/packet-loss views into their dashboards. The data layer
already exists; this is a **frontend-only** feature — no backend, protocol, or
migration changes.

## Background

Network quality data comes from the network-probe subsystem (P13): each server pings
a set of targets (China Telecom / Unicom / Mobile / International) and records
`avg/min/max_latency` and `packet_loss`. The data is already exposed through:

- `useNetworkOverview()` — all servers, per-target summary + latency/loss sparklines + anomaly count
- `useNetworkServerSummary(serverId)` — one server's per-target summary (auto-refresh 60s)
- `useNetworkRecords(serverId, hours, { targetId })` — historical records for charts
- `useNetworkRealtime(serverId)` — realtime points via the global `network-probe-update` window event

The network detail page (`routes/_authed/network/$serverId.tsx`) already combines
records + realtime + a 1h seed into a single record series and renders `LatencyChart`.

## Approach

Three independent built-in widgets (matches the existing 13-widget convention; one
widget = one visual form). Rejected alternatives: a single mode-switching `network`
widget (breaks the one-form-per-widget convention, poor discoverability in the picker),
and the third-party module system (app-internal network hooks aren't reachable from modules).

## Widgets

| id | category | binding | default size | data |
|---|---|---|---|---|
| `network-latency` | Charts | single server | 6×4 | `useNetworkRecords` + realtime merge → `LatencyChart` |
| `network-quality` | Real-time | single server | 4×4 | `useNetworkServerSummary` (60s refresh) → per-target latency/loss list |
| `network-overview` | Status | many / all servers | 8×5 | `useNetworkOverview` → server×target summary table + sparkline; rows link to `/network/$serverId` |

### Config (`config_json`)

```ts
interface NetworkLatencyConfig  { server_id: string; hours?: number; target_ids?: string[] } // hours === 0 means realtime
interface NetworkQualityConfig  { server_id: string; target_ids?: string[] }
interface NetworkOverviewConfig { server_ids?: string[] } // empty/undefined = all servers
```

- Target selection: `target_ids` empty/undefined → show all targets; the config dialog
  offers checkboxes to restrict to specific targets (reusing the detail page's
  target-visibility idea).
- The latency widget's time-range dropdown gains a **Realtime** option alongside
  1h / 6h / 24h / 7d. Realtime uses `useNetworkRealtime`'s sliding window; other ranges
  use `useNetworkRecords`. Encode realtime as `hours === 0` in config to keep the field numeric.

## Shared hook (incidental improvement)

Extract the "records + realtime + 1h seed merge & dedupe" logic currently inlined in
`$serverId.tsx` (the `records` useMemo) into a reusable
`useNetworkChartRecords(serverId, range)` hook next to `use-network-realtime.ts`. Both
the detail page and the `network-latency` widget consume it, removing duplication. The
detail page is refactored to use the hook with no behavior change.

## Registration surface (per new widget)

1. `lib/widget-types.ts` — add 3 entries to `WIDGET_TYPES`, 3 config interfaces, extend the `WidgetConfig` union.
2. `dashboard/widget-renderer.tsx` — 3 imports + 3 `switch` cases.
3. `dashboard/widget-config-dialog.tsx` — 3 config forms + dispatch entries.
4. `dashboard/widget-picker.tsx` — add 3 icons to `WIDGET_ICONS` (e.g. `Network`, `Gauge`, `Globe`).
5. `dashboard/widget-render-dependencies.ts` — single-server widgets use `singleServerScope(server_id, 'name')`; overview uses `selectedServerScope(server_ids, 'name')`.
6. New components: `widgets/network-latency-widget.tsx`, `widgets/network-quality.tsx`, `widgets/network-overview-widget.tsx`.
7. i18n: `locales/{en,zh}/dashboard.json` — picker labels/descriptions + config-form labels. Network-specific copy reuses the existing `network` namespace.

## Error / empty states

- Server has no probe targets configured → empty-state message (reuse `network` namespace no-data copy).
- `server_id` points to a deleted server → `WidgetErrorBoundary` fallback + friendly empty state.
- Overview with no data → empty table message.

## Testing

- Follow existing `gauge.test.tsx` / `widget-config-dialog.test.tsx` patterns.
- Add cases for the 3 new config-form dispatches in the config-dialog test.
- Add at least one render test per widget covering the no-data fallback.
- No backend changes → no cargo tests required.

## Out of scope

- New backend endpoints or aggregation.
- Traceroute / anomaly widgets (latency + quality + overview only).
- Changes to the existing network pages beyond the shared-hook extraction.
