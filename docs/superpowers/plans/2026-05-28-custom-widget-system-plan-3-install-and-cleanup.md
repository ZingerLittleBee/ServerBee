# Custom Widget System — Plan 3: Install UX + Legacy Theme Cleanup

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development.

**Scope:** Ship the install UX (URL install + single-file upload, zip collection deferred) so admins can actually add a widget. Delete the three legacy theme systems (spa_theme full-replacement, custom_theme OKLCH, preset themes) — these are obsolete now that Themes-as-cssVars-pack is the consolidation target. New Theme entity itself is deferred to a follow-up; this plan only removes the legacy code.

**Goal:** After Plan 3, an admin can paste a URL or upload a `.js` file with a valid `@serverbee-widget` JSDoc, see it install, and have it appear in `GET /api/widget-modules`. Legacy theme code is gone from the codebase.

**Architecture:** New backend route `POST /api/widget-modules` (admin-only, multipart form with `file` field OR `url` query parameter, runs JSDoc extractor, stores BLOB). Delete endpoint `DELETE /api/widget-modules/{id}` (Builtin rows protected). Frontend: a single new settings page lists modules with install button + delete button. Legacy theme deletion removes ~28 files including entity/service/router/UI for `spa_theme` and `custom_theme`.

**Spec:** `docs/superpowers/specs/2026-05-28-custom-widget-system-design.md`

---

## Section A — Backend install routes

## Task 1: `POST /api/widget-modules` — install from URL or upload

**Files:**
- Modify: `crates/server/src/router/api/widget_module.rs`
- Modify: `crates/server/src/service/widget_module/service.rs` (add `install_from_source`)

- [ ] **Step 1: Service method**

In `service.rs` add:

```rust
use chrono::Utc;
use sea_orm::ActiveValue::Set;
use sea_orm::ActiveModelTrait;
use sha2::{Digest, Sha256};

use super::extractor::extract_manifest;
use crate::entity::widget_module;

pub struct InstallInput {
    pub source: super::extractor_source::Source,
    pub installed_by: Option<i64>,
}

pub enum InstalledFrom { Url(String), Upload(String) }

impl WidgetModuleService {
    pub async fn install_single_file(
        db: &DatabaseConnection,
        code: Vec<u8>,
        from: InstalledFrom,
        installed_by: Option<i64>,
    ) -> Result<widget_module::Model, super::WidgetModuleError> {
        let source = std::str::from_utf8(&code)
            .map_err(|e| super::WidgetModuleError::ManifestValidation(format!("not utf-8: {e}")))?;
        let manifest = extract_manifest(source)?;

        let sha = {
            let mut h = Sha256::new();
            h.update(&code);
            format!("{:x}", h.finalize())
        };

        let (source_type, source_url) = match from {
            InstalledFrom::Url(u) => (super::super::super::entity::widget_module::SourceType::Url, Some(u)),
            InstalledFrom::Upload(name) => (super::super::super::entity::widget_module::SourceType::Upload, Some(name)),
        };

        let id_clone = manifest.id.clone();
        let version_clone = manifest.version.clone();

        let active = widget_module::ActiveModel {
            id: Set(manifest.id.clone()),
            version: Set(manifest.version.clone()),
            source_type: Set(source_type),
            source_url: Set(source_url),
            bundled_by_theme_id: Set(None),
            manifest_json: Set(serde_json::to_string(&manifest).unwrap()),
            code_sha256: Set(sha),
            entry_path: Set("index.js".into()),
            package_blob: Set(Some(code)),
            installed_by: Set(installed_by),
            installed_at: Set(Utc::now()),
            enabled: Set(true),
        };

        use sea_orm::EntityTrait;
        use sea_orm::sea_query::OnConflict;
        widget_module::Entity::insert(active)
            .on_conflict(
                OnConflict::column(widget_module::Column::Id)
                    .update_columns([
                        widget_module::Column::Version,
                        widget_module::Column::SourceType,
                        widget_module::Column::SourceUrl,
                        widget_module::Column::ManifestJson,
                        widget_module::Column::CodeSha256,
                        widget_module::Column::PackageBlob,
                        widget_module::Column::InstalledBy,
                        widget_module::Column::InstalledAt,
                        widget_module::Column::Enabled,
                    ])
                    .to_owned(),
            )
            .exec(db)
            .await
            .map_err(super::WidgetModuleError::Db)?;

        Self::get(db, &id_clone).await
    }

    pub async fn uninstall(
        db: &DatabaseConnection,
        id: &str,
    ) -> Result<(), super::WidgetModuleError> {
        use sea_orm::EntityTrait;
        let row = Self::get(db, id).await?;
        if matches!(row.source_type, crate::entity::widget_module::SourceType::Builtin) {
            return Err(super::WidgetModuleError::ManifestValidation("cannot uninstall builtin".into()));
        }
        widget_module::Entity::delete_by_id(id.to_string()).exec(db).await?;
        Ok(())
    }
}
```

