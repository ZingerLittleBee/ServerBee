# Servers Table Row Visual Redesign

**Date:** 2026-04-17
**Scope:** `apps/web/src/routes/_authed/servers` (table view rows), plus a small backend additive surface for `server_tags` and `cpu_cores`.

## Problem

The `/servers?view=table` rows are text-heavy. Each metric cell is a thin (1.5px) progress bar plus a single sub-line of text; the `status` column renders a text badge; the `name` column shows only the server name with an optional flag; there is no surface for user-defined labels. Disk I/O got added in an earlier pass (`2026-04-15-servers-table-density-design.md`) as an extra text line on the Disk cell, but the overall "every cell is a thin bar + one-line sub-text" rhythm remains flat and busy.

Two user needs drive this redesign:

1. **Iconography over text.** The page should lean on lucide icons and progress bars to convey metrics at a glance rather than relying on label text.
2. **Traffic quota visibility.** The `Network` column currently shows only per-second speeds and cumulative transfers. Users want to see current-cycle traffic against the configured monthly cap, the same signal that the grid-view `ServerCard` already renders as a ring (`93.2 GB / 1.0 TB`). It must also exist in the table.
3. **User-defined tags.** The `server_tags` table exists in the database (`crates/server/src/entity/server_tag.rs`) but is never read by the API, never pushed by the WebSocket, and never editable in the UI. Users want short tags (e.g., `prod`, `db-primary`, `asia`) displayed under the server name and editable from the server edit dialog.

## Goals

1. Every metric cell (`CPU`, `Memory`, `Disk`, `Network`) becomes a consistent two-line cell: **line 1 = lucide icon + progress bar + percentage**, **line 2 = monospace sub-line with 1–3 datapoints**.
2. The `status` column text badge collapses into a small pulsing dot in a new first column (36px wide). Total column count is unchanged: `status-dot · name · cpu · memory · disk · network · group · uptime · actions` (we drop the existing dedicated `status` column, because its signal is now in the dot).
3. `Name` cell becomes two lines: flag + name on top, colored tag chips below. Rows with no tags render single-line naturally.
4. `Network` cell's top line becomes a **traffic-quota** progress bar (cycle bytes / `traffic_limit`), matching the grid card's `useTrafficOverview` data, falling back to a 1 TiB default when no quota is configured. The bottom line carries `{used} / {limit} · ↓ {in_speed} ↑ {out_speed}`.
5. `Uptime` cell gets a sub-line: online rows show the OS with its emoji (`🐧 Ubuntu 22.04`), offline rows show `last seen 2h ago` (relative time from `last_active`).
6. `server_tags` becomes an end-to-end feature: REST read/write, WS payload inclusion, and an editor in `ServerEditDialog`.
7. Color thresholds are unchanged (`getBarColor` in `index.cells.tsx`: <70% green, 70–90% amber, >90% red).

## Non-goals

- Grid view (`ServerCard`) — untouched.
- Sparkline / mini-charts (explicitly rejected during brainstorming; no client-side history buffer is introduced).
- Ring-chart variant (considered; rejected in favor of the icon+bar direction).
- Realtime tag-change broadcasts to all browsers. Tag edits will manually invalidate the local `['servers']` query; live propagation across browsers is a follow-up (Phase C below).
- Changing sorting, filtering, pagination, or other `DataTable` behavior.
- Changing column sizes or the shadcn `<Table>` primitive.

## Design

### Visual rhythm (authoritative per-cell spec)

| Column | Width | Top line | Bottom line (sub) |
|---|---|---|---|
| `status-dot` (new) | 36px (`w-9`) | 8px pulsing dot: green `bg-emerald-500` with `box-shadow` halo + CSS `pulse` when online; muted grey `bg-muted-foreground/60` when offline | — |
| `name` | 260px | flag (if country_code) + server name (truncates, Link) + UpgradeBadge | `<TagChipRow tags>` — colored chips, wraps; absent when `tags` empty |
| `cpu` | 160px | `<Cpu />` 14px + bar + `%` (monospace, right-aligned, colored by threshold) | `{cores} cores · load {load1.toFixed(2)}` |
| `memory` | 160px | `<MemoryStick />` + bar + `%` | `{formatBytes(used)} / {formatBytes(total)} · swap {swapPct}%` |
| `disk` | 160px | `<HardDrive />` + bar + `%` | `<ArrowDown />{formatSpeed(read)} <ArrowUp />{formatSpeed(write)}` |
| `network` | 160px (stays `hidden lg:table-cell`) | `<Network />` + **traffic quota** bar + `%` | `{formatBytes(used)} / {formatBytes(limit)} · <ArrowDown />{formatSpeed(in)} <ArrowUp />{formatSpeed(out)}` |
| `group` | 140px (stays `hidden xl:table-cell`) | Group name (as today) | — |
| `uptime` | 100px (stays `hidden xl:table-cell`) | Online: `<Clock />` + `formatUptime(uptime)` · Offline: `offline` | Online: `{osEmoji} {os}` · Offline: `last seen {relative(last_active)}` |
| `actions` | 40px | edit button (unchanged) | — |

