# Custom Widget System Design

**Status:** Draft
**Date:** 2026-05-28
**Branch:** `minnetonka-v5`
**Related:** SPA Themes (`docs/superpowers/specs/2026-05-26-custom-spa-themes-design.md`), existing dashboard widget system in `apps/web/src/components/dashboard/`, color theme system in `apps/web/src/themes/`.

## Goal

Replace the hard-coded 14-widget catalog and the three coexisting color-theme mechanisms with a single, pluggable **Widget Module** abstraction. Users — primarily admins — should be able to (a) write their own widgets as a single `.widget.js` file with a stable SDK, (b) install widgets from a URL or by uploading a file/zip, and (c) author SPA Themes that bundle CSS variables and widget collections in one package.

The end state has one mental model: a Widget Module is an ESM file that calls `defineWidget({ ... })`. Built-in widgets, URL-installed widgets, and uploaded widgets are all the same shape. SPA Themes become the single delivery channel for both visual customization (CSS variables) and bundled widgets.

## Non-goals

- Official Marketplace / community widget registry (a curated documentation page lists examples instead).
- Sandboxed execution (iframe / ShadowRealm). Widgets run in the main page context under a **trusted admin** model, equivalent to admin replacing the bundled SPA on disk.
- Data migration. Project is in dev; old `dashboard_widget.widget_type` values, old `custom_theme` rows, and old localStorage keys are dropped without conversion.
- Per-user widget installation. Widget modules are system-wide; only admins can install/uninstall.
- Hot-reload during widget development (rely on rebuild + reinstall cycle; doc site shows a local dev workflow).
- Backwards compatibility shims for the legacy `themes/` directory or `custom_theme` entity.

---

