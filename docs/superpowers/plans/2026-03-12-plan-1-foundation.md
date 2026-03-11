# Plan 1: Foundation — Cargo Workspace + Common + Server Skeleton + DB + Auth + REST API

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the complete server-side foundation: Cargo workspace, shared types, database schema, authentication, and core REST API endpoints.

**Architecture:** Cargo workspace with 3 crates (common, server, agent). Server uses Axum + sea-orm + SQLite. Auth via Session + API Key. All REST APIs return unified JSON responses.

**Tech Stack:** Rust 1.85+, Axum 0.8, sea-orm 1.x (SQLite), tokio, argon2, uuid, figment, tracing, tower-http, utoipa

---

## Chunk 1: Workspace + Common Crate

### Task 1: Initialize Cargo Workspace

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/common/Cargo.toml`
- Create: `crates/common/src/lib.rs`
- Create: `crates/server/Cargo.toml`
- Create: `crates/server/src/main.rs`
- Create: `crates/agent/Cargo.toml`
- Create: `crates/agent/src/main.rs`

- [ ] **Step 1: Create workspace Cargo.toml**

```toml
[workspace]
resolver = "2"
members = [
    "crates/common",
    "crates/server",
    "crates/agent",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"
repository = "https://github.com/ZingerLittleBee/ServerBee"

[workspace.dependencies]
serverbee-common = { path = "crates/common" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
uuid = { version = "1", features = ["v4", "serde"] }
thiserror = "2"
anyhow = "1"
```

- [ ] **Step 2: Create common crate Cargo.toml**

```toml
[package]
name = "serverbee-common"
version.workspace = true
edition.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
uuid.workspace = true
```

- [ ] **Step 3: Create common/src/lib.rs with module declarations**

```rust
pub mod constants;
pub mod protocol;
pub mod types;
```

- [ ] **Step 4: Create server crate Cargo.toml**

```toml
[package]
name = "serverbee-server"
version.workspace = true
edition.workspace = true

[dependencies]
serverbee-common.workspace = true
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
uuid.workspace = true
thiserror.workspace = true
anyhow.workspace = true

axum = { version = "0.8", features = ["ws", "macros"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace", "fs"] }
sea-orm = { version = "1", features = ["sqlx-sqlite", "runtime-tokio-rustls", "macros"] }
sea-orm-migration = { version = "1", features = ["sqlx-sqlite", "runtime-tokio-rustls"] }
argon2 = "0.5"
rand = "0.8"
base64 = "0.22"
dashmap = "6"
figment = { version = "0.10", features = ["toml", "env"] }
utoipa = { version = "5", features = ["axum_extras", "chrono", "uuid"] }
utoipa-swagger-ui = { version = "9", features = ["axum"] }
tokio-cron-scheduler = "0.13"
```

- [ ] **Step 5: Create server/src/main.rs with minimal Axum server**

```rust
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    tracing::info!("ServerBee starting...");

    let listener = tokio::net::TcpListener::bind("0.0.0.0:9527").await?;
    tracing::info!("Listening on {}", listener.local_addr()?);

    let app = axum::Router::new().route("/healthz", axum::routing::get(|| async { "ok" }));

    axum::serve(listener, app).await?;

    Ok(())
}
```

- [ ] **Step 6: Create agent crate Cargo.toml**

```toml
[package]
name = "serverbee-agent"
version.workspace = true
edition.workspace = true

[dependencies]
serverbee-common.workspace = true
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
tokio.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
uuid.workspace = true
anyhow.workspace = true
```

- [ ] **Step 7: Create agent/src/main.rs placeholder**

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    tracing::info!("ServerBee Agent starting...");
    Ok(())
}
```

- [ ] **Step 8: Verify workspace compiles**

Run: `cargo build`
Expected: All 3 crates compile successfully.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock crates/
git commit -m "feat: initialize Cargo workspace with common, server, and agent crates"
```

### Task 2: Define Common Types

**Files:**
- Create: `crates/common/src/types.rs`
- Create: `crates/common/src/constants.rs`

- [ ] **Step 1: Write types.rs with all shared data types**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub cpu_name: String,
    pub cpu_cores: i32,
    pub cpu_arch: String,
    pub os: String,
    pub kernel_version: String,
    pub mem_total: i64,
    pub swap_total: i64,
    pub disk_total: i64,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub virtualization: Option<String>,
    pub agent_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemReport {
    pub cpu: f64,
    pub mem_used: i64,
    pub swap_used: i64,
    pub disk_used: i64,
    pub net_in_speed: i64,
    pub net_out_speed: i64,
    pub net_in_transfer: i64,
    pub net_out_transfer: i64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub tcp_conn: i32,
    pub udp_conn: i32,
    pub process_count: i32,
    pub uptime: u64,
    pub temperature: Option<f64>,
    pub gpu: Option<GpuReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuReport {
    pub count: i32,
    pub average_usage: f64,
    pub detailed_info: Vec<GpuInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub mem_total: i64,
    pub mem_used: i64,
    pub utilization: f64,
    pub temperature: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingTaskConfig {
    pub task_id: String,
    pub probe_type: String,
    pub target: String,
    pub interval: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResult {
    pub task_id: String,
    pub latency: f64,
    pub success: bool,
    pub error: Option<String>,
    pub time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub output: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatus {
    pub id: String,
    pub name: String,
    pub online: bool,
    pub last_active: i64,
    pub uptime: u64,
    pub cpu: f64,
    pub mem_used: i64,
    pub mem_total: i64,
    pub swap_used: i64,
    pub swap_total: i64,
    pub disk_used: i64,
    pub disk_total: i64,
    pub net_in_speed: i64,
    pub net_out_speed: i64,
    pub net_in_transfer: i64,
    pub net_out_transfer: i64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub tcp_conn: i32,
    pub udp_conn: i32,
    pub process_count: i32,
    pub cpu_name: Option<String>,
    pub os: Option<String>,
    pub region: Option<String>,
    pub country_code: Option<String>,
}
```

- [ ] **Step 2: Write constants.rs**

```rust
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_SERVER_PORT: u16 = 9527;
pub const DEFAULT_REPORT_INTERVAL: u32 = 3;
pub const PROTOCOL_VERSION: u32 = 1;

pub const SESSION_TTL_SECS: i64 = 86400;
pub const HEARTBEAT_INTERVAL_SECS: u64 = 30;
pub const OFFLINE_THRESHOLD_SECS: u64 = 30;

pub const MAX_WS_MESSAGE_SIZE: usize = 1024 * 1024; // 1MB
pub const MAX_TASK_OUTPUT_SIZE: usize = 512 * 1024;  // 512KB
pub const MAX_BINARY_FRAME_SIZE: usize = 64 * 1024;  // 64KB
pub const MAX_COMMAND_SIZE: usize = 8 * 1024;        // 8KB
pub const MAX_CONCURRENT_COMMANDS: usize = 5;
pub const MAX_TERMINAL_SESSIONS: usize = 3;
pub const TERMINAL_IDLE_TIMEOUT_SECS: u64 = 600;     // 10 min
pub const DEFAULT_COMMAND_TIMEOUT_SECS: u32 = 300;    // 5 min

pub const RECORDS_RETENTION_DAYS: u32 = 7;
pub const RECORDS_HOURLY_RETENTION_DAYS: u32 = 90;
pub const GPU_RECORDS_RETENTION_DAYS: u32 = 7;
pub const PING_RECORDS_RETENTION_DAYS: u32 = 7;
pub const AUDIT_LOGS_RETENTION_DAYS: u32 = 180;

pub const ALERT_DEBOUNCE_SECS: u64 = 300; // 5 min
pub const ALERT_SAMPLE_MINUTES: u32 = 10;
pub const ALERT_TRIGGER_RATIO: f64 = 0.7;

pub const API_KEY_PREFIX: &str = "sb_";
pub const API_KEY_PREFIX_LEN: usize = 8;
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-common`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/common/
git commit -m "feat(common): define shared types and constants"
```

### Task 3: Define Protocol Messages

**Files:**
- Create: `crates/common/src/protocol.rs`

- [ ] **Step 1: Write protocol.rs**

```rust
use serde::{Deserialize, Serialize};

use crate::types::{PingResult, PingTaskConfig, SystemInfo, SystemReport, TaskResult};

/// Agent -> Server messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentMessage {
    SystemInfo {
        msg_id: String,
        #[serde(flatten)]
        info: SystemInfo,
    },
    Report(SystemReport),
    PingResult(PingResult),
    TaskResult {
        msg_id: String,
        #[serde(flatten)]
        result: TaskResult,
    },
    Pong,
}

/// Server -> Agent messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Welcome {
        server_id: String,
        protocol_version: u32,
        report_interval: u32,
    },
    Ack {
        msg_id: String,
    },
    PingTasksSync {
        tasks: Vec<PingTaskConfig>,
    },
    Exec {
        task_id: String,
        command: String,
        timeout: Option<u32>,
    },
    TerminalClose {
        session_id: String,
    },
    Ping,
    Upgrade {
        version: String,
        download_url: String,
    },
}

