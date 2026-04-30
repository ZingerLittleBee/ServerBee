# Custom Theme System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在现有 8 套预设的基础上,支持用户自定义后台与每个状态页的主题(精简主题包模型)。

**Architecture:** 主题 = JSON CSS 变量集合,服务端落库;`/api/settings/active-theme` 返回 resolved payload(预设/自定义 tagged union)供首屏一次性应用;状态页可独立绑定主题,公开页面 scoped 注入到根节点 `<div class="status-page-root">`。后台编辑器双栏(变量编辑 + 隔离实时预览)。

**Tech Stack:** Rust(Axum 0.8 / sea-orm / SQLite / utoipa),React 19 / TanStack Router+Query / shadcn/ui / Vite,色彩转换库 `culori`(待依赖确认)。

**Spec:** `docs/superpowers/specs/2026-04-30-custom-theme-design.md`

---

## File Structure

### Backend (Rust)
- Create: `crates/server/src/migration/m20260430_000019_create_custom_theme.rs`
- Create: `crates/server/src/migration/m20260430_000020_add_status_page_theme_ref.rs`
- Modify: `crates/server/src/migration/mod.rs` — register new migrations
- Create: `crates/server/src/entity/custom_theme.rs`
- Modify: `crates/server/src/entity/status_page.rs` — add `theme_ref: Option<String>`
- Modify: `crates/server/src/entity/mod.rs` — export new entity
- Create: `crates/server/src/service/theme_validator.rs` — variable whitelist + OKLCH regex + range checks
- Create: `crates/server/src/service/theme_ref.rs` — URN parser/validator/reference list
- Create: `crates/server/src/service/custom_theme.rs` — CRUD + active-theme resolution
- Modify: `crates/server/src/service/status_page.rs` — accept `theme_ref` in update; return resolved theme on public read
- Modify: `crates/server/src/service/mod.rs`
- Create: `crates/server/src/router/api/theme.rs` — REST endpoints
- Modify: `crates/server/src/router/api/mod.rs` — wire up
- Modify: `crates/server/src/router/api/status_page.rs` — accept `theme_ref` in DTO; embed `theme: ThemeResolved` in `PublicStatusPageData`
- Modify: `crates/server/src/config.rs` — add `feature.custom_themes: bool` (default true)
- Create: `crates/server/tests/custom_theme_integration.rs`

### Frontend (TS/React)
- Modify: `apps/web/package.json` — add `culori`
- Create: `apps/web/src/themes/preset-vars.ts` — full variable maps for 8 presets
- Create: `apps/web/src/themes/preset-vars.test.ts` — invariant test vs CSS files
- Modify: `apps/web/src/themes/index.ts` — export `PresetThemeId`
- Create: `apps/web/src/lib/theme-ref.ts` — URN parsing
- Create: `apps/web/src/lib/theme-ref.test.ts`
- Create: `apps/web/src/lib/oklch.ts` — hex ↔ oklch via `culori`
- Create: `apps/web/src/lib/oklch.test.ts`
- Modify: `apps/web/src/components/theme-provider.tsx` — apply resolved payload + cache
- Create: `apps/web/src/components/theme-provider.test.tsx` (or extend existing)
- Create: `apps/web/src/api/themes.ts` — TanStack Query hooks
- Modify: `apps/web/src/routes/_authed/settings/appearance.tsx` — add custom-theme grid + delete dialog
- Create: `apps/web/src/routes/_authed/settings/appearance/themes.new.tsx`
- Create: `apps/web/src/routes/_authed/settings/appearance/themes.$id.tsx`
- Create: `apps/web/src/components/theme/theme-card.tsx`
- Create: `apps/web/src/components/theme/theme-editor.tsx`
- Create: `apps/web/src/components/theme/theme-preview.tsx`
- Create: `apps/web/src/components/theme/oklch-picker.tsx`
- Create: `apps/web/src/components/theme/delete-theme-dialog.tsx`
- Modify: `apps/web/src/routes/_authed/settings/status-pages.tsx` — add theme selector to status-page edit form
- Modify: `apps/web/src/routes/status.$slug.tsx` — apply scoped theme
- Modify: `apps/web/src/locales/{zh,en}/settings.json` — `appearance.custom_themes.*` / `appearance.editor.*`
- Modify: `apps/web/src/locales/{zh,en}/status.json` — `theme.*`

### Docs
- Create: `apps/docs/content/docs/cn/custom-themes.mdx`
- Create: `apps/docs/content/docs/en/custom-themes.mdx`
- Create: `tests/appearance/custom-theme.md` — E2E manual checklist

---

## Milestone 1 · Data Layer

### Task 1: Migration — `custom_theme` table

**Files:**
- Create: `crates/server/src/migration/m20260430_000019_create_custom_theme.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Write the migration**

```rust
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260430_000019_create_custom_theme"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS custom_theme (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                description TEXT NULL,
                based_on TEXT NULL,
                vars_light TEXT NOT NULL,
                vars_dark TEXT NOT NULL,
                created_by TEXT NOT NULL,
                created_at DATETIME NOT NULL,
                updated_at DATETIME NOT NULL
            )",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_custom_theme_updated_at
                ON custom_theme (updated_at DESC)",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

- [ ] **Step 2: Register migration**

In `crates/server/src/migration/mod.rs`, add `mod m20260430_000019_create_custom_theme;` to the module list and add `Box::new(m20260430_000019_create_custom_theme::Migration),` after the last existing entry in the `vec![...]`.

- [ ] **Step 3: Verify build**

Run: `cargo build -p serverbee-server`
Expected: builds; new migration name shows up under `serverbee-server` crate.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/migration/m20260430_000019_create_custom_theme.rs crates/server/src/migration/mod.rs
git commit -m "feat(server): add custom_theme migration"
```

---

### Task 2: Migration — `status_page.theme_ref`

**Files:**
- Create: `crates/server/src/migration/m20260430_000020_add_status_page_theme_ref.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Write the migration**

```rust
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260430_000020_add_status_page_theme_ref"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "ALTER TABLE status_page ADD COLUMN theme_ref TEXT NULL",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

- [ ] **Step 2: Register migration**

Add `mod m20260430_000020_add_status_page_theme_ref;` and `Box::new(m20260430_000020_add_status_page_theme_ref::Migration),` to `migration/mod.rs`.

- [ ] **Step 3: Verify build**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/migration/m20260430_000020_add_status_page_theme_ref.rs crates/server/src/migration/mod.rs
git commit -m "feat(server): add status_page.theme_ref column"
```

---

### Task 3: Entity — `custom_theme` + `status_page.theme_ref`

**Files:**
- Create: `crates/server/src/entity/custom_theme.rs`
- Modify: `crates/server/src/entity/mod.rs`
- Modify: `crates/server/src/entity/status_page.rs`

- [ ] **Step 1: Write the entity**

```rust
// crates/server/src/entity/custom_theme.rs
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "custom_theme")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
    pub based_on: Option<String>,
    pub vars_light: String,
    pub vars_dark: String,
    pub created_by: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 2: Wire up module**

In `crates/server/src/entity/mod.rs`, add `pub mod custom_theme;`.

- [ ] **Step 3: Extend status_page entity**

Open `crates/server/src/entity/status_page.rs`. Add a new field below the existing fields in `Model`:

```rust
    pub theme_ref: Option<String>,
```

(Place it after the existing primary fields, before `created_at`.)

- [ ] **Step 4: Verify build**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/entity/custom_theme.rs crates/server/src/entity/mod.rs crates/server/src/entity/status_page.rs
git commit -m "feat(server): add custom_theme entity and status_page.theme_ref field"
```

---

### Task 4: `theme_validator` service + tests

**Files:**
- Create: `crates/server/src/service/theme_validator.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Write the failing test**

Create the file with both production and test code (single file with `#[cfg(test)] mod tests`). First write the API surface plus tests:

```rust
// crates/server/src/service/theme_validator.rs
use std::collections::{HashMap, HashSet};

use once_cell::sync::Lazy;
use regex::Regex;

use crate::error::AppError;

/// 主题变量白名单 — 必须全部出现,顺序无关
pub const REQUIRED_VARS: &[&str] = &[
    "background", "foreground",
    "card", "card-foreground",
    "popover", "popover-foreground",
    "primary", "primary-foreground",
    "secondary", "secondary-foreground",
    "muted", "muted-foreground",
    "accent", "accent-foreground",
    "destructive",
    "border", "input", "ring",
    "chart-1", "chart-2", "chart-3", "chart-4", "chart-5",
    "sidebar", "sidebar-foreground",
    "sidebar-primary", "sidebar-primary-foreground",
    "sidebar-accent", "sidebar-accent-foreground",
    "sidebar-border", "sidebar-ring",
];

static OKLCH_RE: Lazy<Regex> = Lazy::new(|| {
    // oklch(L C H) or oklch(L C H / α) where α is number 0..1 or percent 0..100
    Regex::new(r"^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)(?:\s*/\s*([\d.]+)(%)?)?\s*\)$")
        .expect("static regex")
});

pub type VarMap = HashMap<String, String>;

pub fn validate_var_map(map: &VarMap) -> Result<(), AppError> {
    let required: HashSet<&str> = REQUIRED_VARS.iter().copied().collect();
    let actual: HashSet<&str> = map.keys().map(|s| s.as_str()).collect();

    if let Some(missing) = required.difference(&actual).next() {
        return Err(AppError::Validation(format!("missing variable: {missing}")));
    }
    if let Some(extra) = actual.difference(&required).next() {
        return Err(AppError::Validation(format!("unknown variable: {extra}")));
    }

    for key in REQUIRED_VARS {
        let value = &map[*key];
        validate_oklch_value(key, value)?;
    }
    Ok(())
}

fn validate_oklch_value(key: &str, value: &str) -> Result<(), AppError> {
    let caps = OKLCH_RE
        .captures(value)
        .ok_or_else(|| AppError::Validation(format!("{key}: invalid oklch syntax")))?;

    let l: f64 = caps[1].parse().map_err(|_| AppError::Validation(format!("{key}: L not a number")))?;
    let _c: f64 = caps[2].parse().map_err(|_| AppError::Validation(format!("{key}: C not a number")))?;
    let h: f64 = caps[3].parse().map_err(|_| AppError::Validation(format!("{key}: H not a number")))?;

    if !(0.0..=1.0).contains(&l) {
        return Err(AppError::Validation(format!("{key}: L out of range [0,1]")));
    }
    if !(0.0..=360.0).contains(&h) {
        return Err(AppError::Validation(format!("{key}: H out of range [0,360]")));
    }
    // C: no hard cap (spec §7.4 — high-chroma values are legal, only gamut-clipped at render).

    if let Some(alpha_match) = caps.get(4) {
        let alpha: f64 = alpha_match.as_str().parse().map_err(|_| {
            AppError::Validation(format!("{key}: alpha not a number"))
        })?;
        let is_percent = caps.get(5).is_some();
        let max = if is_percent { 100.0 } else { 1.0 };
        if !(0.0..=max).contains(&alpha) {
            return Err(AppError::Validation(format!("{key}: alpha out of range")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_map(value: &str) -> VarMap {
        REQUIRED_VARS.iter().map(|k| ((*k).to_string(), value.to_string())).collect()
    }

    #[test]
    fn accepts_valid_full_map() {
        let map = full_map("oklch(0.5 0.1 180)");
        assert!(validate_var_map(&map).is_ok());
    }

    #[test]
    fn accepts_alpha_number() {
        let map = full_map("oklch(0.5 0.1 180 / 0.5)");
        assert!(validate_var_map(&map).is_ok());
    }

    #[test]
    fn accepts_alpha_percent() {
        let map = full_map("oklch(1 0 0 / 10%)");
        assert!(validate_var_map(&map).is_ok());
    }

    #[test]
    fn rejects_missing_var() {
        let mut map = full_map("oklch(0.5 0.1 180)");
        map.remove("background");
        let err = validate_var_map(&map).unwrap_err();
        assert!(err.to_string().contains("missing variable"));
    }

    #[test]
    fn rejects_unknown_var() {
        let mut map = full_map("oklch(0.5 0.1 180)");
        map.insert("rogue".into(), "oklch(0.5 0.1 180)".into());
        let err = validate_var_map(&map).unwrap_err();
        assert!(err.to_string().contains("unknown variable"));
    }

    #[test]
    fn rejects_l_out_of_range() {
        let map = full_map("oklch(1.5 0.1 180)");
        assert!(validate_var_map(&map).is_err());
    }

    #[test]
    fn rejects_h_out_of_range() {
        let map = full_map("oklch(0.5 0.1 400)");
        assert!(validate_var_map(&map).is_err());
    }

    #[test]
    fn allows_high_chroma() {
        // Spec §7.4: C has no hard cap.
        let map = full_map("oklch(0.5 0.9 180)");
        assert!(validate_var_map(&map).is_ok());
    }

    #[test]
    fn rejects_alpha_over_one() {
        let map = full_map("oklch(0.5 0.1 180 / 1.5)");
        assert!(validate_var_map(&map).is_err());
    }

    #[test]
    fn rejects_alpha_percent_over_hundred() {
        let map = full_map("oklch(0.5 0.1 180 / 150%)");
        assert!(validate_var_map(&map).is_err());
    }
}
```

