# P9: Service Monitor Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a server-side monitoring engine that checks SSL certificates, DNS records, HTTP keywords, TCP ports, and domain WHOIS expiry on configurable intervals, with alerting and a management UI.

**Architecture:** New `service_monitor` / `service_monitor_record` tables with a background task that ticks every 10s, dispatches due checks via `tokio::spawn` (semaphore-limited to 20), writes results, and triggers notifications on failure. Fully independent from the Agent-side Ping system.

**Tech Stack:** Rust (Axum, sea-orm, tokio), `x509-parser`, `hickory-resolver`, `whois-rust`, `reqwest`; React (TanStack Router/Query, shadcn/ui, Recharts)

**Spec:** `docs/superpowers/specs/2026-03-19-batch1-batch2-features-design.md` Section 1

---

## File Structure

### New Files (Rust)
- `crates/server/src/migration/m20260319_000007_service_monitor.rs` — Migration: 2 tables + indexes
- `crates/server/src/entity/service_monitor.rs` — SeaORM entity
- `crates/server/src/entity/service_monitor_record.rs` — SeaORM entity
- `crates/server/src/service/service_monitor.rs` — CRUD service + DTOs
- `crates/server/src/service/checker/mod.rs` — ServiceChecker trait + dispatch
- `crates/server/src/service/checker/ssl.rs` — SSL certificate checker
- `crates/server/src/service/checker/dns.rs` — DNS record checker
- `crates/server/src/service/checker/http_keyword.rs` — HTTP keyword checker
- `crates/server/src/service/checker/tcp.rs` — TCP port checker
- `crates/server/src/service/checker/whois.rs` — WHOIS domain checker
- `crates/server/src/router/api/service_monitor.rs` — REST API handlers
- `crates/server/src/task/service_monitor_checker.rs` — Background execution engine

### New Files (Frontend)
- `apps/web/src/routes/_authed/settings/service-monitors.tsx` — CRUD management page
- `apps/web/src/routes/_authed/service-monitors/$id.tsx` — Detail page

### Modified Files
- `crates/server/Cargo.toml` — Add dependencies
- `crates/server/src/migration/mod.rs` — Register migration
- `crates/server/src/entity/mod.rs` — Register entities
- `crates/server/src/service/mod.rs` — Register service + checker modules
- `crates/server/src/router/api/mod.rs` — Register route module
- `crates/server/src/task/mod.rs` — Register task module
- `crates/server/src/config.rs` — Add `service_monitor_days` to RetentionConfig
- `crates/server/src/task/cleanup.rs` — Add cleanup for service_monitor_record
- `crates/server/src/openapi.rs` — Register endpoints + schemas + tag
- `crates/server/src/main.rs` — Spawn service_monitor_checker task
- `apps/web/src/components/layout/sidebar.tsx` — Add sidebar entry

---

### Task 1: Add Dependencies

**Files:**
- Modify: `crates/server/Cargo.toml`

- [ ] **Step 1: Add new crate dependencies**

Add to `[dependencies]` in `crates/server/Cargo.toml`:

```toml
x509-parser = "0.16"
hickory-resolver = "0.25"
whois-rust = "1"
```

`reqwest` is already present. Verify with `cargo check -p serverbee-server` that the dependencies resolve.

- [ ] **Step 2: Commit**

```bash
git add crates/server/Cargo.toml
git commit -m "chore(server): add x509-parser, hickory-resolver, whois-rust dependencies"
```

---

### Task 2: Database Migration

