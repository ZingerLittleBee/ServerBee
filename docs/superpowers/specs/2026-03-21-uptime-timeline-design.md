# Uptime 90-Day Timeline — Design Spec

> Date: 2026-03-21 (rev 5)

## Overview

Add a 90-day uptime timeline bar (GitHub Status / Atlassian Statuspage style) to three locations: the public status page, server detail page, and custom dashboard widget. Each day is a color-coded segment with hover tooltip showing date, uptime percentage, online duration, and incident count.

## Existing Infrastructure

### No Changes Needed

- **`uptime_daily` table**: `server_id`, `date`, `total_minutes`, `online_minutes`, `downtime_incidents` — already populated hourly by the aggregator task.
- **`UptimeService::aggregate_daily(db)`**: Runs hourly in the aggregator task.

### Requires Fix

- **`UptimeService::get_daily(db, server_id, days)`**: Current query uses `date >= today - days`, which for `days=90` returns up to 91 calendar days (today minus 90 = 91 inclusive days). **Fix**: Change to `date > today - days` (exclusive lower bound) so `days=90` returns exactly 90 entries (from `today - 89` through `today`).
- **Gap filling**: Current `get_daily` returns sparse records — missing dates have no row. **Fix**: After querying, iterate the expected date range and fill gaps with zero-valued entries (`online_minutes: 0, total_minutes: 0, downtime_incidents: 0`). This guarantees the API always returns exactly `days` entries in date-ascending order. The gap-fill logic lives in the backend so the frontend component receives a fixed-length array.

New method signature (replaces existing):

```rust
/// Returns exactly `days` entries, one per date, gap-filled with zeros.
/// Date range: [today - (days-1), today] inclusive.
pub async fn get_daily_filled(
    db: &DatabaseConnection,
    server_id: &str,
    days: u32,
) -> Result<Vec<UptimeDailyEntry>, AppError>
```

## Shared DTO

