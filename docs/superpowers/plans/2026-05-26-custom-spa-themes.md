# Custom SPA Themes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** [`docs/superpowers/specs/2026-05-26-custom-spa-themes-design.md`](../specs/2026-05-26-custom-spa-themes-design.md). When this plan and the spec disagree, the spec wins — fix the plan.

**Goal:** Let admins replace the entire ServerBee SPA with an uploaded `.sbtheme` package, using existing same-origin auth and the existing REST/WS API. Includes preview-before-activate, recovery escape hatch, starter template, and bilingual docs.

**Architecture:** New `spa_themes` table (zip BLOB + manifest), `ArcSwap<Option<LoadedTheme>>` on `AppState` for lock-free serve path, refactored catch-all handler that picks between default SPA, active theme, or preview theme based on a cookie/query precedence list. Frontend gets a new section in `/settings/appearance`. New `templates/serverbee-theme-starter/` repo-local starter template ships alongside.

**Tech Stack:** Rust (Axum 0.8, sea-orm, `zip` crate, `arc-swap`, `semver`, `uuid` v4, `serde_json`), React 19 + TanStack Query + shadcn/ui, Vite, Bun.

**Test discipline:** TDD throughout. For each new function/route, write a failing test first, then implement. Commit after each green TDD cycle. Run `cargo clippy --workspace -- -D warnings` and `bun x ultracite check` before every commit that touches the respective stack.

---

## File Inventory

### Rust (backend)

| File | Action | Responsibility |
|---|---|---|
| `crates/server/Cargo.toml` | modify | Add deps: `zip`, `arc-swap`, `semver`, `tempfile` (dev) |
| `crates/server/src/error.rs` | modify | Add `AppError::Domain` variant; add optional `details` to `ErrorDetail` |
| `crates/server/src/migration/m20260526_000021_create_spa_themes.rs` | create | Create `spa_themes` table |
| `crates/server/src/migration/mod.rs` | modify | Register migration |
| `crates/server/src/entity/spa_theme.rs` | create | sea-orm entity for `spa_themes` |
| `crates/server/src/entity/mod.rs` | modify | `pub mod spa_theme` |
| `crates/server/src/service/spa_theme/mod.rs` | create | re-exports |
| `crates/server/src/service/spa_theme/error.rs` | create | `SpaThemeError` enum → `AppError::Domain` |
| `crates/server/src/service/spa_theme/manifest.rs` | create | `ThemeManifest` struct + validator |
| `crates/server/src/service/spa_theme/extractor.rs` | create | Zip extraction with security checks |
| `crates/server/src/service/spa_theme/loaded.rs` | create | `LoadedTheme` struct + helpers |
| `crates/server/src/service/spa_theme/service.rs` | create | `SpaThemeService` (CRUD + activate) |
| `crates/server/src/service/mod.rs` | modify | `pub mod spa_theme` |
| `crates/server/src/state.rs` | modify | Add `active_spa_theme: Arc<ArcSwap<Option<LoadedTheme>>>`; load on startup |
| `crates/server/src/router/api/spa_theme.rs` | create | REST routes + `SpaThemeUpload` extractor |
| `crates/server/src/router/api/mod.rs` | modify | `pub mod spa_theme` + nest into protected router |
| `crates/server/src/router/static_files.rs` | modify | Theme-aware serve handler (precedence + CSP + injection) |
| `crates/server/src/router/system.rs` | create | `/__system/*` reserved routes |
| `crates/server/src/router/mod.rs` | modify | Nest `/__system/*`; ensure `static_handler` receives `State<Arc<AppState>>` |
| `crates/server/src/openapi.rs` | modify | Register `spa_theme` routes + schemas |
| `crates/server/tests/spa_theme_integration.rs` | create | Integration tests |
| `crates/server/tests/fixtures/spa_themes/build_fixtures.rs` | create | Helper that generates fixture `.sbtheme` files at runtime |

### Frontend (apps/web)

| File | Action | Responsibility |
|---|---|---|
| `apps/web/src/api/spa-themes.ts` | create | React Query hooks for SPA theme endpoints |
| `apps/web/src/components/spa-theme/custom-spa-theme-section.tsx` | create | Orchestrator |
| `apps/web/src/components/spa-theme/spa-theme-card.tsx` | create | Theme card |
| `apps/web/src/components/spa-theme/spa-theme-upload-card.tsx` | create | Drag-drop upload card |
| `apps/web/src/components/spa-theme/activate-spa-theme-dialog.tsx` | create | Activation confirmation |
| `apps/web/src/components/spa-theme/preview-confirm-dialog.tsx` | create | Preview confirmation (browser-wide warning) |
| `apps/web/src/components/spa-theme/spa-theme-details-drawer.tsx` | create | Details drawer |
| Test files alongside each component (e.g. `spa-theme-card.test.tsx`) | create | vitest |
| `apps/web/src/routes/_authed/settings/appearance.tsx` | modify | Mount `<CustomSpaThemeSection />`; conditional banner on color/brand sections |
| `apps/web/src/locales/en/spa-theme.json` | create | English strings |
| `apps/web/src/locales/zh/spa-theme.json` | create | Chinese strings |
| `apps/web/src/lib/i18n.ts` | modify | Register `spa-theme` namespace |

### Starter template

| File | Action | Notes |
|---|---|---|
| `templates/serverbee-theme-starter/manifest.json` | create | `min_serverbee_version` commented out (alpha gotcha) |
| `templates/serverbee-theme-starter/package.json` | create | bun + vite + ts |
| `templates/serverbee-theme-starter/vite.config.ts` | create | `base: '/'` REQUIRED |
| `templates/serverbee-theme-starter/index.html` | create | minimal entry |
| `templates/serverbee-theme-starter/public/preview.png` | create | placeholder |
| `templates/serverbee-theme-starter/src/main.tsx` | create | React entry |
| `templates/serverbee-theme-starter/src/App.tsx` | create | Calls `/api/servers` to demo |
| `templates/serverbee-theme-starter/src/lib/serverbee.ts` | create | API client wrapper |
| `templates/serverbee-theme-starter/src/index.css` | create | Minimal Tailwind or vanilla |
| `templates/serverbee-theme-starter/pack.ts` | create | `bun run pack` → `.sbtheme` |
| `templates/serverbee-theme-starter/README.md` | create | Quickstart |
| `templates/serverbee-theme-starter/.gitignore` | create | `dist/`, `node_modules/`, `*.sbtheme` |

### Documentation

| File | Action |
|---|---|
| `apps/docs/content/docs/cn/themes/custom-frontend.mdx` | create |
| `apps/docs/content/docs/en/themes/custom-frontend.mdx` | create |
| `apps/docs/content/docs/cn/themes/meta.json` | create (or update parent) |
| `apps/docs/content/docs/en/themes/meta.json` | create |
| `apps/docs/content/docs/cn/configuration.mdx` | modify — add recovery URL note |
| `apps/docs/content/docs/en/configuration.mdx` | modify — add recovery URL note |

### Manual E2E

| File | Action |
|---|---|
| `tests/spa-themes.md` | create — manual checklist mirroring spec § 10.4 |

---

## Phase 1 — Prerequisites (Error contract, deps, migration, entity)

### Task 1: Extend `AppError` with `Domain` variant + `ErrorDetail.details`

**Files:**
- Modify: `crates/server/src/error.rs`

- [ ] **Step 1: Write failing test in `crates/server/src/error.rs` (bottom of file)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn domain_error_serializes_with_code_and_details() {
        let err = AppError::Domain {
            status: StatusCode::BAD_REQUEST,
            code: "ZIP_SLIP",
            message: "package contains unsafe path".to_string(),
            details: Some(serde_json::json!({ "entry": "../etc/passwd" })),
        };

        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "ZIP_SLIP");
        assert_eq!(json["error"]["message"], "package contains unsafe path");
        assert_eq!(json["error"]["details"]["entry"], "../etc/passwd");
    }

    #[tokio::test]
    async fn existing_variant_response_unchanged() {
        let err = AppError::BadRequest("test".into());
        let resp = err.into_response();
        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "BAD_REQUEST");
        assert!(json["error"].get("details").is_none(), "details must be omitted when absent");
    }
}
```

- [ ] **Step 2: Run test, verify failure**

```bash
cargo test -p serverbee-server --lib error::tests
```
Expected: compile fail (`Domain` not a variant; `details` field missing).

- [ ] **Step 3: Modify `crates/server/src/error.rs`**

Replace `ErrorDetail`:

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}
```

Add to `AppError`:

```rust
    #[error("{message}")]
    Domain {
        status: StatusCode,
        code: &'static str,
        message: String,
        details: Option<serde_json::Value>,
    },
```

Replace `IntoResponse for AppError`:

```rust
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "BAD_REQUEST".to_string(), m, None),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED".to_string(), "Unauthorized".into(), None),
            AppError::Forbidden(m) => (StatusCode::FORBIDDEN, "FORBIDDEN".to_string(), m, None),
            AppError::TooManyRequests(m) => (StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_REQUESTS".to_string(), m, None),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, "NOT_FOUND".to_string(), m, None),
            AppError::Conflict(m) => (StatusCode::CONFLICT, "CONFLICT".to_string(), m, None),
            AppError::Validation(m) => (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR".to_string(), m, None),
            AppError::RequestTimeout(m) => (StatusCode::REQUEST_TIMEOUT, "REQUEST_TIMEOUT".to_string(), m, None),
            AppError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR".to_string(), m, None),
            AppError::Domain { status, code, message, details } => (status, code.to_string(), message, details),
        };

        let body = ErrorBody { error: ErrorDetail { code, message, details } };
        (status, Json(body)).into_response()
    }
}
```

- [ ] **Step 4: Run tests, verify pass**

```bash
cargo test -p serverbee-server --lib error::tests
cargo clippy --workspace -- -D warnings
```
Expected: both PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/error.rs
git commit -m "feat(server): add AppError::Domain variant with structured details"
```

---

### Task 2: Add Cargo dependencies

**Files:**
- Modify: `crates/server/Cargo.toml`

- [ ] **Step 1: Add deps under `[dependencies]`**

```toml
zip = { version = "2", default-features = false, features = ["deflate"] }
arc-swap = "1"
semver = "1"
```

Under `[dev-dependencies]` add:

```toml
tempfile = "3"
```

- [ ] **Step 2: Verify build**

```bash
cargo build -p serverbee-server
```
Expected: success.

- [ ] **Step 3: Commit**

```bash
git add crates/server/Cargo.toml Cargo.lock
git commit -m "chore(server): add zip, arc-swap, semver deps for SPA themes"
```

---

### Task 3: Migration — create `spa_themes` table

**Files:**
- Create: `crates/server/src/migration/m20260526_000021_create_spa_themes.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Create migration file**

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum SpaThemes {
    Table,
    Id,
    Uuid,
    ManifestId,
    Name,
    Version,
    Author,
    Description,
    ManifestJson,
    PackageData,
    PreviewData,
    PreviewMime,
    SizeBytes,
    UploadedBy,
    UploadedAt,
    IsSuperseded,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SpaThemes::Table)
                    .col(ColumnDef::new(SpaThemes::Id).integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(SpaThemes::Uuid).text().not_null().unique_key())
                    .col(ColumnDef::new(SpaThemes::ManifestId).text().not_null())
                    .col(ColumnDef::new(SpaThemes::Name).text().not_null())
                    .col(ColumnDef::new(SpaThemes::Version).text().not_null())
                    .col(ColumnDef::new(SpaThemes::Author).text())
                    .col(ColumnDef::new(SpaThemes::Description).text())
                    .col(ColumnDef::new(SpaThemes::ManifestJson).text().not_null())
                    .col(ColumnDef::new(SpaThemes::PackageData).blob().not_null())
                    .col(ColumnDef::new(SpaThemes::PreviewData).blob())
                    .col(ColumnDef::new(SpaThemes::PreviewMime).text())
                    .col(ColumnDef::new(SpaThemes::SizeBytes).big_integer().not_null())
                    .col(ColumnDef::new(SpaThemes::UploadedBy).text().not_null())
                    .col(ColumnDef::new(SpaThemes::UploadedAt).timestamp().not_null().default(Expr::current_timestamp()))
                    .col(ColumnDef::new(SpaThemes::IsSuperseded).integer().not_null().default(0))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_spa_themes_manifest_id_version")
                    .table(SpaThemes::Table)
                    .col(SpaThemes::ManifestId)
                    .col(SpaThemes::Version)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_spa_themes_uploaded_at")
                    .table(SpaThemes::Table)
                    .col(SpaThemes::UploadedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

- [ ] **Step 2: Register in `crates/server/src/migration/mod.rs`**

Add `mod m20260526_000021_create_spa_themes;` and append `Box::new(m20260526_000021_create_spa_themes::Migration)` to the migration list.

- [ ] **Step 3: Verify migration runs**

```bash
cargo test -p serverbee-server --lib migration 2>&1 | tail -20
```
Or run an integration test that hits a fresh DB to confirm.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/migration/
git commit -m "feat(server): add spa_themes table migration"
```

---

### Task 4: Entity — `spa_theme`

**Files:**
- Create: `crates/server/src/entity/spa_theme.rs`
- Modify: `crates/server/src/entity/mod.rs`

- [ ] **Step 1: Create entity**

```rust
use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize)]
#[sea_orm(table_name = "spa_themes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub uuid: String,
    pub manifest_id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub manifest_json: String,
    #[serde(skip)]
    pub package_data: Vec<u8>,
    #[serde(skip)]
    pub preview_data: Option<Vec<u8>>,
    pub preview_mime: Option<String>,
    pub size_bytes: i64,
    pub uploaded_by: String,
    pub uploaded_at: DateTimeUtc,
    pub is_superseded: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 2: Register in `crates/server/src/entity/mod.rs`**

Add `pub mod spa_theme;` (alphabetical order matching neighbors).

- [ ] **Step 3: Verify compiles**

```bash
cargo build -p serverbee-server
```

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/entity/
git commit -m "feat(server): add spa_theme entity"
```

---

## Phase 2 — Manifest, Errors, Zip Extraction

### Task 5: `SpaThemeError` enum + `AppError::Domain` conversions

**Files:**
- Create: `crates/server/src/service/spa_theme/mod.rs`
- Create: `crates/server/src/service/spa_theme/error.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Add `pub mod spa_theme;` to `crates/server/src/service/mod.rs`**