**Files:**
- Create: `crates/server/src/migration/m20260319_000007_service_monitor.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Create migration file**

Follow the pattern in `m20260318_000006_docker_support.rs`. Create `service_monitor` and `service_monitor_record` tables with indexes.

```rust
// crates/server/src/migration/m20260319_000007_service_monitor.rs
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // service_monitor table
        manager
            .create_table(
                Table::create()
                    .table(ServiceMonitor::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(ServiceMonitor::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(ServiceMonitor::Name).string().not_null())
                    .col(ColumnDef::new(ServiceMonitor::MonitorType).string().not_null())
                    .col(ColumnDef::new(ServiceMonitor::Target).string().not_null())
                    .col(ColumnDef::new(ServiceMonitor::Interval).integer().not_null().default(300))
                    .col(ColumnDef::new(ServiceMonitor::ConfigJson).text().not_null().default("{}"))
                    .col(ColumnDef::new(ServiceMonitor::NotificationGroupId).string().null())
                    .col(ColumnDef::new(ServiceMonitor::RetryCount).integer().not_null().default(1))
                    .col(ColumnDef::new(ServiceMonitor::ServerIdsJson).text().null())
                    .col(ColumnDef::new(ServiceMonitor::Enabled).boolean().not_null().default(true))
                    .col(ColumnDef::new(ServiceMonitor::LastStatus).boolean().null())
                    .col(ColumnDef::new(ServiceMonitor::ConsecutiveFailures).integer().not_null().default(0))
                    .col(ColumnDef::new(ServiceMonitor::LastCheckedAt).timestamp().null())
                    .col(ColumnDef::new(ServiceMonitor::CreatedAt).timestamp().not_null().default(Expr::current_timestamp()))
                    .col(ColumnDef::new(ServiceMonitor::UpdatedAt).timestamp().not_null().default(Expr::current_timestamp()))
                    .to_owned(),
            )
            .await?;

        // Index on enabled for background task queries
        manager
            .create_index(
                Index::create()
                    .name("idx_service_monitor_enabled")
                    .table(ServiceMonitor::Table)
                    .col(ServiceMonitor::Enabled)
                    .to_owned(),
            )
            .await?;

        // service_monitor_record table
        manager
            .create_table(
                Table::create()
                    .table(ServiceMonitorRecord::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(ServiceMonitorRecord::Id).big_integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(ServiceMonitorRecord::MonitorId).string().not_null())
                    .col(ColumnDef::new(ServiceMonitorRecord::Success).boolean().not_null())
                    .col(ColumnDef::new(ServiceMonitorRecord::Latency).double().null())
                    .col(ColumnDef::new(ServiceMonitorRecord::DetailJson).text().not_null().default("{}"))
                    .col(ColumnDef::new(ServiceMonitorRecord::Error).text().null())
                    .col(ColumnDef::new(ServiceMonitorRecord::Time).timestamp().not_null().default(Expr::current_timestamp()))
                    .to_owned(),
            )
            .await?;

        // Composite index for history queries and retention cleanup
        manager
            .create_index(
                Index::create()
                    .name("idx_service_monitor_record_monitor_time")
                    .table(ServiceMonitorRecord::Table)
                    .col(ServiceMonitorRecord::MonitorId)
                    .col(ServiceMonitorRecord::Time)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Not reversible
        Ok(())
    }
}

#[derive(Iden)]
enum ServiceMonitor {
    Table,
    Id,
    Name,
    MonitorType,
    Target,
    Interval,
    ConfigJson,
    NotificationGroupId,
    RetryCount,
    ServerIdsJson,
    Enabled,
    LastStatus,
    ConsecutiveFailures,
    LastCheckedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum ServiceMonitorRecord {
    Table,
    Id,
    MonitorId,
    Success,
    Latency,
    DetailJson,
    Error,
    Time,
}
```

- [ ] **Step 2: Register migration in mod.rs**

In `crates/server/src/migration/mod.rs`, add:
- `mod m20260319_000007_service_monitor;` in the module declarations
- `Box::new(m20260319_000007_service_monitor::Migration),` at the end of the `vec![]`

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-server`
Expected: compiles without errors

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/migration/
git commit -m "feat(server): add service_monitor migration with tables and indexes"
```

---

### Task 3: SeaORM Entities

**Files:**
- Create: `crates/server/src/entity/service_monitor.rs`
- Create: `crates/server/src/entity/service_monitor_record.rs`
- Modify: `crates/server/src/entity/mod.rs`

- [ ] **Step 1: Create service_monitor entity**

Follow the pattern in `entity/ping_task.rs`:

```rust
// crates/server/src/entity/service_monitor.rs
use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = ServiceMonitor)]
#[sea_orm(table_name = "service_monitor")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub name: String,
    pub monitor_type: String,
    pub target: String,
    pub interval: i32,
    pub config_json: String,
    pub notification_group_id: Option<String>,
    pub retry_count: i32,
    pub server_ids_json: Option<String>,
    pub enabled: bool,
    pub last_status: Option<bool>,
    pub consecutive_failures: i32,
    #[schema(value_type = Option<String>, format = DateTime)]
    pub last_checked_at: Option<DateTimeUtc>,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: DateTimeUtc,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
