# Custom Widget System Design

**Status:** Draft (v2, post-review)
**Date:** 2026-05-28
**Branch:** `minnetonka-v5`
**Supersedes:** `2026-05-26-custom-spa-themes-design.md` (full-SPA-replacement model is deleted in this redesign)
**Related:** existing dashboard widgets in `apps/web/src/components/dashboard/`, color theme system in `apps/web/src/themes/`.

## Goal

Replace the hard-coded 14-widget catalog and the three coexisting color-theme mechanisms with a single, pluggable **Widget Module** abstraction. Admins should be able to (a) write their own widgets as a single `.widget.js` file using a stable SDK, (b) install widgets from a URL or by uploading a file/zip, and (c) author **Themes** that bundle CSS variables and widget collections in one package.

The end state has one mental model: a Widget Module is an ESM file that calls `defineWidget({ ... })`. Built-in widgets, URL-installed widgets, and uploaded widgets are all the same shape. Themes are the single delivery channel for visual customization (CSS variables) plus bundled widgets.

## Non-goals

- Official Marketplace / community widget registry (a curated documentation page lists examples instead).
- Sandboxed execution (iframe / ShadowRealm). Widgets run in the main page context under a **trusted admin** model.
- Full-SPA replacement. The existing `spa_theme` mechanism (upload `.sbtheme` zip that supplants the entire frontend through the catch-all route) is **removed** as part of this work.
- Data migration. Project is in dev; old `dashboard_widget.widget_type` values, old `custom_theme` rows, and old `spa_theme` rows are dropped without conversion.
- Per-user widget installation. Widget modules are system-wide; only admins can install/uninstall.
- Hot-reload during widget development (rebuild + reinstall cycle).
- Per-widget API permission whitelisting. Widgets run with the current logged-in user's identity and can call any endpoint that identity can call. The trust model is "admin-installed widget = trusted code, equivalent to replacing the bundled SPA on disk."

---

