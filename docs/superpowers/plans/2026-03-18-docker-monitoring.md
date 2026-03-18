# Docker Monitoring Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Add Docker container monitoring and management to ServerBee — Agent connects to local Docker daemon via bollard, reports containers/stats/logs/events through the existing WebSocket channel, Server caches and broadcasts to the React frontend.

**Architecture:** Agent-side `DockerManager` (independent manager like PingManager) connects to Docker via bollard, sends data as `AgentMessage::Docker*` variants through the existing Agent→Server WebSocket. Server caches in `AgentManager`, broadcasts to browsers via `browser_tx`. Log streaming uses a dedicated WebSocket (same pattern as Terminal). Stats/events use viewer refcount via `DockerViewerTracker` to start/stop Agent-side polling on demand.

**Tech Stack:** Rust (bollard 0.18, tokio, sea-orm, axum 0.8), React 19 (TanStack Router/Query, shadcn/ui, Recharts), WebSocket

**Spec:** `docs/superpowers/specs/2026-03-18-docker-monitoring-design.md`

---

## File Structure

### New files

```
crates/common/src/docker_types.rs          — Docker data structures (DockerContainer, DockerContainerStats, etc.)

crates/agent/src/docker/
├── mod.rs                                 — DockerManager lifecycle, Docker client init, reconnect
├── containers.rs                          — Container list, stats polling, container actions
├── logs.rs                                — Log stream session management with batching
├── events.rs                              — Docker event stream
├── networks.rs                            — Network list query
└── volumes.rs                             — Volume list query

crates/server/src/entity/docker_event.rs   — sea-orm entity for docker_event table
crates/server/src/service/docker.rs        — DockerService (event save/query)
crates/server/src/service/docker_viewer.rs — DockerViewerTracker (viewer refcount)
crates/server/src/router/api/docker.rs     — Docker REST API endpoints
crates/server/src/router/ws/docker_logs.rs — Dedicated log WebSocket handler
crates/server/src/migration/m20260318_000006_docker_support.rs — Migration

apps/web/src/contexts/servers-ws-context.tsx                           — React context for WS send/connectionState
apps/web/src/routes/_authed/servers/$serverId/docker/
├── index.tsx                              — Docker Tab main page
├── types.ts                               — Frontend Docker types
├── hooks/
│   ├── use-docker-subscription.ts         — DockerSubscribe/Unsubscribe via global WS
│   └── use-docker-logs.ts                 — Dedicated WS for log streaming
└── components/
    ├── docker-overview.tsx                — Overview cards (running/stopped/CPU/mem/version)
    ├── container-list.tsx                 — Container table with search/filter
    ├── container-detail-dialog.tsx        — Detail Dialog (meta + stats + logs)
    ├── container-logs.tsx                 — Log terminal component
    ├── container-stats.tsx                — Stats mini cards
    ├── docker-events.tsx                  — Events timeline
    ├── docker-networks-dialog.tsx         — Networks list Dialog
    └── docker-volumes-dialog.tsx          — Volumes list Dialog
```

### Modified files

```
crates/common/src/lib.rs                   — export docker_types module
crates/common/src/protocol.rs              — add Docker variants to AgentMessage/ServerMessage/BrowserMessage
crates/common/src/constants.rs             — add CAP_DOCKER, update CAP_VALID_MASK
crates/common/src/types.rs                 — add features field to SystemInfo

crates/agent/Cargo.toml                    — add bollard dependency
crates/agent/src/main.rs or lib.rs         — export docker module
crates/agent/src/reporter.rs               — integrate DockerManager into tokio::select! loop

crates/server/src/entity/mod.rs            — export docker_event entity
crates/server/src/service/mod.rs           — export docker, docker_viewer modules
crates/server/src/service/agent_manager.rs — add Docker cache fields, log session routing, capabilities cache
crates/server/src/state.rs                 — async new() with preload, add docker_viewers field
crates/server/src/router/api/mod.rs        — register Docker routes
crates/server/src/router/ws/mod.rs         — register docker_logs WS route
crates/server/src/router/ws/browser.rs     — add BrowserClientMessage upstream handling
crates/server/src/router/ws/agent.rs       — handle Docker AgentMessage variants
crates/server/src/task/cleanup.rs          — add docker_event cleanup
crates/server/src/migration/mod.rs         — register new migration

apps/web/src/lib/ws-client.ts              — add send(), connectionState tracking
apps/web/src/hooks/use-servers-ws.ts        — add Docker message handlers (docker_update, docker_event, docker_availability_changed)
apps/web/src/routes/_authed.tsx            — wrap with ServersWsContext provider
```

---

## Task 1: Common — Docker Data Structures

**Files:**
- Create: `crates/common/src/docker_types.rs`
- Modify: `crates/common/src/lib.rs`

- [x] **Step 1: Write tests for Docker type serialization**

Create `crates/common/src/docker_types.rs` with the data structures and inline tests:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub created: i64,
    pub ports: Vec<DockerPort>,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerPort {
    pub private_port: u16,
    pub public_port: Option<u16>,
    pub port_type: String,
    pub ip: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerContainerStats {
    pub id: String,
    pub name: String,
    pub cpu_percent: f64,
    pub memory_usage: u64,
    pub memory_limit: u64,
    pub memory_percent: f64,
    pub network_rx: u64,
    pub network_tx: u64,
    pub block_read: u64,
    pub block_write: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerLogEntry {
    pub timestamp: Option<String>,
    pub stream: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerEventInfo {
    pub timestamp: i64,
    pub event_type: String,
    pub action: String,
    pub actor_id: String,
    pub actor_name: Option<String>,
    pub attributes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerSystemInfo {
    pub docker_version: String,
    pub api_version: String,
    pub os: String,
    pub arch: String,
    pub containers_running: i64,
    pub containers_paused: i64,
    pub containers_stopped: i64,
    pub images: i64,
    pub memory_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerNetwork {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub containers: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DockerVolume {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub created_at: Option<String>,
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DockerAction {
    Start,
    Stop { timeout: Option<i64> },
    Restart { timeout: Option<i64> },
    Remove { force: bool },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_container_serde() {
        let container = DockerContainer {
            id: "abc123".into(),
            name: "nginx".into(),
            image: "nginx:alpine".into(),
            state: "running".into(),
            status: "Up 3 hours".into(),
            created: 1710000000,
            ports: vec![DockerPort {
                private_port: 80,
                public_port: Some(8080),
                port_type: "tcp".into(),
                ip: Some("0.0.0.0".into()),
            }],
            labels: HashMap::new(),
        };
        let json = serde_json::to_string(&container).unwrap();
        let deserialized: DockerContainer = serde_json::from_str(&json).unwrap();
        assert_eq!(container, deserialized);
    }

    #[test]
    fn test_docker_action_serde() {
        let action = DockerAction::Stop { timeout: Some(10) };
        let json = serde_json::to_string(&action).unwrap();
        let deserialized: DockerAction = serde_json::from_str(&json).unwrap();
        assert_eq!(action, deserialized);
    }

    #[test]
    fn test_docker_log_entry_serde() {
        let entry = DockerLogEntry {
            timestamp: Some("2026-03-18T10:00:00Z".into()),
            stream: "stdout".into(),
            message: "Server started".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: DockerLogEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, deserialized);
    }
}
```

- [x] **Step 2: Export module from lib.rs**

In `crates/common/src/lib.rs`, add:
```rust
pub mod docker_types;
```

- [x] **Step 3: Run tests to verify**

Run: `cargo test -p serverbee-common docker`
Expected: 3 tests PASS

- [x] **Step 4: Commit**

```bash
git add crates/common/src/docker_types.rs crates/common/src/lib.rs
git commit -m "feat(common): add Docker data structures"
```

---

## Task 2: Common — Capability Constants & SystemInfo Features

**Files:**
- Modify: `crates/common/src/constants.rs`
- Modify: `crates/common/src/types.rs`

- [x] **Step 1: Add CAP_DOCKER constant**

In `crates/common/src/constants.rs`, add after `CAP_FILE`:
```rust
pub const CAP_DOCKER: u32 = 1 << 7; // 128
```

Update `CAP_VALID_MASK`:
```rust
pub const CAP_VALID_MASK: u32 = 0b1111_1111; // 255
```

Add to the `CAPABILITIES` array (the static metadata list used by the capabilities dialog):
```rust
CapabilityMeta {
    bit: CAP_DOCKER,
    key: "docker",
    display_name: "Docker Management",
    default_enabled: false,
    risk_level: "high",
},
```

- [x] **Step 2: Add features field to SystemInfo**

In `crates/common/src/types.rs`, add to `SystemInfo`:
```rust
#[serde(default)]
pub features: Vec<String>,
```

- [x] **Step 3: Bump PROTOCOL_VERSION**

In `crates/common/src/constants.rs` (or wherever `PROTOCOL_VERSION` is defined), bump:
```rust
pub const PROTOCOL_VERSION: u32 = 3;
```

- [x] **Step 4: Write test for CAP_DOCKER**

Add test in `crates/common/src/constants.rs`:
```rust
#[test]
fn test_cap_docker() {
    assert_eq!(CAP_DOCKER, 128);
    assert!(has_capability(CAP_DOCKER, CAP_DOCKER));
    assert!(!has_capability(CAP_DEFAULT, CAP_DOCKER)); // not in default
    assert!(CAP_DOCKER & CAP_VALID_MASK != 0); // valid
}
```

- [x] **Step 5: Write test for SystemInfo features serde**

Add test in `crates/common/src/types.rs`:
```rust
#[test]
fn test_system_info_features_default() {
    // Old agents don't send features — must default to empty
    let json = r#"{"cpu_name":"x86","cpu_cores":4,"cpu_arch":"x86_64","os":"linux","kernel_version":"6.1","mem_total":8000,"swap_total":4000,"disk_total":50000,"agent_version":"0.4.0","protocol_version":2}"#;
    let info: SystemInfo = serde_json::from_str(json).unwrap();
    assert!(info.features.is_empty());
}

#[test]
fn test_system_info_features_present() {
    let json = r#"{"cpu_name":"x86","cpu_cores":4,"cpu_arch":"x86_64","os":"linux","kernel_version":"6.1","mem_total":8000,"swap_total":4000,"disk_total":50000,"agent_version":"0.4.0","protocol_version":3,"features":["docker"]}"#;
    let info: SystemInfo = serde_json::from_str(json).unwrap();
    assert_eq!(info.features, vec!["docker"]);
}
```

- [x] **Step 6: Run tests**

Run: `cargo test -p serverbee-common`
Expected: All tests PASS

- [x] **Step 7: Commit**

```bash
git add crates/common/src/constants.rs crates/common/src/types.rs
git commit -m "feat(common): add CAP_DOCKER capability and SystemInfo features field"
```

---

## Task 3: Common — Protocol Messages

**Files:**
- Modify: `crates/common/src/protocol.rs`

- [x] **Step 1: Add Docker variants to AgentMessage**

Add to the `AgentMessage` enum in `crates/common/src/protocol.rs`:
```rust
DockerInfo {
    msg_id: Option<String>,
    info: DockerSystemInfo,
},
DockerContainers {
    msg_id: Option<String>,
    containers: Vec<DockerContainer>,
},
DockerStats {
    stats: Vec<DockerContainerStats>,
},
DockerLog {
    session_id: String,
    entries: Vec<DockerLogEntry>,
},
DockerEvent {
    event: DockerEventInfo,
},
FeaturesUpdate {
    features: Vec<String>,
},
DockerUnavailable,
DockerNetworks {
    msg_id: String,
    networks: Vec<DockerNetwork>,
},
DockerVolumes {
    msg_id: String,
    volumes: Vec<DockerVolume>,
},
DockerActionResult {
    msg_id: String,
    success: bool,
    error: Option<String>,
},
```

Add the necessary `use` import at the top:
```rust
use crate::docker_types::*;
```

- [x] **Step 2: Add Docker variants to ServerMessage**

Add to the `ServerMessage` enum:
```rust
DockerListContainers { msg_id: String },
DockerStartStats { interval_secs: u32 },
DockerStopStats,
DockerContainerAction {
    msg_id: String,
    container_id: String,
    action: DockerAction,
},
DockerLogsStart {
    session_id: String,
    container_id: String,
    tail: Option<u64>,
    follow: bool,
},
DockerLogsStop { session_id: String },
DockerEventsStart,
DockerEventsStop,
DockerGetInfo { msg_id: String },
DockerListNetworks { msg_id: String },
DockerListVolumes { msg_id: String },
```

- [x] **Step 3: Add Docker variants to BrowserMessage**

Add to the `BrowserMessage` enum:
```rust
DockerUpdate {
    server_id: String,
    containers: Vec<DockerContainer>,
    stats: Option<Vec<DockerContainerStats>>,
},
DockerEvent {
    server_id: String,
    event: DockerEventInfo,
},
DockerAvailabilityChanged {
    server_id: String,
    available: bool,
},
```

- [x] **Step 4: Add BrowserClientMessage enum**

Add a new enum for browser→server messages (same file):
```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserClientMessage {
    DockerSubscribe { server_id: String },
    DockerUnsubscribe { server_id: String },
}
```

- [x] **Step 5: Write serialization tests**

Add tests in `protocol.rs`:
```rust
#[test]
fn test_docker_agent_message_serde() {
    let msg = AgentMessage::DockerInfo {
        msg_id: None,
        info: DockerSystemInfo {
            docker_version: "27.1.1".into(),
            api_version: "1.46".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            containers_running: 5,
            containers_paused: 0,
            containers_stopped: 2,
            images: 10,
            memory_total: 8_000_000_000,
        },
    };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"docker_info\""));
    let deserialized: AgentMessage = serde_json::from_str(&json).unwrap();
    // Verify round-trip
}

#[test]
fn test_docker_server_message_serde() {
    let msg = ServerMessage::DockerStartStats { interval_secs: 3 };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"docker_start_stats\""));
}

#[test]
fn test_browser_client_message_serde() {
    let json = r#"{"type":"docker_subscribe","server_id":"abc123"}"#;
    let msg: BrowserClientMessage = serde_json::from_str(json).unwrap();
    match msg {
        BrowserClientMessage::DockerSubscribe { server_id } => assert_eq!(server_id, "abc123"),
        _ => panic!("wrong variant"),
    }
}

#[test]
fn test_features_update_serde() {
    let msg = AgentMessage::FeaturesUpdate { features: vec!["docker".into()] };
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"features_update\""));
    let deserialized: AgentMessage = serde_json::from_str(&json).unwrap();
}

#[test]
fn test_docker_unavailable_serde() {
    let msg = AgentMessage::DockerUnavailable;
    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"docker_unavailable\""));
}
```

- [x] **Step 6: Run tests**

Run: `cargo test -p serverbee-common`
Expected: All tests PASS

- [x] **Step 7: Commit**

```bash
git add crates/common/src/protocol.rs
git commit -m "feat(common): add Docker protocol message variants"
```

---

## Task 4: Server — Database Migration

**Files:**
- Create: `crates/server/src/migration/m20260318_000006_docker_support.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [x] **Step 1: Create migration file**