The unused `InstallInput` / `extractor_source` references above are illustrative — drop them and inline the source kind. Adjust paths if needed; the signatures that matter are:
- `install_single_file(db, code, from, installed_by) -> Result<widget_module::Model, WidgetModuleError>`
- `uninstall(db, id) -> Result<(), WidgetModuleError>`

- [ ] **Step 2: Routes**

In `router/api/widget_module.rs`, add:

```rust
use axum::extract::{Multipart, Query};
use axum::routing::{delete, post};
use serde::Deserialize;

use crate::middleware::auth::CurrentUser;

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/widget-modules", post(install_module))
        .route("/widget-modules/{id}", delete(uninstall_module))
}

#[derive(Debug, Deserialize)]
struct InstallQuery {
    url: Option<String>,
}

#[utoipa::path(
    post, path = "/api/widget-modules", tag = "widget-modules",
    responses((status = 200, description = "Installed module")),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn install_module(
    State(state): State<Arc<AppState>>,
    user: axum::extract::Extension<CurrentUser>,
    Query(q): Query<InstallQuery>,
    mut multipart: Option<Multipart>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let user_id = user.0.user_id.parse::<i64>().ok();

    // Resolve code bytes either from `url` query param (URL install) or multipart `file` field.
    let (bytes, from) = if let Some(url) = q.url.clone() {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(AppError::BadRequest("url must be http(s)".into()));
        }
        // Reject private/loopback ranges
        let parsed = url::Url::parse(&url).map_err(|e| AppError::BadRequest(format!("bad url: {e}")))?;
        if let Some(host) = parsed.host_str() {
            if host == "localhost" || host == "127.0.0.1" || host.starts_with("10.") || host.starts_with("192.168.") {
                return Err(AppError::BadRequest("private/loopback urls rejected".into()));
            }
        }
        let resp = reqwest::Client::new()
            .get(&url)
            .send().await
            .map_err(|e| AppError::BadRequest(format!("fetch: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::BadRequest(format!("fetch {}: {}", url, resp.status())));
        }
        let bytes = resp.bytes().await
            .map_err(|e| AppError::Internal(format!("read body: {e}")))?
            .to_vec();
        if bytes.len() > 1_048_576 {
            return Err(AppError::BadRequest("module too large (>1MB)".into()));
        }
        (bytes, crate::service::widget_module::service::InstalledFrom::Url(url))
    } else if let Some(mp) = multipart.as_mut() {
        let mut bytes_opt = None;
        let mut name_opt = None;
        while let Some(field) = mp.next_field().await.map_err(|e| AppError::BadRequest(format!("multipart: {e}")))? {
            if field.name() == Some("file") {
                name_opt = field.file_name().map(|s| s.to_string());
                let data = field.bytes().await.map_err(|e| AppError::BadRequest(format!("multipart body: {e}")))?;
                if data.len() > 1_048_576 {
                    return Err(AppError::BadRequest("module too large (>1MB)".into()));
                }
                bytes_opt = Some(data.to_vec());
                break;
            }
        }
        let bytes = bytes_opt.ok_or_else(|| AppError::BadRequest("missing 'file' part".into()))?;
        (bytes, crate::service::widget_module::service::InstalledFrom::Upload(name_opt.unwrap_or_else(|| "upload.js".into())))
    } else {
        return Err(AppError::BadRequest("provide ?url=... or multipart file".into()));
    };

    let row = crate::service::widget_module::WidgetModuleService::install_single_file(
        &state.db, bytes, from, user_id,
    ).await?;
    Ok(axum::Json(crate::error::ApiResponse {
        data: serde_json::json!({
            "id": row.id, "version": row.version,
        }),
    }))
}

#[utoipa::path(
    delete, path = "/api/widget-modules/{id}", tag = "widget-modules",
    params(("id" = String, Path)),
    responses((status = 204)), security(("session_cookie" = []), ("api_key" = []))
)]
async fn uninstall_module(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    crate::service::widget_module::WidgetModuleService::uninstall(&state.db, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

The `write_router()` function is automatically mounted behind `require_admin` middleware in `router/api/mod.rs`. Verify by checking how other modules' `write_router` are composed there.

- [ ] **Step 3: Mount router**

In `crates/server/src/router/api/mod.rs`, in the write router builder, add `.merge(widget_module::write_router())`. Find the pattern by grep `write_router` in that file.

- [ ] **Step 4: Register in OpenAPI**

`openapi.rs` — add `install_module` and `uninstall_module` to `paths(...)`.

- [ ] **Step 5: Build + clippy**

`cargo build -p serverbee-server && cargo clippy -p serverbee-server -- -D warnings`

- [ ] **Step 6: Commit**

```bash
git add crates/server/src
git -c commit.gpgsign=false commit -m "feat(server): POST /api/widget-modules install + DELETE uninstall"
```

---

## Task 2: Integration test for install + uninstall

**Files:**
- Modify: `crates/server/tests/widget_module_integration.rs` (append)

- [ ] **Step 1: Add test**

```rust
#[tokio::test]
async fn install_single_file_via_multipart() {
    let ctx = start_test_server_with_db().await.expect("server");
    let code = r#"/**
 * @serverbee-widget {
 *   "id": "com.test.uploaded",
 *   "version": "1.0.0",
 *   "name": "Uploaded",
 *   "category": "Real-time",
 *   "sizing": { "defaultW": 2, "defaultH": 2, "minW": 1, "minH": 1, "strategy": "free" },
 *   "sdkVersion": "^0.1.0"
 * }
 */
export default {};"#;

    let form = reqwest::multipart::Form::new()
        .part(
            "file",
            reqwest::multipart::Part::bytes(code.as_bytes().to_vec()).file_name("uploaded.js"),
        );

    let client = ctx.http_client();
    let res = client
        .post(format!("{}/api/widget-modules", ctx.base_url))
        .header("cookie", &ctx.admin_cookie)
        .multipart(form)
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["data"]["id"], "com.test.uploaded");

    // Verify listed
    let res2 = client.get(format!("{}/api/widget-modules", ctx.base_url))
        .header("cookie", &ctx.admin_cookie).send().await.unwrap();
    let list: serde_json::Value = res2.json().await.unwrap();
    assert!(list["data"].as_array().unwrap().iter().any(|m| m["id"] == "com.test.uploaded"));

    // Uninstall
    let res3 = client.delete(format!("{}/api/widget-modules/com.test.uploaded", ctx.base_url))
        .header("cookie", &ctx.admin_cookie).send().await.unwrap();
    assert_eq!(res3.status(), 204);
}

