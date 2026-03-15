# Network Probe Target Storage Refactor — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move built-in probe targets from database migrations to an embedded TOML config file, keeping the DB for user-created targets only.

**Architecture:** Preset targets are defined in `targets.toml`, embedded via `include_str!`, parsed at startup into a `LazyLock`-cached `Vec<FlatPresetTarget>`. The `list_targets` API merges presets (from memory) with custom targets (from DB), returning a unified `TargetDto` with a `source` field. All `is_builtin` logic throughout the stack is replaced with `PresetTargets::is_preset(id)` checks.

**Tech Stack:** Rust (sea-orm, serde, toml), React (TypeScript), SQLite

**Spec:** `docs/superpowers/specs/2026-03-16-network-probe-target-refactor-design.md`

---

## Chunk 1: Preset Module + Database Cleanup

### Task 1: Create the preset TOML definition file

**Files:**
- Create: `crates/server/src/presets/targets.toml`

- [ ] **Step 1: Create `targets.toml` with all 96 targets**

The file contains 4 preset groups: china-telecom (31), china-unicom (31), china-mobile (31), international (3).

Province codes and names (31 entries):
```
("bj", "Beijing"), ("tj", "Tianjin"), ("he", "Hebei"), ("sx", "Shanxi"), ("nm", "InnerMongolia"),
("ln", "Liaoning"), ("jl", "Jilin"), ("hl", "Heilongjiang"),
("sh", "Shanghai"), ("js", "Jiangsu"), ("zj", "Zhejiang"), ("ah", "Anhui"), ("fj", "Fujian"),
("jx", "Jiangxi"), ("sd", "Shandong"),
("ha", "Henan"), ("hb", "Hubei"), ("hn", "Hunan"), ("gd", "Guangdong"), ("gx", "Guangxi"), ("hi", "Hainan"),
("cq", "Chongqing"), ("sc", "Sichuan"), ("gz", "Guizhou"), ("yn", "Yunnan"), ("xz", "Tibet"),
("sn", "Shaanxi"), ("gs", "Gansu"), ("qh", "Qinghai"), ("nx", "Ningxia"), ("xj", "Xinjiang")
```

ISP codes: `ct`=Telecom, `cu`=Unicom, `cm`=Mobile

Target domain pattern: `{province}-{isp}-v4.ip.zstaticcdn.com:80` (TCP)

International targets: Cloudflare `1.1.1.1` (ICMP), Google DNS `8.8.8.8` (ICMP), AWS Tokyo `13.112.63.251` (ICMP)

ID pattern: `cn-{province}-{isp}` for China, `intl-{name}` for international.

Add a comment header about the ID freeze policy.

- [ ] **Step 2: Commit**

```bash
git add crates/server/src/presets/targets.toml
git commit -m "feat(server): add preset probe targets TOML definition (96 targets)"
```

---

### Task 2: Create the preset Rust module with tests

**Files:**
- Create: `crates/server/src/presets/mod.rs`
- Modify: `crates/server/src/lib.rs:1-10`
- Modify: `crates/server/Cargo.toml` (add `toml` dependency)

- [ ] **Step 0: Add `toml` crate dependency**

Add `toml = "0.8"` to `[dependencies]` in `crates/server/Cargo.toml`. The crate is already a transitive dependency of `figment` but is not re-exported.

- [ ] **Step 1: Write tests for preset loading**