Create `crates/server/src/migration/m20260318_000006_docker_support.rs`:

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create docker_event table
        manager
            .create_table(
                Table::create()
                    .table(DockerEvent::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(DockerEvent::Id).integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(DockerEvent::ServerId).string().not_null())
                    .col(ColumnDef::new(DockerEvent::Timestamp).big_integer().not_null())
                    .col(ColumnDef::new(DockerEvent::EventType).string().not_null())
                    .col(ColumnDef::new(DockerEvent::Action).string().not_null())
                    .col(ColumnDef::new(DockerEvent::ActorId).string().not_null())
                    .col(ColumnDef::new(DockerEvent::ActorName).string().null())
                    .col(ColumnDef::new(DockerEvent::Attributes).text().null())
                    .col(ColumnDef::new(DockerEvent::CreatedAt).timestamp().default(Expr::current_timestamp()))
                    .to_owned(),
            )
            .await?;

        // Create index for querying events by server and time
        manager
            .create_index(
                Index::create()
                    .name("idx_docker_event_server_time")
                    .table(DockerEvent::Table)
                    .col(DockerEvent::ServerId)
                    .col(DockerEvent::Timestamp)
                    .to_owned(),
            )
            .await?;

        // Add features column to servers table
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("servers"))
                    .add_column(ColumnDef::new(Alias::new("features")).text().default("[]"))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(DockerEvent::Table).to_owned()).await?;
        // SQLite doesn't support DROP COLUMN easily; skip for down migration
        Ok(())
    }
}

#[derive(Iden)]
enum DockerEvent {
    Table,
    Id,
    ServerId,
    Timestamp,
    EventType,
    Action,
    ActorId,
    ActorName,
    Attributes,
    CreatedAt,
}
```

- [x] **Step 2: Register migration in mod.rs**

In `crates/server/src/migration/mod.rs`, add:
```rust
mod m20260318_000006_docker_support;
```

And add to the `Migrator::migrations()` vec:
```rust
Box::new(m20260318_000006_docker_support::Migration),
```

- [x] **Step 3: Add `features` field to server entity**

In `crates/server/src/entity/server.rs`, add the `features` column to the `Model` struct:
```rust
pub features: String,  // JSON array string, default "[]"
```

This matches the `features` column added by the migration (Step 1). The field stores a JSON-encoded `Vec<String>` (e.g., `["docker"]`).

- [x] **Step 4: Verify compilation**

Run: `cargo build -p serverbee-server`
Expected: compiles without errors

- [x] **Step 5: Commit**

```bash
git add crates/server/src/migration/ crates/server/src/entity/server.rs
git commit -m "feat(server): add Docker database migration and features entity field"
```

---

## Task 5: Server — Docker Event Entity & Service

**Files:**
- Create: `crates/server/src/entity/docker_event.rs`
- Modify: `crates/server/src/entity/mod.rs`
- Create: `crates/server/src/service/docker.rs`
- Modify: `crates/server/src/service/mod.rs`

- [x] **Step 1: Create sea-orm entity**

Create `crates/server/src/entity/docker_event.rs`:
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "docker_event")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub server_id: String,
    pub timestamp: i64,
    pub event_type: String,
    pub action: String,
    pub actor_id: String,
    pub actor_name: Option<String>,
    pub attributes: Option<String>,
    pub created_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [x] **Step 2: Export entity**

In `crates/server/src/entity/mod.rs`, add:
```rust
pub mod docker_event;
```

- [x] **Step 3: Create DockerService**

Create `crates/server/src/service/docker.rs`:
```rust
use sea_orm::*;
use crate::entity::docker_event;
use serverbee_common::docker_types::DockerEventInfo;

pub struct DockerService;

impl DockerService {
    pub async fn save_event(
        db: &DatabaseConnection,
        server_id: &str,
        event: &DockerEventInfo,
    ) -> Result<(), DbErr> {
        let model = docker_event::ActiveModel {
            server_id: Set(server_id.to_string()),
            timestamp: Set(event.timestamp),
            event_type: Set(event.event_type.clone()),
            action: Set(event.action.clone()),
            actor_id: Set(event.actor_id.clone()),
            actor_name: Set(event.actor_name.clone()),
            attributes: Set(Some(serde_json::to_string(&event.attributes).unwrap_or_default())),
            ..Default::default()
        };
        docker_event::Entity::insert(model).exec(db).await?;
        Ok(())
    }

    pub async fn get_events(
        db: &DatabaseConnection,
        server_id: &str,
        limit: u64,
    ) -> Result<Vec<DockerEventInfo>, DbErr> {
        let events = docker_event::Entity::find()
            .filter(docker_event::Column::ServerId.eq(server_id))
            .order_by_desc(docker_event::Column::Timestamp)
            .limit(limit)
            .all(db)
            .await?;

        Ok(events.into_iter().map(|e| DockerEventInfo {
            timestamp: e.timestamp,
            event_type: e.event_type,
            action: e.action,
            actor_id: e.actor_id,
            actor_name: e.actor_name,
            attributes: e.attributes
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
        }).collect())
    }

    pub async fn cleanup_expired(
        db: &DatabaseConnection,
        retention_days: u32,
    ) -> Result<u64, DbErr> {
        let cutoff = chrono::Utc::now().timestamp() - (retention_days as i64 * 86400);
        let result = docker_event::Entity::delete_many()
            .filter(docker_event::Column::Timestamp.lt(cutoff))
            .exec(db)
            .await?;
        Ok(result.rows_affected)
    }
}
```

- [x] **Step 4: Export service**

In `crates/server/src/service/mod.rs`, add:
```rust
pub mod docker;
```

- [x] **Step 5: Verify compilation**

Run: `cargo build -p serverbee-server`
Expected: compiles

- [x] **Step 6: Commit**

```bash
git add crates/server/src/entity/docker_event.rs crates/server/src/entity/mod.rs \
  crates/server/src/service/docker.rs crates/server/src/service/mod.rs
git commit -m "feat(server): add Docker event entity and service"
```

---

## Task 6: Server — DockerViewerTracker

**Files:**
- Create: `crates/server/src/service/docker_viewer.rs`
- Modify: `crates/server/src/service/mod.rs`

- [x] **Step 1: Write failing tests**

Create `crates/server/src/service/docker_viewer.rs` with tests first:

```rust
use dashmap::DashMap;
use std::collections::HashSet;

pub struct DockerViewerTracker {
    viewers: DashMap<String, HashSet<String>>,
}

impl DockerViewerTracker {
    pub fn new() -> Self {
        Self { viewers: DashMap::new() }
    }

    pub fn add_viewer(&self, server_id: &str, connection_id: &str) -> bool {
        let mut set = self.viewers.entry(server_id.to_string()).or_default();
        let was_empty = set.is_empty();
        set.insert(connection_id.to_string());
        was_empty
    }

    pub fn remove_viewer(&self, server_id: &str, connection_id: &str) -> bool {
        if let Some(mut set) = self.viewers.get_mut(server_id) {
            set.remove(connection_id);
            if set.is_empty() {
                drop(set);
                self.viewers.remove(server_id);
                return true;
            }
        }
        false
    }

