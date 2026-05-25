# Custom SPA Themes Design

**Status:** Draft
**Date:** 2026-05-26
**Branch:** `victoria`
**Related:** existing color-theme system in `apps/web/src/themes/`, brand settings at `crates/server/src/router/api/brand.rs`

## Goal

Let administrators replace the entire ServerBee frontend (login page, dashboards, settings, every route) with a custom SPA they upload as a `.sbtheme` package. The default React SPA becomes one of many possible frontends. The HTTP/WebSocket API, authentication, and Rust backend are unchanged — the theme is pure frontend that talks to the existing API.

This is the "WordPress theme" model adapted to a React + Rust monitoring tool: full visual freedom, admin-only upload, browser-sandboxed execution, CSP-enforced data-exfiltration prevention.

## Non-goals (v1)

- Marketplace / community theme registry (file upload only)
- Server-side code in themes (WASM, JS sandbox, etc.) — themes are pure HTML/JS/CSS
- Slot-based plugin extension points (custom widgets, sidebar items) — whole-SPA replacement only
- Per-user theme preference — one active theme system-wide
- Hot-reload during theme development (rely on `bun run pack` cycle)
- Theme signing / code review workflow
- Auto-upgrade on new version upload (admin must explicitly activate)
- Theme that ships its own backend assets, Rust crates, or migrations

These constraints keep v1 implementable in one development cycle and confine the security model to "admin trusts themselves; CSP contains blast radius."

---

