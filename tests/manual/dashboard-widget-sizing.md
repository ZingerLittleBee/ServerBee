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
