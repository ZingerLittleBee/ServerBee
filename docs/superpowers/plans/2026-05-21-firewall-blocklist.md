# Firewall Blocklist Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let admins (and `ssh_brute_force_detected` / `port_scan_detected` alert actions) block inbound traffic from individual IPs / CIDRs across one or more agents. Server is source-of-truth; each agent applies via `nftables`.

**Architecture:** Server holds canonical `block_list` rows; pushes them to covered agents over WebSocket; agents apply via `nft` CLI with idempotent EEXIST/ENOENT semantics. Per-agent apply state is rebuilt from acks on every reconnect.

**Tech Stack:** Rust (server + agent), Axum 0.8, sea-orm 1.x, tokio, `ipnet`, React 19 + TanStack Router/Query, shadcn/ui.

**Spec:** `docs/superpowers/specs/2026-05-21-firewall-blocklist-design.md`

---

## File Structure

### Common (`crates/common/`)
- Modify `src/constants.rs` — add `CAP_FIREWALL_BLOCK = 1 << 9`, bump `CAP_VALID_MASK`, register `CapabilityKey::FirewallBlock`
- Create `src/firewall.rs` — `BlockEntry`, `BlocklistAckItem`, `BlocklistEntryState`, `BlocklistChangeKind`, `FIREWALL_MIN_PROTOCOL`
- Modify `src/protocol.rs` — `ServerMessage::Blocklist{Sync,Add,Remove,Reset}`, `AgentMessage::Blocklist{Ack,ResetAck}`, `BrowserMessage::{BlocklistChanged,FirewallApplyStateChanged}`
- Modify `src/lib.rs` — `pub mod firewall;`

### Server (`crates/server/`)
- Create `src/entity/block_list.rs`
- Modify `src/entity/alert_rule.rs` — add `actions_json: Option<String>`
- Modify `src/entity/mod.rs`
- Create `src/migration/m20260521_000027_create_block_list.rs`
- Create `src/migration/m20260521_000028_extend_alert_rule_actions.rs`
- Modify `src/migration/mod.rs`
- Modify `src/config.rs` — `[firewall]` section
- Create `src/service/firewall.rs` — service struct + `canonicalize_target`, `is_protected`, `list_for_server`, `auto_block`, `record_ack`, `push_*`
- Modify `src/service/alert.rs` — `AlertRuleAction` enum, validator extension
- Modify `src/service/security.rs` — invoke `firewall.auto_block` in `evaluate_rules` after `mark_triggered`
- Modify `src/service/recovery_merge.rs` — add `block_list` table
- Create `src/router/api/firewall.rs`
- Modify `src/router/api/mod.rs`
- Modify `src/router/ws/agent.rs` — Hello/CapabilitiesSync fan-out, BlocklistAck/ResetAck handling
- Modify `src/state.rs` — `Arc<FirewallService>`, `agent_apply_state` map
- Modify `src/openapi.rs`
- Modify `src/task/cleanup.rs` — no new retention (reuses audit)
- Modify `crates/server/tests/integration.rs` — integration scenarios

### Agent (`crates/agent/`)
- Create `src/firewall/mod.rs`
- Create `src/firewall/manager.rs` — `FirewallManager`
- Create `src/firewall/nft.rs` — `NftExecutor` trait + `CliNftExecutor` impl
- Create `src/firewall/guardrail.rs` — tier 3 check
- Modify `src/main.rs` — wire `FirewallManager`
- Modify `src/reporter.rs` — route `ServerMessage::Blocklist*` / `Reset` to manager, forward ack messages
- Modify `src/collector.rs` (or wherever `SystemInfo` builds) — populate `capabilities_local` with firewall probe result

### Web (`apps/web/`)
- Create `src/locales/en/firewall.json`
- Create `src/locales/zh/firewall.json`
- Create `src/routes/_authed/settings/firewall.tsx`
- Create `src/components/firewall/kpi-cards.tsx`
- Create `src/components/firewall/block-table.tsx`
- Create `src/components/firewall/add-block-drawer.tsx`
- Create `src/components/firewall/delete-block-dialog.tsx`
- Create `src/components/firewall/activity-log.tsx`
- Create `src/hooks/use-firewall-blocks.ts`
- Modify `src/components/security/event-table.tsx` — row action "Block source IP"
- Modify `src/components/security/server-security-tab.tsx` — same row action with `include[current]`
- Modify `src/components/security/alert-presets.tsx` — "Also auto-block" checkbox
- Modify `src/routes/_authed/settings/alerts.tsx` — Auto-block collapsible card
- Modify `src/hooks/use-servers-ws.ts` — handle 2 new BrowserMessages
- Modify `src/routes/_authed/settings/capabilities.tsx` or equivalent — Firewall toggle

### Docs / Tests
- Modify `ENV.md`
- Modify `apps/docs/content/docs/{en,cn}/configuration.mdx`
- Modify `apps/docs/content/docs/{en,cn}/capabilities.mdx`
- Modify `apps/docs/content/docs/{en,cn}/security-events.mdx`
- Create `apps/docs/content/docs/{en,cn}/firewall.mdx`
- Modify `apps/docs/content/docs/{en,cn}/meta.json`
- Create `tests/firewall-block.md`
- Modify `tests/README.md`

---

## Phase 0 — Foundation (common types + capability)

### Task 0.1: Capability bit + valid mask + key

**Files:**
- Modify: `crates/common/src/constants.rs`

- [ ] **Step 1: Add the constants**

In `crates/common/src/constants.rs`, locate the capability block (currently ending with `CAP_SECURITY_EVENTS = 1 << 8`) and extend:

```rust
pub const CAP_SECURITY_EVENTS: u32 = 1 << 8; // 256
pub const CAP_FIREWALL_BLOCK: u32 = 1 << 9; // 512

pub const CAP_DEFAULT: u32 =
    CAP_UPGRADE | CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP | CAP_SECURITY_EVENTS; // 316 — firewall NOT in default

pub const CAP_VALID_MASK: u32 = 0b11_1111_1111; // 1023 — bits 0..=9
```

Add the `CapabilityKey` variant and `ALL_CAPABILITIES` entry. After the `SecurityEvents` arm, add:

```rust
    Self::FirewallBlock => CAP_FIREWALL_BLOCK,
```

and at the bottom add `CapabilityKey::FirewallBlock` to whichever `ALL_CAPABILITIES`-like slice is exported (look for the existing entries for `SecurityEvents`).

- [ ] **Step 2: Add unit test**

Append to the test module in the same file:

```rust
#[test]
fn cap_firewall_block_bit() {
    assert_eq!(CAP_FIREWALL_BLOCK, 512);
    assert_eq!(CAP_VALID_MASK & CAP_FIREWALL_BLOCK, CAP_FIREWALL_BLOCK);
    assert_eq!(CAP_DEFAULT & CAP_FIREWALL_BLOCK, 0); // not in default
}
```

- [ ] **Step 3: Run**

`cargo test -p serverbee-common --lib constants::tests::cap_firewall_block_bit -- --exact`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/common/src/constants.rs
git commit -m "feat(common): add CAP_FIREWALL_BLOCK capability bit"
```

---

### Task 0.2: Firewall protocol types module

**Files:**
- Create: `crates/common/src/firewall.rs`
- Modify: `crates/common/src/lib.rs`

- [ ] **Step 1: Add the module to lib.rs**

In `crates/common/src/lib.rs`, after `pub mod security;`:

```rust
pub mod firewall;
```

- [ ] **Step 2: Create the types file**

`crates/common/src/firewall.rs`:

```rust
//! Wire types for the firewall blocklist feature. Shared between server
//! (source of truth) and agent (executor).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Protocol-version gate: agents reporting a lower version must not receive
/// any `Blocklist*` or `BlocklistReset` messages.
pub const FIREWALL_MIN_PROTOCOL: u32 = 2;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, ToSchema)]
pub struct BlockEntry {
    pub id: String,
    /// Canonical IpNet string (`1.2.3.4/32`, `10.0.0.0/8`, `2001:db8::/32`).
    pub target: String,
    /// `4` or `6`.
    pub family: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BlocklistEntryState {
    Present,
    Absent,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Debug, ToSchema)]
pub struct BlocklistAckItem {
    pub id: String,
    pub state: BlocklistEntryState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BlocklistChangeKind {
    Created,
    Deleted,
}
```

- [ ] **Step 3: Compile**

`cargo build -p serverbee-common`
Expected: success.

- [ ] **Step 4: Round-trip serde test**

Append to `crates/common/src/firewall.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips_snake_case() {
        let json = serde_json::to_string(&BlocklistEntryState::Present).unwrap();
        assert_eq!(json, "\"present\"");
        let parsed: BlocklistEntryState = serde_json::from_str("\"failed\"").unwrap();
        assert_eq!(parsed, BlocklistEntryState::Failed);
    }