- [ ] **Step 2: Create `crates/server/src/service/spa_theme/mod.rs`**

```rust
pub mod error;
pub mod extractor;
pub mod loaded;
pub mod manifest;
pub mod service;

pub use error::SpaThemeError;
pub use loaded::LoadedTheme;
pub use manifest::ThemeManifest;
pub use service::SpaThemeService;
```

Note: subsequent tasks create `extractor.rs`, `loaded.rs`, `manifest.rs`, `service.rs`. They must exist (even as empty stubs) before `mod.rs` compiles. Either create the stubs first or temporarily comment out the missing `pub mod` lines until the file is created in its task.

- [ ] **Step 3: Write failing test in `crates/server/src/service/spa_theme/error.rs`**

```rust
use axum::http::StatusCode;
use serde_json::json;

use crate::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum SpaThemeError {
    #[error("multipart upload exceeds the size limit")]
    UploadTooLarge { limit_bytes: u64 },
    #[error("multipart payload is malformed")]
    InvalidMultipart(String),
    #[error("manifest.json is missing")]
    MissingManifest,
    #[error("manifest is invalid")]
    InvalidManifest { field: &'static str, reason: String },
    #[error("entry HTML not present in package")]
    MissingEntry { entry: String },
    #[error("requires a newer ServerBee than running")]
    IncompatibleVersion { min: String, running: String },
    #[error("package contains unsafe path")]
    ZipSlip { entry: String },
    #[error("compression ratio too high")]
    ZipBomb { entry: String, ratio: u64 },
    #[error("symlinks are not allowed")]
    SymlinkNotAllowed { entry: String },
    #[error("duplicate zip entry")]
    DuplicateEntry { entry: String },
    #[error("file extension is not allowed")]
    DisallowedExtension { entry: String, ext: String },
    #[error("file too large")]
    FileTooLarge { entry: String, size: u64, limit: u64 },
    #[error("too many files in package")]
    TooManyFiles { count: usize, limit: usize },
    #[error("total uncompressed size exceeded")]
    TotalSizeExceeded { size: u64, limit: u64 },
    #[error("preview image too large")]
    PreviewTooLarge { size: u64, limit: u64 },
    #[error("version downgrade not allowed")]
    NoDowngrade { uploaded: String, existing: String },
    #[error("this version already exists")]
    VersionExists { manifest_id: String, version: String },
    #[error("theme is currently active")]
    ThemeInUse { uuid: String },
    #[error("theme not found")]
    ThemeNotFound { uuid: String },
}

impl SpaThemeError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UploadTooLarge { .. } => "UPLOAD_TOO_LARGE",
            Self::InvalidMultipart(_) => "INVALID_MULTIPART",
            Self::MissingManifest => "MISSING_MANIFEST",
            Self::InvalidManifest { .. } => "INVALID_MANIFEST",
            Self::MissingEntry { .. } => "MISSING_ENTRY",
            Self::IncompatibleVersion { .. } => "INCOMPATIBLE_VERSION",
            Self::ZipSlip { .. } => "ZIP_SLIP",
            Self::ZipBomb { .. } => "ZIP_BOMB",
            Self::SymlinkNotAllowed { .. } => "SYMLINK_NOT_ALLOWED",
            Self::DuplicateEntry { .. } => "DUPLICATE_ENTRY",
            Self::DisallowedExtension { .. } => "DISALLOWED_EXTENSION",
            Self::FileTooLarge { .. } => "FILE_TOO_LARGE",
            Self::TooManyFiles { .. } => "TOO_MANY_FILES",
            Self::TotalSizeExceeded { .. } => "TOTAL_SIZE_EXCEEDED",
            Self::PreviewTooLarge { .. } => "PREVIEW_TOO_LARGE",
            Self::NoDowngrade { .. } => "NO_DOWNGRADE",
            Self::VersionExists { .. } => "VERSION_EXISTS",
            Self::ThemeInUse { .. } => "THEME_IN_USE",
            Self::ThemeNotFound { .. } => "THEME_NOT_FOUND",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::UploadTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::VersionExists { .. } | Self::ThemeInUse { .. } => StatusCode::CONFLICT,
            Self::ThemeNotFound { .. } => StatusCode::NOT_FOUND,
            _ => StatusCode::BAD_REQUEST,
        }
    }

    fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::UploadTooLarge { limit_bytes } => Some(json!({ "limit_bytes": limit_bytes })),
            Self::InvalidMultipart(reason) => Some(json!({ "reason": reason })),
            Self::InvalidManifest { field, reason } => Some(json!({ "field": field, "reason": reason })),
            Self::MissingEntry { entry } => Some(json!({ "entry": entry })),
            Self::IncompatibleVersion { min, running } => Some(json!({ "min": min, "running": running })),
            Self::ZipSlip { entry } | Self::SymlinkNotAllowed { entry } | Self::DuplicateEntry { entry } => Some(json!({ "entry": entry })),
            Self::ZipBomb { entry, ratio } => Some(json!({ "entry": entry, "ratio": ratio })),
            Self::DisallowedExtension { entry, ext } => Some(json!({ "entry": entry, "ext": ext })),
            Self::FileTooLarge { entry, size, limit } => Some(json!({ "entry": entry, "size": size, "limit": limit })),
            Self::TooManyFiles { count, limit } => Some(json!({ "count": count, "limit": limit })),
            Self::TotalSizeExceeded { size, limit } | Self::PreviewTooLarge { size, limit } => Some(json!({ "size": size, "limit": limit })),
            Self::NoDowngrade { uploaded, existing } => Some(json!({ "uploaded": uploaded, "existing": existing })),
            Self::VersionExists { manifest_id, version } => Some(json!({ "manifest_id": manifest_id, "version": version })),
            Self::ThemeNotFound { uuid } | Self::ThemeInUse { uuid } => Some(json!({ "uuid": uuid })),
            Self::MissingManifest => None,
        }
    }
}

impl From<SpaThemeError> for AppError {
    fn from(err: SpaThemeError) -> Self {
        let status = err.status();
        let code = err.code();
        let details = err.details();
        let message = err.to_string();
        AppError::Domain { status, code, message, details }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn zip_slip_maps_to_domain_error() {
        let err: AppError = SpaThemeError::ZipSlip { entry: "../etc/passwd".into() }.into();
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "ZIP_SLIP");
        assert_eq!(json["error"]["details"]["entry"], "../etc/passwd");
    }

    #[tokio::test]
    async fn theme_in_use_is_409() {
        let err: AppError = SpaThemeError::ThemeInUse { uuid: "abc".into() }.into();
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn upload_too_large_is_413() {
        let err: AppError = SpaThemeError::UploadTooLarge { limit_bytes: 25 * 1024 * 1024 }.into();
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
```

- [ ] **Step 4: Create stub files so module compiles**

Touch `crates/server/src/service/spa_theme/{manifest,extractor,loaded,service}.rs` as empty files:

```bash
touch crates/server/src/service/spa_theme/{manifest,extractor,loaded,service}.rs
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p serverbee-server --lib service::spa_theme::error
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/service/spa_theme/ crates/server/src/service/mod.rs
git commit -m "feat(server): add SpaThemeError enum and AppError mapping"
```

---

### Task 6: `ThemeManifest` parser + validator

**Files:**
- Modify: `crates/server/src/service/spa_theme/manifest.rs`

Constants must match spec § 3:

```
schema_version == 1
id: /^[a-z][a-z0-9-]{2,63}$/
name: 1..=64 chars
author: ..=64 chars
description: ..=500 chars
homepage: valid http(s)://
version: valid semver
min_serverbee_version: valid semver, ≤ running
entry: defaults to "index.html", must end ".html"
preview: relative path, ≤ 500 KB (size enforced later)
```

- [ ] **Step 1: Write failing tests at the bottom of `manifest.rs`**

```rust
use serde::{Deserialize, Serialize};
use crate::service::spa_theme::error::SpaThemeError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThemeManifest {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default)]
    pub min_serverbee_version: Option<String>,
    #[serde(default)]
    pub preview: Option<String>,
}

fn default_entry() -> String { "index.html".into() }

pub const SCHEMA_VERSION: u32 = 1;
pub const ID_REGEX: &str = r"^[a-z][a-z0-9-]{2,63}$";
pub const MAX_NAME: usize = 64;
pub const MAX_AUTHOR: usize = 64;
pub const MAX_DESCRIPTION: usize = 500;

impl ThemeManifest {
    /// Parse manifest JSON bytes and validate every field. `running_version`
    /// is the server's semver (used for min_serverbee_version check).
    /// `file_paths` is the set of paths present in the package (for entry/preview existence).
    pub fn parse_and_validate(
        bytes: &[u8],
        running_version: &semver::Version,
        file_paths: &std::collections::HashSet<String>,
    ) -> Result<Self, SpaThemeError> {
        let mut m: Self = serde_json::from_slice(bytes).map_err(|e| SpaThemeError::InvalidManifest {
            field: "$",
            reason: format!("JSON parse: {e}"),
        })?;

        if m.schema_version != SCHEMA_VERSION {
            return Err(SpaThemeError::InvalidManifest { field: "schema_version", reason: format!("must be {SCHEMA_VERSION}") });
        }

        let re = regex::Regex::new(ID_REGEX).expect("static regex");
        if !re.is_match(&m.id) {
            return Err(SpaThemeError::InvalidManifest { field: "id", reason: format!("must match {ID_REGEX}") });
        }

        let name_trim = m.name.trim();
        if name_trim.is_empty() || name_trim.chars().count() > MAX_NAME {
            return Err(SpaThemeError::InvalidManifest { field: "name", reason: format!("1..={MAX_NAME} chars") });
        }
        m.name = strip_html(name_trim);

        if let Some(a) = &m.author {
            if a.chars().count() > MAX_AUTHOR {
                return Err(SpaThemeError::InvalidManifest { field: "author", reason: format!("..={MAX_AUTHOR} chars") });
            }
            m.author = Some(strip_html(a));
        }
        if let Some(d) = &m.description {
            if d.chars().count() > MAX_DESCRIPTION {
                return Err(SpaThemeError::InvalidManifest { field: "description", reason: format!("..={MAX_DESCRIPTION} chars") });
            }
            m.description = Some(strip_html(d));
        }
        if let Some(h) = &m.homepage {
            let u = url::Url::parse(h).map_err(|_| SpaThemeError::InvalidManifest { field: "homepage", reason: "invalid URL".into() })?;
            if !matches!(u.scheme(), "http" | "https") {
                return Err(SpaThemeError::InvalidManifest { field: "homepage", reason: "must be http(s)".into() });
            }
        }

        semver::Version::parse(&m.version).map_err(|_| SpaThemeError::InvalidManifest { field: "version", reason: "invalid semver".into() })?;

        if let Some(min) = &m.min_serverbee_version {
            let parsed = semver::Version::parse(min).map_err(|_| SpaThemeError::InvalidManifest { field: "min_serverbee_version", reason: "invalid semver".into() })?;
            if &parsed > running_version {
                return Err(SpaThemeError::IncompatibleVersion { min: min.clone(), running: running_version.to_string() });
            }
        }

        if !m.entry.ends_with(".html") {
            return Err(SpaThemeError::InvalidManifest { field: "entry", reason: "must end with .html".into() });
        }
        if !file_paths.contains(&m.entry) {
            return Err(SpaThemeError::MissingEntry { entry: m.entry.clone() });
        }
        if let Some(p) = &m.preview {
            if !file_paths.contains(p) {
                return Err(SpaThemeError::InvalidManifest { field: "preview", reason: format!("not in package: {p}") });
            }
            let lower = p.to_ascii_lowercase();
            if !(lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".webp")) {
                return Err(SpaThemeError::InvalidManifest { field: "preview", reason: "must be png/jpg/webp".into() });
            }
        }

        Ok(m)
    }
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            other if !in_tag => out.push(other),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn files(paths: &[&str]) -> HashSet<String> { paths.iter().map(|s| (*s).into()).collect() }
    fn v(s: &str) -> semver::Version { semver::Version::parse(s).unwrap() }

    fn valid_manifest() -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "id": "acme",
            "name": "Acme",
            "version": "1.0.0",
        })
    }

    #[test]
    fn happy_path() {
        let m = ThemeManifest::parse_and_validate(
            valid_manifest().to_string().as_bytes(),
            &v("1.0.0-alpha.3"),
            &files(&["index.html"]),
        ).unwrap();
        assert_eq!(m.id, "acme");
        assert_eq!(m.entry, "index.html");
    }

    #[test]
    fn rejects_bad_id() {
        let mut m = valid_manifest();
        m["id"] = serde_json::json!("Acme!");
        let err = ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0"), &files(&["index.html"])).unwrap_err();
        matches!(err, SpaThemeError::InvalidManifest { field: "id", .. });
    }

    #[test]
    fn rejects_wrong_schema_version() {
        let mut m = valid_manifest();
        m["schema_version"] = serde_json::json!(2);
        let err = ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0"), &files(&["index.html"])).unwrap_err();
        matches!(err, SpaThemeError::InvalidManifest { field: "schema_version", .. });
    }

    #[test]
    fn rejects_missing_entry() {
        let m = valid_manifest();
        let err = ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0"), &files(&[])).unwrap_err();
        matches!(err, SpaThemeError::MissingEntry { .. });
    }

    #[test]
    fn rejects_min_version_above_running() {
        let mut m = valid_manifest();
        m["min_serverbee_version"] = serde_json::json!("2.0.0");
        let err = ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0-alpha.3"), &files(&["index.html"])).unwrap_err();
        matches!(err, SpaThemeError::IncompatibleVersion { .. });
    }

    #[test]
    fn accepts_min_version_lt_alpha() {
        let mut m = valid_manifest();
        m["min_serverbee_version"] = serde_json::json!("0.9.0");
        ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0-alpha.3"), &files(&["index.html"])).unwrap();
    }

    #[test]
    fn strips_html_from_name() {
        let mut m = valid_manifest();
        m["name"] = serde_json::json!("<script>alert(1)</script>Acme");
        let parsed = ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0"), &files(&["index.html"])).unwrap();
        assert!(!parsed.name.contains('<'));
        assert!(parsed.name.contains("Acme"));
    }

    #[test]
    fn rejects_bad_homepage() {
        let mut m = valid_manifest();
        m["homepage"] = serde_json::json!("javascript:alert(1)");
        let err = ThemeManifest::parse_and_validate(m.to_string().as_bytes(), &v("1.0.0"), &files(&["index.html"])).unwrap_err();
        matches!(err, SpaThemeError::InvalidManifest { field: "homepage", .. });
    }
}
```