#[allow(dead_code)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 2: Create service_monitor_record entity**

```rust
// crates/server/src/entity/service_monitor_record.rs
use sea_orm::entity::prelude::*;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, utoipa::ToSchema)]
#[schema(as = ServiceMonitorRecord)]
#[sea_orm(table_name = "service_monitor_record")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub monitor_id: String,
    pub success: bool,
    pub latency: Option<f64>,
    pub detail_json: String,
    pub error: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub time: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
#[allow(dead_code)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 3: Register entities in mod.rs**

Add to `crates/server/src/entity/mod.rs`:
```rust
pub mod service_monitor;
pub mod service_monitor_record;
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p serverbee-server`

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/entity/
git commit -m "feat(server): add service_monitor and service_monitor_record entities"
```

---

### Task 4: Service Monitor CRUD Service

**Files:**
- Create: `crates/server/src/service/service_monitor.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Write unit tests for the service**

At the bottom of the service file, write tests for create, list, get, update, delete. Follow the pattern in `service/ping.rs` for DTOs and methods.

- [ ] **Step 2: Create the service with DTOs and CRUD methods**

```rust
// crates/server/src/service/service_monitor.rs
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{service_monitor, service_monitor_record};
use crate::error::AppError;

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CreateServiceMonitor {
    pub name: String,
    pub monitor_type: String, // ssl, dns, http_keyword, tcp, whois
    pub target: String,
    #[serde(default = "default_interval")]
    pub interval: i32,
    #[serde(default)]
    pub config_json: serde_json::Value,
    pub notification_group_id: Option<String>,
    #[serde(default = "default_retry_count")]
    pub retry_count: i32,
    pub server_ids_json: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct UpdateServiceMonitor {
    pub name: Option<String>,
    pub target: Option<String>,
    pub interval: Option<i32>,
    pub config_json: Option<serde_json::Value>,
    pub notification_group_id: Option<Option<String>>,
    pub retry_count: Option<i32>,
    pub server_ids_json: Option<Option<Vec<String>>>,
    pub enabled: Option<bool>,
}

fn default_interval() -> i32 { 300 }
fn default_retry_count() -> i32 { 1 }
fn default_true() -> bool { true }

const VALID_TYPES: &[&str] = &["ssl", "dns", "http_keyword", "tcp", "whois"];

pub struct ServiceMonitorService;

impl ServiceMonitorService {
    pub async fn list(db: &DatabaseConnection, type_filter: Option<&str>) -> Result<Vec<service_monitor::Model>, AppError> {
        let mut query = service_monitor::Entity::find();
        if let Some(t) = type_filter {
            query = query.filter(service_monitor::Column::MonitorType.eq(t));
        }
        Ok(query.all(db).await?)
    }

    pub async fn get(db: &DatabaseConnection, id: &str) -> Result<service_monitor::Model, AppError> {
        service_monitor::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound("Service monitor not found".into()))
    }

    pub async fn create(db: &DatabaseConnection, input: CreateServiceMonitor) -> Result<service_monitor::Model, AppError> {
        if !VALID_TYPES.contains(&input.monitor_type.as_str()) {
            return Err(AppError::BadRequest(format!("Invalid monitor type: {}", input.monitor_type)));
        }
        let id = Uuid::new_v4().to_string();
        let model = service_monitor::ActiveModel {
            id: Set(id),
            name: Set(input.name),
            monitor_type: Set(input.monitor_type),
            target: Set(input.target),
            interval: Set(input.interval),
            config_json: Set(input.config_json.to_string()),
            notification_group_id: Set(input.notification_group_id),
            retry_count: Set(input.retry_count),
            server_ids_json: Set(input.server_ids_json.map(|v| serde_json::to_string(&v).unwrap_or_default())),
            enabled: Set(input.enabled),
            ..Default::default()
        };
        let result = service_monitor::Entity::insert(model).exec(db).await?;
        Self::get(db, &result.last_insert_id).await
    }

    pub async fn update(db: &DatabaseConnection, id: &str, input: UpdateServiceMonitor) -> Result<service_monitor::Model, AppError> {
        let existing = Self::get(db, id).await?;
        let mut model: service_monitor::ActiveModel = existing.into();
        if let Some(name) = input.name { model.name = Set(name); }
        if let Some(target) = input.target { model.target = Set(target); }
        if let Some(interval) = input.interval { model.interval = Set(interval); }
        if let Some(config) = input.config_json { model.config_json = Set(config.to_string()); }
        if let Some(ngid) = input.notification_group_id { model.notification_group_id = Set(ngid); }
        if let Some(rc) = input.retry_count { model.retry_count = Set(rc); }
        if let Some(sids) = input.server_ids_json {
            model.server_ids_json = Set(sids.map(|v| serde_json::to_string(&v).unwrap_or_default()));
        }
        if let Some(enabled) = input.enabled { model.enabled = Set(enabled); }
        model.updated_at = Set(chrono::Utc::now());
        model.update(db).await?;
        Self::get(db, id).await
    }

    pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let result = service_monitor::Entity::delete_by_id(id).exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound("Service monitor not found".into()));
        }
        // Cascade delete records
        service_monitor_record::Entity::delete_many()
            .filter(service_monitor_record::Column::MonitorId.eq(id))
            .exec(db).await?;
        Ok(())
    }

    pub async fn get_records(
        db: &DatabaseConnection,
        monitor_id: &str,
        from: Option<chrono::DateTime<chrono::Utc>>,
        to: Option<chrono::DateTime<chrono::Utc>>,
        limit: Option<u64>,
    ) -> Result<Vec<service_monitor_record::Model>, AppError> {
        let mut query = service_monitor_record::Entity::find()
            .filter(service_monitor_record::Column::MonitorId.eq(monitor_id))
            .order_by_desc(service_monitor_record::Column::Time);
        if let Some(from) = from {
            query = query.filter(service_monitor_record::Column::Time.gte(from));
        }
        if let Some(to) = to {
            query = query.filter(service_monitor_record::Column::Time.lte(to));
        }
        if let Some(limit) = limit {
            query = query.limit(limit);
        }
        Ok(query.all(db).await?)
    }

    pub async fn get_latest_record(db: &DatabaseConnection, monitor_id: &str) -> Result<Option<service_monitor_record::Model>, AppError> {
        Ok(service_monitor_record::Entity::find()
            .filter(service_monitor_record::Column::MonitorId.eq(monitor_id))
            .order_by_desc(service_monitor_record::Column::Time)
            .one(db).await?)
    }

    pub async fn list_enabled(db: &DatabaseConnection) -> Result<Vec<service_monitor::Model>, AppError> {
        Ok(service_monitor::Entity::find()
            .filter(service_monitor::Column::Enabled.eq(true))
            .all(db).await?)
    }

    pub async fn update_check_state(
        db: &DatabaseConnection,
        id: &str,
        success: bool,
        consecutive_failures: i32,
    ) -> Result<(), AppError> {
        service_monitor::Entity::update_many()
            .col_expr(service_monitor::Column::LastStatus, Expr::value(success))
            .col_expr(service_monitor::Column::ConsecutiveFailures, Expr::value(consecutive_failures))
            .col_expr(service_monitor::Column::LastCheckedAt, Expr::value(chrono::Utc::now()))
            .filter(service_monitor::Column::Id.eq(id))
            .exec(db).await?;
        Ok(())
    }

    pub async fn insert_record(
        db: &DatabaseConnection,
        monitor_id: &str,
        success: bool,
        latency: Option<f64>,
        detail: serde_json::Value,
        error: Option<String>,
    ) -> Result<(), AppError> {
        let record = service_monitor_record::ActiveModel {
            monitor_id: Set(monitor_id.to_string()),
            success: Set(success),
            latency: Set(latency),
            detail_json: Set(detail.to_string()),
            error: Set(error),
            time: Set(chrono::Utc::now()),
            ..Default::default()
        };
        service_monitor_record::Entity::insert(record).exec(db).await?;
        Ok(())
    }

    pub async fn cleanup_records(db: &DatabaseConnection, days: u32) -> Result<u64, AppError> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
        let result = service_monitor_record::Entity::delete_many()
            .filter(service_monitor_record::Column::Time.lt(cutoff))
            .exec(db).await?;
        Ok(result.rows_affected)
    }
}
```

- [ ] **Step 3: Register in service/mod.rs**

Add `pub mod service_monitor;` and `pub mod checker;` to `crates/server/src/service/mod.rs`.

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p serverbee-server`

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/
git commit -m "feat(server): add ServiceMonitorService with CRUD and record management"
```

---

### Task 5: Checker Implementations

**Files:**
- Create: `crates/server/src/service/checker/mod.rs`
- Create: `crates/server/src/service/checker/ssl.rs`
- Create: `crates/server/src/service/checker/dns.rs`
- Create: `crates/server/src/service/checker/http_keyword.rs`
- Create: `crates/server/src/service/checker/tcp.rs`
- Create: `crates/server/src/service/checker/whois.rs`

- [ ] **Step 1: Create checker trait and dispatcher (mod.rs)**

```rust
// crates/server/src/service/checker/mod.rs
pub mod ssl;
pub mod dns;
pub mod http_keyword;
pub mod tcp;
pub mod whois;

