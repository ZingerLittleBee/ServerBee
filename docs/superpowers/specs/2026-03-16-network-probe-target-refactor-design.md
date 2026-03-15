# Network Probe Target Storage Refactor

**Date**: 2026-03-16
**Status**: Approved

## Problem

Built-in probe targets (96 entries: 31 provinces x 3 ISPs + 3 international) are stored in the database via migrations. This causes:

1. **Migration bloat** — each update requires a new migration with 96 INSERT statements
2. **Destructive updates** — updating built-in targets deletes associated configs and historical records
3. **Mixed concerns** — static application data and user data share the same table, requiring `is_builtin` guards throughout the stack

## Design

### Core Principle

**Preset targets live in code, not in the database.** The database stores only user-created targets. The API merges both sources transparently.

### Preset Definition File

`crates/server/src/presets/targets.toml`, embedded via `include_str!`:

```toml
[[presets]]
id = "china-telecom"
name = "中国电信"
description = "31 provinces, China Telecom"

[[presets.targets]]
id = "cn-bj-ct"
name = "Beijing Telecom"
provider = "Telecom"
location = "Beijing"
target = "bj-ct-v4.ip.zstaticcdn.com:80"
probe_type = "tcp"
# ... remaining 30 provinces

[[presets]]
id = "china-unicom"
name = "中国联通"
description = "31 provinces, China Unicom"
# ... same structure

[[presets]]
id = "china-mobile"
name = "中国移动"
description = "31 provinces, China Mobile"
# ... same structure

[[presets]]
id = "international"
name = "国际节点"
description = "Well-known international targets"

[[presets.targets]]
id = "intl-cloudflare"
name = "Cloudflare"
provider = "Cloudflare"
location = "US"
target = "1.1.1.1"
probe_type = "icmp"

[[presets.targets]]
id = "intl-google"
name = "Google DNS"
provider = "Google"
location = "US"
target = "8.8.8.8"
probe_type = "icmp"

[[presets.targets]]
id = "intl-aws-tokyo"
name = "AWS Tokyo"
provider = "AWS"
location = "Tokyo"
target = "13.112.63.251"
probe_type = "icmp"
```

Four presets organized by ISP: China Telecom (31), China Unicom (31), China Mobile (31), International (3). Total: 96 targets.

Parsed once at startup via `LazyLock`, cached in memory.

### Database Changes

**Modify `m20260315_000004_network_probe`:**
- `network_probe_target`: remove `is_builtin` column, remove all 96 seed INSERTs
- `network_probe_config`: remove `REFERENCES network_probe_target(id) ON DELETE CASCADE` foreign key on `target_id` (target_id may reference in-memory preset targets)
- `network_probe_record` and `network_probe_record_hourly`: remove `target_id` foreign key constraints

**Delete `m20260315_000005_update_builtin_targets`** entirely.

Since the project is in MVP stage with no production databases, this is a clean rewrite of m004 rather than an additive migration.

**Entity change** (`network_probe_target.rs`):
```diff
- pub is_builtin: bool,
```

### Service Layer

**New module: `crates/server/src/presets/mod.rs`**

```rust
pub struct PresetTargets;

impl PresetTargets {
    /// Parse embedded TOML, return all preset targets. Cached via LazyLock.
    pub fn load() -> &'static [PresetTarget];

    /// Find a single preset target by ID.
    pub fn find(id: &str) -> Option<&'static PresetTarget>;

    /// Check if an ID belongs to a preset target.
    pub fn is_preset(id: &str) -> bool;
}
```

**TOML deserialization structs:**

```rust
#[derive(Deserialize)]
struct PresetsFile {
    presets: Vec<PresetGroup>,
}

#[derive(Deserialize)]
struct PresetGroup {
    id: String,
    name: String,
    description: String,
    targets: Vec<PresetTarget>,
}

#[derive(Deserialize, Clone)]
pub struct PresetTarget {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub location: String,
    pub target: String,
    pub probe_type: String,
}
```

**Unified DTO returned by API:**

```rust
pub struct TargetDto {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub location: String,
    pub target: String,
    pub probe_type: String,
    pub source: Option<String>,  // "preset:china-telecom" | None
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}
```

`created_at` and `updated_at` are `Option<String>` — preset targets return `None`, user-created targets return timestamps.

**Changes to `network_probe.rs`:**

