# Uptime 90-Day Timeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a GitHub-style 90-day uptime timeline bar to the public status page, server detail page, and custom dashboard widget.

**Architecture:** Backend exposes daily uptime data via a new authenticated endpoint and embeds it in the existing public status page API. A shared `<UptimeTimeline>` React component renders the segmented bar across all three scenes. Color thresholds are configurable per status page.

**Tech Stack:** Rust (Axum, sea-orm, SQLite), React 19 (TanStack Query, shadcn/ui Tooltip), TypeScript

**Spec:** `docs/superpowers/specs/2026-03-21-uptime-timeline-design.md` (rev 5)

---

## File Map

### Backend — New Files
- `crates/server/src/router/api/uptime.rs` — Uptime daily endpoint handler
- `crates/server/src/migration/m20260321_000011_status_page_uptime_thresholds.rs` — Migration for threshold columns

### Backend — Modified Files
- `crates/server/src/service/uptime.rs` — Add `UptimeDailyEntry` DTO, `get_daily_filled`, remove `get_availability`
- `crates/server/src/entity/status_page.rs` — Add threshold fields
- `crates/server/src/service/status_page.rs` — Add threshold fields to create/update inputs
- `crates/server/src/router/api/status_page.rs` — Add serde renames, `uptime_daily` field, thresholds in responses
- `crates/server/src/router/api/mod.rs` — Register uptime route
- `crates/server/src/openapi.rs` — Register new endpoint + schemas
- `crates/server/src/migration/mod.rs` — Register new migration
- `crates/server/src/service/dashboard.rs` — Add `"uptime-timeline"` to VALID_WIDGET_TYPES
- `crates/server/tests/integration.rs` — Add integration tests

### Frontend — New Files
- `apps/web/src/components/uptime/uptime-timeline.tsx` — Core timeline component
- `apps/web/src/components/uptime/uptime-timeline.test.tsx` — Timeline unit tests
- `apps/web/src/components/dashboard/widgets/uptime-timeline-widget.tsx` — Dashboard widget wrapper

### Frontend — Modified Files
- `apps/web/src/lib/widget-helpers.ts` — Add uptime utility functions
- `apps/web/src/lib/api-schema.ts` — Add UptimeDailyEntry, update PublicStatusPageData, StatusPageItem
- `apps/web/src/lib/widget-types.ts` — Add UptimeTimelineConfig, WIDGET_TYPES entry
- `apps/web/src/hooks/use-api.ts` — Add useUptimeDaily hook
- `apps/web/src/routes/status.$slug.tsx` — Replace UptimeBar with UptimeTimeline
- `apps/web/src/routes/_authed/servers/$id.tsx` — Add uptime card
- `apps/web/src/routes/_authed/settings/status-pages.tsx` — Add threshold inputs
- `apps/web/src/components/dashboard/widget-renderer.tsx` — Add uptime-timeline case
- `apps/web/src/components/dashboard/widget-config-dialog.tsx` — Add UptimeTimelineForm
- `apps/web/src/components/dashboard/widget-picker.tsx` — Add icon/description
- `apps/web/src/components/dashboard/widget-renderer.test.tsx` — Add test case
- `apps/web/src/components/dashboard/widget-config-dialog.test.tsx` — Add test case

---

## Task 1: Backend — UptimeDailyEntry DTO + get_daily_filled

**Files:**
- Modify: `crates/server/src/service/uptime.rs`

- [ ] **Step 1: Add UptimeDailyEntry DTO**

Add at the top of `crates/server/src/service/uptime.rs`, after imports:

```rust
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct UptimeDailyEntry {
    pub date: String,
    pub online_minutes: i32,
    pub total_minutes: i32,
    pub downtime_incidents: i32,
}
```

Add `use serde::Serialize;` to imports if not already present.

- [ ] **Step 2: Write get_daily_filled method**

Add below the existing `get_daily` method:

```rust
/// Returns exactly `days` entries, one per date, gap-filled with zeros.
/// Date range: [today - (days-1), today] inclusive.
pub async fn get_daily_filled(
    db: &DatabaseConnection,
    server_id: &str,
    days: u32,
) -> Result<Vec<UptimeDailyEntry>, AppError> {
    let today = Utc::now().date_naive();
    let start = today - chrono::Duration::days((days as i64) - 1);

    let rows = uptime_daily::Entity::find()
        .filter(uptime_daily::Column::ServerId.eq(server_id))
        .filter(uptime_daily::Column::Date.gte(start))
        .filter(uptime_daily::Column::Date.lte(today))
        .order_by_asc(uptime_daily::Column::Date)
        .all(db)
        .await?;

    let mut row_map: std::collections::HashMap<chrono::NaiveDate, &uptime_daily::Model> =
        std::collections::HashMap::new();
    for row in &rows {
        row_map.insert(row.date, row);
    }

    let mut result = Vec::with_capacity(days as usize);
    let mut current = start;
    while current <= today {
        if let Some(row) = row_map.get(&current) {
            result.push(UptimeDailyEntry {
                date: current.format("%Y-%m-%d").to_string(),
                online_minutes: row.online_minutes,
                total_minutes: row.total_minutes,
                downtime_incidents: row.downtime_incidents,
            });
        } else {
            result.push(UptimeDailyEntry {
                date: current.format("%Y-%m-%d").to_string(),
                online_minutes: 0,
                total_minutes: 0,
                downtime_incidents: 0,
            });
        }
        current += chrono::Duration::days(1);
    }

    Ok(result)
}
```

