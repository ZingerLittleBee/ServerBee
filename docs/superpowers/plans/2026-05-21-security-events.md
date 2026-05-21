# Security Events Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Linux-only SSH login / brute-force / port-scan detection in the agent, surface events through the existing WebSocket + alert pipeline, and add a Security page in the web UI.

**Architecture:** Agent-side sliding-window detection over journalctl + nfnetlink_conntrack. Events flow over WS as `AgentMessage::SecurityEvent`, persisted to a new `security_event` table, fan-out to browsers via `BrowserMessage::SecurityEvent`, and trigger event-driven alerts that mirror the `ip_changed` pattern. Port scan detection is **opt-in** because it requires `CAP_NET_ADMIN`.

**Tech Stack:** Rust (sea-orm 1.x, axum 0.8, tokio, tokio-tungstenite), React 19 + TanStack Router/Query, shadcn/ui, Recharts. New dependencies: `netlink-sys`, `netlink-packet-netfilter`, `notify` (Rust), `regex` (already present).

**Spec:** `docs/superpowers/specs/2026-05-21-security-events-design.md` is authoritative — read it before starting.

**Critical conventions:**
- Conventional Commits, lowercase types, no Claude attribution anywhere.
- PR/commit text in English; conversation in 中文.
- Server IDs are `String` (UUID v4). All new IDs: `Uuid::new_v4().to_string()`.
- Capability column is `servers.capabilities` (`i32`). Runtime effective mask: `agent_manager.get_effective_capabilities`.
- Frontend WS types live inline in `apps/web/src/hooks/use-servers-ws.ts:55` (no separate ws.ts file).
- All scrollable UI uses shadcn `<ScrollArea>`, no naked `overflow-auto`.

---

## Phase 0 — Foundation (protocol + entity + migrations)

### Task 0.1: Add CAP_SECURITY_EVENTS to capability registry

**Files:**
- Modify: `crates/common/src/constants.rs`
- Test: `crates/common/src/constants.rs` (inline `#[cfg(test)]` module already exists, append)

- [ ] **Step 1: Add the capability bit constant**

Append after `CAP_DOCKER` in `constants.rs`:
```rust
pub const CAP_SECURITY_EVENTS: u32 = 1 << 8; // 256
```

- [ ] **Step 2: Update CAP_DEFAULT and CAP_VALID_MASK**

```rust
pub const CAP_DEFAULT: u32 =
    CAP_UPGRADE | CAP_PING_ICMP | CAP_PING_TCP | CAP_PING_HTTP | CAP_SECURITY_EVENTS; // 316
pub const CAP_VALID_MASK: u32 = 0b1_1111_1111; // 511 — bits 0-8
```

- [ ] **Step 3: Add `SecurityEvents` to `CapabilityKey` enum**

Add the variant after `Docker`, update both `to_bit` (around L77-84) and `FromStr` impl (around L89):
```rust
pub enum CapabilityKey {
    // ... existing
    Docker,
    SecurityEvents,
}
// to_bit:
Self::SecurityEvents => CAP_SECURITY_EVENTS,
// FromStr:
"security_events" => Ok(Self::SecurityEvents),
```

- [ ] **Step 4: Append the `ALL_CAPABILITIES` entry**

After the `Docker` entry in the `ALL_CAPABILITIES` array:
```rust
CapabilityMeta {
    bit: CAP_SECURITY_EVENTS,
    key: "security_events",
    display_name: "Security Events",
    default_enabled: true,
    risk_level: "low",
},
```

- [ ] **Step 5: Update `parse_cap` helper**

Around L194 — add the new mapping where the existing `"icmp"/"tcp"/"http"` aliases live; also accept `"security_events"`:
```rust
"security_events" => Some(CAP_SECURITY_EVENTS),
```

- [ ] **Step 6: Add unit tests**

```rust
#[test]
fn cap_default_includes_security_events() {
    assert!(has_capability(CAP_DEFAULT, CAP_SECURITY_EVENTS));
    assert_eq!(CAP_DEFAULT, 316);
}

#[test]
fn cap_valid_mask_covers_new_bit() {
    assert_eq!(CAP_VALID_MASK & CAP_SECURITY_EVENTS, CAP_SECURITY_EVENTS);
}

#[test]
fn all_capabilities_includes_security_events() {
    let entry = ALL_CAPABILITIES.iter().find(|m| m.bit == CAP_SECURITY_EVENTS);
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().key, "security_events");
    assert!(entry.unwrap().default_enabled);
}

#[test]
fn capability_key_security_events_round_trip() {
    let key: CapabilityKey = "security_events".parse().unwrap();
    assert_eq!(key.to_bit(), CAP_SECURITY_EVENTS);
}
```

- [ ] **Step 7: Run tests**

Run: `cargo test -p serverbee-common`
Expected: all pass, including the four new tests.

- [ ] **Step 8: Commit**

```bash
git add crates/common/src/constants.rs
git commit -m "feat(common): add CAP_SECURITY_EVENTS capability bit"
```

---

### Task 0.2: Add SecurityEvent message variants and payload types

**Files:**
- Create: `crates/common/src/security.rs`
- Modify: `crates/common/src/lib.rs`
- Modify: `crates/common/src/agent_message.rs`
- Modify: `crates/common/src/browser_message.rs`
- Test: inline in `security.rs`

- [ ] **Step 1: Create security.rs with payload types**

```rust
// crates/common/src/security.rs
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecurityEventType {
    SshLogin,
    SshBruteForce,
    PortScan,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DetectorSource {
    Journal,
    AuthLog,
    Conntrack,
    FirewallLog,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SshAuthMethod {
    Publickey,
    Password,
    KeyboardInteractive,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SecurityEvidence {
    SshLogin {
        auth_method: SshAuthMethod,
    },
    SshBruteForce {
        failed_count: u32,
        distinct_users: u32,
        sample_users: Vec<String>,
        invalid_user_count: u32,
        window_seconds: u32,
        threshold: u32,
    },
    PortScan {
        distinct_ports: u32,
        sample_ports: Vec<u16>,
        total_attempts: u32,
        window_seconds: u32,
        threshold: u32,
        blocked_count: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SecurityEventPayload {
    pub event_type: SecurityEventType,
    pub severity: Severity,
    pub source_ip: String,
    pub source_port: Option<u16>,
    pub username: Option<String>,
    pub started_at: i64, // unix seconds UTC
    pub ended_at: i64,
    pub first_seen: bool,
    pub detector_source: DetectorSource,
    pub evidence: SecurityEvidence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_round_trips() {
        let p = SecurityEventPayload {
            event_type: SecurityEventType::SshBruteForce,
            severity: Severity::High,
            source_ip: "203.0.113.5".into(),
            source_port: None,
            username: None,
            started_at: 1_700_000_000,
            ended_at: 1_700_000_060,
            first_seen: false,
            detector_source: DetectorSource::Journal,
            evidence: SecurityEvidence::SshBruteForce {
                failed_count: 47,
                distinct_users: 3,
                sample_users: vec!["root".into(), "admin".into()],
                invalid_user_count: 8,
                window_seconds: 60,
                threshold: 10,
            },
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: SecurityEventPayload = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.event_type, SecurityEventType::SshBruteForce));
        assert_eq!(back.source_ip, "203.0.113.5");
    }

    #[test]
    fn evidence_tag_serializes_to_kind() {
        let e = SecurityEvidence::SshLogin {
            auth_method: SshAuthMethod::Publickey,
        };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["kind"], "ssh_login");
    }
}
```