| Function | Change |
|---|---|
| `list_targets` | Merge preset targets (from TOML) + custom targets (from DB), return `Vec<TargetDto>` |
| `create_target` | Unchanged, creates in DB with `source = None` |
| `update_target` | Check `PresetTargets::is_preset(id)` **before** DB lookup → 403. Then proceed with DB update |
| `delete_target` | Check `PresetTargets::is_preset(id)` **before** DB lookup → 403. Then proceed with DB delete |
| `get_server_targets` | Resolve target IDs from `network_probe_config` against both `PresetTargets::find(id)` and DB, return `Vec<TargetDto>` instead of `Vec<network_probe_target::Model>` |
| `get_overview` | Merge `PresetTargets::load()` into the `target_map` so preset target names display correctly |
| `get_anomalies` | Inherits fix from `get_server_targets` — uses TargetDto for name lookup |
| `get_server_summary` | Mechanical change: `get_server_targets` now returns `Vec<TargetDto>`, update field access accordingly |
| `set_server_targets` | Validate target IDs via `PresetTargets::is_preset(id) \|\| exists_in_db(id)`, reject invalid IDs with 400 |
| `apply_defaults` | Unchanged (inserts config rows referencing preset IDs; works because FK constraints are removed) |

### API Changes

No new endpoints. Existing endpoints change behavior:

| Endpoint | Change |
|---|---|
| `GET /api/network-probes/targets` | Returns merged preset + custom list with `source` field. Response type: `Vec<TargetDto>` |
| `POST /api/network-probes/targets` | Unchanged |
| `PUT /api/network-probes/targets/{id}` | Returns 403 if target is a preset |
| `DELETE /api/network-probes/targets/{id}` | Returns 403 if target is a preset |
| `GET /api/servers/{id}/network-probes/targets` | Returns `Vec<TargetDto>` (resolved from presets + DB) |
| `GET /api/servers/{id}/network-probes/summary` | Target names resolved from presets + DB |

### Frontend Changes

**`network-types.ts`:**
```diff
  export interface NetworkProbeTarget {
-   is_builtin: boolean
+   source: string | null
  }
```

**`settings/network-probes.tsx`:**
- Replace `is_builtin` column with `source` column: `null` shows nothing, `"preset:xxx"` shows preset name tag
- Actions column: hide edit/delete buttons when `source !== null`

**No new components or hooks needed.** Existing `useNetworkTargets()` automatically receives the merged list.

### Preset Domain Convention

China targets use Zstatic CDN backbone nodes: `{province_code}-{isp_code}-v4.ip.zstaticcdn.com:80`

- Domain is fixed; DNS resolves to rotating IPs (updated every ~30 minutes)
- Agent resolves DNS on every probe via `TcpStream::connect`, automatically tracking current node IPs
- No IP addresses stored or maintained for China targets

International targets use fixed well-known IPs with ICMP probe type.

### ID Collision Prevention

Custom targets use UUID-based IDs (`Uuid::new_v4().to_string()`), while preset targets use structured IDs like `cn-bj-ct`. UUID format makes collisions impossible in practice. No additional guard needed since `create_target` always generates UUIDs server-side.

## Files Changed

### Rust (server)
- `crates/server/src/presets/mod.rs` — **new**, preset loading + LazyLock cache
- `crates/server/src/presets/targets.toml` — **new**, preset definitions
- `crates/server/src/entity/network_probe_target.rs` — remove `is_builtin`
- `crates/server/src/service/network_probe.rs` — merge logic in `list_targets`/`get_server_targets`/`get_overview`/`get_anomalies`, remove `is_builtin` guards, add validation in `set_server_targets`, return `TargetDto`
- `crates/server/src/router/api/network_probe.rs` — response type changes for targets and per-server endpoints
- `crates/server/src/router/api/server.rs` — update `NetworkProbeTarget` mapping to use `TargetDto`
- `crates/server/src/router/ws/agent.rs` — update target mapping to use `TargetDto`
- `crates/server/src/migration/m20260315_000004_network_probe.rs` — remove `is_builtin`, remove seed INSERTs, remove FK constraints on `target_id`
- `crates/server/src/migration/m20260315_000005_update_builtin_targets.rs` — **delete**
- `crates/server/src/migration/mod.rs` — remove m005 reference

### Frontend (web)
- `apps/web/src/lib/network-types.ts` — `is_builtin` → `source`, `created_at`/`updated_at` become optional
- `apps/web/src/routes/_authed/settings/network-probes.tsx` — source column, conditional actions

### Tests
- Update unit tests in `network_probe.rs` that reference `is_builtin`
- Update integration test if it checks `is_builtin` field