use serde_json::Value;

pub struct CheckResult {
    pub success: bool,
    pub latency: Option<f64>,
    pub detail: Value,
    pub error: Option<String>,
}

pub async fn run_check(monitor_type: &str, target: &str, config: &Value) -> CheckResult {
    match monitor_type {
        "ssl" => ssl::check(target, config).await,
        "dns" => dns::check(target, config).await,
        "http_keyword" => http_keyword::check(target, config).await,
        "tcp" => tcp::check(target, config).await,
        "whois" => whois::check(target, config).await,
        _ => CheckResult {
            success: false,
            latency: None,
            detail: Value::Null,
            error: Some(format!("Unknown monitor type: {monitor_type}")),
        },
    }
}
```

- [ ] **Step 2: Implement TCP checker (simplest — good for TDD)**

```rust
// crates/server/src/service/checker/tcp.rs
use super::CheckResult;
use serde_json::{Value, json};
use std::time::Instant;
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

pub async fn check(target: &str, config: &Value) -> CheckResult {
    let timeout_secs = config.get("timeout").and_then(|v| v.as_u64()).unwrap_or(10);
    let start = Instant::now();

    match timeout(Duration::from_secs(timeout_secs), TcpStream::connect(target)).await {
        Ok(Ok(_)) => CheckResult {
            success: true,
            latency: Some(start.elapsed().as_secs_f64() * 1000.0),
            detail: json!({ "connected": true }),
            error: None,
        },
        Ok(Err(e)) => CheckResult {
            success: false,
            latency: Some(start.elapsed().as_secs_f64() * 1000.0),
            detail: json!({ "connected": false }),
            error: Some(e.to_string()),
        },
        Err(_) => CheckResult {
            success: false,
            latency: None,
            detail: json!({ "connected": false }),
            error: Some("Connection timed out".into()),
        },
    }
}
```

- [ ] **Step 3: Implement SSL checker**

Use `rustls` + `x509-parser` to connect and extract certificate details. Parse `not_after` to compute `days_remaining`, compare against `warning_days`/`critical_days` from config.

- [ ] **Step 4: Implement DNS checker**

Use `hickory-resolver` to resolve the target with the configured `record_type`. Compare results against `expected_values` if provided.

- [ ] **Step 5: Implement HTTP keyword checker**

Use `reqwest` to send a request with configured method/headers/body. Check response status against `expected_status` list and search body for `keyword` (present or absent per `keyword_exists`).

- [ ] **Step 6: Implement WHOIS checker**

Use `whois-rust` crate to query domain WHOIS info, parse the `expiry_date` / `registrar` fields. Compute `days_remaining`, compare against `warning_days`/`critical_days`. Fall back to shell `whois` command if the crate fails.

- [ ] **Step 7: Write unit tests for each checker**

Test TCP checker against known ports (e.g., `127.0.0.1:PORT` with a local listener). Test other checkers with mocked or well-known targets. Add tests at the bottom of each checker file.

- [ ] **Step 8: Verify all tests pass**

Run: `cargo test -p serverbee-server -- checker`

- [ ] **Step 9: Commit**

```bash
git add crates/server/src/service/checker/
git commit -m "feat(server): implement 5 service monitor checkers (SSL/DNS/HTTP/TCP/WHOIS)"
```

---

### Task 6: Background Execution Engine

**Files:**
- Create: `crates/server/src/task/service_monitor_checker.rs`
- Modify: `crates/server/src/task/mod.rs`
- Modify: `crates/server/src/main.rs`

- [ ] **Step 1: Create the background task**

```rust
// crates/server/src/task/service_monitor_checker.rs
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{Duration, Instant};