#[tokio::test]
async fn cannot_uninstall_builtin() {
    let ctx = start_test_server_with_db().await.expect("server");
    let client = ctx.http_client();
    let res = client
        .delete(format!("{}/api/widget-modules/com.serverbee.hello-world", ctx.base_url))
        .header("cookie", &ctx.admin_cookie)
        .send().await.unwrap();
    assert!(res.status().is_client_error());
}
```

Adjust `ctx.http_client()` to whatever helper the existing test file exposes — match the established convention.

- [ ] **Step 2: Run tests**

`cargo test -p serverbee-server --test widget_module_integration` — 7 tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/server/tests
git -c commit.gpgsign=false commit -m "test(server): widget_module install + uninstall integration"
```

---

## Section B — Frontend install UX

## Task 3: API client + settings page

**Files:**
- Create: `apps/web/src/api/widget-modules.ts`
- Create: `apps/web/src/routes/_authed/settings/widgets.tsx`
- Modify: `apps/web/src/lib/i18n.ts` (add strings) — only if i18n file is small; otherwise just hardcode English for now

- [ ] **Step 1: API client**

`apps/web/src/api/widget-modules.ts`:

```ts
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

export interface ModuleSummary {
  id: string
  version: string
  source_type: string
  entry_path: string
  code_sha256: string
  manifest: Record<string, any>
  enabled: boolean
}

export function useWidgetModules() {
  return useQuery<ModuleSummary[]>({
    queryKey: ['widget-modules'],
    queryFn: async () => {
      const res = await fetch('/api/widget-modules', { credentials: 'include' })
      if (!res.ok) throw new Error(`list failed: ${res.status}`)
      const json = await res.json()
      return json.data
    },
  })
}

export function useInstallFromUrl() {
  const qc = useQueryClient()
  return useMutation<{ id: string; version: string }, Error, string>({
    mutationFn: async (url) => {
      const res = await fetch(`/api/widget-modules?url=${encodeURIComponent(url)}`, {
        method: 'POST', credentials: 'include',
      })
      if (!res.ok) throw new Error(await res.text())
      const json = await res.json()
      return json.data
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['widget-modules'] }),
  })
}

export function useInstallFromFile() {
  const qc = useQueryClient()
  return useMutation<{ id: string; version: string }, Error, File>({
    mutationFn: async (file) => {
      const fd = new FormData()
      fd.append('file', file)
      const res = await fetch('/api/widget-modules', {
        method: 'POST', credentials: 'include', body: fd,
      })
      if (!res.ok) throw new Error(await res.text())
      const json = await res.json()
      return json.data
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['widget-modules'] }),
  })
}

export function useUninstall() {
  const qc = useQueryClient()
  return useMutation<void, Error, string>({
    mutationFn: async (id) => {
      const res = await fetch(`/api/widget-modules/${id}`, {
        method: 'DELETE', credentials: 'include',
      })
      if (!res.ok) throw new Error(await res.text())
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['widget-modules'] }),
  })
}
```