- [ ] **Step 2: Add `regex`, `url` deps if not already present**

Verify `crates/server/Cargo.toml` has `regex` and `url`. If missing, add:

```bash
grep -E "^(regex|url)" crates/server/Cargo.toml || echo "ADD MISSING"
```

If "ADD MISSING", add:

```toml
regex = "1"
url = "2"
```

- [ ] **Step 3: Run tests, verify pass**

```bash
cargo test -p serverbee-server --lib service::spa_theme::manifest
cargo clippy --workspace -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/spa_theme/manifest.rs crates/server/Cargo.toml Cargo.lock
git commit -m "feat(server): add ThemeManifest parser and validator"
```

---

### Task 7: Zip extractor with security checks

**Files:**
- Modify: `crates/server/src/service/spa_theme/extractor.rs`

Constants (spec § 3.2):

```
MAX_TOTAL_BYTES = 20 * 1024 * 1024
MAX_FILES = 1000
MAX_FILE_BYTES = 5 * 1024 * 1024
MAX_PREVIEW_BYTES = 500 * 1024
MAX_PATH_LEN = 255
MAX_RATIO = 100  // single-entry compression ratio
ALLOWED_EXTS = ["html","htm","js","mjs","css","png","jpg","jpeg","svg","webp","gif","ico","woff","woff2","ttf","otf","json","txt","map"]
```

- [ ] **Step 1: Write extractor + tests in `extractor.rs`**

```rust
use std::collections::{HashMap, HashSet};
use std::io::Read;

use crate::service::spa_theme::error::SpaThemeError;

pub const MAX_TOTAL_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_FILES: usize = 1000;
pub const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_PREVIEW_BYTES: u64 = 500 * 1024;
pub const MAX_PATH_LEN: usize = 255;
pub const MAX_RATIO: u64 = 100;

pub const ALLOWED_EXTS: &[&str] = &[
    "html", "htm", "js", "mjs", "css", "png", "jpg", "jpeg", "svg", "webp",
    "gif", "ico", "woff", "woff2", "ttf", "otf", "json", "txt", "map",
];

pub struct ExtractedPackage {
    pub files: HashMap<String, Vec<u8>>,
    pub manifest_bytes: Vec<u8>,
    pub preview: Option<(String, Vec<u8>, String /* mime */)>,
    pub total_bytes: u64,
}

pub fn extract(zip_bytes: &[u8]) -> Result<ExtractedPackage, SpaThemeError> {
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|e| SpaThemeError::InvalidMultipart(format!("zip open: {e}")))?;

    if archive.len() > MAX_FILES {
        return Err(SpaThemeError::TooManyFiles { count: archive.len(), limit: MAX_FILES });
    }

    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut total: u64 = 0;
    let mut manifest_bytes: Option<Vec<u8>> = None;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)
            .map_err(|e| SpaThemeError::InvalidMultipart(format!("zip entry {i}: {e}")))?;

        // Symlink check (Unix mode bits)
        if let Some(mode) = entry.unix_mode() {
            const S_IFMT: u32 = 0o170000;
            const S_IFLNK: u32 = 0o120000;
            if mode & S_IFMT == S_IFLNK {
                return Err(SpaThemeError::SymlinkNotAllowed { entry: entry.name().to_string() });
            }
        }
        if entry.is_dir() { continue; }

        // Path normalization & traversal check
        let raw = entry.name().to_string();
        if raw.len() > MAX_PATH_LEN {
            return Err(SpaThemeError::ZipSlip { entry: raw });
        }
        let normalized = normalize_path(&raw)
            .ok_or_else(|| SpaThemeError::ZipSlip { entry: raw.clone() })?;

        if !seen.insert(normalized.clone()) {
            return Err(SpaThemeError::DuplicateEntry { entry: normalized });
        }

        // Extension check
        let ext = std::path::Path::new(&normalized)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if !ALLOWED_EXTS.contains(&ext.as_str()) {
            return Err(SpaThemeError::DisallowedExtension { entry: normalized, ext });
        }

        // Per-file size
        let size = entry.size();
        if size > MAX_FILE_BYTES {
            return Err(SpaThemeError::FileTooLarge { entry: normalized, size, limit: MAX_FILE_BYTES });
        }

        // Compression ratio
        let compressed = entry.compressed_size();
        if compressed > 0 {
            let ratio = size / compressed.max(1);
            if ratio > MAX_RATIO {
                return Err(SpaThemeError::ZipBomb { entry: normalized, ratio });
            }
        }

        // Running total
        total = total.saturating_add(size);
        if total > MAX_TOTAL_BYTES {
            return Err(SpaThemeError::TotalSizeExceeded { size: total, limit: MAX_TOTAL_BYTES });
        }

        // Read bytes
        let mut buf = Vec::with_capacity(size as usize);
        entry.read_to_end(&mut buf)
            .map_err(|e| SpaThemeError::InvalidMultipart(format!("read {normalized}: {e}")))?;

        if normalized == "manifest.json" {
            manifest_bytes = Some(buf.clone());
        }
        files.insert(normalized, buf);
    }

    let manifest_bytes = manifest_bytes.ok_or(SpaThemeError::MissingManifest)?;

    Ok(ExtractedPackage { files, manifest_bytes, preview: None, total_bytes: total })
}

/// Normalize a zip entry path to POSIX. Returns None for unsafe paths.
fn normalize_path(raw: &str) -> Option<String> {
    if raw.is_empty() { return None; }
    // Reject absolute, drive letters, backslashes
    if raw.starts_with('/') { return None; }
    if raw.len() >= 2 && raw.as_bytes()[1] == b':' { return None; } // C:\...
    if raw.contains('\\') { return None; }
    let mut parts: Vec<&str> = Vec::new();
    for part in raw.split('/') {
        match part {
            "" | "." => continue,
            ".." => return None,
            other => parts.push(other),
        }
    }
    if parts.is_empty() { return None; }
    Some(parts.join("/"))
}

/// Locate preview bytes/mime if `preview` field set in manifest.
pub fn locate_preview(files: &HashMap<String, Vec<u8>>, preview_path: &str) -> Result<Option<(String, Vec<u8>, String)>, SpaThemeError> {
    let Some(bytes) = files.get(preview_path) else { return Ok(None); };
    let size = bytes.len() as u64;
    if size > MAX_PREVIEW_BYTES {
        return Err(SpaThemeError::PreviewTooLarge { size, limit: MAX_PREVIEW_BYTES });
    }
    let mime = match preview_path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }.to_string();
    Ok(Some((preview_path.to_string(), bytes.clone(), mime)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, data) in entries {
                w.start_file(*name, opts).unwrap();
                w.write_all(data).unwrap();
            }
            w.finish().unwrap();
        }
        buf
    }

    #[test]
    fn extracts_minimal_package() {
        let zip = build_zip(&[
            ("manifest.json", br#"{"schema_version":1,"id":"a","name":"A","version":"1.0.0"}"#),
            ("index.html", b"<html></html>"),
        ]);
        let pkg = extract(&zip).unwrap();
        assert!(pkg.files.contains_key("index.html"));
        assert!(!pkg.manifest_bytes.is_empty());
    }

    #[test]
    fn rejects_zip_slip() {
        let zip = build_zip(&[("../etc/passwd", b"x"), ("manifest.json", b"{}")]);
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::ZipSlip { .. }));
    }

    #[test]
    fn rejects_absolute_path() {
        let zip = build_zip(&[("/abs/path.html", b"x"), ("manifest.json", b"{}")]);
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::ZipSlip { .. }));
    }

    #[test]
    fn rejects_disallowed_extension() {
        let zip = build_zip(&[("evil.sh", b"#!/bin/sh"), ("manifest.json", b"{}")]);
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::DisallowedExtension { .. }));
    }

    #[test]
    fn rejects_too_many_files() {
        let mut entries: Vec<(String, Vec<u8>)> = (0..1001).map(|i| (format!("a{i}.txt"), vec![0u8; 8])).collect();
        entries.push(("manifest.json".into(), b"{}".to_vec()));
        let zip = build_zip(&entries.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect::<Vec<_>>());
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::TooManyFiles { .. }));
    }

    #[test]
    fn rejects_oversize_file() {
        let big = vec![b'a'; (MAX_FILE_BYTES + 1) as usize];
        let zip = build_zip(&[("big.js", &big), ("manifest.json", b"{}")]);
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::FileTooLarge { .. }));
    }

    #[test]
    fn rejects_total_over_limit() {
        let chunk = vec![b'a'; (MAX_FILE_BYTES) as usize];
        let entries: Vec<(String, Vec<u8>)> = (0..5).map(|i| (format!("c{i}.css"), chunk.clone())).collect();
        let mut owned = entries;
        owned.push(("manifest.json".into(), b"{}".to_vec()));
        let zip = build_zip(&owned.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect::<Vec<_>>());
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::TotalSizeExceeded { .. }));
    }

    #[test]
    fn rejects_missing_manifest() {
        let zip = build_zip(&[("index.html", b"<html/>")]);
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::MissingManifest));
    }

    #[test]
    fn rejects_duplicate_entries() {
        // The zip crate may itself reject duplicates, but build with raw API to verify our guard.
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
            w.start_file("a.html", opts).unwrap();
            w.write_all(b"1").unwrap();
            w.start_file("a.html", opts).unwrap();
            w.write_all(b"2").unwrap();
            w.start_file("manifest.json", opts).unwrap();
            w.write_all(b"{}").unwrap();
            w.finish().unwrap();
        }
        let err = extract(&buf).unwrap_err();
        assert!(matches!(err, SpaThemeError::DuplicateEntry { .. } | SpaThemeError::InvalidMultipart(_)));
    }
}
```

- [ ] **Step 2: Run tests, verify pass**

```bash
cargo test -p serverbee-server --lib service::spa_theme::extractor
```

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/service/spa_theme/extractor.rs
git commit -m "feat(server): add zip extractor with security checks for SPA themes"
```

---

## Phase 3 — Service & State

### Task 8: `LoadedTheme` + AppState integration

**Files:**
- Modify: `crates/server/src/service/spa_theme/loaded.rs`
- Modify: `crates/server/src/state.rs`

- [ ] **Step 1: Write `loaded.rs`**

```rust
use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::body::Bytes;

use crate::service::spa_theme::manifest::ThemeManifest;

#[derive(Debug, Clone)]
pub struct LoadedTheme {
    pub uuid: String,
    pub manifest: ThemeManifest,
    pub entry: String,
    pub files: HashMap<String, Bytes>,
}

pub type ActiveSpaThemeSlot = Arc<ArcSwap<Option<LoadedTheme>>>;

pub fn new_slot() -> ActiveSpaThemeSlot {
    Arc::new(ArcSwap::from_pointee(None))
}

impl LoadedTheme {
    pub fn from_extracted(
        uuid: String,
        manifest: ThemeManifest,
        files: HashMap<String, Vec<u8>>,
    ) -> Self {
        let entry = manifest.entry.clone();
        let files = files.into_iter().map(|(k, v)| (k, Bytes::from(v))).collect();
        Self { uuid, manifest, entry, files }
    }

    pub fn get(&self, path: &str) -> Option<Bytes> {
        let p = if path.is_empty() || path == "/" { self.entry.as_str() } else { path.trim_start_matches('/') };
        self.files.get(p).cloned()
    }

    pub fn entry_html(&self) -> Option<Bytes> {
        self.files.get(&self.entry).cloned()
    }
}
```

- [ ] **Step 2: Modify `crates/server/src/state.rs`**

In the `AppState` struct definition add:

```rust
    pub active_spa_theme: crate::service::spa_theme::loaded::ActiveSpaThemeSlot,
```

In `AppState::new` (search for where it constructs the struct), initialize:

```rust
            active_spa_theme: crate::service::spa_theme::loaded::new_slot(),
```

- [ ] **Step 3: Verify build**

```bash
cargo build -p serverbee-server
```

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/spa_theme/loaded.rs crates/server/src/state.rs
git commit -m "feat(server): add LoadedTheme slot on AppState (arc-swap)"
```

---

### Task 9: `SpaThemeService` — upload + list + get + delete

**Files:**
- Modify: `crates/server/src/service/spa_theme/service.rs`

Constants (`ACTIVE_SPA_THEME_KEY`) per spec § 2.2.

- [ ] **Step 1: Write `service.rs`**