## 1. Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  Source                                                           │
│    • Builtin (Vite multi-entry → embedded into server binary)     │
│    • URL     (admin pastes https://.../foo.widget.js)             │
│    • Upload  (admin drops .js single file or .zip collection)     │
│    • Bundled in a Theme (theme install registers contained widgets)│
└────────────────────────┬─────────────────────────────────────────┘
                         │ stored in widget_module table (BLOB)
                         ▼
┌──────────────────────────────────────────────────────────────────┐
│  Backend                                                          │
│    /api/widget-modules            list / install / uninstall      │
│    /api/widget-modules/{id}/...   serve module bundle + assets   │
│    /api/themes                    list / upload / activate        │
│    /api/themes/{id}/preview       preview image                   │
└────────────────────────┬─────────────────────────────────────────┘
                         │ on SPA boot: GET /api/widget-modules
                         ▼
┌──────────────────────────────────────────────────────────────────┐
│  Browser Loader                                                   │
│    for each enabled module:                                       │
│      mod = await import('/api/widget-modules/' + id + '/' + entry)│
│      Registry.register(mod.default)                               │
│    Relative imports and assets inside a module resolve through    │
│    the same /api/widget-modules/{id}/... path naturally.          │
└────────────────────────┬─────────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────────┐
│  Widget Registry (singleton in SPA)                               │
│    get(id) / list() / register(module) / unregister(id)           │
└────────────────────────┬─────────────────────────────────────────┘
                         │ resolved at render time
                         ▼
┌──────────────────────────────────────────────────────────────────┐
│  Dashboard Grid (react-grid-layout, unchanged shell)              │
│    instance = { id, module_id, config_json, grid_x/y/w/h }        │
│    WidgetRenderer: Registry.get(module_id) → render via SDK       │
└──────────────────────────────────────────────────────────────────┘

                              ▲
                              │ stable hook + action API
                              │
┌──────────────────────────────────────────────────────────────────┐
│  Widget SDK (@serverbee/widget-sdk)                               │
│    defineWidget({...})                                            │
│                                                                   │
│  Live metrics (WS):                                               │
│    useServers() useServer(id) useMetric(id, path)                 │
│    useCapability(id, cap)                                         │
│                                                                   │
│  Domain data (REST + cache):                                      │
│    useHistory(id, path, range)                                    │
│    useAlerts() useServiceMonitors()                               │
│    useTraffic(id, range) useUptime(id, days) useGeoIp()           │
│                                                                   │
│  Escape hatch (any endpoint):                                     │
│    useApiQuery(path, params)                                      │
│    useApiMutation(method, path)                                   │
│                                                                   │
│  Host context:                                                    │
│    useTheme() useConfigUpdate()                                   │
│                                                                   │
│  Schema mini-validator: z.*                                       │
│                                                                   │
│  Runtime injection via import-map:                                │
│    react, react-dom, react/jsx-runtime, @serverbee/widget-sdk     │
│      → /runtime/{name}.js shims re-exporting host singletons      │
└──────────────────────────────────────────────────────────────────┘
```

### Key replacements

| Old | New |
|---|---|
| `apps/web/src/lib/widget-types.ts` (14 hard-coded types) | One `.widget.tsx` per built-in widget under `apps/web/src/builtin-widgets/`, compiled via Vite multi-entry |
| `dashboard_widget.widget_type` (free string) | `dashboard_widget.module_id` (references `widget_module.id`) |
| `apps/web/src/themes/` (8 preset CSS + `preset-vars.ts`) | **Deleted.** CSS variables come from Theme manifest |
| `custom_theme` entity + OKLCH editor + `/api/settings/themes` | **Deleted.** |
| `spa_theme` full-SPA-replacement (28 files, see §6.1) | **Deleted.** Name "Theme" is reclaimed for the new cssVars + widgets concept |
| Per-widget data subscriptions scattered everywhere | All data flows through SDK hooks |

---

## 2. Widget SDK

The SDK is the single contract between widget authors and the host. Stability of this surface is the project's primary long-term commitment to authors.

### 2.1 Widget file layout

A widget file has two pieces: a JSDoc manifest block at the top (extracted statically by the backend at install time) and a `defineWidget` default export (executed at runtime).

```ts
/**
 * @serverbee-widget {
 *   "id": "com.example.cpu-gauge",
 *   "version": "1.0.0",
 *   "name": "CPU Gauge",
 *   "category": "Real-time",
 *   "sizing": { "defaultW": 3, "defaultH": 3, "minW": 2, "minH": 2, "strategy": "aspect-square" },
 *   "requiredCaps": [],
 *   "sdkVersion": "^1.0.0"
 * }
 */
import { defineWidget, z, useMetric } from '@serverbee/widget-sdk'

const ConfigSchema = z.object({
  serverId: z.serverId().describe('Target server'),
  metric: z.metricPath().default('cpu.usage'),
  threshold: z.number().min(0).max(100).default(80),
})

export default defineWidget({
  configSchema: ConfigSchema,
  component: ({ config }) => {
    const value = useMetric(config.serverId, config.metric)
    return <Gauge value={value ?? 0} max={100} warn={config.threshold} />
  },
})
```

The JSDoc block is **the** source of truth for everything the backend needs to know about the module before its code runs (id, version, sizing, required capabilities, SDK version). The `defineWidget` call carries the runtime-only pieces (configSchema, component, optional actions). This split is what makes installation safe and reliable — see §3.4.

### 2.2 Data subscription hooks

**Live metric hooks** — reuse the existing `useServersWs` WebSocket store:

| Hook | Returns |
|---|---|
| `useServers()` | `ServerSummary[]` — id, name, online, lastSeen, capabilities |
| `useServer(id)` | Full `ServerMetrics \| undefined` for one server |
| `useMetric(id, path)` | Single value extracted by dot/bracket path (`'cpu.usage'`, `'disks[0].used'`) |
| `useCapability(id, cap)` | `boolean` |

**Domain hooks** — first-class wrappers around concrete REST endpoints used by today's built-in widgets. Each one already has a known query key, cache strategy, and TS shape. They exist so the common cases don't need raw `useApiQuery`.

| Hook | Endpoint |
|---|---|
| `useHistory(id, path, range)` | Time-series for a metric path |
| `useAlerts({ limit })` | `GET /api/alert-events` |
| `useServiceMonitors()` | `GET /api/service-monitors` |
| `useTraffic(id, range)` | `GET /api/servers/{id}/traffic` (per-server) or `GET /api/traffic/overview/daily` (global) |
| `useUptime(id, days)` | `GET /api/servers/{id}/uptime-daily` |
| `useGeoIp()` | `GET /api/geoip/status` + `POST /api/geoip/download` (returns `{ status, download }` pair) |

**Escape hatch** — for anything not yet promoted to a first-class hook, or for custom widgets that call new endpoints:

```ts
const { data, error, isLoading } = useApiQuery<MyType>('/api/some/endpoint', { params: {...} })
const mutation = useApiMutation<MyResp, MyReq>('POST', '/api/some/action')
mutation.mutate(body)
```

These wrap TanStack Query with the host's `api-client.ts` (auto-unwraps `{ data: T }`, attaches credentials, sends `X-API-Key` or session cookie). They run with **the current logged-in user's identity** — there is no per-widget permission whitelist (see Non-goals).

**Host context:**

| Hook | Returns |
|---|---|
| `useTheme()` | `{ mode: 'light' \| 'dark', cssVar: (name) => string }` |
| `useConfigUpdate()` | `(patch: Partial<TConfig>) => void` — lets a widget mutate its own config (e.g. "save view") |

### 2.3 Actions: declarative buttons

Many real widgets need a button: "Download GeoIP database", "Restart service", "Acknowledge alert". The SDK provides a declarative `actions` array on `defineWidget`, which renders standard buttons with confirm-dialog, loading state, success/error toast, and emits to the audit log — without each widget reinventing it.

```ts
export default defineWidget({
  configSchema: ConfigSchema,
  actions: [
    {
      id: 'download-geoip',
      label: 'Download database',
      icon: 'download',
      confirm: { title: 'Download GeoIP database?', body: 'This may take ~30s.' },
      run: async ({ apiMutation }) => {
        await apiMutation('POST', '/api/geoip/download')
      },
    },
  ],
  component: ({ config, actions }) => (
    <div>
      <GeoIpStatus />
      {actions.render('download-geoip')}
    </div>
  ),
})
```

Actions are optional. Widgets that prefer can call `useApiMutation` directly and roll their own UI.

### 2.4 `configSchema` → auto-generated form

The SDK ships a Zod-like mini-validator (`z`) with the standard primitives plus four widget-specific extensions:

- `z.serverId()` — renders a server picker
- `z.metricPath()` — renders a metric-path picker (browses the live ServerMetrics shape)
- `z.color()` — renders a color picker
- `z.duration()` — renders a duration input (e.g. `5m`, `1h`)

`renderConfigForm(schema, value, onChange)` consumes the schema and produces the entire ConfigDialog UI. Widget authors write no form code for typical configs.

Rationale for not pulling full `zod`: keeps the SDK runtime under ~10KB gzipped, avoids two zod versions colliding when authors also use zod elsewhere.

### 2.5 Component contract

```ts
type WidgetProps<TConfig> = {
  config: TConfig                        // validated + defaulted
  size: { w: number; h: number }         // current grid pixel size
  isEditing: boolean                     // dashboard is in edit mode
  actions: {
    render: (id: string) => ReactNode    // mount a declared action button
  }
}
```

No additional globals. All host capabilities are exposed through SDK hooks and the `actions` injector.

### 2.6 Runtime injection — React/SDK singletons

```html
<!-- index.html -->
<script type="importmap">
{
  "imports": {
    "@serverbee/widget-sdk": "/runtime/widget-sdk-{hash}.js",
    "react":                  "/runtime/react-{hash}.js",
    "react-dom":              "/runtime/react-dom-{hash}.js",
    "react/jsx-runtime":      "/runtime/react-jsx-runtime-{hash}.js"
  }
}
</script>
```

Each `/runtime/*.js` is a thin shim re-exporting the host's instance from `globalThis`. The hash is the SPA build's content hash, so an SPA upgrade invalidates them deterministically.

This is required because:

- JSX compilation emits `import { jsx } from 'react/jsx-runtime'` — without externalising, every widget bundles its own React copy, hooks collapse.
- Authors will write `import { useMemo } from 'react'` — same failure mode.
- Two SDK copies would mean two `WidgetRegistry` singletons — registration races.

The SDK package's `peerDependencies` declares `react`, `react-dom`. Widget bundler templates (Vite, Rollup) in the docs ship pre-configured with these externals plus `@serverbee/widget-sdk`. CI in the example repo verifies the produced bundle declares no React copy.

### 2.7 Version compatibility

The JSDoc manifest's `sdkVersion` field is a semver range. The Registry validates on registration:

- Major mismatch (host outside range) → reject load, surface error in admin UI ("widget needs SDK ^2.x, host is 3.x").
- Allowed range, but newer minor available → load silently.
- Deprecated hooks emit a `console.warn` for at least one minor cycle before removal.

---

## 3. Distribution & Installation

### 3.1 `widget_module` table

```rust
pub struct Model {
    pub id: String,                       // 'com.example.cpu-gauge'
    pub version: String,                  // semver
    pub source_type: SourceType,          // Builtin | Url | Upload | BundledByTheme
    pub source_url: Option<String>,       // original URL for Url; filename for Upload
    pub bundled_by_theme_id: Option<String>, // when source_type = BundledByTheme; deletes cascade
    pub manifest_json: String,            // JSDoc-extracted metadata (id, version, sizing, ...)
    pub code_sha256: String,              // entry file fingerprint
    pub entry_path: String,               // e.g. "index.js" for single-file; "widgets/cpu.js" for collection
    pub package_blob: Option<Vec<u8>>,    // packed archive (single-file or zip); empty for Builtin
    pub installed_by: Option<i64>,        // user_id, null for Builtin
    pub installed_at: DateTimeWithTimeZone,
    pub enabled: bool,
}
```

Decision: widget code lives in SQLite, not on a remote CDN — keeps offline VPS installs functional, lets the backend cache-control responses, prevents silent upstream tampering after install, and gives admins one place to audit.

### 3.2 Asset serving

```
GET /api/widget-modules
  → [{ id, version, manifest, entry_path, code_sha256 }, ...]

GET /api/widget-modules/{id}/{*asset_path}
  → serves any file inside the module's package
  → cache: ETag = "{version}-{sha256_prefix}", Cache-Control: public, max-age=86400, immutable
  → content-type by extension (js → text/javascript, svg/png/css/json/...)
  → 404 on path-traversal attempts
```

For a Builtin module, "package" is the Vite-emitted directory under `dist/builtin-widgets/{id}/`. For Upload (single file), package is a one-file archive. For Upload (zip) or BundledByTheme, package is the original zip. The server unpacks to a temp directory at startup for fast serving, or streams from the BLOB with a small in-process LRU cache.

### 3.3 Loader sequence

```js
// in SPA boot
const modules = await fetch('/api/widget-modules').then(r => r.json())
await Promise.allSettled(modules.map(async m => {
  try {
    const url = `/api/widget-modules/${m.id}/${m.entry_path}`
    const mod = await import(/* @vite-ignore */ url)
    Registry.register(m.id, mod.default, m.manifest)
  } catch (err) {
    Registry.recordLoadFailure(m.id, err)   // shown in admin UI; other widgets keep loading
  }
}))
```

Native `import(url)` — no Blob URL. This is what lets relative imports inside a module (`import './utils.js'`, `<img src="./icon.svg" />`) resolve naturally through the same `/api/widget-modules/{id}/...` path.

### 3.4 Three install paths

| Source | Admin UX | Backend behavior |
|---|---|---|
| **Builtin** | n/a — registered at server boot | At boot, read `dist/builtin-widgets/manifest.json`, upsert one `widget_module` per entry with `source_type = Builtin` |
| **URL** | Settings → Widgets → "Add by URL" → paste raw JS URL → preview JSDoc manifest → confirm | Server-side fetch (http(s) only, reject private/loopback ranges, 1 MB body cap) → extract JSDoc → validate → store |
| **Upload (.js)** | Drop single `.js` → preview → confirm | Read JSDoc → validate → store as one-file package (`entry_path = "index.js"`) |
| **Upload (.zip)** | Drop `.zip` → preview `collection.json` → confirm | Unzip with zip-slip guard, read `collection.json`, extract JSDoc from each listed entry, register one `widget_module` per entry |

### 3.5 JSDoc manifest extraction

Backend extraction is a single, deterministic step:

1. Locate the first JSDoc block matching `/\*\*[\s\S]*?@serverbee-widget\s+(\{[\s\S]*?\})[\s\S]*?\*/`.
2. Strip leading `* ` line decorations.
3. Parse the captured `{...}` with `serde_json`.
4. Validate against the `WidgetManifest` Rust struct (required: `id`, `version`, `name`, `category`, `sizing`, `sdkVersion`).
5. Reject if missing, malformed, or if `id` collides with an existing module that has a different `source_type`.

This is robust because the manifest is a pure JSON literal in a comment — no AST traversal, no variable resolution, no spread / computed values to worry about. The runtime `defineWidget({...})` call may use any variables, helper functions, dynamic imports, etc.

A second-layer validation runs **at SPA load time**: the Registry checks that the executed `defineWidget` returned a `WidgetModule` whose internal fields (configSchema, component) are well-formed, and that any optional `actions[]` parse correctly. Failures here surface as "broken widget" cards without blocking the dashboard.

### 3.6 Zip collection format

```
my-collection.zip
├── collection.json
├── widgets/
│   ├── cpu-gauge.widget.js
│   ├── mem-chart.widget.js
│   └── disk-io.widget.js
└── assets/                  # optional shared static assets
    └── icon.svg