## 1. Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  Source                                                       │
│    • Builtin (rust-embed, registered on server boot)          │
│    • URL     (admin pastes https://.../foo.widget.js)         │
│    • Upload  (admin drops .js or .zip collection)             │
└────────────────────────┬─────────────────────────────────────┘
                         │ stored as BLOB in widget_module table
                         ▼
┌──────────────────────────────────────────────────────────────┐
│  Backend: widget_module table + /api/widgets/* + /api/widgets │
│  /{id}/code.js (raw ESM with cache-control + sha256 etag)     │
└────────────────────────┬─────────────────────────────────────┘
                         │ on SPA boot: GET /api/widgets
                         ▼
┌──────────────────────────────────────────────────────────────┐
│  Browser Loader                                               │
│    for each module:                                           │
│      fetch(code_url)                                          │
│      blob = new Blob([code], {type:'text/javascript'})        │
│      mod  = await import(URL.createObjectURL(blob))           │
│      Registry.register(mod.default)  // = WidgetModule        │
└────────────────────────┬─────────────────────────────────────┘
                         │
                         ▼
┌──────────────────────────────────────────────────────────────┐
│  Widget Registry (singleton in SPA)                           │
│    get(id) / list() / register(module) / unregister(id)       │
└────────────────────────┬─────────────────────────────────────┘
                         │ resolved at render time
                         ▼
┌──────────────────────────────────────────────────────────────┐
│  Dashboard Grid (react-grid-layout, unchanged)                │
│    instance = { id, module_id, config_json, grid_x/y/w/h }    │
│    WidgetRenderer: Registry.get(module_id) → render via SDK   │
└──────────────────────────────────────────────────────────────┘

                              ▲
                              │ stable hook API
                              │
┌──────────────────────────────────────────────────────────────┐
│  Widget SDK (@serverbee/widget-sdk)                           │
│    defineWidget({...})                                        │
│    useServers() useServer(id) useMetric(id, path)             │
│    useHistory(id, path, range) useCapability(id, cap)         │
│    useTheme() useConfigUpdate()                               │
│    z.* — schema for configSchema → auto ConfigDialog          │
│                                                               │
│  Runtime injection via import-map:                            │
│    @serverbee/widget-sdk → /runtime/widget-sdk.js (shim)      │
│      shim re-exports globalThis.__SERVERBEE_SDK__             │
└──────────────────────────────────────────────────────────────┘
```

### Key replacements

| Old | New |
|---|---|
| `apps/web/src/lib/widget-types.ts` with 14 hard-coded types | One `.widget.tsx` module file per built-in widget, registered at server boot via rust-embed |
| `dashboard_widget.widget_type` (free string) | `dashboard_widget.module_id` (FK-like string referencing `widget_module.id`) |
| `apps/web/src/themes/` (8 preset CSS files + `preset-vars.ts`) | **Deleted.** CSS variables are declared inside a SPA Theme manifest |
| `custom_theme` entity + OKLCH editor + `/api/settings/themes` | **Deleted.** Authoring happens by writing `manifest.json` |
| Three coexisting theme mechanisms (preset / custom / SPA) | One: SPA Theme |
| Data subscriptions duplicated in every widget | All data flows through SDK hooks |

---

## 2. Widget SDK

The SDK is the single contract between widget authors and the host. Stability of this surface is the project's primary long-term commitment to widget authors.

### 2.1 `defineWidget`

```ts
// @serverbee/widget-sdk
import { defineWidget, z, useMetric } from '@serverbee/widget-sdk'

const ConfigSchema = z.object({
  serverId: z.serverId().describe('Target server'),
  metric: z.metricPath().default('cpu.usage'),
  threshold: z.number().min(0).max(100).default(80),
})

export default defineWidget({
  id: 'com.example.cpu-gauge',          // reverse-DNS, globally unique
  name: 'CPU Gauge',
  version: '1.0.0',
  category: 'Real-time',                // 'Real-time' | 'Charts' | 'Status'
  sizing: {
    defaultW: 3, defaultH: 3,
    minW: 2, minH: 2,
    strategy: 'aspect-square',           // 'fixed' | 'free' | 'aspect-square' | 'content-height'
  },
  requiredCaps: [],                      // e.g. ['CAP_DOCKER']
  configSchema: ConfigSchema,
  component: ({ config }) => {
    const value = useMetric(config.serverId, config.metric)
    return <Gauge value={value ?? 0} max={100} warn={config.threshold} />
  },
})
```

`defineWidget` returns a `WidgetModule` value. The loader expects each `.widget.js` to `export default` exactly one `WidgetModule`. A `.zip` collection may contain many files, each one widget.

### 2.2 Data subscription hooks

| Hook | Returns | Underlying source |
|---|---|---|
| `useServers()` | `ServerSummary[]` (id, name, online, lastSeen) | Reuses `useServersWs` store |
| `useServer(id)` | Full `ServerMetrics \| undefined` for one server | WS broadcast |
| `useMetric(id, path)` | Single value extracted by dot/bracket path (`'cpu.usage'`, `'disks[0].used'`) | Derived from `useServer` |
| `useHistory(id, path, range)` | `{ ts, value }[]` time series | TanStack Query → REST historical endpoint |
| `useCapability(id, cap)` | `boolean` | Derived from `useServer` capability bitmask |
| `useTheme()` | `{ mode: 'light' \| 'dark', cssVar: (name) => string }` | ThemeProvider context |
| `useConfigUpdate()` | `(patch: Partial<TConfig>) => void` | Dispatches into dashboard editor state |

All hooks accept `serverId | 'aggregate' | null`. With `null` they return `undefined`, letting components render a uniform placeholder.

### 2.3 `configSchema` → auto-generated form

The SDK ships a Zod-like mini-validator (`z`) with the standard primitives plus four widget-specific extensions:

- `z.serverId()` — renders a server picker
- `z.metricPath()` — renders a metric-path picker (browses the live ServerMetrics shape)
- `z.color()` — renders a color picker
- `z.duration()` — renders a duration input (e.g. `5m`, `1h`)

`renderConfigForm(schema, value, onChange)` consumes the schema and produces the entire ConfigDialog UI. Widget authors write no form code.

Rationale for not pulling full `zod`: keeps SDK runtime under ~10KB gzipped, avoids two zod versions colliding when authors also use zod in their own code.

### 2.4 Component contract

```ts
type WidgetProps<TConfig> = {
  config: TConfig                        // validated + defaulted
  size: { w: number; h: number }         // current grid pixel size
  isEditing: boolean                     // dashboard is in edit mode
}
```

No additional globals. All host capabilities are exposed through SDK hooks. Widgets may freely use React 19, JSX, and any pure-frontend npm dependencies they bundle themselves.

### 2.5 SDK runtime injection

```html
<!-- index.html -->
<script type="importmap">
{ "imports": { "@serverbee/widget-sdk": "/runtime/widget-sdk.js" } }
</script>
```

`/runtime/widget-sdk.js` is a thin shim emitted during SPA build that re-exports `globalThis.__SERVERBEE_SDK__`. The shim's URL is versioned (`/runtime/widget-sdk-{hash}.js`) so SPA upgrades invalidate the import-map entry deterministically.

Effect:
- Widget authors `import { ... } from '@serverbee/widget-sdk'` like any npm package, with full TypeScript type-completion.
- At runtime each loaded widget shares the host's React/SDK singletons — no duplicate React copies, no hook-rule violations.
- Updating the SDK ships once with the main SPA; all widgets follow.

### 2.6 Version compatibility

Each `defineWidget` call implicitly carries the `sdkVersion` it was built against (injected by the SDK build). The Registry validates on registration:

- Major mismatch → reject load, surface error in admin UI ("widget built for SDK v2, host is v3 — please update widget").
- Minor older → load with `console.warn`.
- Patch difference → silent.

Deprecations live for at least one minor cycle before removal.

---

## 3. Distribution & Installation

### 3.1 `widget_module` table

```rust
pub struct Model {
    pub id: String,                  // 'com.example.cpu-gauge'
    pub version: String,             // semver
    pub source_type: SourceType,     // Builtin | Url | Upload
    pub source_url: Option<String>,  // original URL for Url, original filename for Upload
    pub manifest_json: String,       // cached metadata extracted at install time
    pub code_sha256: String,         // content fingerprint
    pub code_blob: Vec<u8>,          // ESM bytes; empty for Builtin (served via rust-embed)
    pub installed_by: Option<i64>,   // user_id, null for Builtin
    pub installed_at: DateTimeWithTimeZone,
    pub enabled: bool,
}
```

Decision: **widget code lives in SQLite, not on a remote CDN.** This keeps offline VPS installs functional, lets the backend serve with cache headers, prevents silent upstream tampering after install, and gives admins one place to audit.

### 3.2 Loader sequence

```
1. SPA boots → GET /api/widgets
   → returns [{ id, version, manifest, code_url, sha256 }, ...]
2. For each module in parallel:
     fetch(code_url)                          // cache-friendly, etag = sha256
     blob = new Blob([text], { type: 'text/javascript' })
     mod  = await import(URL.createObjectURL(blob))
     Registry.register(mod.default)
3. Failures are isolated: a broken module surfaces in the admin UI as
   "Failed to load" without blocking other widgets or the dashboard.
```

### 3.3 Installation paths

| Source | Admin UX | Backend behavior |
|---|---|---|
| **Builtin** | n/a — registered at server boot from rust-embed | `INSERT … ON CONFLICT(id) DO UPDATE` on every boot |
| **URL** | Settings → Widgets → "Add by URL" → paste raw JS URL → backend fetches → preview manifest → confirm install | Server-side fetch with allowlist for `http(s)`, body size cap, MIME check; static ESM parse (via `oxc` or `swc`) to extract `defineWidget({...})` literal for manifest |
| **Upload (.js)** | Settings → Widgets → drag file → preview manifest → confirm install | Same parse pipeline as URL, skipping fetch |
| **Upload (.zip)** | Settings → Widgets → drag zip → preview collection (list of widgets) → confirm install | Unzip with zip-slip guard, read `collection.json`, register each listed `.widget.js` |

### 3.4 Zip collection format

```
my-collection.zip
├── collection.json
├── widgets/
│   ├── cpu-gauge.widget.js
│   ├── mem-chart.widget.js
│   └── disk-io.widget.js
└── assets/                  # optional static assets (icons etc.)
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

Each listed file must be valid ESM exporting a single `defineWidget({ id, version, ... })`. The backend extracts the id and version statically and refuses zips with duplicate ids.

### 3.5 Permissions

- **Admin** — install, uninstall, enable/disable widget modules; install via URL or upload.
- **Member** — see installed widgets in the picker, use them on dashboards. Cannot install/uninstall.
- `requiredCaps` is enforced at render time. When the selected server lacks a capability, the widget renders a disabled placeholder card explaining which capability is missing.

### 3.6 Marketplace (out of scope)

No official marketplace. Documentation maintains a hand-curated "Community Widgets" page with example URLs. Future extension point: a `marketplace_url` setting whose JSON manifest the SPA can iterate over for browsing — not built now.

---

## 4. Theme System Consolidation

### 4.1 What gets deleted

| Path | Action |
|---|---|
| `apps/web/src/themes/` (entire directory: 8 preset `.css` files, `preset-vars.ts`, `index.ts`) | Delete |
| `apps/web/src/api/themes.ts` | Delete |
| `apps/web/src/components/theme/` (theme-card, theme-editor, theme-preview, oklch-picker, delete-theme-dialog) | Delete |
| `apps/web/src/routes/_authed/settings/appearance/themes.*` (all custom-theme subroutes) | Delete |
| `apps/web/src/lib/theme-ref.ts` (the `preset:` / `custom:` prefix parser) | Delete |
| `crates/server/src/entity/custom_theme.rs` | Delete |
| Custom theme CRUD in `crates/server/src/router/api/theme.rs` | Delete (leave file if it still hosts unrelated handlers; otherwise remove) |
| sea-orm migration to `DROP TABLE custom_theme` | New migration; `down()` is a no-op per project convention |

### 4.2 What stays

- `apps/web/src/components/spa-theme/` (SPA Theme admin UI) — primary path going forward
- `apps/web/src/api/spa-themes.ts`
- `crates/server/src/entity/spa_theme.rs`
- `ThemeProvider` — heavily simplified

### 4.3 SPA Theme manifest, extended

```json
{
  "id": "com.example.dark-night",
  "name": "Dark Night",
  "version": "1.2.0",
  "author": "Jane Doe",
  "description": "Dark monochrome with custom gauge widgets.",

  "cssVars": {
    "light": {
      "--background": "oklch(1 0 0)",
      "--foreground": "oklch(0.145 0 0)",
      "--primary":    "oklch(0.205 0 0)"
    },
    "dark": {
      "--background": "oklch(0.145 0 0)",
      "--foreground": "oklch(0.985 0 0)",
      "--primary":    "oklch(0.985 0 0)"
    }
  },

  "widgets": [
    "widgets/special-gauge.widget.js"
  ],

  "preview": "preview.png"
}
```

A theme may declare **only** `cssVars` (pure recolor), **only** `widgets` (a widget pack), or both. This unifies the two contributor mental models — "I want a different look" and "I want new widgets" — into a single artifact and a single upload UX.

Zip structure:

```
my-theme.zip
├── manifest.json
├── preview.png            (optional)
└── widgets/               (optional, present when manifest.widgets non-empty)
    └── special-gauge.widget.js
```

### 4.4 Simplified `ThemeProvider`

```ts
type ThemeContext = {
  mode: 'light' | 'dark' | 'system'      // localStorage 'theme-mode'
  setMode: (m) => void
  activeSpaThemeId: string | null         // server-side via /api/settings/active-spa-theme
}
```

Activation flow:

1. SPA boots → `GET /api/settings/active-spa-theme` → load manifest.
2. Inject `manifest.cssVars.light` into `<style>` keyed under `:root`.
3. Inject `manifest.cssVars.dark` keyed under `.dark`.
4. If no SPA theme is active, fall back to the SPA's built-in default CSS variables (in `apps/web/src/index.css`, unchanged).
5. Light/dark switching toggles `html.dark`; variables are already in place.

Complexity removed:

- `theme-ref.ts` `preset:foo` / `custom:bar` dual-prefix resolver — gone, single uuid space.
- Dynamic loading of `themes/{id}.css` files — gone, vars come from manifest.
- OKLCH editor and `custom_theme` CRUD — gone.
- Two-layer activation state (mode × colorTheme) — collapsed to one (active SPA theme already carries both light and dark variable sets).

---

## 5. Built-in Widget Rewrite

All 14 built-in widgets are rewritten to the new SDK shape:

`stat-number`, `metric-card`, `server-cards`, `gauge`, `line-chart`, `multi-line`, `top-n`, `alert-list`, `service-status`, `traffic-bar`, `disk-io`, `server-map`, `markdown`, `uptime-timeline`.

Each becomes a `.widget.tsx` file in `apps/web/src/builtin-widgets/`, exporting `default defineWidget({...})`. At server startup, `rust-embed` includes them, and the boot routine upserts a `widget_module` row per file with `source_type = Builtin`.

The rewrite serves two purposes:

1. **Dogfooding the SDK** — if any of the 14 existing widgets cannot be expressed cleanly via `defineWidget` + the hook surface, the SDK API is wrong and must be extended.
2. **Eliminating the dual code path** — once rewritten, the old `widget-types.ts` registry, `widget-renderer.tsx` type switching, and `widget-config-dialog.tsx` per-type forms are deleted.

Per project convention (no data migrations during dev), the `dashboard_widget.widget_type` column is renamed to `module_id` in a schema-only migration. No conversion of existing rows is performed; developers re-create their local dashboards after the rename.

---

## 6. Documentation Deliverables (`apps/docs`)

Bilingual (zh / en), one MDX file per language under `apps/docs/content/docs/{cn,en}/`:

| Doc | Contents |
|---|---|
| `widgets/overview.mdx` | Widget system overview: module concept, source types, builtin vs custom, SDK conceptual diagram |
| `widgets/single-file-guide.mdx` | **Approach B full tutorial** — write a `.widget.js` from scratch: `npm init` → install SDK → `defineWidget` → local dev/preview → install into ServerBee via URL. Linked runnable example repo |
| `widgets/collection-guide.mdx` | **Approach C full tutorial** — zip collection structure, `collection.json` schema, sharing utils across widgets, local packing script, upload & install flow |
| `widgets/sdk-reference.mdx` | Full SDK API reference: `defineWidget` signature, every hook signature + return type + example, every `z.*` type + extensions |
| `widgets/configuration-schema.mdx` | `configSchema` deep dive: using `z.serverId()` / `z.metricPath()` / `z.color()` / `z.duration()` to get auto-generated forms |
| `widgets/sizing.mdx` | Sizing strategy guide (`fixed` / `free` / `aspect-square` / `content-height`) with use-case examples |
| `widgets/capabilities.mdx` | `requiredCaps` usage and the "capability missing on this server" rendering contract |
| `widgets/publishing.mdx` | Sharing a widget: hosting raw JS (GitHub raw / Gist / self-hosted), versioning, getting listed on the community page |
| `themes/spa-theme-guide.mdx` | Rewritten SPA Theme docs: complete manifest field reference, combined `cssVars` + `widgets` usage, relationship with widget collections |

Side-bar navigation is restructured to introduce a top-level **Widgets** section. Existing pages referencing the deleted preset/custom-theme features are rewritten or removed.

**Example repo:** a separate `serverbee-widget-examples` GitHub repository hosts three reference widgets: `hello-world.widget.js`, `metric-gauge.widget.js`, and a small `example-pack.zip` collection. The documentation links to it.

---

## 7. Testing Strategy

| Layer | Coverage |
|---|---|
| SDK unit tests (vitest, `apps/web`) | `defineWidget` validation, `renderConfigForm` snapshot tests, `z` schema parsing, SDK version compatibility decisions |
| Widget Registry unit tests | `register` / `get` / `list` / `unregister`, id collision, version conflict, enable/disable |
| Loader unit tests | Blob URL generation, import failure isolation, import-map shim resolving to host SDK |
| Backend unit tests (Rust) | `widget_module` CRUD, URL fetch path validation (allowlist + size cap), zip extraction safety (zip-slip), ESM static parse extracting `defineWidget({...})` metadata |
| Backend integration tests | Full lifecycle: upload zip → list → activate → render via SPA → uninstall |
| Built-in widget regression | One minimal render test per rewritten widget (mock SDK hooks) to guard against regressions during the rewrite |
| E2E manual checklist | `tests/dashboard-widgets-v2.md`: URL install, zip upload, configure widget, all 14 built-ins render, theme switch does not break loaded widgets |

**Not in scope:** iframe sandbox tests (trusted-admin model, no sandbox); marketplace tests (no marketplace).

---

## 8. Implementation Phasing

Detailed in separate plan documents under `docs/superpowers/plans/`. Preview:

1. **Plan 1 — SDK + Registry foundation**
   - `@serverbee/widget-sdk` package skeleton, type definitions, `defineWidget`, `z` mini-validator
   - Frontend Registry + Loader + import-map shim
   - Backend `widget_module` entity + migration + boot-time builtin registration
   - Unit-test coverage of the new surfaces

2. **Plan 2 — Rewrite the 14 built-in widgets**
   - One `.widget.tsx` per widget under `apps/web/src/builtin-widgets/`
   - Rename `dashboard_widget.widget_type` → `module_id` (schema only, no data migration)
   - Rewrite `WidgetRenderer` against the Registry; delete `widget-types.ts`, the old `widget-config-dialog.tsx` per-type forms
   - One render snapshot per widget

3. **Plan 3 — Custom widget install + SPA Theme manifest extension + delete legacy theme code**
   - URL install + .js upload + .zip collection install UX
   - Backend: URL fetch with allowlist, zip parsing with zip-slip guard, static ESM parse for manifest extraction
   - Extend SPA Theme manifest with `cssVars` + `widgets`; update activation pipeline
   - Simplify `ThemeProvider`
   - Delete `apps/web/src/themes/`, `custom_theme` entity, theme editor UI, related routes/APIs

4. **Plan 4 — Documentation + example repo**
   - The 9 MDX pages above, in both `cn` and `en`
   - First version of `serverbee-widget-examples` repo with 3 examples
   - Docs site sidebar restructure

Order: Plan 1 → Plan 2 → Plan 3. Plan 4 can proceed in parallel with Plan 3.

---

## 9. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Older browsers do not support import-maps | Require Chrome ≥ 89 / Safari ≥ 16.4 / Firefox ≥ 108. Document in install requirements. Current ServerBee user base already runs modern browsers |
| A broken custom widget could break the dashboard | `WidgetRenderer` wraps each instance in an error boundary; loader catches `import()` failures per module and surfaces a placeholder card so other widgets keep rendering |
| SDK API churn breaks already-installed widgets | Strict SemVer on `sdkVersion`; loader rejects major-mismatch with a clear admin message; deprecations live for at least one minor version |
| Rewriting the 14 widgets introduces visual regressions | A render snapshot per widget is written **before** the rewrite begins; reviewer diffs snapshots after rewrite |
| Static ESM parse cannot extract `defineWidget({...})` from minified code | Documented requirement: published widgets ship as unminified ESM with the `defineWidget` call at top-level (the bundling examples in `single-file-guide.mdx` emit this format) |
| Remote URL install could be abused for SSRF | Allowlist `http(s)` only; reject private/loopback ranges; cap body size; record `installed_by` |
| Zip-slip on collection upload | Use a vetted zip crate that validates entry names; reject any entry whose canonicalised path escapes the extraction root |
