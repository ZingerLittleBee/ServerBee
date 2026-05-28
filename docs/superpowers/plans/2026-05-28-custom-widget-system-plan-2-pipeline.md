# Custom Widget System — Plan 2: Build Pipeline + Real Runtime Wire-up

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development.

**Scope:** Make the SDK + Registry + asset endpoints from Plan 1 actually do something visible. Add one example built-in widget, wire the Vite multi-entry pipeline that ships it, write the Rust boot routine that registers it, and replace the runtime bridge's stub stores with the real `useServersWs` + theme + dashboard editor wiring. Rewriting the existing 14 widgets is **deferred** to a follow-up — Plan 2 lands the substrate that future PRs can rewrite against.

**Goal:** After Plan 2, a fresh server boot registers one `Builtin` widget module, the SPA loads it through `/api/widget-modules/...`, and the dashboard editor can drop it onto a dashboard and see live data flowing.

**Architecture:** Vite multi-entry emits `apps/web/dist/builtin-widgets/<id>/index.js` + `manifest.json`. Rust embeds that directory and upserts a `widget_module` row per manifest entry on every boot (`source_type = Builtin`, `package_blob = NULL`, served from rust-embed). The asset endpoint special-cases Builtin sources. A new `WidgetRendererV2` reads `dashboard_widget.module_id`, finds the module in the Registry, and renders the SDK `component`. The existing legacy `WidgetRenderer` keeps working for old `widget_type` rows (parallel until Plan 2 followup deletes them).

**Tech Stack:** Vite 7 plugin API · rust-embed · Axum static file serving · React 19 + zustand

**Spec:** `docs/superpowers/specs/2026-05-28-custom-widget-system-design.md`
**Prior plan:** `docs/superpowers/plans/2026-05-28-custom-widget-system-plan-1-sdk.md`

---

## Conventions

- All built-in widgets live at `apps/web/src/builtin-widgets/*.widget.tsx`.
- Generated manifest at `apps/web/dist/builtin-widgets/manifest.json` is consumed by Rust.
- New parallel dashboard widget id column `module_id` is **added** in this plan (next to `widget_type`); legacy column stays until Plan 3 cleanup.

---

## Task 1: Add example built-in widget source

**Files:**
- Create: `apps/web/src/builtin-widgets/hello-world.widget.tsx`
- Create: `apps/web/src/builtin-widgets/.gitkeep` (ensure directory tracked)

- [ ] **Step 1: Write the widget file**

`apps/web/src/builtin-widgets/hello-world.widget.tsx`:

```tsx
/**
 * @serverbee-widget {
 *   "id": "com.serverbee.hello-world",
 *   "version": "1.0.0",
 *   "name": "Hello World",
 *   "description": "A minimal builtin widget that displays the SPA theme mode and the count of online servers.",
 *   "author": "ServerBee",
 *   "category": "Real-time",
 *   "sizing": { "defaultW": 3, "defaultH": 2, "minW": 2, "minH": 2, "strategy": "free" },
 *   "sdkVersion": "^0.1.0"
 * }
 */
import { defineWidget, useServers, useTheme, z } from '@serverbee/widget-sdk'

const ConfigSchema = z.object({
  greeting: z.string().describe('Greeting text').default('Hello, ServerBee'),
})

export default defineWidget({
  configSchema: ConfigSchema,
  component: ({ config }) => {
    const servers = useServers()
    const theme = useTheme()
    const online = servers.filter((s) => s.online).length
    return (
      <div style={{ padding: 12, fontFamily: 'system-ui' }}>
        <div style={{ fontSize: 16, fontWeight: 600 }}>{config.greeting}</div>
        <div style={{ marginTop: 8, color: 'var(--muted-foreground)' }}>
          {online} / {servers.length} online · {theme.mode} mode
        </div>
      </div>
    )
  },
})
```

- [ ] **Step 2: Commit**

```bash
git add apps/web/src/builtin-widgets
git -c commit.gpgsign=false commit -m "feat(web): add hello-world example builtin widget"
```

---

## Task 2: Vite plugin to emit builtin-widgets manifest

**Files:**
- Create: `apps/web/vite-plugins/builtin-widgets.ts`
- Modify: `apps/web/vite.config.ts`

- [ ] **Step 1: Write the plugin**

`apps/web/vite-plugins/builtin-widgets.ts`:

```ts
import { readFileSync } from 'node:fs'
import path from 'node:path'
import { globSync } from 'tinyglobby'
import type { Plugin } from 'vite'

interface BuiltinManifestEntry {
  id: string
  version: string
  entry_path: string
  manifest: Record<string, unknown>
}

const JSDOC_RE = /\/\*\*[\s\S]*?@serverbee-widget\s+(\{[\s\S]*?\})\s*\*\//
const LINE_DECOR = /^\s*\*\s?/gm

function extractManifest(source: string): Record<string, unknown> {
  const m = source.match(JSDOC_RE)
  if (!m) throw new Error('no @serverbee-widget JSDoc block')
  return JSON.parse(m[1].replace(LINE_DECOR, ''))
}

export function builtinWidgetsPlugin(): Plugin {
  const SRC_DIR = 'src/builtin-widgets'
  const entries = new Map<string, { srcPath: string; manifest: Record<string, unknown> }>()

  return {
    name: 'serverbee-builtin-widgets',
    config(userConfig) {
      const files = globSync(`${SRC_DIR}/*.widget.tsx`, { absolute: false })
      for (const file of files) {
        const id = path.basename(file, '.widget.tsx') // e.g. 'hello-world'
        const source = readFileSync(file, 'utf8')
        const manifest = extractManifest(source)
        entries.set(id, { srcPath: file, manifest })
      }
      const inputs: Record<string, string> = { main: 'index.html' }
      for (const [id, e] of entries) {
        inputs[`builtin-widgets/${id}/index`] = path.resolve(e.srcPath)
      }
      const existing = userConfig.build?.rollupOptions?.input
      const merged =
        typeof existing === 'string' || Array.isArray(existing)
          ? inputs
          : { ...(existing as Record<string, string>), ...inputs }
      return {
        build: {
          rollupOptions: {
            input: merged,
            external: ['react', 'react-dom', 'react/jsx-runtime', '@serverbee/widget-sdk'],
            output: {
              entryFileNames: (chunk: any) =>
                chunk.name?.startsWith('builtin-widgets/')
                  ? `${chunk.name}.js`
                  : 'assets/[name]-[hash].js',
            },
          },
        },
      }
    },
    generateBundle() {
      const list: BuiltinManifestEntry[] = []
      for (const [id, e] of entries) {
        const m = e.manifest as { id: string; version: string }
        list.push({
          id: m.id,
          version: m.version,
          entry_path: `${id}/index.js`,
          manifest: e.manifest,
        })
      }
      this.emitFile({
        type: 'asset',
        fileName: 'builtin-widgets/manifest.json',
        source: JSON.stringify(list, null, 2),
      })
    },
  }
}
```

- [ ] **Step 2: Add `tinyglobby` to apps/web devDependencies if not already present**

Check: `grep tinyglobby apps/web/package.json`. If missing: `cd apps/web && bun add -d tinyglobby`.

- [ ] **Step 3: Wire plugin into vite.config.ts**

In `apps/web/vite.config.ts`:
- `import { builtinWidgetsPlugin } from './vite-plugins/builtin-widgets'`
- Add `builtinWidgetsPlugin()` to the `plugins` array (after `react()`)

- [ ] **Step 4: Verify build emits the artifacts**

Run: `cd apps/web && bun run build`
After build:
- `ls apps/web/dist/builtin-widgets/` → should contain `manifest.json` and `hello-world/index.js`
- `cat apps/web/dist/builtin-widgets/manifest.json` → contains the hello-world entry with the manifest from the JSDoc block
- `head -3 apps/web/dist/builtin-widgets/hello-world/index.js` → should reference imports from `react` / `@serverbee/widget-sdk` (NOT bundle them)

- [ ] **Step 5: Commit**

```bash
git add apps/web/vite-plugins apps/web/vite.config.ts apps/web/package.json bun.lock
git -c commit.gpgsign=false commit -m "build(web): vite plugin emits builtin widget bundles + manifest"
```

---

## Task 3: Rust — register builtin widgets at server boot

**Files:**
- Create: `crates/server/src/service/widget_module/builtin.rs`
- Modify: `crates/server/src/service/widget_module/mod.rs`
- Modify: `crates/server/src/router/static_files.rs` (special-case Builtin in asset serve) — see Task 4
- Modify: `crates/server/src/main.rs` (or wherever `migrate_database` / startup runs) — invoke `register_all` after migrations

- [ ] **Step 1: Write registration routine**

`crates/server/src/service/widget_module/builtin.rs`:

```rust
use chrono::Utc;
use rust_embed::Embed;
use sea_orm::{ActiveValue::Set, DatabaseConnection, EntityTrait};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::entity::widget_module::{self, Entity as WidgetModuleEntity, SourceType};

/// Reads `dist/builtin-widgets/manifest.json` from the embedded SPA assets.
#[derive(Embed)]
#[folder = "../../apps/web/dist/builtin-widgets"]
#[include = "**/*.js"]
#[include = "manifest.json"]
struct BuiltinAssets;

