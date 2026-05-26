# Public Status Page Refactor — Design Spec

> Date: 2026-05-26

## Overview

Refactor the public status page from a multi-slug page builder into a single
global anonymous-accessible site that mirrors the rich content available in the
authenticated app. Visitors should be able to browse servers (list or grid),
drill into per-server detail, inspect network quality, and view IP-quality
unlock data — without exposing IP-level identifiers. Administrators configure a
single global status page from `settings/status-pages`; sub-pages are gated by
per-feature toggles. Sensitive fields are stripped at the API boundary, never
relying on UI hiding.

## Goals

- One canonical anonymous URL: `/status` (multi-slug removed).
- Visitors can view servers as list **or** grid, click into a public server
  detail page, open a network-quality sub-page, and open an IP-quality
  sub-page. Each sub-page (and detail page) is independently gated by an
  admin toggle.
- The public surface reuses the existing authenticated server detail, network
  quality, and IP quality screens — minus management actions and minus
  IP-level identifiers.
- Sensitive fields (server IPv4/IPv6, agent-detected public IP, interface
  names/MACs, IP-quality egress IP/ASN/ISP/operator) are stripped server-side
  before serialization.
- Admin settings are simplified from a CRUD list of slug pages to a single
  global configuration form.

## Non-Goals

- No realtime WebSocket push to anonymous visitors; HTTP polling only.
- No new theming primitives for the public site. The public site follows the
  admin global theme; previous `theme_ref` / `custom_css` columns are removed.
- No new metrics, incident, or maintenance functionality. The existing
  incidents and planned-maintenance models are retained with no behavioral
  change beyond an admin toggle controlling whether they render publicly.
- No anonymous mutation endpoints. All `/api/status/*` endpoints are
  read-only.

## Existing Infrastructure

### Reused unchanged

- `UptimeService::get_daily_filled` and the `uptime_daily` table.
- `incidents` and `maintenances` tables (and their admin CRUD UI). The
  per-row `status_page_ids_json` many-to-many array column is replaced by a
  simpler `is_public` boolean (see migration). Behavior: any row with
  `is_public = true` renders on the public status page when the
  corresponding admin toggle (`show_incidents` / `show_maintenance`) is on.
- The authenticated `_authed/servers/$id-page.tsx`, `_authed/network/*`, and
  `_authed/ip-quality.tsx` page components — refactored to extract a
  presentation-only content component plus a thin authenticated wrapper. The
  public layout mounts the same content component.
- `IpQualityCard`, `UnlockMatrix`, `UptimeTimeline`, `MetricsChart`,
  `TrafficCard`, `DiskIoChart`, anomaly table, traceroute view, latency chart
  — all reused.

### Replaced

- `crates/server/src/router/api/public_status_page.rs` — replaced by a new
  `crates/server/src/router/api/status.rs` module that hosts the full public
  surface (config, servers list, server detail, server metrics, network
  overview, network per-server, IP quality, incidents, maintenances).
- `apps/web/src/routes/status.tsx`, `status.index.tsx`, `status.$slug.tsx`,
  and `status-slug.test.tsx` — replaced by a new layout plus per-sub-page
  routes (see "Frontend routing").
- `apps/web/src/routes/_authed/settings/status-pages.tsx` — rewritten as a
  single-form admin configuration screen.

### Removed

- Multi-slug routing on the public side.
- `status_page.slug`, `status_page.theme_ref`, `status_page.custom_css`,
  `status_page.show_values` columns.
- `incidents.status_page_ids_json`, `maintenances.status_page_ids_json`
  columns (replaced by `is_public`).

### Naming note

The actual SQLite table is `status_page` (singular) per
`crates/server/src/entity/status_page.rs`. The migration and service code
must use that exact name; the column-list references in this spec follow
the same convention.

## Architecture

### Defense-in-depth: redaction at the API boundary

The `/api/status/*` endpoints are designed for anonymous use, but the
existing SPA `api` client always sends credentials (`credentials: 'include'`
in `apps/web/src/lib/api-client.ts`). The redaction policy therefore must be
**unconditional**: handlers ignore any session cookie / API key / agent
token entirely when building the response. The existing
`status_page.rs:274` pattern that unmasks egress IP for authenticated
viewers is removed; on the public surface, an admin in the same browser
sees exactly what a stranger sees.