/// Server -> Browser messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserMessage {
    FullSync {
        servers: Vec<crate::types::ServerStatus>,
    },
    Update {
        servers: Vec<crate::types::ServerStatus>,
    },
    ServerOnline {
        server_id: String,
    },
    ServerOffline {
        server_id: String,
    },
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p serverbee-common`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/common/src/protocol.rs
git commit -m "feat(common): define Agent/Server/Browser protocol messages"
```

## Chunk 2: Database Entities + Migrations

### Task 4: Create sea-orm Entities

**Files:**
- Create: `crates/server/src/entity/mod.rs`
- Create: `crates/server/src/entity/user.rs`
- Create: `crates/server/src/entity/session.rs`
- Create: `crates/server/src/entity/api_key.rs`
- Create: `crates/server/src/entity/server.rs`
- Create: `crates/server/src/entity/server_group.rs`
- Create: `crates/server/src/entity/server_tag.rs`
- Create: `crates/server/src/entity/record.rs`
- Create: `crates/server/src/entity/record_hourly.rs`
- Create: `crates/server/src/entity/gpu_record.rs`
- Create: `crates/server/src/entity/config.rs`
- Create: `crates/server/src/entity/alert_rule.rs`
- Create: `crates/server/src/entity/alert_state.rs`
- Create: `crates/server/src/entity/notification.rs`
- Create: `crates/server/src/entity/notification_group.rs`
- Create: `crates/server/src/entity/ping_task.rs`
- Create: `crates/server/src/entity/ping_record.rs`
- Create: `crates/server/src/entity/task.rs`
- Create: `crates/server/src/entity/task_result.rs`
- Create: `crates/server/src/entity/audit_log.rs`
- Modify: `crates/server/src/main.rs` — add `mod entity;`