use crate::service::checker;
use crate::service::notification::NotificationService;
use crate::service::service_monitor::ServiceMonitorService;
use crate::state::AppState;

const TICK_INTERVAL_SECS: u64 = 10;
const MAX_CONCURRENT_CHECKS: usize = 20;

pub async fn run(state: Arc<AppState>) {
    tracing::info!("Service monitor checker started");

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_CHECKS));
    let mut schedule: HashMap<String, Instant> = HashMap::new();

    // Bootstrap schedule from DB
    if let Ok(monitors) = ServiceMonitorService::list_enabled(&state.db).await {
        let now = Instant::now();
        for m in &monitors {
            let next = if let Some(last) = m.last_checked_at {
                let elapsed = chrono::Utc::now() - last;
                let remaining = m.interval as i64 - elapsed.num_seconds();
                if remaining > 0 {
                    now + Duration::from_secs(remaining as u64)
                } else {
                    now // overdue, check immediately
                }
            } else {
                now // never checked
            };
            schedule.insert(m.id.clone(), next);
        }
    }

    let mut interval = tokio::time::interval(Duration::from_secs(TICK_INTERVAL_SECS));

    loop {
        interval.tick().await;

        let monitors = match ServiceMonitorService::list_enabled(&state.db).await {
            Ok(m) => m,
            Err(e) => {
                tracing::error!("Failed to list monitors: {e}");
                continue;
            }
        };

        let now = Instant::now();

        // Reconcile schedule: add new, remove deleted
        let active_ids: std::collections::HashSet<String> = monitors.iter().map(|m| m.id.clone()).collect();
        schedule.retain(|id, _| active_ids.contains(id));

        for monitor in monitors {
            let next = schedule.entry(monitor.id.clone()).or_insert(now);
            if now < *next {
                continue; // not due yet
            }

            // Try to acquire semaphore permit
            let permit = match semaphore.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => continue, // defer to next tick
            };

            // Schedule next check
            *next = now + Duration::from_secs(monitor.interval as u64);

            let state = state.clone();
            tokio::spawn(async move {
                let _permit = permit;
                execute_check(&state, &monitor).await;
            });
        }
    }
}