#[derive(Debug, Deserialize)]
struct ManifestEntry {
    id: String,
    version: String,
    entry_path: String,
    manifest: serde_json::Value,
}

pub async fn register_all(db: &DatabaseConnection) -> anyhow::Result<()> {
    let raw = BuiltinAssets::get("manifest.json")
        .ok_or_else(|| anyhow::anyhow!("dist/builtin-widgets/manifest.json missing (run `bun run build` first)"))?;
    let entries: Vec<ManifestEntry> = serde_json::from_slice(raw.data.as_ref())?;

    for entry in &entries {
        let code = BuiltinAssets::get(&entry.entry_path)
            .ok_or_else(|| anyhow::anyhow!("builtin entry missing: {}", entry.entry_path))?;
        let sha = {
            let mut h = Sha256::new();
            h.update(code.data.as_ref());
            format!("{:x}", h.finalize())
        };

        let active = widget_module::ActiveModel {
            id: Set(entry.id.clone()),
            version: Set(entry.version.clone()),
            source_type: Set(SourceType::Builtin),
            source_url: Set(None),
            bundled_by_theme_id: Set(None),
            manifest_json: Set(serde_json::to_string(&entry.manifest)?),
            code_sha256: Set(sha),
            entry_path: Set(entry.entry_path.clone()),
            package_blob: Set(None),
            installed_by: Set(None),
            installed_at: Set(Utc::now()),
            enabled: Set(true),
        };

        // Upsert by primary key
        use sea_orm::sea_query::OnConflict;
        WidgetModuleEntity::insert(active)
            .on_conflict(
                OnConflict::column(widget_module::Column::Id)
                    .update_columns([
                        widget_module::Column::Version,
                        widget_module::Column::ManifestJson,
                        widget_module::Column::CodeSha256,
                        widget_module::Column::EntryPath,
                        widget_module::Column::InstalledAt,
                        widget_module::Column::Enabled,
                    ])
                    .to_owned(),
            )
            .exec(db)
            .await?;
    }

    // Disable any stale Builtin rows not present in the current manifest.
    use sea_orm::{ColumnTrait, QueryFilter};
    let active_ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    let mut delete = WidgetModuleEntity::delete_many()
        .filter(widget_module::Column::SourceType.eq(SourceType::Builtin));
    if !active_ids.is_empty() {
        delete = delete.filter(widget_module::Column::Id.is_not_in(active_ids));
    }
    delete.exec(db).await?;

    Ok(())
}

pub fn builtin_asset_bytes(entry_path: &str) -> Option<Vec<u8>> {
    BuiltinAssets::get(entry_path).map(|f| f.data.into_owned())
}
```

- [ ] **Step 2: Register module**

In `crates/server/src/service/widget_module/mod.rs`, add:
```rust
pub mod builtin;
```

- [ ] **Step 3: Invoke at startup**

Find where the server runs migrations (`Migrator::up(&db, None).await?` — likely in `main.rs` or a `start_server` helper). Immediately after migrations, add:

```rust
crate::service::widget_module::builtin::register_all(&db).await?;
```

If the surrounding context returns a non-anyhow error type, map: `.map_err(|e| anyhow::anyhow!(e))?` or convert via the local error wrapper.

- [ ] **Step 4: Verify**

Build: `cargo build -p serverbee-server`.

If the build fails because `apps/web/dist/builtin-widgets/` does not exist yet (i.e. fresh checkout), the developer must run `cd apps/web && bun run build` first. Document this in `CLAUDE.md` build commands section. For now ensure the artifacts exist locally before continuing.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/widget_module crates/server/src/main.rs
git -c commit.gpgsign=false commit -m "feat(server): register builtin widgets on boot from embedded manifest"
```

---

## Task 4: Asset endpoint serves Builtin via rust-embed

**Files:**
- Modify: `crates/server/src/service/widget_module/service.rs`

- [ ] **Step 1: Branch on source_type**