    pub fn has_viewers(&self, server_id: &str) -> bool {
        self.viewers.get(server_id).map_or(false, |set| !set.is_empty())
    }

    pub fn remove_all_for_connection(&self, connection_id: &str) -> Vec<(String, bool)> {
        let server_ids: Vec<String> = self.viewers.iter()
            .filter(|entry| entry.value().contains(connection_id))
            .map(|entry| entry.key().clone())
            .collect();
        let mut results = Vec::new();
        for server_id in server_ids {
            let is_last = self.remove_viewer(&server_id, connection_id);
            results.push((server_id, is_last));
        }
        results
    }

    pub fn remove_all_for_server(&self, server_id: &str) -> bool {
        self.viewers.remove(server_id).map_or(false, |(_, set)| !set.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_viewer() {
        let tracker = DockerViewerTracker::new();
        assert!(tracker.add_viewer("srv1", "conn1")); // first → true
        assert!(!tracker.add_viewer("srv1", "conn2")); // not first → false
    }

    #[test]
    fn test_last_viewer() {
        let tracker = DockerViewerTracker::new();
        tracker.add_viewer("srv1", "conn1");
        tracker.add_viewer("srv1", "conn2");
        assert!(!tracker.remove_viewer("srv1", "conn1")); // not last
        assert!(tracker.remove_viewer("srv1", "conn2")); // last → true
    }

    #[test]
    fn test_has_viewers() {
        let tracker = DockerViewerTracker::new();
        assert!(!tracker.has_viewers("srv1"));
        tracker.add_viewer("srv1", "conn1");
        assert!(tracker.has_viewers("srv1"));
    }

    #[test]
    fn test_remove_all_for_connection() {
        let tracker = DockerViewerTracker::new();
        tracker.add_viewer("srv1", "conn1");
        tracker.add_viewer("srv2", "conn1");
        tracker.add_viewer("srv2", "conn2");

        let affected = tracker.remove_all_for_connection("conn1");
        assert_eq!(affected.len(), 2);
        // srv1 should be last (conn1 was only viewer)
        assert!(affected.iter().any(|(id, last)| id == "srv1" && *last));
        // srv2 should not be last (conn2 still there)
        assert!(affected.iter().any(|(id, last)| id == "srv2" && !*last));
    }

    #[test]
    fn test_remove_all_for_server() {
        let tracker = DockerViewerTracker::new();
        tracker.add_viewer("srv1", "conn1");
        tracker.add_viewer("srv1", "conn2");
        assert!(tracker.remove_all_for_server("srv1")); // had viewers
        assert!(!tracker.has_viewers("srv1"));
        assert!(!tracker.remove_all_for_server("srv1")); // already empty
    }
}
```

- [x] **Step 2: Export module**

In `crates/server/src/service/mod.rs`, add:
```rust
pub mod docker_viewer;
```

- [x] **Step 3: Run tests**

Run: `cargo test -p serverbee-server docker_viewer`
Expected: 5 tests PASS

- [x] **Step 4: Commit**

```bash
git add crates/server/src/service/docker_viewer.rs crates/server/src/service/mod.rs
git commit -m "feat(server): add DockerViewerTracker with viewer refcount"
```

---

## Task 7: Server — AgentManager Extensions

**Files:**
- Modify: `crates/server/src/service/agent_manager.rs`

- [x] **Step 1: Add Docker fields to AgentManager**

Add new fields to the `AgentManager` struct:
```rust
docker_containers: DashMap<String, Vec<DockerContainer>>,
docker_stats: DashMap<String, Vec<DockerContainerStats>>,
docker_info: DashMap<String, DockerSystemInfo>,
features: DashMap<String, Vec<String>>,
capabilities: DashMap<String, u32>,
docker_log_sessions: DashMap<String, DashMap<String, mpsc::Sender<Vec<DockerLogEntry>>>>,
```

Add the necessary imports and initialize in `new()`.

- [x] **Step 2: Add Docker cache methods**

```rust
// Docker container/stats/info cache
pub fn update_docker_containers(&self, server_id: &str, containers: Vec<DockerContainer>) {
    self.docker_containers.insert(server_id.to_string(), containers);
}

pub fn get_docker_containers(&self, server_id: &str) -> Option<Vec<DockerContainer>> {
    self.docker_containers.get(server_id).map(|v| v.clone())
}

pub fn update_docker_stats(&self, server_id: &str, stats: Vec<DockerContainerStats>) {
    self.docker_stats.insert(server_id.to_string(), stats);
}

pub fn get_docker_stats(&self, server_id: &str) -> Option<Vec<DockerContainerStats>> {
    self.docker_stats.get(server_id).map(|v| v.clone())
}

pub fn update_docker_info(&self, server_id: &str, info: DockerSystemInfo) {
    self.docker_info.insert(server_id.to_string(), info);
}

pub fn get_docker_info(&self, server_id: &str) -> Option<DockerSystemInfo> {
    self.docker_info.get(server_id).map(|v| v.clone())
}

pub fn clear_docker_caches(&self, server_id: &str) {
    self.docker_containers.remove(server_id);
    self.docker_stats.remove(server_id);
    self.docker_info.remove(server_id);
}
```

- [x] **Step 3: Add features cache methods**

```rust
pub fn update_features(&self, server_id: &str, features: Vec<String>) {
    self.features.insert(server_id.to_string(), features);
}

pub fn has_feature(&self, server_id: &str, feature: &str) -> bool {
    self.features.get(server_id)
        .map_or(false, |f| f.contains(&feature.to_string()))
}
```

- [x] **Step 4: Add capabilities cache methods**

```rust
pub fn update_capabilities(&self, server_id: &str, caps: u32) {
    self.capabilities.insert(server_id.to_string(), caps);
}

pub fn has_docker_capability(&self, server_id: &str) -> bool {
    self.capabilities.get(server_id)
        .map_or(false, |cap| has_capability(*cap, CAP_DOCKER))
}

pub async fn preload_capabilities(&self, db: &DatabaseConnection) -> Result<(), DbErr> {
    // Query all servers' capabilities from DB
    let servers = server::Entity::find()
        .select_only()
        .column(server::Column::Id)
        .column(server::Column::Capabilities)
        .all(db)
        .await?;
    for s in servers {
        self.capabilities.insert(s.id, s.capabilities as u32);
    }
    Ok(())
}
```

- [x] **Step 5: Add log session routing methods**

```rust
pub fn add_docker_log_session(
    &self,
    server_id: &str,
    session_id: String,
    tx: mpsc::Sender<Vec<DockerLogEntry>>,
) {
    self.docker_log_sessions
        .entry(server_id.to_string())
        .or_default()
        .insert(session_id, tx);
}

pub fn get_docker_log_session(
    &self,
    server_id: &str,
    session_id: &str,
) -> Option<mpsc::Sender<Vec<DockerLogEntry>>> {
    self.docker_log_sessions
        .get(server_id)?
        .get(session_id)
        .map(|tx| tx.clone())
}

pub fn remove_docker_log_session(&self, server_id: &str, session_id: &str) -> bool {
    if let Some(inner) = self.docker_log_sessions.get(server_id) {
        return inner.remove(session_id).is_some();
    }
    false
}

pub fn remove_docker_log_sessions_for_server(&self, server_id: &str) -> Vec<String> {
    if let Some((_, inner)) = self.docker_log_sessions.remove(server_id) {
        inner.into_iter().map(|(id, _)| id).collect()
    } else {
        vec![]
    }
}
```

- [x] **Step 6: Add send_docker_command helper**

This can be a free function or a method. Add near the AgentManager impl:
```rust
pub async fn send_docker_command(
    agent_manager: &AgentManager,
    server_id: &str,
    msg: ServerMessage,
) -> Result<(), AppError> {
    if !agent_manager.has_feature(server_id, "docker") {
        return Err(AppError::Conflict("Docker is not available on this server".into()));
    }
    agent_manager
        .get_sender(server_id)
        .ok_or_else(|| AppError::NotFound("Agent not connected".into()))?
        .send(msg)
        .await
        .map_err(|_| AppError::Internal("Failed to send to agent".into()))
}
```

Note: The existing `AppError` enum already has `Conflict` (409) — use it instead of adding a new variant.

- [x] **Step 7: Run compilation check**

Run: `cargo build -p serverbee-server`
Expected: compiles

- [x] **Step 8: Commit**

```bash
git add crates/server/src/service/agent_manager.rs
git commit -m "feat(server): extend AgentManager with Docker caches and log session routing"
```

---

## Task 8: Server — AppState & Startup Changes

**Files:**
- Modify: `crates/server/src/state.rs`

- [x] **Step 1: Add docker_viewers to AppState**

Add field:
```rust
pub docker_viewers: DockerViewerTracker,
```

Initialize in `new()`:
```rust
docker_viewers: DockerViewerTracker::new(),
```

- [x] **Step 2: Make AppState::new async with capability preload**

Change signature from `pub fn new(...)` to `pub async fn new(...) -> Result<Arc<Self>, anyhow::Error>`.

After `agent_manager` creation, add:
```rust
agent_manager.preload_capabilities(&db).await?;
```

Update return to `Ok(Arc::new(Self { ... }))`.

- [x] **Step 3: Update caller in main.rs**

In `crates/server/src/main.rs`, change `AppState::new(db, config)` to `AppState::new(db, config).await?`.

- [x] **Step 4: Verify compilation**

Run: `cargo build -p serverbee-server`
Expected: compiles

- [x] **Step 5: Commit**

```bash
git add crates/server/src/state.rs crates/server/src/main.rs
git commit -m "feat(server): add DockerViewerTracker to AppState, async startup with capability preload"
```

---

## Task 9: Server — Docker REST API

**Files:**
- Create: `crates/server/src/router/api/docker.rs`
- Modify: `crates/server/src/router/api/mod.rs`

- [x] **Step 1: Create Docker API routes**

Create `crates/server/src/router/api/docker.rs`:

```rust
use axum::{extract::{Path, State, Query}, routing::get, Router, Json};
use std::sync::Arc;
use crate::{state::AppState, error::{AppError, ok, ApiResult}};
use serverbee_common::docker_types::*;

// Guard function: check both CAP_DOCKER and docker feature
fn require_docker(state: &AppState, server_id: &str) -> Result<(), AppError> {
    if !state.agent_manager.has_docker_capability(server_id) {
        return Err(AppError::Forbidden("CAP_DOCKER is not enabled".into()));
    }
    if !state.agent_manager.has_feature(server_id, "docker") {
        return Err(AppError::Conflict("Docker is not available on this server".into()));
    }
    Ok(())
}

async fn get_containers(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> ApiResult<Vec<DockerContainer>> {
    require_docker(&state, &server_id)?;
    let containers = state.agent_manager.get_docker_containers(&server_id)
        .unwrap_or_default();
    ok(containers)
}

async fn get_stats(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> ApiResult<Vec<DockerContainerStats>> {
    require_docker(&state, &server_id)?;
    let stats = state.agent_manager.get_docker_stats(&server_id)
        .unwrap_or_default();
    ok(stats)
}

async fn get_info(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> ApiResult<Option<DockerSystemInfo>> {
    require_docker(&state, &server_id)?;
    ok(state.agent_manager.get_docker_info(&server_id))
}

async fn get_events(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
    Query(params): Query<EventsQuery>,
) -> ApiResult<Vec<DockerEventInfo>> {
    require_docker(&state, &server_id)?;
    let events = crate::service::docker::DockerService::get_events(
        &state.db, &server_id, params.limit.unwrap_or(100),
    ).await.map_err(AppError::from)?;
    ok(events)
}

#[derive(serde::Deserialize)]
struct EventsQuery {
    limit: Option<u64>,
}

// Request-response endpoints (network, volume) use pending_requests pattern.
// NOTE: `register_pending_request(msg_id)` creates the oneshot internally and returns the Receiver.
// Use `get_sender()` + `.send().await` to send to the agent (there is no `send_to_agent` method).
async fn get_networks(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> ApiResult<Vec<DockerNetwork>> {
    require_docker(&state, &server_id)?;
    let msg_id = uuid::Uuid::new_v4().to_string();
    let rx = state.agent_manager.register_pending_request(msg_id.clone());
    let tx = state.agent_manager.get_sender(&server_id)
        .ok_or_else(|| AppError::NotFound("Agent not connected".into()))?;
    tx.send(ServerMessage::DockerListNetworks { msg_id }).await
        .map_err(|_| AppError::Internal("Failed to send to agent".into()))?;
    match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
        Ok(Ok(AgentMessage::DockerNetworks { networks, .. })) => ok(networks),
        _ => Err(AppError::RequestTimeout("Docker networks request timed out".into())),
    }
}

async fn get_volumes(
    State(state): State<Arc<AppState>>,
    Path(server_id): Path<String>,
) -> ApiResult<Vec<DockerVolume>> {
    require_docker(&state, &server_id)?;
    let msg_id = uuid::Uuid::new_v4().to_string();
    let rx = state.agent_manager.register_pending_request(msg_id.clone());
    let tx = state.agent_manager.get_sender(&server_id)
        .ok_or_else(|| AppError::NotFound("Agent not connected".into()))?;
    tx.send(ServerMessage::DockerListVolumes { msg_id }).await
        .map_err(|_| AppError::Internal("Failed to send to agent".into()))?;
    match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
        Ok(Ok(AgentMessage::DockerVolumes { volumes, .. })) => ok(volumes),
        _ => Err(AppError::RequestTimeout("Docker volumes request timed out".into())),
    }
}

#[derive(serde::Deserialize)]
struct ActionBody {
    action: DockerAction,
}

async fn container_action(
    State(state): State<Arc<AppState>>,
    Path((server_id, container_id)): Path<(String, String)>,
    Json(body): Json<ActionBody>,
) -> ApiResult<serde_json::Value> {
    require_docker(&state, &server_id)?;
    let msg_id = uuid::Uuid::new_v4().to_string();
    let rx = state.agent_manager.register_pending_request(msg_id.clone());
    let tx = state.agent_manager.get_sender(&server_id)
        .ok_or_else(|| AppError::NotFound("Agent not connected".into()))?;
    tx.send(ServerMessage::DockerContainerAction {
        msg_id,
        container_id,
        action: body.action,
    }).await.map_err(|_| AppError::Internal("Failed to send to agent".into()))?;
    match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(AgentMessage::DockerActionResult { success, error, .. })) => {
            if success {
                ok(serde_json::json!({ "success": true }))
            } else {
                Err(AppError::BadRequest(error.unwrap_or_default()))
            }
        }
        _ => Err(AppError::RequestTimeout("Docker action timed out".into())),
    }
}

/// Read routes — authenticated users (no admin required)
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/servers/{id}/docker/containers", get(get_containers))
        .route("/api/servers/{id}/docker/stats", get(get_stats))
        .route("/api/servers/{id}/docker/info", get(get_info))
        .route("/api/servers/{id}/docker/events", get(get_events))
        .route("/api/servers/{id}/docker/networks", get(get_networks))
        .route("/api/servers/{id}/docker/volumes", get(get_volumes))
}

/// Write routes — admin only
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/servers/{id}/docker/containers/{cid}/action", axum::routing::post(container_action))
}
```

- [x] **Step 2: Register routes in api/mod.rs**

In `crates/server/src/router/api/mod.rs`, add:
```rust
mod docker;
```

In the router builder, add `docker::read_router()` to authenticated routes and `docker::write_router()` to admin routes.

- [x] **Step 3: Verify compilation**

Run: `cargo build -p serverbee-server`
Expected: compiles

- [x] **Step 4: Commit**

```bash
git add crates/server/src/router/api/docker.rs crates/server/src/router/api/mod.rs
git commit -m "feat(server): add Docker REST API endpoints"
```

---

## Task 10: Server — Agent WS Docker Message Handling

**Files:**
- Modify: `crates/server/src/router/ws/agent.rs`

- [x] **Step 1: Handle DockerInfo message**

In `handle_agent_message()`, add match arms. Note: the function signature is `fn handle_agent_message(state, server_id, msg: AgentMessage)`. Follow the existing codebase pattern: use `ref msg_id` to borrow and `msg.clone()` when passing to `dispatch_pending_response`.

```rust
AgentMessage::DockerInfo { ref msg_id, info } => {
    state.agent_manager.update_docker_info(&server_id, info);
    if let Some(msg_id) = msg_id {
        state.agent_manager.dispatch_pending_response(msg_id, msg.clone());
    }
    state.agent_manager.broadcast_browser(BrowserMessage::DockerAvailabilityChanged {
        server_id: server_id.clone(),
        available: true,
    });
}
```

- [x] **Step 2: Handle DockerContainers and DockerStats**

```rust
AgentMessage::DockerContainers { ref msg_id, containers } => {
    state.agent_manager.update_docker_containers(&server_id, containers.clone());
    if let Some(msg_id) = msg_id {
        state.agent_manager.dispatch_pending_response(msg_id, msg.clone());
    }
    let stats = state.agent_manager.get_docker_stats(&server_id);
    state.agent_manager.broadcast_browser(BrowserMessage::DockerUpdate {
        server_id: server_id.clone(),
        containers,
        stats,
    });
}