- [ ] **Step 2: Export from lib.rs**

Add `pub mod security;` to `crates/common/src/lib.rs`.

- [ ] **Step 3: Add AgentMessage variant**

In `crates/common/src/agent_message.rs`, add to the `AgentMessage` enum:
```rust
SecurityEvent(crate::security::SecurityEventPayload),
```
Place it grouped with other event-style messages (after `PingResult`).

- [ ] **Step 4: Add BrowserMessage variant**

In `crates/common/src/browser_message.rs`, add:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEventBroadcast {
    pub server_id: String,
    pub event_id: String,
    pub event: crate::security::SecurityEventPayload,
}

// then in the enum:
SecurityEvent(SecurityEventBroadcast),
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p serverbee-common`
Expected: all pass.

- [ ] **Step 6: Build full workspace to confirm no break**

Run: `cargo build --workspace`
Expected: success.

- [ ] **Step 7: Commit**

```bash
git add crates/common/
git commit -m "feat(common): add SecurityEvent message variants and payload types"
```

---

### Task 0.3: Create `security_event` entity and migration

**Files:**
- Create: `crates/server/src/entity/security_event.rs`
- Modify: `crates/server/src/entity/mod.rs`
- Create: `crates/server/src/migration/m20260521_001_create_security_event.rs`
- Modify: `crates/server/src/migration/mod.rs`
- Test: integration test via `cargo test -p serverbee-server` (existing in-memory sqlite test harness)

- [ ] **Step 1: Write entity**

```rust
// crates/server/src/entity/security_event.rs
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "security_event")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub server_id: String,
    pub event_type: String,
    pub severity: String,
    pub source_ip: String,
    pub source_port: Option<i32>,
    pub username: Option<String>,
    pub started_at: DateTimeUtc,
    pub ended_at: DateTimeUtc,
    pub first_seen: bool,
    pub detector_source: String,
    pub evidence: String, // JSON-encoded
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::server::Entity",
        from = "Column::ServerId",
        to = "super::server::Column::Id"
    )]
    Server,
}

impl Related<super::server::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Server.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 2: Register entity in mod.rs**

Append to `crates/server/src/entity/mod.rs`:
```rust
pub mod security_event;
```

- [ ] **Step 3: Write the migration**

```rust
// crates/server/src/migration/m20260521_001_create_security_event.rs
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SecurityEvent::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(SecurityEvent::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(SecurityEvent::ServerId).string().not_null())
                    .col(ColumnDef::new(SecurityEvent::EventType).string().not_null())
                    .col(ColumnDef::new(SecurityEvent::Severity).string().not_null())
                    .col(ColumnDef::new(SecurityEvent::SourceIp).string().not_null())
                    .col(ColumnDef::new(SecurityEvent::SourcePort).integer().null())
                    .col(ColumnDef::new(SecurityEvent::Username).string().null())
                    .col(ColumnDef::new(SecurityEvent::StartedAt).timestamp_with_time_zone().not_null())
                    .col(ColumnDef::new(SecurityEvent::EndedAt).timestamp_with_time_zone().not_null())
                    .col(ColumnDef::new(SecurityEvent::FirstSeen).boolean().not_null().default(false))
                    .col(ColumnDef::new(SecurityEvent::DetectorSource).string().not_null())
                    .col(ColumnDef::new(SecurityEvent::Evidence).text().not_null())
                    .col(ColumnDef::new(SecurityEvent::CreatedAt).timestamp_with_time_zone().not_null())
                    .to_owned(),
            )
            .await?;

        for (name, cols) in [
            ("idx_security_event_server_id_created_at", vec![SecurityEvent::ServerId, SecurityEvent::CreatedAt]),
            ("idx_security_event_source_ip_created_at", vec![SecurityEvent::SourceIp, SecurityEvent::CreatedAt]),
            ("idx_security_event_event_type_created_at", vec![SecurityEvent::EventType, SecurityEvent::CreatedAt]),
            ("idx_security_event_dedupe", vec![SecurityEvent::ServerId, SecurityEvent::EventType, SecurityEvent::SourceIp, SecurityEvent::StartedAt]),
        ] {
            let mut idx = Index::create().name(name).table(SecurityEvent::Table).to_owned();
            for c in cols { idx.col(c); }
            manager.create_index(idx).await?;
        }

        Ok(())
    }

    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> { Ok(()) }
}

#[derive(Iden)]
enum SecurityEvent {
    Table,
    Id,
    ServerId,
    EventType,
    Severity,
    SourceIp,
    SourcePort,
    Username,
    StartedAt,
    EndedAt,
    FirstSeen,
    DetectorSource,
    Evidence,
    CreatedAt,
}
```

- [ ] **Step 4: Register migration in mod.rs**

Append the migration to the `Migrator::migrations` vec in the order it appears chronologically.

- [ ] **Step 5: Run migration test**

Run: `cargo test -p serverbee-server migration`
Expected: existing migration suite still passes (the harness exercises `up()` against an in-memory sqlite).

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/entity/security_event.rs crates/server/src/entity/mod.rs crates/server/src/migration/
git commit -m "feat(server): create security_event entity and migration"
```

---

### Task 0.4: Migrate `alert_states` to add `event_key` and rebuild unique index

**Files:**
- Modify: `crates/server/src/entity/alert_state.rs`
- Create: `crates/server/src/migration/m20260521_002_extend_alert_state_event_key.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Add field to entity**

In `entity/alert_state.rs`, add to `Model`:
```rust
#[sea_orm(default_value = "")]
pub event_key: String,
```

- [ ] **Step 2: Write migration**

```rust
// m20260521_002_extend_alert_state_event_key.rs
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.alter_table(
            Table::alter()
                .table(AlertStates::Table)
                .add_column(
                    ColumnDef::new(AlertStates::EventKey)
                        .text()
                        .not_null()
                        .default("")
                )
                .to_owned()
        ).await?;

        // Drop the old (rule_id, server_id) unique index
        manager.drop_index(
            Index::drop()
                .name("idx_alert_states_rule_id_server_id")
                .table(AlertStates::Table)
                .to_owned()
        ).await?;

        manager.create_index(
            Index::create()
                .name("idx_alert_states_rule_id_server_id_event_key")
                .table(AlertStates::Table)
                .col(AlertStates::RuleId)
                .col(AlertStates::ServerId)
                .col(AlertStates::EventKey)
                .unique()
                .to_owned()
        ).await?;

        Ok(())
    }

    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> { Ok(()) }
}

#[derive(Iden)]
enum AlertStates {
    Table,
    RuleId,
    ServerId,
    EventKey,
}
```