Each endpoint defines a dedicated `PublicXxxDto` struct that contains only
the fields safe to expose. Handlers explicitly `map` entity rows into the
DTO; sensitive fields are not present on the DTO at all, so accidental
serialization is impossible. We do not use `#[serde(skip_serializing_if = …)]`
patterns for redaction — runtime-conditional skipping is too easy to
bypass during a refactor.

Sensitive fields stripped in DTOs:

- **Server identity**: `ipv4`, `ipv6`, agent-detected public IP, full
  interface list (names + per-interface IPs + MACs).
- **IP quality snapshot** (full strip; field names per
  `crates/common/src/protocol.rs:148`): `ip`, `asn`, `as_org`, `region`,
  `city`, `is_proxy`, `is_vpn`, `is_hosting`, `is_tor`, `is_abuser`,
  `is_mobile`, `asn_abuser_score`, `abuse_email`.
- **Unlock result detail string** (`UnlockResultDto.detail`,
  `crates/server/src/service/ip_quality.rs:58`): free-form error / probe
  messages from custom service definitions may leak identifying strings;
  the public DTO drops this field entirely. The unlock status, region
  hint, and latency are retained.

Retained (per user decision, 2026-05-26):

- `hostname`, `kernel_version`, `agent_version`, `cpu_name`, `cpu_arch`,
  `os`, mountpoint paths, per-disk names, GPU model, temperature sensor
  names, process / TCP / UDP counts.
- Network-probe `target` IPs and traceroute hop IPs (admin-configured probe
  topology; treated as public information).
- IP-quality coarse fields that classify without identifying the egress:
  `country`, `ip_type` (residential / datacenter / mobile), `risk_score`,
  `risk_level`, and per-service unlock status / region / latency.

### Public server scope

A response from any `/api/status/*` endpoint must only refer to servers
that admin has explicitly selected in `status_page.server_ids_json` and
that still exist (deleted-server stragglers stripped, mirroring the
existing `status_page.rs:286` "scoped_ids" pattern).

- `GET /api/status` — only servers in the scope set.
- `GET /api/status/servers/:id` and `:id/metrics` and `:id/uptime-daily`
  — `404 Not Found` if `id` is not in scope (intentionally indistinguishable
  from "server does not exist", to avoid confirming existence of hidden
  servers).
- `GET /api/status/network` and `/api/status/network/:id` — same scope
  filter; per-server detail returns `404` for out-of-scope `id`.
- `GET /api/status/ip-quality` — only entries for in-scope server IDs.

Tests assert that requesting an out-of-scope or hidden server ID yields
`404` and that the response body contains no metadata about that server.

### Sub-page gating

The admin config has independent boolean toggles for each sub-page. When a
toggle is off:

- The header nav hides the corresponding link.
- The route still exists in the SPA but renders a 404-style "not enabled"
  placeholder.
- The backing API endpoint returns `403 Disabled` so anonymous probes cannot
  exfiltrate data via a direct URL once an admin disables a section.

The single VPS-detail toggle (`show_server_detail`) controls whether server
cards / rows are clickable on `/status` and whether `/status/server/:id`
serves content.

## Backend

### Routes

All registered under `crates/server/src/router/api/status.rs`, all anonymous,
all read-only.

| Method | Path | Purpose | Gated by |
|---|---|---|---|
| GET | `/api/status/config` | Public config: title, description, default_layout, sub-page toggles, uptime thresholds | always available (returns `enabled=false` payload if admin disabled the page) |
| GET | `/api/status` | Server list with metrics summary + uptime% + status | `enabled` |
| GET | `/api/status/servers/:id` | Per-server detail (info + cost + traffic + caps overview) | `enabled` + `show_server_detail` |
| GET | `/api/status/servers/:id/metrics` | Historical metric series (CPU / memory / disk / network / load / disk-io / GPU / temperature) | `enabled` + `show_server_detail` |
| GET | `/api/status/servers/:id/uptime-daily` | 90-day uptime entries | `enabled` |
| GET | `/api/status/network` | Network overview (per-server averages + target summaries) | `enabled` + `show_network` |
| GET | `/api/status/network/:id` | Per-server network detail (targets, latency history, anomalies, traceroute records) | `enabled` + `show_network` |
| GET | `/api/status/ip-quality` | IP-quality overview + enabled services catalog | `enabled` + `show_ip_quality` |
| GET | `/api/status/incidents` | Active + recent incidents | `enabled` + `show_incidents` |
| GET | `/api/status/maintenances` | Planned maintenances | `enabled` + `show_maintenance` |