AgentMessage::DockerStats { stats } => {
    state.agent_manager.update_docker_stats(&server_id, stats.clone());
    if let Some(containers) = state.agent_manager.get_docker_containers(&server_id) {
        state.agent_manager.broadcast_browser(BrowserMessage::DockerUpdate {
            server_id: server_id.clone(),
            containers,
            stats: Some(stats),
        });
    }
}
```

- [x] **Step 3: Handle DockerLog, DockerEvent, DockerUnavailable**

```rust
AgentMessage::DockerLog { session_id, entries } => {
    if let Some(tx) = state.agent_manager.get_docker_log_session(&server_id, &session_id) {
        let _ = tx.send(entries).await;
    }
}

AgentMessage::DockerEvent { event } => {
    let _ = DockerService::save_event(&state.db, &server_id, &event).await;
    state.agent_manager.broadcast_browser(BrowserMessage::DockerEvent {
        server_id: server_id.clone(),
        event,
    });
}

AgentMessage::DockerUnavailable => {
    state.agent_manager.clear_docker_caches(&server_id);
    state.agent_manager.broadcast_browser(BrowserMessage::DockerAvailabilityChanged {
        server_id: server_id.clone(),
        available: false,
    });
    // Note: viewer subscriptions NOT removed — preserved for auto-recovery
}
```

- [x] **Step 4: Handle FeaturesUpdate**

```rust
AgentMessage::FeaturesUpdate { features } => {
    // Update DB
    ServerService::update_features(&state.db, &server_id, &features).await;
    // Update in-memory cache
    state.agent_manager.update_features(&server_id, features.clone());

    let docker_available = features.contains(&"docker".to_string());
    state.agent_manager.broadcast_browser(BrowserMessage::DockerAvailabilityChanged {
        server_id: server_id.clone(),
        available: docker_available,
    });

    // Auto-resume streaming if Docker became available and viewers exist
    if docker_available && state.docker_viewers.has_viewers(&server_id) {
        let _ = send_docker_command(&state.agent_manager, &server_id,
            ServerMessage::DockerStartStats { interval_secs: 3 });
        let _ = send_docker_command(&state.agent_manager, &server_id,
            ServerMessage::DockerEventsStart);
    }
}
```

- [x] **Step 5: Handle DockerNetworks, DockerVolumes, DockerActionResult**

```rust
AgentMessage::DockerNetworks { ref msg_id, .. } => {
    state.agent_manager.dispatch_pending_response(msg_id, msg.clone());
}
AgentMessage::DockerVolumes { ref msg_id, .. } => {
    state.agent_manager.dispatch_pending_response(msg_id, msg.clone());
}
AgentMessage::DockerActionResult { ref msg_id, .. } => {
    state.agent_manager.dispatch_pending_response(msg_id, msg.clone());
}
```

- [x] **Step 6: Add `update_features` to ServerService**

In `crates/server/src/service/server.rs`, add a new method to persist features:
```rust
pub async fn update_features(
    db: &DatabaseConnection,
    server_id: &str,
    features: &[String],
) -> Result<(), DbErr> {
    use crate::entity::server;
    let features_json = serde_json::to_string(features).unwrap_or_else(|_| "[]".into());
    server::Entity::update_many()
        .filter(server::Column::ServerId.eq(server_id))
        .col_expr(server::Column::Features, Expr::value(features_json))
        .exec(db)
        .await?;
    Ok(())
}
```

Note: This requires a `features` column in the `server` table — add it in the Docker migration (Task 4).

- [x] **Step 7: Enhance SystemInfo handler to persist features**

In the existing `SystemInfo` handler, add:
```rust
// Persist features to DB (replaces previous value)
let _ = ServerService::update_features(&state.db, &server_id, &info.features).await;
state.agent_manager.update_features(&server_id, info.features.clone());
```

- [x] **Step 8: Verify compilation**

Run: `cargo build -p serverbee-server`
Expected: compiles

- [x] **Step 9: Commit**

```bash
git add crates/server/src/router/ws/agent.rs crates/server/src/service/server.rs
git commit -m "feat(server): handle Docker agent messages in WS handler"
```

---

## Task 11: Server — Browser WS Extensions

**Files:**
- Modify: `crates/server/src/router/ws/browser.rs`

- [x] **Step 1: Add BrowserClientMessage handling to browser WS**

In the browser WS handler's `tokio::select!` loop, in the `ws_stream.next()` arm where incoming messages are currently ignored or only Close is handled, add parsing:

```rust
Some(Ok(Message::Text(text))) => {
    if let Ok(client_msg) = serde_json::from_str::<BrowserClientMessage>(&text) {
        match client_msg {
            BrowserClientMessage::DockerSubscribe { server_id } => {
                if !state.agent_manager.has_docker_capability(&server_id) {
                    continue;
                }
                let is_first = state.docker_viewers.add_viewer(&server_id, &connection_id);
                if is_first {
                    let _ = send_docker_command(&state.agent_manager, &server_id,
                        ServerMessage::DockerStartStats { interval_secs: 3 });
                    let _ = send_docker_command(&state.agent_manager, &server_id,
                        ServerMessage::DockerEventsStart);
                }
            }
            BrowserClientMessage::DockerUnsubscribe { server_id } => {
                let is_last = state.docker_viewers.remove_viewer(&server_id, &connection_id);
                if is_last {
                    let _ = send_docker_command(&state.agent_manager, &server_id,
                        ServerMessage::DockerStopStats);
                    let _ = send_docker_command(&state.agent_manager, &server_id,
                        ServerMessage::DockerEventsStop);
                }
            }
        }
    }
}
```

- [x] **Step 2: Add connection_id and disconnect cleanup**

At the start of the handler, generate a connection_id:
```rust
let connection_id = uuid::Uuid::new_v4().to_string();
```

After the main loop (on disconnect), add:
```rust
let affected = state.docker_viewers.remove_all_for_connection(&connection_id);
for (server_id, is_last) in affected {
    if is_last {
        let _ = send_docker_command(&state.agent_manager, &server_id,
            ServerMessage::DockerStopStats);
        let _ = send_docker_command(&state.agent_manager, &server_id,
            ServerMessage::DockerEventsStop);
    }
}
```

- [x] **Step 3: Verify compilation**

Run: `cargo build -p serverbee-server`
Expected: compiles

- [x] **Step 4: Commit**

```bash
git add crates/server/src/router/ws/browser.rs
git commit -m "feat(server): add Docker subscription handling in browser WS"
```

---

## Task 12: Server — Dedicated Log WebSocket

**Files:**
- Create: `crates/server/src/router/ws/docker_logs.rs`
- Modify: `crates/server/src/router/ws/mod.rs`

- [x] **Step 1: Create log WS handler**

Create `crates/server/src/router/ws/docker_logs.rs` following the terminal WS pattern:

```rust
use axum::{extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Query, State}, response::Response};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;
use crate::state::AppState;
use serverbee_common::{protocol::ServerMessage, docker_types::DockerLogEntry};