Create `crates/server/src/presets/mod.rs` with a `#[cfg(test)]` module containing:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_returns_96_targets() {
        let targets = PresetTargets::load();
        assert_eq!(targets.len(), 96);
    }

    #[test]
    fn test_all_ids_unique() {
        let targets = PresetTargets::load();
        let mut ids: Vec<&str> = targets.iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 96);
    }

    #[test]
    fn test_find_existing_target() {
        let target = PresetTargets::find("cn-bj-ct");
        assert!(target.is_some());
        let t = target.unwrap();
        assert_eq!(t.name, "Beijing Telecom");
        assert_eq!(t.group_id, "china-telecom");
        assert_eq!(t.group_name, "中国电信");
        assert_eq!(t.probe_type, "tcp");
    }

    #[test]
    fn test_find_nonexistent_returns_none() {
        assert!(PresetTargets::find("nonexistent").is_none());
    }

    #[test]
    fn test_is_preset() {
        assert!(PresetTargets::is_preset("cn-bj-ct"));
        assert!(PresetTargets::is_preset("intl-cloudflare"));
        assert!(!PresetTargets::is_preset("some-uuid-id"));
    }

    #[test]
    fn test_group_metadata_propagated() {
        let targets = PresetTargets::load();
        let telecom: Vec<_> = targets.iter().filter(|t| t.group_id == "china-telecom").collect();
        assert_eq!(telecom.len(), 31);
        assert!(telecom.iter().all(|t| t.group_name == "中国电信"));
    }

    #[test]
    fn test_probe_types_valid() {
        let targets = PresetTargets::load();
        let valid_types = ["tcp", "icmp", "http"];
        assert!(targets.iter().all(|t| valid_types.contains(&t.probe_type.as_str())));
    }

    #[test]
    fn test_international_targets() {
        let intl: Vec<_> = PresetTargets::load().iter()
            .filter(|t| t.group_id == "international")
            .collect();
        assert_eq!(intl.len(), 3);
        assert!(intl.iter().all(|t| t.probe_type == "icmp"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p serverbee-server presets -- --nocapture
```

Expected: compilation error (structs and impl not defined yet).

- [ ] **Step 3: Implement the preset module**

In `crates/server/src/presets/mod.rs`, above the tests:

```rust
use std::sync::LazyLock;
use serde::Deserialize;

static TOML_CONTENT: &str = include_str!("targets.toml");

static PRESETS: LazyLock<Vec<FlatPresetTarget>> = LazyLock::new(|| {
    let file: PresetsFile = toml::from_str(TOML_CONTENT)
        .expect("Failed to parse presets/targets.toml");

    let mut targets = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    let valid_types = ["tcp", "icmp", "http"];

    for group in &file.presets {
        for target in &group.targets {
            assert!(!target.id.is_empty(), "Preset target ID must not be empty");
            assert!(!target.name.is_empty(), "Preset target name must not be empty");
            assert!(!target.target.is_empty(), "Preset target address must not be empty");
            assert!(
                valid_types.contains(&target.probe_type.as_str()),
                "Invalid probe_type '{}' for preset target '{}'",
                target.probe_type, target.id
            );
            assert!(
                seen_ids.insert(target.id.clone()),
                "Duplicate preset target ID: '{}'", target.id
            );

            targets.push(FlatPresetTarget {
                id: target.id.clone(),
                name: target.name.clone(),
                provider: target.provider.clone(),
                location: target.location.clone(),
                target: target.target.clone(),
                probe_type: target.probe_type.clone(),
                group_id: group.id.clone(),
                group_name: group.name.clone(),
            });
        }
    }

    targets
});

#[derive(Deserialize)]
struct PresetsFile {
    presets: Vec<PresetGroup>,
}

#[derive(Deserialize)]
struct PresetGroup {
    id: String,
    name: String,
    #[allow(dead_code)]
    description: String,
    targets: Vec<PresetTarget>,
}

#[derive(Deserialize)]
struct PresetTarget {
    id: String,
    name: String,
    provider: String,
    location: String,
    target: String,
    probe_type: String,
}

/// Flattened preset target with group metadata, used at runtime.
#[derive(Debug, Clone)]
pub struct FlatPresetTarget {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub location: String,
    pub target: String,
    pub probe_type: String,
    pub group_id: String,
    pub group_name: String,
}

pub struct PresetTargets;

impl PresetTargets {
    /// Return all preset targets. Cached via LazyLock.
    pub fn load() -> &'static [FlatPresetTarget] {
        &PRESETS
    }

    /// Find a single preset target by ID.
    pub fn find(id: &str) -> Option<&'static FlatPresetTarget> {
        PRESETS.iter().find(|t| t.id == id)
    }

    /// Check if an ID belongs to a preset target.
    pub fn is_preset(id: &str) -> bool {
        PRESETS.iter().any(|t| t.id == id)
    }
}
```

- [ ] **Step 4: Register the module in `lib.rs`**

Add `pub mod presets;` to `crates/server/src/lib.rs` (after `pub mod openapi;` line 6).

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p serverbee-server presets -- --nocapture
```

Expected: all 8 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/presets/ crates/server/src/lib.rs
git commit -m "feat(server): add PresetTargets module with TOML loading and validation"
```

---

### Task 3: Clean up database migrations

**Files:**
- Modify: `crates/server/src/migration/m20260315_000004_network_probe.rs:16-119` (rewrite)
- Delete: `crates/server/src/migration/m20260315_000005_update_builtin_targets.rs`
- Modify: `crates/server/src/migration/mod.rs:7,18` (remove m005)

- [ ] **Step 1: Rewrite m004 — remove `is_builtin`, seed INSERTs, FK constraints**

Replace the `up()` body in `m20260315_000004_network_probe.rs` with:

```rust
async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    let db = manager.get_connection();

    db.execute_unprepared(
        "CREATE TABLE network_probe_target (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            provider TEXT NOT NULL,
            location TEXT NOT NULL,
            target TEXT NOT NULL,
            probe_type TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )"
    ).await?;

    db.execute_unprepared(
        "CREATE TABLE network_probe_config (
            id TEXT PRIMARY KEY NOT NULL,
            server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
            target_id TEXT NOT NULL,
            created_at TEXT NOT NULL,
            UNIQUE(server_id, target_id)
        )"
    ).await?;

    db.execute_unprepared(
        "CREATE TABLE network_probe_record (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
            target_id TEXT NOT NULL,
            avg_latency REAL,
            min_latency REAL,
            max_latency REAL,
            packet_loss REAL NOT NULL,
            packet_sent INTEGER NOT NULL,
            packet_received INTEGER NOT NULL,
            timestamp TEXT NOT NULL
        )"
    ).await?;

    db.execute_unprepared(
        "CREATE INDEX idx_network_probe_record_lookup ON network_probe_record (server_id, target_id, timestamp)"
    ).await?;

    db.execute_unprepared(
        "CREATE TABLE network_probe_record_hourly (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
            target_id TEXT NOT NULL,
            avg_latency REAL,
            min_latency REAL,
            max_latency REAL,
            avg_packet_loss REAL NOT NULL,
            sample_count INTEGER NOT NULL,
            hour TEXT NOT NULL,
            UNIQUE(server_id, target_id, hour)
        )"
    ).await?;

    Ok(())
}
```

Key changes vs current:
- `network_probe_target`: removed `is_builtin INTEGER NOT NULL DEFAULT 0`
- `network_probe_config`: removed `REFERENCES network_probe_target(id) ON DELETE CASCADE` on `target_id`
- `network_probe_record`: removed `REFERENCES network_probe_target(id) ON DELETE CASCADE` on `target_id`
- `network_probe_record_hourly`: removed FK on `target_id`
- Removed all 96 seed INSERT statements (lines 74-119)

Keep the `down()` method as-is (it just drops tables).

- [ ] **Step 2: Delete m005 migration file**

Delete `crates/server/src/migration/m20260315_000005_update_builtin_targets.rs`.

- [ ] **Step 3: Remove m005 from migration mod**

In `crates/server/src/migration/mod.rs`:
- Remove line 7: `mod m20260315_000005_update_builtin_targets;`
- Remove line 18: `Box::new(m20260315_000005_update_builtin_targets::Migration),`

Result should be:

```rust
use sea_orm_migration::prelude::*;