- [ ] **Step 1: Create entity/mod.rs**

```rust
pub mod api_key;
pub mod alert_rule;
pub mod alert_state;
pub mod audit_log;
pub mod config;
pub mod gpu_record;
pub mod notification;
pub mod notification_group;
pub mod ping_record;
pub mod ping_task;
pub mod record;
pub mod record_hourly;
pub mod server;
pub mod server_group;
pub mod server_tag;
pub mod session;
pub mod task;
pub mod task_result;
pub mod user;
```

- [ ] **Step 2: Create entity/user.rs**

```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub username: String,
    pub password_hash: String,
    pub role: String,
    pub totp_secret: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::session::Entity")]
    Sessions,
    #[sea_orm(has_many = "super::api_key::Entity")]
    ApiKeys,
}

impl Related<super::session::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Sessions.def()
    }
}

impl Related<super::api_key::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ApiKeys.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 3: Create entity/session.rs**

```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "sessions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub user_id: String,
    #[sea_orm(unique)]
    pub token: String,
    pub ip: String,
    pub user_agent: String,
    pub expires_at: DateTimeUtc,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 4: Create entity/api_key.rs**

```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "api_keys")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(indexed)]
    pub user_id: String,
    pub name: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub last_used_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user::Entity",
        from = "Column::UserId",
        to = "super::user::Column::Id"
    )]
    User,
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 5: Create entity/server.rs**

```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "servers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub token_hash: String,
    pub token_prefix: String,
    pub name: String,
    pub cpu_name: Option<String>,
    pub cpu_cores: Option<i32>,
    pub cpu_arch: Option<String>,
    pub os: Option<String>,
    pub kernel_version: Option<String>,
    pub mem_total: Option<i64>,
    pub swap_total: Option<i64>,
    pub disk_total: Option<i64>,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub region: Option<String>,
    pub country_code: Option<String>,
    pub virtualization: Option<String>,
    pub agent_version: Option<String>,
    pub group_id: Option<String>,
    #[sea_orm(default_value = "0")]
    pub weight: i32,
    #[sea_orm(default_value = "false")]
    pub hidden: bool,
    pub remark: Option<String>,
    pub public_remark: Option<String>,
    pub price: Option<f64>,
    pub billing_cycle: Option<String>,
    pub currency: Option<String>,
    pub expired_at: Option<DateTimeUtc>,
    pub traffic_limit: Option<i64>,
    pub traffic_limit_type: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::server_group::Entity",
        from = "Column::GroupId",
        to = "super::server_group::Column::Id"
    )]
    Group,
}

impl Related<super::server_group::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Group.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 6: Create remaining entity files (server_group, server_tag, record, record_hourly, gpu_record, config, alert_rule, alert_state, notification, notification_group, ping_task, ping_record, task, task_result, audit_log)**