```rust
use sea_orm::*;
use uuid::Uuid;

use crate::entity::spa_theme;
use crate::error::AppError;
use crate::service::config::ConfigService;
use crate::service::spa_theme::error::SpaThemeError;
use crate::service::spa_theme::extractor::{self, ExtractedPackage};
use crate::service::spa_theme::loaded::LoadedTheme;
use crate::service::spa_theme::manifest::ThemeManifest;

pub const ACTIVE_SPA_THEME_KEY: &str = "active_spa_theme_uuid";

pub struct SpaThemeService;

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct SpaThemeSummary {
    pub uuid: String,
    pub manifest_id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub size_bytes: i64,
    pub uploaded_by: String,
    pub uploaded_at: String,
    pub is_active: bool,
    pub is_superseded: bool,
    pub has_preview: bool,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct UploadResult {
    pub uuid: String,
    pub manifest: serde_json::Value,
    pub size_bytes: i64,
    pub preview_url: Option<String>,
    pub is_upgrade_of: Option<UpgradeOf>,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct UpgradeOf {
    pub previous_uuid: String,
    pub previous_version: String,
}

impl SpaThemeService {
    fn running_version() -> semver::Version {
        // Workspace version, hard-coded fallback to env CARGO_PKG_VERSION.
        let s = env!("CARGO_PKG_VERSION");
        semver::Version::parse(s).unwrap_or_else(|_| semver::Version::new(0, 0, 0))
    }

    pub async fn list(db: &DatabaseConnection) -> Result<Vec<SpaThemeSummary>, AppError> {
        let active = ConfigService::get(db, ACTIVE_SPA_THEME_KEY).await?.unwrap_or_default();
        let rows = spa_theme::Entity::find()
            .order_by_desc(spa_theme::Column::UploadedAt)
            .all(db)
            .await?;
        Ok(rows.into_iter().map(|m| SpaThemeSummary {
            is_active: !active.is_empty() && m.uuid == active,
            is_superseded: m.is_superseded != 0,
            has_preview: m.preview_data.is_some(),
            uuid: m.uuid,
            manifest_id: m.manifest_id,
            name: m.name,
            version: m.version,
            author: m.author,
            description: m.description,
            size_bytes: m.size_bytes,
            uploaded_by: m.uploaded_by,
            uploaded_at: m.uploaded_at.to_rfc3339(),
        }).collect())
    }

    pub async fn get(db: &DatabaseConnection, uuid: &str) -> Result<spa_theme::Model, AppError> {
        spa_theme::Entity::find()
            .filter(spa_theme::Column::Uuid.eq(uuid))
            .one(db)
            .await?
            .ok_or_else(|| SpaThemeError::ThemeNotFound { uuid: uuid.to_string() }.into())
    }

    pub async fn delete(db: &DatabaseConnection, uuid: &str) -> Result<(), AppError> {
        let active = ConfigService::get(db, ACTIVE_SPA_THEME_KEY).await?.unwrap_or_default();
        if active == uuid {
            return Err(SpaThemeError::ThemeInUse { uuid: uuid.to_string() }.into());
        }
        let row = Self::get(db, uuid).await?;
        spa_theme::Entity::delete_by_id(row.id).exec(db).await?;
        Ok(())
    }

    /// Validate + persist a new theme. Returns the inserted row + upgrade info.
    pub async fn upload(
        db: &DatabaseConnection,
        zip_bytes: Vec<u8>,
        uploader_user_id: &str,
    ) -> Result<(spa_theme::Model, Option<UpgradeOf>), AppError> {
        let extracted = extractor::extract(&zip_bytes).map_err(AppError::from)?;
        let file_paths: std::collections::HashSet<String> = extracted.files.keys().cloned().collect();
        let manifest = ThemeManifest::parse_and_validate(
            &extracted.manifest_bytes,
            &Self::running_version(),
            &file_paths,
        ).map_err(AppError::from)?;

        // Version policy
        let upgrade_of = Self::check_version_policy(db, &manifest).await?;

        // Preview locate
        let preview = if let Some(p) = &manifest.preview {
            extractor::locate_preview(&extracted.files, p).map_err(AppError::from)?
        } else { None };

        let new_uuid = Uuid::new_v4().to_string();
        let manifest_json = serde_json::to_string(&manifest).map_err(|e| AppError::Internal(e.to_string()))?;
        let am = spa_theme::ActiveModel {
            id: NotSet,
            uuid: Set(new_uuid.clone()),
            manifest_id: Set(manifest.id.clone()),
            name: Set(manifest.name.clone()),
            version: Set(manifest.version.clone()),
            author: Set(manifest.author.clone()),
            description: Set(manifest.description.clone()),
            manifest_json: Set(manifest_json),
            package_data: Set(zip_bytes),
            preview_data: Set(preview.as_ref().map(|(_, b, _)| b.clone())),
            preview_mime: Set(preview.as_ref().map(|(_, _, m)| m.clone())),
            size_bytes: Set(extracted.total_bytes as i64),
            uploaded_by: Set(uploader_user_id.to_string()),
            uploaded_at: Set(chrono::Utc::now()),
            is_superseded: Set(0),
        };

        // Transaction: mark older rows of same manifest_id as superseded, insert new.
        let txn = db.begin().await?;
        if upgrade_of.is_some() {
            spa_theme::Entity::update_many()
                .col_expr(spa_theme::Column::IsSuperseded, Expr::value(1))
                .filter(spa_theme::Column::ManifestId.eq(manifest.id.clone()))
                .exec(&txn).await?;
        }
        let inserted = am.insert(&txn).await?;
        txn.commit().await?;

        Ok((inserted, upgrade_of))
    }

    async fn check_version_policy(
        db: &DatabaseConnection,
        manifest: &ThemeManifest,
    ) -> Result<Option<UpgradeOf>, AppError> {
        let rows = spa_theme::Entity::find()
            .filter(spa_theme::Column::ManifestId.eq(manifest.id.clone()))
            .order_by_desc(spa_theme::Column::UploadedAt)
            .all(db)
            .await?;
        if rows.is_empty() { return Ok(None); }
        let uploaded = semver::Version::parse(&manifest.version)
            .map_err(|_| SpaThemeError::InvalidManifest { field: "version", reason: "invalid semver".into() })?;
        let mut best: Option<(semver::Version, spa_theme::Model)> = None;
        for r in rows {
            if let Ok(v) = semver::Version::parse(&r.version) {
                if best.as_ref().map(|(b, _)| &v > b).unwrap_or(true) {
                    best = Some((v, r));
                }
            }
        }
        let (latest_v, latest_row) = match best { Some(b) => b, None => return Ok(None) };
        if uploaded < latest_v {
            return Err(SpaThemeError::NoDowngrade { uploaded: uploaded.to_string(), existing: latest_v.to_string() }.into());
        }
        if uploaded == latest_v {
            return Err(SpaThemeError::VersionExists { manifest_id: manifest.id.clone(), version: uploaded.to_string() }.into());
        }
        Ok(Some(UpgradeOf { previous_uuid: latest_row.uuid, previous_version: latest_v.to_string() }))
    }
}
```

- [ ] **Step 2: Write unit tests at the bottom of `service.rs` covering**

  - version policy: empty → None
  - version policy: existing v1.0 + upload v1.1 → Some(UpgradeOf)
  - version policy: existing v1.1 + upload v1.0 → Err(NoDowngrade)
  - version policy: existing v1.0 + upload v1.0 → Err(VersionExists)
  - delete active → Err(ThemeInUse)
  - upload + list returns 1 row with is_active=false
  
  Use sea-orm SqliteConnection with `Database::connect("sqlite::memory:")` + migrator to set up DB. See existing `custom_theme.rs` tests for pattern (mod `tests` block).

  Skeleton:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::Migrator;
    use sea_orm_migration::MigratorTrait;

    async fn db() -> DatabaseConnection {
        let conn = Database::connect("sqlite::memory:").await.unwrap();
        Migrator::up(&conn, None).await.unwrap();
        conn
    }

    fn zip_of(manifest: serde_json::Value) -> Vec<u8> {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::SimpleFileOptions = Default::default();
            w.start_file("manifest.json", opts).unwrap();
            w.write_all(manifest.to_string().as_bytes()).unwrap();
            w.start_file("index.html", opts).unwrap();
            w.write_all(b"<html></html>").unwrap();
            w.finish().unwrap();
        }
        buf
    }

    fn manifest(id: &str, v: &str) -> serde_json::Value {
        serde_json::json!({"schema_version":1,"id":id,"name":id,"version":v})
    }

    async fn ensure_user(db: &DatabaseConnection, id: &str) {
        // Insert a minimal user row if your migrations enforce FK. Adapt to actual user entity.
        use crate::entity::user;
        if user::Entity::find_by_id(id.to_string()).one(db).await.unwrap().is_none() {
            let _ = user::ActiveModel {
                id: Set(id.to_string()),
                username: Set(id.to_string()),
                password_hash: Set("x".into()),
                role: Set("admin".into()),
                totp_secret: Set(None),
                must_change_password: Set(false),
                created_at: Set(chrono::Utc::now()),
                updated_at: Set(chrono::Utc::now()),
            }.insert(db).await;
        }
    }

    #[tokio::test]
    async fn upload_succeeds_and_lists() {
        let db = db().await;
        ensure_user(&db, "u1").await;
        let zip = zip_of(manifest("acme", "1.0.0"));
        let (m, up) = SpaThemeService::upload(&db, zip, "u1").await.unwrap();
        assert!(up.is_none());
        let list = SpaThemeService::list(&db).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].uuid, m.uuid);
        assert!(!list[0].is_active);
    }

    #[tokio::test]
    async fn rejects_downgrade() {
        let db = db().await; ensure_user(&db, "u1").await;
        SpaThemeService::upload(&db, zip_of(manifest("a", "1.1.0")), "u1").await.unwrap();
        let err = SpaThemeService::upload(&db, zip_of(manifest("a", "1.0.0")), "u1").await.unwrap_err();
        if let AppError::Domain { code, .. } = err { assert_eq!(code, "NO_DOWNGRADE"); } else { panic!() }
    }

    #[tokio::test]
    async fn rejects_same_version() {
        let db = db().await; ensure_user(&db, "u1").await;
        SpaThemeService::upload(&db, zip_of(manifest("a", "1.0.0")), "u1").await.unwrap();
        let err = SpaThemeService::upload(&db, zip_of(manifest("a", "1.0.0")), "u1").await.unwrap_err();
        if let AppError::Domain { code, .. } = err { assert_eq!(code, "VERSION_EXISTS"); } else { panic!() }
    }

    #[tokio::test]
    async fn upgrade_marks_superseded() {
        let db = db().await; ensure_user(&db, "u1").await;
        SpaThemeService::upload(&db, zip_of(manifest("a", "1.0.0")), "u1").await.unwrap();
        let (_, up) = SpaThemeService::upload(&db, zip_of(manifest("a", "1.1.0")), "u1").await.unwrap();
        assert!(up.is_some());
        let list = SpaThemeService::list(&db).await.unwrap();
        assert_eq!(list.len(), 2);
        let superseded: Vec<_> = list.iter().filter(|s| s.is_superseded).collect();
        assert_eq!(superseded.len(), 1);
        assert_eq!(superseded[0].version, "1.0.0");
    }

    #[tokio::test]
    async fn delete_active_rejected() {
        let db = db().await; ensure_user(&db, "u1").await;
        let (m, _) = SpaThemeService::upload(&db, zip_of(manifest("a", "1.0.0")), "u1").await.unwrap();
        ConfigService::set(&db, ACTIVE_SPA_THEME_KEY, &m.uuid).await.unwrap();
        let err = SpaThemeService::delete(&db, &m.uuid).await.unwrap_err();
        if let AppError::Domain { code, .. } = err { assert_eq!(code, "THEME_IN_USE"); } else { panic!() }
    }
}
```

- [ ] **Step 3: Run tests, fix until green**

```bash
cargo test -p serverbee-server --lib service::spa_theme::service
```

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/spa_theme/service.rs
git commit -m "feat(server): add SpaThemeService upload/list/get/delete"
```

---

### Task 10: `SpaThemeService` — activate / deactivate / startup load

**Files:**
- Modify: `crates/server/src/service/spa_theme/service.rs`
- Modify: `crates/server/src/state.rs`

- [ ] **Step 1: Add methods to `SpaThemeService`**

Append in `service.rs`:

```rust
impl SpaThemeService {
    /// Set the active theme (uuid). None deactivates.
    pub async fn set_active(
        db: &DatabaseConnection,
        slot: &crate::service::spa_theme::loaded::ActiveSpaThemeSlot,
        uuid: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        match uuid {
            None => {
                ConfigService::set(db, ACTIVE_SPA_THEME_KEY, "").await?;
                slot.store(std::sync::Arc::new(None));
                Ok(None)
            }
            Some(u) => {
                let row = Self::get(db, u).await?;
                let loaded = Self::load_row(&row)?;
                ConfigService::set(db, ACTIVE_SPA_THEME_KEY, u).await?;
                slot.store(std::sync::Arc::new(Some(loaded)));
                Ok(Some(u.to_string()))
            }
        }
    }

    pub async fn active_uuid(db: &DatabaseConnection) -> Result<Option<String>, AppError> {
        Ok(ConfigService::get(db, ACTIVE_SPA_THEME_KEY)
            .await?
            .filter(|s| !s.is_empty()))
    }

    pub fn load_row(row: &spa_theme::Model) -> Result<LoadedTheme, AppError> {
        let extracted = crate::service::spa_theme::extractor::extract(&row.package_data)
            .map_err(|e| AppError::Internal(format!("re-extract stored theme: {e}")))?;
        let manifest: ThemeManifest = serde_json::from_str(&row.manifest_json)
            .map_err(|e| AppError::Internal(format!("manifest json: {e}")))?;
        Ok(LoadedTheme::from_extracted(row.uuid.clone(), manifest, extracted.files))
    }

    /// Called on server startup. If active uuid stored but row missing or zip broken,
    /// log warning and leave slot empty (fall back to default SPA).
    pub async fn load_on_startup(
        db: &DatabaseConnection,
        slot: &crate::service::spa_theme::loaded::ActiveSpaThemeSlot,
    ) {
        match Self::active_uuid(db).await {
            Ok(Some(u)) => match Self::get(db, &u).await {
                Ok(row) => match Self::load_row(&row) {
                    Ok(loaded) => slot.store(std::sync::Arc::new(Some(loaded))),
                    Err(e) => tracing::warn!("active SPA theme {u} failed to load: {e}; falling back to default"),
                },
                Err(_) => tracing::warn!("active SPA theme {u} not found; falling back to default"),
            },
            Ok(None) => {}
            Err(e) => tracing::warn!("read active spa theme key failed: {e}"),
        }
    }
}
```

- [ ] **Step 2: Add unit tests**

```rust
    #[tokio::test]
    async fn set_active_loads_into_slot() {
        let db = db().await; ensure_user(&db, "u1").await;
        let (m, _) = SpaThemeService::upload(&db, zip_of(manifest("a", "1.0.0")), "u1").await.unwrap();
        let slot = crate::service::spa_theme::loaded::new_slot();
        SpaThemeService::set_active(&db, &slot, Some(&m.uuid)).await.unwrap();
        let loaded = slot.load();
        assert!(loaded.as_ref().is_some());
        let theme = loaded.as_ref().as_ref().unwrap();
        assert_eq!(theme.uuid, m.uuid);
        assert!(theme.entry_html().is_some());
    }

    #[tokio::test]
    async fn set_none_deactivates() {
        let db = db().await; ensure_user(&db, "u1").await;
        let (m, _) = SpaThemeService::upload(&db, zip_of(manifest("a", "1.0.0")), "u1").await.unwrap();
        let slot = crate::service::spa_theme::loaded::new_slot();
        SpaThemeService::set_active(&db, &slot, Some(&m.uuid)).await.unwrap();
        SpaThemeService::set_active(&db, &slot, None).await.unwrap();
        assert!(slot.load().is_none());
    }

    #[tokio::test]
    async fn startup_load_with_dangling_key_falls_back() {
        let db = db().await;
        ConfigService::set(&db, ACTIVE_SPA_THEME_KEY, "does-not-exist").await.unwrap();
        let slot = crate::service::spa_theme::loaded::new_slot();
        SpaThemeService::load_on_startup(&db, &slot).await;
        assert!(slot.load().is_none());
    }
```