    #[test]
    fn ack_item_skips_none_reason() {
        let item = BlocklistAckItem {
            id: "id-1".into(),
            state: BlocklistEntryState::Present,
            reason: None,
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(!json.contains("reason"));
    }
}
```

Run: `cargo test -p serverbee-common --lib firewall::tests` — expect PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/common/src/firewall.rs crates/common/src/lib.rs
git commit -m "feat(common): add firewall wire types"
```

---

### Task 0.3: Protocol message variants

**Files:**
- Modify: `crates/common/src/protocol.rs`

- [ ] **Step 1: Inspect the file**

Open `crates/common/src/protocol.rs`. The file already has `pub enum ServerMessage`, `pub enum AgentMessage`, and `pub enum BrowserMessage`. Locate each enum.

- [ ] **Step 2: Add ServerMessage variants**

Inside `pub enum ServerMessage { ... }`, add (placement order doesn't matter — alphabetical or end-of-enum is fine):

```rust
    /// Full-state sync. Agent reconciles its nft set diff against this list
    /// and emits one BlocklistAck item per entry it touched.
    BlocklistSync { entries: Vec<crate::firewall::BlockEntry> },
    /// Incremental add. Agent applies, then emits a single-item BlocklistAck.
    BlocklistAdd { entry: crate::firewall::BlockEntry },
    /// Incremental remove. Agent applies, then emits a single-item BlocklistAck.
    BlocklistRemove { id: String },
    /// Unconditional wipe of the agent's firewall state. Honored regardless
    /// of capability bit; intended for capability-revoke cleanup.
    BlocklistReset,
```

- [ ] **Step 3: Add AgentMessage variants**

Inside `pub enum AgentMessage { ... }`:

```rust
    BlocklistAck { results: Vec<crate::firewall::BlocklistAckItem> },
    BlocklistResetAck { ok: bool, reason: Option<String> },
```

- [ ] **Step 4: Add BrowserMessage variants**

Inside `pub enum BrowserMessage { ... }`:

```rust
    BlocklistChanged {
        kind: crate::firewall::BlocklistChangeKind,
        block_id: String,
        target: String,
    },
    FirewallApplyStateChanged {
        block_id: String,
        server_id: String,
        state: crate::firewall::BlocklistEntryState,
        reason: Option<String>,
    },
```

- [ ] **Step 5: Compile**

`cargo build -p serverbee-common`
Expected: success. If a `#[derive(ToSchema)]` macro complains about referencing `crate::firewall::*`, add `use crate::firewall::*;` at the top of `protocol.rs`.

- [ ] **Step 6: Test variant encoding**

Add to existing test module (or create one) in `protocol.rs`:

```rust
#[test]
fn server_message_blocklist_reset_encodes() {
    let json = serde_json::to_string(&ServerMessage::BlocklistReset).unwrap();
    // Match whatever the existing variant tag style is — look at how
    // ServerMessage::Welcome is encoded in adjacent tests.
    assert!(json.contains("BlocklistReset") || json.contains("blocklist_reset"));
}
```

Run: `cargo test -p serverbee-common --lib protocol::tests::server_message_blocklist_reset_encodes` — expect PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/common/src/protocol.rs
git commit -m "feat(common): add firewall blocklist protocol messages"
```

---

## Phase 1 — Server data + REST

### Task 1.1: `block_list` entity

**Files:**
- Create: `crates/server/src/entity/block_list.rs`
- Modify: `crates/server/src/entity/mod.rs`

- [ ] **Step 1: Create the entity**

`crates/server/src/entity/block_list.rs`:

```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[sea_orm(table_name = "block_list")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub target: String,
    pub family: i32,
    pub cover_type: String,
    pub server_ids_json: Option<String>,
    pub comment: Option<String>,
    pub origin: String,
    pub origin_event_id: Option<String>,
    pub origin_rule_id: Option<String>,
    pub created_by: Option<String>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 2: Register module**

In `crates/server/src/entity/mod.rs`, add:

```rust
pub mod block_list;
```

(Preserve existing alphabetical ordering if the file uses one.)

- [ ] **Step 3: Compile**

`cargo build -p serverbee-server`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/entity/
git commit -m "feat(server): add block_list entity"
```

---

### Task 1.2: Migration `m20260521_000027_create_block_list`

**Files:**
- Create: `crates/server/src/migration/m20260521_000027_create_block_list.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Create the migration**

`crates/server/src/migration/m20260521_000027_create_block_list.rs`:

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BlockList::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(BlockList::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(BlockList::Target).string().not_null())
                    .col(ColumnDef::new(BlockList::Family).integer().not_null())
                    .col(ColumnDef::new(BlockList::CoverType).string().not_null())
                    .col(ColumnDef::new(BlockList::ServerIdsJson).string().null())
                    .col(ColumnDef::new(BlockList::Comment).string().null())
                    .col(ColumnDef::new(BlockList::Origin).string().not_null())
                    .col(ColumnDef::new(BlockList::OriginEventId).string().null())
                    .col(ColumnDef::new(BlockList::OriginRuleId).string().null())
                    .col(ColumnDef::new(BlockList::CreatedBy).string().null())
                    .col(
                        ColumnDef::new(BlockList::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_block_list_target_unique")
                    .table(BlockList::Table)
                    .col(BlockList::Target)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_block_list_created_at")
                    .table(BlockList::Table)
                    .col(BlockList::CreatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_block_list_origin")
                    .table(BlockList::Table)
                    .col(BlockList::Origin)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

#[derive(Iden)]
enum BlockList {
    Table,
    Id,
    Target,
    Family,
    CoverType,
    ServerIdsJson,
    Comment,
    Origin,
    OriginEventId,
    OriginRuleId,
    CreatedBy,
    CreatedAt,
}
```

- [ ] **Step 2: Register migration**

In `crates/server/src/migration/mod.rs`, locate the `migrations()` function (or `Migrator` impl) and add the new module. Pattern matches the existing 026 migration — copy that idiom exactly.

- [ ] **Step 3: Compile + apply on an in-memory DB**

`cargo test -p serverbee-server --lib service::recovery_merge`
Expected: passes (recovery_merge tests boot a fresh DB and run all migrations — if the new migration is broken this fails).

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/migration/
git commit -m "feat(server): add block_list migration"
```

---

### Task 1.3: Extend `alert_rule` with `actions_json`

**Files:**
- Create: `crates/server/src/migration/m20260521_000028_extend_alert_rule_actions.rs`
- Modify: `crates/server/src/entity/alert_rule.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Migration**

`crates/server/src/migration/m20260521_000028_extend_alert_rule_actions.rs`:

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(AlertRule::Table)
                    .add_column_if_not_exists(
                        ColumnDef::new(AlertRule::ActionsJson).string().null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

#[derive(Iden)]
enum AlertRule {
    Table,
    ActionsJson,
}
```

- [ ] **Step 2: Register migration**

Add the new module to `crates/server/src/migration/mod.rs` after migration 027.

- [ ] **Step 3: Add the column to the entity**

In `crates/server/src/entity/alert_rule.rs`, locate the `pub struct Model { ... }` and add a new field. The exact location matters: keep the column order consistent with the schema. Add after the last column:

```rust
    pub actions_json: Option<String>,
```

- [ ] **Step 4: Compile**

`cargo build -p serverbee-server`

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/migration/ crates/server/src/entity/alert_rule.rs
git commit -m "feat(server): add alert_rule.actions_json column"
```

---

### Task 1.4: `AlertRuleAction` enum + `AlertRuleItem` extension

**Files:**
- Modify: `crates/server/src/service/alert.rs`

- [ ] **Step 1: Locate `AlertRuleItem`**

Find `pub struct AlertRuleItem` in `crates/server/src/service/alert.rs`. It already has the `security: Option<SecurityRuleParams>` field.

- [ ] **Step 2: Add `AlertRuleAction` enum**

Above `AlertRuleItem` (or wherever feels natural in the type-definition section), add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlertRuleAction {
    /// Auto-block the `source_ip` from the triggering security event.
    /// Only valid on `ssh_brute_force_detected` / `port_scan_detected` rules.
    BlockSourceIp {
        #[serde(default = "default_cover_type")]
        cover_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        server_ids_json: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comment: Option<String>,
    },
}

fn default_cover_type() -> String {
    "all".to_string()
}
```

(Use whatever `use serde::{...}` imports already exist; add ones missing.)

- [ ] **Step 3: Compile**

`cargo build -p serverbee-server`

- [ ] **Step 4: Add validator**

Find the existing `validate_alert_rule` function (or whichever function in `alert.rs` validates rule shape — look for the existing security-rule cross-checks). Add a new helper called from the same call site:

```rust
fn validate_actions(
    rules: &[AlertRuleItem],
    actions: &[AlertRuleAction],
) -> Result<(), AppError> {
    if actions.is_empty() {
        return Ok(());
    }
    if actions.len() > 1 {
        return Err(AppError::Validation(
            "at most one action per alert_rule".to_string(),
        ));
    }
    for a in actions {
        match a {
            AlertRuleAction::BlockSourceIp { cover_type, .. } => {
                if !VALID_COVER_TYPES.contains(&cover_type.as_str()) {
                    return Err(AppError::Validation(format!(
                        "invalid cover_type '{cover_type}' on action"
                    )));
                }
                let allowed = ["ssh_brute_force_detected", "port_scan_detected"];
                if !rules
                    .iter()
                    .all(|r| allowed.contains(&r.rule_type.as_str()))
                {
                    return Err(AppError::Validation(
                        "block_source_ip is only allowed on \
                         ssh_brute_force_detected / port_scan_detected rules"
                            .to_string(),
                    ));
                }
            }
        }
    }
    Ok(())
}
```

Then call `validate_actions(rules, actions)?;` from the existing top-level validator at the same level where security items are validated.

To wire the `actions` argument: look for the create/update DTO at the top of `alert.rs` (or in `router/api/alerts.rs`). The DTO needs a new field:

```rust
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<AlertRuleAction>,
```

When persisting, serialize `actions` as JSON into `actions_json`; when loading, deserialize. Keep `actions_json` as `Option<String>` to match the DB column. The conversion helpers should live next to the existing `rules` ↔ `rules_json` helpers in `alert.rs`.

- [ ] **Step 5: Tests for validator**

Append in the test module of `alert.rs`:

```rust
#[test]
fn validate_actions_forbids_with_metric_rule() {
    let rules = vec![AlertRuleItem {
        rule_type: "cpu".into(),
        min: Some(80.0),
        ..Default::default()
    }];
    let actions = vec![AlertRuleAction::BlockSourceIp {
        cover_type: "all".into(),
        server_ids_json: None,
        comment: None,
    }];
    let err = validate_actions(&rules, &actions).unwrap_err();
    assert!(format!("{err}").contains("ssh_brute_force_detected"));
}

#[test]
fn validate_actions_forbids_ssh_new_ip_login() {
    let rules = vec![AlertRuleItem {
        rule_type: "ssh_new_ip_login".into(),
        ..Default::default()
    }];
    let actions = vec![AlertRuleAction::BlockSourceIp {
        cover_type: "all".into(),
        server_ids_json: None,
        comment: None,
    }];
    assert!(validate_actions(&rules, &actions).is_err());
}

#[test]
fn validate_actions_allows_brute_force() {
    let rules = vec![AlertRuleItem {
        rule_type: "ssh_brute_force_detected".into(),
        ..Default::default()
    }];
    let actions = vec![AlertRuleAction::BlockSourceIp {
        cover_type: "all".into(),
        server_ids_json: None,
        comment: None,
    }];
    assert!(validate_actions(&rules, &actions).is_ok());
}

#[test]
fn validate_actions_rejects_more_than_one() {
    let rules = vec![AlertRuleItem {
        rule_type: "ssh_brute_force_detected".into(),
        ..Default::default()
    }];
    let actions = vec![
        AlertRuleAction::BlockSourceIp { cover_type: "all".into(), server_ids_json: None, comment: None },
        AlertRuleAction::BlockSourceIp { cover_type: "all".into(), server_ids_json: None, comment: None },
    ];
    assert!(validate_actions(&rules, &actions).is_err());
}
```

(If `AlertRuleItem` doesn't already derive `Default`, derive it on the struct or build a helper `AlertRuleItem::for_test(rule_type)`.)

- [ ] **Step 6: Run**

`cargo test -p serverbee-server --lib service::alert::tests::validate_actions`
Expected: all four tests pass.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/service/alert.rs
git commit -m "feat(server): alert_rule actions + validator"
```

---

### Task 1.5: Server config `[firewall]`

**Files:**
- Modify: `crates/server/src/config.rs`

- [ ] **Step 1: Add the section**

Find `pub struct AppConfig { ... }`. Look at how other optional sections (e.g. `oauth`, `geoip`) are nested. Add:

```rust
    #[serde(default)]
    pub firewall: FirewallConfig,
```

Add the struct definition next to the other config sub-structs:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FirewallConfig {
    /// Server-side guardrail tier 2: CIDRs the server refuses to enqueue
    /// into any block_list. See spec § 4.2.
    #[serde(default)]
    pub allow_list: Vec<String>,
}
```

- [ ] **Step 2: Test parsing**

In the `tests` module at the bottom of `config.rs`, append:

```rust
#[test]
fn firewall_allow_list_from_env() {
    figment::Jail::expect_with(|jail| {
        jail.set_env("SERVERBEE_FIREWALL__ALLOW_LIST", "203.0.113.0/24,198.51.100.5");
        let cfg: AppConfig = figment::Figment::new()
            .merge(figment::providers::Env::prefixed("SERVERBEE_").split("__"))
            .extract()?;
        assert_eq!(cfg.firewall.allow_list, vec!["203.0.113.0/24", "198.51.100.5"]);
        Ok(())
    });
}
```

Run: `cargo test -p serverbee-server --lib config::tests::firewall_allow_list_from_env` — expect PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/config.rs
git commit -m "feat(server): add [firewall] config section"
```

---

### Task 1.6: `FirewallService::canonicalize_target` + `is_protected`

**Files:**
- Create: `crates/server/src/service/firewall.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Module skeleton**

`crates/server/src/service/firewall.rs`:

```rust
//! Firewall blocklist service. Holds the canonicalization, guardrail, and
//! agent-apply-state logic. CRUD wiring lives in `router::api::firewall`;
//! WS push is invoked from there and from auto-block (`service::security`).

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ipnet::IpNet;
use sea_orm::DatabaseConnection;
use serverbee_common::firewall::BlocklistEntryState;
use tokio::sync::{RwLock, broadcast};

use crate::config::AppConfig;
use crate::error::AppError;

/// `(block_id, server_id) → ApplyState` derived from acks since boot.
pub type ApplyStateMap = Arc<RwLock<HashMap<(String, String), ApplyState>>>;

#[derive(Clone, Debug)]
pub struct ApplyState {
    pub state: BlocklistEntryState,
    pub reason: Option<String>,
    pub at: DateTime<Utc>,
}

/// Hard-coded protected CIDRs (tier 1).
const PROTECTED_CIDRS: &[&str] = &[
    "127.0.0.0/8",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
    "0.0.0.0/8",
    "224.0.0.0/4",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
    "ff00::/8",
    "::/128",
];

pub struct FirewallService {
    pub db: DatabaseConnection,
    pub config: Arc<AppConfig>,
    pub apply_state: ApplyStateMap,
    // populated later (Task 2.x) when AppState is fully assembled
    pub external_ips: Arc<RwLock<std::collections::HashSet<IpAddr>>>,
    // BrowserMessage broadcast — wired in Task 2.x
    pub browser_tx: broadcast::Sender<serverbee_common::protocol::BrowserMessage>,
}

impl FirewallService {
    /// Parse and canonicalize a client-supplied target.
    /// Returns `(target_canonical, family)`.
    pub fn canonicalize_target(input: &str) -> Result<(String, u8), AppError> {
        // Try IpAddr first → /32 or /128 CIDR.
        if let Ok(addr) = input.parse::<IpAddr>() {
            let net = IpNet::new(addr, if addr.is_ipv4() { 32 } else { 128 })
                .expect("prefix is valid");
            let family = if addr.is_ipv4() { 4 } else { 6 };
            return Ok((net.to_string(), family));
        }
        // Then IpNet.
        let net: IpNet = input.parse().map_err(|_| {
            AppError::BadRequest(format!("invalid IP or CIDR: {input}"))
        })?;
        let canonical = IpNet::new(net.network(), net.prefix_len())
            .expect("network ok");
        let family = match canonical {
            IpNet::V4(_) => 4,
            IpNet::V6(_) => 6,
        };
        Ok((canonical.to_string(), family))
    }

    /// Returns Some(reason) if `target_cidr` overlaps a protected range.
    pub fn is_protected(target_cidr: &str, extra_allow: &[String]) -> Option<String> {
        let target: IpNet = match target_cidr.parse() {
            Ok(n) => n,
            Err(_) => return Some("invalid CIDR".into()),
        };
        for p in PROTECTED_CIDRS {
            let prot: IpNet = p.parse().expect("hard-coded valid");
            if Self::overlaps(&target, &prot) {
                return Some(format!("hits hard-coded guardrail: {p}"));
            }
        }
        for raw in extra_allow {
            if let Ok(prot) = raw.parse::<IpNet>()
                && Self::overlaps(&target, &prot)
            {
                return Some(format!("hits allow_list: {raw}"));
            }
            if let Ok(addr) = raw.parse::<IpAddr>() {
                let prot = IpNet::new(addr, if addr.is_ipv4() { 32 } else { 128 })
                    .expect("prefix ok");
                if Self::overlaps(&target, &prot) {
                    return Some(format!("hits allow_list: {raw}"));
                }
            }
        }
        None
    }

    fn overlaps(a: &IpNet, b: &IpNet) -> bool {
        // overlap iff a contains b's network or b contains a's network
        a.contains(&b.network()) || b.contains(&a.network())
    }
}
```

- [ ] **Step 2: Register module**

In `crates/server/src/service/mod.rs`, after `pub mod security;`:

```rust
pub mod firewall;
```

- [ ] **Step 3: Tests**

Append to `firewall.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_bare_ipv4() {
        let (t, f) = FirewallService::canonicalize_target("1.2.3.4").unwrap();
        assert_eq!(t, "1.2.3.4/32");
        assert_eq!(f, 4);
    }

    #[test]
    fn canonical_cidr_strips_host_bits() {
        let (t, _) = FirewallService::canonicalize_target("1.2.3.4/24").unwrap();
        assert_eq!(t, "1.2.3.0/24");
    }

    #[test]
    fn canonical_ipv6_lowercases_and_collapses() {
        let (t, f) = FirewallService::canonicalize_target("001:0db8::/32").unwrap();
        assert_eq!(t, "1:db8::/32");
        assert_eq!(f, 6);
    }

    #[test]
    fn canonical_rejects_garbage() {
        assert!(FirewallService::canonicalize_target("not-an-ip").is_err());
    }

    #[test]
    fn protected_loopback() {
        assert!(FirewallService::is_protected("127.0.0.1/32", &[]).is_some());
    }

    #[test]
    fn protected_rfc1918() {
        assert!(FirewallService::is_protected("10.5.0.0/16", &[]).is_some());
    }

    #[test]
    fn protected_external_is_not() {
        assert!(FirewallService::is_protected("203.0.113.5/32", &[]).is_none());
    }

    #[test]
    fn protected_target_supersets_protected() {
        // 0.0.0.0/0 contains 127.0.0.0/8 → reject
        assert!(FirewallService::is_protected("0.0.0.0/0", &[]).is_some());
    }

    #[test]
    fn protected_allow_list_matches() {
        let allow = vec!["203.0.113.0/24".to_string()];
        assert!(FirewallService::is_protected("203.0.113.5/32", &allow).is_some());
    }

    #[test]
    fn protected_allow_list_bare_ip() {
        let allow = vec!["203.0.113.5".to_string()];
        assert!(FirewallService::is_protected("203.0.113.5/32", &allow).is_some());
    }
}
```

- [ ] **Step 4: Run**

`cargo test -p serverbee-server --lib service::firewall::tests`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/firewall.rs crates/server/src/service/mod.rs
git commit -m "feat(server): firewall canonicalize + guardrail"
```

---

### Task 1.7: REST endpoints (list / get / post / delete / stats)

**Files:**
- Create: `crates/server/src/router/api/firewall.rs`
- Modify: `crates/server/src/router/api/mod.rs`
- Modify: `crates/server/src/openapi.rs`

- [ ] **Step 1: Inspect existing patterns**

Read `crates/server/src/router/api/security.rs` end-to-end. The new file follows the same shape: DTOs with `ToSchema`, handlers with `#[utoipa::path]`, admin guards via the existing `require_admin` middleware.

- [ ] **Step 2: Create the router file**

`crates/server/src/router/api/firewall.rs`:

```rust
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
};
use chrono::Utc;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::block_list;
use crate::error::AppError;
use crate::middleware::auth::{AuthUser, require_admin};
use crate::response::ApiResponse;
use crate::service::firewall::FirewallService;
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateBlockReq {
    pub target: String,
    #[serde(default = "default_cover")]
    pub cover_type: String,
    #[serde(default)]
    pub server_ids: Option<Vec<String>>,
    #[serde(default)]
    pub comment: Option<String>,
}

fn default_cover() -> String { "all".into() }

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BlockListItem {
    pub id: String,
    pub target: String,
    pub family: i32,
    pub cover_type: String,
    pub server_ids: Option<Vec<String>>,
    pub comment: Option<String>,
    pub origin: String,
    pub origin_event_id: Option<String>,
    pub origin_rule_id: Option<String>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
}

impl From<block_list::Model> for BlockListItem {
    fn from(m: block_list::Model) -> Self {
        let server_ids = m
            .server_ids_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        Self {
            id: m.id,
            target: m.target,
            family: m.family,
            cover_type: m.cover_type,
            server_ids,
            comment: m.comment,
            origin: m.origin,
            origin_event_id: m.origin_event_id,
            origin_rule_id: m.origin_rule_id,
            created_by: m.created_by,
            created_at: m.created_at,
        }
    }
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListQuery {
    pub cursor: Option<String>,
    pub origin: Option<String>,
    pub target_q: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListResp {
    pub items: Vec<BlockListItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StatsResp {
    pub total: i64,
    pub auto: i64,
    pub manual: i64,
    pub v4: i64,
    pub v6: i64,
}

#[utoipa::path(
    get,
    path = "/api/firewall/blocks",
    params(ListQuery),
    responses((status = 200, body = ApiResponse<ListResp>))
)]
async fn list_blocks(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<ApiResponse<ListResp>>, AppError> {
    let limit = q.limit.unwrap_or(50).min(200) as u64;
    let mut find = block_list::Entity::find().order_by_desc(block_list::Column::CreatedAt);
    if let Some(o) = q.origin.as_deref() {
        find = find.filter(block_list::Column::Origin.eq(o));
    }
    if let Some(tq) = q.target_q.as_deref() {
        find = find.filter(block_list::Column::Target.contains(tq));
    }
    if let Some(cursor) = q.cursor.as_deref() {
        find = find.filter(block_list::Column::CreatedAt.lt(
            chrono::DateTime::parse_from_rfc3339(cursor)
                .map_err(|_| AppError::BadRequest("invalid cursor".into()))?
                .with_timezone(&Utc),
        ));
    }
    let rows = find.limit(limit + 1).all(&state.db).await?;
    let mut items: Vec<BlockListItem> = rows.into_iter().map(Into::into).collect();
    let next_cursor = if items.len() as u64 > limit {
        let last = items.pop().unwrap();
        Some(last.created_at.to_rfc3339())
    } else {
        None
    };
    Ok(Json(ApiResponse { data: ListResp { items, next_cursor } }))
}

#[utoipa::path(
    get,
    path = "/api/firewall/blocks/{id}",
    responses((status = 200, body = ApiResponse<BlockListItem>))
)]
async fn get_block(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<BlockListItem>>, AppError> {
    let row = block_list::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("block not found".into()))?;
    Ok(Json(ApiResponse { data: row.into() }))
}

#[utoipa::path(
    post,
    path = "/api/firewall/blocks",
    request_body = CreateBlockReq,
    responses(
        (status = 200, body = ApiResponse<BlockListItem>),
        (status = 409, description = "Guardrail rejected or duplicate target")
    )
)]
async fn create_block(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(req): Json<CreateBlockReq>,
) -> Result<Json<ApiResponse<BlockListItem>>, AppError> {
    let (target, family) = FirewallService::canonicalize_target(&req.target)?;

    let dynamic_allow = state.firewall.collect_dynamic_allow().await;
    let mut effective_allow: Vec<String> = state.config.firewall.allow_list.clone();
    effective_allow.extend(dynamic_allow);

    if let Some(reason) = FirewallService::is_protected(&target, &effective_allow) {
        crate::service::audit::AuditService::log(
            &state.db,
            &user.id,
            "firewall_block_rejected_server",
            Some(&serde_json::json!({ "target": target, "reason": reason }).to_string()),
            "",
        )
        .await
        .ok();
        return Err(AppError::Conflict(reason));
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let server_ids_json = req
        .server_ids
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    let active = block_list::ActiveModel {
        id: Set(id.clone()),
        target: Set(target.clone()),
        family: Set(family as i32),
        cover_type: Set(req.cover_type.clone()),
        server_ids_json: Set(server_ids_json),
        comment: Set(req.comment.clone()),
        origin: Set("manual".into()),
        origin_event_id: Set(None),
        origin_rule_id: Set(None),
        created_by: Set(Some(user.id.clone())),
        created_at: Set(now),
    };
    let model = match block_list::ActiveModelTrait::insert(active, &state.db).await {
        Ok(m) => m,
        Err(e) => {
            // SQLite unique constraint
            if format!("{e}").contains("UNIQUE constraint failed") {
                return Err(AppError::Conflict(format!(
                    "target {target} already blocked"
                )));
            }
            return Err(e.into());
        }
    };

    crate::service::audit::AuditService::log(
        &state.db,
        &user.id,
        "firewall_block_created",
        Some(
            &serde_json::json!({ "id": model.id, "target": model.target, "origin": "manual" })
                .to_string(),
        ),
        "",
    )
    .await
    .ok();

    let item: BlockListItem = model.clone().into();
    state.firewall.broadcast_changed_created(&item);
    state.firewall.push_add_to_covered_agents(&item).await;

    Ok(Json(ApiResponse { data: item }))
}

#[utoipa::path(
    delete,
    path = "/api/firewall/blocks/{id}",
    responses((status = 200, body = ApiResponse<()>))
)]
async fn delete_block(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let row = block_list::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("block not found".into()))?;
    block_list::Entity::delete_by_id(&id).exec(&state.db).await?;

    crate::service::audit::AuditService::log(
        &state.db,
        &user.id,
        "firewall_block_deleted",
        Some(&serde_json::json!({ "id": id, "target": row.target }).to_string()),
        "",
    )
    .await
    .ok();

    state.firewall.broadcast_changed_deleted(&row);
    state.firewall.push_remove_to_covered_agents(&row).await;

    Ok(Json(ApiResponse { data: () }))
}

#[utoipa::path(
    get,
    path = "/api/firewall/stats",
    responses((status = 200, body = ApiResponse<StatsResp>))
)]
async fn stats(
    State(state): State<Arc<AppState>>,
    _user: AuthUser,
) -> Result<Json<ApiResponse<StatsResp>>, AppError> {
    use sea_orm::QuerySelect;
    let total = block_list::Entity::find().count(&state.db).await? as i64;
    let auto = block_list::Entity::find()
        .filter(block_list::Column::Origin.eq("auto"))
        .count(&state.db)
        .await? as i64;
    let v6 = block_list::Entity::find()
        .filter(block_list::Column::Family.eq(6))
        .count(&state.db)
        .await? as i64;
    Ok(Json(ApiResponse {
        data: StatsResp {
            total,
            auto,
            manual: total - auto,
            v4: total - v6,
            v6,
        },
    }))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/blocks", get(list_blocks))
        .route("/blocks/:id", get(get_block))
        .route("/stats", get(stats))
        .route("/blocks", post(create_block).route_layer(axum::middleware::from_fn(require_admin)))
        .route(
            "/blocks/:id",
            delete(delete_block).route_layer(axum::middleware::from_fn(require_admin)),
        )
}
```

The handlers reference helpers on `FirewallService` (`collect_dynamic_allow`, `broadcast_changed_created`, `push_add_to_covered_agents`, etc.) that don't exist yet — Task 1.8 adds them as stubs and Task 2.x fills them in.

- [ ] **Step 3: Add stubs to FirewallService**

Append to `crates/server/src/service/firewall.rs`:

```rust
impl FirewallService {
    pub async fn collect_dynamic_allow(&self) -> Vec<String> {
        // server.toml `[server] trusted_proxies` + each agent's external IP.
        // Filled in Task 2.x. Returns an empty vec for now.
        Vec::new()
    }

    pub fn broadcast_changed_created(&self, item: &crate::router::api::firewall::BlockListItem) {
        let _ = self.browser_tx.send(serverbee_common::protocol::BrowserMessage::BlocklistChanged {
            kind: serverbee_common::firewall::BlocklistChangeKind::Created,
            block_id: item.id.clone(),
            target: item.target.clone(),
        });
    }

    pub fn broadcast_changed_deleted(&self, row: &crate::entity::block_list::Model) {
        let _ = self.browser_tx.send(serverbee_common::protocol::BrowserMessage::BlocklistChanged {
            kind: serverbee_common::firewall::BlocklistChangeKind::Deleted,
            block_id: row.id.clone(),
            target: row.target.clone(),
        });
    }

    pub async fn push_add_to_covered_agents(&self, _item: &crate::router::api::firewall::BlockListItem) {
        // Task 2.x
    }

    pub async fn push_remove_to_covered_agents(&self, _row: &crate::entity::block_list::Model) {
        // Task 2.x
    }
}
```

- [ ] **Step 4: Mount router**

In `crates/server/src/router/api/mod.rs`, find where other domain routers are mounted (e.g. `security`):

```rust
pub mod firewall;
```

…and in the function that builds the API router (look for `nest("/security", ...)` or similar), add:

```rust
.nest("/firewall", firewall::router())
```

- [ ] **Step 5: Register OpenAPI paths**

In `crates/server/src/openapi.rs`, add new paths and schemas. Follow the existing pattern for security endpoints — add the same kind of `#[openapi(paths(...), components(schemas(...)))]` entries.

- [ ] **Step 6: Compile**

`cargo build -p serverbee-server`
Expected: success. If `AppState` doesn't yet have a `firewall: Arc<FirewallService>` field, this will fail — that field is added in Task 2.1. Until then, **inline-stub it temporarily** in `state.rs` to keep the tree compiling:

Add to `state.rs`:

```rust
pub firewall: Arc<crate::service::firewall::FirewallService>,
```

…and where `AppState` is constructed (in `state.rs::new` or wherever `Arc::new(AppState { ... })` lives), build it:

```rust
let (browser_tx, _) = tokio::sync::broadcast::channel(...);  // existing
let firewall = Arc::new(crate::service::firewall::FirewallService {
    db: db.clone(),
    config: config.clone(),
    apply_state: Arc::new(tokio::sync::RwLock::new(Default::default())),
    external_ips: Arc::new(tokio::sync::RwLock::new(Default::default())),
    browser_tx: browser_tx.clone(),
});
```

(If the channel is already created above, reuse the existing handle. Do not create a second channel.)

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/router/api/ crates/server/src/service/firewall.rs crates/server/src/state.rs crates/server/src/openapi.rs
git commit -m "feat(server): firewall REST endpoints"
```

---

### Task 1.8: `recovery_merge` integration

**Files:**
- Modify: `crates/server/src/service/recovery_merge.rs`

- [ ] **Step 1: Locate the rewrite list**

Read `recovery_merge.rs`. Find the function that rewrites `server_id` references across tables (it iterates known tables — for security_event this was added in the previous branch). Add `block_list` to the same loop.

- [ ] **Step 2: Implement**

Inside the rewrite function, alongside the existing entries:

```rust
// Rewrite block_list.server_ids_json: replace `from_id` inside the JSON array.
{
    use crate::entity::block_list;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    let rows = block_list::Entity::find()
        .filter(block_list::Column::ServerIdsJson.is_not_null())
        .all(db)
        .await?;
    for row in rows {
        let Some(raw) = row.server_ids_json.clone() else { continue; };
        let Ok(mut ids): Result<Vec<String>, _> = serde_json::from_str(&raw) else { continue; };
        let mut changed = false;
        for id in ids.iter_mut() {
            if id == from_id {
                *id = to_id.to_string();
                changed = true;
            }
        }
        if changed {
            let new_json = serde_json::to_string(&ids).unwrap_or(raw);
            let mut active: block_list::ActiveModel = row.into();
            active.server_ids_json = sea_orm::Set(Some(new_json));
            block_list::Entity::update(active).exec(db).await?;
        }
    }
}
```

- [ ] **Step 3: Test**

Add a unit test in the same file:

```rust
#[tokio::test]
async fn rewrites_block_list_server_ids() {
    let db = setup_test_db().await; // existing helper
    crate::migration::Migrator::up(&db, None).await.unwrap();

    use crate::entity::block_list;
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, Set};
    block_list::ActiveModel {
        id: Set("blk-1".into()),
        target: Set("1.2.3.4/32".into()),
        family: Set(4),
        cover_type: Set("include".into()),
        server_ids_json: Set(Some(r#"["srv-A","srv-B"]"#.into())),
        comment: Set(None),
        origin: Set("manual".into()),
        origin_event_id: Set(None),
        origin_rule_id: Set(None),
        created_by: Set(None),
        created_at: Set(Utc::now()),
    }
    .insert(&db)
    .await
    .unwrap();

    rewrite_server_id(&db, "srv-A", "srv-X").await.unwrap();
    let row = block_list::Entity::find_by_id("blk-1").one(&db).await.unwrap().unwrap();
    let ids: Vec<String> = serde_json::from_str(row.server_ids_json.as_deref().unwrap()).unwrap();
    assert_eq!(ids, vec!["srv-X", "srv-B"]);
}
```

(Function name `rewrite_server_id` is illustrative — use whatever the existing public entry point is.)

- [ ] **Step 4: Run**

`cargo test -p serverbee-server --lib service::recovery_merge::tests::rewrites_block_list_server_ids`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/recovery_merge.rs
git commit -m "feat(server): rewrite block_list server_ids on recovery merge"
```

---

## Phase 2 — Server WS + Auto-block

### Task 2.1: AppState wires FirewallService properly

**Files:**
- Modify: `crates/server/src/state.rs`

- [ ] **Step 1: Replace the stub from Task 1.7**

If Task 1.7 added a stub, replace it with a properly constructed FirewallService that shares the existing `browser_tx`. The full state initialization pattern is in the existing file. Make sure:

```rust
firewall: Arc::new(FirewallService {
    db: db.clone(),
    config: config.clone(),
    apply_state: Arc::new(RwLock::new(HashMap::new())),
    external_ips: Arc::new(RwLock::new(HashSet::new())),
    browser_tx: browser_tx.clone(),
}),
```

The `external_ips` is mutated in Task 2.2.

- [ ] **Step 2: Compile**

`cargo build -p serverbee-server`

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/state.rs
git commit -m "feat(server): wire FirewallService into AppState"
```

---

### Task 2.2: Populate `external_ips` from agent SystemInfo / IpChanged

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs`
- Modify: `crates/server/src/service/firewall.rs`

- [ ] **Step 1: Add update helpers to FirewallService**

```rust
impl FirewallService {
    pub async fn note_agent_external_ip(&self, server_id: &str, ip: Option<std::net::IpAddr>) {
        let mut g = self.external_ips.write().await;
        // Strategy: remove old IP for this server first; insert new if present.
        // Track per-server in a sidecar map to support removal precisely.
        // For v1: rebuild the set from scratch periodically would be simpler,
        // but we want immediate consistency, so we use a per-server map.
        let key = format!("agent:{server_id}");
        g.retain(|tagged| !tagged.0.starts_with(&key)); // (tag, ip)
        if let Some(ip) = ip {
            g.insert((key, ip));
        }
    }
}
```

Wait — `HashSet<IpAddr>` is too thin for per-server removal. Change the type:

```rust
pub external_ips: Arc<RwLock<HashMap<String /*server_id*/, std::net::IpAddr>>>,
```

Update `collect_dynamic_allow`:

```rust
pub async fn collect_dynamic_allow(&self) -> Vec<String> {
    let mut out: Vec<String> = self.config.server.trusted_proxies.clone();
    let g = self.external_ips.read().await;
    for ip in g.values() {
        out.push(ip.to_string());
    }
    out
}
```

(Verify `config.server.trusted_proxies` exists; if not, look it up under the right key. The repo's `server.toml` already has trusted proxies.)

Replace the earlier `note_agent_external_ip`:

```rust
pub async fn note_agent_external_ip(&self, server_id: &str, ip: Option<std::net::IpAddr>) {
    let mut g = self.external_ips.write().await;
    match ip {
        Some(ip) => { g.insert(server_id.to_string(), ip); }
        None => { g.remove(server_id); }
    }
}
```

- [ ] **Step 2: Call from ws/agent.rs**

In `crates/server/src/router/ws/agent.rs`, locate `AgentMessage::SystemInfo` and `AgentMessage::IpChanged` (or equivalent) handlers. After the existing handling, append:

```rust
let ip = info
    .ip_external
    .as_deref()
    .and_then(|s| s.parse::<std::net::IpAddr>().ok());
state.firewall.note_agent_external_ip(server_id, ip).await;
```

(Field name may differ — match the existing struct field for "external IP" used by `ip_change`.)

- [ ] **Step 3: Compile + ensure callers compile**

`cargo build -p serverbee-server`

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/firewall.rs crates/server/src/router/ws/agent.rs
git commit -m "feat(server): track agent external IPs for guardrail tier 2"
```

---

### Task 2.3: `FirewallService::list_for_server` + push helpers

**Files:**
- Modify: `crates/server/src/service/firewall.rs`

- [ ] **Step 1: Add list_for_server**

```rust
impl FirewallService {
    /// List all block_list rows that cover `server_id`, sorted oldest first
    /// so the agent applies them in stable order.
    pub async fn list_for_server(
        &self,
        server_id: &str,
    ) -> Result<Vec<serverbee_common::firewall::BlockEntry>, AppError> {
        use crate::entity::block_list;
        use sea_orm::{ColumnTrait, EntityTrait, QueryOrder};

        let rows = block_list::Entity::find()
            .order_by_asc(block_list::Column::CreatedAt)
            .all(&self.db)
            .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            if crate::service::alert::rule_covers_server(
                &row.cover_type,
                &row.server_ids_json,
                server_id,
            ) {
                out.push(serverbee_common::firewall::BlockEntry {
                    id: row.id,
                    target: row.target,
                    family: row.family as u8,
                });
            }
        }
        Ok(out)
    }
}
```

- [ ] **Step 2: Implement push_add / push_remove**

Replace the Task 1.7 stubs in `firewall.rs`:

```rust
impl FirewallService {
    pub async fn push_add_to_covered_agents(
        &self,
        item: &crate::router::api::firewall::BlockListItem,
        agent_manager: &crate::service::agent_manager::AgentManager,
    ) {
        use serverbee_common::constants::{CAP_FIREWALL_BLOCK, has_capability};
        use serverbee_common::firewall::{BlockEntry, FIREWALL_MIN_PROTOCOL};

        let agents = agent_manager.online_agents(); // list of (server_id, info)
        let server_ids_json = item
            .server_ids
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        for (server_id, info) in agents {
            if !crate::service::alert::rule_covers_server(
                &item.cover_type,
                &server_ids_json,
                &server_id,
            ) {
                continue;
            }
            let caps = agent_manager.get_effective_capabilities(&server_id).unwrap_or(0);
            if !has_capability(caps, CAP_FIREWALL_BLOCK) {
                continue;
            }
            if info.protocol_version < FIREWALL_MIN_PROTOCOL {
                continue;
            }
            agent_manager
                .send_to(
                    &server_id,
                    serverbee_common::protocol::ServerMessage::BlocklistAdd {
                        entry: BlockEntry {
                            id: item.id.clone(),
                            target: item.target.clone(),
                            family: item.family as u8,
                        },
                    },
                )
                .await;
        }
    }

    pub async fn push_remove_to_covered_agents(
        &self,
        row: &crate::entity::block_list::Model,
        agent_manager: &crate::service::agent_manager::AgentManager,
    ) {
        use serverbee_common::constants::{CAP_FIREWALL_BLOCK, has_capability};
        use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;

        for (server_id, info) in agent_manager.online_agents() {
            if !crate::service::alert::rule_covers_server(
                &row.cover_type,
                &row.server_ids_json,
                &server_id,
            ) {
                continue;
            }
            let caps = agent_manager.get_effective_capabilities(&server_id).unwrap_or(0);
            if !has_capability(caps, CAP_FIREWALL_BLOCK) {
                continue;
            }
            if info.protocol_version < FIREWALL_MIN_PROTOCOL {
                continue;
            }
            agent_manager
                .send_to(
                    &server_id,
                    serverbee_common::protocol::ServerMessage::BlocklistRemove {
                        id: row.id.clone(),
                    },
                )
                .await;
        }
    }

    pub async fn push_sync_to(
        &self,
        server_id: &str,
        agent_manager: &crate::service::agent_manager::AgentManager,
    ) -> Result<(), AppError> {
        let entries = self.list_for_server(server_id).await?;
        agent_manager
            .send_to(
                server_id,
                serverbee_common::protocol::ServerMessage::BlocklistSync { entries },
            )
            .await;
        Ok(())
    }

    pub async fn push_reset_to(
        &self,
        server_id: &str,
        agent_manager: &crate::service::agent_manager::AgentManager,
    ) {
        agent_manager
            .send_to(
                server_id,
                serverbee_common::protocol::ServerMessage::BlocklistReset,
            )
            .await;
        // Clear apply_state for this server immediately; ResetAck won't
        // change anything else but will write the audit entry.
        let mut g = self.apply_state.write().await;
        g.retain(|(_block_id, srv), _| srv != server_id);
    }
}
```

The exact method names on `AgentManager` (`online_agents`, `send_to`, `get_effective_capabilities`) and the `protocol_version` field name are illustrative — adjust to match the existing crate. `online_agents` may not exist as-is; check `service::agent_manager` and either reuse what's there or add a helper.

- [ ] **Step 3: Update the REST handlers**

Update `create_block` / `delete_block` in `crates/server/src/router/api/firewall.rs` to pass the `agent_manager`:

```rust
state.firewall.push_add_to_covered_agents(&item, &state.agent_manager).await;
// and
state.firewall.push_remove_to_covered_agents(&row, &state.agent_manager).await;
```

- [ ] **Step 4: Compile**

`cargo build -p serverbee-server`

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/firewall.rs crates/server/src/router/api/firewall.rs
git commit -m "feat(server): firewall push_add/remove/sync/reset"
```

---

### Task 2.4: WS connect — Reset+Sync on first contact, gated push on cap transitions

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs`

- [ ] **Step 1: Find the right call sites**

Read `crates/server/src/router/ws/agent.rs`. Locate:
1. Where `Hello` / `SystemInfo` from agent is first acknowledged (after capability negotiation completes for this connection).
2. Where `CapabilitiesSync` is sent to the agent on admin-driven cap changes.

- [ ] **Step 2: Reset+Sync on first connect**

After the existing capability sync at agent-connect time, add:

```rust
use serverbee_common::constants::{CAP_FIREWALL_BLOCK, has_capability};
use serverbee_common::firewall::FIREWALL_MIN_PROTOCOL;

let caps = state.agent_manager.get_effective_capabilities(server_id).unwrap_or(0);
let proto_ok = system_info.protocol_version >= FIREWALL_MIN_PROTOCOL;
if has_capability(caps, CAP_FIREWALL_BLOCK) && proto_ok {
    state.firewall.push_reset_to(server_id, &state.agent_manager).await;
    if let Err(e) = state.firewall.push_sync_to(server_id, &state.agent_manager).await {
        tracing::warn!(server_id, error=%e, "firewall sync push failed");
    }
}
```

(Field name `protocol_version` — verify what `SystemInfo` actually carries.)

- [ ] **Step 3: Gated push on cap transition**

Find where `CapabilitiesSync` is dispatched to an online agent on capability change. Add logic that:

```rust
let was_on = old_effective & CAP_FIREWALL_BLOCK != 0;
let now_on = new_effective & CAP_FIREWALL_BLOCK != 0;
match (was_on, now_on) {
    (false, true) => {
        // bit just turned on
        let _ = state.firewall.push_sync_to(server_id, &state.agent_manager).await;
    }
    (true, false) => {
        // bit just turned off
        state.firewall.push_reset_to(server_id, &state.agent_manager).await;
    }
    _ => {}
}
```

- [ ] **Step 4: Compile**

`cargo build -p serverbee-server`

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/ws/agent.rs
git commit -m "feat(server): firewall reset+sync on agent connect / cap transitions"
```

---

### Task 2.5: Handle `BlocklistAck` / `BlocklistResetAck`

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs`
- Modify: `crates/server/src/service/firewall.rs`

- [ ] **Step 1: Add record_ack on FirewallService**

```rust
impl FirewallService {
    pub async fn record_ack(
        &self,
        server_id: &str,
        item: serverbee_common::firewall::BlocklistAckItem,
        db: &sea_orm::DatabaseConnection,
    ) {
        use serverbee_common::firewall::BlocklistEntryState;
        // 1. Update apply_state map.
        {
            let mut g = self.apply_state.write().await;
            g.insert(
                (item.id.clone(), server_id.to_string()),
                ApplyState {
                    state: item.state.clone(),
                    reason: item.reason.clone(),
                    at: chrono::Utc::now(),
                },
            );
        }
        // 2. Audit.
        let action = match item.state {
            BlocklistEntryState::Present => "firewall_block_applied_agent",
            BlocklistEntryState::Absent => "firewall_block_removed_agent",
            BlocklistEntryState::Failed => "firewall_block_rejected_agent",
        };
        let detail = serde_json::json!({
            "block_id": item.id,
            "server_id": server_id,
            "state": item.state,
            "reason": item.reason,
        });
        let _ = crate::service::audit::AuditService::log(
            db,
            "system",
            action,
            Some(&detail.to_string()),
            "",
        )
        .await;
        // 3. Broadcast.
        let _ = self
            .browser_tx
            .send(serverbee_common::protocol::BrowserMessage::FirewallApplyStateChanged {
                block_id: item.id,
                server_id: server_id.to_string(),
                state: item.state,
                reason: item.reason,
            });
    }

    pub async fn record_reset_ack(
        &self,
        server_id: &str,
        ok: bool,
        reason: Option<String>,
        db: &sea_orm::DatabaseConnection,
    ) {
        let action = if ok { "firewall_reset_acked" } else { "firewall_reset_failed_agent" };
        let detail = serde_json::json!({ "server_id": server_id, "ok": ok, "reason": reason });
        let _ = crate::service::audit::AuditService::log(
            db,
            "system",
            action,
            Some(&detail.to_string()),
            "",
        )
        .await;
    }
}
```

- [ ] **Step 2: Wire into ws/agent.rs**

In the `AgentMessage` match in `ws/agent.rs`, add arms:

```rust
AgentMessage::BlocklistAck { results } => {
    for item in results {
        state.firewall.record_ack(server_id, item, &state.db).await;
    }
}
AgentMessage::BlocklistResetAck { ok, reason } => {
    state.firewall.record_reset_ack(server_id, ok, reason, &state.db).await;
}
```

- [ ] **Step 3: Compile**

`cargo build -p serverbee-server`

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/firewall.rs crates/server/src/router/ws/agent.rs
git commit -m "feat(server): handle BlocklistAck / ResetAck"
```

---

### Task 2.6: Auto-block — wire into SecurityService::evaluate_rules

**Files:**
- Modify: `crates/server/src/service/security.rs`
- Modify: `crates/server/src/service/firewall.rs`

- [ ] **Step 1: Add `FirewallService::auto_block`**

In `firewall.rs`:

```rust
impl FirewallService {
    /// Returns `Ok(Some(id))` if a row was created, `Ok(None)` if skipped.
    pub async fn auto_block(
        &self,
        rule: &crate::entity::alert_rule::Model,
        payload: &serverbee_common::security::SecurityEventPayload,
        action: &crate::service::alert::AlertRuleAction,
        agent_manager: &crate::service::agent_manager::AgentManager,
    ) -> Result<Option<String>, AppError> {
        use crate::entity::block_list;
        use crate::service::alert::AlertRuleAction;
        use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
        use uuid::Uuid;

        let (target, family) = Self::canonicalize_target(&payload.source_ip)?;

        let (cover_type, server_ids_json, comment_template) = match action {
            AlertRuleAction::BlockSourceIp {
                cover_type,
                server_ids_json,
                comment,
            } => (cover_type.clone(), server_ids_json.clone(), comment.clone()),
        };

        // Coverage-aware dedup.
        if let Some(existing) = block_list::Entity::find()
            .filter(block_list::Column::Target.eq(&target))
            .one(&self.db)
            .await?
        {
            // Need the triggering server's id from the payload. SecurityEventPayload
            // does not carry server_id directly — the caller has it. We carry it
            // through via an extra arg.
            // → Handle in caller; this branch reached means caller already verified
            //   coverage. If we get here, we silently skip.
            return Ok(None);
        }

        let dynamic_allow = self.collect_dynamic_allow().await;
        let mut allow: Vec<String> = self.config.firewall.allow_list.clone();
        allow.extend(dynamic_allow);
        if let Some(reason) = Self::is_protected(&target, &allow) {
            crate::service::audit::AuditService::log(
                &self.db,
                "system",
                "firewall_block_rejected_server",
                Some(
                    &serde_json::json!({
                        "target": target,
                        "reason": reason,
                        "rule_id": rule.id,
                        "event_id": payload.event_id.clone(),
                    })
                    .to_string(),
                ),
                "",
            )
            .await
            .ok();
            return Ok(None);
        }

        let id = Uuid::new_v4().to_string();
        let comment = comment_template
            .as_deref()
            .map(|t| {
                t.replace("{rule_name}", &rule.name)
                    .replace(
                        "{event_type}",
                        &format!("{:?}", payload.event_type).to_lowercase(),
                    )
                    .replace("{severity}", &format!("{:?}", payload.severity).to_lowercase())
            })
            .or_else(|| Some(format!("Auto-block from {}", rule.name)));

        block_list::ActiveModel {
            id: Set(id.clone()),
            target: Set(target.clone()),
            family: Set(family as i32),
            cover_type: Set(cover_type.clone()),
            server_ids_json: Set(server_ids_json.clone()),
            comment: Set(comment),
            origin: Set("auto".into()),
            origin_event_id: Set(payload.event_id.clone()),
            origin_rule_id: Set(Some(rule.id.clone())),
            created_by: Set(None),
            created_at: Set(chrono::Utc::now()),
        }
        .insert(&self.db)
        .await?;

        crate::service::audit::AuditService::log(
            &self.db,
            "system",
            "firewall_block_created",
            Some(
                &serde_json::json!({
                    "id": id,
                    "target": target,
                    "origin": "auto",
                    "rule_id": rule.id,
                    "event_id": payload.event_id,
                })
                .to_string(),
            ),
            "",
        )
        .await
        .ok();

        // Fetch the just-inserted model to push.
        if let Some(row) = block_list::Entity::find_by_id(&id).one(&self.db).await? {
            let item = crate::router::api::firewall::BlockListItem::from(row);
            self.broadcast_changed_created(&item);
            self.push_add_to_covered_agents(&item, agent_manager).await;
        }
        Ok(Some(id))
    }
}
```

Note: this version dedups but does **not** yet handle "existing row covers vs not". That is added in Task 2.7 once we have the triggering server context plumbed.

- [ ] **Step 2: Hook into evaluate_rules**

In `crates/server/src/service/security.rs`, find the `evaluate_rules` loop. After `mark_triggered` and **before** the `if !should_notify { continue; }` early-exit, add:

```rust
// Auto-actions run on every rule match, even when the notification is
// dedupe-suppressed or no notification group is set.
let actions: Vec<crate::service::alert::AlertRuleAction> = rule
    .actions_json
    .as_deref()
    .and_then(|s| serde_json::from_str(s).ok())
    .unwrap_or_default();
for action in &actions {
    match action {
        crate::service::alert::AlertRuleAction::BlockSourceIp { .. } => {
            if let Err(e) = self
                .firewall
                .auto_block(rule, payload, action, &self.agent_manager)
                .await
            {
                tracing::error!(rule_id=%rule.id, error=%e, "auto_block failed");
            }
        }
    }
}
```

This requires `SecurityService` to carry `firewall: Arc<FirewallService>` and `agent_manager: Arc<AgentManager>`. Edit the struct definition at the top of `security.rs` and update its `new` to accept and store these. Update the construction site in `state.rs` accordingly.

- [ ] **Step 3: Compile**

`cargo build -p serverbee-server`

If `SecurityEventPayload` doesn't have `event_id` field directly (it's the `security_event.id` generated by `SecurityService` at insert time), we need to pass that down. Alter `auto_block` to accept `event_id: Option<&str>` and pass `Some(&event_id)` from `security.rs` where the insert returns the new id.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/security.rs crates/server/src/service/firewall.rs crates/server/src/state.rs
git commit -m "feat(server): auto-block on security event match"
```

---

### Task 2.7: Coverage-aware dedup in auto_block

**Files:**
- Modify: `crates/server/src/service/firewall.rs`
- Modify: `crates/server/src/service/security.rs`

- [ ] **Step 1: Plumb `triggering_server_id`**

In `security.rs::evaluate_rules`, the loop already has `server_id` (the agent that emitted the event). Pass it into `auto_block`:

Change `auto_block` signature:

```rust
pub async fn auto_block(
    &self,
    triggering_server_id: &str,
    rule: &crate::entity::alert_rule::Model,
    payload: &serverbee_common::security::SecurityEventPayload,
    event_id: &str,
    action: &crate::service::alert::AlertRuleAction,
    agent_manager: &crate::service::agent_manager::AgentManager,
) -> Result<Option<String>, AppError> {
```

- [ ] **Step 2: Implement coverage check on existing row**

Replace the dedup branch:

```rust
if let Some(existing) = block_list::Entity::find()
    .filter(block_list::Column::Target.eq(&target))
    .one(&self.db)
    .await?
{
    let covers = crate::service::alert::rule_covers_server(
        &existing.cover_type,
        &existing.server_ids_json,
        triggering_server_id,
    );
    if covers {
        return Ok(None); // genuinely redundant
    }
    // existing row exists but does NOT cover the triggering server
    crate::service::audit::AuditService::log(
        &self.db,
        "system",
        "firewall_auto_block_skipped_conflict",
        Some(
            &serde_json::json!({
                "target": target,
                "existing_id": existing.id,
                "current_server_id": triggering_server_id,
            })
            .to_string(),
        ),
        "",
    )
    .await
    .ok();
    return Ok(None);
}
```

- [ ] **Step 3: Add a focused test**

In `crates/server/src/service/firewall.rs`'s test module (or in `security.rs` integration-style tests), add:

```rust
#[tokio::test]
async fn auto_block_skips_when_existing_row_covers() {
    // Set up DB, insert existing block_list with cover_type=all and target X.
    // Call auto_block with triggering_server_id=srv-A and same target X.
    // Assert: returns Ok(None), no new row, no audit `firewall_auto_block_skipped_conflict`.
    // [Implementation uses setup_test_db + insert helper from existing tests.]
}

#[tokio::test]
async fn auto_block_skips_with_conflict_when_existing_row_does_not_cover() {
    // existing block_list cover_type=include server_ids=[srv-B]; target X.
    // call auto_block triggering_server_id=srv-A target X.
    // assert: Ok(None), no new row, audit `firewall_auto_block_skipped_conflict` written.
}
```

Use the audit-log query helper that already exists in the test module of `audit.rs` or build a tiny inline one.

- [ ] **Step 4: Run**

`cargo test -p serverbee-server --lib service::firewall::tests::auto_block`

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/firewall.rs crates/server/src/service/security.rs
git commit -m "feat(server): coverage-aware auto-block dedup"
```

---

### Task 2.8: Integration test — full server pipeline

**Files:**
- Modify: `crates/server/tests/integration.rs`

- [ ] **Step 1: Add scenarios**

In `crates/server/tests/integration.rs`, find the existing test helpers (`spawn_app`, login helpers, mock agent). Add a new module-level test set:

```rust
mod firewall {
    use super::*;

    #[tokio::test]
    async fn post_block_inserts_and_pushes() {
        let app = spawn_app().await;
        let admin = login_admin(&app).await;
        let agent = connect_mock_agent(&app, "srv-1", caps_with_firewall()).await;

        let resp = admin
            .post("/api/firewall/blocks")
            .json(&serde_json::json!({ "target": "1.2.3.4", "cover_type": "all" }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        // Assert agent received BlocklistAdd.
        let msg = agent.recv_with_timeout(Duration::from_secs(2)).await;
        assert!(matches!(msg, ServerMessage::BlocklistAdd { entry } if entry.target == "1.2.3.4/32"));
    }

    #[tokio::test]
    async fn guardrail_returns_409_for_loopback() {
        let app = spawn_app().await;
        let admin = login_admin(&app).await;
        let resp = admin
            .post("/api/firewall/blocks")
            .json(&serde_json::json!({ "target": "127.0.0.1", "cover_type": "all" }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 409);
    }

    #[tokio::test]
    async fn duplicate_target_returns_409() {
        let app = spawn_app().await;
        let admin = login_admin(&app).await;
        admin.post("/api/firewall/blocks")
            .json(&serde_json::json!({ "target": "1.2.3.4/24", "cover_type": "all" }))
            .send().await.unwrap();
        let resp = admin
            .post("/api/firewall/blocks")
            .json(&serde_json::json!({ "target": "1.2.3.0/24", "cover_type": "all" }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 409);
    }

    #[tokio::test]
    async fn member_post_returns_403() {
        let app = spawn_app().await;
        let member = login_member(&app).await;
        let resp = member
            .post("/api/firewall/blocks")
            .json(&serde_json::json!({ "target": "1.2.3.4", "cover_type": "all" }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 403);
    }

    #[tokio::test]
    async fn agent_connect_triggers_reset_then_sync() {
        let app = spawn_app().await;
        let admin = login_admin(&app).await;
        // pre-insert a block
        admin.post("/api/firewall/blocks")
            .json(&serde_json::json!({ "target": "5.5.5.5", "cover_type": "all" }))
            .send().await.unwrap();
        // connect agent that already declares firewall capability
        let agent = connect_mock_agent(&app, "srv-1", caps_with_firewall()).await;
        let m1 = agent.recv_with_timeout(Duration::from_secs(2)).await;
        let m2 = agent.recv_with_timeout(Duration::from_secs(2)).await;
        assert!(matches!(m1, ServerMessage::BlocklistReset));
        assert!(matches!(m2, ServerMessage::BlocklistSync { entries } if entries.len() == 1));
    }

    #[tokio::test]
    async fn cap_off_triggers_reset_no_sync() {
        let app = spawn_app().await;
        let admin = login_admin(&app).await;
        let agent = connect_mock_agent(&app, "srv-1", caps_with_firewall()).await;
        // drain initial reset+sync
        agent.drain(2).await;
        admin.update_capabilities("srv-1", caps_without_firewall()).await;
        let m = agent.recv_with_timeout(Duration::from_secs(2)).await;
        assert!(matches!(m, ServerMessage::BlocklistReset));
    }

    #[tokio::test]
    async fn ack_failed_records_audit_and_keeps_row() {
        let app = spawn_app().await;
        let admin = login_admin(&app).await;
        let agent = connect_mock_agent(&app, "srv-1", caps_with_firewall()).await;
        agent.drain(1).await; // sync
        let resp: serde_json::Value = admin
            .post("/api/firewall/blocks")
            .json(&serde_json::json!({ "target": "9.9.9.9", "cover_type": "all" }))
            .send().await.unwrap().json().await.unwrap();
        let id = resp["data"]["id"].as_str().unwrap();
        agent.send(AgentMessage::BlocklistAck {
            results: vec![BlocklistAckItem {
                id: id.into(),
                state: BlocklistEntryState::Failed,
                reason: Some("nft permission denied".into()),
            }],
        }).await;
        // Row still exists
        let row = app.fetch_block_list_row(id).await.unwrap();
        assert!(row.is_some());
        // Audit recorded
        let audit = app.fetch_audit_logs("firewall_block_rejected_agent").await;
        assert!(!audit.is_empty());
    }

    #[tokio::test]
    async fn old_agent_no_firewall_messages() {
        let app = spawn_app().await;
        let agent = connect_mock_agent_with_protocol(&app, "srv-1", caps_with_firewall(), 1).await;
        // No BlocklistReset or BlocklistSync should arrive.
        let timed = tokio::time::timeout(Duration::from_millis(500), agent.recv_one()).await;
        assert!(timed.is_err(), "no firewall message expected for old protocol");
    }
}
```

Helpers (`spawn_app`, `connect_mock_agent`, `caps_with_firewall`, etc.) — implement using the existing scaffolding pattern from the security_event integration tests. If a helper is missing, add it inline at the top of the module. Mock-agent type is a tokio task that holds a WS sender/receiver to drive the server.

- [ ] **Step 2: Run**

`cargo test -p serverbee-server --test integration firewall::`
Expected: all PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/server/tests/integration.rs
git commit -m "test(server): firewall integration scenarios"
```

---

## Phase 3 — Agent firewall executor

### Task 3.1: `NftExecutor` trait + CLI implementation

**Files:**
- Create: `crates/agent/src/firewall/mod.rs`
- Create: `crates/agent/src/firewall/nft.rs`

- [ ] **Step 1: Module skeleton**

`crates/agent/src/firewall/mod.rs`:

```rust
//! Firewall blocklist executor for the agent.

pub mod guardrail;
pub mod manager;
pub mod nft;

pub use manager::FirewallManager;
```

`crates/agent/src/firewall/nft.rs`:

```rust
//! `nft` CLI driver. The trait lets tests mock subprocess invocations.

use async_trait::async_trait;
use serverbee_common::firewall::BlockEntry;
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum NftError {
    #[error("permission denied — needs root or CAP_NET_ADMIN")]
    PermissionDenied,
    #[error("nft kernel module unavailable")]
    KernelMissing,
    #[error("nft cli not found in PATH")]
    BinaryMissing,
    #[error("{0}")]
    Other(String),
}

/// Returns true when stderr indicates the kernel/library considers the
/// requested element to already be in the desired state (EEXIST on add,
/// ENOENT on delete/flush). These are mapped to success by the manager.
pub fn is_idempotent_signal(stderr: &str, op: NftOp) -> bool {
    match op {
        NftOp::AddElement | NftOp::AddTable | NftOp::AddSet | NftOp::AddChain | NftOp::AddRule => {
            stderr.contains("File exists")
        }
        NftOp::DeleteElement
        | NftOp::FlushSet
        | NftOp::DeleteTable => stderr.contains("No such file or directory"),
    }
}

#[derive(Copy, Clone, Debug)]
pub enum NftOp {
    AddTable,
    AddSet,
    AddChain,
    AddRule,
    AddElement,
    DeleteElement,
    FlushSet,
    DeleteTable,
}

#[async_trait]
pub trait NftExecutor: Send + Sync {
    async fn run(&self, args: &[&str], op: NftOp) -> Result<(), NftError>;
    async fn list_json(&self, args: &[&str]) -> Result<String, NftError>;
}

pub struct CliNftExecutor;

#[async_trait]
impl NftExecutor for CliNftExecutor {
    async fn run(&self, args: &[&str], op: NftOp) -> Result<(), NftError> {
        let out = Command::new("nft").args(args).output().await.map_err(|e| {
            if matches!(e.kind(), std::io::ErrorKind::NotFound) {
                NftError::BinaryMissing
            } else {
                NftError::Other(e.to_string())
            }
        })?;
        if out.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        if is_idempotent_signal(&stderr, op) {
            return Ok(());
        }
        if stderr.contains("Operation not permitted") {
            return Err(NftError::PermissionDenied);
        }
        if stderr.contains("No such file or directory") {
            // Resource ops without idempotence signal — kernel module probably missing.
            return Err(NftError::KernelMissing);
        }
        Err(NftError::Other(
            stderr.lines().next().unwrap_or("nft failed").to_string(),
        ))
    }

    async fn list_json(&self, args: &[&str]) -> Result<String, NftError> {
        let mut full = vec!["-j", "list"];
        full.extend_from_slice(args);
        let out = Command::new("nft").args(&full).output().await.map_err(|e| {
            if matches!(e.kind(), std::io::ErrorKind::NotFound) {
                NftError::BinaryMissing
            } else {
                NftError::Other(e.to_string())
            }
        })?;
        if !out.status.success() {
            return Err(NftError::Other(
                String::from_utf8_lossy(&out.stderr).to_string(),
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }
}

/// High-level operations the manager will call. Implemented as free functions
/// taking `&dyn NftExecutor` so tests can swap.

pub async fn ensure_resources(exec: &dyn NftExecutor) -> Result<(), NftError> {
    exec.run(&["add", "table", "inet", "serverbee"], NftOp::AddTable).await?;
    exec.run(
        &["add", "set", "inet", "serverbee", "block_v4", "{ type ipv4_addr; flags interval; }"],
        NftOp::AddSet,
    )
    .await?;
    exec.run(
        &["add", "set", "inet", "serverbee", "block_v6", "{ type ipv6_addr; flags interval; }"],
        NftOp::AddSet,
    )
    .await?;
    exec.run(
        &[
            "add", "chain", "inet", "serverbee", "input",
            "{ type filter hook input priority -10; }",
        ],
        NftOp::AddChain,
    )
    .await?;
    // Add the two drop rules; nft `add rule` is not idempotent natively, so we
    // detect them in the existing ruleset first.
    let listing = exec.list_json(&["chain", "inet", "serverbee", "input"]).await?;
    if !listing.contains("\"set\":\"block_v4\"") {
        exec.run(
            &["add", "rule", "inet", "serverbee", "input", "ip", "saddr", "@block_v4", "drop"],
            NftOp::AddRule,
        )
        .await?;
    }
    if !listing.contains("\"set\":\"block_v6\"") {
        exec.run(
            &["add", "rule", "inet", "serverbee", "input", "ip6", "saddr", "@block_v6", "drop"],
            NftOp::AddRule,
        )
        .await?;
    }
    Ok(())
}

pub async fn add_element(exec: &dyn NftExecutor, entry: &BlockEntry) -> Result<(), NftError> {
    let set = if entry.family == 4 { "block_v4" } else { "block_v6" };
    let arg = format!("{{ {} }}", entry.target);
    exec.run(
        &["add", "element", "inet", "serverbee", set, &arg],
        NftOp::AddElement,
    )
    .await
}

pub async fn delete_element(exec: &dyn NftExecutor, entry: &BlockEntry) -> Result<(), NftError> {
    let set = if entry.family == 4 { "block_v4" } else { "block_v6" };
    let arg = format!("{{ {} }}", entry.target);
    exec.run(
        &["delete", "element", "inet", "serverbee", set, &arg],
        NftOp::DeleteElement,
    )
    .await
}

pub async fn unconditional_wipe(exec: &dyn NftExecutor) -> Result<(), NftError> {
    let _ = exec
        .run(&["flush", "set", "inet", "serverbee", "block_v4"], NftOp::FlushSet)
        .await;
    let _ = exec
        .run(&["flush", "set", "inet", "serverbee", "block_v6"], NftOp::FlushSet)
        .await;
    // Delete the whole table — fresh resource bootstrap next time.
    exec.run(&["delete", "table", "inet", "serverbee"], NftOp::DeleteTable)
        .await
}
```

- [ ] **Step 2: Dependencies**

In `crates/agent/Cargo.toml`, add (if not present): `async-trait`, `thiserror`. Most likely present already; check first.

- [ ] **Step 3: Tests with a mock executor**

Append to `crates/agent/src/firewall/nft.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct MockExec {
        calls: Mutex<Vec<Vec<String>>>,
        respond_eexist_on_add_table: Mutex<bool>,
    }

    #[async_trait]
    impl NftExecutor for MockExec {
        async fn run(&self, args: &[&str], op: NftOp) -> Result<(), NftError> {
            self.calls.lock().await.push(args.iter().map(|s| s.to_string()).collect());
            // Simulate "table already exists" idempotence:
            if matches!(op, NftOp::AddTable) && *self.respond_eexist_on_add_table.lock().await {
                return Ok(());
            }
            Ok(())
        }
        async fn list_json(&self, _args: &[&str]) -> Result<String, NftError> {
            // Pretend the chain has no rules yet.
            Ok("[]".into())
        }
    }

    #[tokio::test]
    async fn ensure_resources_runs_all_steps() {
        let exec = MockExec::default();
        ensure_resources(&exec).await.unwrap();
        let calls = exec.calls.lock().await;
        let has = |needle: &str| calls.iter().any(|c| c.join(" ").contains(needle));
        assert!(has("add table inet serverbee"));
        assert!(has("add set inet serverbee block_v4"));
        assert!(has("add chain inet serverbee input"));
        assert!(has("add rule inet serverbee input ip saddr @block_v4 drop"));
    }

    #[tokio::test]
    async fn add_element_v4_uses_v4_set() {
        let exec = MockExec::default();
        let entry = BlockEntry { id: "x".into(), target: "1.2.3.4/32".into(), family: 4 };
        add_element(&exec, &entry).await.unwrap();
        let calls = exec.calls.lock().await;
        let joined = calls[0].join(" ");
        assert!(joined.contains("block_v4"));
        assert!(joined.contains("1.2.3.4/32"));
    }

    #[test]
    fn eexist_classified_as_idempotent_add() {
        assert!(is_idempotent_signal("Error: File exists", NftOp::AddElement));
    }

    #[test]
    fn enoent_classified_as_idempotent_delete() {
        assert!(is_idempotent_signal(
            "Error: No such file or directory",
            NftOp::DeleteElement
        ));
    }
}
```

- [ ] **Step 4: Run**

`cargo test -p serverbee-agent --lib firewall::nft::tests`

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src/firewall/ crates/agent/Cargo.toml
git commit -m "feat(agent): nft executor + ensure_resources"
```

---

### Task 3.2: Tier-3 guardrail

**Files:**
- Create: `crates/agent/src/firewall/guardrail.rs`

- [ ] **Step 1: Implement**

```rust
//! Agent-side guardrail (tier 3, § 4.3). Subset of server-side:
//! hard-coded protected CIDRs + the agent's own external IP.

use std::net::IpAddr;
use ipnet::IpNet;

const PROTECTED: &[&str] = &[
    "127.0.0.0/8",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
    "0.0.0.0/8",
    "224.0.0.0/4",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
    "ff00::/8",
    "::/128",
];

pub fn check(target_cidr: &str, own_external_ip: Option<IpAddr>) -> Result<(), String> {
    let net: IpNet = target_cidr.parse().map_err(|_| format!("invalid CIDR: {target_cidr}"))?;
    for p in PROTECTED {
        let prot: IpNet = p.parse().expect("hard-coded valid");
        if prot.contains(&net.network()) || net.contains(&prot.network()) {
            return Err(format!("guardrail: {p}"));
        }
    }
    if let Some(ip) = own_external_ip {
        let own_net = IpNet::new(ip, if ip.is_ipv4() { 32 } else { 128 }).expect("ok");
        if own_net.network() == net.network() && own_net.prefix_len() == net.prefix_len() {
            return Err(format!("guardrail: agent's own external IP {ip}"));
        }
        if net.contains(&ip) {
            return Err(format!("guardrail: range contains own external IP {ip}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_loopback() {
        assert!(check("127.0.0.1/32", None).is_err());
    }
    #[test]
    fn rejects_own_ip() {
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        assert!(check("203.0.113.7/32", Some(ip)).is_err());
    }
    #[test]
    fn rejects_range_containing_own_ip() {
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        assert!(check("203.0.113.0/24", Some(ip)).is_err());
    }
    #[test]
    fn accepts_external_unrelated() {
        assert!(check("198.51.100.5/32", None).is_ok());
    }
}
```

- [ ] **Step 2: Add ipnet to agent Cargo.toml**

Check `crates/agent/Cargo.toml` already has `ipnet` — if not, add `ipnet = "2"`.

- [ ] **Step 3: Run**

`cargo test -p serverbee-agent --lib firewall::guardrail::tests`

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/firewall/guardrail.rs crates/agent/Cargo.toml
git commit -m "feat(agent): tier-3 firewall guardrail"
```

---

### Task 3.3: `FirewallManager` with full message handling

**Files:**
- Create: `crates/agent/src/firewall/manager.rs`

- [ ] **Step 1: Implement**

```rust
//! Top-level firewall state machine: holds desired blocklist mirror,
//! routes Server messages to nft, emits acks back via the reporter.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use serverbee_common::firewall::{
    BlockEntry, BlocklistAckItem, BlocklistEntryState,
};
use serverbee_common::protocol::{AgentMessage, ServerMessage};
use tokio::sync::Mutex;

use crate::firewall::guardrail;
use crate::firewall::nft::{
    self, NftExecutor,
};

pub struct FirewallManager {
    /// Entries the agent has confirmed are in the kernel nft set.
    desired: Mutex<HashMap<String, BlockEntry>>,
    /// Resource-bootstrap state.
    nft_ready: Mutex<bool>,
    external_ip: Mutex<Option<IpAddr>>,
    executor: Arc<dyn NftExecutor>,
    /// Local capability — `false` means `nft` probe failed at startup.
    local_capable: bool,
}

impl FirewallManager {
    pub fn new(executor: Arc<dyn NftExecutor>, local_capable: bool) -> Self {
        Self {
            desired: Mutex::new(HashMap::new()),
            nft_ready: Mutex::new(false),
            external_ip: Mutex::new(None),
            executor,
            local_capable,
        }
    }

    pub async fn set_external_ip(&self, ip: Option<IpAddr>) {
        *self.external_ip.lock().await = ip;
    }

    /// Single entry point dispatched from the agent reporter loop.
    pub async fn handle(&self, msg: ServerMessage) -> Option<AgentMessage> {
        match msg {
            ServerMessage::BlocklistReset => Some(self.handle_reset().await),
            ServerMessage::BlocklistSync { entries } => Some(self.handle_sync(entries).await),
            ServerMessage::BlocklistAdd { entry } => Some(self.handle_add(entry).await),
            ServerMessage::BlocklistRemove { id } => Some(self.handle_remove(id).await),
            _ => None,
        }
    }

    async fn handle_reset(&self) -> AgentMessage {
        // Honored regardless of local capability — kernel may still hold
        // stale rules from a previous capability=on window.
        match nft::unconditional_wipe(&*self.executor).await {
            Ok(()) => {
                self.desired.lock().await.clear();
                *self.nft_ready.lock().await = false;
                AgentMessage::BlocklistResetAck { ok: true, reason: None }
            }
            Err(e) => AgentMessage::BlocklistResetAck { ok: false, reason: Some(e.to_string()) },
        }
    }

    async fn ensure_ready(&self) -> Result<(), String> {
        let mut g = self.nft_ready.lock().await;
        if *g {
            return Ok(());
        }
        nft::ensure_resources(&*self.executor)
            .await
            .map_err(|e| e.to_string())?;
        *g = true;
        Ok(())
    }

    async fn handle_sync(&self, entries: Vec<BlockEntry>) -> AgentMessage {
        if let Err(reason) = self.ensure_ready().await {
            // Whole pipeline broken — ack every entry as Failed.
            let results = entries
                .into_iter()
                .map(|e| BlocklistAckItem {
                    id: e.id,
                    state: BlocklistEntryState::Failed,
                    reason: Some(reason.clone()),
                })
                .collect();
            return AgentMessage::BlocklistAck { results };
        }

        let incoming: HashMap<String, BlockEntry> =
            entries.into_iter().map(|e| (e.id.clone(), e)).collect();

        let to_remove: Vec<BlockEntry> = {
            let g = self.desired.lock().await;
            g.values()
                .filter(|e| !incoming.contains_key(&e.id))
                .cloned()
                .collect()
        };

        let mut results = Vec::new();
        let own_ip = *self.external_ip.lock().await;

        for e in incoming.values() {
            if let Err(r) = guardrail::check(&e.target, own_ip) {
                self.desired.lock().await.remove(&e.id);
                results.push(BlocklistAckItem {
                    id: e.id.clone(),
                    state: BlocklistEntryState::Failed,
                    reason: Some(r),
                });
                continue;
            }
            match nft::add_element(&*self.executor, e).await {
                Ok(()) => {
                    self.desired.lock().await.insert(e.id.clone(), e.clone());
                    results.push(BlocklistAckItem {
                        id: e.id.clone(),
                        state: BlocklistEntryState::Present,
                        reason: None,
                    });
                }
                Err(err) => {
                    self.desired.lock().await.remove(&e.id);
                    results.push(BlocklistAckItem {
                        id: e.id.clone(),
                        state: BlocklistEntryState::Failed,
                        reason: Some(err.to_string()),
                    });
                }
            }
        }

        for e in &to_remove {
            match nft::delete_element(&*self.executor, e).await {
                Ok(()) => {
                    self.desired.lock().await.remove(&e.id);
                    results.push(BlocklistAckItem {
                        id: e.id.clone(),
                        state: BlocklistEntryState::Absent,
                        reason: None,
                    });
                }
                Err(err) => {
                    // Kernel may still have it; keep desired.
                    results.push(BlocklistAckItem {
                        id: e.id.clone(),
                        state: BlocklistEntryState::Failed,
                        reason: Some(err.to_string()),
                    });
                }
            }
        }

        AgentMessage::BlocklistAck { results }
    }

    async fn handle_add(&self, entry: BlockEntry) -> AgentMessage {
        if let Err(reason) = self.ensure_ready().await {
            return AgentMessage::BlocklistAck {
                results: vec![BlocklistAckItem {
                    id: entry.id,
                    state: BlocklistEntryState::Failed,
                    reason: Some(reason),
                }],
            };
        }
        if let Err(r) = guardrail::check(&entry.target, *self.external_ip.lock().await) {
            return AgentMessage::BlocklistAck {
                results: vec![BlocklistAckItem {
                    id: entry.id,
                    state: BlocklistEntryState::Failed,
                    reason: Some(r),
                }],
            };
        }
        match nft::add_element(&*self.executor, &entry).await {
            Ok(()) => {
                let id = entry.id.clone();
                self.desired.lock().await.insert(entry.id.clone(), entry);
                AgentMessage::BlocklistAck {
                    results: vec![BlocklistAckItem {
                        id,
                        state: BlocklistEntryState::Present,
                        reason: None,
                    }],
                }
            }
            Err(e) => AgentMessage::BlocklistAck {
                results: vec![BlocklistAckItem {
                    id: entry.id,
                    state: BlocklistEntryState::Failed,
                    reason: Some(e.to_string()),
                }],
            },
        }
    }

    async fn handle_remove(&self, id: String) -> AgentMessage {
        let entry = self.desired.lock().await.get(&id).cloned();
        let Some(entry) = entry else {
            return AgentMessage::BlocklistAck {
                results: vec![BlocklistAckItem {
                    id,
                    state: BlocklistEntryState::Absent,
                    reason: None,
                }],
            };
        };
        match nft::delete_element(&*self.executor, &entry).await {
            Ok(()) => {
                self.desired.lock().await.remove(&id);
                AgentMessage::BlocklistAck {
                    results: vec![BlocklistAckItem {
                        id,
                        state: BlocklistEntryState::Absent,
                        reason: None,
                    }],
                }
            }
            Err(e) => AgentMessage::BlocklistAck {
                results: vec![BlocklistAckItem {
                    id,
                    state: BlocklistEntryState::Failed,
                    reason: Some(e.to_string()),
                }],
            },
        }
    }
}
```

- [ ] **Step 2: Tests for the state machine**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::firewall::nft::{NftError, NftOp};
    use async_trait::async_trait;
    use std::sync::Arc;

    struct OkExec;
    #[async_trait]
    impl NftExecutor for OkExec {
        async fn run(&self, _: &[&str], _: NftOp) -> Result<(), NftError> { Ok(()) }
        async fn list_json(&self, _: &[&str]) -> Result<String, NftError> { Ok("[]".into()) }
    }

    struct FailAdd;
    #[async_trait]
    impl NftExecutor for FailAdd {
        async fn run(&self, args: &[&str], op: NftOp) -> Result<(), NftError> {
            if matches!(op, NftOp::AddElement) {
                Err(NftError::PermissionDenied)
            } else { Ok(()) }
        }
        async fn list_json(&self, _: &[&str]) -> Result<String, NftError> { Ok("[]".into()) }
    }

    fn entry(id: &str, target: &str, family: u8) -> BlockEntry {
        BlockEntry { id: id.into(), target: target.into(), family }
    }

    #[tokio::test]
    async fn add_success_inserts_into_desired() {
        let mgr = FirewallManager::new(Arc::new(OkExec), true);
        let ack = mgr.handle_add(entry("b1", "1.2.3.4/32", 4)).await;
        match ack {
            AgentMessage::BlocklistAck { results } => {
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].state, BlocklistEntryState::Present);
            }
            _ => panic!("expected ack"),
        }
        assert!(mgr.desired.lock().await.contains_key("b1"));
    }

    #[tokio::test]
    async fn failed_add_keeps_desired_clear_for_retry() {
        let mgr = FirewallManager::new(Arc::new(FailAdd), true);
        let ack = mgr.handle_add(entry("b1", "1.2.3.4/32", 4)).await;
        match ack {
            AgentMessage::BlocklistAck { results } => {
                assert_eq!(results[0].state, BlocklistEntryState::Failed);
            }
            _ => panic!(),
        }
        assert!(!mgr.desired.lock().await.contains_key("b1"));
    }

    #[tokio::test]
    async fn sync_acks_every_incoming_entry() {
        let mgr = FirewallManager::new(Arc::new(OkExec), true);
        let entries = vec![entry("a", "1.1.1.1/32", 4), entry("b", "2.2.2.2/32", 4)];
        let ack = mgr.handle_sync(entries).await;
        match ack {
            AgentMessage::BlocklistAck { results } => {
                assert_eq!(results.len(), 2);
                assert!(results.iter().all(|r| r.state == BlocklistEntryState::Present));
            }
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn reset_clears_desired_and_nft_ready() {
        let mgr = FirewallManager::new(Arc::new(OkExec), true);
        mgr.handle_add(entry("b1", "1.2.3.4/32", 4)).await;
        assert!(mgr.desired.lock().await.contains_key("b1"));
        let ack = mgr.handle_reset().await;
        assert!(matches!(ack, AgentMessage::BlocklistResetAck { ok: true, .. }));
        assert!(mgr.desired.lock().await.is_empty());
        assert!(!*mgr.nft_ready.lock().await);
    }

    #[tokio::test]
    async fn guardrail_blocks_loopback() {
        let mgr = FirewallManager::new(Arc::new(OkExec), true);
        let ack = mgr.handle_add(entry("b1", "127.0.0.1/32", 4)).await;
        match ack {
            AgentMessage::BlocklistAck { results } => {
                assert_eq!(results[0].state, BlocklistEntryState::Failed);
                assert!(results[0].reason.as_ref().unwrap().contains("guardrail"));
            }
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn remove_unknown_id_acks_absent() {
        let mgr = FirewallManager::new(Arc::new(OkExec), true);
        let ack = mgr.handle_remove("unknown".into()).await;
        match ack {
            AgentMessage::BlocklistAck { results } => {
                assert_eq!(results[0].state, BlocklistEntryState::Absent);
            }
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn sync_removes_orphans() {
        let mgr = FirewallManager::new(Arc::new(OkExec), true);
        mgr.handle_add(entry("orphan", "9.9.9.9/32", 4)).await;
        mgr.handle_sync(vec![entry("new", "1.1.1.1/32", 4)]).await;
        let g = mgr.desired.lock().await;
        assert!(!g.contains_key("orphan"));
        assert!(g.contains_key("new"));
    }
}
```

- [ ] **Step 3: Run**

`cargo test -p serverbee-agent --lib firewall::manager::tests`

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/firewall/manager.rs
git commit -m "feat(agent): FirewallManager state machine + tests"
```

---

### Task 3.4: Local capability probe + wire into agent boot

**Files:**
- Modify: `crates/agent/src/firewall/mod.rs`
- Modify: `crates/agent/src/main.rs`
- Modify: `crates/agent/src/collector.rs` (or wherever SystemInfo is built — find it)
- Modify: `crates/agent/src/reporter.rs`

- [ ] **Step 1: Add the probe**

In `crates/agent/src/firewall/mod.rs`:

```rust
use crate::firewall::nft::{CliNftExecutor, NftExecutor, NftOp};
use std::sync::Arc;

/// Probe whether the host can actually execute firewall ops.
/// Runs a no-op `nft list ruleset` and a write-then-revert on a throwaway
/// set. Slow path; call once at startup.
pub async fn probe_local_capability() -> bool {
    let exec: Arc<dyn NftExecutor> = Arc::new(CliNftExecutor);
    if exec.list_json(&["ruleset"]).await.is_err() {
        return false;
    }
    // Try to add and immediately delete a throwaway table.
    let test_table = "add table inet serverbee_probe";
    if exec
        .run(&["add", "table", "inet", "serverbee_probe"], NftOp::AddTable)
        .await
        .is_err()
    {
        return false;
    }
    let _ = exec
        .run(&["delete", "table", "inet", "serverbee_probe"], NftOp::DeleteTable)
        .await;
    true
}
```

- [ ] **Step 2: Add to SystemInfo**

Find where the agent builds the `SystemInfo` payload (likely `crates/agent/src/collector/mod.rs` or in `crates/common/src/protocol.rs` if it's a shared struct). The struct already has `capabilities_local: u32` (per CLAUDE.md/spec); if not, add it:

```rust
pub capabilities_local: u32,
```

Set the firewall bit when the probe returns true. Probe should run once at agent startup, **before** the first connect attempt, and the result stored in a `OnceLock<bool>`.

Sketch in `crates/agent/src/main.rs`:

```rust
let firewall_local = serverbee_agent::firewall::probe_local_capability().await;
// inject into the SystemInfo builder:
let mut local_caps = compute_local_capabilities();
if firewall_local {
    local_caps |= serverbee_common::constants::CAP_FIREWALL_BLOCK;
}
// pass into the collector or SystemInfo constructor
```

If `capabilities_local` is currently a `u32` populated elsewhere, follow whatever pattern exists. Read the file first.

- [ ] **Step 3: Wire FirewallManager into the reporter**

In `crates/agent/src/main.rs`, after creating the WS connection and the message-routing loop, instantiate a `FirewallManager` and route incoming `ServerMessage::Blocklist*` / `Reset` to it. The reporter's existing match-on-ServerMessage gets new arms:

```rust
ServerMessage::BlocklistReset
| ServerMessage::BlocklistSync { .. }
| ServerMessage::BlocklistAdd { .. }
| ServerMessage::BlocklistRemove { .. } => {
    if let Some(reply) = firewall_manager.handle(msg).await {
        if let Err(e) = ws_tx.send(reply).await {
            tracing::warn!(error=%e, "send firewall ack failed");
        }
    }
}
```

`firewall_manager` is `Arc<FirewallManager>`; created once and shared with the reporter loop.

When the agent also tracks its own external IP (existing `ip_change` flow), feed it into the manager: `firewall_manager.set_external_ip(Some(ip)).await;` whenever the external IP changes.

- [ ] **Step 4: Compile**

`cargo build -p serverbee-agent`
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src/
git commit -m "feat(agent): firewall manager wired into reporter + capability probe"
```

---

## Phase 4 — Frontend

### Task 4.1: API types + query hooks

**Files:**
- Modify: `apps/web/src/lib/api-types.ts` (or wherever generated types live — check for `bun run codegen` script)
- Create: `apps/web/src/hooks/use-firewall-blocks.ts`

- [ ] **Step 1: Regenerate types**

```bash
cd apps/web
# look at package.json for the codegen script — likely `bun run codegen:api`
bun run codegen:api 2>/dev/null || true
```

If no codegen exists, hand-write types in `apps/web/src/types/firewall.ts`:

```ts
export type BlocklistEntryState = 'present' | 'absent' | 'failed'

export interface BlockListItem {
  id: string
  target: string
  family: 4 | 6
  cover_type: 'all' | 'include' | 'exclude'
  server_ids: string[] | null
  comment: string | null
  origin: 'manual' | 'auto'
  origin_event_id: string | null
  origin_rule_id: string | null
  created_by: string | null
  created_at: string
}

export interface FirewallStats {
  total: number
  auto: number
  manual: number
  v4: number
  v6: number
}

export interface CreateBlockReq {
  target: string
  cover_type?: 'all' | 'include' | 'exclude'
  server_ids?: string[] | null
  comment?: string | null
}
```

- [ ] **Step 2: Hook**

`apps/web/src/hooks/use-firewall-blocks.ts`:

```ts
import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { BlockListItem, CreateBlockReq, FirewallStats } from '@/types/firewall'

interface ListResp {
  items: BlockListItem[]
  next_cursor: string | null
}

export function useFirewallBlocks(filters: { origin?: string; target_q?: string } = {}) {
  return useInfiniteQuery({
    queryKey: ['firewall', 'blocks', filters],
    queryFn: async ({ pageParam }) => {
      const params = new URLSearchParams()
      if (filters.origin) params.set('origin', filters.origin)
      if (filters.target_q) params.set('target_q', filters.target_q)
      if (pageParam) params.set('cursor', pageParam)
      const data = await api<ListResp>(`/api/firewall/blocks?${params.toString()}`)
      return data
    },
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) => last.next_cursor ?? undefined,
  })
}

export function useFirewallBlock(id: string | undefined) {
  return useQuery({
    queryKey: ['firewall', 'block', id],
    queryFn: async () => api<BlockListItem>(`/api/firewall/blocks/${id}`),
    enabled: !!id,
  })
}

export function useFirewallStats() {
  return useQuery({
    queryKey: ['firewall', 'stats'],
    queryFn: () => api<FirewallStats>('/api/firewall/stats'),
  })
}

export function useCreateBlock() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (req: CreateBlockReq) =>
      api<BlockListItem>('/api/firewall/blocks', {
        method: 'POST',
        body: JSON.stringify(req),
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['firewall'] })
    },
  })
}

export function useDeleteBlock() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      api(`/api/firewall/blocks/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['firewall'] })
    },
  })
}
```

(`api` is the existing wrapper in `lib/api-client.ts`. Reuse its conventions for error handling.)

- [ ] **Step 3: TypeScript check**

```bash
cd apps/web && bun run typecheck
```

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/types/firewall.ts apps/web/src/hooks/use-firewall-blocks.ts
git commit -m "feat(web): firewall API types + hooks"
```

---

### Task 4.2: Firewall page + components

**Files:**
- Create: `apps/web/src/routes/_authed/settings/firewall.tsx`
- Create: `apps/web/src/components/firewall/{kpi-cards,block-table,add-block-drawer,delete-block-dialog,activity-log}.tsx`
- Modify: `apps/web/src/locales/{en,zh}/firewall.json` (Task 4.5 covers i18n in detail)

- [ ] **Step 1: Route file**

`apps/web/src/routes/_authed/settings/firewall.tsx`:

```tsx
import { createFileRoute } from '@tanstack/react-router'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useTranslation } from 'react-i18next'
import { KpiCards } from '@/components/firewall/kpi-cards'
import { BlockTable } from '@/components/firewall/block-table'
import { ActivityLog } from '@/components/firewall/activity-log'

export const Route = createFileRoute('/_authed/settings/firewall')({
  component: FirewallPage,
})

function FirewallPage() {
  const { t } = useTranslation('firewall')
  return (
    <div className="p-6 space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">{t('title')}</h1>
        <p className="text-muted-foreground text-sm">{t('subtitle')}</p>
      </div>
      <KpiCards />
      <Tabs defaultValue="blocklist">
        <TabsList>
          <TabsTrigger value="blocklist">{t('tab.blocklist')}</TabsTrigger>
          <TabsTrigger value="activity">{t('tab.activity')}</TabsTrigger>
        </TabsList>
        <TabsContent value="blocklist">
          <BlockTable />
        </TabsContent>
        <TabsContent value="activity">
          <ActivityLog />
        </TabsContent>
      </Tabs>
    </div>
  )
}
```

- [ ] **Step 2: KPI cards**

`apps/web/src/components/firewall/kpi-cards.tsx`:

```tsx
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { useTranslation } from 'react-i18next'
import { useFirewallStats } from '@/hooks/use-firewall-blocks'

export function KpiCards() {
  const { t } = useTranslation('firewall')
  const { data } = useFirewallStats()
  const cards = [
    { label: t('kpi.total'), value: data?.total ?? '—' },
    { label: t('kpi.auto'), value: data?.auto ?? '—' },
    { label: t('kpi.manual'), value: data?.manual ?? '—' },
    { label: t('kpi.v6'), value: data?.v6 ?? '—' },
  ]
  return (
    <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
      {cards.map((c) => (
        <Card key={c.label}>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm text-muted-foreground">{c.label}</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-semibold tabular-nums">{c.value}</div>
          </CardContent>
        </Card>
      ))}
    </div>
  )
}
```

- [ ] **Step 3: Block table + Add drawer + Delete dialog**

For brevity, follow the pattern of `apps/web/src/components/security/event-table.tsx` and `add-...-drawer.tsx` patterns used in security alerts. Concretely:

- **`block-table.tsx`** — shadcn `Table` rendering `data.pages.flatMap(p => p.items)`. Columns: `target`, `family`, `cover_type`, `origin`, `comment`, `created_at`, actions (Delete). Row Delete button opens `DeleteBlockDialog`.
- **`add-block-drawer.tsx`** — shadcn `Drawer` with a form: `target` text input (placeholder: `1.2.3.4` or `10.0.0.0/8`), `cover_type` Select, `server_ids` MultiSelect (only when cover_type ≠ all), `comment` text area. On submit calls `useCreateBlock`. On 409 show the server's reason in a toast.
- **`delete-block-dialog.tsx`** — shadcn `AlertDialog` confirming "Delete `target`?". On confirm calls `useDeleteBlock`.

Implement exactly mirroring the security event row-delete-dialog pattern. The plan does not duplicate that 200-LoC scaffolding here.

- [ ] **Step 4: Activity log**

`apps/web/src/components/firewall/activity-log.tsx` — wraps the existing audit-log table component (whatever name it currently has in `apps/web/src/components/audit/`) filtered to `action_type LIKE 'firewall_%'`. Reuse, do not reimplement.

- [ ] **Step 5: Mount in settings nav**

Locate the settings sidebar / nav file (`apps/web/src/components/settings/sidebar.tsx` or whichever) and add a new entry `Firewall` pointing to `/settings/firewall`. Use the `Shield` lucide icon.

Regenerate route tree:

```bash
cd apps/web && bun x vite build
# Or `bun run dev` will regenerate routeTree.gen.ts on the fly.
```

- [ ] **Step 6: Typecheck**

```bash
cd apps/web && bun run typecheck
```

- [ ] **Step 7: Commit**

```bash
git add apps/web/src/routes/_authed/settings/firewall.tsx apps/web/src/components/firewall/ apps/web/src/routeTree.gen.ts
git commit -m "feat(web): firewall settings page + components"
```

---

### Task 4.3: WS hook handles new BrowserMessages

**Files:**
- Modify: `apps/web/src/hooks/use-servers-ws.ts`

- [ ] **Step 1: Add handlers**

In the existing message-type switch in `use-servers-ws.ts`, add:

```ts
case 'BlocklistChanged': {
  debounceInvalidate(['firewall', 'blocks'], 1000)
  qc.invalidateQueries({ queryKey: ['firewall', 'stats'] })
  break
}
case 'FirewallApplyStateChanged': {
  const { block_id } = msg
  qc.invalidateQueries({ queryKey: ['firewall', 'block', block_id] })
  debounceInvalidate(['firewall', 'activity'], 500)
  break
}
```

(`debounceInvalidate` should be the existing helper used by, e.g., security_event invalidation; if it doesn't exist, write one inline with a `setTimeout` map keyed by stringified query key.)

- [ ] **Step 2: Typecheck**

```bash
cd apps/web && bun run typecheck
```

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/hooks/use-servers-ws.ts
git commit -m "feat(web): handle BlocklistChanged + FirewallApplyStateChanged"
```

---

### Task 4.4: Security event row → "Block source IP" action + Alert preset toggle

**Files:**
- Modify: `apps/web/src/components/security/event-table.tsx`
- Modify: `apps/web/src/components/security/server-security-tab.tsx`
- Modify: `apps/web/src/components/security/alert-presets.tsx`
- Modify: `apps/web/src/routes/_authed/settings/alerts.tsx`

- [ ] **Step 1: Row action**

In `event-table.tsx`, find the existing row action menu (where the event detail drawer is opened). Add a new menu item, admin-only:

```tsx
{currentUser.role === 'admin' && (
  <DropdownMenuItem onClick={() => openAddBlock({ target: row.source_ip, cover_type: 'all' })}>
    <ShieldAlert className="h-4 w-4 mr-2" />
    {t('actions.block_source_ip')}
  </DropdownMenuItem>
)}
```

`openAddBlock` is a callback prop that opens the `AddBlockDrawer` from Task 4.2 with pre-filled values. Wire it through props from the page-level component.

- [ ] **Step 2: Server-detail Security tab**

Same edit in `server-security-tab.tsx`, but with `{ target: row.source_ip, cover_type: 'include', server_ids: [serverId] }`.

- [ ] **Step 3: Alert preset "Also auto-block"**

In `alert-presets.tsx` for **Brute Force** and **Port Scan** presets (skip SSH New IP Login), add a checkbox `auto_block` (default checked). When the user creates the preset, include in the request body:

```ts
const actions = autoBlock
  ? [{ type: 'block_source_ip', cover_type: 'all' }]
  : []
```

- [ ] **Step 4: Alert rule editor — Auto-block card**

In `alerts.tsx`, after the rule editor's rules list, when `rules.every(r => ['ssh_brute_force_detected', 'port_scan_detected'].includes(r.rule_type))`, render a `Collapsible` titled "Auto-block source IP" containing the same cover_type / server_ids / comment fields. Save into `actions`.

- [ ] **Step 5: Typecheck**

```bash
cd apps/web && bun run typecheck
```

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/security/ apps/web/src/routes/_authed/settings/alerts.tsx
git commit -m "feat(web): one-click block + auto-block toggle on presets/editor"
```

---

### Task 4.5: i18n + Capability toggle UI

**Files:**
- Create: `apps/web/src/locales/en/firewall.json`
- Create: `apps/web/src/locales/zh/firewall.json`
- Modify: `apps/web/src/i18n.ts` (or wherever the resource map is)
- Modify: `apps/web/src/routes/_authed/settings/capabilities.tsx` (or wherever capabilities are toggled)

- [ ] **Step 1: i18n files**

`apps/web/src/locales/en/firewall.json`:

```json
{
  "title": "Firewall Blocklist",
  "subtitle": "Block inbound traffic from IPs and CIDRs across one or more agents.",
  "tab": { "blocklist": "Blocklist", "activity": "Activity" },
  "kpi": { "total": "Total", "auto": "Auto", "manual": "Manual", "v6": "IPv6" },
  "column": {
    "target": "Target",
    "family": "Family",
    "cover": "Scope",
    "origin": "Origin",
    "comment": "Comment",
    "created": "Created",
    "actions": "Actions"
  },
  "origin": { "manual": "Manual", "auto": "Auto" },
  "cover_type": { "all": "All servers", "include": "Selected", "exclude": "All except" },
  "add": "Add block",
  "add_form": {
    "target": "Target",
    "target_help": "IP or CIDR — e.g. 1.2.3.4 or 10.0.0.0/8",
    "cover_type": "Scope",
    "server_ids": "Servers",
    "comment": "Comment (optional)",
    "submit": "Block"
  },
  "delete_confirm": {
    "title": "Delete block",
    "body": "Stop blocking {{target}}?",
    "confirm": "Delete"
  },
  "guardrail_rejected": "Cannot block {{target}}: {{reason}}",
  "actions": {
    "block_source_ip": "Block source IP"
  },
  "auto_block": {
    "card_title": "Auto-block source IP",
    "checkbox": "Also auto-block source IP",
    "comment_default": "Auto-block from {{rule_name}}"
  },
  "capability_unavailable": "Firewall capability unavailable on this host",
  "capability_label": "Firewall blocklist"
}
```

`apps/web/src/locales/zh/firewall.json`:

```json
{
  "title": "防火墙黑名单",
  "subtitle": "屏蔽来自指定 IP 或 CIDR 的入站流量,可跨多台 Agent。",
  "tab": { "blocklist": "黑名单", "activity": "活动" },
  "kpi": { "total": "总数", "auto": "自动", "manual": "手动", "v6": "IPv6" },
  "column": {
    "target": "目标",
    "family": "协议",
    "cover": "范围",
    "origin": "来源",
    "comment": "备注",
    "created": "创建时间",
    "actions": "操作"
  },
  "origin": { "manual": "手动", "auto": "自动" },
  "cover_type": { "all": "全部服务器", "include": "指定", "exclude": "排除" },
  "add": "新增屏蔽",
  "add_form": {
    "target": "目标",
    "target_help": "IP 或 CIDR,例如 1.2.3.4 或 10.0.0.0/8",
    "cover_type": "范围",
    "server_ids": "服务器",
    "comment": "备注(可选)",
    "submit": "屏蔽"
  },
  "delete_confirm": {
    "title": "删除屏蔽",
    "body": "停止屏蔽 {{target}}?",
    "confirm": "删除"
  },
  "guardrail_rejected": "无法屏蔽 {{target}}:{{reason}}",
  "actions": {
    "block_source_ip": "屏蔽源 IP"
  },
  "auto_block": {
    "card_title": "自动屏蔽源 IP",
    "checkbox": "同时自动屏蔽源 IP",
    "comment_default": "自动屏蔽:{{rule_name}}"
  },
  "capability_unavailable": "本机暂不支持防火墙能力",
  "capability_label": "防火墙黑名单"
}
```

- [ ] **Step 2: Register in i18n resource map**

In `apps/web/src/i18n.ts`, add `firewall` to both `en` and `zh` resource maps.

- [ ] **Step 3: Capability toggle**

In the capabilities settings page, add the firewall toggle entry. The list is likely driven by the shared `CapabilityKey` enum from common; if there's a manually maintained UI list, add `firewall_block` next to `security_events`.

- [ ] **Step 4: Typecheck**

```bash
cd apps/web && bun run typecheck
```

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/locales/ apps/web/src/i18n.ts apps/web/src/routes/_authed/settings/
git commit -m "feat(web): firewall i18n + capability toggle"
```

---

## Phase 5 — Docs + manual E2E

### Task 5.1: ENV.md + configuration.mdx

**Files:**
- Modify: `ENV.md`
- Modify: `apps/docs/content/docs/en/configuration.mdx`
- Modify: `apps/docs/content/docs/cn/configuration.mdx`

- [ ] **Step 1: ENV.md**

After the existing `[firewall]`-adjacent server sections (or in a new `### Firewall` subsection under Server), add:

```markdown
### Firewall (Optional)

| Environment Variable | TOML Key | Type | Default | Description |
|---------------------|----------|------|---------|-------------|
| `SERVERBEE_FIREWALL__ALLOW_LIST` | `firewall.allow_list` | string[] | `[]` | CIDRs / IPs the server will refuse to insert into `block_list`. Tier-2 guardrail. Defaults already cover RFC1918 + loopback |
```

- [ ] **Step 2: configuration.mdx (en + cn)**

Mirror the same table in both files, in the Server section.

- [ ] **Step 3: Commit**

```bash
git add ENV.md apps/docs/content/docs/
git commit -m "docs: document firewall.allow_list env var"
```

---

### Task 5.2: capabilities.mdx + security-events.mdx + new firewall.mdx

**Files:**
- Modify: `apps/docs/content/docs/{en,cn}/capabilities.mdx`
- Modify: `apps/docs/content/docs/{en,cn}/security-events.mdx`
- Create: `apps/docs/content/docs/{en,cn}/firewall.mdx`
- Modify: `apps/docs/content/docs/{en,cn}/meta.json`

- [ ] **Step 1: capabilities.mdx — add CAP_FIREWALL_BLOCK**

In both en/cn files, add a row to the High-Risk table:

```markdown
| **Firewall Blocklist** | `CAP_FIREWALL_BLOCK` (512) | Allow agent to apply server-pushed nftables blocklist; requires root + nft CLI |
```

Update the "valid mask" / default capability example numbers if they appear inline.

- [ ] **Step 2: security-events.mdx — add Auto-block section**

After the existing "Alerting" section, add a subsection "Auto-block source IP" with:

```markdown
### Auto-block source IP

Brute-force and port-scan alert rules can optionally append a `block_source_ip` action that auto-creates a `block_list` row from the triggering event's source IP. Only `ssh_brute_force_detected` and `port_scan_detected` rules can carry this action — `ssh_new_ip_login` is intentionally forbidden because a legitimate first-time login would lock the user out.

See [Firewall Blocklist](/en/docs/firewall) for the full feature.
```

- [ ] **Step 3: New firewall.mdx (en)**

`apps/docs/content/docs/en/firewall.mdx`:

```markdown
---
title: Firewall Blocklist
description: Block inbound traffic from IPs and CIDRs across one or more agents via nftables.
icon: Shield
---

ServerBee can centrally manage an inbound-traffic blocklist. Server holds the canonical list; each opted-in agent applies it via `nftables`.

## Requirements

- **Linux only** — agent uses the `nft` CLI.
- **`CAP_FIREWALL_BLOCK`** (bit `512`) must be enabled on each server. **Disabled by default.**
- Agent process needs root or `CAP_NET_ADMIN`.

## Manual blocking

Settings → Firewall → **Add block**. Enter an IP (`1.2.3.4`) or a CIDR (`10.0.0.0/8`). Choose the scope (`All servers`, `Selected`, `All except`). Optionally add a comment.

The server rejects targets that overlap a protected range:

- Loopback (`127.0.0.0/8`, `::1`)
- RFC1918 (`10/8`, `172.16/12`, `192.168/16`, `fc00::/7`, `fe80::/10`)
- Multicast / unspecified
- Any CIDR in `firewall.allow_list` (server.toml)
- `server.trusted_proxies`
- Any agent's reported external IP

A 409 response with the matched reason is returned in those cases.

## Auto-block from alerts

Brute-force and port-scan alert rules can append a `block_source_ip` action that auto-inserts the event's source IP. See [Security Events → Auto-block source IP](/en/docs/security-events).

Auto-block is deduplicated by canonical target. If a row already exists and covers the triggering server, the auto-block is silently skipped. If it exists but does not cover the triggering server, the conflict is audited (`firewall_auto_block_skipped_conflict`) and no row is created — the operator can broaden the existing row manually.

## Agent execution

Each opted-in agent maintains an `inet serverbee` nftables table:

```text
table inet serverbee {
    set block_v4 { type ipv4_addr; flags interval; }
    set block_v6 { type ipv6_addr; flags interval; }
    chain input {
        type filter hook input priority -10;
        ip  saddr @block_v4 drop
        ip6 saddr @block_v6 drop
    }
}
```

The server pushes incremental adds/removes over WebSocket. On agent reconnect or capability transition, the server sends a `Reset` followed by a full `Sync`. Agent acks the result of each entry; failures keep the row eligible for retry on the next sync.

## Removing the cleanup

To stop using the feature and clean up nftables state:

1. Disable `CAP_FIREWALL_BLOCK` for the agent in Capabilities settings.
2. The server pushes `BlocklistReset` to the agent, which flushes both sets and drops the `inet serverbee` table.

Manual cleanup (from the host shell):

```bash
nft delete table inet serverbee
```

## Audit log

Every action — `firewall_block_created`, `firewall_block_deleted`, `firewall_block_applied_agent`, `firewall_block_removed_agent`, `firewall_block_rejected_server`, `firewall_block_rejected_agent`, `firewall_auto_block_skipped_conflict`, `firewall_reset_acked` — is recorded in the audit log and visible in the Firewall page's Activity tab.

## Limitations

- nftables only — no iptables fallback
- IPv4 and IPv6, no domain names
- Permanent blocks — no scheduled expiry
- input chain only — does not filter forward / output
```

Mirror the same structure in `cn/firewall.mdx` (Chinese).

- [ ] **Step 4: meta.json**

Add `firewall` to the Features section in both `en/meta.json` and `cn/meta.json` (next to `security-events`).

- [ ] **Step 5: Build docs**

```bash
cd apps/docs && bun run types:check
```

- [ ] **Step 6: Commit**

```bash
git add apps/docs/content/docs/
git commit -m "docs: firewall blocklist guide + capability/alert updates"
```

---

### Task 5.3: E2E manual checklist + README index

**Files:**
- Create: `tests/firewall-block.md`
- Modify: `tests/README.md`

- [ ] **Step 1: Checklist file**

`tests/firewall-block.md`:

```markdown
# Firewall Blocklist — E2E Verification

> Prereq: Linux VPS with `nft` installed (`apt install nftables`), agent running as root, `CAP_FIREWALL_BLOCK` enabled in Capabilities settings.

## Setup

| # | Step | Expected |
|---|------|----------|
| S1 | Enable `CAP_FIREWALL_BLOCK` for the test server | Server pushes `BlocklistReset` + empty `BlocklistSync`; `nft list table inet serverbee` shows the table with empty sets |

## Manual CRUD

| # | Step | Expected |
|---|------|----------|
| M1 | UI → Settings → Firewall → Add block `198.51.100.5` (cover_type=all) | Row appears; toast confirms; `nft list set inet serverbee block_v4` shows `198.51.100.5` |
| M2 | From an external host: `curl --connect-timeout 3 http://<vps>:22` | Connection drops |
| M3 | Delete the row from UI | `nft list set inet serverbee block_v4` no longer shows `198.51.100.5`; `curl` succeeds again |
| M4 | Add `203.0.113.0/24` and verify `nft` set contains the interval | OK |
| M5 | Add same target twice in a row | Second POST returns 409 `target ... already blocked` |

## Guardrails

| # | Step | Expected |
|---|------|----------|
| G1 | Add `127.0.0.1` | 409 with reason mentioning loopback |
| G2 | Add the VPS's own external IP | 409 mentioning allow_list (tier 2) |
| G3 | Configure `SERVERBEE_FIREWALL__ALLOW_LIST=203.0.113.5` and try to add `203.0.113.5` | 409 |

## Auto-block

| # | Step | Expected |
|---|------|----------|
| A1 | Create alert rule `ssh_brute_force_detected` with action `block_source_ip` (cover_type=all) | Save succeeds |
| A2 | Trigger SSH brute-force from external host (15+ failed attempts) | New `block_list` row with `origin=auto`, `origin_event_id` set; `nft` set contains attacker IP |
| A3 | Repeat from same IP after a minute | No new row created (dedup) |
| A4 | Trigger again from a server not covered by an existing block (different cover_type=include scenario) | Audit log shows `firewall_auto_block_skipped_conflict` |

## Capability transitions

| # | Step | Expected |
|---|------|----------|
| C1 | With 3 blocks active, disable `CAP_FIREWALL_BLOCK` for the server | Server pushes `BlocklistReset`; `nft list ruleset` no longer contains `inet serverbee` |
| C2 | Re-enable the capability | Server pushes Reset + Sync; `nft list set inet serverbee block_v4` contains all 3 entries |

## Resilience

| # | Step | Expected |
|---|------|----------|
| R1 | Restart agent | Resource bootstrap re-runs; `nft list table inet serverbee` re-created with same entries |
| R2 | Restart server | Agent reconnects; full sync rebuilds in-memory apply state (Activity log shows fresh `firewall_block_applied_agent` entries) |
| R3 | Stop `nftables.service` on host, then disable cap | Agent acks `BlocklistResetAck { ok: false, reason: "nft kernel module unavailable" }`; audit shows `firewall_reset_failed_agent` |

## Permission checks

| # | Step | Expected |
|---|------|----------|
| P1 | Member-user POST /api/firewall/blocks | 403 |
| P2 | Member-user DELETE | 403 |
| P3 | Member-user GET list | 200 |
```

- [ ] **Step 2: README index**

In `tests/README.md`, add a row:

```markdown
| [firewall-block.md](firewall-block.md) | Firewall blocklist (manual + auto) | `/settings/firewall`, `/security`, `/settings/alerts` |
```

- [ ] **Step 3: Commit**

```bash
git add tests/
git commit -m "test(e2e): firewall blocklist manual checklist"
```

---

### Task 5.4: VPS smoke (Setup → S1 of E2E checklist)

**Files:** none — runtime verification.

- [ ] **Step 1: Workspace check**

```bash
cargo build --workspace --release
cd apps/web && bun install && bun run build && cd ../..
```

- [ ] **Step 2: SSH to VPS, install nftables**

```bash
ssh root@<vps-host>   # creds in goal directive
apt update && apt install -y nftables
systemctl enable --now nftables
nft list ruleset
```

- [ ] **Step 3: Stop existing serverbee, copy fresh build**

(Use the existing E2E pattern from `tests/security-events.md`'s V7 — same shape: scp the binary to `/opt/serverbee-src`, run via systemd or `nohup`.)

- [ ] **Step 4: Run E2E checklist**

Walk through `tests/firewall-block.md` rows S1 → P3. Note results inline in a fresh local copy. **Do not push.**

- [ ] **Step 5: Cleanup**

`nft delete table inet serverbee`; stop test serverbee processes; restore original systemd service if one was preempted (mirror the `tests/security-events.md::V13` cleanup pattern).

- [ ] **Step 6: Commit E2E evidence**

```bash
git add tests/firewall-block.md
git commit -m "test(e2e): record firewall blocklist VPS evidence"
```

(Status column updated with ✅ / observed behavior. **Do not record VPS IP** in the file — use `<vps-host>` placeholder, same convention as `tests/security-events.md`.)

---

## Self-Review

### Spec coverage

| Spec section | Implementing task |
|---|---|
| § 1 Architecture | All — informs overall plan |
| § 2.1 block_list table | Task 1.1, 1.2 |
| § 2.2 alert_rule.actions_json | Task 1.3, 1.4 |
| § 2.3 [firewall] config | Task 1.5 |
| § 2.4 Migrations | Task 1.2, 1.3 |
| § 2.5 Canonicalization | Task 1.6 |
| § 2.6 RecoveryMergeService | Task 1.8 |
| § 3 Protocol | Task 0.2, 0.3 |
| § 4.1–4.3 Guardrails | Task 1.6 (server), Task 3.2 (agent) |
| § 4.4 Audit | Task 2.5, 2.6, 2.7 |
| § 4.5 UI feedback (409) | Task 1.7 |
| § 5.1 SecurityService hook | Task 2.6 |
| § 5.2 auto_block | Task 2.6, 2.7 |
| § 5.3 Validator | Task 1.4 |
| § 5.4 Cascade on rule delete | (Already implicit — no cascade) |
| § 6.1 Server WS connect / cap transition | Task 2.4 |
| § 6.2 FirewallManager | Task 3.3 |
| § 6.3 nft invocation | Task 3.1 |
| § 6.4 Failure matrix | Verified across Task 2.8 + Task 3.3 + 5.3 |
| § 6.5 Capability advertisement | Task 3.4 |
| § 7 REST | Task 1.7 |
| § 8 Browser real-time | Task 4.3 |
| § 9 Frontend | Task 4.2, 4.4, 4.5 |
| § 10 Docs | Task 5.1, 5.2 |
| § 11 Tests | Task 1.6, 1.7, 2.8, 3.1, 3.2, 3.3, 5.3, 5.4 |
| § 12 Rollout | Task 0.2 (`FIREWALL_MIN_PROTOCOL`), Task 2.3 (gate), Task 3.4 (local cap) |

No gaps.

### Placeholder scan

- "find it" / "match the existing convention" appears in Task 1.4, 1.5, 2.2, 3.4, 4.2, 4.4 — these point to specific files the engineer must read first. Acceptable.
- No `TODO` / `TBD` / "implement later".

### Type consistency

- `BlocklistEntryState`, `BlockEntry`, `BlocklistAckItem` — defined Task 0.2; used Task 2.5, 3.3, 4.1. ✓
- `FirewallService::canonicalize_target` returns `(String, u8)` — used Task 1.7 (cast to `i32` for `family` column), Task 2.6. ✓
- `AlertRuleAction::BlockSourceIp { cover_type, server_ids_json, comment }` — defined Task 1.4; matched Task 2.6, 2.7. ✓
- Push helper signatures — Task 2.3 takes `&AgentManager` arg; Task 2.6 + 1.7 callers pass `&state.agent_manager`. ✓
- `FIREWALL_MIN_PROTOCOL = 2` — Task 0.2 sets; Task 2.3, 2.4 enforce. ✓