- [ ] **Step 2: Add `regex` and `once_cell` if not present**

Check `crates/server/Cargo.toml`. If `regex` is not in `[dependencies]`, add `regex = "1"`. If `once_cell` is not present, add `once_cell = "1"`. (sea-orm pulls regex transitively in many projects; double-check before adding.)

- [ ] **Step 3: Wire up module**

In `crates/server/src/service/mod.rs`, add `pub mod theme_validator;`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p serverbee-server theme_validator`
Expected: all 10 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/theme_validator.rs crates/server/src/service/mod.rs crates/server/Cargo.toml
git commit -m "feat(server): add theme variable validator with OKLCH range checks"
```

---

### Task 5: `theme_ref` service (URN parser + reference list) + tests

**Files:**
- Create: `crates/server/src/service/theme_ref.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Write the module + tests**

```rust
// crates/server/src/service/theme_ref.rs
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::Serialize;

use crate::entity::{custom_theme, status_page};
use crate::error::AppError;

const PRESET_IDS: &[&str] = &[
    "default", "tokyo-night", "nord", "catppuccin",
    "dracula", "one-dark", "solarized", "rose-pine",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeRef {
    Preset(String),
    Custom(i32),
}

impl ThemeRef {
    pub fn parse(s: &str) -> Result<Self, AppError> {
        if let Some(id) = s.strip_prefix("preset:") {
            if !PRESET_IDS.contains(&id) {
                return Err(AppError::Validation(format!("unknown preset: {id}")));
            }
            return Ok(ThemeRef::Preset(id.to_string()));
        }
        if let Some(rest) = s.strip_prefix("custom:") {
            let id: i32 = rest.parse().map_err(|_| {
                AppError::Validation(format!("invalid custom id: {rest}"))
            })?;
            return Ok(ThemeRef::Custom(id));
        }
        Err(AppError::Validation(format!("malformed theme ref: {s}")))
    }

    pub fn to_urn(&self) -> String {
        match self {
            ThemeRef::Preset(id) => format!("preset:{id}"),
            ThemeRef::Custom(id) => format!("custom:{id}"),
        }
    }
}

pub async fn validate_theme_ref(db: &DatabaseConnection, r: &ThemeRef) -> Result<(), AppError> {
    match r {
        ThemeRef::Preset(_) => Ok(()),
        ThemeRef::Custom(id) => {
            let exists = custom_theme::Entity::find_by_id(*id).one(db).await?.is_some();
            if !exists {
                return Err(AppError::Validation(format!("custom theme {id} not found")));
            }
            Ok(())
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ThemeReferences {
    pub admin: bool,
    pub status_pages: Vec<StatusPageRef>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StatusPageRef {
    pub id: String,
    pub name: String,
}

pub async fn list_references(
    db: &DatabaseConnection,
    custom_id: i32,
) -> Result<ThemeReferences, AppError> {
    let urn = format!("custom:{custom_id}");

    // Admin global active theme
    let admin = crate::service::config::ConfigService::get(db, "active_admin_theme")
        .await?
        .map(|v| v == urn)
        .unwrap_or(false);

    // Status pages
    let status_pages = status_page::Entity::find()
        .filter(status_page::Column::ThemeRef.eq(urn.clone()))
        .all(db)
        .await?
        .into_iter()
        .map(|m| StatusPageRef { id: m.id, name: m.name })
        .collect();

    Ok(ThemeReferences { admin, status_pages })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_preset() {
        assert_eq!(ThemeRef::parse("preset:default").unwrap(), ThemeRef::Preset("default".into()));
    }

    #[test]
    fn parses_custom() {
        assert_eq!(ThemeRef::parse("custom:42").unwrap(), ThemeRef::Custom(42));
    }

    #[test]
    fn rejects_unknown_preset() {
        assert!(ThemeRef::parse("preset:nonsense").is_err());
    }

    #[test]
    fn rejects_bad_custom_id() {
        assert!(ThemeRef::parse("custom:abc").is_err());
    }

    #[test]
    fn rejects_unknown_scheme() {
        assert!(ThemeRef::parse("foo:bar").is_err());
    }

    #[test]
    fn round_trips_urn() {
        let r = ThemeRef::Preset("nord".into());
        assert_eq!(ThemeRef::parse(&r.to_urn()).unwrap(), r);
    }
}
```

- [ ] **Step 2: Wire up module**

In `crates/server/src/service/mod.rs`, add `pub mod theme_ref;`.

- [ ] **Step 3: Verify status_page entity exposes Column::ThemeRef**

Run: `cargo build -p serverbee-server`
If the build fails because `Column::ThemeRef` is missing, ensure Task 3 Step 3 ran (the new field on `status_page::Model`).

- [ ] **Step 4: Run unit tests**

Run: `cargo test -p serverbee-server theme_ref`
Expected: 6 unit tests PASS. (DB-touching `validate_theme_ref` / `list_references` tested in integration suite later.)

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/theme_ref.rs crates/server/src/service/mod.rs
git commit -m "feat(server): add theme_ref URN parser and reference resolver"
```

---

### Task 6: `CustomThemeService` (CRUD + active-theme resolution)

**Files:**
- Create: `crates/server/src/service/custom_theme.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Write the service**

```rust
// crates/server/src/service/custom_theme.rs
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::{Deserialize, Serialize};

use crate::entity::custom_theme;
use crate::error::AppError;
use crate::service::config::ConfigService;
use crate::service::theme_ref::{self, ThemeRef};
use crate::service::theme_validator::{self, VarMap};

const ACTIVE_THEME_KEY: &str = "active_admin_theme";
const DEFAULT_REF: &str = "preset:default";

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ThemeSummary {
    pub id: i32,
    pub name: String,
    pub based_on: Option<String>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Theme {
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
    pub based_on: Option<String>,
    pub vars_light: VarMap,
    pub vars_dark: VarMap,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateThemeInput {
    pub name: String,
    pub description: Option<String>,
    pub based_on: Option<String>,
    pub vars_light: VarMap,
    pub vars_dark: VarMap,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateThemeInput {
    pub name: String,
    pub description: Option<String>,
    pub based_on: Option<String>,
    pub vars_light: VarMap,
    pub vars_dark: VarMap,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThemeResolved {
    Preset {
        id: String,
    },
    Custom {
        id: i32,
        name: String,
        vars_light: VarMap,
        vars_dark: VarMap,
        updated_at: chrono::DateTime<Utc>,
    },
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ActiveThemeResponse {
    pub r#ref: String,
    pub theme: ThemeResolved,
}

pub struct CustomThemeService;

impl CustomThemeService {
    pub async fn list(db: &DatabaseConnection) -> Result<Vec<ThemeSummary>, AppError> {
        let rows = custom_theme::Entity::find()
            .order_by_desc(custom_theme::Column::UpdatedAt)
            .all(db)
            .await?;
        Ok(rows.into_iter().map(|m| ThemeSummary {
            id: m.id,
            name: m.name,
            based_on: m.based_on,
            updated_at: m.updated_at,
        }).collect())
    }

    pub async fn get(db: &DatabaseConnection, id: i32) -> Result<Theme, AppError> {
        let m = custom_theme::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("theme {id}")))?;
        Ok(model_to_theme(&m)?)
    }

    pub async fn create(
        db: &DatabaseConnection,
        input: CreateThemeInput,
        user_id: &str,
    ) -> Result<Theme, AppError> {
        theme_validator::validate_var_map(&input.vars_light)?;
        theme_validator::validate_var_map(&input.vars_dark)?;
        let now = Utc::now();
        let am = custom_theme::ActiveModel {
            name: Set(input.name),
            description: Set(input.description),
            based_on: Set(input.based_on),
            vars_light: Set(serde_json::to_string(&input.vars_light)?),
            vars_dark: Set(serde_json::to_string(&input.vars_dark)?),
            created_by: Set(user_id.to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        let m = am.insert(db).await?;
        Ok(model_to_theme(&m)?)
    }

    pub async fn update(
        db: &DatabaseConnection,
        id: i32,
        input: UpdateThemeInput,
    ) -> Result<Theme, AppError> {
        theme_validator::validate_var_map(&input.vars_light)?;
        theme_validator::validate_var_map(&input.vars_dark)?;
        let existing = custom_theme::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("theme {id}")))?;
        let mut am: custom_theme::ActiveModel = existing.into();
        am.name = Set(input.name);
        am.description = Set(input.description);
        am.based_on = Set(input.based_on);
        am.vars_light = Set(serde_json::to_string(&input.vars_light)?);
        am.vars_dark = Set(serde_json::to_string(&input.vars_dark)?);
        am.updated_at = Set(Utc::now());
        let m = am.update(db).await?;
        Ok(model_to_theme(&m)?)
    }

    pub async fn duplicate(db: &DatabaseConnection, id: i32, user_id: &str) -> Result<Theme, AppError> {
        let src = Self::get(db, id).await?;
        Self::create(
            db,
            CreateThemeInput {
                name: format!("{} (copy)", src.name),
                description: src.description,
                based_on: src.based_on,
                vars_light: src.vars_light,
                vars_dark: src.vars_dark,
            },
            user_id,
        ).await
    }

    pub async fn delete(db: &DatabaseConnection, id: i32) -> Result<(), AppError> {
        let refs = theme_ref::list_references(db, id).await?;
        if refs.admin || !refs.status_pages.is_empty() {
            return Err(AppError::Conflict(
                "Theme is in use by admin or one or more status pages; unbind it first.".into(),
            ));
        }
        let res = custom_theme::Entity::delete_by_id(id).exec(db).await?;
        if res.rows_affected == 0 {
            return Err(AppError::NotFound(format!("theme {id}")));
        }
        Ok(())
    }

    /// 读取后台激活主题,返回 resolved payload(含 vars 真值)。
    /// feature flag 关闭时,custom:* 在读时降级为 preset:default。
    pub async fn active_theme(
        db: &DatabaseConnection,
        feature_enabled: bool,
    ) -> Result<ActiveThemeResponse, AppError> {
        let raw = ConfigService::get(db, ACTIVE_THEME_KEY)
            .await?
            .unwrap_or_else(|| DEFAULT_REF.to_string());
        let parsed = ThemeRef::parse(&raw).unwrap_or(ThemeRef::Preset("default".into()));
        let coerced = match parsed {
            ThemeRef::Custom(_) if !feature_enabled => ThemeRef::Preset("default".into()),
            other => other,
        };
        Self::resolve(db, coerced).await
    }

    pub async fn set_active_theme(
        db: &DatabaseConnection,
        urn: &str,
        feature_enabled: bool,
    ) -> Result<ActiveThemeResponse, AppError> {
        let parsed = ThemeRef::parse(urn)?;
        if !feature_enabled && matches!(parsed, ThemeRef::Custom(_)) {
            return Err(AppError::Validation("custom theme feature disabled".into()));
        }
        theme_ref::validate_theme_ref(db, &parsed).await?;
        ConfigService::set(db, ACTIVE_THEME_KEY, &parsed.to_urn()).await?;
        Self::resolve(db, parsed).await
    }

    pub async fn resolve(db: &DatabaseConnection, r: ThemeRef) -> Result<ActiveThemeResponse, AppError> {
        match r {
            ThemeRef::Preset(id) => Ok(ActiveThemeResponse {
                r#ref: format!("preset:{id}"),
                theme: ThemeResolved::Preset { id },
            }),
            ThemeRef::Custom(id) => {
                let t = Self::get(db, id).await?;
                Ok(ActiveThemeResponse {
                    r#ref: format!("custom:{id}"),
                    theme: ThemeResolved::Custom {
                        id: t.id,
                        name: t.name,
                        vars_light: t.vars_light,
                        vars_dark: t.vars_dark,
                        updated_at: t.updated_at,
                    },
                })
            }
        }
    }
}

fn model_to_theme(m: &custom_theme::Model) -> Result<Theme, AppError> {
    Ok(Theme {
        id: m.id,
        name: m.name.clone(),
        description: m.description.clone(),
        based_on: m.based_on.clone(),
        vars_light: serde_json::from_str(&m.vars_light)?,
        vars_dark: serde_json::from_str(&m.vars_dark)?,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

impl From<serde_json::Error> for AppError {
    fn from(_: serde_json::Error) -> Self {
        AppError::Internal("theme JSON decode error".into())
    }
}
```

> Note: if `From<serde_json::Error> for AppError` already exists in `error.rs`, omit the `impl From` block at the bottom of this file.

- [ ] **Step 2: Wire up module**

In `crates/server/src/service/mod.rs`, add `pub mod custom_theme;`.

- [ ] **Step 3: Build**

Run: `cargo build -p serverbee-server`
Expected: PASS. Fix any duplicate `From<serde_json::Error>` error by removing the local impl in this file.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/custom_theme.rs crates/server/src/service/mod.rs
git commit -m "feat(server): add CustomThemeService with active-theme resolution"
```

---

## Milestone 2 · API Layer

### Task 7: feature flag config

**Files:**
- Modify: `crates/server/src/config.rs`

- [ ] **Step 1: Add the flag**

Locate the existing `AppConfig` (or equivalent Figment-backed struct). Add a sub-struct or field. Match the prevailing style (e.g., if other flags live under `[feature]`, follow that):

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct FeatureConfig {
    #[serde(default = "default_true")]
    pub custom_themes: bool,
}

fn default_true() -> bool { true }

// Inside AppConfig:
//     #[serde(default = "FeatureConfig::default_struct")]
//     pub feature: FeatureConfig,
```

If `AppConfig` doesn't yet have a `feature` group, add it (with `Default` impl) so `config.feature.custom_themes` is reachable from handlers via `state.config.feature.custom_themes`.

- [ ] **Step 2: Update ENV.md and configuration docs**

Add to `ENV.md` (at the section listing `SERVERBEE_*` env keys):

```
SERVERBEE_FEATURE__CUSTOM_THEMES=true   # default; set to false to disable user-defined themes (custom:* refs are read-coerced to preset:default)
```

Also add the same entry to `apps/docs/content/docs/{cn,en}/configuration.mdx` under the existing feature flag table. (Do this in the same commit so docs and code stay in sync — repo rule from CLAUDE.md.)

- [ ] **Step 3: Build**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/config.rs ENV.md apps/docs/content/docs/cn/configuration.mdx apps/docs/content/docs/en/configuration.mdx
git commit -m "feat(server): add feature.custom_themes flag (default true)"
```

---

### Task 8: theme router + DTOs

**Files:**
- Create: `crates/server/src/router/api/theme.rs`
- Modify: `crates/server/src/router/api/mod.rs`

- [ ] **Step 1: Write the router**

```rust
// crates/server/src/router/api/theme.rs
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{delete, get, post, put},
};
use serde::Deserialize;

use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::AuthUser;
use crate::service::custom_theme::{
    ActiveThemeResponse, CreateThemeInput, CustomThemeService, Theme, ThemeSummary, UpdateThemeInput,
};
use crate::service::theme_ref::ThemeReferences;
use crate::state::AppState;

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/settings/themes", get(list_themes))
        .route("/settings/themes/{id}", get(get_theme))
        .route("/settings/themes/{id}/export", get(export_theme))
        .route("/settings/active-theme", get(get_active_theme))
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/settings/themes", post(create_theme))
        .route("/settings/themes/{id}", put(update_theme))
        .route("/settings/themes/{id}", delete(delete_theme))
        .route("/settings/themes/{id}/references", get(get_references))
        .route("/settings/themes/{id}/duplicate", post(duplicate_theme))
        .route("/settings/themes/import", post(import_theme))
        .route("/settings/active-theme", put(put_active_theme))
}

#[utoipa::path(get, path = "/api/settings/themes", responses((status = 200, body = Vec<ThemeSummary>)))]
async fn list_themes(State(state): State<Arc<AppState>>) -> Result<Json<ApiResponse<Vec<ThemeSummary>>>, AppError> {
    ok(CustomThemeService::list(&state.db).await?)
}

#[utoipa::path(get, path = "/api/settings/themes/{id}", responses((status = 200, body = Theme)))]
async fn get_theme(State(state): State<Arc<AppState>>, Path(id): Path<i32>) -> Result<Json<ApiResponse<Theme>>, AppError> {
    ok(CustomThemeService::get(&state.db, id).await?)
}

#[utoipa::path(get, path = "/api/settings/themes/{id}/export", responses((status = 200, body = ExportPayload)))]
async fn export_theme(State(state): State<Arc<AppState>>, Path(id): Path<i32>) -> Result<Json<ApiResponse<ExportPayload>>, AppError> {
    let t = CustomThemeService::get(&state.db, id).await?;
    ok(ExportPayload {
        version: 1,
        name: t.name,
        description: t.description,
        based_on: t.based_on,
        vars_light: t.vars_light,
        vars_dark: t.vars_dark,
    })
}

#[utoipa::path(post, path = "/api/settings/themes", request_body = CreateThemeInput, responses((status = 200, body = Theme)))]
async fn create_theme(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(input): Json<CreateThemeInput>,
) -> Result<Json<ApiResponse<Theme>>, AppError> {
    if !state.config.feature.custom_themes {
        return Err(AppError::Validation("custom theme feature disabled".into()));
    }
    ok(CustomThemeService::create(&state.db, input, &user.id).await?)
}

#[utoipa::path(put, path = "/api/settings/themes/{id}", request_body = UpdateThemeInput, responses((status = 200, body = Theme)))]
async fn update_theme(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
    Json(input): Json<UpdateThemeInput>,
) -> Result<Json<ApiResponse<Theme>>, AppError> {
    ok(CustomThemeService::update(&state.db, id, input).await?)
}

#[utoipa::path(delete, path = "/api/settings/themes/{id}", responses((status = 200, body = ()), (status = 409)))]
async fn delete_theme(State(state): State<Arc<AppState>>, Path(id): Path<i32>) -> Result<Json<ApiResponse<()>>, AppError> {
    CustomThemeService::delete(&state.db, id).await?;
    ok(())
}

#[utoipa::path(get, path = "/api/settings/themes/{id}/references", responses((status = 200, body = ThemeReferences)))]
async fn get_references(State(state): State<Arc<AppState>>, Path(id): Path<i32>) -> Result<Json<ApiResponse<ThemeReferences>>, AppError> {
    ok(crate::service::theme_ref::list_references(&state.db, id).await?)
}

#[utoipa::path(post, path = "/api/settings/themes/{id}/duplicate", responses((status = 200, body = Theme)))]
async fn duplicate_theme(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Theme>>, AppError> {
    ok(CustomThemeService::duplicate(&state.db, id, &user.id).await?)
}

#[utoipa::path(get, path = "/api/settings/active-theme", responses((status = 200, body = ActiveThemeResponse)))]
async fn get_active_theme(State(state): State<Arc<AppState>>) -> Result<Json<ApiResponse<ActiveThemeResponse>>, AppError> {
    ok(CustomThemeService::active_theme(&state.db, state.config.feature.custom_themes).await?)
}

#[derive(Deserialize, utoipa::ToSchema)]
struct PutActiveThemeInput {
    r#ref: String,
}

#[utoipa::path(put, path = "/api/settings/active-theme", request_body = PutActiveThemeInput, responses((status = 200, body = ActiveThemeResponse)))]
async fn put_active_theme(
    State(state): State<Arc<AppState>>,
    Json(input): Json<PutActiveThemeInput>,
) -> Result<Json<ApiResponse<ActiveThemeResponse>>, AppError> {
    ok(CustomThemeService::set_active_theme(&state.db, &input.r#ref, state.config.feature.custom_themes).await?)
}

#[derive(Deserialize, serde::Serialize, utoipa::ToSchema)]
struct ExportPayload {
    version: u32,
    name: String,
    description: Option<String>,
    based_on: Option<String>,
    vars_light: crate::service::theme_validator::VarMap,
    vars_dark: crate::service::theme_validator::VarMap,
}

#[utoipa::path(post, path = "/api/settings/themes/import", request_body = ExportPayload, responses((status = 200, body = Theme)))]
async fn import_theme(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(input): Json<ExportPayload>,
) -> Result<Json<ApiResponse<Theme>>, AppError> {
    if input.version != 1 {
        return Err(AppError::Validation(format!("unsupported version: {}", input.version)));
    }
    ok(CustomThemeService::create(
        &state.db,
        CreateThemeInput {
            name: input.name,
            description: input.description,
            based_on: input.based_on,
            vars_light: input.vars_light,
            vars_dark: input.vars_dark,
        },
        &user.id,
    ).await?)
}
```

> Adjust `AuthUser` import path / shape to match the existing project. If your auth middleware exposes the user via a different extractor or field name, mirror what `crates/server/src/router/api/user.rs` (or any other handler that needs the current user) does.

- [ ] **Step 2: Mount router**

In `crates/server/src/router/api/mod.rs`:

1. Add `pub mod theme;` to the module list.
2. In `router(state)`, merge `theme::read_router()` alongside other read routers, and `theme::write_router()` alongside other write routers (under the admin middleware tier).

- [ ] **Step 3: Build**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/router/api/theme.rs crates/server/src/router/api/mod.rs
git commit -m "feat(server): add custom theme HTTP endpoints"
```

---

### Task 9: Status page integration — accept theme_ref + return resolved theme

**Files:**
- Modify: `crates/server/src/service/status_page.rs`
- Modify: `crates/server/src/router/api/status_page.rs`

- [ ] **Step 1: Extend update DTO and entity write**

In `crates/server/src/service/status_page.rs`, locate `UpdateStatusPage` (or the equivalent) and add a field:

```rust
    pub theme_ref: Option<Option<String>>, // Some(Some) sets, Some(None) clears, None leaves alone
```

(Use `Option<Option<String>>` to distinguish "omit" vs "clear". If existing patterns use a different convention — e.g., `Option<String>` with a separate `clear: bool` — follow that pattern.)

Inside the service `update` method, when the field is provided, parse + validate via `ThemeRef::parse` and `validate_theme_ref`, then write the URN string (or NULL) into `theme_ref`.

- [ ] **Step 2: Embed resolved theme in PublicStatusPageData**

In `crates/server/src/router/api/status_page.rs`, locate `PublicStatusPageData` and add:

```rust
    pub theme: ThemeResolved,
```

In `get_public_status_page`, after loading the page, resolve the theme:

```rust
let r = match page.theme_ref.as_deref() {
    Some(s) => ThemeRef::parse(s).unwrap_or(ThemeRef::Preset("default".into())),
    None => {
        // Fallback: read global active admin theme.
        let raw = ConfigService::get(&state.db, "active_admin_theme").await?
            .unwrap_or_else(|| "preset:default".into());
        ThemeRef::parse(&raw).unwrap_or(ThemeRef::Preset("default".into()))
    }
};
let coerced = match r {
    ThemeRef::Custom(_) if !state.config.feature.custom_themes => ThemeRef::Preset("default".into()),
    other => other,
};
let resolved = CustomThemeService::resolve(&state.db, coerced).await?.theme;
```

Pass `resolved` into the `PublicStatusPageData { theme: resolved, ... }` constructor.

- [ ] **Step 3: PUT status page accepts theme_ref**

In the admin update route, the request body DTO already maps to `UpdateStatusPage`. Add the `theme_ref` field to that DTO with the same `Option<Option<String>>` shape and route it through.

- [ ] **Step 4: Build**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/status_page.rs crates/server/src/router/api/status_page.rs
git commit -m "feat(server): expose theme_ref on status pages and embed resolved theme in public response"
```

---

### Task 10: Integration tests — theme CRUD + active-theme

**Files:**
- Create: `crates/server/tests/custom_theme_integration.rs`

- [ ] **Step 1: Write the integration test**

```rust
// crates/server/tests/custom_theme_integration.rs

mod common;
use common::{TestApp, admin_login, member_login};
use serde_json::{Value, json};

fn full_vars(value: &str) -> Value {
    let keys = [
        "background", "foreground", "card", "card-foreground",
        "popover", "popover-foreground",
        "primary", "primary-foreground",
        "secondary", "secondary-foreground",
        "muted", "muted-foreground", "accent", "accent-foreground",
        "destructive", "border", "input", "ring",
        "chart-1", "chart-2", "chart-3", "chart-4", "chart-5",
        "sidebar", "sidebar-foreground",
        "sidebar-primary", "sidebar-primary-foreground",
        "sidebar-accent", "sidebar-accent-foreground",
        "sidebar-border", "sidebar-ring",
    ];
    let mut m = serde_json::Map::new();
    for k in keys.iter() {
        m.insert((*k).to_string(), Value::String(value.to_string()));
    }
    Value::Object(m)
}

#[tokio::test]
async fn admin_can_create_get_update_delete_theme() {
    let app = TestApp::start().await;
    let admin = admin_login(&app).await;

    // Create
    let body = json!({
        "name": "My Theme",
        "based_on": "tokyo-night",
        "vars_light": full_vars("oklch(0.9 0.05 200)"),
        "vars_dark":  full_vars("oklch(0.2 0.05 200)"),
    });
    let r: Value = admin.post_json("/api/settings/themes", &body).await;
    let id = r["data"]["id"].as_i64().unwrap();

    // Get
    let r: Value = admin.get_json(&format!("/api/settings/themes/{id}")).await;
    assert_eq!(r["data"]["name"], "My Theme");

    // List
    let r: Value = admin.get_json("/api/settings/themes").await;
    assert!(r["data"].as_array().unwrap().iter().any(|t| t["id"] == id));

    // Update
    let mut body = body.clone();
    body["name"] = json!("Renamed");
    let r: Value = admin.put_json(&format!("/api/settings/themes/{id}"), &body).await;
    assert_eq!(r["data"]["name"], "Renamed");

    // Duplicate
    let r: Value = admin.post_json(&format!("/api/settings/themes/{id}/duplicate"), &json!({})).await;
    assert!(r["data"]["name"].as_str().unwrap().ends_with("(copy)"));

    // Delete (no references)
    admin.delete(&format!("/api/settings/themes/{id}")).await.assert_ok();
}

#[tokio::test]
async fn member_cannot_write_themes() {
    let app = TestApp::start().await;
    let member = member_login(&app).await;
    let body = json!({ "name": "x", "vars_light": full_vars("oklch(0.5 0.1 200)"), "vars_dark": full_vars("oklch(0.3 0.1 200)") });
    member.post_json_expect_status("/api/settings/themes", &body, 403).await;
}

#[tokio::test]
async fn rejects_missing_variable() {
    let app = TestApp::start().await;
    let admin = admin_login(&app).await;
    let mut light = full_vars("oklch(0.5 0.1 200)");
    light.as_object_mut().unwrap().remove("background");
    let body = json!({ "name": "bad", "vars_light": light, "vars_dark": full_vars("oklch(0.3 0.1 200)") });
    admin.post_json_expect_status("/api/settings/themes", &body, 422).await;
}

#[tokio::test]
async fn delete_blocked_when_theme_is_active() {
    let app = TestApp::start().await;
    let admin = admin_login(&app).await;

    let body = json!({ "name": "Active", "vars_light": full_vars("oklch(0.5 0.1 200)"), "vars_dark": full_vars("oklch(0.3 0.1 200)") });
    let r: Value = admin.post_json("/api/settings/themes", &body).await;
    let id = r["data"]["id"].as_i64().unwrap();

    admin.put_json("/api/settings/active-theme", &json!({ "ref": format!("custom:{id}") })).await;
    admin.delete(&format!("/api/settings/themes/{id}")).await.assert_status(409);

    // Unbind, then delete succeeds
    admin.put_json("/api/settings/active-theme", &json!({ "ref": "preset:default" })).await;
    admin.delete(&format!("/api/settings/themes/{id}")).await.assert_ok();
}

#[tokio::test]
async fn active_theme_returns_resolved_payload() {
    let app = TestApp::start().await;
    let admin = admin_login(&app).await;
    let body = json!({ "name": "T", "vars_light": full_vars("oklch(0.9 0.05 200)"), "vars_dark": full_vars("oklch(0.2 0.05 200)") });
    let r: Value = admin.post_json("/api/settings/themes", &body).await;
    let id = r["data"]["id"].as_i64().unwrap();

    admin.put_json("/api/settings/active-theme", &json!({ "ref": format!("custom:{id}") })).await;
    let r: Value = admin.get_json("/api/settings/active-theme").await;
    assert_eq!(r["data"]["theme"]["kind"], "custom");
    assert!(r["data"]["theme"]["vars_light"].is_object());
}
```

> Inspect existing integration tests (e.g. `crates/server/tests/notification_update_integration.rs`) for the exact `common` helpers (`TestApp::start`, `admin_login`, `member_login`, `post_json`, `get_json`, etc.). If a helper doesn't exist with the exact name, use the closest equivalent and adjust the test calls to match.

- [ ] **Step 2: Run the tests**

Run: `cargo test -p serverbee-server --test custom_theme_integration`
Expected: 5 tests PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/server/tests/custom_theme_integration.rs
git commit -m "test(server): add custom theme integration tests"
```

---

## Milestone 3 · Frontend Foundation

### Task 11: Preset variable maps + invariant test

**Files:**
- Create: `apps/web/src/themes/preset-vars.ts`
- Create: `apps/web/src/themes/preset-vars.test.ts`
- Modify: `apps/web/src/themes/index.ts`

- [ ] **Step 1: Write `preset-vars.ts`**

This file is large but mechanical: copy each variable value from the existing CSS files (`apps/web/src/themes/*.css` and the `default` block in `apps/web/src/index.css`) into a TS map.

```ts
// apps/web/src/themes/preset-vars.ts
import type { ColorTheme } from './index'

export type VarMap = Record<string, string>

export interface PresetVars {
  light: VarMap
  dark: VarMap
}

export const PRESET_VAR_KEYS = [
  'background', 'foreground',
  'card', 'card-foreground',
  'popover', 'popover-foreground',
  'primary', 'primary-foreground',
  'secondary', 'secondary-foreground',
  'muted', 'muted-foreground',
  'accent', 'accent-foreground',
  'destructive',
  'border', 'input', 'ring',
  'chart-1', 'chart-2', 'chart-3', 'chart-4', 'chart-5',
  'sidebar', 'sidebar-foreground',
  'sidebar-primary', 'sidebar-primary-foreground',
  'sidebar-accent', 'sidebar-accent-foreground',
  'sidebar-border', 'sidebar-ring',
] as const

// For each preset, transcribe both `:root[data-theme="..."]` and `[data-theme="..."].dark`
// blocks into the maps below. Default values come from `apps/web/src/index.css` :root and .dark.
export const presetVars: Record<ColorTheme, PresetVars> = {
  default: {
    light: {
      // ... transcribe from apps/web/src/index.css :root block ...
    },
    dark: {
      // ... transcribe from apps/web/src/index.css .dark block ...
    },
  },
  'tokyo-night': {
    light: { /* transcribe from apps/web/src/themes/tokyo-night.css [data-theme="tokyo-night"] */ },
    dark:  { /* transcribe from apps/web/src/themes/tokyo-night.css [data-theme="tokyo-night"].dark */ },
  },
  // ... repeat for nord, catppuccin, dracula, one-dark, solarized, rose-pine
}
```

> Implementation note: open each CSS file, copy the variable values verbatim. Do not paraphrase or "fix" colors. The invariant test in Step 2 will catch any drift.

- [ ] **Step 2: Write the invariant test**

```ts
// apps/web/src/themes/preset-vars.test.ts
import { describe, expect, it } from 'vitest'
import fs from 'node:fs'
import path from 'node:path'
import { PRESET_VAR_KEYS, presetVars } from './preset-vars'

const ROOT = path.resolve(__dirname, '../../')
const FILES: Record<string, string> = {
  default: 'src/index.css',
  'tokyo-night': 'src/themes/tokyo-night.css',
  nord: 'src/themes/nord.css',
  catppuccin: 'src/themes/catppuccin.css',
  dracula: 'src/themes/dracula.css',
  'one-dark': 'src/themes/one-dark.css',
  solarized: 'src/themes/solarized.css',
  'rose-pine': 'src/themes/rose-pine.css',
}

function parseCssBlock(css: string, selector: string): Record<string, string> {
  const re = new RegExp(`${selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\{([^}]*)\\}`, 's')
  const match = css.match(re)
  if (!match) return {}
  const body = match[1]
  const map: Record<string, string> = {}
  for (const line of body.split(';')) {
    const trimmed = line.trim()
    if (!trimmed.startsWith('--')) continue
    const idx = trimmed.indexOf(':')
    if (idx < 0) continue
    const key = trimmed.slice(2, idx).trim()
    const val = trimmed.slice(idx + 1).trim()
    map[key] = val
  }
  return map
}

describe('preset-vars stays in sync with CSS files', () => {
  for (const [id, file] of Object.entries(FILES)) {
    it(`${id} light matches CSS`, () => {
      const css = fs.readFileSync(path.join(ROOT, file), 'utf-8')
      const lightSelector = id === 'default' ? ':root' : `[data-theme="${id}"]`
      const css_light = parseCssBlock(css, lightSelector)
      const ts_light = presetVars[id as keyof typeof presetVars].light
      for (const key of PRESET_VAR_KEYS) {
        expect(ts_light[key], `${id}.light.${key}`).toBe(css_light[key])
      }
    })

    it(`${id} dark matches CSS`, () => {
      const css = fs.readFileSync(path.join(ROOT, file), 'utf-8')
      const darkSelector = id === 'default' ? '.dark' : `[data-theme="${id}"].dark`
      const css_dark = parseCssBlock(css, darkSelector)
      const ts_dark = presetVars[id as keyof typeof presetVars].dark
      for (const key of PRESET_VAR_KEYS) {
        expect(ts_dark[key], `${id}.dark.${key}`).toBe(css_dark[key])
      }
    })
  }
})
```

- [ ] **Step 3: Run the test**

Run: `cd apps/web && bun run test preset-vars`
Expected: every test PASSES. If any fails, fix the TS map until it matches the CSS verbatim.

- [ ] **Step 4: Export `PresetThemeId` from index**

In `apps/web/src/themes/index.ts`, ensure `ColorTheme` (or rename to `PresetThemeId`) is exported. If renaming, also update existing imports in `theme-provider.tsx` and `appearance.tsx`.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/themes/preset-vars.ts apps/web/src/themes/preset-vars.test.ts apps/web/src/themes/index.ts
git commit -m "feat(web): add preset variable maps with CSS sync invariant test"
```

---

### Task 12: `theme-ref` URN parser + tests

**Files:**
- Create: `apps/web/src/lib/theme-ref.ts`
- Create: `apps/web/src/lib/theme-ref.test.ts`

- [ ] **Step 1: Write the parser**

```ts
// apps/web/src/lib/theme-ref.ts
import type { ColorTheme } from '@/themes'

export type ThemeRef =
  | { kind: 'preset'; id: ColorTheme }
  | { kind: 'custom'; id: number }

const PRESET_IDS: ColorTheme[] = [
  'default', 'tokyo-night', 'nord', 'catppuccin',
  'dracula', 'one-dark', 'solarized', 'rose-pine',
]

export function parseThemeRef(s: string): ThemeRef | null {
  if (s.startsWith('preset:')) {
    const id = s.slice('preset:'.length) as ColorTheme
    return PRESET_IDS.includes(id) ? { kind: 'preset', id } : null
  }
  if (s.startsWith('custom:')) {
    const n = Number(s.slice('custom:'.length))
    return Number.isInteger(n) && n > 0 ? { kind: 'custom', id: n } : null
  }
  return null
}

export function themeRefToString(r: ThemeRef): string {
  return r.kind === 'preset' ? `preset:${r.id}` : `custom:${r.id}`
}
```

- [ ] **Step 2: Write the test**

```ts
// apps/web/src/lib/theme-ref.test.ts
import { describe, expect, it } from 'vitest'
import { parseThemeRef, themeRefToString } from './theme-ref'

describe('theme-ref parser', () => {
  it('parses preset', () => {
    expect(parseThemeRef('preset:default')).toEqual({ kind: 'preset', id: 'default' })
  })
  it('parses custom', () => {
    expect(parseThemeRef('custom:42')).toEqual({ kind: 'custom', id: 42 })
  })
  it('rejects unknown preset', () => {
    expect(parseThemeRef('preset:nonsense')).toBeNull()
  })
  it('rejects bad custom id', () => {
    expect(parseThemeRef('custom:abc')).toBeNull()
    expect(parseThemeRef('custom:0')).toBeNull()
    expect(parseThemeRef('custom:-1')).toBeNull()
  })
  it('rejects unknown scheme', () => {
    expect(parseThemeRef('foo:bar')).toBeNull()
  })
  it('round trips', () => {
    const r = { kind: 'preset', id: 'nord' } as const
    expect(parseThemeRef(themeRefToString(r))).toEqual(r)
  })
})
```

- [ ] **Step 3: Run the test**

Run: `cd apps/web && bun run test theme-ref`
Expected: 6 tests PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/lib/theme-ref.ts apps/web/src/lib/theme-ref.test.ts
git commit -m "feat(web): add theme-ref URN parser"
```

---

### Task 13: OKLCH conversion utility (with `culori` dependency vetting)

**Files:**
- Modify: `apps/web/package.json`
- Create: `apps/web/src/lib/oklch.ts`
- Create: `apps/web/src/lib/oklch.test.ts`

- [ ] **Step 1: Vet the dependency**

Before adding `culori`:
- Check license at `https://www.npmjs.com/package/culori` (must be MIT/Apache/ISC).
- Check most recent release date (should be within last 12 months).
- Check bundle size impact via `https://bundlephobia.com/package/culori` (note tree-shakable subset usage).
- Confirm with the user: "Adding `culori` (color conversion). MIT, ~XX KB tree-shaken to the few functions we need. OK?"

If the user declines, skip Step 2; in `oklch.ts`, expose only an `oklchToString` formatter and let the editor input/edit OKLCH text directly without hex conversion (note the limitation in the editor UI in Task 23).

- [ ] **Step 2: Add dependency**

```bash
cd apps/web
bun add culori
```

This updates `apps/web/package.json` and lockfile.

- [ ] **Step 3: Write the conversion utility**

```ts
// apps/web/src/lib/oklch.ts
import { converter, formatHex, parse } from 'culori'

const toOklch = converter('oklch')

export interface OklchValue {
  l: number
  c: number
  h: number
  alpha?: number
  alphaIsPercent?: boolean
}

const OKLCH_RE = /^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)(?:\s*\/\s*([\d.]+)(%)?)?\s*\)$/

export function parseOklch(s: string): OklchValue | null {
  const m = OKLCH_RE.exec(s.trim())
  if (!m) return null
  const value: OklchValue = {
    l: Number(m[1]),
    c: Number(m[2]),
    h: Number(m[3]),
  }
  if (m[4] !== undefined) {
    value.alpha = Number(m[4])
    value.alphaIsPercent = m[5] === '%'
  }
  return value
}

export function formatOklch(v: OklchValue): string {
  const round = (n: number) => Number(n.toFixed(4))
  const head = `oklch(${round(v.l)} ${round(v.c)} ${round(v.h)}`
  if (v.alpha === undefined) return `${head})`
  const a = round(v.alpha) + (v.alphaIsPercent ? '%' : '')
  return `${head} / ${a})`
}

export function oklchToHex(s: string): string | null {
  const parsed = parse(s)
  if (!parsed) return null
  const hex = formatHex(parsed)
  return hex ?? null
}

export function hexToOklch(hex: string): string | null {
  const parsed = parse(hex)
  if (!parsed) return null
  const oklch = toOklch(parsed)
  if (!oklch) return null
  return formatOklch({ l: oklch.l ?? 0, c: oklch.c ?? 0, h: oklch.h ?? 0 })
}
```

- [ ] **Step 4: Write the test**

```ts
// apps/web/src/lib/oklch.test.ts
import { describe, expect, it } from 'vitest'
import { formatOklch, hexToOklch, oklchToHex, parseOklch } from './oklch'

describe('oklch utils', () => {
  it('parses oklch without alpha', () => {
    expect(parseOklch('oklch(0.5 0.1 180)')).toEqual({ l: 0.5, c: 0.1, h: 180 })
  })
  it('parses oklch with numeric alpha', () => {
    expect(parseOklch('oklch(0.5 0.1 180 / 0.5)')).toEqual({ l: 0.5, c: 0.1, h: 180, alpha: 0.5, alphaIsPercent: false })
  })
  it('parses oklch with percent alpha', () => {
    expect(parseOklch('oklch(1 0 0 / 10%)')).toEqual({ l: 1, c: 0, h: 0, alpha: 10, alphaIsPercent: true })
  })
  it('formats round-trip', () => {
    const v = parseOklch('oklch(0.5 0.1 180 / 50%)')!
    expect(formatOklch(v)).toBe('oklch(0.5 0.1 180 / 50%)')
  })
  it('rejects garbage', () => {
    expect(parseOklch('not-a-color')).toBeNull()
  })
  it('round-trips hex through oklch (within tolerance)', () => {
    const back = oklchToHex(hexToOklch('#ff0000')!)
    expect(back?.toLowerCase()).toMatch(/^#fe0000|^#ff0000/)
  })
})
```

- [ ] **Step 5: Run the test**

Run: `cd apps/web && bun run test oklch`
Expected: 6 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add apps/web/package.json apps/web/bun.lock apps/web/src/lib/oklch.ts apps/web/src/lib/oklch.test.ts
git commit -m "feat(web): add OKLCH/hex conversion via culori"
```

---

### Task 14: ThemeProvider — apply resolved payload + cache

**Files:**
- Modify: `apps/web/src/components/theme-provider.tsx`
- Create: `apps/web/src/api/themes.ts`

- [ ] **Step 1: Add API client hooks**

```ts
// apps/web/src/api/themes.ts
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'

export interface ThemeResolved {
  kind: 'preset' | 'custom'
  id: string | number
  name?: string
  vars_light?: Record<string, string>
  vars_dark?: Record<string, string>
  updated_at?: string
}

export interface ActiveThemeResponse {
  ref: string
  theme: ThemeResolved
}

export function useActiveTheme() {
  return useQuery<ActiveThemeResponse>({
    queryKey: ['active-theme'],
    queryFn: () => api.get<ActiveThemeResponse>('/api/settings/active-theme'),
    staleTime: 30_000,
  })
}

export function useSetActiveTheme() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (ref: string) =>
      api.put<ActiveThemeResponse>('/api/settings/active-theme', { ref }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['active-theme'] }),
  })
}

export interface ThemeSummary {
  id: number
  name: string
  based_on: string | null
  updated_at: string
}

export function useCustomThemes() {
  return useQuery<ThemeSummary[]>({
    queryKey: ['themes'],
    queryFn: () => api.get('/api/settings/themes'),
  })
}

// ... (use* hooks for get / create / update / delete / duplicate / import / export / references)
// Add the rest as needed in later tasks; keep the surface tight for now.
```

- [ ] **Step 2: Refactor ThemeProvider**

Open `apps/web/src/components/theme-provider.tsx`. Replace the `colorTheme` state with `activeTheme: ActiveThemeResponse | null`. Key changes:

- Replace `localStorage.getItem('color-theme')` reads with `localStorage.getItem('active-theme-cache')` parsed as JSON (full `ActiveThemeResponse`).
- After mount, fire `useActiveTheme()`; on data, set state and write the entire response to `localStorage.active-theme-cache`.
- Replace the existing `useEffect` that writes `data-theme` with a single applier:

```tsx
useEffect(() => {
  const root = document.documentElement
  const runtimeStyleId = 'theme-runtime-style'
  const existing = document.getElementById(runtimeStyleId)
  if (existing) existing.remove()

  if (!activeTheme) return

  if (activeTheme.theme.kind === 'preset') {
    root.removeAttribute('style')
    if (activeTheme.theme.id === 'default') {
      root.removeAttribute('data-theme')
    } else {
      loadThemeCSS(activeTheme.theme.id as ColorTheme).then(() => {
        root.setAttribute('data-theme', String(activeTheme.theme.id))
      })
    }
    return
  }

  // Custom theme: inject runtime <style> with both :root and .dark blocks
  root.removeAttribute('data-theme')
  const light = activeTheme.theme.vars_light ?? {}
  const dark = activeTheme.theme.vars_dark ?? {}
  const block = (sel: string, vars: Record<string, string>) =>
    `${sel} { ${Object.entries(vars).map(([k, v]) => `--${k}: ${v};`).join(' ')} }`
  const css = `${block(':root', light)}\n${block('.dark', dark)}`
  const style = document.createElement('style')
  style.id = runtimeStyleId
  style.textContent = css
  document.head.appendChild(style)
}, [activeTheme])
```

- Keep the existing light/dark `theme` state (`'dark' | 'light' | 'system'`) and storage logic (`localStorage.theme`) — only the colorTheme half is being rewritten.
- Expose context value:
```tsx
const value = useMemo(() => ({
  theme, setTheme,
  activeTheme,
  setActiveThemeRef: (ref: string) => setActiveMutation.mutate(ref),
}), [theme, setTheme, activeTheme, setActiveMutation])
```
- Keep the old `colorTheme` and `setColorTheme` exports as **thin shims** that map to/from the active theme ref, for the duration of this task only — Task 16 will rip them out cleanly. This keeps the build green while the call sites are migrated.

- [ ] **Step 3: Verify build + existing tests**

Run: `cd apps/web && bun run typecheck && bun run test`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/components/theme-provider.tsx apps/web/src/api/themes.ts
git commit -m "feat(web): rewrite ThemeProvider to apply resolved payload + cache"
```

---

### Task 15: localStorage migration prompt (admin-only, one-shot)

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/appearance.tsx` (top of file)

- [ ] **Step 1: Add the prompt**

```tsx
function LegacyMigrationPrompt() {
  const { activeTheme, setActiveThemeRef } = useTheme()
  const { data: me } = useMe()  // existing hook returning current user; if named differently, use that
  const [dismissed, setDismissed] = useState(() => localStorage.getItem('theme-migration-prompted') === '1')
  if (dismissed) return null

  const legacy = localStorage.getItem('color-theme')
  if (!legacy) return null
  if (me?.role !== 'admin') return null
  if (activeTheme?.ref && activeTheme.ref !== 'preset:default') return null
  if (!isColorTheme(legacy)) return null

  const dismiss = () => {
    localStorage.setItem('theme-migration-prompted', '1')
    localStorage.removeItem('color-theme')
    setDismissed(true)
  }
  const apply = () => {
    setActiveThemeRef(`preset:${legacy}`)
    dismiss()
  }

  return (
    <div className="rounded-lg border bg-muted p-4 mb-4 flex items-center justify-between">
      <span className="text-sm">
        {`Detected your previous color theme "${legacy}" from this browser. Apply it as the global theme?`}
      </span>
      <div className="flex gap-2">
        <Button size="sm" variant="outline" onClick={dismiss}>Ignore</Button>
        <Button size="sm" onClick={apply}>Apply</Button>
      </div>
    </div>
  )
}
```

Render it at the top of `AppearancePage`:

```tsx
function AppearancePage() {
  const { t } = useTranslation('settings')
  return (
    <div>
      <h1 className="mb-6 font-bold text-2xl">{t('appearance.title')}</h1>
      <LegacyMigrationPrompt />
      <div className="max-w-3xl space-y-6">
        <ThemeGrid />
        <BrandSettingsSection />
      </div>
    </div>
  )
}
```

- [ ] **Step 2: Build + smoke test**

Run: `cd apps/web && bun run typecheck`
Expected: PASS

Run: `cd apps/web && bun run dev`. In a browser, open DevTools → Application → Storage and set `localStorage.color-theme = 'tokyo-night'`. Reload `/settings/appearance` as an admin. The prompt appears with "Apply" / "Ignore". Click "Apply" — toast/success and the dashboard repaints to Tokyo Night. Reload — prompt is gone. Stop dev server.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/settings/appearance.tsx
git commit -m "feat(web): add one-shot legacy color-theme migration prompt for admins"
```

---

## Milestone 4 · Theme List Page

### Task 16: Appearance page — preset + custom grid

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/appearance.tsx`
- Create: `apps/web/src/components/theme/theme-card.tsx`

- [ ] **Step 1: Extract `ThemeCard` component**

```tsx
// apps/web/src/components/theme/theme-card.tsx
import { Check, Copy, Pencil, Trash2 } from 'lucide-react'
import { Button } from '@/components/ui/button'

interface Props {
  name: string
  preview: string[]
  active: boolean
  onActivate: () => void
  actions?: { onEdit?: () => void; onDuplicate?: () => void; onDelete?: () => void }
}

export function ThemeCard({ name, preview, active, onActivate, actions }: Props) {
  return (
    <div className="group relative">
      <button
        type="button"
        onClick={onActivate}
        className={`w-full rounded-lg border-2 p-3 text-left transition-all hover:shadow-md ${
          active ? 'border-primary shadow-sm' : 'border-border hover:border-primary/50'
        }`}
      >
        <div className="mb-2 flex gap-1.5">
          {preview.map((color) => (
            <div key={`${name}-${color}`} className="size-6 rounded-full border border-black/10" style={{ backgroundColor: color }} />
          ))}
        </div>
        <div className="flex items-center gap-1.5">
          <span className="font-medium text-sm">{name}</span>
          {active && <Check className="size-3.5 text-primary" />}
        </div>
      </button>
      {actions && (
        <div className="absolute top-2 right-2 hidden gap-1 group-hover:flex">
          {actions.onEdit && <Button size="icon" variant="ghost" onClick={actions.onEdit}><Pencil className="size-3.5" /></Button>}
          {actions.onDuplicate && <Button size="icon" variant="ghost" onClick={actions.onDuplicate}><Copy className="size-3.5" /></Button>}
          {actions.onDelete && <Button size="icon" variant="ghost" onClick={actions.onDelete}><Trash2 className="size-3.5" /></Button>}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Rewrite `ThemeGrid` for preset + custom**

In `appearance.tsx`, replace the current single grid with two:

```tsx
function ThemeGrid() {
  const { t } = useTranslation('settings')
  const navigate = useNavigate()
  const { activeTheme, setActiveThemeRef } = useTheme()
  const isDark = useIsDark()
  const { data: customs } = useCustomThemes()
  const duplicate = useDuplicateTheme()
  const [pendingDelete, setPendingDelete] = useState<{ id: number; name: string } | null>(null)

  const isActive = (ref: string) => activeTheme?.ref === ref

  return (
    <>
      <div className="rounded-lg border bg-card p-6">
        <h2 className="mb-1 font-semibold text-lg">{t('appearance.color_theme')}</h2>
        <p className="mb-4 text-muted-foreground text-sm">{t('appearance.color_theme_description')}</p>
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
          {themes.map((info) => (
            <ThemeCard
              key={info.id}
              name={info.name}
              preview={isDark ? info.previewColors.dark : info.previewColors.light}
              active={isActive(`preset:${info.id}`)}
              onActivate={() => setActiveThemeRef(`preset:${info.id}`)}
            />
          ))}
        </div>
      </div>

      <div className="rounded-lg border bg-card p-6">
        <div className="mb-4 flex items-center justify-between">
          <div>
            <h2 className="font-semibold text-lg">{t('appearance.custom_themes.title')}</h2>
            <p className="text-muted-foreground text-sm">{t('appearance.custom_themes.description')}</p>
          </div>
          <Button onClick={() => navigate({ to: '/settings/appearance/themes/new' })}>
            <Plus className="size-4" /> {t('appearance.custom_themes.new')}
          </Button>
        </div>
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
          {(customs ?? []).map((c) => (
            <ThemeCard
              key={c.id}
              name={c.name}
              preview={[]}  // Custom card preview: TODO replace with first 4 vars (Step 3)
              active={isActive(`custom:${c.id}`)}
              onActivate={() => setActiveThemeRef(`custom:${c.id}`)}
              actions={{
                onEdit: () => navigate({ to: `/settings/appearance/themes/$id`, params: { id: String(c.id) } }),
                onDuplicate: () => duplicate.mutate(c.id),
                onDelete: () => setPendingDelete({ id: c.id, name: c.name }),
              }}
            />
          ))}
        </div>
      </div>

      {pendingDelete && (
        <DeleteThemeDialog
          theme={pendingDelete}
          onClose={() => setPendingDelete(null)}
        />
      )}
    </>
  )
}
```

- [ ] **Step 3: Custom card preview swatches**

In the custom card branch, derive a 4-color preview from the theme's variables. Add a helper hook `useThemePreviewColors(id)` that fetches the full theme via `useQuery(['theme', id])` and returns `[primary, accent, background, foreground]` resolved to hex via `oklchToHex`. Replace `preview={[]}` with `preview={useThemePreviewColors(c.id, isDark)}`. Cache aggressively (`staleTime: 5 min`) since these change rarely.

> If the cards-on-list-page-each-loading-the-full-theme cost is concerning, deferred: add a `preview_colors` field to `ThemeSummary` server-side (computed at write time) in a follow-up.

- [ ] **Step 4: Verify**

Run: `cd apps/web && bun run typecheck && bun run dev`. Visit `/settings/appearance`:
- Preset grid renders all 8 cards, current one highlighted
- Custom grid (initially empty)
- "New" button navigates to `/settings/appearance/themes/new` (will 404 until Task 19; OK for now)

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed/settings/appearance.tsx apps/web/src/components/theme/theme-card.tsx
git commit -m "feat(web): split appearance page into preset and custom theme grids"
```

---

### Task 17: Delete-with-references confirmation dialog

**Files:**
- Create: `apps/web/src/components/theme/delete-theme-dialog.tsx`
- Modify: `apps/web/src/api/themes.ts`

- [ ] **Step 1: Add API hooks**

In `apps/web/src/api/themes.ts`, append:

```ts
export interface ThemeReferences {
  admin: boolean
  status_pages: { id: string; name: string }[]
}

export function useThemeReferences(id: number) {
  return useQuery<ThemeReferences>({
    queryKey: ['themes', id, 'references'],
    queryFn: () => api.get(`/api/settings/themes/${id}/references`),
  })
}

export function useDeleteTheme() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api.delete(`/api/settings/themes/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['themes'] }),
  })
}

export function useDuplicateTheme() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: number) => api.post(`/api/settings/themes/${id}/duplicate`, {}),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['themes'] }),
  })
}
```

- [ ] **Step 2: Write the dialog**

```tsx
// apps/web/src/components/theme/delete-theme-dialog.tsx
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useDeleteTheme, useThemeReferences } from '@/api/themes'

interface Props {
  theme: { id: number; name: string }
  onClose: () => void
}

export function DeleteThemeDialog({ theme, onClose }: Props) {
  const { t } = useTranslation('settings')
  const { data: refs, isLoading } = useThemeReferences(theme.id)
  const del = useDeleteTheme()

  const blocked = refs && (refs.admin || refs.status_pages.length > 0)

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('appearance.custom_themes.delete_title', { name: theme.name })}</DialogTitle>
        </DialogHeader>
        {isLoading && <p className="text-sm text-muted-foreground">…</p>}
        {refs && !blocked && (
          <p className="text-sm">{t('appearance.custom_themes.delete_confirm')}</p>
        )}
        {blocked && (
          <div className="space-y-2 text-sm">
            <p>{t('appearance.custom_themes.delete_blocked')}</p>
            <ul className="list-disc pl-6">
              {refs!.admin && <li>{t('appearance.custom_themes.delete_used_admin')}</li>}
              {refs!.status_pages.map((p) => (
                <li key={p.id}>{t('appearance.custom_themes.delete_used_status_page', { name: p.name })}</li>
              ))}
            </ul>
          </div>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>{t('common:cancel')}</Button>
          {!blocked && (
            <Button
              variant="destructive"
              disabled={!refs || del.isPending}
              onClick={() => del.mutate(theme.id, {
                onSuccess: () => { toast.success(t('appearance.custom_themes.deleted')); onClose() },
                onError: (e) => toast.error(e instanceof Error ? e.message : ''),
              })}
            >
              {t('common:delete')}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
```

- [ ] **Step 3: Verify**

Run: `cd apps/web && bun run typecheck`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/components/theme/delete-theme-dialog.tsx apps/web/src/api/themes.ts
git commit -m "feat(web): add delete-theme dialog with reference precheck"
```

---

## Milestone 5 · Editor

### Task 18: Editor route shell + create-from-preset flow

**Files:**
- Create: `apps/web/src/routes/_authed/settings/appearance/themes.new.tsx`
- Create: `apps/web/src/routes/_authed/settings/appearance/themes.$id.tsx`

- [ ] **Step 1: Create-new route**

```tsx
// apps/web/src/routes/_authed/settings/appearance/themes.new.tsx
import { createFileRoute, useNavigate } from '@tanstack/react-router'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { themes } from '@/themes'
import { presetVars } from '@/themes/preset-vars'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Input } from '@/components/ui/input'
import { useCreateTheme } from '@/api/themes'

export const Route = createFileRoute('/_authed/settings/appearance/themes/new')({
  component: NewThemePage,
})

function NewThemePage() {
  const { t } = useTranslation('settings')
  const navigate = useNavigate()
  const [name, setName] = useState('')
  const [forkFrom, setForkFrom] = useState<string>('default')
  const create = useCreateTheme()

  const submit = () => {
    const src = presetVars[forkFrom as keyof typeof presetVars]
    create.mutate(
      {
        name: name || t('appearance.editor.untitled'),
        based_on: forkFrom,
        vars_light: src.light,
        vars_dark: src.dark,
      },
      {
        onSuccess: (created) =>
          navigate({ to: '/settings/appearance/themes/$id', params: { id: String(created.id) } }),
      },
    )
  }

  return (
    <div className="max-w-md space-y-4 p-6">
      <h1 className="font-bold text-2xl">{t('appearance.editor.new_title')}</h1>
      <Input placeholder={t('appearance.editor.name_placeholder')} value={name} onChange={(e) => setName(e.target.value)} />
      <Select value={forkFrom} onValueChange={(v) => v && setForkFrom(v)}>
        <SelectTrigger><SelectValue placeholder={t('appearance.editor.fork_from')} /></SelectTrigger>
        <SelectContent>
          {themes.map((th) => <SelectItem key={th.id} value={th.id}>{th.name}</SelectItem>)}
        </SelectContent>
      </Select>
      <div className="flex gap-2">
        <Button variant="outline" onClick={() => navigate({ to: '/settings/appearance' })}>{t('common:cancel')}</Button>
        <Button onClick={submit} disabled={create.isPending}>{t('common:create')}</Button>
      </div>
    </div>
  )
}
```

Add the matching `useCreateTheme` hook to `apps/web/src/api/themes.ts`:

```ts
export function useCreateTheme() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (input: { name: string; description?: string; based_on?: string; vars_light: Record<string, string>; vars_dark: Record<string, string> }) =>
      api.post<{ id: number; name: string }>('/api/settings/themes', input),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['themes'] }),
  })
}
```

> Note: the matching `/themes/{id}` edit route is created together with the editor component in Task 20, so we don't import a not-yet-existing component here.

- [ ] **Step 2: Verify navigation**

Run: `cd apps/web && bun run typecheck`. Run dev, click "New" on the appearance page, fill name, pick a fork, submit. The mutation succeeds (server creates the theme); the navigate call to `/settings/appearance/themes/$id` will land on a 404 until Task 20 creates that route — that is expected at this milestone. As a quick check, the new theme already shows up if you navigate back to `/settings/appearance` manually.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/settings/appearance/themes.new.tsx apps/web/src/api/themes.ts
git commit -m "feat(web): add new theme route"
```

---

### Task 19: OKLCH picker component

**Files:**
- Create: `apps/web/src/components/theme/oklch-picker.tsx`

- [ ] **Step 1: Implement the picker**

```tsx
// apps/web/src/components/theme/oklch-picker.tsx
import { useEffect, useState } from 'react'
import { Input } from '@/components/ui/input'
import { hexToOklch, oklchToHex, parseOklch, formatOklch } from '@/lib/oklch'

interface Props {
  value: string                              // current oklch string
  onChange: (next: string) => void
  showHex?: boolean
}

export function OklchPicker({ value, onChange, showHex = true }: Props) {
  const parsed = parseOklch(value) ?? { l: 0.5, c: 0.1, h: 0 }
  const [hex, setHex] = useState(() => oklchToHex(value) ?? '')

  useEffect(() => {
    const h = oklchToHex(value)
    if (h) setHex(h)
  }, [value])

  const update = (next: { l?: number; c?: number; h?: number }) => {
    const merged = { ...parsed, ...next }
    onChange(formatOklch(merged))
  }

  const onHexChange = (h: string) => {
    setHex(h)
    if (/^#[0-9a-fA-F]{6}$/.test(h)) {
      const oklch = hexToOklch(h)
      if (oklch) onChange(oklch)
    }
  }

  return (
    <div className="flex items-center gap-2">
      <div className="size-6 rounded border" style={{ background: value }} />
      <div className="grid grid-cols-3 gap-1 flex-1">
        <Slider label="L" value={parsed.l} min={0} max={1} step={0.01} onChange={(l) => update({ l })} />
        <Slider label="C" value={parsed.c} min={0} max={0.5} step={0.005} onChange={(c) => update({ c })} />
        <Slider label="H" value={parsed.h} min={0} max={360} step={1} onChange={(h) => update({ h })} />
      </div>
      {showHex && (
        <Input className="w-24" value={hex} onChange={(e) => onHexChange(e.target.value)} placeholder="#rrggbb" />
      )}
    </div>
  )
}

function Slider({ label, value, min, max, step, onChange }: { label: string; value: number; min: number; max: number; step: number; onChange: (n: number) => void }) {
  return (
    <label className="flex items-center gap-1 text-xs">
      <span className="w-3 text-muted-foreground">{label}</span>
      <input type="range" min={min} max={max} step={step} value={value} onChange={(e) => onChange(Number(e.target.value))} className="flex-1" />
      <span className="w-10 text-right tabular-nums">{value.toFixed(label === 'H' ? 0 : 2)}</span>
    </label>
  )
}
```

> If the project disallowed adding `culori` (Task 13 fallback), remove the `showHex` branch entirely; user types OKLCH directly via a single text input. Note this in the editor UI.

- [ ] **Step 2: Verify**

Run: `cd apps/web && bun run typecheck`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/theme/oklch-picker.tsx
git commit -m "feat(web): add OKLCH 3-axis picker with hex sync"
```

---

### Task 20: Theme editor — variable groups + preview + save

**Files:**
- Create: `apps/web/src/routes/_authed/settings/appearance/themes.$id.tsx`
- Create: `apps/web/src/components/theme/theme-editor.tsx`
- Create: `apps/web/src/components/theme/theme-preview.tsx`
- Modify: `apps/web/src/api/themes.ts`

Also: at the end of this task, create the edit-route file:

```tsx
// apps/web/src/routes/_authed/settings/appearance/themes.$id.tsx
import { createFileRoute } from '@tanstack/react-router'
import { ThemeEditor } from '@/components/theme/theme-editor'

export const Route = createFileRoute('/_authed/settings/appearance/themes/$id')({
  component: EditThemePage,
})

function EditThemePage() {
  const { id } = Route.useParams()
  return <ThemeEditor themeId={Number(id)} />
}
```

Add this to the commit at Step 5.

- [ ] **Step 1: Add `useTheme(id)` and `useUpdateTheme` hooks**

```ts
// apps/web/src/api/themes.ts (append)
export interface FullTheme {
  id: number
  name: string
  description: string | null
  based_on: string | null
  vars_light: Record<string, string>
  vars_dark: Record<string, string>
  created_at: string
  updated_at: string
}

export function useThemeQuery(id: number) {
  return useQuery<FullTheme>({
    queryKey: ['themes', id],
    queryFn: () => api.get(`/api/settings/themes/${id}`),
  })
}

export function useUpdateTheme() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, body }: { id: number; body: { name: string; description?: string; based_on?: string; vars_light: Record<string, string>; vars_dark: Record<string, string> } }) =>
      api.put(`/api/settings/themes/${id}`, body),
    onSuccess: (_, vars) => {
      qc.invalidateQueries({ queryKey: ['themes'] })
      qc.invalidateQueries({ queryKey: ['themes', vars.id] })
      qc.invalidateQueries({ queryKey: ['active-theme'] })
    },
  })
}
```

- [ ] **Step 2: Implement the editor**

```tsx
// apps/web/src/components/theme/theme-editor.tsx
import { useEffect, useState } from 'react'
import { useNavigate } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { OklchPicker } from './oklch-picker'
import { ThemePreview } from './theme-preview'
import { useThemeQuery, useUpdateTheme } from '@/api/themes'
import { presetVars } from '@/themes/preset-vars'