- [ ] **Step 3: Add unit tests for get_daily_filled**

Add a new `#[cfg(test)] mod tests` block at the bottom of `crates/server/src/service/uptime.rs` (this file has no test module yet). The `setup_test_db` helper is in `crates/server/src/test_utils.rs` and is used by all other service tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;

    #[tokio::test]
    async fn test_get_daily_filled_returns_exact_count() {
        let (db, _tmp) = setup_test_db().await;
    // No uptime data — should return 90 zero-filled entries
    let result = UptimeService::get_daily_filled(&db, "nonexistent", 90).await.unwrap();
    assert_eq!(result.len(), 90);
    // All should be zero
    for entry in &result {
        assert_eq!(entry.online_minutes, 0);
        assert_eq!(entry.total_minutes, 0);
        assert_eq!(entry.downtime_incidents, 0);
    }
}

#[tokio::test]
async fn test_get_daily_filled_date_boundaries() {
    let (db, _tmp) = setup_test_db().await;
    let result = UptimeService::get_daily_filled(&db, "test", 3).await.unwrap();
    assert_eq!(result.len(), 3);
    // Verify dates are sequential ending today
    let today = Utc::now().date_naive();
    assert_eq!(result[2].date, today.format("%Y-%m-%d").to_string());
    assert_eq!(result[0].date, (today - chrono::Duration::days(2)).format("%Y-%m-%d").to_string());
}
} // mod tests
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p serverbee-server -- uptime`
Expected: All tests pass including new ones.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/uptime.rs
git commit -m "feat(uptime): add UptimeDailyEntry DTO and get_daily_filled with gap-fill"
```

---

## Task 2: Backend — New uptime-daily API endpoint

**Files:**
- Create: `crates/server/src/router/api/uptime.rs`
- Modify: `crates/server/src/router/api/mod.rs`
- Modify: `crates/server/src/openapi.rs`

- [ ] **Step 1: Create the uptime router**

Create `crates/server/src/router/api/uptime.rs`:

```rust
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::entity::server;
use crate::error::AppError;
use crate::response::ApiResponse;
use crate::service::uptime::{UptimeDailyEntry, UptimeService};
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct UptimeDailyQuery {
    /// Number of days (1-365, default 90)
    pub days: Option<u32>,
}

/// Get daily uptime data for a server.
#[utoipa::path(
    get,
    path = "/api/servers/{server_id}/uptime-daily",
    params(
        ("server_id" = String, Path, description = "Server ID"),
        UptimeDailyQuery,
    ),
    responses(
        (status = 200, description = "Daily uptime entries", body = Vec<UptimeDailyEntry>),
        (status = 404, description = "Server not found"),
    ),
    tag = "servers"
)]
pub async fn get_uptime_daily(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Query(query): Query<UptimeDailyQuery>,
) -> Result<Json<ApiResponse<Vec<UptimeDailyEntry>>>, AppError> {
    use sea_orm::EntityTrait;

    // Verify server exists
    server::Entity::find_by_id(&server_id)
        .one(&state.db)
        .await?
        .ok_or(AppError::NotFound("Server not found".into()))?;

    let days = query.days.unwrap_or(90);
    if days == 0 || days > 365 {
        return Err(AppError::BadRequest(
            "days must be between 1 and 365".into(),
        ));
    }

    let entries = UptimeService::get_daily_filled(&state.db, &server_id, days).await?;
    Ok(Json(ApiResponse::ok(entries)))
}

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route(
        "/servers/{server_id}/uptime-daily",
        get(get_uptime_daily),
    )
}
```

- [ ] **Step 2: Register route in mod.rs**

In `crates/server/src/router/api/mod.rs`, add `pub mod uptime;` to the module declarations.

In the `read_routes` (authenticated read) section, add: `.merge(uptime::read_router())`

- [ ] **Step 3: Register in OpenAPI**