- [ ] **Step 2: Settings page**

`apps/web/src/routes/_authed/settings/widgets.tsx`:

```tsx
import { createFileRoute } from '@tanstack/react-router'
import { useState } from 'react'
import {
  useWidgetModules,
  useInstallFromUrl,
  useInstallFromFile,
  useUninstall,
} from '@/api/widget-modules'

export const Route = createFileRoute('/_authed/settings/widgets')({
  component: WidgetsPage,
})

function WidgetsPage() {
  const list = useWidgetModules()
  const installUrl = useInstallFromUrl()
  const installFile = useInstallFromFile()
  const uninstall = useUninstall()

  const [url, setUrl] = useState('')

  return (
    <div style={{ padding: 24, maxWidth: 800 }}>
      <h1 style={{ fontSize: 20, fontWeight: 600, marginBottom: 16 }}>Widget Modules</h1>

      <section style={{ marginBottom: 24 }}>
        <h2 style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>Install from URL</h2>
        <div style={{ display: 'flex', gap: 8 }}>
          <input
            type="url" placeholder="https://example.com/foo.widget.js"
            value={url} onChange={(e) => setUrl(e.target.value)}
            style={{ flex: 1, padding: 6, border: '1px solid var(--border)' }}
          />
          <button
            type="button"
            onClick={() => url && installUrl.mutate(url, { onSuccess: () => setUrl('') })}
            disabled={!url || installUrl.isPending}
          >
            Install
          </button>
        </div>
        {installUrl.error && <p style={{ color: 'var(--destructive)' }}>{installUrl.error.message}</p>}
      </section>

      <section style={{ marginBottom: 24 }}>
        <h2 style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>Upload .js file</h2>
        <input
          type="file" accept=".js,.mjs"
          onChange={(e) => {
            const f = e.target.files?.[0]
            if (f) installFile.mutate(f)
          }}
        />
        {installFile.error && <p style={{ color: 'var(--destructive)' }}>{installFile.error.message}</p>}
      </section>

      <section>
        <h2 style={{ fontSize: 14, fontWeight: 600, marginBottom: 8 }}>Installed modules</h2>
        {list.isLoading && <p>Loading…</p>}
        {list.data?.map((m) => (
          <div key={m.id} style={{ display: 'flex', gap: 12, alignItems: 'center', padding: 8, borderBottom: '1px solid var(--border)' }}>
            <div style={{ flex: 1 }}>
              <div style={{ fontWeight: 500 }}>{(m.manifest.name as string) || m.id}</div>
              <div style={{ fontSize: 12, color: 'var(--muted-foreground)' }}>
                {m.id} · {m.version} · {m.source_type}
              </div>
            </div>
            {m.source_type !== 'Builtin' && (
              <button type="button" onClick={() => uninstall.mutate(m.id)} disabled={uninstall.isPending}>
                Uninstall
              </button>
            )}
          </div>
        ))}
      </section>
    </div>
  )
}
```