async fn execute_check(state: &AppState, monitor: &crate::entity::service_monitor::Model) {
    let config: serde_json::Value = serde_json::from_str(&monitor.config_json).unwrap_or_default();
    let result = checker::run_check(&monitor.monitor_type, &monitor.target, &config).await;

    // Update state
    let new_failures = if result.success { 0 } else { monitor.consecutive_failures + 1 };

    // Insert record + update state
    if let Err(e) = ServiceMonitorService::insert_record(
        &state.db, &monitor.id, result.success, result.latency, result.detail, result.error.clone(),
    ).await {
        tracing::error!("Failed to insert monitor record: {e}");
    }
    if let Err(e) = ServiceMonitorService::update_check_state(
        &state.db, &monitor.id, result.success, new_failures,
    ).await {
        tracing::error!("Failed to update monitor state: {e}");
    }

    // Notification logic
    if !result.success && new_failures >= monitor.retry_count {
        if let Some(ref ngid) = monitor.notification_group_id {
            let msg = format!(
                "[{}] {} check failed for {}: {}",
                monitor.name, monitor.monitor_type, monitor.target,
                result.error.as_deref().unwrap_or("unknown error")
            );
            if let Err(e) = NotificationService::send_group(&state.db, ngid, &monitor.name, &msg).await {
                tracing::error!("Failed to send monitor notification: {e}");
            }
        }
    } else if result.success && monitor.consecutive_failures > 0 {
        // Recovery notification
        if let Some(ref ngid) = monitor.notification_group_id {
            let msg = format!(
                "[{}] {} check recovered for {} (was failing for {} consecutive checks)",
                monitor.name, monitor.monitor_type, monitor.target, monitor.consecutive_failures
            );
            if let Err(e) = NotificationService::send_group(&state.db, ngid, &monitor.name, &msg).await {
                tracing::error!("Failed to send recovery notification: {e}");
            }
        }
    }
}
```

- [ ] **Step 2: Register in task/mod.rs**

Add `pub mod service_monitor_checker;` to `crates/server/src/task/mod.rs`.

- [ ] **Step 3: Spawn in main.rs**

Find the task spawning section in `main.rs` and add:
```rust
tokio::spawn(task::service_monitor_checker::run(state.clone()));
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p serverbee-server`

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/task/ crates/server/src/main.rs
git commit -m "feat(server): add service monitor background execution engine"
```