Each follows the same sea-orm pattern. Create all of them matching the schema in the design spec.

**entity/server_group.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "server_groups")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub name: String,
    #[sea_orm(default_value = "0")]
    pub weight: i32,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::server::Entity")]
    Servers,
}

impl Related<super::server::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Servers.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

**entity/server_tag.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "server_tags")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub server_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub tag: String,
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

**entity/record.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "records")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(indexed)]
    pub server_id: String,
    #[sea_orm(indexed)]
    pub time: DateTimeUtc,
    pub cpu: f64,
    pub mem_used: i64,
    pub swap_used: i64,
    pub disk_used: i64,
    pub net_in_speed: i64,
    pub net_out_speed: i64,
    pub net_in_transfer: i64,
    pub net_out_transfer: i64,
    pub load1: f64,
    pub load5: f64,
    pub load15: f64,
    pub tcp_conn: i32,
    pub udp_conn: i32,
    pub process_count: i32,
    pub temperature: Option<f64>,
    pub gpu_usage: Option<f64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**entity/record_hourly.rs:** Same as record.rs but `table_name = "records_hourly"`.

**entity/gpu_record.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "gpu_records")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(indexed)]
    pub server_id: String,
    #[sea_orm(indexed)]
    pub time: DateTimeUtc,
    pub device_index: i32,
    pub device_name: String,
    pub mem_total: i64,
    pub mem_used: i64,
    pub utilization: f64,
    pub temperature: f64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**entity/config.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "configs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: String,
    pub value: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**entity/alert_rule.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "alert_rules")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    #[sea_orm(default_value = "true")]
    pub enabled: bool,
    pub rules_json: String,
    pub trigger_mode: String,
    pub notification_group_id: Option<String>,
    pub fail_trigger_tasks: Option<String>,
    pub recover_trigger_tasks: Option<String>,
    pub cover_type: String,
    pub server_ids_json: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**entity/alert_state.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "alert_states")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(indexed)]
    pub rule_id: String,
    #[sea_orm(indexed)]
    pub server_id: String,
    pub first_triggered_at: DateTimeUtc,
    pub last_notified_at: DateTimeUtc,
    pub count: i32,
    #[sea_orm(default_value = "false")]
    pub resolved: bool,
    pub resolved_at: Option<DateTimeUtc>,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**entity/notification.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "notifications")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub notify_type: String,
    pub config_json: String,
    #[sea_orm(default_value = "true")]
    pub enabled: bool,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**entity/notification_group.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "notification_groups")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub notification_ids_json: String,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**entity/ping_task.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "ping_tasks")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub probe_type: String,
    pub target: String,
    pub interval: i32,
    pub server_ids_json: String,
    #[sea_orm(default_value = "true")]
    pub enabled: bool,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**entity/ping_record.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "ping_records")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(indexed)]
    pub task_id: String,
    #[sea_orm(indexed)]
    pub server_id: String,
    pub latency: f64,
    pub success: bool,
    pub error: Option<String>,
    #[sea_orm(indexed)]
    pub time: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**entity/task.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "tasks")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub command: String,
    pub server_ids_json: String,
    pub created_by: String,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**entity/task_result.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "task_results")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub task_id: String,
    pub server_id: String,
    pub output: String,
    pub exit_code: i32,
    pub finished_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

**entity/audit_log.rs:**
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "audit_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: String,
    pub action: String,
    pub detail: Option<String>,
    pub ip: String,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 7: Add `mod entity;` to main.rs**

Add `mod entity;` at the top of `crates/server/src/main.rs`.

- [ ] **Step 8: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add crates/server/src/entity/
git commit -m "feat(server): define all sea-orm entities matching design spec"
```

### Task 5: Create sea-orm Migrations

**Files:**
- Create: `crates/server/src/migration/mod.rs`
- Create: `crates/server/src/migration/m20260312_000001_init.rs`

- [ ] **Step 1: Create migration module**

```rust
// crates/server/src/migration/mod.rs
use sea_orm_migration::prelude::*;

mod m20260312_000001_init;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20260312_000001_init::Migration)]
    }
}
```

- [ ] **Step 2: Create init migration with all tables**

Create `crates/server/src/migration/m20260312_000001_init.rs` with CREATE TABLE statements for all 20 tables from the design spec, including indexes.

This migration creates: users, sessions, api_keys, servers, server_groups, server_tags, records, records_hourly, gpu_records, configs, alert_rules, alert_states, notifications, notification_groups, ping_tasks, ping_records, tasks, task_results, audit_logs.

Key indexes: `(server_id, time)` composite on records/records_hourly/gpu_records, `(task_id, server_id, time)` on ping_records, `(rule_id, server_id)` unique on alert_states.

- [ ] **Step 3: Add `async-trait` to server Cargo.toml**

Add: `async-trait = "0.1"`

- [ ] **Step 4: Add `mod migration;` to main.rs**

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/migration/
git commit -m "feat(server): add database migration for all tables"
```