- [ ] **Step 3: Update AlertStateManager to use event_key dimension**

In `crates/server/src/service/alert.rs`:
- Change `triggered: DashMap<(String, String), TriggeredInfo>` to `DashMap<(String, String, String), TriggeredInfo>`.
- Update `is_triggered`, `get_info`, `mark_triggered`, `mark_resolved` to accept `event_key: &str`.
- Update `load_from_db` to read `event_key` column into key.
- Update all existing call sites in `alert_evaluator.rs` and elsewhere to pass `""` as `event_key`.

- [ ] **Step 4: Run cargo test**

Run: `cargo test -p serverbee-server`
Expected: all existing tests pass with the new signature (`""` passed at call sites preserves legacy semantics).

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/entity/alert_state.rs crates/server/src/migration/ crates/server/src/service/alert.rs crates/server/src/task/alert_evaluator.rs
git commit -m "refactor(server): extend AlertStateManager dedupe key with event_key dimension"
```

---

### Task 0.5: Backfill capability for existing server rows

**Files:**
- Create: `crates/server/src/migration/m20260521_003_backfill_capability_default.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Write migration**

```rust
// m20260521_003_backfill_capability_default.rs
use sea_orm_migration::prelude::*;
use sea_orm::Statement;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute(Statement::from_string(
            db.get_database_backend(),
            "UPDATE servers SET capabilities = capabilities | 256 WHERE capabilities = 60".to_string(),
        )).await?;
        Ok(())
    }

    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> { Ok(()) }
}
```

- [ ] **Step 2: Register and run**