---

### Task 7: Config + Cleanup

**Files:**
- Modify: `crates/server/src/config.rs`
- Modify: `crates/server/src/task/cleanup.rs`

- [ ] **Step 1: Add retention config**

In `crates/server/src/config.rs`, add to `RetentionConfig`:
```rust
#[serde(default = "default_30")]
pub service_monitor_days: u32,
```
Add `fn default_30() -> u32 { 30 }` (or reuse existing if one exists).
Update `Default` impl to include `service_monitor_days: 30`.

- [ ] **Step 2: Add cleanup logic**

In `crates/server/src/task/cleanup.rs`, add a call to `ServiceMonitorService::cleanup_records()` using `state.config.retention.service_monitor_days`.

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p serverbee-server`

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/config.rs crates/server/src/task/cleanup.rs
git commit -m "feat(server): add service monitor retention config and cleanup"
```

---

### Task 8: REST API Router

**Files:**
- Create: `crates/server/src/router/api/service_monitor.rs`
- Modify: `crates/server/src/router/api/mod.rs`
- Modify: `crates/server/src/openapi.rs`

- [ ] **Step 1: Create the router with handlers**

Follow the `ping.rs` pattern: `read_router()` for GET, `write_router()` for POST/PUT/DELETE. Each handler annotated with `#[utoipa::path(...)]`. Tag: `"service-monitors"`.