## Chunk 3: Config + AppState + Server Bootstrap

### Task 6: Server Configuration

**Files:**
- Create: `crates/server/src/config.rs`

- [ ] **Step 1: Write config.rs using figment**

```rust
use figment::{providers::{Env, Format, Toml}, Figment};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub admin: AdminConfig,
    #[serde(default)]
    pub retention: RetentionConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub geoip: GeoIpConfig,
    #[serde(default)]
    pub log: LogConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DatabaseConfig {
    #[serde(default = "default_db_path")]
    pub path: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(default = "default_session_ttl")]
    pub session_ttl: i64,
    #[serde(default)]
    pub auto_discovery_key: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AdminConfig {
    #[serde(default = "default_admin_username")]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetentionConfig {
    #[serde(default = "default_7")]
    pub records_days: u32,
    #[serde(default = "default_90")]
    pub records_hourly_days: u32,
    #[serde(default = "default_7")]
    pub gpu_records_days: u32,
    #[serde(default = "default_7")]
    pub ping_records_days: u32,
    #[serde(default = "default_180")]
    pub audit_logs_days: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RateLimitConfig {
    #[serde(default = "default_5")]
    pub login_max: u32,
    #[serde(default = "default_3")]
    pub register_max: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct GeoIpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mmdb_path: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LogConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default)]
    pub file: String,
}

fn default_server() -> ServerConfig {
    ServerConfig {
        listen: default_listen(),
        data_dir: default_data_dir(),
    }
}

fn default_listen() -> String { "0.0.0.0:9527".to_string() }
fn default_data_dir() -> String { "./data".to_string() }
fn default_db_path() -> String { "serverbee.db".to_string() }
fn default_max_connections() -> u32 { 10 }
fn default_session_ttl() -> i64 { 86400 }
fn default_admin_username() -> String { "admin".to_string() }
fn default_log_level() -> String { "info".to_string() }
fn default_7() -> u32 { 7 }
fn default_90() -> u32 { 90 }
fn default_180() -> u32 { 180 }
fn default_5() -> u32 { 5 }
fn default_3() -> u32 { 3 }

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            records_days: 7,
            records_hourly_days: 90,
            gpu_records_days: 7,
            ping_records_days: 7,
            audit_logs_days: 180,
        }
    }
}

impl AppConfig {
    pub fn load() -> anyhow::Result<Self> {
        let config: AppConfig = Figment::new()
            .merge(Toml::file("/etc/serverbee/server.toml"))
            .merge(Toml::file("server.toml"))
            .merge(Env::prefixed("SB_").split("_"))
            .extract()?;
        Ok(config)
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/config.rs
git commit -m "feat(server): add configuration loading with figment"
```

### Task 7: AppState + Database Bootstrap

**Files:**
- Create: `crates/server/src/state.rs`
- Modify: `crates/server/src/main.rs`

- [ ] **Step 1: Write state.rs**

```rust
use std::sync::Arc;

use sea_orm::DatabaseConnection;
use tokio::sync::broadcast;

use serverbee_common::protocol::BrowserMessage;

use crate::config::AppConfig;

pub struct AppState {
    pub db: DatabaseConnection,
    pub browser_tx: broadcast::Sender<BrowserMessage>,
    pub config: AppConfig,
}

impl AppState {
    pub fn new(db: DatabaseConnection, config: AppConfig) -> Arc<Self> {
        let (browser_tx, _) = broadcast::channel(256);
        Arc::new(Self {
            db,
            browser_tx,
            config,
        })
    }
}
```

- [ ] **Step 2: Update main.rs with database connection + migration**