#[derive(serde::Deserialize)]
pub struct LogQuery {
    server_id: String,
    container_id: String,
    tail: Option<u64>,
    follow: Option<bool>,
}

pub async fn docker_logs_ws_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Query(query): Query<LogQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    // Auth: validate session cookie or API key (same as browser WS)
    // Check CAP_DOCKER capability
    // on_upgrade: handle_docker_logs_ws
    ws.on_upgrade(move |socket| {
        handle_docker_logs_ws(socket, state, query.server_id, query.container_id,
            query.tail, query.follow.unwrap_or(true))
    })
}

async fn handle_docker_logs_ws(
    socket: WebSocket,
    state: Arc<AppState>,
    server_id: String,
    container_id: String,
    tail: Option<u64>,
    follow: bool,
) {
    // Feature guard
    if !state.agent_manager.has_feature(&server_id, "docker") {
        let (mut sink, _) = socket.split();
        let _ = sink.send(Message::Close(Some(axum::extract::ws::CloseFrame {
            code: 4001,
            reason: "Agent does not support Docker".into(),
        }))).await;
        return;
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    let (output_tx, mut output_rx) = mpsc::channel::<Vec<DockerLogEntry>>(256);

    state.agent_manager.add_docker_log_session(&server_id, session_id.clone(), output_tx);

    if let Some(tx) = state.agent_manager.get_sender(&server_id) {
        let _ = tx.send(ServerMessage::DockerLogsStart {
            session_id: session_id.clone(),
            container_id,
            tail,
            follow,
        }).await;
    }

    let (mut ws_sink, mut ws_stream) = socket.split();
    loop {
        tokio::select! {
            entries = output_rx.recv() => {
                match entries {
                    Some(entries) => {
                        if let Ok(json) = serde_json::to_string(&entries) {
                            if ws_sink.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    None => { break; }
                }
            }
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => { break; }
                    _ => {}
                }
            }
        }
    }

    let _ = ws_sink.send(Message::Close(None)).await;

    if state.agent_manager.remove_docker_log_session(&server_id, &session_id) {
        if let Some(tx) = state.agent_manager.get_sender(&server_id) {
            let _ = tx.send(ServerMessage::DockerLogsStop { session_id }).await;
        }
    }
}
```

- [x] **Step 2: Register route**

In `crates/server/src/router/ws/mod.rs`, add the route registration for `/api/ws/docker/logs` with auth middleware.

- [x] **Step 3: Verify compilation**

Run: `cargo build -p serverbee-server`
Expected: compiles

- [x] **Step 4: Commit**

```bash
git add crates/server/src/router/ws/docker_logs.rs crates/server/src/router/ws/mod.rs
git commit -m "feat(server): add dedicated Docker log WebSocket endpoint"
```

---

## Task 13: Server — Capability Revocation & Cleanup

**Files:**
- Modify: `crates/server/src/router/api/server.rs` (or wherever capability update handler lives)
- Modify: `crates/server/src/task/cleanup.rs`

- [x] **Step 1: Add CAP_DOCKER teardown to capability update handler**

In the `PUT /api/servers/{id}/capabilities` handler, after updating DB and cache, add:

```rust
// If CAP_DOCKER was just removed, tear down all Docker streams
let old_had_docker = has_capability(old_caps, CAP_DOCKER);
let new_has_docker = has_capability(new_caps, CAP_DOCKER);

if old_had_docker && !new_has_docker {
    // Remove all viewer subscriptions
    state.docker_viewers.remove_all_for_server(&server_id);

    // Stop stats/events on Agent and close log sessions
    if let Some(tx) = state.agent_manager.get_sender(&server_id) {
        let _ = tx.send(ServerMessage::DockerStopStats).await;
        let _ = tx.send(ServerMessage::DockerEventsStop).await;

        let session_ids = state.agent_manager.remove_docker_log_sessions_for_server(&server_id);
        for session_id in session_ids {
            let _ = tx.send(ServerMessage::DockerLogsStop { session_id }).await;
        }
    } else {
        // Agent disconnected — still clean up local state
        state.agent_manager.remove_docker_log_sessions_for_server(&server_id);
    }

    // Clear caches
    state.agent_manager.clear_docker_caches(&server_id);
}

// Update capabilities cache
state.agent_manager.update_capabilities(&server_id, new_caps);
```

- [x] **Step 2: Add docker_event cleanup to cleanup task**

In `crates/server/src/task/cleanup.rs`, add alongside existing cleanup branches:

```rust
match DockerService::cleanup_expired(&state.db, retention.records_days).await {
    Ok(n) if n > 0 => tracing::info!("Cleaned up {n} expired Docker events"),
    Err(e) => tracing::error!("Failed to clean up Docker events: {e}"),
    _ => {}
}
```

- [x] **Step 3: Verify compilation**

Run: `cargo build -p serverbee-server`
Expected: compiles

- [x] **Step 4: Commit**

```bash
git add crates/server/src/router/api/server.rs crates/server/src/task/cleanup.rs
git commit -m "feat(server): add CAP_DOCKER revocation teardown and event cleanup"
```

---

## Task 14: Agent — DockerManager Core

**Files:**
- Create: `crates/agent/src/docker/mod.rs`
- Create: `crates/agent/src/docker/containers.rs`
- Modify: `crates/agent/Cargo.toml`

- [x] **Step 1: Add bollard dependency**

In `crates/agent/Cargo.toml`, add:
```toml
bollard = "0.18"
```

- [x] **Step 2: Create DockerManager lifecycle**

Create `crates/agent/src/docker/mod.rs`:

```rust
pub mod containers;
pub mod logs;
pub mod events;
pub mod networks;
pub mod volumes;

use bollard::Docker;
use std::collections::HashMap;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Interval;
use serverbee_common::protocol::AgentMessage;

pub struct DockerManager {
    docker: Docker,
    agent_tx: mpsc::Sender<AgentMessage>,
    capabilities: Arc<AtomicU32>,
    stats_interval: Option<Interval>,
    log_sessions: HashMap<String, JoinHandle<()>>,
    event_stream_handle: Option<JoinHandle<()>>,
}

impl DockerManager {
    pub fn try_new(
        agent_tx: mpsc::Sender<AgentMessage>,
        capabilities: Arc<AtomicU32>,
    ) -> Result<Self, bollard::errors::Error> {
        let docker = Docker::connect_with_local_defaults()?;
        Ok(Self {
            docker,
            agent_tx,
            capabilities,
            stats_interval: None,
            log_sessions: HashMap::new(),
            event_stream_handle: None,
        })
    }

    pub async fn verify_connection(&self) -> Result<(), bollard::errors::Error> {
        self.docker.ping().await?;
        Ok(())
    }

    pub fn cleanup(&mut self) {
        // Abort all log sessions
        for (_, handle) in self.log_sessions.drain() {
            handle.abort();
        }
        // Stop event stream
        if let Some(handle) = self.event_stream_handle.take() {
            handle.abort();
        }
        self.stats_interval = None;
    }
}
```

- [x] **Step 3: Create container list and stats**

Create `crates/agent/src/docker/containers.rs`:

```rust
use bollard::Docker;
use bollard::container::{ListContainersOptions, StatsOptions};
use futures_util::StreamExt;
use serverbee_common::docker_types::*;
use std::collections::HashMap;

pub async fn list_containers(docker: &Docker) -> Result<Vec<DockerContainer>, bollard::errors::Error> {
    let options = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };
    let containers = docker.list_containers(Some(options)).await?;
    Ok(containers.into_iter().map(|c| {
        DockerContainer {
            id: c.id.unwrap_or_default(),
            name: c.names.and_then(|n| n.first().map(|s| s.trim_start_matches('/').to_string()))
                .unwrap_or_default(),
            image: c.image.unwrap_or_default(),
            state: c.state.unwrap_or_default(),
            status: c.status.unwrap_or_default(),
            created: c.created.unwrap_or(0),
            ports: c.ports.map(|ports| ports.into_iter().map(|p| DockerPort {
                private_port: p.private_port as u16,
                public_port: p.public_port.map(|pp| pp as u16),
                port_type: p.typ.map(|t| format!("{:?}", t).to_lowercase()).unwrap_or("tcp".into()),
                ip: p.ip,
            }).collect()).unwrap_or_default(),
            labels: c.labels.unwrap_or_default(),
        }
    }).collect())
}

pub async fn get_container_stats(docker: &Docker, container_ids: &[String]) -> Vec<DockerContainerStats> {
    let mut results = Vec::new();
    for id in container_ids {
        let options = StatsOptions { stream: false, one_shot: true };
        let mut stream = docker.stats(id, Some(options));
        if let Some(Ok(stats)) = stream.next().await {
            let cpu_percent = calculate_cpu_percent(&stats);
            let (memory_usage, memory_limit) = get_memory_stats(&stats);
            let memory_percent = if memory_limit > 0 {
                (memory_usage as f64 / memory_limit as f64) * 100.0
            } else { 0.0 };
            let (net_rx, net_tx) = get_network_stats(&stats);
            let (block_read, block_write) = get_block_io_stats(&stats);

            results.push(DockerContainerStats {
                id: id.clone(),
                name: stats.name.trim_start_matches('/').to_string(),
                cpu_percent,
                memory_usage,
                memory_limit,
                memory_percent,
                network_rx: net_rx,
                network_tx: net_tx,
                block_read,
                block_write,
            });
        }
    }
    results
}