mod m20260312_000001_init;
mod m20260312_000002_oauth;
mod m20260314_000003_add_capabilities;
mod m20260315_000004_network_probe;

pub struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260312_000001_init::Migration),
            Box::new(m20260312_000002_oauth::Migration),
            Box::new(m20260314_000003_add_capabilities::Migration),
            Box::new(m20260315_000004_network_probe::Migration),
        ]
    }
}
```

- [ ] **Step 4: Update entity — remove `is_builtin` field**

In `crates/server/src/entity/network_probe_target.rs`, remove line 15: `pub is_builtin: bool,`

The full entity becomes:

```rust
use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = NetworkProbeTarget)]
#[sea_orm(table_name = "network_probe_target")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub provider: String,
    pub location: String,
    pub target: String,
    pub probe_type: String,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 5: Delete any existing dev database so migrations re-run cleanly**

```bash
rm -f crates/server/serverbee.db*
```

- [ ] **Step 6: Verify build compiles**

```bash
cargo build -p serverbee-server 2>&1 | head -30
```

Expected: compilation errors in `service/network_probe.rs` referencing `is_builtin`. This is expected — we fix those in Task 4.

- [ ] **Step 7: Commit**

```bash
git add -A crates/server/src/migration/ crates/server/src/entity/network_probe_target.rs
git commit -m "refactor(server): remove is_builtin from migration, entity, and delete m005"
```