```rust
mod config;
mod entity;
mod migration;
mod state;

use sea_orm::{ConnectOptions, Database};
use sea_orm_migration::MigratorTrait;
use tracing_subscriber::EnvFilter;

use crate::config::AppConfig;
use crate::migration::Migrator;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = AppConfig::load().unwrap_or_default();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.log.level.parse().unwrap_or_else(|_| "info".into())),
        )
        .init();

    tracing::info!("ServerBee v{} starting...", serverbee_common::constants::VERSION);

    // Ensure data dir exists
    let data_dir = &config.server.data_dir;
    std::fs::create_dir_all(data_dir)?;

    // Connect database
    let db_path = format!("{}/{}", data_dir, config.database.path);
    let db_url = format!("sqlite://{}?mode=rwc", db_path);

    let mut opt = ConnectOptions::new(&db_url);
    opt.max_connections(config.database.max_connections);
    opt.sqlx_logging(false);

    let db = Database::connect(opt).await?;

    // SQLite pragmas
    use sea_orm::ConnectionTrait;
    db.execute_unprepared("PRAGMA journal_mode=WAL").await?;
    db.execute_unprepared("PRAGMA synchronous=NORMAL").await?;
    db.execute_unprepared("PRAGMA busy_timeout=5000").await?;

    // Run migrations
    Migrator::up(&db, None).await?;
    tracing::info!("Database migrations complete");

    // Build AppState
    let state = AppState::new(db, config.clone());

    // Build router
    let app = axum::Router::new()
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.server.listen).await?;
    tracing::info!("Listening on {}", listener.local_addr()?);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Server stopped");
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C signal handler");
    tracing::info!("Shutdown signal received");
}
```

- [ ] **Step 3: Verify it compiles and runs**

Run: `cargo run -p serverbee-server`
Expected: Server starts, creates data directory, SQLite database, runs migrations, listens on :9527. Ctrl+C gracefully shuts down.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/state.rs crates/server/src/main.rs
git commit -m "feat(server): add AppState, database bootstrap with migrations"
```

## Chunk 4: Auth Service + Middleware

### Task 8: Auth Service

**Files:**
- Create: `crates/server/src/service/mod.rs`
- Create: `crates/server/src/service/auth.rs`

- [ ] **Step 1: Create service/mod.rs**

```rust
pub mod auth;
```

- [ ] **Step 2: Write service/auth.rs**

Implements:
- `hash_password(password: &str) -> Result<String>` using argon2
- `verify_password(password: &str, hash: &str) -> Result<bool>`
- `create_user(db, username, password, role) -> Result<user::Model>`
- `login(db, username, password, ip, user_agent) -> Result<(session::Model, user::Model)>`
- `validate_session(db, token) -> Result<Option<user::Model>>` — also extends session TTL
- `logout(db, token) -> Result<()>`
- `create_api_key(db, user_id, name) -> Result<(api_key::Model, String)>` — returns plaintext key once
- `validate_api_key(db, key) -> Result<Option<user::Model>>`
- `validate_agent_token(db, token) -> Result<Option<server::Model>>`
- `init_admin(db, config) -> Result<()>` — creates admin user if none exists
- `generate_session_token() -> String` — 32 random bytes -> base64url
- `generate_api_key() -> String` — "sb_" + 32 random bytes -> base64url

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/
git commit -m "feat(server): implement auth service (password hashing, sessions, API keys)"
```

### Task 9: Auth Middleware

**Files:**
- Create: `crates/server/src/middleware/mod.rs`
- Create: `crates/server/src/middleware/auth.rs`

- [ ] **Step 1: Create middleware/mod.rs**

```rust
pub mod auth;
```

- [ ] **Step 2: Write middleware/auth.rs**

```rust
use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::service::auth::AuthService;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: String,
    pub username: String,
    pub role: String,
}

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    // Try session cookie first
    let current_user = if let Some(token) = extract_session_cookie(&req) {
        AuthService::validate_session(&state.db, &token)
            .await
            .ok()
            .flatten()
            .map(|user| CurrentUser {
                user_id: user.id.clone(),
                username: user.username.clone(),
                role: user.role.clone(),
            })
    } else {
        None
    };

    // Try API key header
    let current_user = match current_user {
        Some(u) => Some(u),
        None => {
            if let Some(key) = extract_api_key(&req) {
                AuthService::validate_api_key(&state.db, &key)
                    .await
                    .ok()
                    .flatten()
                    .map(|user| CurrentUser {
                        user_id: user.id.clone(),
                        username: user.username.clone(),
                        role: user.role.clone(),
                    })
            } else {
                None
            }
        }
    };

    match current_user {
        Some(user) => {
            req.extensions_mut().insert(user);
            next.run(req).await
        }
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}

fn extract_session_cookie(req: &Request) -> Option<String> {
    req.headers()
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|cookie| {
            let cookie = cookie.trim();
            cookie
                .strip_prefix("session_token=")
                .map(|v| v.to_string())
        })
}

fn extract_api_key(req: &Request) -> Option<String> {
    req.headers()
        .get("x-api-key")?
        .to_str()
        .ok()
        .map(|s| s.to_string())
}
```