fn calculate_cpu_percent(stats: &bollard::container::Stats) -> f64 {
    let cpu_stats = &stats.cpu_stats;
    let precpu_stats = &stats.precpu_stats;

    let cpu_delta = cpu_stats.cpu_usage.total_usage.saturating_sub(
        precpu_stats.cpu_usage.total_usage
    ) as f64;
    let system_delta = cpu_stats.system_cpu_usage.unwrap_or(0).saturating_sub(
        precpu_stats.system_cpu_usage.unwrap_or(0)
    ) as f64;

    if system_delta > 0.0 && cpu_delta > 0.0 {
        let num_cpus = cpu_stats.online_cpus.unwrap_or(1) as f64;
        (cpu_delta / system_delta) * num_cpus * 100.0
    } else {
        0.0
    }
}

fn get_memory_stats(stats: &bollard::container::Stats) -> (u64, u64) {
    let usage = stats.memory_stats.usage.unwrap_or(0);
    let cache = stats.memory_stats.stats
        .as_ref()
        .and_then(|s| match s {
            bollard::container::MemoryStatsStats::V1(v1) => Some(v1.cache),
            _ => None,
        })
        .unwrap_or(0);
    let limit = stats.memory_stats.limit.unwrap_or(0);
    (usage.saturating_sub(cache), limit)
}

fn get_network_stats(stats: &bollard::container::Stats) -> (u64, u64) {
    stats.networks.as_ref().map(|nets| {
        nets.values().fold((0u64, 0u64), |(rx, tx), net| {
            (rx + net.rx_bytes, tx + net.tx_bytes)
        })
    }).unwrap_or((0, 0))
}

fn get_block_io_stats(stats: &bollard::container::Stats) -> (u64, u64) {
    stats.blkio_stats.io_service_bytes_recursive.as_ref().map(|entries| {
        entries.iter().fold((0u64, 0u64), |(read, write), entry| {
            match entry.op.to_lowercase().as_str() {
                "read" => (read + entry.value, write),
                "write" => (read, write + entry.value),
                _ => (read, write),
            }
        })
    }).unwrap_or((0, 0))
}
```

- [x] **Step 4: Export docker module**

In `crates/agent/src/main.rs` (or `lib.rs`), add:
```rust
mod docker;
```

- [x] **Step 5: Verify compilation**

Run: `cargo build -p serverbee-agent`
Expected: compiles (may need to resolve bollard API differences)

- [x] **Step 6: Commit**

```bash
git add crates/agent/Cargo.toml crates/agent/src/docker/
git commit -m "feat(agent): add DockerManager core with container list and stats"
```

---

## Task 15: Agent — Docker Logs, Events, Networks, Volumes

**Files:**
- Create: `crates/agent/src/docker/logs.rs`
- Create: `crates/agent/src/docker/events.rs`
- Create: `crates/agent/src/docker/networks.rs`
- Create: `crates/agent/src/docker/volumes.rs`

- [x] **Step 1: Create log streaming with batching**

Create `crates/agent/src/docker/logs.rs`:

```rust
use bollard::container::LogsOptions;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use serverbee_common::docker_types::DockerLogEntry;
use serverbee_common::protocol::AgentMessage;

/// Spawn a log session task. Returns a JoinHandle that can be aborted to stop.
pub fn spawn_log_session(
    docker: bollard::Docker,
    container_id: String,
    session_id: String,
    tail: Option<u64>,
    follow: bool,
    agent_tx: mpsc::Sender<AgentMessage>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let options = LogsOptions::<String> {
            follow,
            stdout: true,
            stderr: true,
            timestamps: true,
            tail: tail.map(|t| t.to_string()).unwrap_or_else(|| "100".into()),
            ..Default::default()
        };
        let mut stream = docker.logs(&container_id, Some(options));

        // Batching: collect up to 50 entries or flush every 50ms
        let mut batch: Vec<DockerLogEntry> = Vec::with_capacity(50);
        let mut flush_interval = interval(Duration::from_millis(50));

        loop {
            tokio::select! {
                item = stream.next() => {
                    match item {
                        Some(Ok(output)) => {
                            let (stream_type, message) = match &output {
                                bollard::container::LogOutput::StdOut { message } => ("stdout", message),
                                bollard::container::LogOutput::StdErr { message } => ("stderr", message),
                                _ => continue,
                            };
                            let text = String::from_utf8_lossy(message).to_string();
                            // Parse optional timestamp prefix (ISO 8601 followed by space)
                            let (timestamp, msg) = if text.len() > 30 && text.as_bytes().get(4) == Some(&b'-') {
                                text.split_once(' ')
                                    .map(|(ts, m)| (Some(ts.to_string()), m.to_string()))
                                    .unwrap_or((None, text))
                            } else {
                                (None, text)
                            };
                            batch.push(DockerLogEntry { timestamp, stream: stream_type.into(), message: msg });
                            if batch.len() >= 50 {
                                let entries = std::mem::take(&mut batch);
                                if agent_tx.send(AgentMessage::DockerLog { session_id: session_id.clone(), entries }).await.is_err() {
                                    return;
                                }
                            }
                        }
                        Some(Err(_)) | None => {
                            // Stream ended or errored — flush remaining and exit
                            if !batch.is_empty() {
                                let entries = std::mem::take(&mut batch);
                                let _ = agent_tx.send(AgentMessage::DockerLog { session_id: session_id.clone(), entries }).await;
                            }
                            return;
                        }
                    }
                }
                _ = flush_interval.tick() => {
                    if !batch.is_empty() {
                        let entries = std::mem::take(&mut batch);
                        if agent_tx.send(AgentMessage::DockerLog { session_id: session_id.clone(), entries }).await.is_err() {
                            return;
                        }
                    }
                }
            }
        }
    })
}
```

- [x] **Step 2: Create event stream**

Create `crates/agent/src/docker/events.rs`:

```rust
use bollard::system::EventsOptions;
use futures::StreamExt;
use tokio::sync::mpsc;
use serverbee_common::docker_types::DockerEventInfo;
use serverbee_common::protocol::AgentMessage;
use std::collections::HashMap;

/// Spawn an event stream task. Returns a JoinHandle that can be aborted to stop.
pub fn spawn_event_stream(
    docker: bollard::Docker,
    agent_tx: mpsc::Sender<AgentMessage>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let options: EventsOptions<String> = Default::default();
            let mut stream = docker.events(Some(options));

            while let Some(result) = stream.next().await {
                match result {
                    Ok(event) => {
                        let actor = event.actor.as_ref();
                        let event_info = DockerEventInfo {
                            timestamp: event.time.unwrap_or(0),
                            event_type: event.typ.map(|t| format!("{:?}", t).to_lowercase()).unwrap_or_default(),
                            action: event.action.unwrap_or_default().to_string(),
                            actor_id: actor
                                .and_then(|a| a.id.clone())
                                .unwrap_or_default(),
                            actor_name: actor
                                .and_then(|a| a.attributes.as_ref())
                                .and_then(|attrs| attrs.get("name").cloned()),
                            attributes: actor
                                .and_then(|a| a.attributes.clone())
                                .unwrap_or_default(),
                        };
                        if agent_tx.send(AgentMessage::DockerEvent { event: event_info }).await.is_err() {
                            return; // channel closed, agent shutting down
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Docker event stream error: {e}, reconnecting in 5s");
                        break; // break inner loop to reconnect
                    }
                }
            }

            // Auto-reconnect after 5s delay
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    })
}
```

- [x] **Step 3: Create network/volume queries**

Create `crates/agent/src/docker/networks.rs`:
```rust
use bollard::Docker;
use serverbee_common::docker_types::DockerNetwork;

pub async fn list_networks(docker: &Docker) -> Vec<DockerNetwork> {
    match docker.list_networks::<String>(None).await {
        Ok(networks) => networks.into_iter().map(|n| DockerNetwork {
            id: n.id.unwrap_or_default(),
            name: n.name.unwrap_or_default(),
            driver: n.driver.unwrap_or_default(),
            scope: n.scope.unwrap_or_default(),
            containers: n.containers
                .map(|c| c.into_iter().map(|(id, info)| {
                    let name = info.name.unwrap_or_else(|| id.chars().take(12).collect());
                    (id, name)
                }).collect())
                .unwrap_or_default(),
        }).collect(),
        Err(_) => vec![],
    }
}
```

Create `crates/agent/src/docker/volumes.rs`:
```rust
use bollard::Docker;
use serverbee_common::docker_types::DockerVolume;

pub async fn list_volumes(docker: &Docker) -> Vec<DockerVolume> {
    match docker.list_volumes::<String>(None).await {
        Ok(result) => result.volumes.unwrap_or_default().into_iter().map(|v| DockerVolume {
            name: v.name,
            driver: v.driver,
            mountpoint: v.mountpoint,
            created_at: v.created_at,
            labels: v.labels.unwrap_or_default(),
        }).collect(),
        Err(_) => vec![],
    }
}
```

- [x] **Step 4: Add ServerMessage handling to DockerManager**

In `docker/mod.rs`, add method to handle incoming ServerMessages:
```rust
pub async fn handle_server_message(&mut self, msg: ServerMessage) {
    match msg {
        ServerMessage::DockerStartStats { interval_secs } => { /* start stats polling */ }
        ServerMessage::DockerStopStats => { /* stop stats polling */ }
        ServerMessage::DockerLogsStart { session_id, container_id, tail, follow } => { /* spawn log session */ }
        ServerMessage::DockerLogsStop { session_id } => { /* abort log session */ }
        ServerMessage::DockerEventsStart => { /* start event stream */ }
        ServerMessage::DockerEventsStop => { /* stop event stream */ }
        ServerMessage::DockerContainerAction { msg_id, container_id, action } => { /* execute action */ }
        ServerMessage::DockerListContainers { msg_id } => { /* list and send */ }
        ServerMessage::DockerGetInfo { msg_id } => { /* get info and send */ }
        ServerMessage::DockerListNetworks { msg_id } => { /* list and send */ }
        ServerMessage::DockerListVolumes { msg_id } => { /* list and send */ }
        _ => {}
    }
}
```

- [x] **Step 5: Verify compilation**

Run: `cargo build -p serverbee-agent`
Expected: compiles

- [x] **Step 6: Commit**

```bash
git add crates/agent/src/docker/
git commit -m "feat(agent): add Docker logs, events, networks, volumes support"
```

---

## Task 16: Agent — Reporter Integration

**Files:**
- Modify: `crates/agent/src/reporter.rs`

- [x] **Step 1: Add DockerManager initialization**

In `connect_and_report()`, after existing manager creation:
```rust
let (docker_tx, mut docker_rx) = mpsc::channel(256);
let mut docker_manager: Option<DockerManager> = match DockerManager::try_new(
    docker_tx.clone(), Arc::clone(&capabilities)
) {
    Ok(mut mgr) => {
        if mgr.verify_connection().await.is_ok() {
            // Send DockerInfo proactively
            let info = mgr.get_system_info().await;
            if let Some(info) = info {
                let _ = ws_tx.send(AgentMessage::DockerInfo { msg_id: None, info }).await;
            }
            Some(mgr)
        } else {
            None
        }
    }
    Err(_) => None,
};