## 1. Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                            SERVER                                │
│                                                                  │
│  spa_themes table (BLOB storage, source of truth)               │
│       ▲                                                          │
│       │ upload / activate / delete (admin REST)                  │
│       │                                                          │
│  ┌────┴────────────────┐   on activate    ┌─────────────────┐  │
│  │ SpaThemeService     │ ──────────────►  │ ArcSwap<Option> │  │
│  │ - validate manifest │   atomic swap    │ <LoadedTheme>>  │  │
│  │ - validate zip      │                  └────────┬────────┘  │
│  │ - extract to mem    │                           │           │
│  └─────────────────────┘                           │ read      │
│                                                    ▼           │
│         ┌──────────────────────────────────────────────────┐   │
│         │  Axum route handler (catch-all `/*`)             │   │
│         │  if cookie sb_force_default || query theme=def   │   │
│         │     → rust-embed default SPA                      │   │
│         │  else if active_theme = Some(t)                   │   │
│         │     → serve t.files[path] with CSP headers       │   │
│         │  else                                             │   │
│         │     → rust-embed default SPA                      │   │
│         └──────────────────────────────────────────────────┘   │
│                                                                  │
│  /api/*, /api/ws/*, /swagger-ui/*, /__system/* unchanged        │
│  (themes cannot shadow these paths)                              │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                            BROWSER                               │
│                                                                  │
│  Active theme HTML/JS/CSS runs same-origin with ServerBee.       │
│  Auth via existing session cookie (Set-Cookie from /api/auth).  │
│  CSP restricts connect-src to 'self' — no exfiltration possible. │
└─────────────────────────────────────────────────────────────────┘
```

- Default SPA continues to ship via `rust-embed` (compile-time, zero runtime cost).
- Custom themes live in SQLite as zip BLOBs (source of truth), extracted into an in-memory `HashMap<String, Bytes>` on activation.
- `ArcSwap<Option<LoadedTheme>>` gives readers lock-free access on the hot serve path.
- The "active theme" pointer is a value in the existing `ConfigService` KV store under key `ACTIVE_SPA_THEME_KEY` (Section 2.2). Absent / empty = serve the built-in default SPA.
- Color themes and brand settings (existing) are orthogonal and untouched. When a custom SPA theme is active, the existing color/brand controls have no effect (the theme owns its own colors and branding). The UI hides them with an explanatory note.

---

## 2. Data Model

### 2.1 New table `spa_themes`

| Column | Type | Notes |
|---|---|---|
| `id` | INTEGER PRIMARY KEY AUTOINCREMENT | Internal numeric ID |
| `uuid` | TEXT NOT NULL UNIQUE | UUIDv7, used in URLs / API |
| `manifest_id` | TEXT NOT NULL | `manifest.id` from package; multiple rows can share this (version history) |
| `name` | TEXT NOT NULL | From `manifest.name`, HTML-stripped, ≤ 64 chars |
| `version` | TEXT NOT NULL | semver from `manifest.version` |
| `author` | TEXT NULL | From `manifest.author`, ≤ 64 chars |
| `description` | TEXT NULL | From `manifest.description`, ≤ 500 chars |
| `manifest_json` | TEXT NOT NULL | Full sanitized manifest, JSON-encoded |
| `package_data` | BLOB NOT NULL | Original `.zip` bytes (source of truth) |
| `preview_data` | BLOB NULL | Extracted preview image (≤ 500KB) |
| `preview_mime` | TEXT NULL | e.g. `image/png` |
| `size_bytes` | INTEGER NOT NULL | Package size (uncompressed total of allowed files) |
| `uploaded_by` | INTEGER NOT NULL | FK → `users.id` |
| `uploaded_at` | TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP |
| `is_superseded` | INTEGER NOT NULL DEFAULT 0 | 1 when a newer version of the same `manifest_id` was uploaded |

Indexes:
- `UNIQUE(uuid)`
- `INDEX(manifest_id, version)` for upgrade lookup
- `INDEX(uploaded_at DESC)` for listing

### 2.2 Active-theme pointer

Stored via the existing `ConfigService` KV store (used today for `ACTIVE_THEME_KEY` color theme). New key:

- `ACTIVE_SPA_THEME_KEY = "active_spa_theme_uuid"` → value is the `spa_themes.uuid` string, or absent/empty when no custom SPA theme is active.

Storing the UUID (not the numeric id) keeps the value stable across re-imports and matches what the API surface accepts.

When a row is deleted, the service layer deactivates first (clears the config key) — no foreign key, no cascade. If the stored uuid is dangling (row deleted out of band), the runtime falls back to the default SPA and logs a warning, mirroring `CustomThemeService::active_theme` behavior.

### 2.3 Audit log

Reuse the existing `audit_logs` table (entity: `crates/server/src/entity/audit_log.rs`). Schema is `id, user_id, action, detail, ip, created_at`. SPA theme events:

| `action` | `detail` (JSON in the text column) |
|---|---|
| `spa_theme.upload` | `{"uuid":"...", "manifest_id":"...", "version":"...", "size_bytes":...}` |
| `spa_theme.activate` | `{"uuid":"..."}` |
| `spa_theme.deactivate` | `{"uuid":"..."}` (previous active) |
| `spa_theme.delete` | `{"uuid":"...", "manifest_id":"...", "version":"..."}` |

No schema change to `audit_logs`.

---

## 3. Package Format (`.sbtheme`)

A `.sbtheme` file is a standard zip with a fixed layout:

```
acme-dashboard.sbtheme
├── manifest.json       (required)
├── index.html          (required, default entry; path can be overridden in manifest)
├── assets/             (optional; conventional location for built JS/CSS/images)
├── preview.png         (optional; shown on theme card)
└── README.md           (optional; not served)
```

### 3.1 `manifest.json` schema

```json
{
  "schema_version": 1,
  "id": "acme-dashboard",
  "name": "Acme Corp Dashboard",
  "version": "1.2.0",
  "author": "Acme Inc",
  "homepage": "https://acme.example.com",
  "description": "Branded dashboard for Acme.",
  "entry": "index.html",
  "min_serverbee_version": "1.0.0",
  "preview": "preview.png"
}
```

| Field | Required | Constraint |
|---|---|---|
| `schema_version` | yes | Integer; v1 accepts only `1` |
| `id` | yes | Matches `^[a-z][a-z0-9-]{2,63}$` |
| `name` | yes | 1–64 chars; HTML tags stripped |
| `version` | yes | Valid semver |
| `author` | no | ≤ 64 chars; HTML tags stripped |
| `homepage` | no | Valid `http(s)://` URL |
| `description` | no | ≤ 500 chars; HTML tags stripped |
| `entry` | no | Defaults to `index.html`; must exist in package and end with `.html` |
| `min_serverbee_version` | no | semver; must be ≤ running server version |
| `preview` | no | Relative path; must exist; ≤ 500KB; one of `png/jpg/webp` |

### 3.2 Package constraints

- Total uncompressed size ≤ **20 MB**
- File count ≤ **1000**
- Per-file size ≤ **5 MB**
- Per-file path length ≤ 255 chars
- Allowed extensions (case-insensitive): `html, htm, js, mjs, css, png, jpg, jpeg, svg, webp, gif, ico, woff, woff2, ttf, otf, json, txt, map`
- Forbidden: directory traversal (`..`), absolute paths, Windows drive letters, symlink zip entries, duplicate entries, zero-byte `manifest.json`
- Compression ratio guard: any single entry whose decompressed size exceeds compressed size by > 100× is rejected (zip bomb defense)
- Multipart upload hard cap on the HTTP layer: **25 MB** (leaves headroom over the 20 MB content cap for zip overhead)

---

## 4. Upload & Validation Pipeline

```
POST /api/settings/spa-themes (multipart/form-data, field: package)
    │
    ├─ require_admin middleware
    ├─ content-length pre-check (≤ 25 MB)
    │
    ▼
1. Read multipart body into Vec<u8> (bounded)
2. Open as zip (zip crate)
3. Iterate entries:
   - Reject ../, abs paths, drive letters, symlinks, duplicates
   - Reject disallowed extensions
   - Enforce per-file size cap, running total cap, count cap, ratio cap
   - Read manifest.json if present (must be first or random-access)
4. Parse manifest.json → ThemeManifest
5. Validate manifest fields (regex / semver / lengths / entry exists / min_version)
6. Sanitize text fields (HTML-strip)
7. Lookup existing rows with same manifest_id:
   - If newest existing version > uploaded version → 400 NoDowngrade
   - If newest existing version == uploaded version → 409 VersionExists
   - Otherwise mark existing rows is_superseded = 1
8. Insert new spa_themes row (transactional)
9. Audit-log spa_theme.upload
10. Return { uuid, manifest, size_bytes, preview_url, is_upgrade_of }
```

### 4.1 Error contract

All failures return:

```json
{
  "error": {
    "code": "INVALID_MANIFEST" | "ZIP_SLIP" | "FILE_TOO_LARGE" | "TOO_MANY_FILES" |
            "TOTAL_SIZE_EXCEEDED" | "ZIP_BOMB" | "SYMLINK_NOT_ALLOWED" |
            "DUPLICATE_ENTRY" | "DISALLOWED_EXTENSION" | "MISSING_MANIFEST" |
            "MISSING_ENTRY" | "NO_DOWNGRADE" | "VERSION_EXISTS" |
            "INCOMPATIBLE_VERSION" | "PREVIEW_TOO_LARGE" | "UPLOAD_TOO_LARGE",
    "message": "human-readable English",
    "details": { "field": "...", "value": "...", "limit": 5242880 }
  }
}
```

Error code set is stable across versions for i18n in the frontend.

### 4.2 What we deliberately do NOT do

- **No HTML/JS scanning, no AST analysis.** CSP is the only enforcement boundary. Scanning is high cost, low signal, and creates a false sense of security.
- **No virus scanning.** Out of scope for a frontend asset pipeline.
- **No automatic theme activation on upload.** Admin must explicitly activate via a second action (prevents "upload a broken theme by accident → site is down").

---

## 5. Runtime Serving Model

### 5.1 `AppState` extension

```rust
pub struct AppState {
    // existing fields
    pub active_spa_theme: Arc<ArcSwap<Option<LoadedTheme>>>,
}

pub struct LoadedTheme {
    pub uuid: String,
    pub manifest: ThemeManifest,
    pub entry: String,                    // e.g. "index.html"
    pub files: HashMap<String, Bytes>,    // normalized POSIX paths → file bytes
    pub mime_cache: HashMap<String, &'static str>, // path → content-type
}
```

`ArcSwap` is preferred over `RwLock` so the hot serve path takes no lock. Memory cost is bounded by the 20 MB package cap.

### 5.2 Startup

On server start, after migrations:
1. Read `ACTIVE_SPA_THEME_KEY` from `ConfigService`
2. If present and a row matches that uuid, load `package_data`, unzip into a `LoadedTheme`, store in `ArcSwap`
3. On dangling uuid (row missing) or corrupt blob, log a warning and fall back to default — do not abort startup

### 5.3 Activation flow

```
PUT /api/settings/active-spa-theme  { theme_id: <uuid> | null }
    │
    ├─ require_admin
    ▼
1. If body.theme_id is null:
   - Clear ACTIVE_SPA_THEME_KEY (delete config row or store empty string)
   - active_spa_theme.store(Arc::new(None))
   - audit: spa_theme.deactivate (detail captures the previous uuid)
   - 200
2. Otherwise:
   - Look up row by uuid (404 if missing)
   - Unzip package_data into LoadedTheme (error if zip corrupted post-storage)
   - Store row.uuid under ACTIVE_SPA_THEME_KEY
   - active_spa_theme.store(Arc::new(Some(loaded)))
   - audit: spa_theme.activate
   - Set-Cookie: sb_force_default=1; Path=/; Max-Age=3600; HttpOnly; SameSite=Strict
     ↑ ensures the activating admin's current tab stays on default SPA
   - 200 with { activated_uuid }
```

### 5.4 Serve path

Router order (existing routes preserved):

```
/api/*              → existing handlers
/api/ws/*           → existing handlers
/swagger-ui/*       → existing
/api-docs/*         → existing
/__system/*         → new reserved prefix (Section 6.3)
/?theme=default     → matched in middleware (Section 6.2)
/*                  → SPA serve handler:
                       1. If cookie sb_force_default present → default SPA
                       2. If query theme=default present (any path) → default SPA + set cookie
                       3. If active_spa_theme.load() is Some(t):
                          - Normalize requested path; default to t.entry for `/`
                          - If t.files contains path → 200 + bytes + CSP + content-type
                          - Else if t.files contains t.entry → 200 + entry HTML (SPA history routing)
                          - Else → 404
                       4. Otherwise → rust-embed default SPA (existing behavior)
```

### 5.5 CSP headers (theme responses only)

```
Content-Security-Policy:
  default-src 'self';
  script-src 'self' 'unsafe-inline' 'unsafe-eval';
  style-src 'self' 'unsafe-inline';
  img-src 'self' data: blob:;
  font-src 'self' data:;
  connect-src 'self';
  frame-ancestors 'none';
  base-uri 'self';
  form-action 'self';
X-Content-Type-Options: nosniff
X-Frame-Options: DENY
Referrer-Policy: same-origin
```

`connect-src 'self'` is the load-bearing directive: it prevents the theme from sending the user's session cookie or any data to attacker-controlled domains. `unsafe-eval` is included pragmatically (Vue runtime, wasm-bindgen, etc. require it); the default SPA uses a stricter CSP without `unsafe-eval`.

The default SPA's CSP is unchanged.

### 5.6 Caching

- Theme assets: `Cache-Control: public, max-age=31536000, immutable` for paths under `assets/` (assumes fingerprinted filenames per Vite convention).
- `index.html` and other top-level files: `Cache-Control: no-cache`.
- ETag computed from SHA-256 of the file bytes (cheap given in-memory storage).

---

## 6. Recovery & Safety Mechanisms

### 6.1 Why this section exists

A custom SPA replaces the *entire* UI, including the page admins use to switch themes. A broken or malicious theme can lock everyone out. Three layered escape hatches:

### 6.2 `?theme=default` query parameter

Any request with `?theme=default` in the query string returns the default SPA, regardless of the active-theme pointer. The handler additionally sets `Set-Cookie: sb_force_default=1; Max-Age=3600` so subsequent SPA history navigations (which lose the query) stay on the default.

To clear the cookie: `GET /__system/clear-recovery` returns 204 and sets `Set-Cookie: sb_force_default=; Max-Age=0`.

### 6.3 `/__system/*` reserved prefix

Routes under `/__system/*` are always served by the default SPA, never overridden by a custom theme. v1 routes:

- `/__system/clear-recovery` — clear `sb_force_default` cookie
- `/__system/admin/spa-themes` — admin-only theme management page (default SPA)

Themes are forbidden from declaring entries that would collide with `/__system/*` (validator rejects packages with such paths, although in practice they'd just be unreachable).

### 6.4 Activation safety net (UI-side)

After successful `PUT /api/settings/active-spa-theme`, the admin's *current* tab stays on the default SPA (via `sb_force_default` cookie set in the activation response). The UI shows a modal:

```
Theme activated.
- Open in new tab    → window.open('/', '_blank') — sees new theme (no cookie)
- Apply to this tab  → clear cookie, location.reload() — switches to new theme
```

The admin can preview safely before committing their own session.

---

## 7. API Surface

All routes are admin-only (existing `require_admin` middleware), return `ApiResponse<T>`, and are documented via `#[utoipa::path]`.

| Method | Path | Body | Returns |
|---|---|---|---|
| `GET` | `/api/settings/spa-themes` | — | `[{ uuid, manifest, size_bytes, uploaded_by, uploaded_at, is_active, is_superseded }]` |
| `POST` | `/api/settings/spa-themes` | multipart `package` | `{ uuid, manifest, size_bytes, preview_url, is_upgrade_of }` |
| `GET` | `/api/settings/spa-themes/:uuid` | — | full theme metadata |
| `GET` | `/api/settings/spa-themes/:uuid/preview` | — | image bytes (or 404) |
| `GET` | `/api/settings/spa-themes/:uuid/package` | — | original zip bytes (admin download) |
| `DELETE` | `/api/settings/spa-themes/:uuid` | — | 204 (409 if active) |
| `GET` | `/api/settings/active-spa-theme` | — | `{ theme_id: uuid | null }` |
| `PUT` | `/api/settings/active-spa-theme` | `{ theme_id: uuid \| null }` | `{ activated_uuid \| null }` |

Themes themselves consume the existing `/api/*` and `/api/ws/*` surface; no new "theme SDK" endpoint. The existing OpenAPI document at `/api-docs/openapi.json` is the canonical reference for theme authors.

---

## 8. Frontend: `/settings/appearance` Changes

A new top section `CustomSpaThemeSection` lives in `apps/web/src/routes/_authed/settings/appearance.tsx`, placed above the existing `ThemeGrid` (color themes) and `BrandSettingsSection` blocks. It is rendered only for admin users (gated by the existing role check).

### 8.1 Components

- `CustomSpaThemeSection` — orchestrator, warning banner, grid layout
- `SpaThemeCard` — per-theme card with preview, manifest excerpt, active indicator, actions
- `SpaThemeUploadCard` — drag-drop + file picker, progress, error display
- `ActivateSpaThemeDialog` — confirmation modal with checkbox gate
- `ActivationSuccessDialog` — post-activation "preview in new tab vs apply now"
- `SpaThemeDetailsDrawer` — right drawer with full manifest, file list, audit history, package download

### 8.2 API hooks (in `apps/web/src/api/spa-themes.ts`)

- `useSpaThemes()` — list, React Query
- `useUploadSpaTheme()` — multipart, with progress
- `useDeleteSpaTheme()`
- `useActivateSpaTheme()` — also handles the `sb_force_default` cookie set by the server
- `useActiveSpaTheme()`

### 8.3 Behavioural rules

- Non-admins do not see this section at all (server-side enforced; UI is defense in depth)
- When a custom theme is active, the existing color-theme grid and brand-settings section show a banner: "A custom frontend is active. These settings have no effect until it is deactivated."
- Cannot delete the active theme (button disabled, tooltip explains)
- Activation modal requires a checkbox: "I understand and want to activate"

---

## 9. Developer Workflow

### 9.1 Starter template

Location: `templates/serverbee-theme-starter/` in this repo (single source of truth; CI can validate it builds; users can `npx degit ZingerLittleBee/ServerBee/templates/serverbee-theme-starter my-theme`).

Contents:

```
serverbee-theme-starter/
├── manifest.json
├── package.json              # bun + vite + typescript
├── vite.config.ts            # base: './' so assets resolve relatively
├── index.html
├── public/preview.png
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   ├── lib/serverbee.ts      # API client (fetch + WS) using same-origin auth
│   └── index.css
├── pack.ts                   # bun run pack → <id>-<version>.sbtheme
└── README.md
```

`pack.ts` runs `vite build`, validates the output against the same constraints the server enforces (size, count, extensions), and produces a deterministic zip.

### 9.2 Documentation

Add `apps/docs/content/docs/{cn,en}/themes/custom-frontend.mdx` covering:

1. What can/cannot be customized
2. Quickstart (degit → edit → pack → upload)
3. Manifest reference (full table)
4. API & WebSocket reference (link to swagger-ui)
5. CSP constraints and what they mean for theme authors
6. Recovery and debugging (`?theme=default`, `/__system/admin`, DevTools CSP violations)
7. Best practices (i18n, dark mode, accessibility, mobile)
8. Size and file limits

Bilingual, consistent with existing 16-page documentation style.

### 9.3 Out of scope for v1

- `@serverbee/theme-cli` (`sbt new / pack / validate / publish`) — defer to v2; starter + `pack.ts` is enough to ship.
- Theme marketplace, signing, distribution registry — v2+.

---

## 10. Testing

### 10.1 Rust unit tests

`crates/server/src/service/spa_theme/`:

- `manifest_validator` — table-driven cases for each rejection reason and each successful shape
- `zip_extractor`:
  - Zip slip (entries with `../`, absolute paths, drive letters)
  - Zip bomb (single entry with > 100× ratio)
  - Symlink zip entries
  - Duplicate entries
  - Per-file / total / count limits
  - Disallowed extensions
  - Valid package → expected file map
- `version_upgrade` — semver comparison and downgrade rejection

### 10.2 Rust integration tests

`crates/server/tests/spa_theme_integration.rs` (matches existing integration test conventions):

- Non-admin upload → 403
- Admin upload of valid sample fixture → 200, row inserted
- Same id + higher version → 200 + `is_upgrade_of` + previous row marked superseded
- Same id + lower version → 400 `NO_DOWNGRADE`
- Same id + same version → 409 `VERSION_EXISTS`
- Activate by uuid → subsequent `GET /` returns theme entry HTML (assert via response body marker)
- `GET /?theme=default` always returns default SPA marker + sets recovery cookie
- Delete active theme → 409
- Delete inactive theme → 204, blob removed
- Upload pre-built malicious fixtures (zip slip, zip bomb, symlink) → 400 with specific error codes

Fixture packages live under `crates/server/tests/fixtures/spa_themes/`.

### 10.3 Frontend tests (vitest)

`apps/web/src/routes/_authed/settings/appearance.test.tsx` and per-component test files:

- Section visibility per role
- Upload flow with mocked multipart, including error responses
- Activation dialog gating on checkbox
- Active theme deletion disabled state
- API hook contracts

### 10.4 Manual E2E checklist

`tests/spa-themes.md` (follows existing manual-test convention):

- [ ] Admin uploads starter theme → succeeds
- [ ] Non-admin does not see Custom Frontend section
- [ ] Activation shows preview-in-new-tab vs apply-now choice
- [ ] New tab shows new theme; current tab stays on default
- [ ] Apply-now reloads current tab into new theme
- [ ] `?theme=default` recovers from any custom theme
- [ ] Deactivate restores default SPA for all clients on next request
- [ ] Delete non-active theme succeeds
- [ ] Delete active theme rejected with clear message
- [ ] Zip-slip fixture rejected with `ZIP_SLIP` error code
- [ ] Zip-bomb fixture rejected with `ZIP_BOMB` error code
- [ ] CSP headers present on theme responses (verified in DevTools)
- [ ] Version upgrade: v1.0 → v1.1 succeeds and does not auto-activate
- [ ] Version downgrade: v1.1 → v1.0 rejected
- [ ] Audit log shows upload/activate/deactivate/delete events

---

## 11. Migration & Rollout

### 11.1 Migration

One sea-orm migration `mYYYYMMDD_NNNNNN_create_spa_themes.rs`:
- Create `spa_themes` table with indexes per Section 2.1
- No changes to `config`/KV store (the new `ACTIVE_SPA_THEME_KEY` is set lazily on first activation)
- No changes to `audit_logs` (reused as-is)

Per repo convention, only implement `up()`. `down()` is a no-op.

### 11.2 Backwards compatibility

- Default SPA continues to work unchanged for all existing deployments
- Existing color themes, custom color themes, and brand settings are untouched
- A fresh install has no `ACTIVE_SPA_THEME_KEY` value → identical behavior to today

### 11.3 Capability flag

This feature does not need a server capability bit (themes execute in the browser, not on agents). It is gated only by admin role.

### 11.4 Documentation updates

- `apps/docs/content/docs/{cn,en}/themes/custom-frontend.mdx` — new page
- `apps/docs/content/docs/{cn,en}/configuration.mdx` — add note about the recovery URL
- Link from existing `/settings/appearance` documentation page

---

## 12. Open Questions for Planning

These do not block the design but should be resolved during implementation planning:

1. Starter template Vite base-path strategy: `base: './'` (relative assets, simplest) vs `base: '/'` (absolute, requires no path rewriting but harder if theme assets are ever served from a sub-path). v1 recommendation: `base: './'`, with the documented constraint that themes are always served from `/`. Decide for sure when authoring the starter.
2. Whether to keep the `package_data` BLOB in `spa_themes` after extraction. Pros of keeping: source of truth for download/re-extraction; cons: doubles SQLite size. v1 recommendation: keep it (20 MB cap makes the cost bounded), revisit if storage becomes an issue.
3. Whether `unsafe-eval` in the theme CSP should be opt-in via a manifest field rather than always-on. v1 recommendation: always-on for v1 (most React/Vue/Svelte builds need it); add `manifest.csp.allow_eval: false` as an opt-out in a later version if theme authors care.