- [ ] **Step 3: Add `mod middleware;` to main.rs**

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/middleware/
git commit -m "feat(server): implement auth middleware (session + API key)"
```

## Chunk 5: Core REST API

### Task 10: Unified Response Types + Error Handling

**Files:**
- Create: `crates/server/src/error.rs`

- [ ] **Step 1: Write error.rs with unified response format**

```rust
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub data: T,
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Forbidden")]
    Forbidden,
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "BAD_REQUEST"),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "FORBIDDEN"),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            AppError::Conflict(_) => (StatusCode::CONFLICT, "CONFLICT"),
            AppError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        };

        let body = ErrorBody {
            error: ErrorDetail {
                code: code.to_string(),
                message: self.to_string(),
            },
        };

        (status, Json(body)).into_response()
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(err: sea_orm::DbErr) -> Self {
        tracing::error!("Database error: {err}");
        AppError::Internal("Database error".to_string())
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        tracing::error!("Internal error: {err}");
        AppError::Internal("Internal error".to_string())
    }
}

pub type ApiResult<T> = Result<Json<ApiResponse<T>>, AppError>;

pub fn ok<T: Serialize>(data: T) -> Result<Json<ApiResponse<T>>, AppError> {
    Ok(Json(ApiResponse { data }))
}
```

- [ ] **Step 2: Add `mod error;` to main.rs**

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/error.rs
git commit -m "feat(server): add unified API response types and error handling"
```

### Task 11: Auth API Endpoints

**Files:**
- Create: `crates/server/src/router/mod.rs`
- Create: `crates/server/src/router/api/mod.rs`
- Create: `crates/server/src/router/api/auth.rs`

- [ ] **Step 1: Create router/mod.rs**

```rust
pub mod api;

use std::sync::Arc;
use axum::Router;
use crate::state::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .nest("/api", api::router(state.clone()))
        .with_state(state)
}
```

- [ ] **Step 2: Create router/api/mod.rs**

```rust
pub mod auth;

use std::sync::Arc;
use axum::Router;
use crate::state::AppState;

pub fn router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .nest("/auth", auth::router())
}
```

- [ ] **Step 3: Write router/api/auth.rs**

Endpoints:
- `POST /api/auth/login` — body: `{ username, password }`, returns user + set-cookie
- `POST /api/auth/logout` — clears session
- `GET /api/auth/me` — returns current user
- `POST /api/auth/api-keys` — body: `{ name }`, returns key (plaintext, one-time)
- `GET /api/auth/api-keys` — list user's API keys
- `DELETE /api/auth/api-keys/:id` — delete API key
- `PUT /api/auth/password` — body: `{ old_password, new_password }`

- [ ] **Step 4: Update main.rs to use create_router()**

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/router/
git commit -m "feat(server): implement auth API endpoints (login, logout, me, API keys)"
```

### Task 12: Server Management Service + API

**Files:**
- Create: `crates/server/src/service/server.rs`
- Create: `crates/server/src/router/api/server.rs`

- [ ] **Step 1: Write service/server.rs**

Implements:
- `list_servers(db) -> Result<Vec<server::Model>>`
- `get_server(db, id) -> Result<server::Model>`
- `update_server(db, id, UpdateServerDto) -> Result<server::Model>`
- `delete_server(db, id) -> Result<()>`
- `batch_delete(db, ids) -> Result<u64>`
- `register_agent(db, auto_discovery_key, config_key) -> Result<(server::Model, String)>` — creates server, returns token plaintext
- `update_system_info(db, server_id, SystemInfo) -> Result<()>`

- [ ] **Step 2: Write router/api/server.rs**

Endpoints:
- `GET /api/servers` — list all servers
- `GET /api/servers/:id` — server details
- `PUT /api/servers/:id` — update server
- `DELETE /api/servers/:id` — delete server
- `POST /api/servers/batch-delete` — batch delete
- `GET /api/servers/:id/records` — query history records
- `GET /api/servers/:id/gpu-records` — query GPU records

- [ ] **Step 3: Add server routes to api/mod.rs**

- [ ] **Step 4: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/server.rs crates/server/src/router/api/server.rs
git commit -m "feat(server): implement server management service and API"
```

### Task 13: Server Groups + Config + Settings API

**Files:**
- Create: `crates/server/src/service/config.rs`
- Create: `crates/server/src/router/api/server_group.rs`
- Create: `crates/server/src/router/api/setting.rs`

- [ ] **Step 1: Write service/config.rs**