In `crates/server/src/openapi.rs`:
- Add `uptime::get_uptime_daily` to the `paths(...)` list (in the servers section).
- Add `UptimeDailyEntry` to the `components(schemas(...))` list.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy -p serverbee-server -- -D warnings`
Expected: 0 warnings (may fail due to missing `apps/web/dist` — that's pre-existing, ignore).

- [ ] **Step 5: Add integration test**

In `crates/server/tests/integration.rs`, add these tests. They use the existing `start_test_server()` and `login_admin(client, base_url)` helpers. The `login_admin` helper returns a JSON value; extract the session cookie from the login response headers. Follow the pattern from `test_agent_register_connect_report` to register a server via agent WebSocket:

```rust
#[tokio::test]
async fn test_uptime_daily_requires_auth() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    let resp = client
        .get(format!("{}/api/servers/fake/uptime-daily", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_uptime_daily_server_not_found() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;
    let resp = client
        .get(format!("{}/api/servers/nonexistent/uptime-daily", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_uptime_daily_returns_data() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Register a server via agent registration (matches existing test pattern)
    let register_resp = client
        .post(format!("{}/api/agent/register", base_url))
        .header("Authorization", "Bearer test-key")
        .send()
        .await
        .unwrap();
    assert_eq!(register_resp.status(), 200);
    let register_body: serde_json::Value = register_resp.json().await.unwrap();
    let server_id = register_body["data"]["server_id"].as_str().unwrap();

    // days=0 should fail
    let resp = client
        .get(format!("{}/api/servers/{}/uptime-daily?days=0", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // days=366 should fail
    let resp = client
        .get(format!("{}/api/servers/{}/uptime-daily?days=366", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    // Default (no days param) should return 200 with 90 zero-filled entries
    let resp = client
        .get(format!("{}/api/servers/{}/uptime-daily", base_url, server_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let entries = body["data"].as_array().unwrap();
    assert_eq!(entries.len(), 90);
    // All entries should be zero-filled (no actual uptime data recorded)
    for entry in entries {
        assert_eq!(entry["online_minutes"], 0);
        assert_eq!(entry["total_minutes"], 0);
    }
}
```

- [ ] **Step 6: Run integration tests**

Run: `cargo test -p serverbee-server --test integration -- uptime`
Expected: All pass.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/router/api/uptime.rs crates/server/src/router/api/mod.rs crates/server/src/openapi.rs crates/server/tests/integration.rs
git commit -m "feat(api): add GET /api/servers/{id}/uptime-daily endpoint"
```

---

## Task 3: Backend — Migration + threshold fields in entity/service/router

**Files:**
- Create: `crates/server/src/migration/m20260321_000011_status_page_uptime_thresholds.rs`
- Modify: `crates/server/src/migration/mod.rs`
- Modify: `crates/server/src/entity/status_page.rs`
- Modify: `crates/server/src/service/status_page.rs`
- Modify: `crates/server/src/router/api/status_page.rs`
- Modify: `crates/server/src/openapi.rs`
- Modify: `crates/server/src/service/dashboard.rs`

- [ ] **Step 1: Create migration**

Create `crates/server/src/migration/m20260321_000011_status_page_uptime_thresholds.rs`:

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE status_page ADD COLUMN uptime_yellow_threshold REAL NOT NULL DEFAULT 100.0"
        ).await?;
        db.execute_unprepared(
            "ALTER TABLE status_page ADD COLUMN uptime_red_threshold REAL NOT NULL DEFAULT 95.0"
        ).await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

Register in `crates/server/src/migration/mod.rs`: add `mod m20260321_000011_status_page_uptime_thresholds;` and add `Box::new(m20260321_000011_status_page_uptime_thresholds::Migration)` to the migrations vec.

- [ ] **Step 2: Add fields to entity**

In `crates/server/src/entity/status_page.rs`, add to the `Model` struct:

```rust
pub uptime_yellow_threshold: f64,
pub uptime_red_threshold: f64,
```

- [ ] **Step 3: Add threshold fields to service inputs**

In `crates/server/src/service/status_page.rs`:

Add to `CreateStatusPageInput`:
```rust
pub uptime_yellow_threshold: Option<f64>,
pub uptime_red_threshold: Option<f64>,
```

Add to `UpdateStatusPageInput`:
```rust
pub uptime_yellow_threshold: Option<f64>,
pub uptime_red_threshold: Option<f64>,
```

Update the `create` method to use `input.uptime_yellow_threshold.unwrap_or(100.0)` and `input.uptime_red_threshold.unwrap_or(95.0)`.

Update the `update` method to set these fields when provided.

- [ ] **Step 4: Update ServerStatusInfo and status page handler**

In `crates/server/src/router/api/status_page.rs`:

Update `ServerStatusInfo`:
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
    pub uptime_percentage: Option<f64>,
    pub in_maintenance: bool,
    pub uptime_daily: Vec<UptimeDailyEntry>,
}
```

Add `use crate::service::uptime::{UptimeDailyEntry, UptimeService};` to imports.

Update `StatusPageInfo` to include thresholds:
```rust
pub uptime_yellow_threshold: f64,
pub uptime_red_threshold: f64,
```

In the `get_public_status_page` handler, replace the `UptimeService::get_availability` call with:
```rust
let daily = UptimeService::get_daily_filled(&state.db, &s.id, 90).await?;
let total_min: i64 = daily.iter().map(|d| d.total_minutes as i64).sum();
let online_min: i64 = daily.iter().map(|d| d.online_minutes as i64).sum();
let uptime_percentage = if total_min > 0 {
    Some((online_min as f64 / total_min as f64) * 100.0)
} else {
    None
};
```

And set `uptime_daily: daily` in the `ServerStatusInfo` construction.

Also update the admin list/get response to include `uptime_yellow_threshold` and `uptime_red_threshold`.

- [ ] **Step 5: Add "uptime-timeline" to VALID_WIDGET_TYPES**

In `crates/server/src/service/dashboard.rs`, add `"uptime-timeline"` to the `VALID_WIDGET_TYPES` array.

- [ ] **Step 6: Update OpenAPI registrations**

In `crates/server/src/openapi.rs`, ensure all modified DTOs are registered in the schemas list.

- [ ] **Step 7: Remove dead code**

In `crates/server/src/service/uptime.rs`, delete the `get_availability` method (now unused — its logic is replaced by computing from `get_daily_filled` results). Also delete the original `get_daily` method (now replaced by `get_daily_filled`).

- [ ] **Step 8: Run tests**

Run: `cargo test -p serverbee-server`
Expected: All existing + new tests pass. If any test references `get_availability`, update it to use `get_daily_filled`.

- [ ] **Step 9: Commit**

```bash
git add crates/server/src/migration/ crates/server/src/entity/status_page.rs crates/server/src/service/status_page.rs crates/server/src/service/dashboard.rs crates/server/src/service/uptime.rs crates/server/src/router/api/status_page.rs crates/server/src/openapi.rs
git commit -m "feat(uptime): add threshold migration, serde renames, uptime_daily in status page API"
```

---

## Task 4: Frontend — Shared utilities + API schema + hook

**Files:**
- Modify: `apps/web/src/lib/widget-helpers.ts`
- Modify: `apps/web/src/lib/api-schema.ts`
- Modify: `apps/web/src/lib/widget-types.ts`
- Modify: `apps/web/src/hooks/use-api.ts`

- [ ] **Step 1: Add UptimeDailyEntry type to api-schema.ts**

In `apps/web/src/lib/api-schema.ts`, add:

```typescript
export interface UptimeDailyEntry {
  date: string
  online_minutes: number
  total_minutes: number
  downtime_incidents: number
}
```

Update `PublicStatusPageData.servers` inline shape — add:
```typescript
uptime_daily: UptimeDailyEntry[]
```

Update `PublicStatusPageData.page` inline shape — add:
```typescript
uptime_yellow_threshold: number
uptime_red_threshold: number
```

Update `StatusPageItem` interface — add:
```typescript
uptime_yellow_threshold: number
uptime_red_threshold: number
```

- [ ] **Step 2: Add uptime utilities to widget-helpers.ts**

In `apps/web/src/lib/widget-helpers.ts`, add:

```typescript
import type { UptimeDailyEntry } from '@/lib/api-schema'

export function computeUptimeColor(
  onlineMinutes: number,
  totalMinutes: number,
  yellowThreshold = 100,
  redThreshold = 95
): 'green' | 'yellow' | 'red' | 'gray' {
  if (totalMinutes === 0) return 'gray'
  const pct = (onlineMinutes / totalMinutes) * 100
  if (pct >= yellowThreshold) return 'green'
  if (pct >= redThreshold) return 'yellow'
  return 'red'
}

export function computeAggregateUptime(days: UptimeDailyEntry[]): number | null {
  const totalMinutes = days.reduce((sum, d) => sum + d.total_minutes, 0)
  if (totalMinutes === 0) return null
  return (days.reduce((sum, d) => sum + d.online_minutes, 0) / totalMinutes) * 100
}

export function formatUptimeTooltip(entry: UptimeDailyEntry): {
  date: string
  percentage: string
  duration: string
  incidents: string
} {
  const date = new Date(entry.date).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })
  if (entry.total_minutes === 0) {
    return { date, percentage: 'No data', duration: '', incidents: '' }
  }
  const pct = ((entry.online_minutes / entry.total_minutes) * 100).toFixed(1)
  const hours = Math.floor(entry.online_minutes / 60)
  const mins = entry.online_minutes % 60
  const duration = `${hours}h ${mins}m online`
  const downtime = entry.total_minutes - entry.online_minutes
  const incidents = entry.downtime_incidents > 0
    ? `${entry.downtime_incidents} incident${entry.downtime_incidents > 1 ? 's' : ''}, ${downtime}m downtime`
    : 'No incidents'
  return { date, percentage: `${pct}% uptime`, duration, incidents }
}
```

- [ ] **Step 3: Add UptimeTimelineConfig to widget-types.ts**

In `apps/web/src/lib/widget-types.ts`:

Add to the `WIDGET_TYPES` array:
```typescript
{ id: 'uptime-timeline', label: 'Uptime Timeline', category: 'Status', defaultW: 8, defaultH: 3, minW: 4, minH: 2 }
```

Add the config interface:
```typescript
export interface UptimeTimelineConfig {
  server_ids: string[]
  days?: number
}
```

Add `UptimeTimelineConfig` to the `WidgetConfig` union type.

- [ ] **Step 4: Add useUptimeDaily hook**

In `apps/web/src/hooks/use-api.ts`, add:

```typescript
export function useUptimeDaily(serverId: string, days = 90) {
  return useQuery<UptimeDailyEntry[]>({
    queryKey: ['servers', serverId, 'uptime-daily', days],
    queryFn: () => api.get<UptimeDailyEntry[]>(`/api/servers/${serverId}/uptime-daily?days=${days}`),
    enabled: !!serverId,
    staleTime: 300_000
  })
}
```

Add `import type { UptimeDailyEntry } from '@/lib/api-schema'` to imports.

- [ ] **Step 5: Run typecheck + lint**

Run: `bun run typecheck && bun x ultracite check`
Expected: Pass (no errors).

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/lib/widget-helpers.ts apps/web/src/lib/api-schema.ts apps/web/src/lib/widget-types.ts apps/web/src/hooks/use-api.ts
git commit -m "feat(web): add uptime types, utilities, and useUptimeDaily hook"
```

---

## Task 5: Frontend — UptimeTimeline component + tests

**Files:**
- Create: `apps/web/src/components/uptime/uptime-timeline.tsx`
- Create: `apps/web/src/components/uptime/uptime-timeline.test.tsx`

- [ ] **Step 1: Write tests first**

Create `apps/web/src/components/uptime/uptime-timeline.test.tsx`:

```typescript
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import type { UptimeDailyEntry } from '@/lib/api-schema'
import { UptimeTimeline } from './uptime-timeline'

function makeEntry(overrides: Partial<UptimeDailyEntry> = {}): UptimeDailyEntry {
  return { date: '2026-03-15', online_minutes: 1440, total_minutes: 1440, downtime_incidents: 0, ...overrides }
}

function makeDays(count: number, overrides: Partial<UptimeDailyEntry> = {}): UptimeDailyEntry[] {
  return Array.from({ length: count }, (_, i) => makeEntry({ date: `2026-01-${String(i + 1).padStart(2, '0')}`, ...overrides }))
}

describe('UptimeTimeline', () => {
  it('renders correct number of segments', () => {
    const { container } = render(<UptimeTimeline days={makeDays(90)} rangeDays={90} />)
    const segments = container.querySelectorAll('[data-segment]')
    expect(segments).toHaveLength(90)
  })

  it('renders green for 100% uptime', () => {
    const { container } = render(<UptimeTimeline days={makeDays(3)} rangeDays={3} />)
    const segments = container.querySelectorAll('[data-segment]')
    for (const seg of segments) {
      expect(seg.className).toContain('bg-emerald-500')
    }
  })

  it('renders yellow for degraded uptime', () => {
    const days = makeDays(3, { online_minutes: 1400, total_minutes: 1440 }) // ~97.2%
    const { container } = render(<UptimeTimeline days={days} rangeDays={3} />)
    const segments = container.querySelectorAll('[data-segment]')
    for (const seg of segments) {
      expect(seg.className).toContain('bg-amber-500')
    }
  })

  it('renders red for low uptime', () => {
    const days = makeDays(3, { online_minutes: 1300, total_minutes: 1440 }) // ~90.3%
    const { container } = render(<UptimeTimeline days={days} rangeDays={3} />)
    const segments = container.querySelectorAll('[data-segment]')
    for (const seg of segments) {
      expect(seg.className).toContain('bg-red-500')
    }
  })

  it('renders gray for no-data days', () => {
    const days = makeDays(3, { online_minutes: 0, total_minutes: 0 })
    const { container } = render(<UptimeTimeline days={days} rangeDays={3} />)
    const segments = container.querySelectorAll('[data-segment]')
    for (const seg of segments) {
      expect(seg.className).toContain('bg-muted')
    }
  })

  it('respects custom thresholds', () => {
    // 99% uptime with yellowThreshold=99.5 should be yellow
    const days = makeDays(3, { online_minutes: 1425, total_minutes: 1440 }) // 98.96%
    const { container } = render(<UptimeTimeline days={days} rangeDays={3} yellowThreshold={99.5} redThreshold={95} />)
    const segments = container.querySelectorAll('[data-segment]')
    for (const seg of segments) {
      expect(seg.className).toContain('bg-amber-500')
    }
  })

  it('shows labels when showLabels is true', () => {
    render(<UptimeTimeline days={makeDays(90)} rangeDays={90} showLabels />)
    expect(screen.getByText(/ago/)).toBeTruthy()
    expect(screen.getByText(/Today/)).toBeTruthy()
  })

  it('shows legend when showLegend is true', () => {
    render(<UptimeTimeline days={makeDays(3)} rangeDays={3} showLegend />)
    expect(screen.getByText('100%')).toBeTruthy()
  })
})

// Test shared utilities
import { computeAggregateUptime } from '@/lib/widget-helpers'

describe('computeAggregateUptime', () => {
  it('returns percentage for normal data', () => {
    const days = [
      makeEntry({ online_minutes: 1440, total_minutes: 1440 }),
      makeEntry({ online_minutes: 1400, total_minutes: 1440 }),
    ]
    const result = computeAggregateUptime(days)
    expect(result).toBeCloseTo(98.6, 1)
  })

  it('returns null when all total_minutes are zero', () => {
    const days = [
      makeEntry({ online_minutes: 0, total_minutes: 0 }),
      makeEntry({ online_minutes: 0, total_minutes: 0 }),
    ]
    expect(computeAggregateUptime(days)).toBeNull()
  })
})
```

- [ ] **Step 2: Run tests to see them fail**

Run: `bun run vitest run src/components/uptime/uptime-timeline.test.tsx`
Expected: FAIL (component doesn't exist yet).

- [ ] **Step 3: Implement UptimeTimeline component**

Create `apps/web/src/components/uptime/uptime-timeline.tsx`:

```typescript
import { useMemo } from 'react'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import type { UptimeDailyEntry } from '@/lib/api-schema'
import { computeUptimeColor, formatUptimeTooltip } from '@/lib/widget-helpers'

interface UptimeTimelineProps {
  days: UptimeDailyEntry[]
  rangeDays: number
  yellowThreshold?: number
  redThreshold?: number
  showLabels?: boolean
  showLegend?: boolean
  height?: number
}

const COLOR_CLASSES: Record<string, string> = {
  green: 'bg-emerald-500',
  yellow: 'bg-amber-500',
  red: 'bg-red-500',
  gray: 'bg-muted'
}

export function UptimeTimeline({
  days,
  rangeDays,
  yellowThreshold = 100,
  redThreshold = 95,
  showLabels = false,
  showLegend = false,
  height = 28
}: UptimeTimelineProps) {
  const segments = useMemo(
    () =>
      days.slice(0, rangeDays).map((entry) => ({
        entry,
        color: computeUptimeColor(entry.online_minutes, entry.total_minutes, yellowThreshold, redThreshold)
      })),
    [days, rangeDays, yellowThreshold, redThreshold]
  )

  return (
    <div>
      {showLabels && (
        <div className="mb-1 flex justify-between text-muted-foreground text-xs">
          <span>{rangeDays} days ago</span>
          <span>Today</span>
        </div>
      )}
      <TooltipProvider delayDuration={100}>
        <div className="flex items-stretch gap-[1.5px]" style={{ height }}>
          {segments.map(({ entry, color }) => {
            const tip = formatUptimeTooltip(entry)
            return (
              <Tooltip key={entry.date}>
                <TooltipTrigger asChild>
                  <div
                    className={`flex-1 rounded-[2px] ${COLOR_CLASSES[color]}`}
                    data-segment
                  />
                </TooltipTrigger>
                <TooltipContent>
                  <p className="font-medium">{tip.date}</p>
                  <p>{tip.percentage}</p>
                  {tip.duration && <p>{tip.duration}</p>}
                  {tip.incidents && <p>{tip.incidents}</p>}
                </TooltipContent>
              </Tooltip>
            )
          })}
        </div>
      </TooltipProvider>
      {showLegend && (
        <div className="mt-2 flex gap-3 text-muted-foreground text-xs">
          <span className="flex items-center gap-1">
            <span className="inline-block size-2 rounded-[2px] bg-emerald-500" />
            100%
          </span>
          <span className="flex items-center gap-1">
            <span className="inline-block size-2 rounded-[2px] bg-amber-500" />
            &lt;{yellowThreshold}%
          </span>
          <span className="flex items-center gap-1">
            <span className="inline-block size-2 rounded-[2px] bg-red-500" />
            &lt;{redThreshold}%
          </span>
          <span className="flex items-center gap-1">
            <span className="inline-block size-2 rounded-[2px] bg-muted" />
            No data
          </span>
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 4: Run tests**

Run: `bun run vitest run src/components/uptime/uptime-timeline.test.tsx`
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/uptime/
git commit -m "feat(web): add UptimeTimeline component with tests"
```

---

## Task 6: Frontend — Replace UptimeBar on public status page

**Files:**
- Modify: `apps/web/src/routes/status.$slug.tsx`

- [ ] **Step 1: Replace UptimeBar with UptimeTimeline**

In `apps/web/src/routes/status.$slug.tsx`:

1. Add imports:
```typescript
import { UptimeTimeline } from '@/components/uptime/uptime-timeline'
import { computeAggregateUptime } from '@/lib/widget-helpers'
```

2. Remove the `UptimeBar` function.

3. In `ServerRow`, replace the `<UptimeBar percent={server.uptime_percent} />` with:
```typescript
<div className="hidden flex-1 sm:block">
  <UptimeTimeline
    days={server.uptime_daily}
    rangeDays={90}
    yellowThreshold={page.uptime_yellow_threshold}
    redThreshold={page.uptime_red_threshold}
  />
</div>
```

4. Replace the percentage text rendering to use `computeAggregateUptime`:
```typescript
const pct = computeAggregateUptime(server.uptime_daily)
// Display: pct !== null ? `${pct.toFixed(2)}%` : '—'
```

Pass `page` props from the parent component so `ServerRow` has access to `uptime_yellow_threshold` and `uptime_red_threshold`.

- [ ] **Step 2: Run typecheck + lint**

Run: `bun run typecheck && bun x ultracite check`
Expected: Pass.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/status.\$slug.tsx
git commit -m "feat(web): replace UptimeBar with UptimeTimeline on public status page"
```

---

## Task 7: Frontend — Uptime card on server detail page

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`

- [ ] **Step 1: Add uptime card**

In `apps/web/src/routes/_authed/servers/$id.tsx`:

1. Add imports:
```typescript
import { UptimeTimeline } from '@/components/uptime/uptime-timeline'
import { useUptimeDaily } from '@/hooks/use-api'
import { computeAggregateUptime } from '@/lib/widget-helpers'
```

2. In `ServerDetailPage`, add the hook:
```typescript
const { data: uptimeDaily } = useUptimeDaily(id)
```

3. Add an "Uptime" card near the top of the overview section (before or after the metrics tabs):
```typescript
{uptimeDaily && (
  <div className="rounded-lg border bg-card p-4">
    <div className="mb-3 flex items-baseline gap-2">
      <h3 className="font-semibold text-sm">Uptime</h3>
      <span className="font-bold text-lg">
        {(() => {
          const pct = computeAggregateUptime(uptimeDaily)
          return pct !== null ? `${pct.toFixed(2)}%` : '—'
        })()}
      </span>
      <span className="text-muted-foreground text-xs">90 days</span>
    </div>
    <UptimeTimeline days={uptimeDaily} rangeDays={90} showLabels showLegend />
  </div>
)}
```

- [ ] **Step 2: Run typecheck**

Run: `bun run typecheck`
Expected: Pass.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/servers/\$id.tsx
git commit -m "feat(web): add uptime card with 90-day timeline to server detail page"
```

---

## Task 8: Frontend — Threshold config in status page admin

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/status-pages.tsx`

- [ ] **Step 1: Add threshold inputs to create/edit dialog**

In the `StatusPageFormDialog` in `apps/web/src/routes/_authed/settings/status-pages.tsx`:

1. Add state for threshold fields (with defaults for create, reading from existing data for edit):
```typescript
const [yellowThreshold, setYellowThreshold] = useState(editItem?.uptime_yellow_threshold ?? 100)
const [redThreshold, setRedThreshold] = useState(editItem?.uptime_red_threshold ?? 95)
```

2. Add input fields in the form (after existing fields):
```typescript
<div className="space-y-1.5">
  <Label>Uptime Yellow Threshold (%)</Label>
  <Input type="number" min={0} max={100} step={0.1} value={yellowThreshold} onChange={(e) => setYellowThreshold(Number(e.target.value))} />
  <p className="text-muted-foreground text-xs">Days below this % show as yellow (default: 100)</p>
</div>
<div className="space-y-1.5">
  <Label>Uptime Red Threshold (%)</Label>
  <Input type="number" min={0} max={100} step={0.1} value={redThreshold} onChange={(e) => setRedThreshold(Number(e.target.value))} />
  <p className="text-muted-foreground text-xs">Days below this % show as red (default: 95)</p>
</div>
```

3. Include thresholds in the submit payload:
```typescript
uptime_yellow_threshold: yellowThreshold,
uptime_red_threshold: redThreshold,
```

- [ ] **Step 2: Run typecheck + lint**

Run: `bun run typecheck && bun x ultracite check`
Expected: Pass.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/settings/status-pages.tsx
git commit -m "feat(web): add uptime threshold config to status page admin dialog"
```

---

## Task 9: Frontend — Dashboard widget (uptime-timeline)

**Files:**
- Create: `apps/web/src/components/dashboard/widgets/uptime-timeline-widget.tsx`
- Modify: `apps/web/src/components/dashboard/widget-renderer.tsx`
- Modify: `apps/web/src/components/dashboard/widget-config-dialog.tsx`
- Modify: `apps/web/src/components/dashboard/widget-picker.tsx`
- Modify: `apps/web/src/components/dashboard/widget-renderer.test.tsx`
- Modify: `apps/web/src/components/dashboard/widget-config-dialog.test.tsx`

- [ ] **Step 1: Create UptimeTimelineWidget**

Create `apps/web/src/components/dashboard/widgets/uptime-timeline-widget.tsx`:

```typescript
import { useQueries } from '@tanstack/react-query'
import { useMemo } from 'react'
import { UptimeTimeline } from '@/components/uptime/uptime-timeline'
import type { ServerMetrics } from '@/hooks/use-servers-ws'
import { api } from '@/lib/api-client'
import type { UptimeDailyEntry } from '@/lib/api-schema'
import { computeAggregateUptime } from '@/lib/widget-helpers'
import type { UptimeTimelineConfig } from '@/lib/widget-types'

interface UptimeTimelineWidgetProps {
  config: UptimeTimelineConfig
  servers: ServerMetrics[]
}

export function UptimeTimelineWidget({ config, servers }: UptimeTimelineWidgetProps) {
  const { server_ids } = config
  const days = config.days ?? 90

  const queries = useQueries({
    queries: server_ids.map((sid) => ({
      queryKey: ['servers', sid, 'uptime-daily', days],
      queryFn: () => api.get<UptimeDailyEntry[]>(`/api/servers/${sid}/uptime-daily?days=${days}`),
      enabled: sid.length > 0,
      staleTime: 300_000,
      refetchInterval: 300_000
    }))
  })

  const serverNameMap = useMemo(() => {
    const map = new Map<string, string>()
    for (const s of servers) map.set(s.id, s.name)
    return map
  }, [servers])

  if (queries.some((q) => q.isLoading)) {
    return (
      <div className="flex h-full items-center justify-center rounded-lg border bg-card text-muted-foreground text-sm">
        Loading...
      </div>
    )
  }

  return (
    <div className="flex h-full flex-col gap-3 overflow-auto rounded-lg border bg-card p-4">
      {server_ids.map((sid, i) => {
        const data = queries[i]?.data ?? []
        const name = serverNameMap.get(sid) ?? sid.slice(0, 8)
        const pct = computeAggregateUptime(data)
        const pctText = pct !== null ? `${pct.toFixed(2)}%` : '—'

        return (
          <div key={sid}>
            <div className="mb-1 flex items-baseline justify-between">
              <span className="truncate font-medium text-sm">{name}</span>
              <span className="ml-2 shrink-0 font-semibold text-sm tabular-nums">{pctText}</span>
            </div>
            <UptimeTimeline days={data} rangeDays={days} height={server_ids.length > 1 ? 20 : 28} />
          </div>
        )
      })}
      {server_ids.length === 0 && (
        <div className="flex flex-1 items-center justify-center text-muted-foreground text-xs">
          No servers selected
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Register in widget-renderer.tsx**

Add import and case to the switch in `WidgetContent`:
```typescript
import { UptimeTimelineWidget } from './widgets/uptime-timeline-widget'
// ...
case 'uptime-timeline':
  return <UptimeTimelineWidget config={config as unknown as UptimeTimelineConfig} servers={servers} />
```

Add `UptimeTimelineConfig` to the import from `@/lib/widget-types`.

- [ ] **Step 3: Add config form in widget-config-dialog.tsx**

Add a form component:
```typescript
function UptimeTimelineForm({
  config,
  servers,
  onChange
}: {
  config: Partial<UptimeTimelineConfig>
  onChange: (c: Partial<UptimeTimelineConfig>) => void
  servers: ServerMetrics[]
}) {
  return (
    <>
      <ServerMultiSelect
        label="Servers"
        onChange={(ids) => onChange({ ...config, server_ids: ids })}
        selected={config.server_ids ?? []}
        servers={servers}
      />
      <div className="space-y-1.5">
        <Label>Time Range</Label>
        <Select onValueChange={(v) => v !== null && onChange({ ...config, days: Number(v) })} value={String(config.days ?? 90)}>
          <SelectTrigger className="w-full">
            <SelectValue placeholder="Select range" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="30">30 days</SelectItem>
            <SelectItem value="60">60 days</SelectItem>
            <SelectItem value="90">90 days</SelectItem>
          </SelectContent>
        </Select>
      </div>
    </>
  )
}
```

Add the branch in the dialog body:
```typescript
{widgetType === 'uptime-timeline' && (
  <UptimeTimelineForm config={config as Partial<UptimeTimelineConfig>} onChange={setConfig} servers={servers} />
)}
```

Add `UptimeTimelineConfig` to the imports from `@/lib/widget-types`.

- [ ] **Step 4: Add to widget-picker.tsx**

Add to `WIDGET_ICONS`:
```typescript
'uptime-timeline': Activity,  // or BarChart3 from lucide-react
```

Add to `WIDGET_DESCRIPTIONS`:
```typescript
'uptime-timeline': '90-day uptime history bar',
```

- [ ] **Step 5: Add test cases**

In `widget-renderer.test.tsx`, add a test for the new type:
```typescript
it('renders UptimeTimelineWidget for uptime-timeline type', () => {
  const widget = { ...baseWidget, widget_type: 'uptime-timeline', config_json: '{"server_ids":["s1"]}' }
  render(<WidgetRenderer widget={widget} servers={[]} />)
  // Should not show "Unknown widget type"
  expect(screen.queryByText(/Unknown widget type/)).toBeNull()
})
```

In `widget-config-dialog.test.tsx`, add a test:
```typescript
it('renders server multi-select for uptime-timeline widget', () => {
  render(<WidgetConfigDialog open onOpenChange={noop} onSubmit={noop} widgetType="uptime-timeline" servers={mockServers} />)
  expect(screen.getByText('Servers')).toBeTruthy()
})
```

- [ ] **Step 6: Run all frontend tests**

Run: `bun run test`
Expected: All pass.

- [ ] **Step 7: Run typecheck + lint**

Run: `bun run typecheck && bun x ultracite check`
Expected: Pass.

- [ ] **Step 8: Commit**

```bash
git add apps/web/src/components/dashboard/ apps/web/src/components/uptime/
git commit -m "feat(web): add uptime-timeline dashboard widget with config dialog"
```

---

## Task 10: Final verification

- [ ] **Step 1: Run full Rust test suite**

Run: `cargo test -p serverbee-common -p serverbee-agent && cargo test -p serverbee-server`
Expected: All tests pass.

- [ ] **Step 2: Run full frontend test suite**

Run: `bun run test && bun run typecheck && bun x ultracite check`
Expected: All 172+ tests pass, no type errors, no lint errors.

- [ ] **Step 3: Update PROGRESS.md**

Add P18 entry to `docs/superpowers/plans/PROGRESS.md`:

```markdown
| P18 | Uptime 90-Day Timeline | **已完成** | N commits |
```

- [ ] **Step 4: Update TESTING.md**

Add new test counts and describe new test coverage for uptime timeline feature.

- [ ] **Step 5: Final commit**

```bash
git add docs/
git commit -m "docs: update progress and testing docs for uptime 90-day timeline"
```