---

## Chunk 2: Service Layer Refactor

### Task 4: Add TargetDto and resolve_target helper

**Files:**
- Modify: `crates/server/src/service/network_probe.rs:41-113` (add TargetDto, add resolve helpers)

- [ ] **Step 1: Add `TargetDto` struct and conversion helpers**

In `crates/server/src/service/network_probe.rs`, add the following after the existing DTOs (after line ~113, before `impl NetworkProbeService`):

```rust
/// Unified target DTO merging preset and custom targets.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct TargetDto {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub location: String,
    pub target: String,
    pub probe_type: String,
    pub source: Option<String>,
    pub source_name: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

impl TargetDto {
    /// Create from a preset target.
    pub fn from_preset(t: &crate::presets::FlatPresetTarget) -> Self {
        Self {
            id: t.id.clone(),
            name: t.name.clone(),
            provider: t.provider.clone(),
            location: t.location.clone(),
            target: t.target.clone(),
            probe_type: t.probe_type.clone(),
            source: Some(format!("preset:{}", t.group_id)),
            source_name: Some(t.group_name.clone()),
            created_at: None,
            updated_at: None,
        }
    }

    /// Create from a DB model (user-created target).
    pub fn from_model(m: &network_probe_target::Model) -> Self {
        Self {
            id: m.id.clone(),
            name: m.name.clone(),
            provider: m.provider.clone(),
            location: m.location.clone(),
            target: m.target.clone(),
            probe_type: m.probe_type.clone(),
            source: None,
            source_name: None,
            created_at: Some(m.created_at.to_rfc3339()),
            updated_at: Some(m.updated_at.to_rfc3339()),
        }
    }
}
```

- [ ] **Step 2: Add `resolve_target` shared helper inside `impl NetworkProbeService`**

Add this method early in the `impl NetworkProbeService` block (after the Target management comment):

```rust
/// Resolve a target ID to TargetDto. Checks presets first, then DB.
async fn resolve_target(db: &DatabaseConnection, id: &str) -> Option<TargetDto> {
    if let Some(preset) = crate::presets::PresetTargets::find(id) {
        return Some(TargetDto::from_preset(preset));
    }
    network_probe_target::Entity::find_by_id(id)
        .one(db)
        .await
        .ok()
        .flatten()
        .map(|m| TargetDto::from_model(&m))
}

/// Check if a target ID is valid (exists as preset or in DB).
async fn is_valid_target(db: &DatabaseConnection, id: &str) -> bool {
    crate::presets::PresetTargets::is_preset(id)
        || network_probe_target::Entity::find_by_id(id)
            .one(db)
            .await
            .ok()
            .flatten()
            .is_some()
}

/// Build a target name+provider lookup map from presets + DB.
async fn build_target_map(db: &DatabaseConnection) -> HashMap<String, (String, String)> {
    let mut map: HashMap<String, (String, String)> = HashMap::new();
    for t in crate::presets::PresetTargets::load() {
        map.insert(t.id.clone(), (t.name.clone(), t.provider.clone()));
    }
    if let Ok(custom) = network_probe_target::Entity::find().all(db).await {
        for t in custom {
            map.insert(t.id.clone(), (t.name.clone(), t.provider.clone()));
        }
    }
    map
}
```

Add `use std::collections::HashMap;` to imports at the top of the file if not already present.

- [ ] **Step 3: Verify build**

```bash
cargo build -p serverbee-server 2>&1 | head -30
```