- [ ] **Step 3: Call `load_on_startup` from `AppState::new`**

In `crates/server/src/state.rs` after slot init, call:

```rust
        crate::service::spa_theme::SpaThemeService::load_on_startup(&db, &active_spa_theme).await;
```

(Adjust binding name if `AppState::new` constructs differently. Inspect the existing function first.)

- [ ] **Step 4: Run all spa_theme service tests**

```bash
cargo test -p serverbee-server --lib service::spa_theme
```

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/spa_theme/service.rs crates/server/src/state.rs
git commit -m "feat(server): add SPA theme activate/deactivate + startup load"
```

---

## Phase 4 — HTTP API Surface

### Task 11: Custom multipart extractor `SpaThemeUpload`

**Files:**
- Create: `crates/server/src/router/api/spa_theme.rs` (begin with extractor only; routes added in next tasks)
- Modify: `crates/server/src/router/api/mod.rs` (add `pub mod spa_theme;`)

The extractor wraps Axum's `Multipart` and converts `MultipartRejection` (esp. `PayloadTooLarge`) into our JSON error contract (spec § 4).

- [ ] **Step 1: Create file with extractor + smoke test**

```rust
use axum::extract::{FromRequest, Multipart, Request};
use axum::extract::multipart::MultipartRejection;
use axum::http::StatusCode;

use crate::error::AppError;
use crate::service::spa_theme::error::SpaThemeError;

pub const UPLOAD_LIMIT_BYTES: u64 = 25 * 1024 * 1024;

/// Wrapper around `Multipart` that maps body-limit / parse rejections to our JSON contract.
pub struct SpaThemeUpload(pub Multipart);

impl<S> FromRequest<S> for SpaThemeUpload
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Multipart::from_request(req, state).await {
            Ok(m) => Ok(Self(m)),
            Err(rej) => Err(map_rejection(rej)),
        }
    }
}

fn map_rejection(rej: MultipartRejection) -> AppError {
    let status = rej.status();
    if status == StatusCode::PAYLOAD_TOO_LARGE {
        return SpaThemeError::UploadTooLarge { limit_bytes: UPLOAD_LIMIT_BYTES }.into();
    }
    SpaThemeError::InvalidMultipart(rej.to_string()).into()
}

pub fn router(_state: std::sync::Arc<crate::state::AppState>) -> axum::Router<std::sync::Arc<crate::state::AppState>> {
    // Routes added in subsequent tasks. Placeholder router so `mod.rs` can wire it.
    axum::Router::new()
}
```

- [ ] **Step 2: Register module**

Edit `crates/server/src/router/api/mod.rs`: add `pub mod spa_theme;` alphabetically and merge the router in the **admin-only** chain. Search for the section near `setting` / `firewall` / `theme` and append:

```rust
.merge(spa_theme::router(state.clone()))
```

If unclear about admin-only vs read-only chains, place under the same chain as `theme` (existing custom color theme routes).

- [ ] **Step 3: Build**

```bash
cargo build -p serverbee-server
```

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/router/api/spa_theme.rs crates/server/src/router/api/mod.rs
git commit -m "feat(server): add SpaThemeUpload multipart extractor that maps rejections to UPLOAD_TOO_LARGE"
```

---

### Task 12: REST routes — list / upload / get / preview / package / delete

**Files:**
- Modify: `crates/server/src/router/api/spa_theme.rs`

- [ ] **Step 1: Define request/response DTOs + route handlers**

Append to `spa_theme.rs`:

```rust
use std::sync::Arc;

use axum::Json;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::response::IntoResponse;
use axum::routing::{delete, get, post, put};

use crate::error::{ApiResponse, ok};
use crate::service::audit::AuditService;
use crate::service::spa_theme::SpaThemeService;
use crate::service::spa_theme::service::{SpaThemeSummary, UploadResult};

pub fn router(state: Arc<crate::state::AppState>) -> axum::Router<Arc<crate::state::AppState>> {
    use axum::Router;
    Router::new()
        .route("/settings/spa-themes", get(list).post(upload))
        .route("/settings/spa-themes/:uuid", get(get_one).delete(delete_one))
        .route("/settings/spa-themes/:uuid/preview", get(get_preview))
        .route("/settings/spa-themes/:uuid/package", get(get_package))
        .route("/settings/active-spa-theme", get(get_active).put(put_active))
        .layer(DefaultBodyLimit::max(UPLOAD_LIMIT_BYTES as usize))
        .with_state(state)
}

#[utoipa::path(get, path = "/api/settings/spa-themes",
    responses((status = 200, body = Vec<SpaThemeSummary>)))]
async fn list(State(state): State<Arc<crate::state::AppState>>)
    -> Result<Json<ApiResponse<Vec<SpaThemeSummary>>>, crate::error::AppError>
{
    ok(SpaThemeService::list(&state.db).await?)
}

#[utoipa::path(get, path = "/api/settings/spa-themes/{uuid}",
    params(("uuid" = String, Path)),
    responses((status = 200, body = SpaThemeSummary)))]
async fn get_one(State(state): State<Arc<crate::state::AppState>>, Path(uuid): Path<String>)
    -> Result<Json<ApiResponse<SpaThemeSummary>>, crate::error::AppError>
{
    let row = SpaThemeService::get(&state.db, &uuid).await?;
    let active = SpaThemeService::active_uuid(&state.db).await?.unwrap_or_default();
    ok(SpaThemeSummary {
        is_active: active == row.uuid,
        is_superseded: row.is_superseded != 0,
        has_preview: row.preview_data.is_some(),
        uuid: row.uuid,
        manifest_id: row.manifest_id,
        name: row.name,
        version: row.version,
        author: row.author,
        description: row.description,
        size_bytes: row.size_bytes,
        uploaded_by: row.uploaded_by,
        uploaded_at: row.uploaded_at.to_rfc3339(),
    })
}

#[utoipa::path(get, path = "/api/settings/spa-themes/{uuid}/preview",
    params(("uuid" = String, Path)))]
async fn get_preview(State(state): State<Arc<crate::state::AppState>>, Path(uuid): Path<String>)
    -> Result<axum::response::Response, crate::error::AppError>
{
    let row = SpaThemeService::get(&state.db, &uuid).await?;
    let Some(bytes) = row.preview_data else { return Err(crate::error::AppError::NotFound("no preview".into())); };
    let mime = row.preview_mime.unwrap_or_else(|| "application/octet-stream".into());
    Ok(([(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response())
}

#[utoipa::path(get, path = "/api/settings/spa-themes/{uuid}/package",
    params(("uuid" = String, Path)))]
async fn get_package(State(state): State<Arc<crate::state::AppState>>, Path(uuid): Path<String>)
    -> Result<axum::response::Response, crate::error::AppError>
{
    let row = SpaThemeService::get(&state.db, &uuid).await?;
    let filename = format!("{}-{}.sbtheme", row.manifest_id, row.version);
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "application/zip".to_string()),
            (axum::http::header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", filename)),
        ],
        row.package_data,
    ).into_response())
}

#[utoipa::path(delete, path = "/api/settings/spa-themes/{uuid}", params(("uuid" = String, Path)))]
async fn delete_one(
    State(state): State<Arc<crate::state::AppState>>,
    Path(uuid): Path<String>,
    axum::Extension(user): axum::Extension<crate::middleware::auth::AuthUser>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
) -> Result<axum::http::StatusCode, crate::error::AppError> {
    let row = SpaThemeService::get(&state.db, &uuid).await?;
    SpaThemeService::delete(&state.db, &uuid).await?;
    let _ = AuditService::log(
        &state.db,
        &user.id,
        "spa_theme.delete",
        Some(&serde_json::json!({"uuid": uuid, "manifest_id": row.manifest_id, "version": row.version}).to_string()),
        &addr.ip().to_string(),
    ).await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/api/settings/spa-themes", request_body(content_type = "multipart/form-data"))]
async fn upload(
    State(state): State<Arc<crate::state::AppState>>,
    axum::Extension(user): axum::Extension<crate::middleware::auth::AuthUser>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    SpaThemeUpload(mut mp): SpaThemeUpload,
) -> Result<Json<ApiResponse<UploadResult>>, crate::error::AppError> {
    let mut package_bytes: Option<Vec<u8>> = None;
    while let Some(field) = mp.next_field().await
        .map_err(|e| crate::error::AppError::from(SpaThemeError::InvalidMultipart(e.to_string())))?
    {
        if field.name() == Some("package") {
            package_bytes = Some(field.bytes().await
                .map_err(|e| crate::error::AppError::from(SpaThemeError::InvalidMultipart(e.to_string())))?
                .to_vec());
            break;
        }
    }
    let bytes = package_bytes.ok_or_else(|| crate::error::AppError::from(SpaThemeError::InvalidMultipart("missing 'package' field".into())))?;
    let (row, upgrade) = SpaThemeService::upload(&state.db, bytes, &user.id).await?;

    let _ = AuditService::log(
        &state.db,
        &user.id,
        "spa_theme.upload",
        Some(&serde_json::json!({
            "uuid": row.uuid, "manifest_id": row.manifest_id,
            "version": row.version, "size_bytes": row.size_bytes
        }).to_string()),
        &addr.ip().to_string(),
    ).await;

    ok(UploadResult {
        uuid: row.uuid.clone(),
        manifest: serde_json::from_str(&row.manifest_json).unwrap_or(serde_json::Value::Null),
        size_bytes: row.size_bytes,
        preview_url: if row.preview_data.is_some() { Some(format!("/api/settings/spa-themes/{}/preview", row.uuid)) } else { None },
        is_upgrade_of: upgrade,
    })
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
struct PutActiveBody { theme_id: Option<String> }

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
struct ActiveResp { theme_id: Option<String> }

#[utoipa::path(get, path = "/api/settings/active-spa-theme")]
async fn get_active(State(state): State<Arc<crate::state::AppState>>)
    -> Result<Json<ApiResponse<ActiveResp>>, crate::error::AppError>
{
    ok(ActiveResp { theme_id: SpaThemeService::active_uuid(&state.db).await? })
}

#[utoipa::path(put, path = "/api/settings/active-spa-theme", request_body = PutActiveBody)]
async fn put_active(
    State(state): State<Arc<crate::state::AppState>>,
    axum::Extension(user): axum::Extension<crate::middleware::auth::AuthUser>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Json(body): Json<PutActiveBody>,
) -> Result<Json<ApiResponse<ActiveResp>>, crate::error::AppError> {
    let previous = SpaThemeService::active_uuid(&state.db).await?;
    let new = SpaThemeService::set_active(&state.db, &state.active_spa_theme, body.theme_id.as_deref()).await?;
    let (action, audit_uuid) = match (&previous, &new) {
        (_, Some(u)) => ("spa_theme.activate", u.clone()),
        (Some(p), None) => ("spa_theme.deactivate", p.clone()),
        (None, None) => ("spa_theme.deactivate", String::new()),
    };
    if !audit_uuid.is_empty() {
        let _ = AuditService::log(
            &state.db,
            &user.id,
            action,
            Some(&serde_json::json!({"uuid": audit_uuid}).to_string()),
            &addr.ip().to_string(),
        ).await;
    }
    ok(ActiveResp { theme_id: new })
}
```

- [ ] **Step 2: Verify the `AuthUser` extension shape matches the existing middleware**

```bash
grep -n "pub struct AuthUser\|pub id" crates/server/src/middleware/auth.rs | head
```

If the field is named differently (e.g. `user_id`), adapt the handler code accordingly. Apply the same naming convention used by other admin handlers.

- [ ] **Step 3: Build and clippy**

```bash
cargo build -p serverbee-server
cargo clippy --workspace -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/router/api/spa_theme.rs
git commit -m "feat(server): add SPA theme REST routes (list/upload/get/preview/package/delete/active)"
```

---

### Task 13: Register schemas in OpenAPI

**Files:**
- Modify: `crates/server/src/openapi.rs`

- [ ] **Step 1: Find existing pattern**

```bash
grep -n "components\|paths\|utoipa" crates/server/src/openapi.rs | head -30
```

- [ ] **Step 2: Add the new schemas and paths**

Add to `components(schemas(...))` array: `SpaThemeSummary, UploadResult, UpgradeOf, ActiveResp, PutActiveBody`.

Add to `paths(...)` array: the handlers from `spa_theme.rs`.

If the openapi file uses `OpenApi` derive macros, follow existing patterns (look at how `theme` and `brand` routes are registered).

- [ ] **Step 3: Build, verify `/swagger-ui/` shows new routes**

```bash
cargo build -p serverbee-server
```

Manual: start `cargo run -p serverbee-server`, open http://localhost:9527/swagger-ui/, search for `spa-themes`. Stop server.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/openapi.rs
git commit -m "feat(server): register SPA theme routes in OpenAPI"
```

---

## Phase 5 — Serve Path

### Task 14: Theme-aware serve handler with cookie precedence

This task replaces `crates/server/src/router/static_files.rs`. The new handler implements the precedence list in spec § 6.5 plus CSP and banner injection (Section 6.6).

**Files:**
- Modify: `crates/server/src/router/static_files.rs`
- Modify: `crates/server/src/router/mod.rs` (handler now needs `State<Arc<AppState>>`)

- [ ] **Step 1: Replace `static_files.rs`**

```rust
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;
use serde::Deserialize;

use crate::middleware::auth::AuthUser;
use crate::state::AppState;

#[derive(Embed)]
#[folder = "../../apps/web/dist"]
struct Assets;

const FORCE_DEFAULT_COOKIE: &str = "sb_force_default";
const PREVIEW_COOKIE: &str = "sb_preview_theme";

#[derive(Debug, Deserialize)]
pub struct ThemeQuery { theme: Option<String> }

/// Decision the handler made about which source to serve.
enum Source<'a> {
    Default,
    Active(arc_swap::Guard<Arc<Option<crate::service::spa_theme::LoadedTheme>>>),
    Preview { uuid: &'a str, theme: arc_swap::Guard<Arc<Option<crate::service::spa_theme::LoadedTheme>>> },
}