A disabled section returns `403 Disabled` with a stable error body; the SPA
treats this identically to "feature not configured".

### Service layer

`crates/server/src/service/public_status.rs` (new) owns all queries and DTO
mapping. Handlers stay thin (parse params → call service → wrap
`ApiResponse`). Each DTO type:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PublicServerSummary {
    pub id: String,
    pub name: String,
    pub group_name: Option<String>,
    pub region: Option<String>,
    pub country_code: Option<String>,
    pub online: bool,
    pub in_maintenance: bool,
    pub public_remark: Option<String>,
    pub metrics: Option<PublicMetricsSummary>,
    pub uptime_percent: Option<f64>,
    pub uptime_daily: Vec<UptimeDailyEntry>,
    // No ipv4, ipv6, hostname-from-interface, interface_list, public_ip
}
```

Analogous `PublicServerDetail`, `PublicNetworkOverview`,
`PublicNetworkServerDetail`, `PublicIpQualityEntry`,
`PublicIpQualityServiceMeta`, `PublicIncident`, `PublicMaintenance`,
`PublicStatusConfig` types are defined in the same module.

### Rate limiting

`/api/status/*` shares a new `public_rate_limit: DashMap<IpAddr, Window>` on
`AppState`, scoped per IP. Default budget: 60 requests / 60 s window. Burst
handling matches the existing login rate limiter pattern. The intent is
DDoS / scraping protection, not abuse detection.

### OpenAPI

Every handler annotated with `#[utoipa::path]`, every DTO with
`#[derive(ToSchema)]`. The Swagger UI surface (`/swagger-ui/`) gains a
`Public Status` tag.

### Database migration

A single new migration file
`crates/server/src/migration/mNNNN_simplify_status_page.rs`:

1. **Resolve the surviving `status_page` row.** Prefer the most-recently-
   updated `enabled = true` row; fall back to the most-recently-updated row
   if none is enabled; if the table is empty, insert a default row with
   `enabled = false` and default toggles. Capture its `id` as
   `surviving_id`.
2. **Drop blocking constraints / triggers before column changes.** SQLite's
   ALTER TABLE for column drops requires no dependent objects on those
   columns:
   - Drop unique index `idx_status_page_slug_unique`
     (`m20260320_000009_status_page.rs:65`).
   - Drop the `trg_custom_theme_status_page_insert_ref_exists`,
     `trg_custom_theme_status_page_update_ref_exists`, and any sibling
     triggers from `m20260430_000021_custom_theme_ref_integrity.rs` that
     reference `theme_ref`.
3. **Extend `status_page`** with new columns:
   - `default_layout` TEXT NOT NULL DEFAULT `'grid'`
   - `show_server_detail` BOOLEAN NOT NULL DEFAULT `true`
   - `show_network` BOOLEAN NOT NULL DEFAULT `false`
   - `show_incidents` BOOLEAN NOT NULL DEFAULT `true`
   - `show_maintenance` BOOLEAN NOT NULL DEFAULT `true`
4. **Extend `incidents` and `maintenances`** with
   `is_public BOOLEAN NOT NULL DEFAULT false`.
5. **Backfill `is_public`** preserving the existing public-visibility
   semantics (per `status_page.rs:223`/`:264`): a row was visible on a
   public slug page if its `status_page_ids_json` was `NULL`, empty `[]`,
   or contained that page's id. After collapsing to a single page, mark
   public:
   ```sql
   UPDATE incidents
   SET is_public = 1
   WHERE status_page_ids_json IS NULL
      OR status_page_ids_json = '[]'
      OR status_page_ids_json LIKE '%"' || :surviving_id || '"%';
   -- same for maintenances
   ```
   The `LIKE` is a pragmatic substring check; the JSON value is always a
   simple array of quoted ids in the existing serializer. (The
   alternative — parsing JSON per row in SQL — is brittle in SQLite.)
   Rows referencing only deleted slug pages remain `is_public = false`.
6. **Drop legacy columns** in order: `incidents.status_page_ids_json`,
   `maintenances.status_page_ids_json`, then on `status_page`: `slug`,
   `theme_ref`, `custom_css`, `show_values`.
7. **Delete non-surviving `status_page` rows** (`WHERE id != surviving_id`).
8. **Singleton invariant** is enforced at the service layer: reads always
   select first row by `ORDER BY created_at LIMIT 1`, writes target the
   singleton row's id. SQLite does not easily enforce a row-count CHECK.

The migration is `up()`-only per project convention.

### Removed/changed back-end files

- `crates/server/src/router/api/public_status_page.rs` → deleted.
- `crates/server/src/router/api/status_page.rs` (admin CRUD, if a separate
  file) → rewritten as a single `GET` / `PUT` pair on the singleton config.
- Service / entity references to `slug`, `theme_ref`, `custom_css`,
  `show_values` removed.

## Frontend

### Routing

```
routes/
  status.tsx                          ← public layout (header + Outlet)
  status.index.tsx                    ← /status — VPS list (list or grid)
  status.server.$serverId.tsx         ← /status/server/:id — VPS detail
  status.network.tsx                  ← layout for /status/network/*
  status.network.index.tsx            ← /status/network — overview
  status.network.$serverId.tsx        ← /status/network/:id — per-server detail
  status.ip-quality.tsx               ← /status/ip-quality
```

The status layout is a sibling of `_authed`, has no auth requirement, mounts
its own header (`<StatusHeader />`), and renders `<Outlet />`. There is no
sidebar. The layout fetches `/api/status/config` once and provides the
result via context so children can render conditionally.

### Header

`<StatusHeader />` includes:

- Logo + admin-configured title (links back to `/status`).
- Sub-page navigation (Servers / Network / IP Quality) — items hidden when
  the corresponding toggle is off in config.
- Language switch (zh / en).
- Theme switch (light / dark / system) — reuses the existing
  `<ThemeToggle />` component.
- Admin login link → `/login`.

### List vs. Grid

A user-facing toggle (segmented control) lives at the top-right of the
servers index. Selected mode persists in `localStorage` under
`serverbee.status.layout`. The initial value falls back to the admin's
`default_layout`.

- **Grid**: existing `ServerStatusCard`-style card, three columns on `lg+`,
  shows progress bars and rolling metric summaries.
- **List**: a table-row variant adds a 90-day `UptimeTimeline` column.

### Public variants of reused screens

Existing `_authed/servers/$id-page.tsx`, `_authed/network/index.tsx`,
`_authed/network/$serverId.tsx`, `_authed/ip-quality.tsx` are refactored:

1. Extract a presentation-only `<ServerDetailContent>`,
   `<NetworkOverviewContent>`, `<NetworkServerDetailContent>`,
   `<IpQualityContent>` from each page.
2. The authenticated route keeps its current wrapper (sidebar + breadcrumbs +
   action buttons) and mounts the content component.
3. The public route mounts the same content component with a `variant="public"`
   prop. Public variant:
   - Hides admin action buttons (Edit, Recover, Capabilities, Terminal,
     Files, Docker, anomaly delete, target edit, agent upgrade).
   - Hides metadata rows whose data is `null` after backend redaction (IP
     rows, interface names, etc.).
4. `<IpQualityCard variant="public">` renders only `country`, `ip_type`,
   `risk_score`, `risk_level`, and the per-service unlock summary. The
   public DTO does not contain `ip`, `asn`, `as_org`, region/city,
   `is_proxy` / `is_vpn` / `is_hosting` / `is_tor` / `is_abuser` /
   `is_mobile`, `asn_abuser_score`, or `abuse_email`, so those rows never
   render even if a logged-in admin views `/status` in the same browser.

The content components live in `apps/web/src/components/status-content/`
or in the existing feature folders alongside their authenticated wrappers —
final placement decided during implementation, but the variant prop pattern
is fixed.

### Data fetching

TanStack Query, one query key per endpoint, `refetchInterval: 30_000` for
servers/network/IP quality, `refetchInterval: 5 * 60_000` for config. No
WebSocket. The shared `api` client (`apps/web/src/lib/api-client.ts`) sends
`credentials: 'include'` on every request, so an admin logged into the
same browser will still send a session cookie when visiting `/status`.
Backend redaction is unconditional and ignores auth state (see
"Defense-in-depth"), so this is not a leak vector — the admin sees the
same DTO as a logged-out visitor.

### Removed/changed frontend files

- `routes/status.$slug.tsx`, `routes/status-slug.test.tsx` → deleted.
- `routes/status.tsx`, `routes/status.index.tsx` → rewritten.
- `routes/_authed/settings/status-pages.tsx`,
  `routes/_authed/settings/status-pages.test.tsx` → rewritten as
  single-form config screen.
- `apps/web/src/lib/api-schema.ts`: replace `PublicStatusPageData` and
  `StatusPageResponse` with the new public DTO types; remove
  `StatusPageItem`'s `slug`, `theme_ref`, `show_values` fields.

## Admin Settings UI

`settings/status-pages` becomes a single form (no list, no create/edit
dialogs). Fields:

- `enabled` (master switch)
- `title`
- `description`
- `server_ids[]` (existing multi-select picker)
- `default_layout` (`list` | `grid`)
- `show_server_detail`
- `show_network`
- `show_ip_quality`
- `show_incidents`
- `show_maintenance`
- `uptime_yellow_threshold`
- `uptime_red_threshold`

Saving issues `PUT /api/status-pages` (admin endpoint, distinct from public
endpoints). The page also embeds the existing incidents and maintenance
management widgets — they remain accessible regardless of the public
display toggles (the toggles only hide from the public site, not from
admins).

## Testing

### Rust integration tests (`crates/server/tests/`)

The redaction tests walk the full JSON-key tree (not a string contains
check) and assert the listed keys are absent.

- `public_status_redaction.rs`:
  - `GET /api/status` and `GET /api/status/servers/:id` response JSON must
    not contain any of: `ipv4`, `ipv6`, `interfaces`, `public_ip`,
    `mac_address`, or nested `network_interface` arrays.
- `public_status_ip_quality_redaction.rs`:
  - `GET /api/status/ip-quality` must not contain any of: `ip`, `asn`,
    `as_org`, `region`, `city`, `is_proxy`, `is_vpn`, `is_hosting`,
    `is_tor`, `is_abuser`, `is_mobile`, `asn_abuser_score`, `abuse_email`.
  - Each entry under `unlock_results` must not contain the `detail` key.
- `public_status_redaction_authenticated.rs`:
  - Same assertions as the two tests above, but the requests carry a
    valid admin session cookie **and** a valid admin API key. Redaction
    must be identical to the anonymous case — no field unmasked for
    authenticated callers.
- `public_status_scope.rs`:
  - Configure `server_ids = [A]`; request `/api/status/servers/B` →
    `404`; `/api/status/network/B` → `404`; `/api/status` and
    `/api/status/network` and `/api/status/ip-quality` responses must
    not reference `B` in any field.
- `public_status_gating.rs`:
  - Disabling each toggle independently causes the corresponding endpoint
    to return `403 Disabled`. Test matrix: `show_server_detail`,
    `show_network`, `show_ip_quality`, `show_incidents`,
    `show_maintenance`.
- `public_status_anonymous.rs`:
  - All endpoints return 200 without any auth header, with `enabled=true`
    config and at least one server.

### Frontend (vitest)

- `status.layout.test.tsx`: header renders only enabled sub-page links.
- `status.layout-toggle.test.tsx`: list/grid toggle persists to
  `localStorage`; initial value falls back to admin default.
- `status.public-variants.test.tsx`: public variant of server detail does
  not render action buttons.

### Manual checklist

A new `tests/status-page/` checklist covering anonymous browsing flows,
toggle behavior, and visual regressions across light/dark + zh/en.

## Migration & Rollout

- Single migration runs on startup (project convention).
- No feature flag; the change is a refactor of an existing surface.
- Frontend deploys atomically with the backend (single binary embeds the
  SPA via `rust-embed`).
- **Rollback is database-restore only.** Reverting the binary alone after
  this migration runs will fail at startup because the prior version's
  `status_page` / `incident` / `maintenance` entity structs reference
  columns (`slug`, `theme_ref`, `custom_css`, `show_values`,
  `status_page_ids_json`) that no longer exist. To roll back, restore from
  a pre-deploy SQLite backup; do not attempt to revert the binary against
  the migrated database.

## Open Questions

### Incidents / maintenances linked only to deleted slug pages

Rows whose `status_page_ids_json` referenced one or more **non-surviving**
slug pages — and never the surviving page, never NULL, never empty — are
backfilled as `is_public = false` (private). The reasoning: those rows
were intentionally scoped to a specific customer / audience page that no
longer exists; promoting them to the global public surface would surprise
admins. They remain editable in the admin UI, where an operator can flip
`is_public` if they decide the announcement should now be globally visible.

This matches the spec's backfill rules in step 5 of the migration. The
decision is recorded here because the alternative ("promote all old
linkage to public") would be a one-way data event that's hard to undo.
Flagged for confirmation during spec review.