let mut docker_retry_interval = tokio::time::interval(Duration::from_secs(30));
```

- [x] **Step 2: Include features in SystemInfo**

When building the `SystemInfo` message to send on connect, add:
```rust
features: if docker_manager.is_some() { vec!["docker".into()] } else { vec![] },
```

- [x] **Step 3: Add DockerManager arms to tokio::select!**

```rust
// Docker messages from DockerManager
msg = docker_rx.recv() => {
    if let Some(msg) = msg {
        let _ = ws_tx.send(msg).await;
    }
}

// Docker retry (only when manager is None)
_ = docker_retry_interval.tick(), if docker_manager.is_none() => {
    if let Ok(mut mgr) = DockerManager::try_new(docker_tx.clone(), Arc::clone(&capabilities)) {
        if mgr.verify_connection().await.is_ok() {
            let info = mgr.get_system_info().await;
            if let Some(info) = info {
                let _ = ws_tx.send(AgentMessage::DockerInfo { msg_id: None, info }).await;
            }
            let _ = ws_tx.send(AgentMessage::FeaturesUpdate { features: vec!["docker".into()] }).await;
            docker_manager = Some(mgr);
        }
    }
}
```

- [x] **Step 4: Route Docker ServerMessages to DockerManager**

In the `server_msg` handling arm, add Docker message dispatch:
```rust
ServerMessage::DockerStartStats { .. }
| ServerMessage::DockerStopStats
| ServerMessage::DockerLogsStart { .. }
| ServerMessage::DockerLogsStop { .. }
| ServerMessage::DockerEventsStart
| ServerMessage::DockerEventsStop
| ServerMessage::DockerContainerAction { .. }
| ServerMessage::DockerListContainers { .. }
| ServerMessage::DockerGetInfo { .. }
| ServerMessage::DockerListNetworks { .. }
| ServerMessage::DockerListVolumes { .. } => {
    if let Some(mgr) = &mut docker_manager {
        if has_capability(capabilities.load(Ordering::Relaxed), CAP_DOCKER) {
            mgr.handle_server_message(msg).await;
        } else {
            // Capability denied
            if let Some(msg_id) = extract_msg_id(&msg) {
                let _ = ws_tx.send(AgentMessage::CapabilityDenied {
                    msg_id: Some(msg_id),
                    session_id: None,
                    capability: "docker".into(),
                }).await;
            }
        }
    }
}
```

- [x] **Step 5: Add cleanup on disconnect**

In the WebSocket disconnect cleanup section:
```rust
if let Some(mut mgr) = docker_manager.take() {
    mgr.cleanup();
}
```

- [x] **Step 6: Handle CapabilitiesSync for Docker**

In the existing `CapabilitiesSync` handler, add Docker cleanup:
```rust
// If CAP_DOCKER was just removed
let old_had_docker = has_capability(old_caps, CAP_DOCKER);
let new_has_docker = has_capability(new_caps, CAP_DOCKER);
if old_had_docker && !new_has_docker {
    if let Some(mgr) = &mut docker_manager {
        mgr.cleanup();
    }
}
```

- [x] **Step 7: Verify compilation**

Run: `cargo build -p serverbee-agent`
Expected: compiles

- [x] **Step 8: Commit**

```bash
git add crates/agent/src/reporter.rs
git commit -m "feat(agent): integrate DockerManager into Reporter main loop"
```

---

## Task 17: Frontend — Types, API Client & WsClient Extensions

**Files:**
- Create: `apps/web/src/routes/_authed/servers/$serverId/docker/types.ts`
- Modify: `apps/web/src/lib/ws-client.ts`

- [x] **Step 1: Create frontend Docker types**

Create `apps/web/src/routes/_authed/servers/$serverId/docker/types.ts`:

```typescript
export interface DockerContainer {
    id: string
    name: string
    image: string
    state: string
    status: string
    created: number
    ports: DockerPort[]
    labels: Record<string, string>
}

export interface DockerPort {
    private_port: number
    public_port: number | null
    port_type: string
    ip: string | null
}

export interface DockerContainerStats {
    id: string
    name: string
    cpu_percent: number
    memory_usage: number
    memory_limit: number
    memory_percent: number
    network_rx: number
    network_tx: number
    block_read: number
    block_write: number
}

export interface DockerLogEntry {
    timestamp: string | null
    stream: string
    message: string
}

export interface DockerEventInfo {
    timestamp: number
    event_type: string
    action: string
    actor_id: string
    actor_name: string | null
    attributes: Record<string, string>
}

export interface DockerSystemInfo {
    docker_version: string
    api_version: string
    os: string
    arch: string
    containers_running: number
    containers_paused: number
    containers_stopped: number
    images: number
    memory_total: number
}

export interface DockerNetwork {
    id: string
    name: string
    driver: string
    scope: string
    containers: Record<string, string>
}

export interface DockerVolume {
    name: string
    driver: string
    mountpoint: string
    created_at: string | null
    labels: Record<string, string>
}
```

- [x] **Step 2: Add send() and connectionState to WsClient**

In `apps/web/src/lib/ws-client.ts`, add to the `WsClient` class:

```typescript
private connectionStateListeners: Set<(state: 'connected' | 'disconnected') => void> = new Set()
private _connectionState: 'connected' | 'disconnected' = 'disconnected'

get connectionState(): 'connected' | 'disconnected' {
    return this._connectionState
}

private setConnectionState(state: 'connected' | 'disconnected'): void {
    this._connectionState = state
    for (const listener of this.connectionStateListeners) listener(state)
}

onConnectionStateChange(listener: (state: 'connected' | 'disconnected') => void): () => void {
    this.connectionStateListeners.add(listener)
    return () => this.connectionStateListeners.delete(listener)
}

send(data: unknown): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
        this.ws.send(JSON.stringify(data))
    }
}
```

In the existing `connect()` method's `onopen` handler, add `this.setConnectionState('connected')`.
In the `onclose` handler, add `this.setConnectionState('disconnected')`.

- [x] **Step 3: Verify frontend compiles**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [x] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/servers/\$serverId/docker/types.ts \
  apps/web/src/lib/ws-client.ts
git commit -m "feat(web): add Docker types and WsClient send/connectionState"
```

---

## Task 18: Frontend — WS Context & Message Handlers

**Files:**
- Create: `apps/web/src/contexts/servers-ws-context.tsx`
- Modify: `apps/web/src/hooks/use-servers-ws.ts`
- Modify: `apps/web/src/routes/_authed.tsx`

- [x] **Step 1: Create ServersWsContext**

Create `apps/web/src/contexts/servers-ws-context.tsx`:

```typescript
import { createContext, useContext } from 'react'

interface ServersWsContextValue {
    send: (data: unknown) => void
    connectionState: 'connected' | 'disconnected'
}

export const ServersWsContext = createContext<ServersWsContextValue | null>(null)

export const useServersWsSend = () => {
    const ctx = useContext(ServersWsContext)
    if (!ctx) throw new Error('useServersWsSend must be used within ServersWsContext provider')
    return ctx
}
```

- [x] **Step 2: Add Docker message handlers to useServersWs**

In `apps/web/src/hooks/use-servers-ws.ts`, add new cases to the message switch:

```typescript
case 'docker_update': {
    queryClient.setQueryData(['docker', 'containers', msg.server_id], msg.containers)
    if (msg.stats) {
        queryClient.setQueryData(['docker', 'stats', msg.server_id], msg.stats)
    }
    break
}
case 'docker_event': {
    queryClient.setQueryData(
        ['docker', 'events', msg.server_id],
        (prev: DockerEventInfo[] = []) => [...prev, msg.event].slice(-100)
    )
    break
}
case 'docker_availability_changed': {
    const updateFeatures = (server: ServerMetrics) => ({
        ...server,
        features: msg.available
            ? [...new Set([...(server.features ?? []), 'docker'])]
            : (server.features ?? []).filter((f: string) => f !== 'docker')
    })
    queryClient.setQueryData(['servers'], (prev: ServerMetrics[]) =>
        prev?.map(s => s.id === msg.server_id ? updateFeatures(s) : s)
    )
    queryClient.setQueryData(['servers', msg.server_id], (prev: ServerMetrics | undefined) =>
        prev ? updateFeatures(prev) : prev
    )
    break
}
```

Also modify `useServersWs` to return the WsClient ref so the provider can access it.

- [x] **Step 3: Wrap _authed.tsx with ServersWsContext provider**

In `apps/web/src/routes/_authed.tsx`, wrap the layout with the context provider that exposes `send` and `connectionState` from the WsClient.

- [x] **Step 4: Verify frontend compiles**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [x] **Step 5: Commit**

```bash
git add apps/web/src/contexts/servers-ws-context.tsx \
  apps/web/src/hooks/use-servers-ws.ts \
  apps/web/src/routes/_authed.tsx
git commit -m "feat(web): add ServersWsContext and Docker WS message handlers"
```

---

## Task 19: Frontend — Docker Subscription & Log Hooks

**Files:**
- Create: `apps/web/src/routes/_authed/servers/$serverId/docker/hooks/use-docker-subscription.ts`
- Create: `apps/web/src/routes/_authed/servers/$serverId/docker/hooks/use-docker-logs.ts`

- [x] **Step 1: Create useDockerSubscription hook**

```typescript
import { useEffect } from 'react'
import { useServersWsSend } from '@/contexts/servers-ws-context'

export const useDockerSubscription = (serverId: string) => {
    const { send, connectionState } = useServersWsSend()

    useEffect(() => {
        if (connectionState === 'connected') {
            send({ type: 'docker_subscribe', server_id: serverId })
        }
        return () => {
            send({ type: 'docker_unsubscribe', server_id: serverId })
        }
    }, [serverId, send, connectionState])
}
```

- [x] **Step 2: Create useDockerLogs hook**

```typescript
import { useEffect, useRef, useState } from 'react'
import type { DockerLogEntry } from '../types'

export const useDockerLogs = (
    serverId: string,
    containerId: string,
    options?: { tail?: number; follow?: boolean; enabled?: boolean }
) => {
    const [entries, setEntries] = useState<DockerLogEntry[]>([])
    const [connected, setConnected] = useState(false)
    const wsRef = useRef<WebSocket | null>(null)

    useEffect(() => {
        if (options?.enabled === false) return

        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
        const params = new URLSearchParams({
            server_id: serverId,
            container_id: containerId,
            follow: String(options?.follow ?? true),
        })
        if (options?.tail) params.set('tail', String(options.tail))

        const ws = new WebSocket(`${protocol}//${window.location.host}/api/ws/docker/logs?${params}`)
        wsRef.current = ws

        ws.onopen = () => setConnected(true)
        ws.onclose = () => setConnected(false)
        ws.onmessage = (event) => {
            const batch: DockerLogEntry[] = JSON.parse(event.data)
            setEntries(prev => [...prev, ...batch].slice(-5000))
        }

        return () => {
            ws.close()
            wsRef.current = null
        }
    }, [serverId, containerId, options?.tail, options?.follow, options?.enabled])

    const clear = () => setEntries([])

    return { entries, connected, clear }
}
```

- [x] **Step 3: Verify frontend compiles**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [x] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/servers/\$serverId/docker/hooks/
git commit -m "feat(web): add Docker subscription and log streaming hooks"
```