const VAR_GROUPS: { id: string; vars: string[] }[] = [
  { id: 'surface', vars: ['background', 'foreground', 'card', 'card-foreground', 'popover', 'popover-foreground'] },
  { id: 'primary', vars: ['primary', 'primary-foreground', 'secondary', 'secondary-foreground'] },
  { id: 'state', vars: ['muted', 'muted-foreground', 'accent', 'accent-foreground', 'destructive'] },
  { id: 'border', vars: ['border', 'input', 'ring'] },
  { id: 'chart', vars: ['chart-1', 'chart-2', 'chart-3', 'chart-4', 'chart-5'] },
  { id: 'sidebar', vars: ['sidebar', 'sidebar-foreground', 'sidebar-primary', 'sidebar-primary-foreground', 'sidebar-accent', 'sidebar-accent-foreground', 'sidebar-border', 'sidebar-ring'] },
]

interface Props { themeId: number }

export function ThemeEditor({ themeId }: Props) {
  const { t } = useTranslation('settings')
  const navigate = useNavigate()
  const { data, isLoading } = useThemeQuery(themeId)
  const update = useUpdateTheme()
  const [name, setName] = useState('')
  const [light, setLight] = useState<Record<string, string>>({})
  const [dark, setDark] = useState<Record<string, string>>({})
  const [editingMode, setEditingMode] = useState<'light' | 'dark'>('light')
  const [previewMode, setPreviewMode] = useState<'light' | 'dark' | null>(null)
  const [dirty, setDirty] = useState(false)

  useEffect(() => {
    if (!data || dirty) return
    setName(data.name)
    setLight(data.vars_light)
    setDark(data.vars_dark)
  }, [data, dirty])

  const forkSrc = data?.based_on && data.based_on in presetVars ? presetVars[data.based_on as keyof typeof presetVars] : null
  const currentMap = editingMode === 'light' ? light : dark
  const setCurrentMap = editingMode === 'light' ? setLight : setDark
  const previewLight = light
  const previewDark = dark
  const previewWhich = previewMode ?? editingMode

  const onVarChange = (key: string, val: string) => {
    setCurrentMap({ ...currentMap, [key]: val })
    setDirty(true)
  }
  const reset = (key: string) => {
    if (!forkSrc) return
    const src = editingMode === 'light' ? forkSrc.light : forkSrc.dark
    setCurrentMap({ ...currentMap, [key]: src[key] })
    setDirty(true)
  }

  const save = () => {
    update.mutate(
      { id: themeId, body: { name, based_on: data?.based_on ?? undefined, vars_light: light, vars_dark: dark } },
      {
        onSuccess: () => { toast.success(t('appearance.editor.saved')); setDirty(false); navigate({ to: '/settings/appearance' }) },
        onError: (e) => toast.error(e instanceof Error ? e.message : ''),
      },
    )
  }

  useEffect(() => {
    if (!dirty) return
    const handler = (e: BeforeUnloadEvent) => { e.preventDefault(); e.returnValue = '' }
    window.addEventListener('beforeunload', handler)
    return () => window.removeEventListener('beforeunload', handler)
  }, [dirty])

  if (isLoading || !data) return <div className="p-6 text-sm text-muted-foreground">…</div>

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b p-4">
        <Input className="max-w-xs" value={name} onChange={(e) => { setName(e.target.value); setDirty(true) }} />
        {data.based_on && <span className="text-sm text-muted-foreground">{t('appearance.editor.based_on', { name: data.based_on })}</span>}
        <div className="ml-auto flex gap-2">
          <Button variant="outline" onClick={() => navigate({ to: '/settings/appearance' })}>{t('common:cancel')}</Button>
          <Button onClick={save} disabled={!dirty || update.isPending}>{t('common:save')}</Button>
        </div>
      </div>

      <div className="grid flex-1 grid-cols-1 lg:grid-cols-2 gap-0 overflow-hidden">
        <div className="border-r overflow-y-auto">
          <Tabs value={editingMode} onValueChange={(v) => setEditingMode(v as 'light' | 'dark')}>
            <TabsList className="m-3">
              <TabsTrigger value="light">{t('appearance.editor.light')}</TabsTrigger>
              <TabsTrigger value="dark">{t('appearance.editor.dark')}</TabsTrigger>
            </TabsList>
            <TabsContent value={editingMode} className="px-3 pb-3">
              <Accordion type="multiple" defaultValue={VAR_GROUPS.map((g) => g.id)}>
                {VAR_GROUPS.map((group) => (
                  <AccordionItem key={group.id} value={group.id}>
                    <AccordionTrigger>{t(`appearance.editor.groups.${group.id}`)}</AccordionTrigger>
                    <AccordionContent>
                      <div className="space-y-2">
                        {group.vars.map((key) => (
                          <div key={key} className="flex items-center gap-2">
                            <span className="w-32 text-xs">{key}</span>
                            <OklchPicker value={currentMap[key] ?? ''} onChange={(v) => onVarChange(key, v)} />
                            {forkSrc && (
                              <Button variant="ghost" size="icon" onClick={() => reset(key)} title={t('appearance.editor.reset')}>
                                ↺
                              </Button>
                            )}
                          </div>
                        ))}
                      </div>
                    </AccordionContent>
                  </AccordionItem>
                ))}
              </Accordion>
            </TabsContent>
          </Tabs>
        </div>

        <div className="flex flex-col">
          <ThemePreview vars={previewWhich === 'light' ? previewLight : previewDark} dark={previewWhich === 'dark'} />
          <div className="border-t p-3 flex gap-2 items-center text-xs">
            <span>{t('appearance.editor.preview_mode')}</span>
            <Button size="sm" variant={previewMode === null ? 'default' : 'outline'} onClick={() => setPreviewMode(null)}>{t('appearance.editor.linked')}</Button>
            <Button size="sm" variant={previewMode === 'light' ? 'default' : 'outline'} onClick={() => setPreviewMode('light')}>{t('appearance.editor.light')}</Button>
            <Button size="sm" variant={previewMode === 'dark' ? 'default' : 'outline'} onClick={() => setPreviewMode('dark')}>{t('appearance.editor.dark')}</Button>
          </div>
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Implement the preview pane**

```tsx
// apps/web/src/components/theme/theme-preview.tsx
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Badge } from '@/components/ui/badge'

interface Props { vars: Record<string, string>; dark: boolean }

export function ThemePreview({ vars, dark }: Props) {
  const style: Record<string, string> = {}
  for (const [k, v] of Object.entries(vars)) style[`--${k}`] = v

  return (
    <div
      className={`flex-1 overflow-auto p-6 ${dark ? 'dark' : ''}`}
      style={{ ...style, background: 'var(--background)', color: 'var(--foreground)' }}
      data-theme-preview
    >
      <div className="rounded-lg border p-4" style={{ background: 'var(--card)', borderColor: 'var(--border)' }}>
        <h3 className="font-semibold mb-2">Sample Card</h3>
        <p className="text-sm mb-3" style={{ color: 'var(--muted-foreground)' }}>
          Preview of typography, buttons, and inputs.
        </p>
        <div className="flex gap-2 mb-3">
          <Button>Primary</Button>
          <Button variant="secondary">Secondary</Button>
          <Button variant="destructive">Destructive</Button>
        </div>
        <Input placeholder="Input field" className="mb-3" />
        <div className="flex gap-2">
          <Badge>Default</Badge>
          <Badge variant="secondary">Secondary</Badge>
          <Badge variant="outline">Outline</Badge>
        </div>
      </div>

      <div className="mt-4 grid grid-cols-5 gap-2">
        {[1, 2, 3, 4, 5].map((i) => (
          <div key={i} className="h-16 rounded" style={{ background: `var(--chart-${i})` }} />
        ))}
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Run + smoke-test**

Run: `cd apps/web && bun run typecheck`. Then `bun run dev`. Visit `/settings/appearance/themes/<id>` for an existing theme:
- Editor loads, shows variables grouped, sliders work
- Right pane updates color in real time on slider drag
- Light/dark tab toggles editing target
- Save returns to list

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/theme/theme-editor.tsx apps/web/src/components/theme/theme-preview.tsx apps/web/src/routes/_authed/settings/appearance/themes.\$id.tsx apps/web/src/api/themes.ts
git commit -m "feat(web): add theme editor with grouped vars and live preview"
```

---

### Task 21: Import / Export

**Files:**
- Modify: `apps/web/src/components/theme/theme-editor.tsx` (add export button)
- Modify: `apps/web/src/routes/_authed/settings/appearance.tsx` (add import button)
- Modify: `apps/web/src/api/themes.ts`

- [ ] **Step 1: Add API hooks**

```ts
// apps/web/src/api/themes.ts (append)
export function useExportTheme(id: number) {
  return useQuery<unknown>({
    queryKey: ['themes', id, 'export'],
    queryFn: () => api.get(`/api/settings/themes/${id}/export`),
    enabled: false,
  })
}
export function useImportTheme() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (payload: unknown) => api.post('/api/settings/themes/import', payload),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['themes'] }),
  })
}
```

- [ ] **Step 2: Editor — add Export button**

In `theme-editor.tsx`, add an "Export" button to the toolbar that fetches `/api/settings/themes/{id}/export` and triggers a JSON file download:

```tsx
const onExport = async () => {
  const data = await api.get(`/api/settings/themes/${themeId}/export`)
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url; a.download = `${name}.theme.json`; a.click()
  URL.revokeObjectURL(url)
}
```

- [ ] **Step 3: List page — Import button + dialog**

In `appearance.tsx`, add an "Import" button next to "New":

```tsx
<input
  ref={fileInputRef}
  type="file"
  accept="application/json"
  className="hidden"
  onChange={async (e) => {
    const file = e.target.files?.[0]; if (!file) return
    const text = await file.text()
    try {
      const payload = JSON.parse(text)
      importMut.mutate(payload, {
        onSuccess: () => toast.success(t('appearance.custom_themes.imported')),
        onError: (err) => toast.error(err instanceof Error ? err.message : ''),
      })
    } catch {
      toast.error(t('appearance.custom_themes.import_invalid_json'))
    }
  }}
/>
<Button variant="outline" onClick={() => fileInputRef.current?.click()}>
  <Upload className="size-4" /> {t('appearance.custom_themes.import')}
</Button>
```

- [ ] **Step 4: Verify**

Run: `cd apps/web && bun run typecheck && bun run dev`. Smoke-test:
- Edit a theme, click Export → file downloads as JSON
- Click Import on list page, select a JSON → new theme appears

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/components/theme/theme-editor.tsx apps/web/src/routes/_authed/settings/appearance.tsx apps/web/src/api/themes.ts
git commit -m "feat(web): add theme import/export via JSON files"
```

---

## Milestone 6 · Status Page Binding

### Task 22: Status page edit form — theme selector

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/status-pages.tsx`

- [ ] **Step 1: Locate the edit form**

Open `apps/web/src/routes/_authed/settings/status-pages.tsx`. Identify where individual status pages are edited (likely a dialog or inline form with name, slug, etc.).

- [ ] **Step 2: Add a theme dropdown**

Add a `<Select>` field labeled "Theme" with options:
- "Follow admin default" (value: empty / null)
- One option per preset (value: `preset:<id>`)
- One option per custom theme (value: `custom:<id>`)

Wire it into the existing PUT request payload as `theme_ref: string | null`.

```tsx
const { data: customs } = useCustomThemes()
const themeOptions = [
  { value: '', label: t('status_page.theme.follow_admin') },
  ...themes.map((p) => ({ value: `preset:${p.id}`, label: `Preset · ${p.name}` })),
  ...(customs ?? []).map((c) => ({ value: `custom:${c.id}`, label: `Custom · ${c.name}` })),
]

// In form fields:
<Select value={form.theme_ref ?? ''} onValueChange={(v) => setForm({ ...form, theme_ref: v || null })}>
  <SelectTrigger><SelectValue /></SelectTrigger>
  <SelectContent>
    {themeOptions.map((o) => <SelectItem key={o.value} value={o.value}>{o.label}</SelectItem>)}
  </SelectContent>
</Select>

// Submit body:
{ ..., theme_ref: form.theme_ref }   // string or null
```

- [ ] **Step 3: Verify**

Run: `cd apps/web && bun run typecheck && bun run dev`. Edit a status page, set a theme, save, reopen — value persists.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/settings/status-pages.tsx
git commit -m "feat(web): add theme selector to status page edit form"
```

---

### Task 23: Public status page — scoped theme rendering

**Files:**
- Modify: `apps/web/src/routes/status.$slug.tsx`

- [ ] **Step 1: Apply scoped theme on root**

Open `apps/web/src/routes/status.$slug.tsx`. The page already fetches `PublicStatusPageData` containing the new `theme: ThemeResolved` field.

Add a wrapping div with `class="status-page-root"` and render the theme:

```tsx
function applyTheme(root: HTMLElement, theme: ThemeResolved) {
  // Reset
  root.removeAttribute('data-theme')
  const existing = root.querySelector('style[data-status-theme]')
  if (existing) existing.remove()

  if (theme.kind === 'preset') {
    if (theme.id !== 'default') {
      // Lazy load preset CSS first
      void loadThemeCSS(theme.id as ColorTheme).then(() => {
        root.setAttribute('data-theme', String(theme.id))
      })
    }
    return
  }
  // Custom theme: scoped <style>
  const block = (sel: string, vars: Record<string, string>) =>
    `${sel} { ${Object.entries(vars).map(([k, v]) => `--${k}: ${v};`).join(' ')} }`
  const style = document.createElement('style')
  style.dataset.statusTheme = '1'
  style.textContent = `${block('.status-page-root', theme.vars_light ?? {})}\n${block('.status-page-root.dark', theme.vars_dark ?? {})}`
  root.appendChild(style)
}

function StatusPage() {
  const ref = useRef<HTMLDivElement>(null)
  const { data } = ...  // existing fetch
  const isDark = useMatchMedia('(prefers-color-scheme: dark)')

  useEffect(() => {
    if (ref.current && data?.theme) applyTheme(ref.current, data.theme)
  }, [data?.theme])

  return (
    <div ref={ref} className={`status-page-root ${isDark ? 'dark' : ''}`}>
      {/* existing page content */}
    </div>
  )
}
```

> If the page already wraps its content in some root element, repurpose that element by adding the `status-page-root` class instead of nesting another wrapper.

- [ ] **Step 2: Verify**

Run: `cd apps/web && bun run typecheck && bun run dev`. Visit a public status page (`/status/<slug>`):
- Bind a custom theme via admin → reload public page → colors apply
- Bind a preset → public page uses that preset
- Unbind (`Follow admin default`) → public page falls back to active admin theme

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/status.\$slug.tsx
git commit -m "feat(web): apply scoped theme on public status pages"
```

---

## Milestone 7 · i18n + Docs + Acceptance

### Task 24: i18n strings (CN + EN)

**Files:**
- Modify: `apps/web/src/locales/zh/settings.json`
- Modify: `apps/web/src/locales/en/settings.json`
- Modify: `apps/web/src/locales/zh/status.json`
- Modify: `apps/web/src/locales/en/status.json`

- [ ] **Step 1: Add settings keys**

Append to `apps/web/src/locales/en/settings.json` (mirror in `zh/settings.json` with translations):

```json
{
  "appearance": {
    "custom_themes": {
      "title": "My themes",
      "description": "Custom themes you can fork, edit, import, and export.",
      "new": "New theme",
      "import": "Import",
      "imported": "Theme imported",
      "import_invalid_json": "Invalid JSON file",
      "delete_title": "Delete \"{{name}}\"?",
      "delete_confirm": "This action cannot be undone.",
      "delete_blocked": "This theme is currently in use:",
      "delete_used_admin": "Active for the admin dashboard",
      "delete_used_status_page": "Status page \"{{name}}\"",
      "deleted": "Theme deleted"
    },
    "editor": {
      "new_title": "New theme",
      "name_placeholder": "Theme name",
      "fork_from": "Fork from",
      "untitled": "Untitled theme",
      "based_on": "Based on {{name}}",
      "saved": "Theme saved",
      "light": "Light",
      "dark": "Dark",
      "linked": "Linked",
      "preview_mode": "Preview:",
      "reset": "Reset to fork value",
      "groups": {
        "surface": "Surfaces",
        "primary": "Primary & Secondary",
        "state": "States",
        "border": "Borders & Inputs",
        "chart": "Charts",
        "sidebar": "Sidebar"
      }
    }
  }
}
```

For `zh`, translate. Example: `"new": "新建主题"`, `"groups.surface": "表面色"`, etc.

- [ ] **Step 2: Add status page theme key**

In `en/status.json` and `zh/status.json`:

```json
{
  "theme": {
    "label": "Theme",
    "follow_admin": "Follow admin default"
  }
}
```

- [ ] **Step 3: Verify**

Run: `cd apps/web && bun run typecheck && bun run dev`. Switch language between EN and ZH; all editor / list / dialog labels render in the target language.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/locales/
git commit -m "feat(web): add i18n strings for custom themes (CN+EN)"
```

---

### Task 25: Fumadocs documentation

**Files:**
- Create: `apps/docs/content/docs/cn/custom-themes.mdx`
- Create: `apps/docs/content/docs/en/custom-themes.mdx`

- [ ] **Step 1: English doc**

```mdx
---
title: Custom themes
description: Build, share, and apply your own theme variables.
---

ServerBee ships with a curated set of preset themes. From this release, **administrators can also author full custom themes** — every variable, light and dark — and bind them to the admin dashboard or to individual status pages.

## Concepts

A custom theme is a JSON document of OKLCH variable values, with both light and dark variants.

- **Preset themes** are immutable; they are part of the application code.
- **Custom themes** live in the database. You can list, fork, edit, duplicate, import, export, and delete them.
- **Active admin theme** controls what the admin dashboard looks like for everyone.
- **Status page theme** is per-page: each public status page can pin its own theme, or follow the admin default.

## Creating a theme

1. Go to **Settings → Appearance**.
2. Click **New theme** in the "My themes" section.
3. Pick a name and choose a preset to fork from.
4. The editor opens with that preset's variables prefilled. Tweak L/C/H sliders or hex inputs; light and dark are edited as separate tabs.
5. Save.

## Activating a theme

- For the **admin dashboard**: click any preset or custom card on the appearance page; it becomes the active admin theme immediately.
- For a **status page**: open the status page edit form and pick a theme from the Theme dropdown. Choose "Follow admin default" to inherit the global active theme.

## Importing / exporting

- **Export**: in the editor, click "Export" to download the theme as a JSON file.
- **Import**: on the appearance page, click "Import" and choose a JSON file.

The JSON shape is `{ version, name, description?, based_on?, vars_light, vars_dark }`. Only `version: 1` is currently accepted.

## Validation rules

- All required CSS variables must be present (32 in total).
- Every value must be a valid `oklch(L C H)` or `oklch(L C H / α)` string.
- L is in `[0, 1]`, H in `[0, 360]`. Alpha is in `[0, 1]` or `[0%, 100%]`.
- Chroma has no hard cap — high-chroma values are legal but may render desaturated outside the display gamut.

## Disabling the feature

If you want to lock the deployment to preset themes only, set:

```toml
[feature]
custom_themes = false
```

Or via env: `SERVERBEE_FEATURE__CUSTOM_THEMES=false`.

When disabled:
- Custom-theme endpoints return 404.
- Any active reference to a custom theme degrades read-side to the default preset.
- Stored custom themes are preserved; flipping the flag back on restores them.
```

- [ ] **Step 2: Chinese doc**

Mirror the structure in `cn/custom-themes.mdx` with translated content.

- [ ] **Step 3: Register in sidebar**

Open `apps/docs/content/docs/{cn,en}/meta.json` (or equivalent index) and add `"custom-themes"` to the page list. Match the existing structure for other settings docs.

- [ ] **Step 4: Verify build**

Run: `cd apps/docs && bun run build`
Expected: PASS, both pages generated.

- [ ] **Step 5: Commit**

```bash
git add apps/docs/content/docs/
git commit -m "docs: add custom themes guide (CN+EN)"
```

---

### Task 26: E2E manual checklist

**Files:**
- Create: `tests/appearance/custom-theme.md`

- [ ] **Step 1: Write the checklist**

```markdown
# Custom theme — E2E manual checklist

## Setup
- Fresh `cargo run -p serverbee-server` against an empty DB.
- Two browsers (or browser + private window) for admin / member roles.

## Smoke
- [ ] Admin: `/settings/appearance` shows 8 preset cards + empty "My themes" + "New" / "Import" buttons.
- [ ] Click a preset → dashboard repaints, card highlighted.
- [ ] Member browser sees the same active theme on next reload.

## Create / edit
- [ ] New theme → name + fork from "Tokyo Night" → editor opens with prefilled vars.
- [ ] Drag the L slider on `--primary` → preview button color changes live.
- [ ] Switch to Dark tab → vars and preview reflect dark values.
- [ ] Save → returns to list, new card appears.
- [ ] Activate the new card → dashboard repaints to the new colors.

## Delete with references
- [ ] Active a custom theme → try to delete it → dialog shows "in use by admin".
- [ ] Switch admin back to a preset → delete the custom → succeeds.
- [ ] Bind the same theme to a status page → delete → blocked, lists the status page.

## Import / export
- [ ] Edit a theme → Export → JSON downloads, opens with all 32 vars in both maps.
- [ ] Tweak the JSON name in a text editor → Import → new theme card appears.

## Status page
- [ ] Bind a status page to a custom theme → public `/status/<slug>` repaints.
- [ ] Bind it to "Follow admin default" → public page matches admin theme.
- [ ] Bind to a preset → public page picks up that preset.

## Migration
- [ ] As Admin in a fresh browser, set `localStorage.color-theme = 'tokyo-night'` → reload → migration prompt appears.
- [ ] Click Apply → admin theme switches to Tokyo Night.
- [ ] Reload → prompt is gone.
- [ ] Repeat as Member → no prompt appears.

## Feature flag
- [ ] Restart server with `SERVERBEE_FEATURE__CUSTOM_THEMES=false`.
- [ ] All `/api/settings/themes*` return 404.
- [ ] Admin "My themes" section is hidden, only presets remain.
- [ ] If a custom theme was active before the flag flip, the dashboard now renders default preset.
- [ ] Re-enable flag → dashboard returns to the previous custom theme automatically.

## Mobile
- [ ] On a 375px viewport, the editor stacks (single column) and remains usable.
- [ ] Status page renders correctly on mobile with custom theme.
```

- [ ] **Step 2: Update `tests/README.md`** (if it lists feature checklists) — add a link to this file.

- [ ] **Step 3: Commit**

```bash
git add tests/appearance/custom-theme.md tests/README.md
git commit -m "test: add E2E manual checklist for custom themes"
```

---

## Final Validation

### Task 27: Full-suite green run + smoke

- [ ] **Step 1: Backend**

Run: `cargo test --workspace`
Expected: PASS (existing + new tests).

Run: `cargo clippy --workspace -- -D warnings`
Expected: 0 warnings.

- [ ] **Step 2: Frontend**

Run: `cd apps/web && bun run test && bun run typecheck && bun x ultracite check`
Expected: PASS / 0 issues.

- [ ] **Step 3: Build**

Run: `cd apps/web && bun run build`
Expected: PASS.

- [ ] **Step 4: Smoke**

Run: `cargo run -p serverbee-server` against a clean DB. Open `http://localhost:9527`:
- Login as admin → activate a custom theme → dashboard repaints
- Open `/swagger-ui/` — every new endpoint is listed with full schema
- Hit `/api/settings/active-theme` directly via curl with the admin cookie/API key — response body shape matches the spec

If any step fails, fix and re-run from Step 1.

- [ ] **Step 5: Final commit (if anything was tweaked)**

```bash
git status
# if changes:
git add -A
git commit -m "chore: final lint and validation tweaks for custom themes"
```

---

## Open Risks / Notes

- **`AuthUser` extractor**: Tasks 8 / 18 / 21 assume an auth extractor exposes `user.id`. If the project's middleware uses a different surface, adjust those tasks to use the actual mechanism.
- **`UpdateStatusPage` shape**: Task 9 assumes the existing service uses an `Update*` struct that maps from a router DTO. If the router writes the entity directly, refactor the bare minimum to thread the new field through cleanly — do not hide the change in the request handler.
- **`use*` hook names**: Frontend hooks (`useMe`, `useIsDark`, `useMatchMedia`) referenced by the tasks may have slightly different names in the current codebase. Use whichever existing hook covers the same intent; do not introduce duplicates.
- **`culori` rejection fallback** (Task 13): if the dependency review fails, the editor must drop hex inputs and require manual OKLCH text entry. Note this prominently in the editor UI and in the doc page (Task 25).
