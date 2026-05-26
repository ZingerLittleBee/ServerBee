# Public Status Page Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the multi-slug public status surface with a single anonymous `/status` site that exposes server list, server detail, network quality, and IP quality — all gated by per-feature admin toggles, all redacted of IP-level identifiers at the API boundary.

**Architecture:** A new `/api/status/*` router serves redacted DTOs to anonymous and authenticated callers alike (unconditional redaction). A migration collapses `status_page` to a singleton row and replaces the many-to-many `status_page_ids_json` on `incident` / `maintenance` with a single `is_public` boolean. Frontend extracts presentation-only content components from the existing authenticated pages and remounts them under a new public layout with `variant="public"`.

**Tech Stack:** Rust (Axum 0.8, sea-orm, SQLite, utoipa), React 19 + TypeScript, TanStack Router/Query, shadcn/ui, Vitest.

**Spec:** `docs/superpowers/specs/2026-05-26-status-page-refactor-design.md` — **read end-to-end before starting**. The plan refers back to it for field lists and rationale.

**Conventions:**
- Conventional Commits (`type(scope): imperative summary`).
- Migrations implement `up()` only; `down()` is `Ok(())`.
- All REST endpoints carry `#[utoipa::path]`, all DTOs `#[derive(ToSchema)]`.
- Frontend: `bun x ultracite fix` before commit; `bun run typecheck` clean.
- Backend: `cargo clippy --workspace -- -D warnings` clean.
- No Claude attribution anywhere (commits / PRs / comments / docs).
- Don't push during execution. Commit frequently; user pushes manually.

---

## File Structure

**Backend — created:**
- `crates/server/src/migration/m20260526_000035_simplify_status_page.rs`
- `crates/server/src/router/api/status.rs` (new public-status router)
- `crates/server/src/service/public_status.rs` (new service module — queries, DTO mapping, scope/gating)
- `crates/server/tests/public_status_anonymous.rs`
- `crates/server/tests/public_status_redaction.rs`
- `crates/server/tests/public_status_redaction_authenticated.rs`
- `crates/server/tests/public_status_ip_quality_redaction.rs`
- `crates/server/tests/public_status_scope.rs`
- `crates/server/tests/public_status_gating.rs`

**Backend — modified:**
- `crates/server/src/migration/mod.rs` (register migration)
- `crates/server/src/entity/status_page.rs` (drop legacy cols, add toggles + layout)
- `crates/server/src/entity/incident.rs` (drop `status_page_ids_json`, add `is_public`)
- `crates/server/src/entity/maintenance.rs` (drop `status_page_ids_json`, add `is_public`)
- `crates/server/src/router/api/mod.rs` (wire new public router; remove old `status_page::public_router`)
- `crates/server/src/router/api/status_page.rs` (drop `public_router`; simplify admin to singleton `GET` + `PUT`; remove `create_*` / `delete_*`)
- `crates/server/src/service/status_page.rs` (simplify — singleton patterns only; drop `slug`, `theme_ref`, `custom_css`, `show_values`)
- `crates/server/src/service/incident.rs` and `service/maintenance.rs` (drop `status_page_ids_json` mapping; add `is_public`)

**Backend — deleted:**
- nothing yet (`status_page.rs::get_public_status_page` is moved logic into the new public service then deleted as a step)

**Frontend — created:**
- `apps/web/src/routes/status.tsx` (rewritten as a real layout)
- `apps/web/src/routes/status.index.tsx` (rewritten — list/grid)
- `apps/web/src/routes/status.server.$serverId.tsx`
- `apps/web/src/routes/status.network.tsx`
- `apps/web/src/routes/status.network.index.tsx`
- `apps/web/src/routes/status.network.$serverId.tsx`
- `apps/web/src/routes/status.ip-quality.tsx`
- `apps/web/src/components/status/status-header.tsx`
- `apps/web/src/components/status/layout-toggle.tsx`
- `apps/web/src/components/status/server-detail-content.tsx` (extracted from `_authed/servers/$id-page.tsx`)
- `apps/web/src/components/status/network-overview-content.tsx`
- `apps/web/src/components/status/network-detail-content.tsx`
- `apps/web/src/components/status/ip-quality-content.tsx`
- `apps/web/src/hooks/use-public-status.ts`

**Frontend — modified:**
- `apps/web/src/lib/api-schema.ts` (replace `PublicStatusPageData` + `StatusPageItem`; add public DTOs)
- `apps/web/src/routes/_authed/servers/$id-page.tsx` (mount extracted content)
- `apps/web/src/routes/_authed/network/index.tsx` (mount extracted content)
- `apps/web/src/routes/_authed/network/$serverId.tsx` (mount extracted content)
- `apps/web/src/routes/_authed/ip-quality.tsx` (mount extracted content)
- `apps/web/src/components/ip-quality/ip-quality-card.tsx` (`variant?: 'admin' | 'public'`)
- `apps/web/src/routes/_authed/settings/status-pages.tsx` (rewritten — singleton form)
- `apps/web/src/locales/{en,zh}/status.json` (new keys for header, layout toggle, sub-page nav)

**Frontend — deleted:**
- `apps/web/src/routes/status.$slug.tsx`
- `apps/web/src/routes/status-slug.test.tsx`
- `apps/web/src/routes/_authed/settings/status-pages.test.tsx` (rewritten under the same name)

**Docs — modified:**
- `docs/superpowers/plans/PROGRESS.md`

---

# Phase 1 — Database migration

### Task 1: Create the singleton migration

**Files:**
- Create: `crates/server/src/migration/m20260526_000035_simplify_status_page.rs`
- Modify: `crates/server/src/migration/mod.rs`

The migration follows the 8-step recipe in spec §"Database migration". Read that section before writing code.

- [ ] **Step 1: Read prior migrations for patterns**

Read `crates/server/src/migration/m20260522_000030_status_page_show_ip_quality.rs` and `m20260320_000009_status_page.rs` end-to-end. Take note of how SeaORM/SQLite ALTER TABLE is invoked, how the slug unique index was created, how triggers are spelled out via `db.execute_unprepared(...)`.

- [ ] **Step 2: Write the migration file**

Path: `crates/server/src/migration/m20260526_000035_simplify_status_page.rs`.

The migration must:

1. Resolve the surviving `status_page` row id (`SELECT id FROM status_page WHERE enabled = 1 ORDER BY updated_at DESC LIMIT 1`; if none, the first row by `updated_at` desc; if table empty, insert a default row with a generated UUID, `enabled = false`, sensible defaults, and capture that id).
2. `DROP INDEX IF EXISTS idx_status_page_slug_unique`.
3. Drop these triggers from `m20260430_000021_custom_theme_ref_integrity.rs`:
   - `trg_custom_theme_status_page_insert_ref_exists`
   - `trg_custom_theme_status_page_update_ref_exists`
   - any other trigger that references `status_page.theme_ref` — grep for triggers naming `theme_ref` on `status_page` and drop them all. Use `db.execute_unprepared("DROP TRIGGER IF EXISTS …;")`.