Expected: still has errors from `is_builtin` references in service methods. We fix those next.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/network_probe.rs
git commit -m "feat(server): add TargetDto, resolve_target, and build_target_map helpers"
```

---

### Task 5: Refactor service methods — remove is_builtin, use PresetTargets

**Files:**
- Modify: `crates/server/src/service/network_probe.rs` (multiple methods)

This task updates all service methods that reference `is_builtin` or return `network_probe_target::Model` where they should return `TargetDto`.

- [ ] **Step 1: Refactor `list_targets`** (line ~125)

Replace current implementation:

```rust
pub async fn list_targets(db: &DatabaseConnection) -> Result<Vec<TargetDto>, AppError> {
    // Presets first, grouped by preset group
    let mut targets: Vec<TargetDto> = crate::presets::PresetTargets::load()
        .iter()
        .map(TargetDto::from_preset)
        .collect();

    // Then custom targets from DB
    let custom = network_probe_target::Entity::find()
        .order_by_asc(network_probe_target::Column::Name)
        .all(db)
        .await?;
    targets.extend(custom.iter().map(TargetDto::from_model));

    Ok(targets)
}
```

- [ ] **Step 2: Refactor `create_target`** (line ~132)

Remove `is_builtin: Set(false),` from the ActiveModel. The field no longer exists on the entity.

- [ ] **Step 3: Refactor `update_target`** (line ~159)

Replace `is_builtin` DB check with preset check **before** DB lookup:

```rust
pub async fn update_target(
    db: &DatabaseConnection,
    id: &str,
    input: UpdateNetworkProbeTarget,
) -> Result<network_probe_target::Model, AppError> {
    if crate::presets::PresetTargets::is_preset(id) {
        return Err(AppError::Forbidden(
            "Cannot modify a preset probe target".to_string(),
        ));
    }

    let existing = network_probe_target::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Probe target {id} not found")))?;

    // ... rest unchanged (update fields, save)
```

- [ ] **Step 4: Refactor `delete_target`** (line ~205)

Replace `is_builtin` check with preset check before DB lookup. Keep the existing non-transactional cascade pattern (matching current behavior — `ConfigService` signatures use `&DatabaseConnection`, changing them to `&impl ConnectionTrait` would cascade to many callers):

```rust
pub async fn delete_target(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
    if crate::presets::PresetTargets::is_preset(id) {
        return Err(AppError::Forbidden(
            "Cannot delete a preset probe target".to_string(),
        ));
    }

    let _existing = network_probe_target::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Probe target {id} not found")))?;

    // Cascade delete config entries
    network_probe_config::Entity::delete_many()
        .filter(network_probe_config::Column::TargetId.eq(id))
        .exec(db)
        .await?;

    // Cascade delete raw records
    network_probe_record::Entity::delete_many()
        .filter(network_probe_record::Column::TargetId.eq(id))
        .exec(db)
        .await?;

    // Cascade delete hourly records
    network_probe_record_hourly::Entity::delete_many()
        .filter(network_probe_record_hourly::Column::TargetId.eq(id))
        .exec(db)
        .await?;

    // Remove from default_target_ids in setting
    let mut setting = Self::get_setting(db).await?;
    setting.default_target_ids.retain(|tid| tid != id);
    Self::save_setting(db, &setting).await?;

    // Delete the target itself
    network_probe_target::Entity::delete_by_id(id).exec(db).await?;

    Ok(())
}
```

- [ ] **Step 5: Refactor `update_setting`** (line ~260)

Add validation of `default_target_ids`:

```rust
pub async fn update_setting(
    db: &DatabaseConnection,
    setting: &NetworkProbeSetting,
) -> Result<(), AppError> {
    if !(30..=600).contains(&setting.interval) {
        return Err(AppError::BadRequest(
            "interval must be between 30 and 600 seconds".to_string(),
        ));
    }
    if !(5..=20).contains(&setting.packet_count) {
        return Err(AppError::BadRequest(
            "packet_count must be between 5 and 20".to_string(),
        ));
    }
    // Validate default_target_ids
    for id in &setting.default_target_ids {
        if !Self::is_valid_target(db, id).await {
            return Err(AppError::BadRequest(
                format!("Invalid target ID in default_target_ids: {id}"),
            ));
        }
    }
    Self::save_setting(db, setting).await
}
```

- [ ] **Step 6: Refactor `get_server_targets`** (line ~290)

Replace the entity JOIN with preset+DB resolution. Preserve the existing early-return optimization for servers with no targets:

```rust
pub async fn get_server_targets(
    db: &DatabaseConnection,
    server_id: &str,
) -> Result<Vec<TargetDto>, AppError> {
    let configs = network_probe_config::Entity::find()
        .filter(network_probe_config::Column::ServerId.eq(server_id))
        .all(db)
        .await?;

    if configs.is_empty() {
        return Ok(Vec::new());
    }

    let target_ids: Vec<String> = configs.iter().map(|c| c.target_id.clone()).collect();

    let mut targets = Vec::new();
    // Collect DB IDs to batch-query
    let mut db_ids = Vec::new();
    for id in &target_ids {
        if let Some(preset) = crate::presets::PresetTargets::find(id) {
            targets.push(TargetDto::from_preset(preset));
        } else {
            db_ids.push(id.clone());
        }
    }

    if !db_ids.is_empty() {
        let db_targets = network_probe_target::Entity::find()
            .filter(network_probe_target::Column::Id.is_in(db_ids))
            .all(db)
            .await?;
        targets.extend(db_targets.iter().map(TargetDto::from_model));
    }

    Ok(targets)
}
```

- [ ] **Step 7: Refactor `set_server_targets`** (line ~315)

Add validation of target IDs (use `AppError::Validation` to match existing max-20 check pattern):

```rust
pub async fn set_server_targets(
    db: &DatabaseConnection,
    server_id: &str,
    target_ids: Vec<String>,
) -> Result<(), AppError> {
    if target_ids.len() > 20 {
        return Err(AppError::Validation(
            "Cannot assign more than 20 targets to a server".to_string(),
        ));
    }

    // Validate all target IDs
    for id in &target_ids {
        if !Self::is_valid_target(db, id).await {
            return Err(AppError::Validation(
                format!("Invalid target ID: {id}"),
            ));
        }
    }

    // ... rest unchanged (delete existing configs, insert new ones)
```

- [ ] **Step 8: Refactor `get_overview`** (line ~534)

Replace the `target_map` construction to use `build_target_map`:

Find the line that builds `target_map` from DB (around line 550-552) and replace with:

```rust
let target_map = Self::build_target_map(db).await;
```

Then update usages — the map values are `(name, provider)` tuples, so update lookups like:
```rust
// Before:
let target_name = target_map.get(&target_id).map(|t| t.name.clone()).unwrap_or_else(|| target_id.clone());
// After:
let (target_name, provider) = target_map.get(&target_id)
    .cloned()
    .unwrap_or_else(|| (target_id.clone(), String::new()));
```

Adjust field access to match the existing code's pattern.

- [ ] **Step 9: Refactor `get_server_summary`** (line ~466)

Update to work with `Vec<TargetDto>` from `get_server_targets`:

The function calls `get_server_targets` (line ~480) and then uses `.id`, `.name`, `.provider` etc. Since `TargetDto` has the same field names, this should be a mechanical change — just update the type annotation if explicit.

- [ ] **Step 10: Refactor `get_anomalies`** (line ~633)

The function calls `get_server_targets` to build a name map. Since `get_server_targets` now returns `Vec<TargetDto>`, update the `target_map` construction:

```rust
let targets = Self::get_server_targets(db, server_id).await?;
let target_map: HashMap<String, String> = targets
    .into_iter()
    .map(|t| (t.id, t.name))
    .collect();
```

- [ ] **Step 11: Verify build compiles**

```bash
cargo build -p serverbee-server 2>&1 | head -30
```

Expected: may still have errors in router layer (next task). Fix any remaining service compile errors.

- [ ] **Step 12: Commit**

```bash
git add crates/server/src/service/network_probe.rs
git commit -m "refactor(server): remove is_builtin from service layer, use PresetTargets"
```

---

## Chunk 3: Router Layer + Tests

### Task 6: Update router handlers

**Files:**
- Modify: `crates/server/src/router/api/network_probe.rs` (response types)
- Modify: `crates/server/src/router/api/server.rs:573-581` (target mapping)
- Modify: `crates/server/src/router/ws/agent.rs:98-110` (target mapping)

- [ ] **Step 1: Update `list_targets` handler** (network_probe.rs line ~47)

Change return type from `Vec<network_probe_target::Model>` to `Vec<TargetDto>`:

```rust
pub async fn list_targets(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<crate::service::network_probe::TargetDto>>>, AppError> {
    let targets = NetworkProbeService::list_targets(&state.db).await?;
    ok(targets)
}
```

- [ ] **Step 2: Update `get_server_network_targets` handler** (network_probe.rs line ~259)

Change return type from `Vec<network_probe_target::Model>` to `Vec<TargetDto>`:

```rust
pub async fn get_server_network_targets(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<crate::service::network_probe::TargetDto>>>, AppError> {
    let targets = NetworkProbeService::get_server_targets(&state.db, &id).await?;
    ok(targets)
}
```

- [ ] **Step 3: Update `server.rs` target mapping** (line ~573)

In the handler that calls `get_server_targets` and maps to `serverbee_common::types::NetworkProbeTarget`, update to use `TargetDto` fields:

```rust
let targets: Vec<NetworkProbeTarget> = targets
    .iter()
    .map(|t| NetworkProbeTarget {
        target_id: t.id.clone(),
        name: t.name.clone(),
        target: t.target.clone(),
        probe_type: t.probe_type.clone(),
    })
    .collect();
```

- [ ] **Step 4: Update `agent.rs` target mapping** (line ~98)

Same change — map from `TargetDto` to `NetworkProbeTargetDto`:

```rust
let targets: Vec<NetworkProbeTargetDto> = targets
    .iter()
    .map(|t| NetworkProbeTargetDto {
        target_id: t.id.clone(),
        name: t.name.clone(),
        target: t.target.clone(),
        probe_type: t.probe_type.clone(),
    })
    .collect();
```

- [ ] **Step 5: Verify other handlers that call `get_server_targets`**

The `delete_target` handler (network_probe.rs lines 164-173) and `update_setting` handler (lines 207-216) also call `get_server_targets` and map results to `NetworkProbeTarget`. Since `TargetDto` has the same field names (`.id`, `.name`, `.target`, `.probe_type`) as the old `network_probe_target::Model`, these handlers compile without changes. Verify this — no code modification needed.

The `create_target` and `update_target` handlers still return `network_probe_target::Model` directly — this is fine since they only deal with user-created targets (DB-backed). Verify they compile without `is_builtin`.

- [ ] **Step 6: Verify full build compiles**

```bash
cargo build -p serverbee-server 2>&1 | head -30
```

Expected: clean build.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/router/
git commit -m "refactor(server): update router handlers to use TargetDto"
```

---

### Task 7: Update tests

**Files:**
- Modify: `crates/server/src/service/network_probe.rs` (unit tests, line ~870+)
- Modify: `crates/server/tests/integration.rs` (integration tests)

- [ ] **Step 1: Update unit test `test_create_and_list_targets`** (line ~902)

Remove `assert!(!created.is_builtin)` (line ~917). The `list_targets` now returns `Vec<TargetDto>` — update assertions to check `source` field instead:

```rust
let list = NetworkProbeService::list_targets(&db).await.unwrap();
// Presets (96) + 1 custom
assert_eq!(list.len(), before_count + 1);
// Custom target should have source = None
let custom = list.iter().find(|t| t.name == "Test Target").unwrap();
assert!(custom.source.is_none());
```

Note: `before_count` was previously the count of DB targets. Now `list_targets` returns presets + DB targets. The initial count will be 96 (presets only, DB is empty). After creating one, it should be 97. Adjust accordingly.

- [ ] **Step 2: Update other unit tests that reference `is_builtin`**

Scan all tests in the `#[cfg(test)]` block. The `test_update_target` and `test_delete_target` tests may reference `is_builtin` implicitly through the entity model — verify they still compile after removing the field.

- [ ] **Step 3: Update integration test `test_network_probe_target_crud`** (line ~659)

The test expects 96 builtin targets in the initial list. After refactor, `list_targets` returns 96 presets + 0 custom = 96 total. The count should be the same, but the response shape changes (`is_builtin` → `source`).

Update assertions:
- Remove any `is_builtin` field checks
- Check `source` field instead: preset targets have `source: "preset:..."`, custom targets have `source: null`

- [ ] **Step 4: Update integration test `test_builtin_target_cannot_be_deleted`** (line ~890)

This test finds a target with `is_builtin == true` and tries to delete it. Update to:
- Use a known preset ID directly (e.g., `"cn-bj-ct"`)
- Assert DELETE returns 403
- Remove any `is_builtin` field parsing

- [ ] **Step 5: Run all tests**

```bash
cargo test --workspace 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 6: Run clippy**

```bash
cargo clippy --workspace -- -D warnings 2>&1 | tail -20
```

Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/service/network_probe.rs crates/server/tests/integration.rs
git commit -m "test(server): update tests for preset target refactor"
```

---

## Chunk 4: Frontend Changes

### Task 8: Update frontend types and settings page

**Files:**
- Modify: `apps/web/src/lib/network-types.ts:1-11`
- Modify: `apps/web/src/routes/_authed/settings/network-probes.tsx:223-242`

- [ ] **Step 1: Update `NetworkProbeTarget` type**

In `apps/web/src/lib/network-types.ts`, replace:

```typescript
export interface NetworkProbeTarget {
  created_at: string
  id: string
  is_builtin: boolean
  location: string
  name: string
  probe_type: string
  provider: string
  target: string
  updated_at: string
}
```

With:

```typescript
export interface NetworkProbeTarget {
  created_at: string | null
  id: string
  location: string
  name: string
  probe_type: string
  provider: string
  source: string | null
  source_name: string | null
  target: string
  updated_at: string | null
}
```

- [ ] **Step 2: Update settings page — source column**

In `apps/web/src/routes/_authed/settings/network-probes.tsx`, find the `is_builtin` column definition (line ~223) and replace with:

```typescript
{
  accessorKey: 'source',
  header: 'Status',
  enableSorting: false,
  cell: ({ row }) =>
    row.original.source ? (
      <span className="flex items-center gap-1 text-muted-foreground text-xs">
        <Lock className="size-3" />
        {row.original.source_name ?? t('preset')}
      </span>
    ) : (
      <span className="text-muted-foreground text-xs">{t('custom')}</span>
    )
},
```

- [ ] **Step 3: Update settings page — actions column guard**

Find the actions column guard (line ~242) and replace `!row.original.is_builtin` with `!row.original.source`:

```typescript
cell: ({ row }) =>
  !row.original.source && (
    // ... edit/delete buttons unchanged
  )
```

- [ ] **Step 4: Check for any other `is_builtin` references in frontend**

```bash
grep -r "is_builtin" apps/web/src/
```

Expected: no matches. If any remain, update them.

- [ ] **Step 5: Run TypeScript check**

```bash
cd apps/web && bun run typecheck
```

Expected: no errors.

- [ ] **Step 6: Run lint**

```bash
cd apps/web && bun x ultracite check
```

Expected: no errors.

- [ ] **Step 7: Run frontend tests**

```bash
cd apps/web && bun run test
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add apps/web/src/lib/network-types.ts apps/web/src/routes/_authed/settings/network-probes.tsx
git commit -m "refactor(web): replace is_builtin with source field for preset targets"
```

---

## Chunk 5: Final Verification

### Task 9: End-to-end verification

- [ ] **Step 1: Full workspace build**

```bash
cargo build --workspace
```

Expected: clean build, no errors.

- [ ] **Step 2: Full Rust test suite**

```bash
cargo test --workspace
```

Expected: all tests pass (unit + integration).

- [ ] **Step 3: Clippy**

```bash
cargo clippy --workspace -- -D warnings
```

Expected: no warnings.

- [ ] **Step 4: Frontend checks**

```bash
cd apps/web && bun run typecheck && bun x ultracite check && bun run test
```

Expected: all pass.

- [ ] **Step 5: Verify preset count**

Write a quick sanity check — the `test_load_returns_96_targets` test in presets module should confirm 96 targets are loaded.

```bash
cargo test -p serverbee-server test_load_returns_96_targets -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Update `TESTING.md`**

Per CLAUDE.md: "Keep `TESTING.md` in sync with code changes." Update test counts to reflect:
- 8 new tests in `presets/mod.rs` (test_load_returns_96_targets, test_all_ids_unique, test_find_existing_target, etc.)
- Modified tests in `service/network_probe.rs` (is_builtin → source checks)
- Modified integration tests (is_builtin → preset ID checks)

- [ ] **Step 7: Verify no `is_builtin` references remain**

```bash
grep -r "is_builtin" crates/ apps/web/src/ --include="*.rs" --include="*.ts" --include="*.tsx"
```

Expected: no matches (except possibly in this plan doc or spec doc).