`UptimeDailyEntry` is used by both `UptimeService` and multiple router handlers (`uptime.rs`, `status_page.rs`). Define it once in the uptime service module (`crates/server/src/service/uptime.rs`) and import from there:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UptimeDailyEntry {
    pub date: String,              // "2026-01-01"
    pub online_minutes: i32,
    pub total_minutes: i32,
    pub downtime_incidents: i32,
}
```

## Aggregate Percentage Semantics

When displaying a headline percentage across the full date range (status page row text, detail page headline, widget label), compute it from the daily entries:

```
sum(online_minutes) / sum(total_minutes) * 100
```

**Edge case — all entries have `total_minutes == 0`** (server exists but never reported data): display `"—"` (em-dash), not a numeric percentage. This avoids division-by-zero and is visually distinct from both 0% and 100%.

This replaces the existing `UptimeService::get_availability` behavior (which returns `100.0` for no data).

**Migration of `uptime_percentage` field**: The existing `ServerStatusInfo` in `crates/server/src/router/api/status_page.rs` has an `uptime_percentage: f64` field (populated by `get_availability`). **Change this field to `uptime_percentage: Option<f64>`** and compute it from `get_daily_filled` using the same sum-based formula. When all `total_minutes == 0`, set it to `None` (serialized as `null`). This keeps the field name for backwards compatibility while aligning the semantics. The old `get_availability` method is then unused and can be removed.

Frontend helper in `widget-helpers.ts`:

```typescript
export function computeAggregateUptime(days: UptimeDailyEntry[]): number | null {
  const totalMinutes = days.reduce((sum, d) => sum + d.total_minutes, 0)
  if (totalMinutes === 0) return null  // display as "—"
  return (days.reduce((sum, d) => sum + d.online_minutes, 0) / totalMinutes) * 100
}
```

All three scenes use this single function for the percentage text. `null` renders as `"—"`.

## API Changes

### New Endpoint: Per-Server Daily Uptime (Authenticated)

```
GET /api/servers/{server_id}/uptime-daily?days=90
```

- **Auth**: Required (session / API key)
- **Parameters**: `days` — optional, default 90, min 1, max 365. Values outside [1, 365] return `400 Bad Request`.
- **404**: If `server_id` does not exist in the `server` table, return `404 Not Found` (consistent with other `/api/servers/{id}/*` endpoints like `crates/server/src/router/api/server.rs:137`). An existing server with no uptime data returns `200` with all-zero entries (gap-filled).
- **Response**: `ApiResponse<Vec<UptimeDailyEntry>>`

**Route registration** (all files involved):

| File | Change |
|------|--------|
| `crates/server/src/router/api/uptime.rs` | New file. Handler: verify server exists (404 if not), parse/validate `days` param (1–365, default 90), call `UptimeService::get_daily_filled`. |
| `crates/server/src/router/api/mod.rs` | Import `uptime` module, merge `.nest("/servers/{server_id}", uptime::router())` into the authenticated API router (or add route alongside existing server sub-routes). |
| `crates/server/src/openapi.rs` | Register the new endpoint path and `UptimeDailyEntry` schema in the `ApiDoc` derive. |
| `apps/web/package.json` (openapi script) | Re-run type generation after OpenAPI spec updates (`bun run generate:api-types`). |

**Used by**: Server detail page, dashboard widget.

### Fix: ServerStatusInfo Field Naming

The backend `ServerStatusInfo` (`crates/server/src/router/api/status_page.rs:34`) uses `id`, `name`, `uptime_percentage`, but the frontend schema (`apps/web/src/lib/api-schema.ts:167`) expects `server_id`, `server_name`, `uptime_percent`. This is a pre-existing inconsistency (the frontend schema was hand-written with different names).

**Resolution**: Add `#[serde(rename = "...")]` to the backend struct to match the frontend's existing naming:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ServerStatusInfo {
    #[serde(rename = "server_id")]
    pub id: String,
    #[serde(rename = "server_name")]
    pub name: String,
    pub region: Option<String>,
    pub country_code: Option<String>,
    pub os: Option<String>,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub online: bool,
    #[serde(rename = "uptime_percent")]
    pub uptime_percentage: Option<f64>,  // changed from f64 to Option<f64>
    pub in_maintenance: bool,
    pub uptime_daily: Vec<UptimeDailyEntry>,  // new field
}
```

This avoids changing the frontend schema names and all downstream consumers. The frontend `PublicStatusPageData.servers` inline shape keeps `server_id` / `server_name` / `uptime_percent` as-is, only adding `uptime_daily`.

### Extended: Public Status Page API

`GET /api/status/{slug}` — the existing `ServerStatusInfo` gains one new field:

```rust
pub uptime_daily: Vec<UptimeDailyEntry>,
```

The handler calls `UptimeService::get_daily_filled(db, server_id, 90)` for each server in the status page's `server_ids_json` and includes the result inline. This avoids N+1 requests from the frontend.

**Data size**: 90 entries × ~50 bytes ≈ 4.5 KB per server. Acceptable even with 20+ servers.

### Database Migration: Color Threshold Config

New migration file `crates/server/src/migration/m20260321_000011_status_page_uptime_thresholds.rs`.

Add two columns to `status_page`:

```sql
ALTER TABLE status_page ADD COLUMN uptime_yellow_threshold REAL NOT NULL DEFAULT 100.0;
ALTER TABLE status_page ADD COLUMN uptime_red_threshold REAL NOT NULL DEFAULT 95.0;
```

- `uptime_yellow_threshold`: Day percentage below which segment turns yellow (default 100.0 = anything less than perfect is yellow).
- `uptime_red_threshold`: Day percentage below which segment turns red (default 95.0).

**Full data chain for threshold fields** (all files requiring changes):

| Layer | File | Change |
|-------|------|--------|
| Entity | `crates/server/src/entity/status_page.rs` | Add `uptime_yellow_threshold: f64`, `uptime_red_threshold: f64` fields |
| Migration | `crates/server/src/migration/m20260321_000011_*.rs` | New migration, register in `migration/mod.rs` |
| Service (create) | `crates/server/src/service/status_page.rs` (`CreateStatusPageInput`) | Add optional threshold fields, default to 100.0/95.0 |
| Service (update) | `crates/server/src/service/status_page.rs` (`UpdateStatusPageInput`) | Add optional threshold fields |
| Public API response | `crates/server/src/router/api/status_page.rs` (`StatusPageInfo` in `PublicStatusPageData`) | Include thresholds in public response |
| Admin API response | `crates/server/src/router/api/status_page.rs` (admin list/get response type) | Include thresholds so edit dialog can read saved values |
| Admin API handlers | `crates/server/src/router/api/status_page.rs` (create/update handlers) | Accept threshold fields in request body |
| OpenAPI | `crates/server/src/openapi.rs` | Update schema registrations for modified DTOs |
| Frontend admin type | `apps/web/src/lib/api-schema.ts` (`StatusPageItem`) | Add `uptime_yellow_threshold: number`, `uptime_red_threshold: number` |
| Frontend public type | `apps/web/src/lib/api-schema.ts` (`PublicStatusPageData.page` inline shape) | Add `uptime_yellow_threshold: number`, `uptime_red_threshold: number` |
| Frontend admin UI | `apps/web/src/routes/_authed/settings/status-pages.tsx` (create/edit dialog) | Add threshold input fields (two number inputs), read saved values from `StatusPageItem` for edit mode, use defaults (100/95) for create mode |
| OpenAPI regen | `apps/web/openapi.json` + `apps/web/src/lib/api-types.ts` | Regenerate after backend changes |

### Dashboard Widget Type

**All files requiring changes for the `uptime-timeline` widget type:**

| Layer | File | Change |
|-------|------|--------|
| Backend validation | `crates/server/src/service/dashboard.rs` (`VALID_WIDGET_TYPES`) | Add `"uptime-timeline"` |
| Frontend type def | `apps/web/src/lib/widget-types.ts` (`WIDGET_TYPES` array) | Add `{ id: 'uptime-timeline', ... }` entry |
| Frontend type def | `apps/web/src/lib/widget-types.ts` (`WidgetConfig` union) | Add `UptimeTimelineConfig` to union |
| Render dispatch | `apps/web/src/components/dashboard/widget-renderer.tsx` (`WidgetContent` switch) | Add `case 'uptime-timeline'` |
| Config dialog | `apps/web/src/components/dashboard/widget-config-dialog.tsx` (form branches) | Add `UptimeTimelineForm` component and corresponding `{widgetType === 'uptime-timeline' && ...}` branch |
| Widget picker | `apps/web/src/components/dashboard/widget-picker.tsx` (`WIDGET_ICONS`, `WIDGET_DESCRIPTIONS`) | Add icon and description for `uptime-timeline` |
| Widget component | `apps/web/src/components/dashboard/widgets/uptime-timeline-widget.tsx` | New file (widget wrapper that fetches data and renders `<UptimeTimeline>`) |
| Tests | `apps/web/src/components/dashboard/widget-renderer.test.tsx` | Add test case for `uptime-timeline` type |
| Tests | `apps/web/src/components/dashboard/widget-config-dialog.test.tsx` | Add test case for config form |

## Frontend

### Core Component: `<UptimeTimeline>`

**File**: `apps/web/src/components/uptime/uptime-timeline.tsx`

```typescript
interface UptimeDailyEntry {
  date: string
  online_minutes: number
  total_minutes: number
  downtime_incidents: number
}

interface UptimeTimelineProps {
  days: UptimeDailyEntry[]   // Fixed-length array from backend (gap-filled)
  rangeDays: number          // 30, 60, or 90 — determines segment count
  yellowThreshold?: number   // default 100
  redThreshold?: number      // default 95
  showLabels?: boolean       // "90 days ago" / "Today"
  showLegend?: boolean       // color legend below the bar
  height?: number            // segment height in px, default 28
}
```

The component renders exactly `rangeDays` segments. `days.length` must equal `rangeDays` (enforced by the backend gap-fill). The `days` array is ordered date-ascending (oldest first → leftmost segment).

**Color logic** (per segment):
- `total_minutes > 0`: compute `percentage = online_minutes / total_minutes * 100`
  - `percentage >= yellowThreshold` → green (emerald-500)
  - `percentage >= redThreshold` → yellow (amber-500)
  - `percentage < redThreshold` → red (red-500)
- `total_minutes === 0` → gray (muted) — no data for this day

**Segment rendering**: `rangeDays` `<div>` elements in a flex row with 1.5px gap. Each is an equal-width bar with border-radius 2px (GitHub style).

**Tooltip** (shadcn Tooltip on hover per segment):
```
Mar 15, 2026
99.8% uptime
23h 57m online
1 incident, 3m downtime
```

When `total_minutes === 0`: show "No data".

**Responsive**: On mobile (< 768px), the timeline can be hidden (replaced by just the percentage text) where space is constrained (status page rows). On detail pages and dashboard widgets where there's more room, it remains visible.

### Scene 1: Public Status Page (`/status/:slug`)

Replace the existing `UptimeBar` function (defined inline in `apps/web/src/routes/status.$slug.tsx`) with `<UptimeTimeline>`.

Each server row becomes:
```
[●] Server Name  [██████████████████████████████] 99.95%
```

- Data source: `ServerStatusInfo.uptime_daily` from the existing API response (now includes daily data).
- `rangeDays`: Always 90 for the public status page.
- Thresholds: Read from `page.uptime_yellow_threshold` and `page.uptime_red_threshold`.
- Percentage text: Computed via `computeAggregateUptime(uptime_daily)` → number or `"—"`.
- Mobile: Timeline hidden (existing `hidden sm:block` behavior preserved), percentage text always visible.

### Scene 2: Server Detail Page (`/servers/:serverId`)

Add an "Uptime" card in the server overview section:
- Headline: `computeAggregateUptime(days)` → `"99.95%"` or `"—"` if no data.
- `<UptimeTimeline>` with `rangeDays={90}`, `showLabels={true}` and `showLegend={true}`.
- Data source: `GET /api/servers/{id}/uptime-daily?days=90` via a new `useUptimeDaily(serverId)` hook.
- Default thresholds (100/95), not configurable per detail page.

### Scene 3: Custom Dashboard Widget

New widget type `uptime-timeline` in the dashboard system.

**Widget type definition** (in `widget-types.ts`):
```typescript
{ id: 'uptime-timeline', label: 'Uptime Timeline', category: 'Status', defaultW: 8, defaultH: 3, minW: 4, minH: 2 }
```

**Config interface** (add to `WidgetConfig` union type in `widget-types.ts`):
```typescript
interface UptimeTimelineConfig {
  server_ids: string[]    // one or more servers
  days?: number           // default 90
}
```

**Rendering**:
- **Single server** (`server_ids.length === 1`): Server name + `computeAggregateUptime` percentage (or `"—"`) + full-width timeline.
- **Multiple servers**: Vertical stack, each row: server name + timeline + percentage. Scrollable if exceeds widget height.

**Config dialog**: `ServerMultiSelect` (reuse existing component) + optional days selector (30/60/90). The `days` field in the widget config is validated to one of [30, 60, 90] by the UI; the API enforces [1, 365].

**Data fetching**: `useQueries` to fetch `GET /api/servers/{id}/uptime-daily?days=N` for each `server_id`. Refetch every 5 minutes.

### Shared Utilities

Add to the existing `apps/web/src/lib/widget-helpers.ts` (created in the P17 simplify pass):

```typescript
export function computeUptimeColor(
  onlineMinutes: number,
  totalMinutes: number,
  yellowThreshold: number,
  redThreshold: number
): 'green' | 'yellow' | 'red' | 'gray'

export function computeAggregateUptime(days: UptimeDailyEntry[]): number | null
// null → display as "—"

export function formatUptimeTooltip(entry: UptimeDailyEntry): {
  date: string
  percentage: string
  duration: string
  incidents: string
}
```

### API Client Updates

- New hook: `useUptimeDaily(serverId: string, days?: number)` in `apps/web/src/hooks/use-api.ts`.
- Frontend `apps/web/src/lib/api-schema.ts`:
  - Add `UptimeDailyEntry` interface.
  - Update `PublicStatusPageData.servers` inline shape: add `uptime_daily: UptimeDailyEntry[]`, change `uptime_percent: number | null` to match new `Option<f64>` semantics (already nullable, no change needed).
  - Update `PublicStatusPageData.page` inline shape: add `uptime_yellow_threshold: number`, `uptime_red_threshold: number`.
  - Update `StatusPageItem` interface: add `uptime_yellow_threshold: number`, `uptime_red_threshold: number` (used by admin edit dialog to read saved values).
- OpenAPI types regeneration: `cd apps/web && bun run generate:api-types` (updates `openapi.json` + `api-types.ts`).

## Testing

### Backend
- `UptimeService::get_daily_filled` — verify: returns exactly N entries, gap-fills missing dates with zeros, correct date range boundaries (no off-by-one), handles server with no data (all zeros).
- New endpoint `/api/servers/{id}/uptime-daily` — 404 for non-existent server, 200 with zero-filled entries for existing server with no data, auth required, days parameter validation (`days=0` → 400, `days=366` → 400, `days` omitted → 90), correct response shape.
- Status page API — verify `uptime_daily` included per server (90 entries each), thresholds included in page info.
- Migration — verify default threshold values (100.0/95.0).
- Status page create — verify threshold fields accepted (with defaults when omitted).
- Status page update — verify threshold fields accepted and persisted, verify admin response includes saved values.

### Frontend
- `<UptimeTimeline>` unit tests: color logic for all threshold combinations (green/yellow/red/gray), correct segment count matches `rangeDays`, tooltip content formatting, zero total_minutes renders gray.
- `computeAggregateUptime` — returns number for normal data, returns `null` when all total_minutes are zero.
- Dashboard widget: config dialog renders with ServerMultiSelect + days selector, single vs multi-server rendering, percentage displays "—" for no-data.
- Widget integration: widget-renderer dispatches to `UptimeTimelineWidget`, widget-picker shows correct icon/description.
- Status page: renders timeline instead of progress bar, thresholds applied from page config.
- Status page settings: threshold inputs appear in create/edit dialog, saved values populated in edit mode, defaults populated in create mode.

## Out of Scope

- Per-hour granularity (current daily granularity is sufficient for 90-day view).
- Animated transitions between days.
- Click-to-drill-down on individual day segments.
- Exporting uptime reports.