Find `serve_asset` in service.rs. Modify to check `row.source_type`:
- If `SourceType::Builtin` → read bytes from `crate::service::widget_module::builtin::builtin_asset_bytes(requested)` (which already includes the full path like `hello-world/index.js`)
- Otherwise → existing BLOB unpacking logic

Replace the existing `serve_asset` with:

```rust
pub async fn serve_asset(
    db: &DatabaseConnection,
    id: &str,
    requested: &str,
) -> Result<(Vec<u8>, String), WidgetModuleError> {
    let row = Self::get(db, id).await?;

    if matches!(row.source_type, crate::entity::widget_module::SourceType::Builtin) {
        // Builtin: requested path is module-local (e.g. "index.js"). Builtin manifest
        // stores entry_path as e.g. "hello-world/index.js" — the URL `<id>/index.js`
        // means resolve `<requested>` relative to the module's folder.
        let folder = row.entry_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let full = if folder.is_empty() {
            requested.to_string()
        } else {
            format!("{folder}/{requested}")
        };
        if full.contains("..") {
            return Err(WidgetModuleError::InvalidAssetPath);
        }
        let bytes = crate::service::widget_module::builtin::builtin_asset_bytes(&full)
            .ok_or(WidgetModuleError::InvalidAssetPath)?;
        return Ok((bytes, mime_for(&full)));
    }

    let blob = row
        .package_blob
        .ok_or_else(|| WidgetModuleError::NotFound(format!("{id}: no blob")))?;

    let package = if blob.starts_with(b"PK\x03\x04") {
        UnpackedPackage::from_zip(&blob)?
    } else {
        UnpackedPackage::from_single_file(&row.entry_path, blob)
    };

    let bytes = package
        .get(requested)
        .ok_or(WidgetModuleError::InvalidAssetPath)?
        .to_vec();
    let mime = mime_for(requested);
    Ok((bytes, mime))
}
```

- [ ] **Step 2: Verify integration tests still pass**

Run: `cargo test -p serverbee-server --test widget_module_integration`
Expected: 3 tests still pass.

- [ ] **Step 3: Add a 4th test for Builtin path**

Append to `crates/server/tests/widget_module_integration.rs`:

```rust
#[tokio::test]
async fn list_includes_builtin_hello_world() {
    let ctx = start_test_server_with_db().await.expect("server");
    let client = reqwest::Client::new();
    let res = client
        .get(format!("{}/api/widget-modules", ctx.base_url))
        .header("cookie", &ctx.admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    let list = body["data"].as_array().expect("list");
    assert!(
        list.iter().any(|m| m["id"] == "com.serverbee.hello-world"),
        "expected hello-world in list, got {body:#?}"
    );
}
```

If the test helper doesn't already run register_all at startup (it should, via the normal main flow), the test will fail. Ensure `register_all` is called inside the test harness too — by virtue of running the real `create_router` + setup that the integration test already uses.

Run: `cargo test -p serverbee-server --test widget_module_integration` — 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/server
git -c commit.gpgsign=false commit -m "feat(server): serve Builtin widget assets via rust-embed"
```

---

## Task 5: dashboard_widget — add `module_id` column

**Files:**
- Create: `crates/server/src/migration/m20260528_000051_dashboard_widget_module_id.rs`
- Modify: `crates/server/src/migration/mod.rs`
- Modify: `crates/server/src/entity/dashboard_widget.rs`

- [ ] **Step 1: Write migration**

`crates/server/src/migration/m20260528_000051_dashboard_widget_module_id.rs`:

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE dashboard_widget ADD COLUMN module_id TEXT",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_dashboard_widget_module_id ON dashboard_widget(module_id)",
        )
        .await?;
        Ok(())
    }
    async fn down(&self, _m: &SchemaManager) -> Result<(), DbErr> { Ok(()) }
}
```

- [ ] **Step 2: Register migration + entity field**

- `crates/server/src/migration/mod.rs`: `mod m20260528_000051_dashboard_widget_module_id;` + append to migrations vec.
- `crates/server/src/entity/dashboard_widget.rs`: add `pub module_id: Option<String>,` field to the `Model` struct (next to `widget_type`).

- [ ] **Step 3: Verify**

`cargo build -p serverbee-server && cargo clippy -p serverbee-server -- -D warnings`

- [ ] **Step 4: Commit**

```bash
git add crates/server
git -c commit.gpgsign=false commit -m "feat(server): dashboard_widget.module_id column (alongside widget_type)"
```

---

## Task 6: Front-end — real `mountRuntimeBridge` wiring