4. `ALTER TABLE status_page ADD COLUMN default_layout TEXT NOT NULL DEFAULT 'grid'`.
5. Same pattern for `show_server_detail`, `show_network`, `show_incidents`, `show_maintenance` BOOLEAN NOT NULL DEFAULT TRUE; `show_network` defaults FALSE.
6. `ALTER TABLE incident ADD COLUMN is_public BOOLEAN NOT NULL DEFAULT FALSE` — same for `maintenance`.
7. Backfill `incident.is_public = 1` for rows where `status_page_ids_json IS NULL OR status_page_ids_json = '[]' OR status_page_ids_json LIKE '%"' || :surviving_id || '"%'`. Same for `maintenance`. Use `db.execute_unprepared` with the surviving id substituted into the SQL string (it is a generated UUID so quoting is safe, but still wrap in single-quote string literal).
8. `ALTER TABLE incident DROP COLUMN status_page_ids_json`. Same for `maintenance`. (SQLite supports drop column since 3.35; confirm the project's SQLite is new enough by checking `Cargo.toml` for the `libsqlite3-sys` version pinned. If drop column is unsupported, fall back to "create new table, copy rows, drop, rename" — but only after confirming.)
9. `ALTER TABLE status_page DROP COLUMN slug; DROP COLUMN theme_ref; DROP COLUMN custom_css; DROP COLUMN show_values`. Separate statements.
10. `DELETE FROM status_page WHERE id != :surviving_id`.

Wrap all `unprepared` SQL in a single `db.transaction(...)` if possible to keep the migration atomic, otherwise rely on SeaORM's per-statement execution (consistent with sibling migrations).

- [ ] **Step 3: Register migration**

Edit `crates/server/src/migration/mod.rs`: add `mod m20260526_000035_simplify_status_page;` and append `Box::new(m20260526_000035_simplify_status_page::Migration)` to the migrator list.

- [ ] **Step 4: Compile**

```
cargo build -p serverbee-server
```

Expected: clean build.

- [ ] **Step 5: Smoke-run the migration**

Apply migrations on a copy of a populated SQLite database to ensure SQL is well-formed:

```
cp ~/.serverbee/serverbee.db /tmp/migration_test.db
SERVERBEE_DB_PATH=/tmp/migration_test.db cargo run -p serverbee-server -- --check 2>&1 | head -40
```

(If the binary has no `--check`, start it briefly and Ctrl-C after "Migrations applied".)

Then inspect the resulting schema:

```
sqlite3 /tmp/migration_test.db ".schema status_page"
sqlite3 /tmp/migration_test.db ".schema incident"
sqlite3 /tmp/migration_test.db ".schema maintenance"
sqlite3 /tmp/migration_test.db "SELECT COUNT(*) FROM status_page;"
sqlite3 /tmp/migration_test.db "SELECT id, is_public FROM incident;"
```

Expected: `status_page` has the new columns, no slug/theme_ref/custom_css/show_values, exactly one row. `incident`/`maintenance` have `is_public`, no `status_page_ids_json`.

If you don't have a populated DB lying around, skip the row-count assertion but still verify schema. Do not skip the schema verification.

- [ ] **Step 6: Commit**

```
git add crates/server/src/migration/
git commit -m "feat(server): migrate status_page to singleton + is_public columns"
```

---

### Task 2: Update entities

**Files:**
- Modify: `crates/server/src/entity/status_page.rs`
- Modify: `crates/server/src/entity/incident.rs`
- Modify: `crates/server/src/entity/maintenance.rs`

- [ ] **Step 1: Update `status_page` entity**

Drop these fields from the `Model` struct: `slug`, `show_values`, `custom_css`, `theme_ref`. Add: `default_layout: String`, `show_server_detail: bool`, `show_network: bool`, `show_incidents: bool`, `show_maintenance: bool`. Keep `show_ip_quality`. Keep all existing `#[schema(...)]` and `#[sea_orm(...)]` attributes consistent with the table.

- [ ] **Step 2: Update `incident` entity**

Drop `status_page_ids_json: Option<String>`. Add `is_public: bool`.

- [ ] **Step 3: Update `maintenance` entity**

Drop `status_page_ids_json: Option<String>`. Add `is_public: bool`.

- [ ] **Step 4: Compile (will fail)**

```
cargo build -p serverbee-server 2>&1 | head -80
```

Expect compile errors throughout services and routers that reference the removed fields. That's normal — Phase 2/3 fixes them. Stop here; do not patch errors yet.

- [ ] **Step 5: Commit (with `--allow-empty-message-body` of failing intermediate state)**

```
git add crates/server/src/entity/
git commit -m "feat(server): drop legacy status_page columns from entities"
```

Note: this commit leaves the workspace temporarily uncompilable. That is acceptable in a feature branch; subsequent tasks restore compilation.

---

# Phase 2 — Backend public API

### Task 3: Public DTOs

**Files:**
- Create: `crates/server/src/service/public_status.rs`

This task establishes the DTO surface; queries are added in Task 4.

- [ ] **Step 1: Create the module with DTO types only**

Create `crates/server/src/service/public_status.rs`. Add `pub mod public_status;` to `crates/server/src/service/mod.rs`.

DTOs to define (each `#[derive(Debug, Clone, Serialize, ToSchema)]`):

```rust
use serde::Serialize;
use utoipa::ToSchema;

use crate::service::uptime::UptimeDailyEntry;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicStatusConfig {
    pub enabled: bool,
    pub title: String,
    pub description: Option<String>,
    pub default_layout: String, // "list" | "grid"
    pub show_server_detail: bool,
    pub show_network: bool,
    pub show_ip_quality: bool,
    pub show_incidents: bool,
    pub show_maintenance: bool,
    pub uptime_yellow_threshold: f64,
    pub uptime_red_threshold: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicMetricsSummary {
    pub cpu: f64,
    pub mem_used: u64,
    pub mem_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub net_in_speed: u64,
    pub net_out_speed: u64,
    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,
    pub uptime: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicServerSummary {
    pub id: String,
    pub name: String,
    pub group_name: Option<String>,
    pub region: Option<String>,
    pub country_code: Option<String>,
    pub online: bool,
    pub in_maintenance: bool,
    pub public_remark: Option<String>,
    pub os: Option<String>,
    pub metrics: Option<PublicMetricsSummary>,
    pub uptime_percent: Option<f64>,
    pub uptime_daily: Vec<UptimeDailyEntry>,
    // No ipv4/ipv6/hostname/interfaces/public_ip — by design absent.
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicServerDetail {
    #[serde(flatten)]
    pub summary: PublicServerSummary,
    pub cpu_name: Option<String>,
    pub cpu_cores: Option<u32>,
    pub cpu_arch: Option<String>,
    pub kernel_version: Option<String>,
    pub agent_version: Option<String>,
    pub mem_total: Option<u64>,
    pub disk_total: Option<u64>,
    pub process_count: Option<u32>,
    pub tcp_conn: Option<u32>,
    pub udp_conn: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicIpQualitySnapshot {
    // Retained fields per spec §"Defense-in-depth"
    pub country: Option<String>,
    pub ip_type: String,
    pub risk_score: Option<i32>,
    pub risk_level: String,
    pub checked_at: String,
    // Explicitly NOT included: ip, asn, as_org, region, city,
    // is_proxy, is_vpn, is_hosting, is_tor, is_abuser, is_mobile,
    // asn_abuser_score, abuse_email.
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicUnlockResult {
    pub service_id: String,
    pub status: String,
    pub region: Option<String>,    // unlock region, not egress region
    pub latency_ms: Option<i32>,
    pub checked_at: String,
    // Explicitly NOT included: `detail` (free-form, may leak).
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicIpQualityEntry {
    pub server_id: String,
    // Absent when the agent has not yet reported an ip-quality snapshot.
    pub ip_quality: Option<PublicIpQualitySnapshot>,
    pub unlock_results: Vec<PublicUnlockResult>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicIpQualityServiceMeta {
    pub id: String,
    pub key: String,
    pub name: String,
    pub category: String,
    pub popularity: i32,
    pub is_builtin: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicIpQualityOverview {
    pub entries: Vec<PublicIpQualityEntry>,
    pub services: Vec<PublicIpQualityServiceMeta>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicIncident {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub updates: Vec<PublicIncidentUpdate>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicIncidentUpdate {
    pub id: String,
    pub status: String,
    pub message: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PublicMaintenance {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub start_at: String,
    pub end_at: String,
}
```

Additional public DTOs for network overview / detail will be added in Task 4 when wiring up the network queries — they wrap existing `NetworkServerSummary` / per-server data without server IP fields. Network probe `target` IPs and traceroute hop IPs are **retained** (spec §"Defense-in-depth").

- [ ] **Step 2: Compile**

```
cargo build -p serverbee-server 2>&1 | head -40
```

Compilation should succeed for this file (errors from Task 2 still present elsewhere — that's fine).

- [ ] **Step 3: Commit**

```
git add crates/server/src/service/public_status.rs crates/server/src/service/mod.rs
git commit -m "feat(server): add PublicStatus DTOs"
```

---

### Task 4: Public-status service queries + scope guard

**Files:**
- Modify: `crates/server/src/service/public_status.rs`

The service module loads the singleton config, resolves the scoped server id set, and exposes one function per public endpoint, returning DTOs.

- [ ] **Step 1: Read references**

Read these files end-to-end before writing the service:

- Spec §"Public server scope", §"Defense-in-depth: redaction at the API boundary".
- `crates/server/src/router/api/status_page.rs:215-300` — the existing scope/redaction logic that will be replicated.
- `crates/server/src/router/api/status.rs` — existing default `GET /api/status` implementation (the route this PR replaces).
- `crates/server/src/service/ip_quality.rs` — see how `get_summaries` is wired.
- `crates/server/src/service/uptime.rs` — `get_daily_filled` signature.

- [ ] **Step 2: Implement `load_config`**

```rust
use sea_orm::{DatabaseConnection, EntityTrait, QueryOrder};
use crate::entity::status_page;

pub async fn load_config(db: &DatabaseConnection) -> Result<status_page::Model, AppError> {
    status_page::Entity::find()
        .order_by_asc(status_page::Column::CreatedAt)
        .one(db)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::NotFound("status_page singleton row missing".into()))
}
```

- [ ] **Step 3: Implement `to_public_config`**

Map `status_page::Model` to `PublicStatusConfig`. The config endpoint always returns this DTO regardless of `enabled`; the SPA uses the `enabled` field to short-circuit other queries.

- [ ] **Step 4: Implement `resolve_scope`**

```rust
pub struct PublicScope {
    pub config: status_page::Model,
    pub server_ids: Vec<String>, // intersected with: exists AND hidden = false
}

pub async fn resolve_scope(db: &DatabaseConnection) -> Result<PublicScope, AppError> {
    let config = load_config(db).await?;
    let selected: Vec<String> = serde_json::from_str(&config.server_ids_json)
        .map_err(|e| AppError::Internal(format!("invalid server_ids_json: {e}")))?;
    let live_ids: HashSet<String> = server::Entity::find()
        .filter(server::Column::Hidden.eq(false))
        .all(db)
        .await?
        .into_iter()
        .map(|s| s.id)
        .collect();
    let server_ids = selected.into_iter().filter(|id| live_ids.contains(id)).collect();
    Ok(PublicScope { config, server_ids })
}
```

`PublicScope` is shared by all subsequent functions so each endpoint pays the scope cost once.

- [ ] **Step 5: Implement per-endpoint queries**

Each function takes `&DatabaseConnection` and returns the corresponding DTO. Signatures:

```rust
pub async fn list_servers(db: &DatabaseConnection) -> Result<Vec<PublicServerSummary>, AppError>;
pub async fn get_server_detail(db: &DatabaseConnection, id: &str) -> Result<PublicServerDetail, AppError>;
pub async fn get_server_metrics(
    db: &DatabaseConnection,
    id: &str,
    range: MetricsRangeQuery,
) -> Result<Vec<MetricsPoint>, AppError>; // reuse the existing types from
                                          // crates/server/src/service/metrics.rs
                                          // (or wherever the auth'd metrics
                                          // endpoint defines them — grep for
                                          // MetricsRange first).
pub async fn get_server_uptime_daily(db: &DatabaseConnection, id: &str) -> Result<Vec<UptimeDailyEntry>, AppError>;
pub async fn network_overview(db: &DatabaseConnection) -> Result<PublicNetworkOverview, AppError>;
pub async fn network_server_detail(db: &DatabaseConnection, id: &str) -> Result<PublicNetworkServerDetail, AppError>;
pub async fn ip_quality_overview(db: &DatabaseConnection) -> Result<PublicIpQualityOverview, AppError>;
pub async fn list_incidents(db: &DatabaseConnection) -> Result<(Vec<PublicIncident>, Vec<PublicIncident>), AppError>;
pub async fn list_maintenances(db: &DatabaseConnection) -> Result<Vec<PublicMaintenance>, AppError>;
```

For network types: read `crates/server/src/router/api/network.rs` and
`crates/server/src/service/network.rs` to confirm the auth'd types. If the
auth'd `NetworkServerSummary` exposes server `ipv4` / `ipv6`, define
`PublicNetworkServerSummary` here that strips those, and have
`PublicNetworkOverview { servers: Vec<PublicNetworkServerSummary>, ... }`.
If the auth'd types are already IP-free (only `server_id` / `server_name`),
you can re-export them under `Public*` aliases — verify before assuming.

For each `*_server_*` query that takes an `id`, first call `resolve_scope` and check `scope.server_ids.contains(id)`. If not, return `AppError::NotFound`. The 404 is intentional — do **not** return 403 here, the API must be indistinguishable from "server does not exist" (spec §"Public server scope").

Network DTOs reuse the existing `NetworkServerSummary` etc. without `ipv4`/`ipv6`. If those existing types contain server IPs, define `PublicNetworkServerSummary` that omits them. **Verify** by reading `crates/server/src/lib/network-types.ts` and the matching Rust types.

IP-quality `list_incidents` / `list_maintenances` must filter by `is_public = true` AND respect the appropriate `show_*` toggles upstream (handler-level gate; see Task 5).

`network_overview` / `network_server_detail` use the existing `NetworkService` queries scoped to `scope.server_ids`.

`ip_quality_overview` uses `IpQualityService::get_summaries(&scope.server_ids)` then maps each result into `PublicIpQualityEntry` — dropping the snapshot's `ip`/`asn`/`as_org`/`region`/`city`/`is_*`/`asn_abuser_score`/`abuse_email` fields and each `unlock_result.detail`. The service catalog is loaded via the existing `IpQualityService` list and filtered to `enabled = true`, mapped to `PublicIpQualityServiceMeta`.

- [ ] **Step 6: Compile**

```
cargo build -p serverbee-server 2>&1 | head -80
```

If you reference types that haven't been refactored yet (e.g. old IP-quality DTOs that still expose `ip`), define small intermediate mapping helpers. Resolve every error before continuing.

- [ ] **Step 7: Commit**

```
git add crates/server/src/service/public_status.rs
git commit -m "feat(server): add public_status service queries with scope guard"
```

---

### Task 5: Public router + handlers

**Files:**
- Create: `crates/server/src/router/api/status.rs` **(this file already exists — rename it or merge)**. Inspect first: `git status crates/server/src/router/api/status.rs`. The existing file holds the authenticated `GET /api/status` (server list). Since the spec routes the **public** server list to `/api/status`, this file becomes the public router. The previous authenticated server-list endpoint (used by the admin UI) must be moved to `/api/servers` if it isn't already there. Read `crates/server/src/router/api/mod.rs` carefully to plan the move.

- [ ] **Step 1: Audit the existing `/api/status` route usage**

```
rg -n "'/api/status'" apps/web/src crates/
rg -n '"/api/status"' apps/web/src crates/
```

Document the callers. The new spec mandates `/api/status` be anonymous public. Callers that need the admin server list must be migrated to `/api/servers` (or another existing admin route — verify with `rg "Router::new\(\)" crates/server/src/router/api/`). Outcome: the file `crates/server/src/router/api/status.rs` currently used for admin reads must either be (a) repurposed for public reads (move admin logic to a new file `dashboard.rs` or fold into `servers.rs`) or (b) renamed and a fresh `status.rs` is created. Choose based on which is cleaner; document the decision in the commit message.

- [ ] **Step 2: Rewrite `status.rs` as the public router**

Final file structure:

```rust
//! Public anonymous status endpoints. Unconditional redaction — see spec §"Defense-in-depth".

use std::sync::Arc;
use axum::{Router, routing::get, extract::{State, Path, Query}};
use crate::state::AppState;
use crate::error::AppError;
use crate::response::ApiResponse;
use crate::service::public_status;

pub fn public_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/status/config", get(get_config))
        .route("/status", get(list_servers))
        .route("/status/servers/{id}", get(get_server_detail))
        .route("/status/servers/{id}/metrics", get(get_server_metrics))
        .route("/status/servers/{id}/uptime-daily", get(get_server_uptime_daily))
        .route("/status/network", get(network_overview))
        .route("/status/network/{id}", get(network_server_detail))
        .route("/status/ip-quality", get(ip_quality_overview))
        .route("/status/incidents", get(list_incidents))
        .route("/status/maintenances", get(list_maintenances))
}
```

Each handler reads the singleton config, checks `enabled` (returns `403 Disabled` if false), checks the relevant sub-page toggle (`show_server_detail`, `show_network`, `show_ip_quality`, `show_incidents`, `show_maintenance`), then delegates to `public_status::*`.

Example pattern:

```rust
#[utoipa::path(
    get,
    path = "/api/status/servers/{id}",
    tag = "public-status",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, body = ApiResponse<PublicServerDetail>),
        (status = 403, description = "Public status disabled"),
        (status = 404, description = "Server not in scope or not found"),
    )
)]
async fn get_server_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<ApiResponse<PublicServerDetail>, AppError> {
    let scope = public_status::resolve_scope(&state.db).await?;
    if !scope.config.enabled {
        return Err(AppError::Forbidden("disabled".into()));
    }
    if !scope.config.show_server_detail {
        return Err(AppError::Forbidden("disabled".into()));
    }
    if !scope.server_ids.contains(&id) {
        return Err(AppError::NotFound("server".into()));
    }
    Ok(ApiResponse::new(public_status::get_server_detail(&state.db, &id).await?))
}
```

`get_config` does **not** gate on `enabled` — it always returns the config DTO so the SPA can render a "site disabled" notice.

- [ ] **Step 3: Wire it in `router/api/mod.rs`**

```rust
pub mod status; // already declared
// ...
// In the public route assembly:
.merge(status::public_router())
.merge(status_page::public_router())  // remove this line — see Task 7
```

The old `status_page::public_router()` (the legacy `GET /status/{slug}`) is removed in Task 7. For now, it can co-exist briefly; the slug route is unreachable once the singleton row is the only one but doesn't error.

- [ ] **Step 4: Rate limiting**

The spec calls for a 60 req/60s per-IP bucket on `/api/status/*` (see spec §"Rate limiting"). Inspect `crates/server/src/state.rs` for existing rate-limit DashMaps (`login_rate_limit`, `register_rate_limit`) and add `public_rate_limit: DashMap<IpAddr, Window>`. Add a small middleware function `public_status_rate_limit` that lives next to `status.rs` and is applied with `.layer(middleware::from_fn_with_state(...))` to the public router. Mirror the existing rate-limit pattern.

- [ ] **Step 5: Compile + clippy**

```
cargo build -p serverbee-server 2>&1 | head -40
cargo clippy -p serverbee-server -- -D warnings 2>&1 | head -60
```

Resolve every error and warning.

- [ ] **Step 6: Commit**

```
git add crates/server/src/router/api/status.rs crates/server/src/router/api/mod.rs crates/server/src/state.rs
git commit -m "feat(server): wire /api/status/* public router with redaction"
```

---

# Phase 3 — Backend admin reshape & cleanup

### Task 6: Simplify admin status-page endpoints

**Files:**
- Modify: `crates/server/src/router/api/status_page.rs`
- Modify: `crates/server/src/service/status_page.rs`

- [ ] **Step 1: Drop legacy fields from `service/status_page.rs`**

Remove `CreateStatusPage::slug`, `CreateStatusPage::theme_ref`, `CreateStatusPage::custom_css`, `CreateStatusPage::show_values` (and the same on `UpdateStatusPage`). Replace with `default_layout`, `show_server_detail`, `show_network`, `show_incidents`, `show_maintenance`. Keep `show_ip_quality`.

The singleton accessor `get_singleton(db) -> Result<status_page::Model, AppError>` reuses `public_status::load_config`. The update path takes the singleton's id implicitly (caller resolves it).

Delete `StatusPageService::create` and `StatusPageService::delete` — singleton can't be created/deleted via API; the migration ensures the row exists.

- [ ] **Step 2: Rewrite admin routes**

In `crates/server/src/router/api/status_page.rs`:

- Delete `public_router()` (the `/status/{slug}` route).
- Delete `get_public_status_page` handler entirely.
- Replace `read_router()` with: `GET /status-page` → `get_singleton`.
- Replace `write_router()` with: `PUT /status-page` → `update_singleton`. Delete `create_status_page` and `delete_status_page` handlers.

The new admin DTOs:

```rust
#[derive(Deserialize, ToSchema)]
pub struct UpdateStatusPageRequest {
    pub enabled: bool,
    pub title: String,
    pub description: Option<String>,
    pub server_ids: Vec<String>,
    pub default_layout: String,
    pub show_server_detail: bool,
    pub show_network: bool,
    pub show_ip_quality: bool,
    pub show_incidents: bool,
    pub show_maintenance: bool,
    pub uptime_yellow_threshold: f64,
    pub uptime_red_threshold: f64,
}
```

- [ ] **Step 3: Update `service/incident.rs` and `service/maintenance.rs`**

Anywhere `status_page_ids_json` is read/written, replace with `is_public`. Update the create / update DTOs to accept `is_public: bool` instead of `status_page_ids: Vec<String>`. Existing list/list-public queries become simple `is_public = true` filters.

- [ ] **Step 4: Update mod.rs wiring**

In `crates/server/src/router/api/mod.rs`, remove the merge of `status_page::public_router()`. Keep the read/write router merges, which now expose only the singleton GET/PUT.

- [ ] **Step 5: Compile + clippy**

```
cargo build -p serverbee-server 2>&1 | head -40
cargo clippy -p serverbee-server -- -D warnings 2>&1 | head -60
cargo build --workspace
```

The workspace must compile cleanly. Fix all callers (router wiring, handler bodies, tests' compile sites).

- [ ] **Step 6: Commit**

```
git add crates/server/src/
git commit -m "feat(server): collapse admin status-page API to singleton GET/PUT"
```

---

# Phase 4 — Backend integration tests

Each test in this phase follows the project's existing integration test pattern. Read `crates/server/tests/` for examples (e.g., `tests/api_servers.rs`) to see how `TestApp::new()` is constructed and how authenticated vs unauthenticated requests are made.

### Task 7: Anonymous baseline test

**Files:**
- Create: `crates/server/tests/public_status_anonymous.rs`

- [ ] **Step 1: Write the test**

The test seeds a status page (`enabled = true`, `show_server_detail/network/ip_quality/incidents/maintenance = true`) and at least one server with metrics, then hits every public endpoint anonymously and asserts `200`.

```rust
#[tokio::test]
async fn all_endpoints_return_200_anonymously() {
    let app = TestApp::seed().await;
    app.enable_public_status_with_server("server-A").await;

    for path in [
        "/api/status/config",
        "/api/status",
        "/api/status/servers/server-A",
        "/api/status/servers/server-A/uptime-daily",
        "/api/status/network",
        "/api/status/ip-quality",
        "/api/status/incidents",
        "/api/status/maintenances",
    ] {
        let res = app.get(path).await;
        assert_eq!(res.status(), 200, "{path} returned {}", res.status());
    }
}
```

`TestApp::enable_public_status_with_server` is a new helper in `crates/server/tests/common/mod.rs`. Implement it: hits the admin `PUT /api/status-page` with all toggles `true`, `server_ids = [id]`, then seeds a `server_report` for that id (use `IpQualityService` mock if needed for the ip-quality endpoint).

- [ ] **Step 2: Run, fix, repeat**

```
cargo test -p serverbee-server --test public_status_anonymous
```

If `TestApp` lacks a method, add it in `tests/common/mod.rs`. Iterate until green.

- [ ] **Step 3: Commit**

```
git add crates/server/tests/public_status_anonymous.rs crates/server/tests/common/mod.rs
git commit -m "test(server): public status endpoints respond 200 anonymously"
```

---

### Task 8: Server-identity redaction test

**Files:**
- Create: `crates/server/tests/public_status_redaction.rs`

- [ ] **Step 1: Write the test**

The test seeds a server with explicit `ipv4 = "1.2.3.4"`, `ipv6 = "fe80::1"`, network interface list with MACs. Then GETs `/api/status` and `/api/status/servers/{id}` and asserts the response JSON contains **no** key matching:

```
"ipv4", "ipv6", "interfaces", "public_ip", "mac_address",
"network_interface", "network_interfaces"
```

Use a recursive JSON walker:

```rust
fn assert_no_keys(value: &serde_json::Value, forbidden: &[&str]) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                assert!(!forbidden.contains(&k.as_str()), "found forbidden key: {k}");
                assert_no_keys(v, forbidden);
            }
        }
        serde_json::Value::Array(arr) => arr.iter().for_each(|v| assert_no_keys(v, forbidden)),
        _ => {}
    }
}
```

Also assert the response body does not contain the literal substring `"1.2.3.4"` or `"fe80::1"` — defense-in-depth against fields renamed but still leaking.

- [ ] **Step 2: Run, fix, repeat**

```
cargo test -p serverbee-server --test public_status_redaction
```

- [ ] **Step 3: Commit**

```
git add crates/server/tests/public_status_redaction.rs
git commit -m "test(server): server identity redaction on public status endpoints"
```

---

### Task 9: Authenticated-but-redacted test

**Files:**
- Create: `crates/server/tests/public_status_redaction_authenticated.rs`

- [ ] **Step 1: Write the test**

Same assertions as Task 8, but each request is made with a valid admin session cookie **and** a valid admin API key header. The response must be byte-identical (or at least field-set-identical) to the anonymous case. The test proves the unconditional redaction policy.

- [ ] **Step 2: Run, fix, commit**

```
cargo test -p serverbee-server --test public_status_redaction_authenticated
git add crates/server/tests/public_status_redaction_authenticated.rs
git commit -m "test(server): public status redaction is unconditional"
```

---

### Task 10: IP-quality redaction test

**Files:**
- Create: `crates/server/tests/public_status_ip_quality_redaction.rs`

- [ ] **Step 1: Write the test**

Seed an `ip_quality_snapshot` with `ip = "9.9.9.9"`, `asn = "AS12345"`, `as_org = "EvilCo"`, `region = "US-WA"`, `city = "Seattle"`, all booleans (`is_proxy`, etc.) set, `abuse_email = "abuse@evilco"`. Seed unlock results with `detail = "secret error blob"`.

GET `/api/status/ip-quality` and:

1. Walk **only** each entry's `ip_quality` object (not the entire response): assert keys absent — `ip`, `asn`, `as_org`, `region`, `city`, `is_proxy`, `is_vpn`, `is_hosting`, `is_tor`, `is_abuser`, `is_mobile`, `asn_abuser_score`, `abuse_email`. **Do not** apply this list to the full tree because `unlock_results[*].region` legitimately exists (different `region` semantics — service unlock region).
2. Walk each `unlock_results` entry: assert `detail` is absent.
3. Assert response body contains no `"9.9.9.9"`, `"AS12345"`, `"EvilCo"`, `"abuse@evilco"`, `"secret error blob"` substrings.

- [ ] **Step 2: Run, fix, commit**

```
cargo test -p serverbee-server --test public_status_ip_quality_redaction
git add crates/server/tests/public_status_ip_quality_redaction.rs
git commit -m "test(server): ip-quality redaction on public status endpoints"
```

---

### Task 11: Server scope test

**Files:**
- Create: `crates/server/tests/public_status_scope.rs`

- [ ] **Step 1: Write the test**

Seed three servers: `A`, `B`, `H`. Config: `server_ids = [A, B]`. Then mark `H.hidden = true` AND add `H` to `server_ids` (simulating the unsafe configuration the guard must defeat).

Assert:

- `GET /api/status` body contains `A` and `B` but **not** `H`.
- `GET /api/status/servers/H` → `404`.
- `GET /api/status/servers/Z` (never existed) → `404`.
- `GET /api/status/network/H` → `404`.
- `GET /api/status/ip-quality` response contains no entry with `server_id = H`.

- [ ] **Step 2: Run, fix, commit**

```
cargo test -p serverbee-server --test public_status_scope
git add crates/server/tests/public_status_scope.rs
git commit -m "test(server): public status scope filters hidden and out-of-list servers"
```

---

### Task 12: Sub-page gating test

**Files:**
- Create: `crates/server/tests/public_status_gating.rs`

- [ ] **Step 1: Write the test**

Parameterize over the five toggles and their corresponding endpoints:

| Toggle | Endpoint that should 403 when off |
|---|---|
| `show_server_detail` | `/api/status/servers/{id}`, `/api/status/servers/{id}/metrics` |
| `show_network` | `/api/status/network`, `/api/status/network/{id}` |
| `show_ip_quality` | `/api/status/ip-quality` |
| `show_incidents` | `/api/status/incidents` |
| `show_maintenance` | `/api/status/maintenances` |

For each toggle, set that toggle to false (others true), hit the endpoint, assert `403`. Then set the toggle back to true, assert `200`.

Also test: `enabled = false` returns `403` for every endpoint **except** `/api/status/config` (which always returns `200` with `enabled = false` in the body).

- [ ] **Step 2: Run, fix, commit**

```
cargo test -p serverbee-server --test public_status_gating
git add crates/server/tests/public_status_gating.rs
git commit -m "test(server): public status sub-page toggles gate each endpoint"
```

---

### Task 13: Full test suite gate

- [ ] **Step 1: Run full workspace test**

```
cargo test --workspace 2>&1 | tail -40
cargo clippy --workspace -- -D warnings 2>&1 | tail -20
```

Both must be green. If existing tests broke due to entity/service refactoring, fix them (their assertions on `status_page.slug` / `incidents.status_page_ids_json` will need to migrate to the new schema).

- [ ] **Step 2: Commit any fixes**

```
git add -p   # carefully stage only the test fixes
git commit -m "test(server): adapt existing tests to status_page singleton schema"
```

---

# Phase 5 — Frontend types

### Task 14: Replace public status types in api-schema.ts

**Files:**
- Modify: `apps/web/src/lib/api-schema.ts`

- [ ] **Step 1: Locate existing types**

Read the section starting at `apps/web/src/lib/api-schema.ts:168` (the `PublicStatusPageData` interface from the old slug-based world). Delete the entire `PublicStatusPageData` block and the legacy `StatusPageItem` fields `slug`, `theme_ref`, `custom_css`, `show_values`, `show_ip_quality` is retained, plus all the new toggles.

- [ ] **Step 2: Add new public DTO mirrors**

```typescript
export interface PublicStatusConfig {
  enabled: boolean
  title: string
  description: string | null
  default_layout: 'list' | 'grid'
  show_server_detail: boolean
  show_network: boolean
  show_ip_quality: boolean
  show_incidents: boolean
  show_maintenance: boolean
  uptime_yellow_threshold: number
  uptime_red_threshold: number
}

// One TS interface per Rust DTO in Task 3. Translation rules:
// - field names: keep snake_case as-is (we don't camelCase the API surface)
// - Rust `Option<T>` → TS `T | null`
// - Rust `String` / `&str` → TS `string`
// - Rust `f32` / `f64` / `i32` / `u64` / `usize` → TS `number`
// - Rust `Vec<T>` → TS `T[]`
// - Rust `#[serde(flatten)] summary: PublicServerSummary` on `PublicServerDetail`
//   means the TS type is `PublicServerSummary & { /* extra fields */ }`
// Copy each Rust DTO struct from Task 3 into this section field-for-field.
```

Field names and types must exactly match the Rust DTOs from Task 3. Read Task 3 again before writing this; do not invent fields not present there.

Update `StatusPageItem` to match the new admin singleton shape:

```typescript
export interface StatusPageItem {
  id: string
  enabled: boolean
  title: string
  description: string | null
  server_ids: string[]
  default_layout: 'list' | 'grid'
  show_server_detail: boolean
  show_network: boolean
  show_ip_quality: boolean
  show_incidents: boolean
  show_maintenance: boolean
  uptime_yellow_threshold: number
  uptime_red_threshold: number
  created_at: string
  updated_at: string
}
```

- [ ] **Step 3: Typecheck**

```
cd apps/web && bun run typecheck 2>&1 | tail -40
```

Expect errors at every callsite of the removed types — those are addressed in subsequent tasks. Don't fix them yet; record the failing callsites in a brief note so subsequent tasks know what to target.

- [ ] **Step 4: Commit**

```
git add apps/web/src/lib/api-schema.ts
git commit -m "feat(web): replace public status DTOs and singleton status-page schema"
```

---

# Phase 6 — Frontend public layout + servers

### Task 15: Public layout shell + StatusHeader

**Files:**
- Create: `apps/web/src/components/status/status-header.tsx`
- Create: `apps/web/src/hooks/use-public-status.ts`
- Modify: `apps/web/src/routes/status.tsx`

- [ ] **Step 1: Hook for fetching public config**

`hooks/use-public-status.ts`:

```typescript
import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { PublicStatusConfig } from '@/lib/api-schema'

export function usePublicStatusConfig() {
  return useQuery({
    queryKey: ['public-status', 'config'],
    queryFn: () => api.get<PublicStatusConfig>('/api/status/config'),
    refetchInterval: 5 * 60_000,
    staleTime: 5 * 60_000
  })
}
```

- [ ] **Step 2: StatusHeader component**

Reads config from `usePublicStatusConfig`, renders:

- Left: logo (Lucide `Server` icon) + `{config.title}`, links to `/status`.
- Center / right: sub-page links to `/status/network` (if `show_network`), `/status/ip-quality` (if `show_ip_quality`). Use TanStack Router `<Link>`. Mark the active link with an underline + foreground color.
- Right: existing `<ThemeToggle />` from `components/layout/theme-toggle.tsx`, the i18n `EN/中文` toggle (same pattern as old `status.index.tsx:90`), and a "Sign in" link → `/login`.

Use `cn()` for styling. Layout: `border-b`, `max-w-6xl mx-auto`, `flex items-center justify-between px-4 py-3`.

- [ ] **Step 3: Rewrite `routes/status.tsx`**

```tsx
import { createFileRoute, Outlet } from '@tanstack/react-router'
import { StatusHeader } from '@/components/status/status-header'

export const Route = createFileRoute('/status')({
  component: StatusLayout
})

function StatusLayout() {
  return (
    <div className="min-h-screen bg-background">
      <StatusHeader />
      <main className="mx-auto max-w-6xl px-4 py-8"><Outlet /></main>
    </div>
  )
}
```

- [ ] **Step 4: Typecheck + lint**

```
cd apps/web && bun run typecheck && bun x ultracite check
```

- [ ] **Step 5: Commit**

```
git add apps/web/src/components/status/status-header.tsx apps/web/src/hooks/use-public-status.ts apps/web/src/routes/status.tsx
git commit -m "feat(web): public status layout with header and config hook"
```

---

### Task 16: VPS list page with list/grid toggle

**Files:**
- Create: `apps/web/src/components/status/layout-toggle.tsx`
- Modify: `apps/web/src/routes/status.index.tsx`

- [ ] **Step 1: LayoutToggle**

A small segmented control (two icon buttons — Lucide `LayoutGrid` and `List`) that takes `value: 'list' | 'grid'` and `onChange`. Persists nothing itself.

- [ ] **Step 2: Rewrite status.index.tsx**

The page:

1. Calls `usePublicStatusConfig`. If `data.enabled === false`, render "Status page is currently disabled by the administrator" message.
2. Calls `useQuery<PublicServerSummary[]>({ queryKey: ['public-status', 'servers'], queryFn: () => api.get('/api/status'), refetchInterval: 30_000 })`.
3. Reads `localStorage['serverbee.status.layout']` (with fallback to `config.default_layout`) into state.
4. Renders `<LayoutToggle>` in the page header.
5. Renders either a `<Grid>` of `<ServerSummaryCard>` or a `<Table>` of `<ServerSummaryRow>`. The grid cell **must reuse the existing `ServerStatusCard` pattern** from the old `status.index.tsx:29-72` for consistency, ported into `apps/web/src/components/status/server-summary-card.tsx`. The list row reuses the row layout from `status.$slug.tsx` (the soon-to-be-deleted file) — port it to `apps/web/src/components/status/server-summary-row.tsx` and include the 90-day `<UptimeTimeline>`.
6. Clicking a card / row navigates to `/status/server/$id` **only if `config.show_server_detail`** — otherwise the card is non-interactive (no hover state, no nav).

- [ ] **Step 3: Typecheck + visual check**

```
cd apps/web && bun run typecheck && bun x ultracite check
```

Run `bun run dev` and visit `http://localhost:5173/status`. Verify list and grid render and the toggle persists across reloads.

- [ ] **Step 4: Commit**

```
git add apps/web/src/routes/status.index.tsx apps/web/src/components/status/
git commit -m "feat(web): public status servers list with grid/list toggle"
```

---

# Phase 7 — Frontend content extraction

The four extraction tasks all follow the same pattern: take an existing `_authed/...` route's render body, extract a presentation-only React component that accepts a `variant: 'admin' | 'public'` prop, then make the authenticated route a thin wrapper and create the public route as another thin wrapper.

### Task 17: Extract ServerDetailContent

**Files:**
- Create: `apps/web/src/components/status/server-detail-content.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$id-page.tsx`
- Create: `apps/web/src/routes/status.server.$serverId.tsx`

- [ ] **Step 1: Extract the body**

Read `apps/web/src/routes/_authed/servers/$id-page.tsx`. Identify the section that renders server info, metrics charts, traffic, cost — everything below the page header. Move that body to a new function component:

```tsx
export interface ServerDetailContentProps {
  serverId: string
  server: ServerResponse | PublicServerDetail
  variant: 'admin' | 'public'
}

export function ServerDetailContent({ serverId, server, variant }: ServerDetailContentProps) {
  const isPublic = variant === 'public'
  // ... existing render, with conditional gates
}
```

When `isPublic`:

- Do not render `<ServerActionButtons>` (Edit / Recover / Capabilities / Terminal / Files / Docker).
- Do not render `<AgentVersionSection>` (or render only `agent_version` text without the upgrade button — keep this consistent with the rest of the public detail, which retains `agent_version` per spec).
- Skip the metadata rows that reference `server.ipv4`, `server.ipv6`. These should naturally be `undefined` on the public DTO, but guard with `'ipv4' in server` checks if TypeScript narrowing requires it.

- [ ] **Step 2: Use the component in `_authed/servers/$id-page.tsx`**

Replace the existing body with `<ServerDetailContent serverId={id} server={server} variant="admin" />` — keep the page header / breadcrumbs / dialogs at this level.

- [ ] **Step 3: Create the public route**

`apps/web/src/routes/status.server.$serverId.tsx`:

```tsx
import { useQuery } from '@tanstack/react-query'
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { ServerDetailContent } from '@/components/status/server-detail-content'
import { usePublicStatusConfig } from '@/hooks/use-public-status'
import { api } from '@/lib/api-client'
import type { PublicServerDetail } from '@/lib/api-schema'

export const Route = createFileRoute('/status/server/$serverId')({
  component: PublicServerDetailPage
})

function PublicServerDetailPage() {
  const { serverId } = Route.useParams()
  const navigate = useNavigate()
  const { data: config } = usePublicStatusConfig()
  const { data, isLoading, error } = useQuery({
    queryKey: ['public-status', 'server', serverId],
    queryFn: () => api.get<PublicServerDetail>(`/api/status/servers/${serverId}`),
    refetchInterval: 30_000,
    retry: false
  })

  if (config && !config.show_server_detail) {
    navigate({ to: '/status' })
    return null
  }
  if (isLoading) return <Loading />
  if (error) return <NotFound />
  if (!data) return null
  return <ServerDetailContent serverId={serverId} server={data} variant="public" />
}
```

- [ ] **Step 4: Typecheck + visual check**

```
cd apps/web && bun run typecheck && bun x ultracite check
```

`bun run dev` → visit `/status` → click a server card → confirm the detail renders without admin buttons.

- [ ] **Step 5: Commit**

```
git add apps/web/src/components/status/server-detail-content.tsx apps/web/src/routes/_authed/servers/\$id-page.tsx apps/web/src/routes/status.server.\$serverId.tsx
git commit -m "feat(web): public status server detail page with variant=public"
```

---

### Task 18: Extract NetworkOverviewContent

**Files:**
- Create: `apps/web/src/components/status/network-overview-content.tsx`
- Modify: `apps/web/src/routes/_authed/network/index.tsx`
- Create: `apps/web/src/routes/status.network.tsx` (layout)
- Create: `apps/web/src/routes/status.network.index.tsx`

- [ ] **Step 1: Extract the body**

Same pattern as Task 17. The public variant hides any edit buttons / target management actions.

- [ ] **Step 2: Public network layout**

`status.network.tsx`:

```tsx
import { createFileRoute, Outlet } from '@tanstack/react-router'
export const Route = createFileRoute('/status/network')({ component: () => <Outlet /> })
```

- [ ] **Step 3: Index page**

`status.network.index.tsx` mounts `<NetworkOverviewContent variant="public" />` after fetching `/api/status/network`, gated on `config.show_network`.

- [ ] **Step 4: Typecheck + visual check + commit**

```
cd apps/web && bun run typecheck && bun x ultracite check
git add apps/web/src/components/status/network-overview-content.tsx apps/web/src/routes/_authed/network/index.tsx apps/web/src/routes/status.network.tsx apps/web/src/routes/status.network.index.tsx
git commit -m "feat(web): public status network overview"
```

---

### Task 19: Extract NetworkDetailContent

**Files:**
- Create: `apps/web/src/components/status/network-detail-content.tsx`
- Modify: `apps/web/src/routes/_authed/network/$serverId.tsx`
- Create: `apps/web/src/routes/status.network.$serverId.tsx`

Same pattern as Task 17 / 18. Apply each sub-step explicitly:

- [ ] **Step 1: Extract `NetworkDetailContent` from `_authed/network/$serverId.tsx`**

Move the page body (latency chart, target cards, traceroute history, anomaly table, traceroute start button, target edit dialog) into the new component. Accept `variant: 'admin' | 'public'`.

When `variant === 'public'`, hide:
- Traceroute start button (anonymous can't trigger probes).
- Target add / edit / delete buttons and dialogs.
- Anomaly clear / delete buttons.

- [ ] **Step 2: Mount in `_authed/network/$serverId.tsx`**

Replace the body with `<NetworkDetailContent serverId={id} data={data} variant="admin" />`. Keep the route's data fetching at the page level.

- [ ] **Step 3: Create `status.network.$serverId.tsx`**

```tsx
export const Route = createFileRoute('/status/network/$serverId')({ component: PublicNetworkDetailPage })

function PublicNetworkDetailPage() {
  const { serverId } = Route.useParams()
  const { data: config } = usePublicStatusConfig()
  const { data, isLoading, error } = useQuery({
    queryKey: ['public-status', 'network', serverId],
    queryFn: () => api.get<PublicNetworkServerDetail>(`/api/status/network/${serverId}`),
    refetchInterval: 30_000,
    retry: false
  })
  if (config && !config.show_network) { /* redirect to /status */ }
  if (isLoading) return <Loading />
  if (error || !data) return <NotFound />
  return <NetworkDetailContent serverId={serverId} data={data} variant="public" />
}
```

- [ ] **Step 4: Typecheck + visual check + commit**

```
cd apps/web && bun run typecheck && bun x ultracite check
git add apps/web/src/components/status/network-detail-content.tsx apps/web/src/routes/_authed/network/'$serverId.tsx' apps/web/src/routes/status.network.'$serverId.tsx'
git commit -m "feat(web): public status network per-server detail"
```

---

### Task 20: Extract IpQualityContent + public IpQualityCard

**Files:**
- Modify: `apps/web/src/components/ip-quality/ip-quality-card.tsx` (add `variant?: 'admin' | 'public'`)
- Create: `apps/web/src/components/status/ip-quality-content.tsx`
- Modify: `apps/web/src/routes/_authed/ip-quality.tsx`
- Create: `apps/web/src/routes/status.ip-quality.tsx`

- [ ] **Step 1: Extend `IpQualityCard`**

Add `variant?: 'admin' | 'public'` (default `'admin'`). When `'public'`, render only:

- `country` (flag + name),
- `ip_type` (residential / datacenter / mobile chip),
- `risk_score` + `risk_level` badge.

Do not render any of: `ip`, `asn`, `as_org`, `region`/`city`, `is_proxy`/`is_vpn`/`is_hosting`/`is_tor`/`is_abuser`/`is_mobile`, `asn_abuser_score`, `abuse_email`. These fields are absent from the public DTO; the variant flag mostly serves as a type-narrowing hint for the renderer.

- [ ] **Step 2: Extract IpQualityContent**

Move the body of `_authed/ip-quality.tsx` to `components/status/ip-quality-content.tsx` with `variant: 'admin' | 'public'`. The `UnlockMatrix` is rendered unchanged in both modes. The card grid uses the variant prop.

- [ ] **Step 3: Mount in routes**

`_authed/ip-quality.tsx` becomes a thin wrapper. `status.ip-quality.tsx` is a new file that fetches `/api/status/ip-quality`, gated on `config.show_ip_quality`, and mounts the content with `variant="public"`.

- [ ] **Step 4: Typecheck + visual check + commit**

```
cd apps/web && bun run typecheck && bun x ultracite check
git add ...
git commit -m "feat(web): public status ip-quality page with redacted card variant"
```

---

# Phase 8 — Frontend admin settings reshape

### Task 21: Singleton form in settings/status-pages.tsx

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/status-pages.tsx`
- Modify: `apps/web/src/routes/_authed/settings/status-pages.test.tsx`

- [ ] **Step 1: Replace the page**

The new page is one form, plus the existing Incidents and Maintenances management widgets (which are already implemented; keep them).

Form fields:

- Master switch: `enabled` (Switch component).
- Text: `title`, `description`.
- Multi-select: `server_ids` (existing picker).
- Select: `default_layout` (`list` | `grid`).
- Five switches: `show_server_detail`, `show_network`, `show_ip_quality`, `show_incidents`, `show_maintenance`.
- Two number inputs: `uptime_yellow_threshold`, `uptime_red_threshold`.

Load via `GET /api/status-page`. Save via `PUT /api/status-page`. Use `useMutation` + `toast.success` / `toast.error`.

Incidents / Maintenances management widgets continue to call their existing CRUD endpoints, plus they gain an `is_public` checkbox in their create/edit dialogs.

- [ ] **Step 2: Rewrite the test**

Replace the existing test file. The new test verifies:

- The form loads existing values from `GET /api/status-page` mock.
- Toggling each switch and saving sends the correct PUT body.
- The form shows a 'site disabled' notice when `enabled = false`.

- [ ] **Step 3: Typecheck + lint + test**

```
cd apps/web && bun run typecheck && bun x ultracite check && bun run test -- status-pages
```

- [ ] **Step 4: Commit**

```
git add apps/web/src/routes/_authed/settings/status-pages.tsx apps/web/src/routes/_authed/settings/status-pages.test.tsx
git commit -m "feat(web): simplify status-page settings to singleton form"
```

---

# Phase 9 — Cleanup

### Task 22: Delete legacy files

**Files:**
- Delete: `apps/web/src/routes/status.$slug.tsx`
- Delete: `apps/web/src/routes/status-slug.test.tsx`

- [ ] **Step 1: Delete and re-typecheck**

```
rm apps/web/src/routes/status.\$slug.tsx apps/web/src/routes/status-slug.test.tsx
cd apps/web && bun run typecheck
```

- [ ] **Step 2: Commit**

```
git rm apps/web/src/routes/status.\$slug.tsx apps/web/src/routes/status-slug.test.tsx
git commit -m "chore(web): drop legacy slug-based public status page"
```

---

### Task 23: Add i18n keys

**Files:**
- Modify: `apps/web/src/locales/en/status.json`
- Modify: `apps/web/src/locales/zh/status.json`

- [ ] **Step 1: Add keys used by the new components**

Skim the new components for hardcoded user-facing English strings. Replace each with a `t('key')` call and add the corresponding key to both locale files. Examples:

- Header: `signin`, `nav_servers`, `nav_network`, `nav_ip_quality`.
- LayoutToggle: `layout_grid_tooltip`, `layout_list_tooltip`.
- StatusIndex: `site_disabled_notice`, `loading`, `load_failed`.

Match the existing localization style in `apps/web/src/locales/en/status.json`.

- [ ] **Step 2: Commit**

```
git add apps/web/src/locales/
git commit -m "feat(web): i18n keys for public status header and toggles"
```

---

### Task 24: Final checks

- [ ] **Step 1: Full backend tests + clippy**

```
cargo test --workspace 2>&1 | tail -30
cargo clippy --workspace -- -D warnings 2>&1 | tail -20
```

Both green.

- [ ] **Step 2: Full frontend checks**

```
cd apps/web && bun run typecheck && bun x ultracite check && bun run test
```

All green.

- [ ] **Step 3: Update progress log**

`docs/superpowers/plans/PROGRESS.md` — append a section dated `2026-05-26 — Public Status Page Refactor` summarizing what shipped.

- [ ] **Step 4: Commit**

```
git add docs/superpowers/plans/PROGRESS.md
git commit -m "docs: log public status page refactor"
```

---

# Phase 10 — Remote verification

VPS credentials: `root@207.241.173.217:22` / password `2ucW09DzI@!LZ!e47yG`. The host is the project's reusable test VPS (Ubuntu 24.04). The CLAUDE.md / memory note `reference_test_vps.md` covers it.

### Task 25: Build, ship, smoke test

- [ ] **Step 1: Cross-compile**

The host architecture varies; check via SSH first:

```
sshpass -p '2ucW09DzI@!LZ!e47yG' ssh -o StrictHostKeyChecking=no root@207.241.173.217 'uname -m'
```

If `x86_64`, run on macOS host (assuming `aarch64-apple-darwin` dev machine):

```
cargo build --release --target x86_64-unknown-linux-musl -p serverbee-server
```

Confirm a working toolchain exists; otherwise build inside a Linux container or use a cross helper present in this repo (`scripts/build-cross.sh` if it exists; otherwise document the gap).

- [ ] **Step 2: scp and run**

```
sshpass -p '...' scp -o StrictHostKeyChecking=no \
  target/x86_64-unknown-linux-musl/release/serverbee-server \
  root@207.241.173.217:/opt/serverbee/serverbee-server.new

sshpass -p '...' ssh root@207.241.173.217 'systemctl stop serverbee || true; \
  mv /opt/serverbee/serverbee-server.new /opt/serverbee/serverbee-server; \
  chmod +x /opt/serverbee/serverbee-server; \
  systemctl start serverbee || /opt/serverbee/serverbee-server &'
```

(Confirm service unit name first via `systemctl list-units 'serverbee*'`.)

- [ ] **Step 3: Smoke test the public surface**

```
curl -s http://207.241.173.217:9527/api/status/config | jq
curl -s http://207.241.173.217:9527/api/status | jq '.data[0] | keys'
curl -s http://207.241.173.217:9527/api/status/ip-quality | jq '.data.entries[0].ip_quality | keys'
```

Assertions:

- `/api/status/config` returns `enabled` boolean and all toggles.
- `/api/status` array entries do **not** contain `ipv4` / `ipv6` / `interfaces` keys (`jq 'keys'` shows the field set).
- `/api/status/ip-quality` entries' `ip_quality` object does not contain `ip`, `asn`, `as_org`, `abuse_email`.
- Disable each toggle in admin, re-curl the corresponding endpoint, expect `403`.

- [ ] **Step 4: Visual check**

Open `http://207.241.173.217:9527/status` in a desktop browser. Verify:

- Header shows title + enabled sub-page nav + language/theme/signin.
- List/grid toggle persists across reload.
- Server detail page renders without admin actions.
- Network and IP Quality pages load when enabled.
- An admin logged into the same browser sees the same redacted DTO (open DevTools → Network → confirm the response body).

- [ ] **Step 5: Log findings**

If any test fails, file the regression as a new task here in this plan with a clear repro, and fix before declaring done. Otherwise, commit a checkpoint:

```
git commit --allow-empty -m "chore: public status refactor smoke-verified on test VPS"
```

---

## Done criteria

- Every checkbox above is ticked.
- `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`, `bun run typecheck`, `bun x ultracite check`, `bun run test` all green.
- Manual smoke on test VPS passed (Phase 10).
- No commits include Claude attribution.
- No `git push` invoked at any point (user pushes manually).