pub async fn theme_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ThemeQuery>,
    headers: HeaderMap,
    user: Option<axum::Extension<AuthUser>>,
    uri: Uri,
) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Parse cookies
    let cookie_str = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()).unwrap_or("");
    let cookies: Vec<(&str, &str)> = cookie_str
        .split(';')
        .filter_map(|c| {
            let mut it = c.trim().splitn(2, '=');
            Some((it.next()?, it.next().unwrap_or("")))
        })
        .collect();
    let has_force_default = cookies.iter().any(|(k, _)| *k == FORCE_DEFAULT_COOKIE);
    let preview_cookie = cookies.iter().find(|(k, _)| *k == PREVIEW_COOKIE).map(|(_, v)| *v);

    let is_admin = user.as_ref().map(|u| u.role == "admin").unwrap_or(false);

    // Precedence per spec § 6.5
    let mut set_cookie: Option<String> = None;
    let mut preview_uuid: Option<String> = None;

    let source: Source = match q.theme.as_deref() {
        Some("default") => {
            set_cookie = Some(format!("{FORCE_DEFAULT_COOKIE}=1; Path=/; Max-Age=3600; SameSite=Strict"));
            Source::Default
        }
        Some(t) if t.starts_with("preview:") && is_admin => {
            let uuid = t.trim_start_matches("preview:").to_string();
            set_cookie = Some(format!("{PREVIEW_COOKIE}={uuid}; Path=/; Max-Age=900; SameSite=Strict"));
            preview_uuid = Some(uuid);
            let guard = state.active_spa_theme.load();
            // Resolve preview theme: we need to load it on-demand if active != uuid.
            // For simplicity, look up via DB and load (cached in memory of this response).
            // Implementation note: keeping this fast requires a small per-uuid cache;
            // v1 acceptable to re-extract.
            Source::Preview { uuid: preview_uuid.as_deref().unwrap(), theme: guard }
        }
        Some("active") => {
            set_cookie = Some(format!("{FORCE_DEFAULT_COOKIE}=; Path=/; Max-Age=0\
                                       , {PREVIEW_COOKIE}=; Path=/; Max-Age=0"));
            let guard = state.active_spa_theme.load();
            if guard.is_some() { Source::Active(guard) } else { Source::Default }
        }
        _ => {
            if has_force_default { Source::Default }
            else if let Some(uuid) = preview_cookie.filter(|_| is_admin) {
                preview_uuid = Some(uuid.to_string());
                let guard = state.active_spa_theme.load();
                Source::Preview { uuid: preview_uuid.as_deref().unwrap(), theme: guard }
            } else {
                let guard = state.active_spa_theme.load();
                if guard.is_some() { Source::Active(guard) } else { Source::Default }
            }
        }
    };

    let resp = match &source {
        Source::Default => serve_default(path),
        Source::Active(guard) => {
            let theme = guard.as_ref().as_ref().expect("Active source has theme");
            serve_theme(path, theme, false)
        }
        Source::Preview { uuid, .. } => {
            // Preview may be a *different* theme from active; load on demand.
            match load_preview_on_demand(&state, uuid).await {
                Some(loaded) => serve_theme(path, &loaded, true),
                None => serve_default(path),
            }
        }
    };

    if let Some(c) = set_cookie {
        let mut r = resp;
        for header_val in c.split(", ") {
            if let Ok(v) = HeaderValue::from_str(header_val) {
                r.headers_mut().append(header::SET_COOKIE, v);
            }
        }
        r
    } else { resp }
}

async fn load_preview_on_demand(
    state: &Arc<AppState>,
    uuid: &str,
) -> Option<crate::service::spa_theme::LoadedTheme> {
    let row = crate::service::spa_theme::SpaThemeService::get(&state.db, uuid).await.ok()?;
    crate::service::spa_theme::SpaThemeService::load_row(&row).ok()
}

fn serve_default(path: &str) -> Response {
    if let Some(file) = Assets::get(path) {
        return embedded_file_response(path, &file);
    }
    match Assets::get("index.html") {
        Some(file) => embedded_file_response("index.html", &file),
        None => (StatusCode::NOT_FOUND, "Frontend not embedded").into_response(),
    }
}

fn embedded_file_response(path: &str, file: &rust_embed::EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut builder = Response::builder().header(header::CONTENT_TYPE, mime.as_ref());
    if path.starts_with("assets/") {
        builder = builder.header(header::CACHE_CONTROL, "public, max-age=31536000, immutable");
    } else {
        builder = builder.header(header::CACHE_CONTROL, "public, max-age=60");
    }
    builder.body(Body::from(file.data.clone())).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn serve_theme(path: &str, theme: &crate::service::spa_theme::LoadedTheme, inject_banner: bool) -> Response {
    let p = if path.is_empty() { theme.entry.as_str() } else { path };
    let (served_path, bytes) = if let Some(b) = theme.get(p) {
        (p.to_string(), b)
    } else if let Some(b) = theme.entry_html() {
        // SPA history routing fallback
        (theme.entry.clone(), b)
    } else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    let mime = mime_guess::from_path(&served_path).first_or_octet_stream();
    let is_html = mime.essence_str() == "text/html";
    let body_bytes = if is_html && inject_banner {
        inject_preview_banner(&bytes)
    } else { bytes };

    let mut builder = Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CONTENT_SECURITY_POLICY,
            "default-src 'self'; \
             script-src 'self' 'unsafe-inline' 'unsafe-eval'; \
             style-src 'self' 'unsafe-inline'; \
             img-src 'self' data: blob:; \
             font-src 'self' data:; \
             connect-src 'self'; \
             frame-ancestors 'none'; \
             base-uri 'self'; \
             form-action 'self'")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::X_FRAME_OPTIONS, "DENY")
        .header(header::REFERRER_POLICY, "same-origin");
    if served_path.starts_with("assets/") && !is_html {
        builder = builder.header(header::CACHE_CONTROL, "public, max-age=31536000, immutable");
    } else {
        builder = builder.header(header::CACHE_CONTROL, "no-cache");
    }
    builder.body(Body::from(body_bytes)).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn inject_preview_banner(html: &axum::body::Bytes) -> axum::body::Bytes {
    const BANNER: &str = r##"<div id="__sb_preview" style="position:fixed;top:0;left:0;right:0;z-index:2147483647;background:#fde68a;color:#111;padding:8px 12px;font:14px/1.4 sans-serif;text-align:center;box-shadow:0 1px 4px rgba(0,0,0,.2)">Preview mode &middot; this theme is being previewed by an admin &middot; <button id="__sb_exit" style="margin-left:8px;padding:4px 10px;border:1px solid #333;background:#fff;cursor:pointer">Exit preview</button></div><script>(function(){var b=document.getElementById('__sb_exit');if(!b)return;b.onclick=function(){fetch('/__system/clear-preview',{method:'POST',credentials:'include'}).then(function(){location.reload()})}})();</script>"##;
    let s = std::str::from_utf8(html).unwrap_or("");
    let lower = s.to_ascii_lowercase();
    let injected = match lower.rfind("</body>") {
        Some(i) => format!("{}{}{}", &s[..i], BANNER, &s[i..]),
        None => format!("{s}{BANNER}"),
    };
    axum::body::Bytes::from(injected)
}
```

- [ ] **Step 2: Wire handler in `crates/server/src/router/mod.rs`**

Replace `.fallback(static_files::static_handler)` with `.fallback(static_files::theme_handler)`. The `State<Arc<AppState>>` will be extracted from `.with_state(state)` on the router.

- [ ] **Step 3: Add `axum-extra` cookie parsing if not present**

We rolled our own cookie parsing above to avoid adding a dep. If you prefer, add `axum-extra = { version = "0.10", features = ["cookie"] }` and use `CookieJar`.

- [ ] **Step 4: Build**

```bash
cargo build -p serverbee-server
cargo clippy --workspace -- -D warnings
```

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/static_files.rs crates/server/src/router/mod.rs
git commit -m "feat(server): theme-aware serve handler with cookie precedence, CSP, preview banner"
```

---

### Task 15: `/__system/*` reserved routes

**Files:**
- Create: `crates/server/src/router/system.rs`
- Modify: `crates/server/src/router/mod.rs`

- [ ] **Step 1: Create `system.rs`**

```rust
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;

pub fn router() -> axum::Router {
    axum::Router::new()
        .route("/clear-recovery", post(clear_recovery))
        .route("/clear-preview", post(clear_preview))
}

async fn clear_recovery() -> Response {
    let mut r = StatusCode::NO_CONTENT.into_response();
    r.headers_mut().append(header::SET_COOKIE, HeaderValue::from_static("sb_force_default=; Path=/; Max-Age=0"));
    r
}

async fn clear_preview() -> Response {
    let mut r = StatusCode::NO_CONTENT.into_response();
    r.headers_mut().append(header::SET_COOKIE, HeaderValue::from_static("sb_preview_theme=; Path=/; Max-Age=0"));
    r
}
```

- [ ] **Step 2: Mount in `router/mod.rs`**

Before the catch-all fallback, add:

```rust
        .nest("/__system", system::router())
```

And `mod system;` at the top of `router/mod.rs`.

- [ ] **Step 3: Build**