```

`collection.json`:

```json
{
  "id": "com.example.my-pack",
  "name": "Example Widget Pack",
  "version": "1.0.0",
  "author": "Jane Doe",
  "description": "CPU, memory, and disk widgets.",
  "widgets": [
    "widgets/cpu-gauge.widget.js",
    "widgets/mem-chart.widget.js",
    "widgets/disk-io.widget.js"
  ],
  "license": "MIT"
}
```

Each listed file must have a `@serverbee-widget` JSDoc block. The zip extraction step refuses collections with duplicate widget ids and any zip entry whose canonicalised path escapes the extraction root (zip-slip).

### 3.7 Permissions & trust

- **Admin** — install / uninstall / enable / disable widget modules; install via URL or upload.
- **Member** — see installed widgets in the picker; use them on dashboards. Cannot install.
- `requiredCaps` is enforced at render time. When the selected server lacks a capability, the widget renders a disabled placeholder explaining which capability is missing.
- Widgets run with the **current logged-in user's** session/API-key identity, in the same JavaScript context as the host SPA. They can call any same-origin API that identity can call, read DOM, read storage. The trust model: admin-installed widget code is treated as trusted, comparable to installing a browser extension on a colleague's machine — there is no per-widget allowlist and no sandbox. Members using a dashboard cannot install widgets, but they execute whatever widgets admins have installed.

### 3.8 Marketplace (out of scope)

No official marketplace. Documentation maintains a hand-curated "Community Widgets" section inside `publishing.mdx` listing example URLs. Future extension point: a `marketplace_url` setting whose JSON manifest the SPA could iterate over — not built now.

---

## 4. Built-in Widget Build Pipeline

The 14 built-in widgets need to compile to ESM modules that the host can serve and the loader can `import()`.

### 4.1 Source layout

```
apps/web/src/builtin-widgets/
├── cpu-gauge.widget.tsx
├── line-chart.widget.tsx
├── ...                          (one file per builtin)
└── _shared/                     (internal helpers, not widgets)
    └── format.ts