Append to migrator. Run: `cargo test -p serverbee-server`. Expected: pass.

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/migration/
git commit -m "feat(server): backfill capability default for existing servers"
```

---

## Phase 1 — Agent (security collectors and detectors)

### Task 1.1: Agent config additions

**Files:**
- Modify: `crates/agent/src/config.rs`

- [ ] **Step 1: Add SecurityConfig struct with serde defaults**

```rust
// in config.rs
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_security_data_dir")]
    pub data_dir: String,
    #[serde(default)]
    pub ssh: SshDetectorConfig,
    #[serde(default)]
    pub port_scan: PortScanConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SshDetectorConfig {
    #[serde(default = "default_ssh_window")]    pub window_seconds: u32,    // 60
    #[serde(default = "default_ssh_threshold")] pub failed_threshold: u32,  // 10
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PortScanConfig {
    #[serde(default)] // bool default false
    pub enabled: bool,
    #[serde(default = "default_scan_window")]    pub window_seconds: u32,           // 30
    #[serde(default = "default_scan_threshold")] pub distinct_port_threshold: u32,  // 20
}

fn default_true() -> bool { true }
fn default_security_data_dir() -> String { "/var/lib/serverbee/security".into() }
fn default_ssh_window() -> u32 { 60 }
fn default_ssh_threshold() -> u32 { 10 }
fn default_scan_window() -> u32 { 30 }
fn default_scan_threshold() -> u32 { 20 }

impl Default for SshDetectorConfig {
    fn default() -> Self { Self { window_seconds: 60, failed_threshold: 10 } }
}
impl Default for PortScanConfig {
    fn default() -> Self { Self { enabled: false, window_seconds: 30, distinct_port_threshold: 20 } }
}
impl Default for SecurityConfig {
    fn default() -> Self { Self {
        enabled: true,
        data_dir: default_security_data_dir(),
        ssh: SshDetectorConfig::default(),
        port_scan: PortScanConfig::default(),
    }}
}
```

- [ ] **Step 2: Add `security: SecurityConfig` field to AgentConfig**

```rust
#[serde(default)]
pub security: SecurityConfig,
```

- [ ] **Step 3: Test config round-trip**

Add a `#[test]` that loads an example TOML and confirms defaults apply.

- [ ] **Step 4: Run cargo test**

Run: `cargo test -p serverbee-agent config`
Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add crates/agent/src/config.rs
git commit -m "feat(agent): add SecurityConfig with sensible defaults"
```

---

### Task 1.2: SSH log parser

**Files:**
- Create: `crates/agent/src/security/mod.rs`
- Create: `crates/agent/src/security/ssh_parser.rs`
- Modify: `crates/agent/src/main.rs` (add `mod security;`)
- Modify: `crates/agent/Cargo.toml` (add `regex` if not yet present)

- [ ] **Step 1: Skeleton mod**

```rust
// crates/agent/src/security/mod.rs
mod ssh_parser;
pub use ssh_parser::{AuthAttempt, AuthOutcome, parse_sshd_line};
```

- [ ] **Step 2: Write failing tests**

```rust
// crates/agent/src/security/ssh_parser.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthOutcome {
    Success { auth_method: AuthMethodHint },
    Failure { invalid_user: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethodHint {
    Publickey,
    Password,
    KeyboardInteractive,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthAttempt {
    pub outcome: AuthOutcome,
    pub username: String,
    pub source_ip: String,
    pub source_port: Option<u16>,
}

pub fn parse_sshd_line(line: &str) -> Option<AuthAttempt> {
    // Implementation below
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_accepted_publickey() {
        let line = "Accepted publickey for root from 203.0.113.5 port 12345 ssh2: ED25519 SHA256:abc";
        let a = parse_sshd_line(line).unwrap();
        assert_eq!(a.username, "root");
        assert_eq!(a.source_ip, "203.0.113.5");
        assert_eq!(a.source_port, Some(12345));
        assert!(matches!(a.outcome, AuthOutcome::Success { auth_method: AuthMethodHint::Publickey }));
    }

    #[test]
    fn parses_failed_password() {
        let line = "Failed password for root from 198.51.100.7 port 60000 ssh2";
        let a = parse_sshd_line(line).unwrap();
        assert!(matches!(a.outcome, AuthOutcome::Failure { invalid_user: false }));
        assert_eq!(a.source_ip, "198.51.100.7");
    }

    #[test]
    fn parses_failed_invalid_user() {
        let line = "Failed password for invalid user fake from 10.0.0.5 port 50000 ssh2";
        let a = parse_sshd_line(line).unwrap();
        assert!(matches!(a.outcome, AuthOutcome::Failure { invalid_user: true }));
        assert_eq!(a.username, "fake");
    }

    #[test]
    fn parses_invalid_user_line() {
        let line = "Invalid user attacker from 192.0.2.5 port 22000";
        let a = parse_sshd_line(line).unwrap();
        assert!(matches!(a.outcome, AuthOutcome::Failure { invalid_user: true }));
        assert_eq!(a.username, "attacker");
    }

    #[test]
    fn parses_ipv6() {
        let line = "Accepted publickey for ubuntu from 2001:db8::1 port 22 ssh2: RSA SHA256:xyz";
        let a = parse_sshd_line(line).unwrap();
        assert_eq!(a.source_ip, "2001:db8::1");
    }

    #[test]
    fn returns_none_on_unrelated_line() {
        assert!(parse_sshd_line("pam_unix(sshd:session): session opened").is_none());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p serverbee-agent ssh_parser -- --include-ignored 2>&1 | head -50`
Expected: 6 failures with `not yet implemented`.

- [ ] **Step 4: Implement parser**

Replace `todo!()` with `regex::Regex::new(...)` once-lazily evaluated patterns:
```rust
use once_cell::sync::Lazy;
use regex::Regex;

static RE_ACCEPTED: Lazy<Regex> = Lazy::new(|| Regex::new(
    r"^Accepted (publickey|password|keyboard-interactive|\S+) for (\S+) from (\S+) port (\d+)"
).unwrap());
static RE_FAILED: Lazy<Regex> = Lazy::new(|| Regex::new(
    r"^Failed \S+ for (invalid user )?(\S+) from (\S+) port (\d+)"
).unwrap());
static RE_INVALID_USER: Lazy<Regex> = Lazy::new(|| Regex::new(
    r"^Invalid user (\S+) from (\S+) port (\d+)"
).unwrap());

pub fn parse_sshd_line(line: &str) -> Option<AuthAttempt> {
    if let Some(c) = RE_ACCEPTED.captures(line) {
        let method = match &c[1] {
            "publickey" => AuthMethodHint::Publickey,
            "password" => AuthMethodHint::Password,
            "keyboard-interactive" => AuthMethodHint::KeyboardInteractive,
            _ => AuthMethodHint::Other,
        };
        return Some(AuthAttempt {
            outcome: AuthOutcome::Success { auth_method: method },
            username: c[2].to_string(),
            source_ip: c[3].to_string(),
            source_port: c[4].parse().ok(),
        });
    }
    if let Some(c) = RE_FAILED.captures(line) {
        let invalid_user = c.get(1).is_some();
        return Some(AuthAttempt {
            outcome: AuthOutcome::Failure { invalid_user },
            username: c[2].to_string(),
            source_ip: c[3].to_string(),
            source_port: c[4].parse().ok(),
        });
    }
    if let Some(c) = RE_INVALID_USER.captures(line) {
        return Some(AuthAttempt {
            outcome: AuthOutcome::Failure { invalid_user: true },
            username: c[1].to_string(),
            source_ip: c[2].to_string(),
            source_port: c[3].parse().ok(),
        });
    }
    None
}
```

Add `regex` and `once_cell` to `crates/agent/Cargo.toml` if missing.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p serverbee-agent ssh_parser`
Expected: 6 passing.

- [ ] **Step 6: Commit**

```bash
git add crates/agent/src/security/ crates/agent/src/main.rs crates/agent/Cargo.toml
git commit -m "feat(agent): add sshd log line parser"
```

---

### Task 1.3: SSH brute-force detector with severity escalation

**Files:**
- Create: `crates/agent/src/security/ssh_detector.rs`
- Modify: `crates/agent/src/security/mod.rs`

- [ ] **Step 1: Write tests first**

```rust
// crates/agent/src/security/ssh_detector.rs
use std::time::{Duration, Instant};
use crate::security::ssh_parser::{AuthAttempt, AuthOutcome};
use serverbee_common::security::{Severity, SecurityEvidence};

pub struct SshDetector {
    window: Duration,
    threshold: u32,
    clock: Box<dyn Fn() -> Instant + Send + Sync>,
    state: std::collections::HashMap<String, std::collections::VecDeque<(Instant, AuthAttempt)>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DetectorEmit {
    None,
    BruteForce { source_ip: String, severity: Severity, evidence: SecurityEvidence },
    Login { username: String, source_ip: String, source_port: Option<u16>, auth_method: serverbee_common::security::SshAuthMethod },
}

impl SshDetector {
    pub fn new(window: Duration, threshold: u32) -> Self { ... }
    pub fn with_clock(window: Duration, threshold: u32, clock: impl Fn() -> Instant + Send + Sync + 'static) -> Self { ... }
    pub fn observe(&mut self, attempt: AuthAttempt) -> DetectorEmit { ... }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn attempt(user: &str, ip: &str, success: bool) -> AuthAttempt {
        AuthAttempt {
            outcome: if success {
                AuthOutcome::Success { auth_method: super::super::ssh_parser::AuthMethodHint::Publickey }
            } else {
                AuthOutcome::Failure { invalid_user: false }
            },
            username: user.into(),
            source_ip: ip.into(),
            source_port: Some(22),
        }
    }

    #[test]
    fn single_user_hammering_triggers() {
        let now = Arc::new(Mutex::new(Instant::now()));
        let nowc = now.clone();
        let mut det = SshDetector::with_clock(Duration::from_secs(60), 10, move || *nowc.lock().unwrap());
        for _ in 0..9 {
            assert_eq!(det.observe(attempt("root", "1.2.3.4", false)), DetectorEmit::None);
        }
        let emit = det.observe(attempt("root", "1.2.3.4", false));
        match emit {
            DetectorEmit::BruteForce { severity, .. } => assert_eq!(severity, Severity::Medium),
            _ => panic!("expected brute force trigger"),
        }
    }

    #[test]
    fn distinct_users_escalates_severity() {
        let mut det = SshDetector::new(Duration::from_secs(60), 5);
        for u in &["root", "admin", "ubuntu", "postgres", "git"] {
            det.observe(attempt(u, "1.2.3.4", false));
        }
        // 5th distinct → critical
        let last = det.observe(attempt("nginx", "1.2.3.4", false));
        match last {
            DetectorEmit::BruteForce { severity, .. } => {
                assert!(matches!(severity, Severity::Critical | Severity::High));
            }
            _ => panic!("expected fire"),
        }
    }

    #[test]
    fn window_expiry_resets() {
        let now = Arc::new(Mutex::new(Instant::now()));
        let nowc = now.clone();
        let mut det = SshDetector::with_clock(Duration::from_secs(60), 3, move || *nowc.lock().unwrap());
        det.observe(attempt("root", "1.2.3.4", false));
        det.observe(attempt("root", "1.2.3.4", false));
        *now.lock().unwrap() += Duration::from_secs(120);
        // First two should now have aged out
        assert_eq!(det.observe(attempt("root", "1.2.3.4", false)), DetectorEmit::None);
    }

    #[test]
    fn success_emits_login() {
        let mut det = SshDetector::new(Duration::from_secs(60), 10);
        let e = det.observe(attempt("ubuntu", "5.6.7.8", true));
        assert!(matches!(e, DetectorEmit::Login { .. }));
    }
}
```

- [ ] **Step 2: Implement**

Severity rule per spec §6.4:
- `distinct_users == 1` → `Medium`
- `distinct_users` in `2..=4` → `High`
- `distinct_users >= 5` → `Critical`

After firing, clear that IP's queue.

- [ ] **Step 3: Run tests**

Run: `cargo test -p serverbee-agent ssh_detector`
Expected: 4 passing.

- [ ] **Step 4: Commit**

```bash
git add crates/agent/src/security/
git commit -m "feat(agent): add SSH brute-force detector with severity escalation"
```

---

### Task 1.4: first_seen store with persistence

**Files:**
- Create: `crates/agent/src/security/first_seen_store.rs`
- Modify: `crates/agent/src/security/mod.rs`

- [ ] **Step 1: TDD test list**

Tests to write:
- `mark_first_returns_true_initially`
- `mark_first_returns_false_on_repeat`
- `persists_across_reload` — write file, recreate store from same path, assert no longer first
- `corrupted_file_resets_and_continues` — write garbage to file, instantiate, assert no panic and starts empty
- `lru_evicts_when_over_cap` — set cap to 10, insert 12, oldest should be gone

Use `tempfile::TempDir` for paths.

- [ ] **Step 2: Implementation**

```rust
pub struct FirstSeenStore {
    path: PathBuf,
    cap: usize,
    map: HashMap<(String, String), i64>,  // (user, ip) -> first_seen_unix_ts
    dirty: bool,
}

impl FirstSeenStore {
    pub fn open(path: PathBuf, cap: usize) -> Self { ... }
    pub fn mark(&mut self, user: &str, ip: &str, now_ts: i64) -> bool { ... }  // returns true if first
    pub fn flush(&mut self) -> std::io::Result<()> { ... }  // atomic tmp + rename
}
```

JSON format: serialize keys as `format!("{user}\x00{ip}")`.

- [ ] **Step 3: Run tests, then commit**

Run: `cargo test -p serverbee-agent first_seen`
```bash
git add crates/agent/src/security/
git commit -m "feat(agent): add persistent first_seen store"
```

---

### Task 1.5: Port-scan detector

**Files:**
- Create: `crates/agent/src/security/scan_detector.rs`
- Modify: `crates/agent/src/security/mod.rs`

- [ ] **Step 1: TDD**

```rust
pub struct ScanDetector {
    window: Duration,
    threshold: u32,
    clock: Box<dyn Fn() -> Instant + Send + Sync>,
    per_ip: HashMap<String, ScanState>,
}

struct ScanState {
    events: VecDeque<(Instant, u16)>,
    port_counts: HashMap<u16, u32>,
    total: u32,
}

pub enum ScanEmit {
    None,
    PortScan { source_ip: String, evidence: SecurityEvidence },
}

impl ScanDetector {
    pub fn observe(&mut self, source_ip: String, dst_port: u16) -> ScanEmit;
    pub fn record_blocked(&mut self, source_ip: &str);  // increments blocked_count from firewall log
    pub fn sweep(&mut self);  // expire entries
}
```

Tests:
- `distinct_ports_threshold_triggers` — 20 unique ports → fire
- `same_port_repeat_does_not_trigger` — port 22 × 50 → never
- `window_slide_drops_ports` — fill, advance time past window, verify port_counts empty
- `firewall_blocked_count_threads_through_evidence`

- [ ] **Step 2: Implement using `VecDeque<(Instant, u16)>` + `HashMap<u16, u32>` per spec §6.4**

On `observe`: push to deque, increment `port_counts.entry(port)`. Then call `expire_head` which pops entries `< now - window` and for each popped port decrements `port_counts`; remove key on hit 0.

- [ ] **Step 3: Run & commit**

```bash
cargo test -p serverbee-agent scan_detector
git add crates/agent/src/security/
git commit -m "feat(agent): add port-scan detector with proper window expiry"
```

---

### Task 1.6: Journal watcher (sshd stream + auth.log fallback)

**Files:**
- Create: `crates/agent/src/security/journal_watcher.rs`
- Modify: `crates/agent/src/security/mod.rs`
- Modify: `crates/agent/Cargo.toml` (add `notify`)

- [ ] **Step 1: Design**

```rust
pub async fn run_sshd_stream(out_tx: mpsc::Sender<AuthAttempt>) -> std::io::Result<()> {
    if has_journalctl().await {
        run_journalctl_sshd(out_tx).await
    } else {
        run_auth_log_tail(out_tx).await
    }
}
```

`run_journalctl_sshd`: spawn `journalctl -f --output=json -n 0 SYSLOG_IDENTIFIER=sshd + _SYSTEMD_UNIT=ssh.service + _SYSTEMD_UNIT=sshd.service + _COMM=sshd`, read line by line, parse `MESSAGE` field via `serde_json`, feed into `ssh_parser::parse_sshd_line`, send valid `AuthAttempt` to `out_tx`.

`run_auth_log_tail`: try `/var/log/auth.log` then `/var/log/secure`. Use `notify` crate to react to inode changes (logrotate). Read incrementally from last offset, emit line by line.

Both have an exponential-backoff retry loop on subprocess crash or file open failure.

- [ ] **Step 2: Tests (subprocess mocks via injected `Stdio` reader)**

Refactor `run_journalctl_sshd` to accept a `Box<dyn AsyncRead + Send + Unpin>` for the line source, then write a test that feeds canned journalctl JSON lines and asserts emitted `AuthAttempt`s.

- [ ] **Step 3: Implement, run tests, commit**

```bash
cargo test -p serverbee-agent journal_watcher
git add crates/agent/src/security/ crates/agent/Cargo.toml
git commit -m "feat(agent): add journal watcher with sshd stream and auth.log fallback"
```

---

### Task 1.7: Kernel firewall log stream (opt-in with scan detection)

**Files:**
- Modify: `crates/agent/src/security/journal_watcher.rs`

- [ ] **Step 1: Add `run_kernel_stream(blocked_tx: mpsc::Sender<String>)`**

Spawns `journalctl -k -f --output=json -n 0`. Filter `MESSAGE` for `[UFW BLOCK]`, `iptables: ` prefixes, `nftables`. Extract `SRC=` IP, emit on `blocked_tx`.

- [ ] **Step 2: Tests with canned input**

- [ ] **Step 3: Commit**

```bash
git add crates/agent/src/security/
git commit -m "feat(agent): add kernel firewall log stream for scan enrichment"
```

---

### Task 1.8: Conntrack watcher (netlink)

**Files:**
- Modify: `crates/agent/Cargo.toml` (add `netlink-sys`, `netlink-packet-netfilter`)
- Create: `crates/agent/src/security/conntrack_watcher.rs`
- Modify: `crates/agent/src/security/mod.rs`

- [ ] **Step 1: Cargo.toml additions**

```toml
netlink-sys = "0.8"
netlink-packet-core = "0.7"
netlink-packet-netfilter = "0.2"
```

(Pin to whichever maintained set works; verify with `cargo check` before committing.)

- [ ] **Step 2: Skeleton**

```rust
pub async fn run_conntrack_stream(events_tx: mpsc::Sender<ConntrackEvent>) -> std::io::Result<()> { ... }

pub struct ConntrackEvent { pub source_ip: String, pub dst_port: u16 }
```

Bind to `NETLINK_NETFILTER`, subscribe to `NF_NETLINK_CONNTRACK_NEW` group. On each NEW message: parse, filter TCP + SYN_SENT, emit.

- [ ] **Step 3: Test**

Write a `#[cfg(target_os = "linux")] #[ignore]` integration test that requires running as root and connects to localhost ports — verified on VPS during acceptance, not in CI.

- [ ] **Step 4: Bind-failure path**

When `bind()` errs (`EPERM` because we lack CAP_NET_ADMIN), return that error so SecurityManager can log a warning and disable only the scan watcher.

- [ ] **Step 5: Commit**

```bash
cargo build -p serverbee-agent
git add crates/agent/Cargo.toml crates/agent/src/security/ Cargo.lock
git commit -m "feat(agent): add conntrack watcher for port-scan detection"
```

---

### Task 1.9: SecurityManager wiring

**Files:**
- Create: `crates/agent/src/security/manager.rs`
- Modify: `crates/agent/src/security/mod.rs`
- Modify: `crates/agent/src/main.rs`
- Modify: `crates/agent/src/reporter.rs` (route SecurityEvent through the WS sender)

- [ ] **Step 1: SecurityManager struct**

```rust
pub struct SecurityManager {
    cfg: Arc<RwLock<SecurityConfig>>,
    tx: mpsc::Sender<AgentMessage>,
    handles: Vec<JoinHandle<()>>,
}

impl SecurityManager {
    pub async fn start(
        cfg: SecurityConfig,
        agent_caps: u32,
        tx: mpsc::Sender<AgentMessage>,
    ) -> anyhow::Result<Self> {
        // Capability self-check
        if !has_capability(agent_caps, CAP_SECURITY_EVENTS) {
            tracing::info!("CAP_SECURITY_EVENTS not granted locally; SecurityManager disabled");
            return Ok(Self { cfg: Arc::new(RwLock::new(cfg)), tx, handles: vec![] });
        }
        if !cfg.enabled || cfg!(not(target_os = "linux")) {
            return Ok(Self { cfg: Arc::new(RwLock::new(cfg)), tx, handles: vec![] });
        }

        // Start ssh + journal pipeline
        // Conditionally start conntrack pipeline if cfg.port_scan.enabled
        // ...
    }
}
```

Build the pipeline (SshDetector + ScanDetector + FirstSeenStore + JournalWatcher + ConntrackWatcher) using mpsc channels; on each emit, package as `SecurityEventPayload` and `tx.send(AgentMessage::SecurityEvent(payload)).await`.

- [ ] **Step 2: Wire into agent main**

In `crates/agent/src/main.rs` after reporter is set up, call `SecurityManager::start(config.security.clone(), local_caps, reporter.message_tx()).await?`. Keep the handle alive for the program lifetime.

- [ ] **Step 3: Smoke test on the VPS will exercise this; for unit testing, validate the disable paths**

Tests:
- when cap is off, `start` returns empty handles
- when target_os != linux, returns empty (test via `cfg!`)
- when port_scan.enabled = false, no conntrack handle

- [ ] **Step 4: cargo build**

```bash
cargo build --workspace
```
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add crates/agent/
git commit -m "feat(agent): wire SecurityManager into agent boot"
```

---

## Phase 2 — Server

### Task 2.1: Extend AlertRuleItem with SecurityRuleParams + validator

**Files:**
- Modify: `crates/server/src/service/alert.rs`

- [ ] **Step 1: Add types**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SecurityRuleParams {
    #[serde(default)] pub min_failed_count: Option<u32>,
    #[serde(default)] pub min_distinct_ports: Option<u32>,
    #[serde(default)] pub exclude_users: Vec<String>,
    #[serde(default)] pub exclude_cidrs: Vec<String>,
    #[serde(default = "default_dedupe_secs")] pub dedupe_window_seconds: u32,
}

fn default_dedupe_secs() -> u32 { 600 }
```

Add `#[serde(default)] pub security: Option<SecurityRuleParams>` to `AlertRuleItem`.

- [ ] **Step 2: Extend EVENT_DRIVEN_RULE_TYPES**

```rust
const EVENT_DRIVEN_RULE_TYPES: &[&str] = &[
    "ip_changed",
    "ssh_brute_force_detected",
    "ssh_new_ip_login",
    "port_scan_detected",
];
```

- [ ] **Step 3: Validator helper**

```rust
pub fn validate_alert_rule_items(items: &[AlertRuleItem]) -> Result<(), AppError> {
    let security_types: &[&str] = &["ssh_brute_force_detected", "ssh_new_ip_login", "port_scan_detected"];
    let security_count = items.iter().filter(|i| security_types.contains(&i.rule_type.as_str())).count();
    let non_security_count = items.len() - security_count;
    if security_count > 0 && non_security_count > 0 {
        return Err(AppError::BadRequest("cannot mix security rule types with other items".into()));
    }
    if security_count > 1 {
        return Err(AppError::BadRequest("only one security item per alert_rule is supported".into()));
    }
    Ok(())
}
```

Call from `create` and `update` paths.

- [ ] **Step 4: Tests**

Add tests covering: mixing rejected, multi-security rejected, single security accepted, all-metric accepted.

- [ ] **Step 5: Run, commit**

```bash
cargo test -p serverbee-server alert
git add crates/server/src/service/alert.rs
git commit -m "feat(server): extend AlertRuleItem with SecurityRuleParams and validator"
```

---

### Task 2.2: SecurityService — record_event with retries, broadcast, inline evaluation

**Files:**
- Create: `crates/server/src/service/security.rs`
- Modify: `crates/server/src/service/mod.rs`
- Modify: `crates/server/src/state.rs` (instantiate)

- [ ] **Step 1: Service shell**

```rust
pub struct SecurityService {
    db: DatabaseConnection,
    browser_tx: broadcast::Sender<BrowserMessage>,
    alert_state_manager: Arc<AlertStateManager>,
    config: Arc<AppConfig>,
}

impl SecurityService {
    pub async fn record_event(&self, server_id: &str, p: SecurityEventPayload) -> Result<String, AppError> {
        // 1. Validate source_ip parses as IpAddr
        // 2. JSON-encode evidence
        // 3. Build ActiveModel, insert with 3x retry (100ms/500ms/2s)
        // 4. Broadcast BrowserMessage::SecurityEvent
        // 5. Evaluate matching alert rules inline:
        //    load enabled rules with cover including this server_id,
        //    find single security item per rule (validator guarantees ≤1),
        //    match rule_type to event_type,
        //    apply SecurityRuleParams filters,
        //    read alert_state -> compute should_notify -> mark_triggered -> send_group if should_notify
        Ok(saved.id)
    }

    fn validate_source_ip(ip: &str) -> Result<(), AppError> { ... }
    fn matches_rule(item: &AlertRuleItem, p: &SecurityEventPayload) -> bool { ... }
}
```

- [ ] **Step 2: Add to AppState**

Add `pub security_service: Arc<SecurityService>` to `AppState`; instantiate in `AppState::new`.

- [ ] **Step 3: Tests**

```rust
#[tokio::test]
async fn record_event_persists_and_broadcasts() { ... }
#[tokio::test]
async fn record_event_rejects_malformed_ip() { ... }
#[tokio::test]
async fn record_event_triggers_matching_rule() { ... }
#[tokio::test]
async fn record_event_dedupes_within_window() { ... }
#[tokio::test]
async fn ssh_new_ip_login_only_fires_on_first_seen() { ... }
```

Use in-memory sqlite + `tokio::sync::broadcast::channel(16)` for browser_tx.

- [ ] **Step 4: cargo test**

Run: `cargo test -p serverbee-server security`
Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/ crates/server/src/state.rs
git commit -m "feat(server): add SecurityService with inline rule evaluation"
```

---

### Task 2.3: WS handler branch + capability check

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs`

- [ ] **Step 1: Add SecurityEvent arm to handle_agent_message**

Per spec §7.1 snippet — uses `agent_manager.get_effective_capabilities(server_id)` with fallback to a DB lookup.

- [ ] **Step 2: Audit log on denial**

Call `AuditService::log(&state.db, "system", "security_event_denied", Some(&detail), "").await.ok();`.

- [ ] **Step 3: Test**

Integration test:
- Open a fake WS, send SecurityEvent on a server whose capabilities mask includes 256 → row appears.
- Mask without 256 → no row, audit_log row appears.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/router/ws/agent.rs crates/server/tests/
git commit -m "feat(server): handle AgentMessage::SecurityEvent over WS"
```

---

### Task 2.4: recovery_merge addition

**Files:**
- Modify: `crates/server/src/service/recovery_merge.rs`

- [ ] **Step 1: Add merge call**

In `merge_server_history_on_connection`:
```rust
Self::merge_raw_table_on_connection(db, "security_event", "created_at", target_server_id, source_server_id).await?;
```

- [ ] **Step 2: Update test if there's a coverage assertion**

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/service/recovery_merge.rs
git commit -m "feat(server): include security_event in recovery merge"
```

---

### Task 2.5: Retention cleanup

**Files:**
- Modify: `crates/server/src/config.rs` (add `security_event_days: u32`)
- Modify: `crates/server/src/task/cleanup.rs`

- [ ] **Step 1: Config field**

```rust
#[serde(default = "default_30")]
pub security_event_days: u32,
```

Add into `RetentionConfig::default` literal too.

- [ ] **Step 2: Cleanup step**

```rust
let cutoff = Utc::now() - Duration::days(cfg.retention.security_event_days as i64);
security_event::Entity::delete_many()
    .filter(security_event::Column::CreatedAt.lt(cutoff))
    .exec(db).await?;
```

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/config.rs crates/server/src/task/cleanup.rs
git commit -m "feat(server): add security_event retention cleanup"
```

---

### Task 2.6: REST API

**Files:**
- Create: `crates/server/src/router/api/security.rs`
- Modify: `crates/server/src/router/api/mod.rs`

- [ ] **Step 1: Endpoints**

```
GET    /api/security/events          (cursor-paginated, filters)
GET    /api/security/events/:id
GET    /api/security/stats           (group_by event_type|source_ip|day)
DELETE /api/security/events/:id      (admin only)
```

All with `#[utoipa::path]`. Response shape: `Json<ApiResponse<...>>`.

- [ ] **Step 2: Plug into read_router + write_router**

```rust
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/security/events", get(list_events))
        .route("/api/security/events/:id", get(get_event))
        .route("/api/security/stats", get(stats))
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/security/events/:id", delete(delete_event))
        .layer(middleware::from_fn(require_admin))
}
```

Mount in `router/api/mod.rs`.

- [ ] **Step 3: Tests**

```rust
#[tokio::test] async fn list_events_filters_by_server_id() { ... }
#[tokio::test] async fn list_events_supports_cursor() { ... }
#[tokio::test] async fn delete_event_requires_admin() { ... }
#[tokio::test] async fn stats_groups_correctly() { ... }
```

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/router/api/
git commit -m "feat(server): add /api/security REST endpoints"
```

---

## Phase 3 — Frontend

### Task 3.1: Regenerate API types

**Files:**
- Modify: `apps/web/src/lib/api-types.gen.ts` (regenerated)

- [ ] **Step 1: From workspace root**

Run: `cargo run -p serverbee-server --bin dump_openapi -- > /tmp/openapi.json && cd apps/web && bun run generate:api-types`

(Verify the existing `dump_openapi` script's invocation matches local conventions; falls back to `bun run generate:api-types` if that's a one-step Makefile target.)

- [ ] **Step 2: Confirm SecurityEventPayload appears in types**

```bash
grep -i "SecurityEvent" apps/web/src/lib/api-types.gen.ts | head -5
```

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/lib/api-types.gen.ts
git commit -m "chore(web): regenerate api types for security events"
```

---

### Task 3.2: WS union arm + Zustand fan-out

**Files:**
- Modify: `apps/web/src/hooks/use-servers-ws.ts`

- [ ] **Step 1: Add union arm to WsMessage**

Add inside the `WsMessage` union:
```ts
| {
    type: 'security_event'
    server_id: string
    event_id: string
    event: components['schemas']['SecurityEventPayload']  // or local-typed equivalent
  }
```

- [ ] **Step 2: Handler in `onMessage`**

Per spec §8.3 snippet — invalidate stats query, prepend to events query cache, toast on `high`/`critical`.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/hooks/use-servers-ws.ts
git commit -m "feat(web): handle security_event WS messages"
```

---

### Task 3.3: Security routes + sidebar entry

**Files:**
- Create: `apps/web/src/routes/_authed/security/index.tsx`
- Create: `apps/web/src/routes/_authed/security/$serverId.tsx`
- Modify: `apps/web/src/components/app-sidebar.tsx` (or wherever nav items live; grep `Servers` route to find it)

- [ ] **Step 1: Skeleton routes**

Each route exports `Route = createFileRoute(...)({ component: ... })`. Body can start as a placeholder `<div>Security</div>` — content lands in 3.4.

- [ ] **Step 2: Sidebar entry**

Add `ShieldAlert` icon import + nav item.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/security/ apps/web/src/components/app-sidebar.tsx
git commit -m "feat(web): scaffold Security routes and sidebar entry"
```

---

### Task 3.4: Security overview page (KPI + chart + table)

**Files:**
- Modify: `apps/web/src/routes/_authed/security/index.tsx`
- Create: `apps/web/src/components/security/kpi-cards.tsx`
- Create: `apps/web/src/components/security/event-table.tsx`
- Create: `apps/web/src/components/security/event-detail-drawer.tsx`
- Create: `apps/web/src/components/security/timeline-chart.tsx`
- Create: `apps/web/src/hooks/use-security-events.ts`

- [ ] **Step 1: Hook**

```ts
export function useSecurityEvents(filters: SecurityEventFilters) {
  return useInfiniteQuery({
    queryKey: ['security', 'events', filters],
    queryFn: async ({ pageParam }) => api.get(`/api/security/events?${qs}&cursor=${pageParam}`),
    initialPageParam: '',
    getNextPageParam: (last) => last.next_cursor || undefined,
  });
}

export function useSecurityStats(filters) {
  return useQuery({ queryKey: ['security', 'stats', filters], queryFn: ... });
}
```

- [ ] **Step 2: KPI cards** — 4 cards: brute force count, port scans, new IP logins, top attacker.

- [ ] **Step 3: Timeline chart** — Recharts `BarChart` stacked by event_type.

- [ ] **Step 4: Event table** — TanStack Table wrapped in shadcn `<ScrollArea>`; row click opens drawer.

- [ ] **Step 5: Drawer** — show full evidence JSON + VirusTotal link.

- [ ] **Step 6: Run frontend lint + typecheck**

```bash
cd apps/web && bun run typecheck && bun x ultracite check
```

- [ ] **Step 7: Commit**

```bash
git add apps/web/
git commit -m "feat(web): security overview page with KPI / chart / table / drawer"
```

---

### Task 3.5: Server detail "Security" tab

**Files:**
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx` (or `.../$id/index.tsx` — confirm path)
- Modify: `apps/web/src/routes/_authed/security/$serverId.tsx` (full per-server view; reuse table component)

- [ ] **Step 1: Add tab**

Add a `<TabsTrigger value="security">` and `<TabsContent>` with the last 50 events plus a link to the full page.

- [ ] **Step 2: Commit**

```bash
git add apps/web/src/routes/_authed/
git commit -m "feat(web): add Security tab to server detail page"
```

---

### Task 3.6: Alert rule preset cards in Settings → Alerts

**Files:**
- Modify: `apps/web/src/routes/_authed/settings/alerts.tsx`
- Create: `apps/web/src/components/security/alert-presets.tsx`

- [ ] **Step 1: Three preset cards**

For each preset (`ssh_brute_force_detected`, `ssh_new_ip_login`, `port_scan_detected`): a card with title, description, the relevant `SecurityRuleParams` inputs, notification-group selector, Save button → POST to `/api/alert-rules`.

- [ ] **Step 2: i18n entries**

Add `apps/web/src/locales/{en,zh}/security.json` with ~30 strings (event type labels, severity, filter labels, empty-state copy, alert preset cards).

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/routes/ apps/web/src/components/ apps/web/src/locales/
git commit -m "feat(web): security alert preset cards and i18n"
```

---

## Phase 4 — Verification & Acceptance

### Task 4.1: Full local test suite

- [ ] `cargo test --workspace` — all green.
- [ ] `cargo clippy --workspace --tests -- -D warnings` — zero warnings.
- [ ] `cd apps/web && bun run typecheck && bun run test && bun x ultracite check` — green.
- [ ] If any failure, **fix root cause** (do not patch test). Commit fixes individually.

### Task 4.2: Build release artifacts

- [ ] `cargo build --release --workspace`
- [ ] `cd apps/web && bun run build` (frontend embeds into server binary via rust-embed in next build)
- [ ] `cargo build --release -p serverbee-server -p serverbee-agent`

Confirm the produced binaries:
- `target/release/serverbee-server`
- `target/release/serverbee-agent`

### Task 4.3: Manual E2E checklist authoring

**Files:** Create `tests/security-events.md` per spec §10.3.

Include the explicit preconditions for nmap test (`security.port_scan.enabled=true`, systemd unit `AmbientCapabilities=CAP_NET_RAW CAP_NET_ADMIN`, restart agent).

Commit the doc.

### Task 4.4: VPS acceptance test

Target VPS:
- Host: `<vps-host>`
- User: `root`
- Port: 22

**Sub-steps:**

- [ ] Copy `target/release/serverbee-agent` to the VPS (`scp`); install as systemd unit, point at a locally-running server (either via SSH local-forward `9527`, or run server on the VPS too).
- [ ] Verify SSH brute-force detection: from another box (or `localhost`), `for i in {1..15}; do sshpass -p wrong ssh -o StrictHostKeyChecking=no -o ConnectTimeout=2 root@vps true 2>/dev/null; done` — wait ≤ 90s, confirm a `ssh_brute_force` event in the UI / API.
- [ ] Verify SSH success + first_seen: `ssh -i ~/.ssh/known_key newuser@vps` — confirm `ssh_login` with `first_seen=true` and the configured `ssh_new_ip_login` rule fires.
- [ ] Verify port-scan detection: enable `security.port_scan.enabled=true`, add CAP_NET_ADMIN to the unit, restart agent; from another host run `nmap -p 1-1000 <vps-host>` and confirm `port_scan` event.
- [ ] Verify capability gate: toggle `CAP_SECURITY_EVENTS` off from the UI for this server; confirm watcher stops emitting (no new events) and an audit log row appears for any inbound event the server rejects.
- [ ] Verify recovery merge: drop the agent's local fingerprint, re-register the agent under a new server id, then merge — confirm the previously written `security_event` rows now reference the target server.

Capture results in a single comment on the open PR (or append to `tests/security-events.md`).

### Task 4.5: Final commit & summary

If any leftover unstaged formatting changes from review remain, `git add -A && git commit -m "chore: housekeeping"`. **Do not push** — the goal is local commits only.

Report:
- Total commits added in this branch.
- Test results.
- Known issues / phase-2 work explicitly deferred.

---

## File Structure Summary

**New files:**

```
crates/common/src/security.rs
crates/server/src/entity/security_event.rs
crates/server/src/migration/m20260521_001_create_security_event.rs
crates/server/src/migration/m20260521_002_extend_alert_state_event_key.rs
crates/server/src/migration/m20260521_003_backfill_capability_default.rs
crates/server/src/service/security.rs
crates/server/src/router/api/security.rs
crates/agent/src/security/mod.rs
crates/agent/src/security/manager.rs
crates/agent/src/security/ssh_parser.rs
crates/agent/src/security/ssh_detector.rs
crates/agent/src/security/first_seen_store.rs
crates/agent/src/security/scan_detector.rs
crates/agent/src/security/journal_watcher.rs
crates/agent/src/security/conntrack_watcher.rs
apps/web/src/routes/_authed/security/index.tsx
apps/web/src/routes/_authed/security/$serverId.tsx
apps/web/src/components/security/{kpi-cards,event-table,event-detail-drawer,timeline-chart,alert-presets}.tsx
apps/web/src/hooks/use-security-events.ts
apps/web/src/locales/{en,zh}/security.json
tests/security-events.md
```

**Modified files** (selection):

```
crates/common/src/constants.rs
crates/common/src/lib.rs
crates/common/src/agent_message.rs
crates/common/src/browser_message.rs
crates/common/Cargo.toml (utoipa already present)
crates/agent/src/main.rs
crates/agent/src/config.rs
crates/agent/src/reporter.rs
crates/agent/Cargo.toml (regex, once_cell, notify, netlink-* additions)
crates/server/src/state.rs
crates/server/src/config.rs
crates/server/src/entity/mod.rs
crates/server/src/entity/alert_state.rs
crates/server/src/migration/mod.rs
crates/server/src/router/api/mod.rs
crates/server/src/router/ws/agent.rs
crates/server/src/service/alert.rs
crates/server/src/service/recovery_merge.rs
crates/server/src/task/alert_evaluator.rs
crates/server/src/task/cleanup.rs
apps/web/src/hooks/use-servers-ws.ts
apps/web/src/components/app-sidebar.tsx
apps/web/src/routes/_authed/servers/$id.tsx
apps/web/src/routes/_authed/settings/alerts.tsx
apps/web/src/lib/api-types.gen.ts
ENV.md
apps/docs/content/docs/{en,cn}/configuration.mdx
```