Handlers:
- `list_monitors` — GET `/api/service-monitors` with optional `?type=` query
- `get_monitor` — GET `/api/service-monitors/:id` (include latest record in response)
- `create_monitor` — POST `/api/service-monitors`
- `update_monitor` — PUT `/api/service-monitors/:id`
- `delete_monitor` — DELETE `/api/service-monitors/:id`
- `get_records` — GET `/api/service-monitors/:id/records` with `?from=&to=&limit=` query
- `trigger_check` — POST `/api/service-monitors/:id/check` (run check immediately, return result)

- [ ] **Step 2: Register in router/api/mod.rs**

Add `pub mod service_monitor;` and merge `read_router()` / `write_router()` into the appropriate auth blocks.

- [ ] **Step 3: Register in openapi.rs**

Add all handler paths to `paths(...)`, add `CreateServiceMonitor`, `UpdateServiceMonitor`, `service_monitor::Model`, `service_monitor_record::Model` to `schemas(...)`, add tag `(name = "service-monitors", description = "Server-side service monitoring (SSL/DNS/HTTP/TCP/WHOIS)")`.

- [ ] **Step 4: Write integration test**

Create test in `crates/server/tests/integration/` that creates a TCP monitor, triggers a check, and verifies the record is created.

- [ ] **Step 5: Run tests**

Run: `cargo test -p serverbee-server -- service_monitor`

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/router/ crates/server/src/openapi.rs
git commit -m "feat(server): add service monitor REST API with OpenAPI annotations"
```

---

### Task 9: Frontend — Management Page

**Files:**
- Create: `apps/web/src/routes/_authed/settings/service-monitors.tsx`
- Modify: `apps/web/src/components/layout/sidebar.tsx`

- [ ] **Step 1: Create the management page**

Page structure:
- List of monitors in a table (name, type, target, interval, status indicator, last checked)
- "Add Monitor" button opens a dialog
- Dialog: name, type (select), target, interval, config fields (dynamic based on type), notification group (select), retry count, enabled toggle
- Row actions: edit, delete, trigger check

Follow patterns from `settings/ping-tasks.tsx` for TanStack Query hooks (`useQuery`, `useMutation`) and dialog pattern.

- [ ] **Step 2: Add sidebar entry**

In `sidebar.tsx`, add "Service Monitor" link under the appropriate section, with an icon (e.g., `Activity` from lucide-react).

- [ ] **Step 3: Verify frontend builds**

Run: `cd apps/web && bun run typecheck && bun run build`

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/
git commit -m "feat(web): add service monitor management page and sidebar entry"
```

---

### Task 10: Frontend — Detail Page

**Files:**
- Create: `apps/web/src/routes/_authed/service-monitors/$id.tsx`

- [ ] **Step 1: Create the detail page**

Page structure:
- Header: monitor name, type badge, target, status indicator
- Stats row: uptime % (calculated from records), average latency, last check time
- Response time chart (Recharts line chart, similar to ping results)
- Type-specific detail card (SSL: certificate info, DNS: resolved values, etc.)
- History table (paginated records list)

- [ ] **Step 2: Verify frontend builds**

Run: `cd apps/web && bun run typecheck && bun run build`

- [ ] **Step 3: Run frontend lint**

Run: `cd apps/web && bun x ultracite check`

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/routes/_authed/service-monitors/
git commit -m "feat(web): add service monitor detail page with charts and history"
```

---

### Task 11: Final Verification

- [ ] **Step 1: Run all Rust tests**

Run: `cargo test --workspace`
Expected: all tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: 0 warnings

- [ ] **Step 3: Run frontend checks**

Run: `cd apps/web && bun run typecheck && bun x ultracite check`

- [ ] **Step 4: Update TESTING.md**

Add service monitor test counts and file locations.

- [ ] **Step 5: Update PROGRESS.md**

Add P9 section with all tasks marked done.

- [ ] **Step 6: Commit**

```bash
git add .
git commit -m "docs: update TESTING.md and PROGRESS.md for P9 service monitor"
```