```

Each `*.widget.tsx` file has the JSDoc manifest block + `export default defineWidget({...})` — same shape as a user-written widget. They are not special.

### 4.2 Vite multi-entry

Extend `apps/web/vite.config.ts`:

```ts
import { globSync } from 'tinyglobby'

const builtinWidgets = Object.fromEntries(
  globSync('src/builtin-widgets/*.widget.tsx').map(p => {
    const id = path.basename(p, '.widget.tsx')                       // 'cpu-gauge'
    return [`builtin-widgets/${id}/index`, p]
  })
)

export default defineConfig({
  build: {
    rollupOptions: {
      input: { main: 'index.html', ...builtinWidgets },
      external: ['react', 'react-dom', 'react/jsx-runtime', '@serverbee/widget-sdk'],
      output: {
        entryFileNames: assetInfo =>
          assetInfo.name?.startsWith('builtin-widgets/')
            ? `${assetInfo.name}.js`
            : 'assets/[name]-[hash].js',
      },
    },
  },
})
```

Output:

```
apps/web/dist/
├── (main SPA assets, unchanged)
└── builtin-widgets/
    ├── manifest.json          # generated by a vite plugin (id → entry_path map)
    ├── cpu-gauge/
    │   └── index.js
    ├── line-chart/
    │   └── index.js
    └── ...