- [ ] **Step 3: Verify**

`cd apps/web && bun run typecheck && bun run build` — both succeed. TanStack Router will pick up the new file route automatically (route file regeneration runs on dev/build).

- [ ] **Step 4: Commit**

```bash
git add apps/web/src
git -c commit.gpgsign=false commit -m "feat(web): widget modules settings page (install URL/.js, list, uninstall)"
```

---

## Section C — Delete legacy theme code

## Task 4: Inventory and stage legacy deletion

Before deleting anything, inventory the files. Run:
```bash
grep -rlE "spa_theme|SpaTheme|active_spa_theme|ActiveSpaTheme|custom_theme|CustomTheme" \
  crates/server/src/ apps/web/src/ 2>/dev/null | sort -u > /tmp/legacy-theme-files.txt
cat /tmp/legacy-theme-files.txt
```

Expect ~28 files. Each must be evaluated:
- **Delete entirely** if the file's sole purpose is the legacy theme system.
- **Modify and keep** if it has unrelated content (e.g. `appearance.tsx` route, `i18n.ts`, `state.rs`, `openapi.rs`, `router/api/mod.rs`).

Report the list to the controller before proceeding. Do not delete blindly.

---

## Task 5: Delete legacy backend code

**Files to delete entirely:**
- `crates/server/src/service/spa_theme/` (whole directory: mod.rs, service.rs, manifest.rs, loaded.rs, extractor.rs, error.rs)
- `crates/server/src/entity/spa_theme.rs`
- `crates/server/src/entity/custom_theme.rs` (if exists)
- `crates/server/src/router/api/spa_theme.rs`
- `crates/server/src/router/api/theme.rs` IF it's solely custom-theme CRUD; else strip just those handlers