Offline rows render `—` for metric cells' top lines (as today) and render a subdued sub-line where applicable (e.g. `last seen 2h ago` on Uptime). Tag chips on `name` still show, independent of online status.

The `status` data column defined in today's `index.tsx` is removed (the signal moves to the pulsing dot in the new first column). Its filter (`status: online / offline`) migrates to the new `status-dot` column's `meta.options` so `DataTableToolbar` continues to offer that filter pill.

### Color & threshold rules

- `getBarColor(pct)` is reused verbatim for CPU / Memory / Disk usage and for Network traffic quota.
- Percentage text in the bar row adopts the same color (via `getBarColor` mapped to text class) so an 87% CPU shows red both on the bar and the number.
- Swap percentage in the Memory sub-line uses the same thresholds against `swap_total`.

### Component structure

```
apps/web/src/routes/_authed/servers/
  index.tsx              # column defs updated (status-dot first, status column dropped)
  index.cells.tsx        # REWRITTEN
    <StatusDot />          online|offline
    <NameCell />           flag + name + tags
    <TagChipRow />         server_tags as colored chips
    <MetricBarRow />       reusable: icon + bar + %
    <CpuCell />            <MetricBarRow/> + sub
    <MemoryCell />         <MetricBarRow/> + sub
    <DiskCell />           <MetricBarRow/> + sub
    <NetworkCell />        <MetricBarRow/>(traffic%) + sub
    <UptimeCell />         online vs offline branch
```

`MetricBarRow` is the new primitive; it takes `{ icon: ReactNode; pct: number; label?: string; valueClassName?: string }`. It does NOT render any sub-line — each metric cell composes `<MetricBarRow />` with its own `<div className="sub-line">...`.