**Files:**
- Modify: `apps/web/src/widgets-runtime/runtime-bridge.ts`
- Modify: `apps/web/src/main.tsx`

- [ ] **Step 1: Inspect existing servers store**

Read `apps/web/src/hooks/use-servers-ws.ts` and find:
- The exposed hook that returns the array of ServerMetrics
- The exact shape of a ServerMetrics object (we need `id`, `name`, `online`, capabilities bitmask)

The `mountRuntimeBridge` callbacks need to be CALLED at hook-time (because they call zustand subscriptions). Since `serversStore: () => ServerSummary[]` is a plain function (not a React hook), we can't put a `useSyncExternalStore` in there directly. Solution: expose zustand store via `useServersWsStore.getState().servers` (raw snapshot) and map to ServerSummary.

If the servers store doesn't have a `useStore.getState()` access (i.e. it's not a zustand store), we may need to expose a side-channel ref that the React tree updates. Inspect first; report findings.

Strategy (most likely works):
```ts
import { useServersWsStore } from '@/hooks/use-servers-ws'  // confirm actual export

serversStore: () => {
  const snap = useServersWsStore.getState()
  return snap.servers.map((s) => ({
    id: s.id,
    name: s.name ?? s.id,
    online: s.online,
    lastSeen: s.last_seen ? Date.parse(s.last_seen) : null,
    capabilities: s.capabilities ?? 0,
  }))
},
serverByIdStore: (id) => useServersWsStore.getState().servers.find((s) => s.id === id),
```

If `use-servers-ws.ts` exports something different (a TanStack Query hook, or a Context), instead expose the snapshot via a module-level mutable ref that gets updated by a `useEffect` in the layout. Adapt as needed.

- [ ] **Step 2: Wire theme**

In runtime-bridge.ts, the `themeStore` callback already works (reads `html.dark` class). Keep as-is.

- [ ] **Step 3: Wire onConfigUpdate**

This requires the dashboard editor to expose an update method. For Plan 2 keep as a stub (Plan 2-followup will wire it):

```ts
onConfigUpdate: (instanceId, patch) => {
  console.warn('onConfigUpdate not yet wired', instanceId, patch)
},
```

- [ ] **Step 4: Verify**

`cd apps/web && bun run typecheck && bun run test` — all pass.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src
git -c commit.gpgsign=false commit -m "feat(web): wire real servers store into widget runtime bridge"
```

---

## Task 7: WidgetRendererV2 — render modules by `module_id`

**Files:**
- Create: `apps/web/src/components/dashboard/widget-renderer-v2.tsx`
- Create: `apps/web/src/components/dashboard/widget-renderer-v2.test.tsx`

- [ ] **Step 1: Write minimal test**

`apps/web/src/components/dashboard/widget-renderer-v2.test.tsx`:

```tsx
import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import { useWidgetRegistry } from '@/widgets-runtime/registry'
import type { WidgetModule, WidgetManifest } from '@serverbee/widget-sdk'
import { WidgetRendererV2 } from './widget-renderer-v2'

const manifest: WidgetManifest = {
  id: 'com.test.t',
  version: '1.0.0',
  name: 'T',
  category: 'Real-time',
  sizing: { defaultW: 2, defaultH: 2, minW: 1, minH: 1, strategy: 'free' },
  sdkVersion: '^0.1.0',
}
const widgetModule: WidgetModule = {
  __brand: 'WidgetModule',
  configSchema: {} as any,
  component: ({ config }: any) => <div>greeting: {config?.text ?? 'none'}</div>,
  actions: [],
}