---

## Task 20: Frontend — Docker Tab Page & Overview

**Files:**
- Create: `apps/web/src/routes/_authed/servers/$serverId/docker/index.tsx`
- Create: `apps/web/src/routes/_authed/servers/$serverId/docker/components/docker-overview.tsx`
- Create: `apps/web/src/routes/_authed/servers/$serverId/docker/components/docker-events.tsx`

- [x] **Step 1: Create Docker Tab main page**

Create `index.tsx` — the Docker Tab entry point. Uses `useDockerSubscription` for real-time data. Shows "unavailable" placeholder when `!features.includes('docker')`. When available, shows: DockerOverview → ContainerList → DockerEvents.

- [x] **Step 2: Create DockerOverview component**

Create `docker-overview.tsx` — 5 cards: Running, Stopped, Total CPU, Total Memory, Docker Version. Uses data from `useQuery(['docker', 'containers', serverId])` and `useQuery(['docker', 'stats', serverId])`.

- [x] **Step 3: Create DockerEvents component**

Create `docker-events.tsx` — chronological timeline of recent events. Uses `useQuery(['docker', 'events', serverId])` cache (populated by WS).

- [x] **Step 4: Verify frontend compiles and renders**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [x] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed/servers/\$serverId/docker/
git commit -m "feat(web): add Docker Tab page with overview and events"
```

---

## Task 21: Frontend — Container List

**Files:**
- Create: `apps/web/src/routes/_authed/servers/$serverId/docker/components/container-list.tsx`

- [x] **Step 1: Create ContainerList component**

Table with columns: Name, Image, Status, CPU, Memory, Net I/O, Actions. Features:
- Search by name/image
- Filter: All / Running / Stopped
- Click row opens ContainerDetailDialog
- Actions (stop/restart/remove) only visible to admin users
- Data from `useQuery(['docker', 'containers', serverId], { enabled: dockerAvailable })`
- Stats merged from `useQuery(['docker', 'stats', serverId])`

- [x] **Step 2: Verify frontend compiles**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [x] **Step 3: Commit**

```bash
git add apps/web/src/routes/_authed/servers/\$serverId/docker/components/container-list.tsx
git commit -m "feat(web): add Docker container list with search and filter"
```

---

## Task 22: Frontend — Container Detail Dialog

**Files:**
- Create: `apps/web/src/routes/_authed/servers/$serverId/docker/components/container-detail-dialog.tsx`
- Create: `apps/web/src/routes/_authed/servers/$serverId/docker/components/container-stats.tsx`
- Create: `apps/web/src/routes/_authed/servers/$serverId/docker/components/container-logs.tsx`

- [x] **Step 1: Create ContainerStats component**

4 mini cards: CPU (with sparkline), Memory (usage bar + percentage), Net I/O (rx/tx), Block I/O (read/write).

- [x] **Step 2: Create ContainerLogs component**

Monospace log output area using `useDockerLogs` hook. Features:
- Follow toggle button
- Tail selector (100/500/All)
- stdout/stderr checkboxes
- Auto-scroll when following

- [x] **Step 3: Create ContainerDetailDialog**

Dialog component with vertically stacked sections:
1. Top: container meta info (image, status, ports) + action buttons (admin only)
2. Middle: ContainerStats
3. Bottom: ContainerLogs

- [x] **Step 4: Verify frontend compiles**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [x] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed/servers/\$serverId/docker/components/
git commit -m "feat(web): add container detail dialog with stats and logs"
```

---

## Task 23: Frontend — Networks & Volumes Dialogs

**Files:**
- Create: `apps/web/src/routes/_authed/servers/$serverId/docker/components/docker-networks-dialog.tsx`
- Create: `apps/web/src/routes/_authed/servers/$serverId/docker/components/docker-volumes-dialog.tsx`

- [x] **Step 1: Create DockerNetworksDialog**

Dialog listing networks fetched via `api.get<DockerNetwork[]>(/api/servers/${serverId}/docker/networks)`. Shows: name, driver, scope, connected containers.

- [x] **Step 2: Create DockerVolumesDialog**

Dialog listing volumes fetched via `api.get<DockerVolume[]>(/api/servers/${serverId}/docker/volumes)`. Shows: name, driver, mountpoint, created date.

- [x] **Step 3: Wire into Docker Tab**

Add buttons/links in `index.tsx` to open these dialogs.

- [x] **Step 4: Verify frontend compiles**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [x] **Step 5: Commit**

```bash
git add apps/web/src/routes/_authed/servers/\$serverId/docker/components/docker-networks-dialog.tsx \
  apps/web/src/routes/_authed/servers/\$serverId/docker/components/docker-volumes-dialog.tsx \
  apps/web/src/routes/_authed/servers/\$serverId/docker/index.tsx
git commit -m "feat(web): add Docker networks and volumes dialogs"
```

---

## Task 24: DTO Propagation & Tab Visibility Wiring

**Files:**
- Modify: `crates/common/src/types.rs` — add `features` to `ServerStatus`
- Modify: `crates/server/src/router/ws/browser.rs` — populate `features` in `build_full_sync`
- Modify: `apps/web/src/lib/capabilities.ts` — add `CAP_DOCKER`
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx` — add Docker route link
- Modify: Frontend Server type definition

- [x] **Step 1: Add `CAP_DOCKER` to frontend capabilities**

In `apps/web/src/lib/capabilities.ts`, add:
```typescript
export const CAP_DOCKER = 128

// And in the CAPABILITIES array:
{ bit: CAP_DOCKER, key: 'docker', labelKey: 'cap_docker' as const, risk: 'medium' as const },
```

- [x] **Step 2: Add features field to server DTOs**

In `crates/common/src/types.rs`, add to `ServerStatus`:
```rust
#[serde(default)]
pub features: Vec<String>,
```

In `crates/server/src/router/ws/browser.rs`, in `build_full_sync` and the update broadcast path, populate `features` from the server entity:
```rust
features: serde_json::from_str(&server.features).unwrap_or_default(),
```

- [x] **Step 3: Add features to frontend Server type**

In the frontend Server/ServerMetrics type definition, add:
```typescript
features: string[]
```

- [x] **Step 4: Add Docker route link to server detail page**

In `apps/web/src/routes/_authed/servers/$id.tsx`, following the existing pattern for `terminalEnabled`/`fileEnabled`, add:
```typescript
const dockerEnabled = hasCap(serverWithCaps.capabilities ?? 0, CAP_DOCKER)

// Then in the action buttons area, alongside Terminal/Files links:
{isOnline && dockerEnabled && (
  <Link params={{ serverId: id }} to="/servers/$serverId/docker">
    <Button size="sm" variant="outline">
      Docker
    </Button>
  </Link>
)}
```

- [x] **Step 5: Run full typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [x] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: wire Docker Tab visibility via CAP_DOCKER and features DTO propagation"
```

---

## Task 25: Integration Testing & Final Verification

**Files:**
- Various test files

- [x] **Step 1: Run all Rust tests**

Run: `cargo test --workspace`
Expected: All existing tests still pass + new Docker tests pass

- [x] **Step 2: Run frontend tests**

Run: `cd apps/web && bun run test`
Expected: All existing tests pass

- [x] **Step 3: Run lint checks**

Run: `cargo clippy --workspace -- -D warnings`
Run: `cd apps/web && bun x ultracite check`
Expected: 0 warnings/errors

- [x] **Step 4: Run frontend typecheck**

Run: `cd apps/web && bun run typecheck`
Expected: no type errors

- [x] **Step 5: Manual verification**

Start the server and agent locally:
```bash
cargo run -p serverbee-server &
cargo run -p serverbee-agent &
```

1. Verify Docker Tab appears when CAP_DOCKER is enabled
2. Verify container list loads
3. Verify real-time stats update
4. Verify container actions work (start/stop/restart)
5. Verify log streaming in container detail dialog
6. Verify Docker events appear in timeline
7. Verify tab shows "unavailable" placeholder when Docker daemon is down

- [x] **Step 6: Final commit**

```bash
git add -A
git commit -m "test: add Docker monitoring integration tests"
```

---

## Dependencies Between Tasks

```
Task 1 (Data Structures)
  ↓
Task 2 (Capabilities + SystemInfo)
  ↓
Task 3 (Protocol Messages)
  ↓
Task 4 (DB Migration) ──────────────────────────────────────┐
  ↓                                                          │
Task 5 (Entity + Service) ──────────────────────────────────┤
  ↓                                                          │
Task 6 (DockerViewerTracker)                                 │
  ↓                                                          │
Task 7 (AgentManager Extensions) ───────────────────────────┤
  ↓                                                          │
Task 8 (AppState Changes)                                    │
  ↓                                                          │
Task 9 (REST API) ──────────────────────────────────────────┤
  ↓                                                          │
Task 10 (Agent WS Handler) ────────────────────────────────┐│
  ↓                                                         ││
Task 11 (Browser WS Extensions) ──────────────────────────┐││
  ↓                                                        │││
Task 12 (Log WS Endpoint) ───────────────────────────────┐│││
  ↓                                                       ││││
Task 13 (Cap Revocation + Cleanup)                        ││││
                                                          ││││
Task 14 (Agent DockerManager Core) ←── depends on Task 3 ─┘│││
  ↓                                                         │││
Task 15 (Agent Logs/Events/Networks/Volumes)                │││
  ↓                                                         │││
Task 16 (Agent Reporter Integration)                        │││
                                                            │││
Task 17 (Frontend Types + WsClient) ←── independent ───────┘││
  ↓                                                          ││
Task 18 (WS Context + Message Handlers)                      ││
  ↓                                                          ││
Task 19 (Docker Hooks) ←── depends on Task 18 ──────────────┘│
  ↓                                                           │
Task 20 (Docker Tab Page)                                     │
  ↓                                                           │
Task 21 (Container List)                                      │
  ↓                                                           │
Task 22 (Container Detail Dialog)                             │
  ↓                                                           │
Task 23 (Networks + Volumes Dialogs)                          │
  ↓                                                           │
Task 24 (DTO Wiring) ←── depends on all above ───────────────┘
  ↓
Task 25 (Integration Testing)
```

**Parallelizable groups:**
- Tasks 14-16 (Agent) can be done in parallel with Tasks 9-13 (Server API/WS)
- Tasks 17-19 (Frontend infra) can be done in parallel with Tasks 14-16 (Agent)
- Tasks 20-23 (Frontend UI) must be sequential but can start as soon as Tasks 17-19 are done