The existing `MiniBar` component is retained for other callers (none today in `src/`, but it's exported) — we will leave it as a thin wrapper that calls `MetricBarRow` with an empty icon slot, for back-compat.

`TagChipRow` uses a deterministic palette: `tag.split('').reduce((h,c)=>h*31+c.charCodeAt(0),0) % N` to pick one of 6 muted colors (emerald, sky, amber, rose, violet, slate). Individual chips are truncated with `max-w-[80px]` plus `title={tag}`; the row allows wrap (`flex flex-wrap`).

### Data dependencies

**Already available on `ServerMetrics` WebSocket payload** — no backend change:

- `cpu`, `load1`
- `mem_used`, `mem_total`, `swap_used`, `swap_total`
- `disk_used`, `disk_total`, `disk_read_bytes_per_sec`, `disk_write_bytes_per_sec`
- `net_in_speed`, `net_out_speed`, `net_in_transfer`, `net_out_transfer`
- `uptime`, `last_active`, `os`, `country_code`

**Already available via a separate query** — reused, no backend change:

- `useTrafficOverview()` → `/api/traffic/overview` → `{ cycle_in, cycle_out, traffic_limit, days_remaining }` per server. The table view will call this query at the page level (once), then lookup per row. Fallback to `DEFAULT_TRAFFIC_LIMIT_BYTES = 1 TiB` when no quota is configured, identical to `ServerCard`.

**New on `ServerStatus` (backend work required)**:

- `tags: Vec<String>` — `#[serde(default)]` empty vec; added to `crates/common/src/types.rs::ServerStatus` and populated in `crates/server/src/router/ws/browser.rs::build_full_sync` (single query: `server_tag::Entity::find().all(&db)` grouped by `server_id` in memory).
- `cpu_cores: Option<i32>` — `#[serde(default)]`; populated from `servers.cpu_cores` column (already exists in the DB).

Both fields are static across most updates; they will be included on `full_sync` and on any `update` message where the underlying data changed. Because `STATIC_FIELDS` in `apps/web/src/hooks/use-servers-ws.ts` guards against overwriting static fields with `null`/`0`, we will add `cpu_cores` to that set. `tags` is an array and is not subject to the static-fields guard (an explicit `[]` from the server should overwrite local state).

### Backend: tags API

Two new endpoints in `crates/server/src/router/api/server_tags.rs` (new file), mounted under `/api`:

**`GET /api/servers/:id/tags`** → `ApiResponse<Vec<String>>`
- Auth: any authenticated user (reuses the default auth middleware — members can read, same as reading servers today).

**`PUT /api/servers/:id/tags`** body `{ tags: Vec<String> }` → `ApiResponse<Vec<String>>`
- Auth: `require_admin`.
- Replaces the tag set atomically inside a transaction: delete all rows for `server_id`, insert the new ones.
- Validation: `tags.len() <= 8`, each `tag.len() <= 16`, each tag matches `[A-Za-z0-9_.-]+` and is non-empty after trim. Duplicates are de-duplicated server-side (case-sensitive). Returns 400 with a `validation_error` on violation.
- Returns the canonical (sorted, deduped) tag list.

Both endpoints are annotated with `#[utoipa::path]` and include a `ToSchema`-derived DTO for the request body.

After a successful `PUT`, the server broadcasts no new WS event in Phase B; the frontend manually invalidates `['servers']` on success, which causes a refetch (there is no REST for the servers list — the list is WS-only). **However**, since no REST refetch exists, the table will only pick up the new tag when the next `update` or `full_sync` rolls in. To avoid the "I just edited the tags but my row hasn't updated" lag, the `PUT` response's `data: string[]` is used to optimistically patch `queryClient.setQueryData(['servers'], prev => prev.map(s => s.id === id ? { ...s, tags: data } : s))`.

### Frontend: tag editor in `ServerEditDialog`

A new block in the existing `ServerEditDialog` form:

- Label: `t('servers:tags_label')` ("Tags" / "标签")
- Input: shadcn `<Input />` with helper text `t('servers:tags_hint')` ("Comma or space separated, up to 8 tags, 16 chars each"). On blur, the string is split on `/[\s,]+/`, trimmed, deduped, and normalized against the same validation rules as the backend.
- Submit: a separate PUT to `/api/servers/:id/tags` fires on save (not bundled into the existing server-update PATCH). Success toast; optimistic cache update as above.
- Fetched on open via `useQuery(['server-tags', id])` → `GET /api/servers/:id/tags`. The initial form value is populated from this query.

### Phasing

**Phase A** (frontend-only, ships first):
- Rewrite `index.cells.tsx` and adjust `columns` in `index.tsx` (status-dot column, no Status text column, Network traffic quota bar, Uptime sub-line).
- Use `useTrafficOverview()` at the page level; pass per-row lookup to `NetworkCell`.
- Render tag chips when `server.tags?.length > 0`, otherwise single-line Name. Since backend does not yet push `tags`, the chip row is dormant.
- Add optional `cpu_cores?: number | null` and `tags?: string[]` to the `ServerMetrics` TS interface now, so Phase B plugs in without a second wave of type churn.

**Phase B** (backend + editor):
- Add `tags: Vec<String>` and `cpu_cores: Option<i32>` to `ServerStatus` and `build_full_sync` in `crates/server/src/router/ws/browser.rs`.
- Add `server_tags` REST endpoints (`GET` / `PUT`).
- Add tag editor in `ServerEditDialog` with optimistic cache update.
- Add `cpu_cores` to the frontend `STATIC_FIELDS` guard.
- Swagger/OpenAPI auto-updates via utoipa annotations.

**Phase C** (optional, follow-up spec if desired): broadcast a `tags_changed` WS event so all connected browsers see tag edits live. Not in scope for this spec.

Phase A and Phase B may be shipped together in a single PR if convenient; they are separated here only to clarify which change depends on which.

### i18n keys (new)

Added to `apps/web/public/locales/{en,zh}/servers.json`:

- `tags_label` — "Tags" / "标签"
- `tags_hint` — editor helper text
- `tags_placeholder` — input placeholder `prod, db, web`
- `tags_validation_too_many` — "At most 8 tags" / "最多 8 个标签"
- `tags_validation_too_long` — "Each tag must be ≤16 chars" / "单个标签最多 16 字符"
- `tags_validation_invalid_char` — "Only letters, digits, and `._-` allowed"
- `last_seen_ago` — "last seen {{time}}" / "最后上线 {{time}}"

Existing keys reused: `card_load`, `col_cpu`, `col_memory`, `col_disk`, `col_network`, `col_uptime`, `status_online`, `status_offline`.

## Testing

### Rust (Phase B)

`crates/server/tests/` integration coverage:

- `server_tags_crud` — PUT then GET returns same list; dedup + trim; 400 on too many / too long / invalid chars; RBAC (member GET 200, member PUT 403).
- `full_sync_includes_tags` — seed two servers each with two tags, open the browser WS, assert the first `full_sync` frame contains `tags: ["a","b"]` for each.

No new unit test for `build_full_sync` shape beyond the integration test; existing WS tests cover the rest.

### Frontend (vitest)

`apps/web/src/routes/_authed/servers/__tests__/`:

- `cells.test.tsx`
  - `MetricBarRow`: color threshold at 69/70/89/90/91; custom icon slot renders; `%` rounds to 0 decimals.
  - `CpuCell`: renders `{cores} cores · load {1.23}` with `cpu_cores=8, load1=1.234`; hides sub when offline.
  - `MemoryCell`: renders `7.2 GB / 16 GB · swap 3%`; swap color follows threshold.
  - `DiskCell`: renders read/write arrow row; hides I/O sub when offline (same as today's rule).
  - `NetworkCell`: uses `trafficEntry.traffic_limit` when present; falls back to 1 TiB default when null; clamps pct to 100.
  - `UptimeCell`: online shows OS emoji + name; offline shows `last seen 2h ago` derived from `last_active` 2h in the past.
  - `NameCell`: 0 tags → single line, no tag row rendered; 3 tags → chips wrap; long tag truncates with `title` attr.
  - `TagChipRow`: same tag → same palette color (hash stability).
- `index.test.tsx` already exists for `/servers`; extend the "renders online/offline rows" block to assert the pulsing dot and no text badge column.

### Manual QA checklist

New file `tests/servers/table-row-visual-redesign.md`:

1. Open `/servers?view=table` with mixed online/offline rows; verify pulsing dot vs grey dot.
2. Add tags via `ServerEditDialog`, save, verify chips appear immediately (optimistic), persist after reload.
3. Verify tag validation: 9 tags / 17-char tag / tag with spaces → form-level error.
4. Configure a server with `traffic_limit = 1GB`, push it past 50%/80%/95% usage in fixture data; verify Network bar color transitions and `%` color.
5. Configure a server without `traffic_limit`; verify Network bar renders against 1 TiB fallback (stays small).
6. Take a server offline; verify row dims, metric cells show `—`, Uptime sub shows `last seen … ago`, tags still visible.
7. Resize viewport: network column hides below `lg:` (1024px); group + uptime hide below `xl:` (1280px). No horizontal scroll bleed at any breakpoint.
8. Verify ultracite + typecheck + `cargo clippy` all pass.

## Rollout

1. Phase A PR — frontend rewrite. No migration, no backend changes. User-visible change: table looks different; tags row is empty.
2. Phase B PR — `cpu_cores` + `tags` on WS, REST endpoints, editor in `ServerEditDialog`. `cpu_cores` retro-populates from existing DB column; `tags` defaults to empty for all servers.
3. Documentation update: add a short section to `apps/docs/content/docs/{en,cn}/*` about tags if user requests it (not bundled by default — CLAUDE.md only mandates docs updates for env var changes).

No schema migration required: `server_tags` and `cpu_cores` already exist in the database.

## Open questions

None at spec-approval time. All resolved during brainstorming:

- Visual direction: C (icon + bar), not rings or sparklines.
- Disk I/O placement: C1 (inline under Disk bar), not a separate column.
- Sub-line data for CPU: `{cores} cores · load {load1}`.
- Sub-line data for Memory: `{used}/{total} · swap {pct}%`.
- Name sub-line: `server_tags` (not `public_remark`, not group name).
- Uptime sub-line: OS line for online, `last seen` for offline.