describe('WidgetRendererV2', () => {
  beforeEach(() => {
    useWidgetRegistry.setState({ modules: new Map(), failures: new Map() })
  })

  it('renders the registered module component with parsed config', () => {
    useWidgetRegistry.setState({
      modules: new Map([['com.test.t', { manifest, module: widgetModule }]]),
      failures: new Map(),
    })
    render(<WidgetRendererV2 moduleId="com.test.t" configJson='{"text":"hi"}' isEditing={false} size={{ w: 200, h: 100 }} />)
    expect(screen.getByText('greeting: hi')).toBeInTheDocument()
  })

  it('renders a placeholder when module is not registered', () => {
    render(<WidgetRendererV2 moduleId="com.test.missing" configJson="{}" isEditing={false} size={{ w: 200, h: 100 }} />)
    expect(screen.getByText(/not loaded/i)).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run — FAIL**

`cd apps/web && bunx vitest run src/components/dashboard/widget-renderer-v2.test.tsx`

- [ ] **Step 3: Implement**

`apps/web/src/components/dashboard/widget-renderer-v2.tsx`:

```tsx
import { Component, type ErrorInfo, type ReactNode, useMemo } from 'react'
import { createActionsHelper } from '@serverbee/widget-sdk'
import { useWidgetRegistry } from '@/widgets-runtime/registry'

interface Props {
  moduleId: string
  configJson: string
  isEditing: boolean
  size: { w: number; h: number }
}

class ErrorBoundary extends Component<{ children: ReactNode }, { hasError: boolean }> {
  state = { hasError: false }
  static getDerivedStateFromError() {
    return { hasError: true }
  }
  componentDidCatch(err: Error, info: ErrorInfo) {
    console.error('widget render error', err, info)
  }
  render() {
    return this.state.hasError ? (
      <div className="flex h-full items-center justify-center text-destructive text-sm">
        Widget crashed
      </div>
    ) : (
      this.props.children
    )
  }
}

export function WidgetRendererV2({ moduleId, configJson, isEditing, size }: Props) {
  const entry = useWidgetRegistry((s) => s.modules.get(moduleId))

  const config = useMemo(() => {
    try {
      const raw = JSON.parse(configJson || '{}')
      return entry?.module.configSchema?.parse?.(raw) ?? raw
    } catch (e) {
      console.warn('widget config parse failed', e)
      return {}
    }
  }, [configJson, entry])

  const actions = useMemo(
    () => createActionsHelper(entry?.module.actions ?? []),
    [entry?.module.actions],
  )

  if (!entry) {
    return (
      <div className="flex h-full items-center justify-center rounded-lg border bg-card p-3 text-muted-foreground text-sm">
        Widget module not loaded: {moduleId}
      </div>
    )
  }

  const Component = entry.module.component
  return (
    <ErrorBoundary>
      <Component config={config} size={size} isEditing={isEditing} actions={actions} />
    </ErrorBoundary>
  )
}
```

- [ ] **Step 4: Run — PASS (2 tests)**

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/dashboard/widget-renderer-v2.tsx apps/web/src/components/dashboard/widget-renderer-v2.test.tsx
git -c commit.gpgsign=false commit -m "feat(web): WidgetRendererV2 renders modules from Registry"
```

---

## Task 8: Final integration verification

- [ ] **Step 1: Full sequence sanity check**

```bash
cd apps/web && bun run build           # emits builtin-widgets/manifest.json + hello-world/index.js
cargo build -p serverbee-server         # embeds the dist directory
cargo test -p serverbee-server --test widget_module_integration  # 4 tests pass (incl. hello-world)
cd apps/web && bun run test             # frontend tests pass
cargo clippy --workspace -- -D warnings  # clean
```

- [ ] **Step 2: Manual smoke test (visual)**

If a local dev server can be started, do not start it here — the goal-driven workflow will do VPS-based verification later. Just verify the artifacts on disk.

- [ ] **Step 3: No commit** (already covered by per-task commits)

---

## Plan 2 — Completion Criteria

After Tasks 1–8:

- `apps/web/dist/builtin-widgets/manifest.json` lists the `com.serverbee.hello-world` module with the JSDoc manifest.
- `apps/web/dist/builtin-widgets/hello-world/index.js` exists, imports React + SDK as externals, exports a `WidgetModule` default.
- Rust server boot registers it as `widget_module(source_type='Builtin')`.
- `GET /api/widget-modules` returns the hello-world entry.
- `GET /api/widget-modules/com.serverbee.hello-world/index.js` returns the JS bytes with `Content-Type: text/javascript` + ETag.
- `mountRuntimeBridge` is wired to the real `useServersWs` store.
- `WidgetRendererV2` can render a module fetched from the Registry.
- `dashboard_widget.module_id` column exists (no rows reference it yet — wiring it into Dashboard Grid is left to Plan 2-followup or Plan 3).

What's intentionally deferred (NOT in Plan 2):

- Rewriting the 14 existing widgets to the new shape (parallel work, future PRs).
- Adding `WidgetPicker` v2 / `widget-config-dialog` v2.
- Hooking `WidgetRendererV2` into the actual `DashboardGrid` (current grid still uses legacy renderer).
- `onConfigUpdate` wiring (stub for now).
- Deleting legacy widget code (`widget-types.ts`, `WidgetRenderer`, per-type dialogs) — Plan 3 cleanup.