**Files to modify:**
- `crates/server/src/entity/mod.rs` — remove `pub mod spa_theme;` and `pub mod custom_theme;` (if listed)
- `crates/server/src/service/mod.rs` — remove `pub mod spa_theme;`
- `crates/server/src/router/api/mod.rs` — remove `pub mod spa_theme;` and any `.merge(spa_theme::...)` calls. Remove similar for custom_theme.
- `crates/server/src/state.rs` — remove the `active_spa_theme: ActiveSpaThemeSlot` field and any related types; delete `ActiveSpaThemeSlot` struct if local
- `crates/server/src/router/static_files.rs` — remove ALL `active_spa_theme` / `LoadedTheme` references; the catch-all should serve only rust-embed default SPA assets
- `crates/server/src/openapi.rs` — remove paths/schemas referring to deleted modules

**New migration** `crates/server/src/migration/m20260528_000060_drop_legacy_theme_tables.rs`:
```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE IF EXISTS spa_themes").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS custom_theme").await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> { Ok(()) }
}
```
Register in `migration/mod.rs`.

- [ ] **Step 1: Inventory + delete**

Work iteratively: delete a file, run `cargo build -p serverbee-server`, fix imports, repeat. If something compiles cleanly without a delete, prefer keeping it minimal until clippy is clean.

- [ ] **Step 2: Verify**

`cargo build -p serverbee-server && cargo clippy --workspace -- -D warnings && cargo test --workspace`

The previously passing `spa_theme_integration.rs` (and similar) integration test files reference deleted code — DELETE those test files too.

- [ ] **Step 3: Commit**

```bash
git add crates/server
git -c commit.gpgsign=false commit -m "refactor(server): delete legacy spa_theme + custom_theme code"
```

---

## Task 6: Delete legacy frontend code

**Files to delete entirely:**
- `apps/web/src/themes/` (whole directory: preset CSS files + `preset-vars.ts` + `index.ts`)
- `apps/web/src/api/themes.ts`
- `apps/web/src/api/spa-themes.ts`
- `apps/web/src/components/theme/` (whole directory if all files are legacy theme UI)
- `apps/web/src/components/spa-theme/` (whole directory)
- `apps/web/src/lib/theme-ref.ts`
- `apps/web/src/routes/_authed/settings/appearance/themes.*` files (if any exist)

**Files to modify:**
- `apps/web/src/routes/_authed/settings/appearance.tsx` and `appearance.test.tsx` — strip the spa-theme / custom-theme sections; if the page becomes empty, replace with a simple stub linking to widgets page
- `apps/web/src/components/theme-provider.tsx` — collapse to just light/dark mode toggle. Remove all `ThemeRef` parsing, dynamic CSS file loading, custom theme variable injection. Verify nothing in the app breaks (`bun run test`).
- `apps/web/src/lib/i18n.ts` — remove spa-theme / custom-theme strings

- [ ] **Step 1: Iterate**

Run `cd apps/web && bun run typecheck` after each deletion to catch broken imports. Common pattern:
- Delete `api/themes.ts` → consumer files in `components/theme/` will fail → delete those next → consumer routes will fail → delete or stub those next.

- [ ] **Step 2: Verify**

`cd apps/web && bun run typecheck && bun run test && bun run build` — all pass.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src
git -c commit.gpgsign=false commit -m "refactor(web): delete legacy theme code (preset/custom/spa)"
```

---

## Plan 3 — Completion Criteria

- `POST /api/widget-modules?url=...` installs from a URL.
- `POST /api/widget-modules` with multipart `file` installs from upload.
- `DELETE /api/widget-modules/{id}` uninstalls non-Builtin modules.
- `/_authed/settings/widgets` page lists modules, installs, uninstalls.
- All legacy theme code is removed (`spa_theme`, `custom_theme`, preset themes); compile is clean; tests pass.
- A subsequent `cargo build` after removing the apps/web/dist directory should fail with a clear message about needing `bun run build` first (acceptable — this is expected for rust-embed).

Deferred to follow-ups:

- Zip collection upload (`Plan 3 follow-up`).
- New Theme entity (cssVars + widgets pack) — not in this plan since SPA Theme replacement is deleted but the new Theme is a future concept.
- 14-widget rewrite.
- WidgetRendererV2 mounting in DashboardGrid.