```

A small Vite plugin reads each `*.widget.tsx`'s JSDoc block at build time and writes `dist/builtin-widgets/manifest.json`:

```json
[
  { "id": "com.serverbee.cpu-gauge",  "version": "1.0.0", "entry_path": "cpu-gauge/index.js",  "manifest": { ... full JSDoc ... } },
  { "id": "com.serverbee.line-chart", "version": "1.0.0", "entry_path": "line-chart/index.js", "manifest": { ... } }
]
```

### 4.3 Server boot registration

`crates/server/src/router/static_files.rs` already embeds `apps/web/dist` via `rust-embed`. At server boot, a new `builtin_widgets::register_all(&db)` routine:

1. Reads embedded `builtin-widgets/manifest.json`.
2. For each entry, computes `code_sha256` of the entry file's bytes.
3. Upserts `widget_module` row with `source_type = Builtin`, `package_blob = None` (served directly from the embedded FS).
4. Removes any stale Builtin rows whose `id` is not in the new manifest.

The asset-serving route (§3.2) special-cases `source_type = Builtin` to serve from `rust_embed` instead of the BLOB.

---

## 5. Built-in Widget Rewrite

All 14 widgets are rewritten to the new shape:

`stat-number`, `metric-card`, `server-cards`, `gauge`, `line-chart`, `multi-line`, `top-n`, `alert-list`, `service-status`, `traffic-bar`, `disk-io`, `server-map`, `markdown`, `uptime-timeline`.

Rewrite serves two purposes:

1. **Dogfooding** — if any of the 14 cannot be expressed cleanly via `defineWidget` + SDK hooks + `actions`, the SDK is wrong and must be extended **before** continuing.
2. **Eliminating dual code paths** — once rewritten, `widget-types.ts`, the old `WidgetRenderer` switch statement, and per-type `widget-config-dialog.tsx` branches are deleted.

The rewrite is chunked to surface SDK gaps early:

- **Chunk A — Live-metric-only widgets** (smallest, prove the live hook surface): `stat-number`, `metric-card`, `gauge`, `server-cards`.
- **Chunk B — Historical chart widgets** (prove `useHistory` and time-series shape): `line-chart`, `multi-line`, `top-n`, `disk-io`.
- **Chunk C — Domain-data widgets** (prove first-class domain hooks and `actions`): `alert-list`, `service-status`, `traffic-bar`, `server-map`, `markdown`, `uptime-timeline`.

After Chunk C lands, the old `WidgetRenderer` + `widget-types.ts` + per-type config dialog code is deleted in a single cleanup commit.

Per project convention, `dashboard_widget.widget_type` is renamed to `module_id` in a schema-only migration; no data conversion (dev period).

---

## 6. Theme System

The name "Theme" is reclaimed for the new concept: **a package of CSS variable overrides + optional bundled widget modules.** It is an overlay on the host SPA, not a replacement of it.

### 6.1 What gets deleted

The existing "SPA Theme" (full-SPA-replacement) feature is removed in its entirety:

| Path | Action |
|---|---|
| `crates/server/src/service/spa_theme/` (whole module: `mod.rs`, `service.rs`, `manifest.rs`, `loaded.rs`, `extractor.rs`, `error.rs`) | Delete |
| `crates/server/src/entity/spa_theme.rs` | Delete |
| `crates/server/src/router/api/spa_theme.rs` | Delete |
| `crates/server/src/migration/m20260526_000035_create_spa_themes.rs` | Keep file (history), add new migration that drops `spa_themes` table |
| `crates/server/src/state.rs` — `active_spa_theme: ArcSwap<...>` field | Remove |
| `crates/server/src/router/static_files.rs` — all references to `active_spa_theme` and `LoadedTheme` (catch-all theme dispatch, lines around 14/99/200/203) | Remove; catch-all serves only the rust-embed default SPA |
| `crates/server/src/openapi.rs` — SPA theme route registrations | Remove |
| `apps/web/src/api/spa-themes.ts` | Delete |
| `apps/web/src/components/spa-theme/` (whole directory, incl. activate/preview/card/upload/details components and their tests) | Delete |
| `apps/web/src/routes/_authed/settings/appearance.tsx` and `.test.tsx` — SPA theme sections | Rewrite to new Theme management |
| `apps/web/src/lib/i18n.ts` — SPA theme strings | Replace with new Theme strings |
| `apps/web/src/themes/` (8 preset CSS files + `preset-vars.ts` + `index.ts`) | Delete |
| `apps/web/src/lib/theme-ref.ts` (`preset:` / `custom:` prefix parser) | Delete |
| `crates/server/src/entity/custom_theme.rs` + custom-theme CRUD in `crates/server/src/router/api/theme.rs` + `apps/web/src/components/theme/*` (theme-card, theme-editor, theme-preview, oklch-picker, delete-theme-dialog) + `apps/web/src/routes/_authed/settings/appearance/themes.*` | Delete |
| sea-orm migration | One new migration: `DROP TABLE spa_themes; DROP TABLE custom_theme;` (`down` no-op per project convention) |

Spec `2026-05-26-custom-spa-themes-design.md` is marked **Superseded** in its header by this document.

### 6.2 New `theme` table

```rust
pub struct Model {
    pub id: String,                       // 'com.example.dark-night'
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub manifest_json: String,
    pub css_vars_light_json: String,      // { "--background": "oklch(...)", ... }
    pub css_vars_dark_json: String,
    pub preview_blob: Option<Vec<u8>>,
    pub package_sha256: String,
    pub package_blob: Vec<u8>,            // full zip; widgets are also unpacked into widget_module
    pub installed_by: Option<i64>,
    pub installed_at: DateTimeWithTimeZone,
}
```

A separate `active_theme` settings row (single row) records which theme is currently active.

### 6.3 Theme manifest

```json
{
  "id": "com.example.dark-night",
  "name": "Dark Night",
  "version": "1.2.0",
  "author": "Jane Doe",
  "description": "Dark monochrome with custom gauge widgets.",

  "cssVars": {
    "light": { "--background": "oklch(1 0 0)", "--foreground": "oklch(0.145 0 0)", "...": "..." },
    "dark":  { "--background": "oklch(0.145 0 0)", "...": "..." }
  },

  "widgets": [ "widgets/special-gauge.widget.js" ],   // optional

  "preview": "preview.png"
}
```

A theme may declare **only** `cssVars` (pure recolor), **only** `widgets` (a widget pack), or both. Single concept, single upload UX.

Zip structure:

```
my-theme.zip
├── manifest.json
├── preview.png            (optional)
└── widgets/               (optional, present when manifest.widgets non-empty)
    └── special-gauge.widget.js
```

On theme install:
1. Validate manifest (required fields, valid CSS var keys, valid widget paths).
2. For each `widgets[]` entry: extract its JSDoc block, register a `widget_module` row with `source_type = BundledByTheme` and `bundled_by_theme_id = theme.id`.
3. Store the theme row.

On theme uninstall:
1. Cascade-delete `widget_module` rows where `bundled_by_theme_id = theme.id`.
2. Delete the theme row.
3. Dashboards referencing now-deleted widget instances render a "Widget removed" placeholder until edited.

### 6.4 Simplified `ThemeProvider`

```ts
type ThemeContext = {
  mode: 'light' | 'dark' | 'system'      // localStorage 'theme-mode'
  setMode: (m) => void
  activeThemeId: string | null            // server-side via /api/settings/active-theme
}
```

Activation flow:

1. SPA boots → `GET /api/settings/active-theme` → load manifest.
2. Inject `cssVars.light` into a `<style>` keyed under `:root`.
3. Inject `cssVars.dark` keyed under `.dark`.
4. If no theme is active, fall back to host SPA's built-in default CSS variables (in `apps/web/src/index.css`, unchanged).
5. Light/dark switching toggles `html.dark`; variables are already in place.

Removed complexity:
- `theme-ref.ts` dual-prefix resolver — gone.
- Dynamic loading of preset `.css` files — gone.
- OKLCH editor + custom_theme CRUD — gone.
- Full-SPA-replacement catch-all and `ArcSwap<Option<LoadedTheme>>` in state — gone.
- Two-layer activation state — collapsed to one.

---

## 7. Documentation Deliverables

Bilingual (zh / en) MDX under `apps/docs/content/docs/{cn,en}/`:

| Doc | Contents |
|---|---|
| `widgets/overview.mdx` | Widget system overview, source types, builtin vs custom, SDK conceptual diagram |
| `widgets/single-file-guide.mdx` | **Approach B full tutorial** — write a `.widget.js` from scratch, JSDoc manifest, `defineWidget`, local dev/preview, URL install. Linked runnable example repo |
| `widgets/collection-guide.mdx` | **Approach C full tutorial** — zip collection structure, `collection.json`, sharing utils across widgets, local packing script, upload & install |
| `widgets/sdk-reference.mdx` | Full SDK API reference — `defineWidget`, every hook (live/domain/escape-hatch/host), every `z.*` type, `actions[]` |
| `widgets/configuration-schema.mdx` | `configSchema` deep dive with `z.serverId()` / `z.metricPath()` / `z.color()` / `z.duration()` |
| `widgets/data-and-recipes.mdx` | Cookbook: common metric paths, building a chart from `useHistory`, joining live + historical, paginating via `useApiQuery` |
| `widgets/sizing.mdx` | Sizing strategy guide (`fixed` / `free` / `aspect-square` / `content-height`) with use-case examples |
| `widgets/capabilities.mdx` | `requiredCaps` usage and the "capability missing on this server" rendering contract |
| `widgets/security-and-trust.mdx` | **Trust model**: admin-installed = trusted; widgets run as the logged-in user; threats and what is *not* sandboxed; review checklist before installing community widgets |
| `widgets/troubleshooting.mdx` | "Widget failed to load" causes, JSDoc manifest errors, version-mismatch errors, React-singleton symptoms (duplicate React, broken hooks), how to read the admin error panel |
| `widgets/publishing.mdx` | Hosting raw JS (GitHub raw / Gist / self-hosted), versioning, **community widgets section** (curated example list) |
| `themes/theme-guide.mdx` | New Theme guide — manifest fields, combined `cssVars` + `widgets`, install/activate/remove flow |

11 widget pages + 1 theme page × 2 languages = **24 MDX files**. Sidebar gets a new top-level **Widgets** section.

**Example repo:** separate `serverbee-widget-examples` GitHub repo with three starting examples — `hello-world.widget.js`, `metric-gauge.widget.js`, a small `example-pack.zip` — plus a pre-configured Vite template producing externalised React + SDK bundles. Docs link to it.

---

## 8. Testing Strategy

| Layer | Coverage |
|---|---|
| SDK unit tests (vitest, `apps/web`) | `defineWidget` validation, `z` schema parsing, `renderConfigForm` snapshots, version compatibility decisions, action runner |
| Widget Registry unit tests | `register` / `get` / `list` / `unregister`, id collision, version conflict, load-failure recording |
| Loader unit tests | Native `import(url)` happy path, isolation of one module's failure, import-map shim resolving to host React/SDK singletons |
| Backend unit tests (Rust) | `widget_module` CRUD, URL fetch (allowlist + size cap + reject private/loopback), zip-slip guard, JSDoc-block regex + JSON parse + manifest validation, theme install with bundled widgets, theme uninstall cascade |
| Backend integration tests | Full lifecycle per source type: URL install / single-file upload / zip upload / theme upload → list → render via SPA → uninstall. Asset-serve route 404s on path traversal |
| Built-in widget regression | One render snapshot per rewritten widget, written **before** rewrite begins. Reviewer diffs snapshots after each chunk |
| E2E manual checklist | `tests/dashboard-widgets-v2.md`: URL install, zip upload, theme upload, configure widget, all 14 built-ins render, theme switch does not break loaded widgets, action button triggers mutation |

**Not in scope:** sandbox tests (trusted-admin model); marketplace tests (no marketplace); per-widget permission tests (no allowlist).

---

## 9. Implementation Phasing

Plans live under `docs/superpowers/plans/`. Preview:

1. **Plan 1 — SDK + Registry + asset serving**
   - `@serverbee/widget-sdk` package skeleton, types, `defineWidget`, `z` mini-validator, all hooks (live + domain + escape hatch + host), `actions` runner
   - Frontend Registry + Loader + import-map shim for React/SDK
   - Backend `widget_module` entity + migration + `/api/widget-modules` (list + asset serve)
   - JSDoc extractor + manifest validator (Rust)
   - Unit-test coverage

2. **Plan 2 — Built-in widget build pipeline + rewrite**
   - **2A** Vite multi-entry config + builtin manifest plugin + Rust boot registration + render-snapshot harness
   - **2B** Rewrite Chunk A (live-only): `stat-number`, `metric-card`, `gauge`, `server-cards`
   - **2C** Rewrite Chunks B + C (historical + domain): the other 10 widgets, then delete old `widget-types.ts`, old `WidgetRenderer`, old per-type config dialogs in a cleanup commit
   - Rename `dashboard_widget.widget_type` → `module_id` (schema-only migration)

3. **Plan 3 — Install UX, Theme system, legacy deletion**
   - **3A** Widget install UX: URL install + .js upload + .zip collection install (frontend + backend)
   - **3B** Theme system: new `theme` entity + install/uninstall (cascade) + activate API + simplified ThemeProvider + new appearance settings page
   - **3C** Delete legacy: SPA full-SPA-replacement code (28 files listed in §6.1) + `apps/web/src/themes/` + `custom_theme` entity & UI + drop `spa_themes` and `custom_theme` tables

4. **Plan 4 — Docs + example repo** (drafts may proceed in parallel with Plan 3; finalised only after SDK API freezes at end of Plan 1)
   - 12 MDX pages × 2 languages
   - `serverbee-widget-examples` repo seeded with 3 examples and Vite template
   - Docs sidebar restructure

Order: Plan 1 → 2A → 2B → 2C → 3A → 3B → 3C. Plan 4 drafting parallel with Plan 3, final pass after.

---

## 10. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Older browsers do not support import-maps | Require Chrome ≥ 89 / Safari ≥ 16.4 / Firefox ≥ 108. Document in install requirements |
| Broken custom widget breaks dashboard | `WidgetRenderer` per-instance error boundary; loader catches `import()` failures per module and records to Registry as `loadFailure`; admin UI shows a "broken" list |
| Duplicate React copy (singleton breakage) | Import-map externalises `react`, `react-dom`, `react/jsx-runtime`; SDK `peerDependencies` declares them; bundler templates in docs ship pre-configured with externals; example repo CI verifies bundles have no React |
| JSDoc extraction brittleness | Single regex + `serde_json` is deterministic. Documented requirement: `@serverbee-widget` block must be at top of file, in a JSDoc comment, with valid JSON. `docs/single-file-guide.mdx` provides copy-pasteable templates |
| SDK API churn breaks installed widgets | Strict SemVer on SDK; widget manifest declares `sdkVersion` range; loader rejects out-of-range with clear admin message; deprecations live ≥ 1 minor cycle |
| Rewriting 14 widgets introduces visual regressions | Render snapshots written before each chunk; reviewer diffs snapshots after the chunk |
| Bundle minification strips JSDoc | Documented requirement: published widget bundles must not minify-away top-of-file JSDoc comments. Vite template in example repo sets `terserOptions.format.comments = /@serverbee-widget/` |
| URL install SSRF | Allowlist `http(s)` only; reject private/loopback/link-local ranges; cap body size; record `installed_by`; admin-only |
| Zip-slip on collection or theme upload | Use a zip crate that validates entry names; reject any entry whose canonicalised path escapes extraction root |
| Trust model misunderstanding by admins | `security-and-trust.mdx` doc; install confirmation dialog shows "This widget will run with your full account permissions" before any URL install |
| Plan 2 SDK gaps surface mid-chunk | Each chunk is its own commit; if Chunk B reveals a missing hook, pause, extend SDK in Plan 1's package, then resume. Chunk ordering (live → historical → domain) front-loads the riskiest discovery |