Implements:
- `get(db, key) -> Result<Option<String>>`
- `set(db, key, value) -> Result<()>`
- `get_typed<T: DeserializeOwned>(db, key) -> Result<Option<T>>`
- `set_typed<T: Serialize>(db, key, value) -> Result<()>`

- [ ] **Step 2: Write router/api/server_group.rs**

CRUD for server groups (GET, POST, PUT, DELETE).

- [ ] **Step 3: Write router/api/setting.rs**

- `GET /api/settings` — returns system settings from configs table
- `PUT /api/settings` — updates system settings
- `GET /api/settings/auto-discovery-key` — returns current key
- `PUT /api/settings/auto-discovery-key` — regenerates key

- [ ] **Step 4: Add routes to api/mod.rs**

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/service/config.rs crates/server/src/router/api/server_group.rs crates/server/src/router/api/setting.rs
git commit -m "feat(server): add server groups, config service, and settings API"
```

### Task 14: Record Service

**Files:**
- Create: `crates/server/src/service/record.rs`

- [ ] **Step 1: Write service/record.rs**

Implements:
- `save_report(db, server_id, report) -> Result<()>` — inserts into records table
- `query_history(db, server_id, from, to, interval) -> Result<Vec<record::Model>>` — auto/raw/hourly
- `query_gpu_history(db, server_id, from, to) -> Result<Vec<gpu_record::Model>>`
- `aggregate_hourly(db) -> Result<u64>` — aggregates last hour's records into records_hourly
- `cleanup_expired(db, retention) -> Result<u64>` — deletes records older than retention period

- [ ] **Step 2: Add to service/mod.rs**

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/record.rs
git commit -m "feat(server): implement record service (save, query, aggregate, cleanup)"
```

### Task 15: Agent Registration Endpoint

**Files:**
- Create: `crates/server/src/router/api/agent.rs`

- [ ] **Step 1: Write router/api/agent.rs**

- `POST /api/agent/register` — validates Bearer auto_discovery_key, creates server, returns `{ server_id, token }`
- Agent WebSocket handler will be in Plan 3

- [ ] **Step 2: Add agent route to api/mod.rs**

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p serverbee-server`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/router/api/agent.rs
git commit -m "feat(server): add agent registration endpoint"
```

### Task 16: Admin Init + Startup Integration

**Files:**
- Modify: `crates/server/src/main.rs`

- [ ] **Step 1: Add admin user initialization to main.rs**

After migrations, call `AuthService::init_admin(&db, &config.admin)`. If users table is empty, create admin user. If password not configured, generate random one and log it.

- [ ] **Step 2: Generate auto_discovery_key if not set**

Call `ConfigService::get(&db, "auto_discovery_key")`. If empty, generate random key, store it, and log it.

- [ ] **Step 3: Verify full startup flow**

Run: `cargo run -p serverbee-server`
Expected: Starts, creates DB, runs migrations, creates admin user (logs password), generates discovery key, listens on :9527. Test `GET /healthz` returns "ok". Test `POST /api/auth/login` with admin credentials returns session cookie.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/main.rs
git commit -m "feat(server): add admin init, discovery key generation, and full startup flow"
```

## Chunk 6: OpenAPI + Final Integration

### Task 17: OpenAPI Documentation

**Files:**
- Modify: `crates/server/src/router/mod.rs`

- [ ] **Step 1: Add utoipa OpenAPI spec generation**

Add `#[derive(utoipa::ToSchema)]` to request/response types. Add `#[utoipa::path(...)]` annotations to handler functions. Mount Swagger UI at `/swagger-ui/` and JSON spec at `/api/openapi.json`.

- [ ] **Step 2: Verify Swagger UI loads**

Run: `cargo run -p serverbee-server`
Open: `http://localhost:9527/swagger-ui/`
Expected: Swagger UI with all endpoints documented

- [ ] **Step 3: Commit**

```bash
git add crates/server/
git commit -m "feat(server): add OpenAPI documentation with Swagger UI"
```

### Task 18: CORS + Logging Middleware

**Files:**
- Modify: `crates/server/src/router/mod.rs`

- [ ] **Step 1: Add tower-http CORS and tracing middleware**

```rust
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

// In create_router():
let cors = CorsLayer::new()
    .allow_origin(Any)  // Dev mode; production behind reverse proxy
    .allow_methods(Any)
    .allow_headers(Any);

Router::new()
    // ... routes ...
    .layer(TraceLayer::new_for_http())
    .layer(cors)
```

- [ ] **Step 2: Verify it works**

Run: `cargo run -p serverbee-server`
Expected: Request logging in console, CORS headers in responses

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/router/mod.rs
git commit -m "feat(server): add CORS and request tracing middleware"
```