```bash
cargo build -p serverbee-server
```

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/router/system.rs crates/server/src/router/mod.rs
git commit -m "feat(server): add /__system/* clear-recovery and clear-preview endpoints"
```

---

## Phase 6 — Backend Integration Tests

### Task 16: Integration test suite

**Files:**
- Create: `crates/server/tests/spa_theme_integration.rs`

The repo already has integration tests; mirror their pattern (search for `tests/*.rs` in `crates/server/`). The tests below assume a `spawn_test_app` helper exists; if not, add a minimal one inline (boot the Axum app on an ephemeral port with an in-memory SQLite).

- [ ] **Step 1: Inspect existing integration test pattern**

```bash
ls crates/server/tests/ && head -50 crates/server/tests/integration.rs 2>/dev/null
```

- [ ] **Step 2: Create `spa_theme_integration.rs` covering**

  1. Non-admin POST `/api/settings/spa-themes` → 403
  2. Admin upload valid fixture → 200, returns uuid + manifest
  3. Admin upload same id + higher version → 200 + `is_upgrade_of`
  4. Admin upload same id + lower version → 400, `code == "NO_DOWNGRADE"`
  5. Admin upload same id + same version → 409, `code == "VERSION_EXISTS"`
  6. Admin activate via PUT `/api/settings/active-spa-theme` → 200; GET `/` returns theme body
  7. GET `/?theme=default` returns default SPA body (no theme bytes) + sets `sb_force_default` cookie
  8. Subsequent GET `/` with that cookie returns default
  9. GET `/?theme=active` clears the cookie and returns the active theme
  10. GET `/?theme=preview:<uuid>` as admin returns that theme + sets `sb_preview_theme` cookie
  11. Same query as non-admin returns active (preview ignored)
  12. DELETE active theme → 409 `THEME_IN_USE`
  13. DELETE inactive theme → 204; row removed
  14. Upload pre-built zip-slip fixture → 400 `ZIP_SLIP`
  15. Upload pre-built zip-bomb fixture → 400 `ZIP_BOMB`
  16. Upload package > 25 MB → 413 `UPLOAD_TOO_LARGE` with JSON body matching contract

Use the helper `build_zip` from `extractor.rs` tests (move it into a `crates/server/tests/fixtures.rs` if you want it shared) to synthesize fixtures inline. No on-disk fixture files needed for v1.

- [ ] **Step 3: Run**

```bash
cargo test -p serverbee-server --test spa_theme_integration
```

- [ ] **Step 4: Commit**

```bash
git add crates/server/tests/spa_theme_integration.rs
git commit -m "test(server): SPA theme upload/activate/preview/recovery integration tests"
```

---

## Phase 7 — Frontend

### Task 17: API hooks `apps/web/src/api/spa-themes.ts`

**Files:**
- Create: `apps/web/src/api/spa-themes.ts`

- [ ] **Step 1: Create hooks file**

```ts
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'

export interface SpaThemeSummary {
  uuid: string
  manifest_id: string
  name: string
  version: string
  author?: string | null
  description?: string | null
  size_bytes: number
  uploaded_by: string
  uploaded_at: string
  is_active: boolean
  is_superseded: boolean
  has_preview: boolean
}

export interface UploadResult {
  uuid: string
  manifest: Record<string, unknown>
  size_bytes: number
  preview_url: string | null
  is_upgrade_of: { previous_uuid: string; previous_version: string } | null
}

export function useSpaThemes() {
  return useQuery<SpaThemeSummary[]>({
    queryKey: ['spa-themes'],
    queryFn: () => api.get<SpaThemeSummary[]>('/api/settings/spa-themes'),
  })
}

export function useActiveSpaTheme() {
  return useQuery<{ theme_id: string | null }>({
    queryKey: ['active-spa-theme'],
    queryFn: () => api.get('/api/settings/active-spa-theme'),
    staleTime: 30_000,
  })
}

export function useActivateSpaTheme() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (themeId: string | null) =>
      api.put<{ theme_id: string | null }>('/api/settings/active-spa-theme', { theme_id: themeId }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['active-spa-theme'] })
      qc.invalidateQueries({ queryKey: ['spa-themes'] })
    },
  })
}

export function useDeleteSpaTheme() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: async (uuid: string) => {
      const res = await fetch(`/api/settings/spa-themes/${uuid}`, { method: 'DELETE', credentials: 'include' })
      if (!res.ok) {
        const text = await res.text().catch(() => '')
        throw new Error(text || res.statusText)
      }
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['spa-themes'] }),
  })
}

export interface UploadError extends Error {
  code?: string
  details?: Record<string, unknown>
}

export function useUploadSpaTheme() {
  const qc = useQueryClient()
  return useMutation<UploadResult, UploadError, File>({
    mutationFn: async (file: File) => {
      const fd = new FormData()
      fd.append('package', file)
      const res = await fetch('/api/settings/spa-themes', {
        method: 'POST', credentials: 'include', body: fd,
      })
      const text = await res.text()
      let parsed: unknown
      try { parsed = JSON.parse(text) } catch { parsed = null }
      if (!res.ok) {
        const err = new Error((parsed as { error?: { message?: string } })?.error?.message ?? text) as UploadError
        err.code = (parsed as { error?: { code?: string } })?.error?.code
        err.details = (parsed as { error?: { details?: Record<string, unknown> } })?.error?.details
        throw err
      }
      return (parsed as { data: UploadResult }).data
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ['spa-themes'] }),
  })
}
```

- [ ] **Step 2: Type check**

```bash
bun run typecheck
```

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/api/spa-themes.ts
git commit -m "feat(web): add spa-themes API hooks"
```

---

### Task 18: i18n strings + namespace registration

**Files:**
- Create: `apps/web/src/locales/en/spa-theme.json`
- Create: `apps/web/src/locales/zh/spa-theme.json`
- Modify: `apps/web/src/lib/i18n.ts`

- [ ] **Step 1: Create both JSON files**

`en/spa-theme.json`:

```json
{
  "section_title": "Custom Frontend",
  "section_badge": "Beta",
  "section_description": "Replace the entire ServerBee UI with a custom theme. Recovery: append ?theme=default to any URL.",
  "empty_title": "No custom themes yet",
  "empty_cta": "Upload first theme",
  "empty_docs": "Read theme dev docs",
  "upload_drag": "Drag .sbtheme here or click",
  "upload_progress": "Uploading…",
  "upload_validating": "Validating…",
  "active_indicator": "Active",
  "superseded": "Superseded",
  "actions": {
    "preview": "Preview",
    "activate": "Activate",
    "delete": "Delete",
    "details": "Details",
    "download": "Download package"
  },
  "preview_dialog": {
    "title": "Open preview in a new tab?",
    "body": "Preview is browser-wide for up to 15 minutes. Any tab that reloads / in this browser will see the preview theme until you exit. If you need the management UI live during preview, use a separate browser or an incognito window.",
    "confirm": "Open preview",
    "cancel": "Cancel"
  },
  "activate_dialog": {
    "title": "Activate {{name}} v{{version}}?",
    "body": "This will replace the entire UI for ALL users (including the login page).",
    "recovery_hint": "Recovery: if the theme breaks, open any URL with ?theme=default to restore the built-in UI.",
    "confirm_checkbox": "I understand and want to activate",
    "confirm": "Activate",
    "cancel": "Cancel"
  },
  "delete_active_blocked": "Cannot delete the active theme. Deactivate first.",
  "color_brand_disabled_banner": "A custom frontend is active. The color theme and brand settings below have no effect until it is deactivated.",
  "errors": {
    "UPLOAD_TOO_LARGE": "Package exceeds the 25 MB upload limit.",
    "MISSING_MANIFEST": "manifest.json is missing from the package.",
    "INVALID_MANIFEST": "manifest is invalid: {{reason}}",
    "MISSING_ENTRY": "Entry HTML {{entry}} is not in the package.",
    "INCOMPATIBLE_VERSION": "Requires ServerBee {{min}}; running {{running}}.",
    "ZIP_SLIP": "Package contains an unsafe path: {{entry}}",
    "ZIP_BOMB": "Compression ratio too high on {{entry}}",
    "SYMLINK_NOT_ALLOWED": "Symlinks are not allowed: {{entry}}",
    "DUPLICATE_ENTRY": "Duplicate entry: {{entry}}",
    "DISALLOWED_EXTENSION": "File extension not allowed: {{entry}} ({{ext}})",
    "FILE_TOO_LARGE": "{{entry}} is too large ({{size}} bytes; limit {{limit}}).",
    "TOO_MANY_FILES": "Too many files ({{count}}; limit {{limit}}).",
    "TOTAL_SIZE_EXCEEDED": "Total size exceeded ({{size}} bytes; limit {{limit}}).",
    "PREVIEW_TOO_LARGE": "Preview image too large ({{size}} bytes; limit {{limit}}).",
    "NO_DOWNGRADE": "Cannot downgrade: uploaded {{uploaded}}, existing {{existing}}.",
    "VERSION_EXISTS": "Version {{version}} of {{manifest_id}} already exists.",
    "THEME_IN_USE": "Cannot delete: this theme is currently active.",
    "THEME_NOT_FOUND": "Theme {{uuid}} not found.",
    "default": "Upload failed: {{message}}"
  }
}
```

`zh/spa-theme.json`: same structure with Chinese translations. Translator note: keep `{{placeholders}}` intact. Translate per existing style in `apps/web/src/locales/zh/settings.json` (terse, technical).

- [ ] **Step 2: Register namespace in `apps/web/src/lib/i18n.ts`**

Add the imports and resource entries for `'spa-theme'` mirroring the existing pattern.

- [ ] **Step 3: Type check**

```bash
bun run typecheck
```

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/locales/en/spa-theme.json apps/web/src/locales/zh/spa-theme.json apps/web/src/lib/i18n.ts
git commit -m "feat(web): add spa-theme i18n namespace"
```

---

### Task 19: Component — `SpaThemeCard` + test

**Files:**
- Create: `apps/web/src/components/spa-theme/spa-theme-card.tsx`
- Create: `apps/web/src/components/spa-theme/spa-theme-card.test.tsx`

- [ ] **Step 1: Write component**

```tsx
import { useTranslation } from 'react-i18next'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import type { SpaThemeSummary } from '@/api/spa-themes'

interface Props {
  theme: SpaThemeSummary
  onPreview: () => void
  onActivate: () => void
  onDeactivate: () => void
  onDelete: () => void
  onOpenDetails: () => void
}

export function SpaThemeCard({ theme, onPreview, onActivate, onDeactivate, onDelete, onOpenDetails }: Props) {
  const { t } = useTranslation('spa-theme')
  const previewUrl = theme.has_preview ? `/api/settings/spa-themes/${theme.uuid}/preview` : null
  return (
    <div className="border rounded-lg overflow-hidden flex flex-col" data-testid={`spa-theme-card-${theme.uuid}`}>
      <div className="aspect-video bg-muted">
        {previewUrl ? <img src={previewUrl} alt="" className="w-full h-full object-cover" /> : null}
      </div>
      <div className="p-3 flex-1 flex flex-col gap-2">
        <div className="flex items-center justify-between gap-2">
          <div className="font-medium truncate" title={theme.name}>{theme.name}</div>
          {theme.is_active ? <Badge variant="default">{t('active_indicator')}</Badge> : null}
          {theme.is_superseded && !theme.is_active ? <Badge variant="secondary">{t('superseded')}</Badge> : null}
        </div>
        <div className="text-xs text-muted-foreground">v{theme.version}{theme.author ? ` · ${theme.author}` : ''}</div>
        <div className="mt-auto flex flex-wrap gap-2">
          <Button size="sm" variant="outline" onClick={onPreview}>{t('actions.preview')}</Button>
          {theme.is_active
            ? <Button size="sm" variant="outline" onClick={onDeactivate}>Deactivate</Button>
            : <Button size="sm" onClick={onActivate}>{t('actions.activate')}</Button>}
          <Button size="sm" variant="ghost" onClick={onOpenDetails}>{t('actions.details')}</Button>
          <Button size="sm" variant="destructive" disabled={theme.is_active} title={theme.is_active ? t('delete_active_blocked') : undefined} onClick={onDelete}>{t('actions.delete')}</Button>
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Test**

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { SpaThemeCard } from './spa-theme-card'
import '@/lib/i18n'

const base = {
  uuid: 'u1', manifest_id: 'm', name: 'Acme', version: '1.0.0',
  author: 'Inc', description: null, size_bytes: 1, uploaded_by: 'u', uploaded_at: '2026-05-26',
  is_active: false, is_superseded: false, has_preview: false,
}

describe('SpaThemeCard', () => {
  it('shows activate when not active', () => {
    render(<SpaThemeCard theme={base} onPreview={() => {}} onActivate={() => {}} onDeactivate={() => {}} onDelete={() => {}} onOpenDetails={() => {}} />)
    expect(screen.getByText(/Activate/)).toBeInTheDocument()
  })
  it('disables delete when active', () => {
    const onDelete = vi.fn()
    render(<SpaThemeCard theme={{ ...base, is_active: true }} onPreview={() => {}} onActivate={() => {}} onDeactivate={() => {}} onDelete={onDelete} onOpenDetails={() => {}} />)
    const btn = screen.getByText(/Delete/) as HTMLButtonElement
    expect(btn).toBeDisabled()
    fireEvent.click(btn)
    expect(onDelete).not.toHaveBeenCalled()
  })
})
```

- [ ] **Step 3: Run**

```bash
bun run test -- spa-theme-card
bun x ultracite check apps/web/src/components/spa-theme/
```

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/components/spa-theme/
git commit -m "feat(web): SpaThemeCard component + test"
```

---

### Task 20: Component — `SpaThemeUploadCard` + test

**Files:**
- Create: `apps/web/src/components/spa-theme/spa-theme-upload-card.tsx`
- Create: `apps/web/src/components/spa-theme/spa-theme-upload-card.test.tsx`

- [ ] **Step 1: Write component**

```tsx
import { type ChangeEvent, type DragEvent, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Upload } from 'lucide-react'
import { toast } from 'sonner'
import { type UploadError, useUploadSpaTheme } from '@/api/spa-themes'

export function SpaThemeUploadCard() {
  const { t } = useTranslation('spa-theme')
  const upload = useUploadSpaTheme()
  const inputRef = useRef<HTMLInputElement>(null)
  const [dragOver, setDragOver] = useState(false)

  function pick() { inputRef.current?.click() }

  function onChange(e: ChangeEvent<HTMLInputElement>) {
    const f = e.target.files?.[0]
    if (f) submit(f)
  }

  function onDrop(e: DragEvent<HTMLDivElement>) {
    e.preventDefault(); setDragOver(false)
    const f = e.dataTransfer.files?.[0]
    if (f) submit(f)
  }

  function submit(file: File) {
    upload.mutate(file, {
      onError: (err: UploadError) => {
        const code = err.code ?? 'default'
        const details = (err.details ?? {}) as Record<string, unknown>
        toast.error(t(`errors.${code}` as never, { ...details, message: err.message }))
      },
      onSuccess: () => toast.success('Theme uploaded'),
    })
  }

  return (
    <div
      onDragOver={(e) => { e.preventDefault(); setDragOver(true) }}
      onDragLeave={() => setDragOver(false)}
      onDrop={onDrop}
      className={`border-2 border-dashed rounded-lg flex flex-col items-center justify-center aspect-video cursor-pointer transition ${dragOver ? 'border-primary bg-accent/40' : 'border-muted'}`}
      onClick={pick}
      role="button"
      tabIndex={0}
    >
      <input ref={inputRef} type="file" accept=".sbtheme,.zip,application/zip" className="hidden" onChange={onChange} />
      <Upload className="size-6 text-muted-foreground mb-2" />
      <div className="text-sm text-muted-foreground">
        {upload.isPending ? t('upload_progress') : t('upload_drag')}
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Test** (mock fetch, verify FormData posted, verify error toast keys)

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { SpaThemeUploadCard } from './spa-theme-upload-card'
import '@/lib/i18n'

function wrap(node: React.ReactNode) {
  return <QueryClientProvider client={new QueryClient()}>{node}</QueryClientProvider>
}

describe('SpaThemeUploadCard', () => {
  it('renders the drag prompt', () => {
    render(wrap(<SpaThemeUploadCard />))
    expect(screen.getByText(/Drag .sbtheme/)).toBeInTheDocument()
  })

  it('submits a multipart POST when a file is selected', async () => {
    const spy = vi.spyOn(global, 'fetch').mockResolvedValue(new Response(JSON.stringify({ data: { uuid: 'u', manifest: {}, size_bytes: 1, preview_url: null, is_upgrade_of: null } }), { status: 200 }))
    render(wrap(<SpaThemeUploadCard />))
    const input = document.querySelector('input[type=file]') as HTMLInputElement
    const file = new File(['x'], 'a.sbtheme', { type: 'application/zip' })
    fireEvent.change(input, { target: { files: [file] } })
    await vi.waitFor(() => expect(spy).toHaveBeenCalled())
    expect(spy.mock.calls[0][1]?.method).toBe('POST')
  })
})
```

- [ ] **Step 3: Run + lint**

```bash
bun run test -- spa-theme-upload-card
bun x ultracite check apps/web/src/components/spa-theme/
```

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/components/spa-theme/spa-theme-upload-card.tsx apps/web/src/components/spa-theme/spa-theme-upload-card.test.tsx
git commit -m "feat(web): SpaThemeUploadCard with drag-drop + structured error toast"
```

---

### Task 21: Component — `ActivateSpaThemeDialog` + `PreviewConfirmDialog`

**Files:**
- Create: `apps/web/src/components/spa-theme/activate-spa-theme-dialog.tsx`
- Create: `apps/web/src/components/spa-theme/preview-confirm-dialog.tsx`
- Create test files alongside

Both use `<Dialog>` from `apps/web/src/components/ui/dialog.tsx` (check shadcn install).

- [ ] **Step 1: `activate-spa-theme-dialog.tsx`**

```tsx
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import type { SpaThemeSummary } from '@/api/spa-themes'

interface Props {
  theme: SpaThemeSummary | null
  open: boolean
  onOpenChange: (v: boolean) => void
  onConfirm: () => void
}

export function ActivateSpaThemeDialog({ theme, open, onOpenChange, onConfirm }: Props) {
  const { t } = useTranslation('spa-theme')
  const [agreed, setAgreed] = useState(false)
  if (!theme) return null
  return (
    <Dialog open={open} onOpenChange={(v) => { setAgreed(false); onOpenChange(v) }}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('activate_dialog.title', { name: theme.name, version: theme.version })}</DialogTitle>
          <DialogDescription>{t('activate_dialog.body')}</DialogDescription>
        </DialogHeader>
        <p className="text-sm text-muted-foreground">{t('activate_dialog.recovery_hint')}</p>
        <label className="flex items-center gap-2 text-sm cursor-pointer">
          <Checkbox checked={agreed} onCheckedChange={(v) => setAgreed(Boolean(v))} />
          {t('activate_dialog.confirm_checkbox')}
        </label>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>{t('activate_dialog.cancel')}</Button>
          <Button disabled={!agreed} onClick={onConfirm}>{t('activate_dialog.confirm')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
```

- [ ] **Step 2: `preview-confirm-dialog.tsx`** mirrors with single confirm button (no checkbox); on confirm calls back, which opens `/?theme=preview:<uuid>` in a new tab.

```tsx
import { useTranslation } from 'react-i18next'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'

interface Props { open: boolean; onOpenChange: (v: boolean) => void; onConfirm: () => void }

export function PreviewConfirmDialog({ open, onOpenChange, onConfirm }: Props) {
  const { t } = useTranslation('spa-theme')
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('preview_dialog.title')}</DialogTitle>
          <DialogDescription>{t('preview_dialog.body')}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>{t('preview_dialog.cancel')}</Button>
          <Button onClick={onConfirm}>{t('preview_dialog.confirm')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
```

- [ ] **Step 3: Tests**

For `ActivateSpaThemeDialog`: confirm button disabled until checkbox clicked.
For `PreviewConfirmDialog`: clicking confirm calls `onConfirm`.

- [ ] **Step 4: Run + commit**

```bash
bun run test -- activate-spa-theme-dialog preview-confirm-dialog
bun x ultracite check apps/web/src/components/spa-theme/
git add apps/web/src/components/spa-theme/
git commit -m "feat(web): SPA theme activate/preview confirm dialogs"
```

---

### Task 22: Component — `SpaThemeDetailsDrawer`

**Files:**
- Create: `apps/web/src/components/spa-theme/spa-theme-details-drawer.tsx`
- Create alongside test

Drawer shows manifest JSON (pretty-printed), file list (from re-fetched detail if needed; v1 can fetch fresh `useQuery` for `/api/settings/spa-themes/:uuid` and just show the summary), and a "Download package" button linking to `/api/settings/spa-themes/:uuid/package`.

- [ ] **Step 1: Implement using existing `<Sheet>` shadcn component**

(Inspect `apps/web/src/components/ui/sheet.tsx` first.)

- [ ] **Step 2: Test rendering happy path**

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/spa-theme/spa-theme-details-drawer.tsx apps/web/src/components/spa-theme/spa-theme-details-drawer.test.tsx
git commit -m "feat(web): SPA theme details drawer"
```

---

### Task 23: Orchestrator — `CustomSpaThemeSection` + integration into `/settings/appearance`

**Files:**
- Create: `apps/web/src/components/spa-theme/custom-spa-theme-section.tsx`
- Modify: `apps/web/src/routes/_authed/settings/appearance.tsx`

- [ ] **Step 1: `custom-spa-theme-section.tsx`** assembles upload card + grid of `SpaThemeCard` + dialogs + drawer. Pseudocode:

```tsx
export function CustomSpaThemeSection() {
  const { t } = useTranslation('spa-theme')
  const themes = useSpaThemes()
  const active = useActiveSpaTheme()
  const activate = useActivateSpaTheme()
  const del = useDeleteSpaTheme()

  const [pendingActivate, setPendingActivate] = useState<SpaThemeSummary | null>(null)
  const [pendingPreview, setPendingPreview] = useState<SpaThemeSummary | null>(null)
  const [drawer, setDrawer] = useState<SpaThemeSummary | null>(null)

  return (
    <section className="border-2 border-amber-300/40 rounded-lg p-4 mb-6 bg-amber-50/30 dark:bg-amber-950/20">
      <header className="flex items-center gap-2 mb-3">
        <h2 className="text-lg font-semibold">{t('section_title')}</h2>
        <Badge variant="outline">{t('section_badge')}</Badge>
      </header>
      <p className="text-sm text-muted-foreground mb-4">{t('section_description')}</p>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
        <SpaThemeUploadCard />
        {(themes.data ?? []).map((th) => (
          <SpaThemeCard
            key={th.uuid}
            theme={th}
            onPreview={() => setPendingPreview(th)}
            onActivate={() => setPendingActivate(th)}
            onDeactivate={() => activate.mutate(null)}
            onDelete={() => { if (confirm('Delete?')) del.mutate(th.uuid) }}
            onOpenDetails={() => setDrawer(th)}
          />
        ))}
      </div>

      <PreviewConfirmDialog
        open={!!pendingPreview}
        onOpenChange={(v) => !v && setPendingPreview(null)}
        onConfirm={() => {
          if (pendingPreview) window.open(`/?theme=preview:${pendingPreview.uuid}`, '_blank')
          setPendingPreview(null)
        }}
      />

      <ActivateSpaThemeDialog
        theme={pendingActivate}
        open={!!pendingActivate}
        onOpenChange={(v) => !v && setPendingActivate(null)}
        onConfirm={() => {
          if (pendingActivate) activate.mutate(pendingActivate.uuid)
          setPendingActivate(null)
        }}
      />

      <SpaThemeDetailsDrawer theme={drawer} onClose={() => setDrawer(null)} />
    </section>
  )
}
```

- [ ] **Step 2: Mount in `appearance.tsx`**

Add import + place `<CustomSpaThemeSection />` at the top of `AppearancePage`'s return, ABOVE the existing `<LegacyMigrationPrompt />`. Only render for admin (use `useAuth()` hook already imported in the file).

Also: when `active.data?.theme_id` is set, render a banner above the existing `<ThemeGrid />` and `<BrandSettingsSection />` reading `t('color_brand_disabled_banner')`.

- [ ] **Step 3: Visual check**

```bash
make web-dev-prod   # or `bun --filter @serverbee/web dev` against local server
```

Open `http://localhost:5173/settings/appearance` and confirm: section visible for admin, banner visible when SPA theme is active.

If browser-based verification is unavailable in the worker environment, run `bun x ultracite check` + `bun run typecheck` and state explicitly that visual verification is pending.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/components/spa-theme/custom-spa-theme-section.tsx apps/web/src/routes/_authed/settings/appearance.tsx
git commit -m "feat(web): custom SPA theme section integrated into /settings/appearance"
```

---

## Phase 8 — Starter Template

### Task 24: Starter template scaffold

**Files (all under `templates/serverbee-theme-starter/`):**

- [ ] **Step 1: Create directory + files**

```bash
mkdir -p templates/serverbee-theme-starter/{src/lib,public}
```

`manifest.json`:

```json
{
  "schema_version": 1,
  "id": "starter",
  "name": "ServerBee Starter Theme",
  "version": "0.1.0",
  "author": "you",
  "description": "Starter template for custom ServerBee SPA themes",
  "entry": "index.html",
  "preview": "preview.png"
}
```

Note: `min_serverbee_version` deliberately omitted (alpha-prerelease gotcha — see spec § 3.1).

`package.json`:

```json
{
  "name": "serverbee-theme-starter",
  "version": "0.1.0",
  "type": "module",
  "private": true,
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "pack": "bun run build && bun run pack.ts",
    "typecheck": "tsc --noEmit"
  },
  "dependencies": {
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^5.0.0",
    "typescript": "^5.6.0",
    "vite": "^7.0.0",
    "jszip": "^3.10.0"
  }
}
```

`vite.config.ts`:

```ts
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
export default defineConfig({
  base: '/', // REQUIRED — see spec §9.1
  plugins: [react()],
  build: { outDir: 'dist', emptyOutDir: true },
})
```

`index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>ServerBee — Starter Theme</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`src/main.tsx`:

```tsx
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import './index.css'

createRoot(document.getElementById('root')!).render(<StrictMode><App /></StrictMode>)
```

`src/App.tsx`:

```tsx
import { useEffect, useState } from 'react'
import { fetchServers, type Server } from './lib/serverbee'

export function App() {
  const [servers, setServers] = useState<Server[]>([])
  const [error, setError] = useState<string | null>(null)
  useEffect(() => { fetchServers().then(setServers).catch((e) => setError(String(e))) }, [])
  if (error) return <pre style={{ color: 'red' }}>{error}</pre>
  return (
    <div style={{ fontFamily: 'system-ui', padding: 16 }}>
      <h1>Starter Theme</h1>
      <ul>{servers.map((s) => <li key={s.id}>{s.name}</li>)}</ul>
    </div>
  )
}
```

`src/lib/serverbee.ts`:

```ts
export interface Server { id: string; name: string }

async function get<T>(path: string): Promise<T> {
  const res = await fetch(path, { credentials: 'include' })
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`)
  const j = await res.json()
  return j.data as T
}

export const fetchServers = () => get<Server[]>('/api/servers')
```

`src/index.css`:

```css
* { box-sizing: border-box; }
body { margin: 0; }
```

`pack.ts`:

```ts
import fs from 'node:fs'
import path from 'node:path'
import JSZip from 'jszip'

const ROOT = path.resolve('.')
const DIST = path.join(ROOT, 'dist')
const MANIFEST = path.join(ROOT, 'manifest.json')
const PREVIEW = path.join(ROOT, 'public', 'preview.png')

const ALLOWED = new Set(['html','htm','js','mjs','css','png','jpg','jpeg','svg','webp','gif','ico','woff','woff2','ttf','otf','json','txt','map'])
const MAX_FILE = 5 * 1024 * 1024
const MAX_TOTAL = 20 * 1024 * 1024
const MAX_COUNT = 1000

function walk(dir: string, base = ''): { rel: string; abs: string }[] {
  const out: { rel: string; abs: string }[] = []
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const abs = path.join(dir, entry.name)
    const rel = path.join(base, entry.name).split(path.sep).join('/')
    if (entry.isDirectory()) out.push(...walk(abs, rel))
    else out.push({ rel, abs })
  }
  return out
}

const manifest = JSON.parse(fs.readFileSync(MANIFEST, 'utf8'))
const zip = new JSZip()
zip.file('manifest.json', fs.readFileSync(MANIFEST))
if (fs.existsSync(PREVIEW)) zip.file('preview.png', fs.readFileSync(PREVIEW))

const distFiles = walk(DIST)
if (distFiles.length + 2 > MAX_COUNT) throw new Error(`too many files (${distFiles.length})`)
let total = 0
for (const { rel, abs } of distFiles) {
  const ext = rel.split('.').pop()?.toLowerCase() ?? ''
  if (!ALLOWED.has(ext)) throw new Error(`disallowed extension: ${rel}`)
  const size = fs.statSync(abs).size
  if (size > MAX_FILE) throw new Error(`file too large: ${rel} (${size})`)
  total += size
  if (total > MAX_TOTAL) throw new Error(`total size exceeded: ${total}`)
  zip.file(rel, fs.readFileSync(abs))
}

const out = `${manifest.id}-${manifest.version}.sbtheme`
const blob = await zip.generateAsync({ type: 'nodebuffer', compression: 'DEFLATE' })
fs.writeFileSync(out, blob)
console.log(`wrote ${out} (${blob.byteLength} bytes)`)
```

`README.md`:

```md
# ServerBee Theme Starter

```sh
bun install
bun run dev      # against http://localhost:9527
bun run pack     # → starter-0.1.0.sbtheme
```

Upload the generated `.sbtheme` at `/settings/appearance` (admin only).
See [docs](https://...) for the manifest reference and CSP constraints.
```

`.gitignore`:

```
node_modules/
dist/
*.sbtheme
```

`public/preview.png`: ship a 1×1 PNG placeholder (admin can replace).

```bash
printf '\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\rIDATx\x9cc\xfa\xcf\x00\x00\x00\x02\x00\x01\xe5\x27\xde\xfc\x00\x00\x00\x00IEND\xaeB`\x82' > templates/serverbee-theme-starter/public/preview.png
```

- [ ] **Step 2: `bun install` inside the template; verify build**

```bash
cd templates/serverbee-theme-starter && bun install && bun run build && bun run pack && ls *.sbtheme
```

- [ ] **Step 3: Commit (exclude the generated `.sbtheme`)**

```bash
cd ../..
git add templates/serverbee-theme-starter/
git commit -m "feat(templates): SPA theme starter template with pack.ts"
```

---

## Phase 9 — Documentation + Manual Checklist

### Task 25: Documentation pages (cn + en)

**Files:**
- Create: `apps/docs/content/docs/en/themes/custom-frontend.mdx`
- Create: `apps/docs/content/docs/cn/themes/custom-frontend.mdx`
- Create: `apps/docs/content/docs/en/themes/meta.json` (or update parent meta if themes/ is new)
- Modify: `apps/docs/content/docs/en/configuration.mdx`, `apps/docs/content/docs/cn/configuration.mdx`

- [ ] **Step 1: Inspect a sibling doc page for shape**

```bash
head -40 apps/docs/content/docs/en/custom-themes.mdx
```

- [ ] **Step 2: Write `custom-frontend.mdx` with the 8 sections enumerated in spec §9.2**

  1. What can / cannot be customized
  2. Quickstart (`degit` template → edit → pack → upload)
  3. Manifest reference (full table including the `min_serverbee_version` alpha gotcha)
  4. API & WebSocket reference (link to `/swagger-ui/`)
  5. CSP constraints (`connect-src 'self'` etc.)
  6. Recovery and debugging (`?theme=default`, `/__system/clear-recovery`, DevTools CSP violations)
  7. Best practices (i18n, dark mode, accessibility, mobile)
  8. Size and file limits (mirror spec § 3.2 table)

- [ ] **Step 3: cn translation, same structure**

- [ ] **Step 4: Add a sentence + recovery URL example in `configuration.mdx` (both langs)**

- [ ] **Step 5: Build docs once to confirm MDX parses**

```bash
cd apps/docs && bun install && bun run build
```

- [ ] **Step 6: Commit**

```bash
cd ../..
git add apps/docs/
git commit -m "docs: add custom frontend theme guide (en + cn)"
```

---

### Task 26: Manual E2E checklist

**Files:**
- Create: `tests/spa-themes.md`

- [ ] **Step 1: Mirror the checklist in spec § 10.4** plus the round-2 additions (`?theme=preview`, `UPLOAD_TOO_LARGE` JSON body assertion, deep-link reload, `NO_DOWNGRADE`, `VERSION_EXISTS`, audit log entries).

- [ ] **Step 2: Add a "Setup" section** describing how to start the local server, log in as admin, upload `templates/serverbee-theme-starter/starter-0.1.0.sbtheme`.

- [ ] **Step 3: Commit**

```bash
git add tests/spa-themes.md
git commit -m "test: add SPA theme manual E2E checklist"
```

---

## Final verification

- [ ] Run full test suites

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
bun run test
bun run typecheck
bun x ultracite check
```

All should pass with zero warnings.

- [ ] Visual smoke test — see spec § 10.4 / `tests/spa-themes.md`. Upload starter theme, preview, activate, exit, recover via `?theme=default`. Tick every box in the checklist or document specific gaps.

- [ ] Last commit — bump CHANGELOG entry under `[Unreleased]`:

```bash
$EDITOR CHANGELOG.md  # add bullet under [Unreleased]: feat: custom SPA themes
git add CHANGELOG.md
git commit -m "chore: changelog entry for custom SPA themes"
```
