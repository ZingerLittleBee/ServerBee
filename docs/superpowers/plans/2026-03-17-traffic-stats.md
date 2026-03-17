# Monthly Traffic Statistics Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add monthly traffic statistics with billing cycle support, quota alerts, prediction, and frontend visualization.

**Architecture:** Server-side delta calculation in `record_writer` converts Agent cumulative counters into hourly/daily traffic tables. A `TrafficService` provides cycle computation, querying, and prediction. The existing alert system is refactored to use the new traffic tables. Frontend adds a progress bar in BillingInfoBar and a collapsible traffic detail card.

**Tech Stack:** Rust (sea-orm, chrono, chrono-tz), React (TanStack Query, Recharts), SQLite

**Spec:** `docs/superpowers/specs/2026-03-17-traffic-stats-scheduled-tasks-design.md` sections 1.1-1.11, 3, 4

---

## Task 1: Add chrono-tz dependency

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Modify: `crates/server/Cargo.toml`

- [ ] **Step 1: Add chrono-tz to workspace dependencies**

In root `Cargo.toml`, add to `[workspace.dependencies]`:
```toml
chrono-tz = "0.10"
```

In `crates/server/Cargo.toml`, add:
```toml
chrono-tz.workspace = true
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p serverbee-server`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml crates/server/Cargo.toml Cargo.lock
git commit -m "chore: add chrono-tz dependency for timezone-aware billing"
```

---

## Task 2: Database migration

Creates all new tables and columns for both traffic stats AND scheduled tasks (shared migration).

**Files:**
- Create: `crates/server/src/migration/m20260317_000005_traffic_and_scheduled_tasks.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Create the migration file**

Create `crates/server/src/migration/m20260317_000005_traffic_and_scheduled_tasks.rs` with:

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 1. traffic_hourly
        manager
            .create_table(
                Table::create()
                    .table(TrafficHourly::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(TrafficHourly::Id).big_integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(TrafficHourly::ServerId).string().not_null())
                    .col(ColumnDef::new(TrafficHourly::Hour).timestamp_with_time_zone().not_null())
                    .col(ColumnDef::new(TrafficHourly::BytesIn).big_integer().not_null().default(0))
                    .col(ColumnDef::new(TrafficHourly::BytesOut).big_integer().not_null().default(0))
                    .foreign_key(
                        ForeignKey::create()
                            .from(TrafficHourly::Table, TrafficHourly::ServerId)
                            .to(Servers::Table, Servers::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_traffic_hourly_unique")
                    .table(TrafficHourly::Table)
                    .col(TrafficHourly::ServerId)
                    .col(TrafficHourly::Hour)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // 2. traffic_daily
        manager
            .create_table(
                Table::create()
                    .table(TrafficDaily::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(TrafficDaily::Id).big_integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(TrafficDaily::ServerId).string().not_null())
                    .col(ColumnDef::new(TrafficDaily::Date).date().not_null())
                    .col(ColumnDef::new(TrafficDaily::BytesIn).big_integer().not_null().default(0))
                    .col(ColumnDef::new(TrafficDaily::BytesOut).big_integer().not_null().default(0))
                    .foreign_key(
                        ForeignKey::create()
                            .from(TrafficDaily::Table, TrafficDaily::ServerId)
                            .to(Servers::Table, Servers::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_traffic_daily_unique")
                    .table(TrafficDaily::Table)
                    .col(TrafficDaily::ServerId)
                    .col(TrafficDaily::Date)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // 3. traffic_state
        manager
            .create_table(
                Table::create()
                    .table(TrafficState::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(TrafficState::ServerId).string().not_null().primary_key())
                    .col(ColumnDef::new(TrafficState::LastIn).big_integer().not_null().default(0))
                    .col(ColumnDef::new(TrafficState::LastOut).big_integer().not_null().default(0))
                    .col(ColumnDef::new(TrafficState::UpdatedAt).timestamp_with_time_zone().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(TrafficState::Table, TrafficState::ServerId)
                            .to(Servers::Table, Servers::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // 4. servers: add billing_start_day
        manager
            .alter_table(
                Table::alter()
                    .table(Servers::Table)
                    .add_column(ColumnDef::new(Servers::BillingStartDay).integer().null())
                    .to_owned(),
            )
            .await?;

        // CHECK constraint for billing_start_day (SQLite raw SQL)
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TRIGGER IF NOT EXISTS check_billing_start_day \
                 BEFORE INSERT ON servers \
                 FOR EACH ROW \
                 WHEN NEW.billing_start_day IS NOT NULL AND (NEW.billing_start_day < 1 OR NEW.billing_start_day > 28) \
                 BEGIN SELECT RAISE(ABORT, 'billing_start_day must be between 1 and 28'); END;"
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TRIGGER IF NOT EXISTS check_billing_start_day_update \
                 BEFORE UPDATE ON servers \
                 FOR EACH ROW \
                 WHEN NEW.billing_start_day IS NOT NULL AND (NEW.billing_start_day < 1 OR NEW.billing_start_day > 28) \
                 BEGIN SELECT RAISE(ABORT, 'billing_start_day must be between 1 and 28'); END;"
            )
            .await?;

        // 5. tasks: add scheduled task columns
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::TaskType).string().not_null().default("oneshot"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::Name).string().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::CronExpression).string().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::Enabled).boolean().not_null().default(true))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::Timeout).integer().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::RetryCount).integer().not_null().default(0))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::RetryInterval).integer().not_null().default(60))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::LastRunAt).timestamp_with_time_zone().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::NextRunAt).timestamp_with_time_zone().null())
                    .to_owned(),
            )
            .await?;

        // 6. task_results: add run_id, attempt, started_at
        manager
            .alter_table(
                Table::alter()
                    .table(TaskResults::Table)
                    .add_column(ColumnDef::new(TaskResults::RunId).string().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(TaskResults::Table)
                    .add_column(ColumnDef::new(TaskResults::Attempt).integer().not_null().default(1))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(TaskResults::Table)
                    .add_column(ColumnDef::new(TaskResults::StartedAt).timestamp_with_time_zone().null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(TrafficHourly::Table).to_owned()).await?;
        manager.drop_table(Table::drop().table(TrafficDaily::Table).to_owned()).await?;
        manager.drop_table(Table::drop().table(TrafficState::Table).to_owned()).await?;
        // Note: SQLite does not support DROP COLUMN; reverting column additions requires table rebuild.
        // For development, dropping and recreating the DB is acceptable.
        Ok(())
    }
}

// --- Iden enums ---

#[derive(Iden)]
enum TrafficHourly {
    Table,
    Id,
    ServerId,
    Hour,
    BytesIn,
    BytesOut,
}

#[derive(Iden)]
enum TrafficDaily {
    Table,
    Id,
    ServerId,
    Date,
    BytesIn,
    BytesOut,
}

#[derive(Iden)]
enum TrafficState {
    Table,
    ServerId,
    LastIn,
    LastOut,
    UpdatedAt,
}

#[derive(Iden)]
enum Servers {
    Table,
    Id,
    BillingStartDay,
}

#[derive(Iden)]
enum Tasks {
    Table,
    TaskType,
    Name,
    CronExpression,
    Enabled,
    Timeout,
    RetryCount,
    RetryInterval,
    LastRunAt,
    NextRunAt,
}

#[derive(Iden)]
enum TaskResults {
    Table,
    RunId,
    Attempt,
    StartedAt,
}
```

- [ ] **Step 2: Register migration in mod.rs**

In `crates/server/src/migration/mod.rs`, add:
```rust
mod m20260317_000005_traffic_and_scheduled_tasks;
```
And in the `vec![]`, add:
```rust
Box::new(m20260317_000005_traffic_and_scheduled_tasks::Migration),
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p serverbee-server`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/migration/
git commit -m "feat: add migration for traffic tables and scheduled task columns"
```

---

## Task 3: Entity definitions

**Files:**
- Create: `crates/server/src/entity/traffic_hourly.rs`
- Create: `crates/server/src/entity/traffic_daily.rs`
- Create: `crates/server/src/entity/traffic_state.rs`
- Modify: `crates/server/src/entity/mod.rs`
- Modify: `crates/server/src/entity/server.rs`
- Modify: `crates/server/src/entity/task.rs`
- Modify: `crates/server/src/entity/task_result.rs`

- [ ] **Step 1: Create traffic_hourly entity**

Create `crates/server/src/entity/traffic_hourly.rs`:
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "traffic_hourly")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub server_id: String,
    pub hour: DateTimeUtc,
    pub bytes_in: i64,
    pub bytes_out: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 2: Create traffic_daily entity**

Create `crates/server/src/entity/traffic_daily.rs`:
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "traffic_daily")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub server_id: String,
    pub date: Date,
    pub bytes_in: i64,
    pub bytes_out: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 3: Create traffic_state entity**

Create `crates/server/src/entity/traffic_state.rs`:
```rust
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "traffic_state")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub server_id: String,
    pub last_in: i64,
    pub last_out: i64,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 4: Register entities in mod.rs**

In `crates/server/src/entity/mod.rs`, append:
```rust
pub mod traffic_daily;
pub mod traffic_hourly;
pub mod traffic_state;
```

- [ ] **Step 5: Add billing_start_day to server entity**

In `crates/server/src/entity/server.rs`, add field after `traffic_limit_type`:
```rust
pub billing_start_day: Option<i32>,
```

- [ ] **Step 6: Add scheduled task fields to task entity**

In `crates/server/src/entity/task.rs`, add fields:
```rust
pub task_type: String,
pub name: Option<String>,
pub cron_expression: Option<String>,
pub enabled: bool,
pub timeout: Option<i32>,
pub retry_count: i32,
pub retry_interval: i32,
pub last_run_at: Option<DateTimeUtc>,
pub next_run_at: Option<DateTimeUtc>,
```

- [ ] **Step 7: Add fields to task_result entity**

In `crates/server/src/entity/task_result.rs`, add fields:
```rust
pub run_id: Option<String>,
pub attempt: i32,
pub started_at: Option<DateTimeUtc>,
```

- [ ] **Step 8: Verify it compiles**

Run: `cargo check -p serverbee-server`
Expected: compiles with no errors

- [ ] **Step 9: Commit**

```bash
git add crates/server/src/entity/
git commit -m "feat: add traffic entities and extend server/task/task_result entities"
```

---

## Task 4: Configuration changes

**Files:**
- Modify: `crates/server/src/config.rs`

- [ ] **Step 1: Add retention fields and scheduler config**

In `RetentionConfig` struct, add after `network_probe_hourly_days`:
```rust
#[serde(default = "default_7")]
pub traffic_hourly_days: u32,
#[serde(default = "default_400")]
pub traffic_daily_days: u32,
#[serde(default = "default_7")]
pub task_results_days: u32,
```

Add the `default_400` function:
```rust
fn default_400() -> u32 { 400 }
```

**IMPORTANT:** Also update the manual `impl Default for RetentionConfig` (currently around line 128-139) to include the new fields:
```rust
traffic_hourly_days: 7,
traffic_daily_days: 400,
task_results_days: 7,
```
Failing to do this will cause a **compilation error** since `RetentionConfig` uses a manual `Default` impl, not `#[derive(Default)]`.

Add a new `SchedulerConfig` struct:
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct SchedulerConfig {
    #[serde(default = "default_utc")]
    pub timezone: String,
}

fn default_utc() -> String { "UTC".to_string() }

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self { timezone: default_utc() }
    }
}
```

Add to `AppConfig`:
```rust
#[serde(default)]
pub scheduler: SchedulerConfig,
```

Add a startup validation test for timezone parsing:
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_timezone_parsing() {
        use chrono_tz::Tz;
        assert!("UTC".parse::<Tz>().is_ok());
        assert!("Asia/Shanghai".parse::<Tz>().is_ok());
        assert!("Invalid/Zone".parse::<Tz>().is_err());
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p serverbee-server`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/config.rs
git commit -m "feat: add traffic retention and scheduler timezone config"
```

---

## Task 5: TrafficService — delta calculation and cycle range

**Files:**
- Create: `crates/server/src/service/traffic.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Write tests for delta calculation**

At the bottom of the new `service/traffic.rs` file, write test module:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_delta_normal() {
        let (d_in, d_out) = compute_delta(100, 200, 150, 250);
        assert_eq!(d_in, 50);
        assert_eq!(d_out, 50);
    }

    #[test]
    fn test_compute_delta_both_restart() {
        // Both counters reset (system reboot)
        let (d_in, d_out) = compute_delta(100_000, 50_000, 500, 300);
        assert_eq!(d_in, 500);
        assert_eq!(d_out, 300);
    }

    #[test]
    fn test_compute_delta_single_direction_restart_in() {
        // Only inbound resets, outbound continues
        let (d_in, d_out) = compute_delta(100_000, 50_000, 500, 51_000);
        assert_eq!(d_in, 500);    // Restarted: raw value
        assert_eq!(d_out, 1_000); // Normal: 51000 - 50000
    }

    #[test]
    fn test_compute_delta_single_direction_restart_out() {
        // Only outbound resets, inbound continues
        let (d_in, d_out) = compute_delta(100_000, 50_000, 101_000, 300);
        assert_eq!(d_in, 1_000);  // Normal: 101000 - 100000
        assert_eq!(d_out, 300);   // Restarted: raw value
    }

    #[test]
    fn test_compute_delta_zero() {
        let (d_in, d_out) = compute_delta(100, 200, 100, 200);
        assert_eq!(d_in, 0);
        assert_eq!(d_out, 0);
    }
}
```

- [ ] **Step 2: Implement delta calculation**

At the top of `service/traffic.rs`:
```rust
use chrono::{Datelike, Duration, NaiveDate, Utc};
use sea_orm::*;

use crate::entity::{traffic_daily, traffic_hourly, traffic_state};

/// Compute per-direction independent delta.
/// If a direction's current value < previous, treat as restart (use raw value).
pub fn compute_delta(prev_in: i64, prev_out: i64, curr_in: i64, curr_out: i64) -> (i64, i64) {
    let delta_in = if curr_in >= prev_in { curr_in - prev_in } else { curr_in };
    let delta_out = if curr_out >= prev_out { curr_out - prev_out } else { curr_out };
    (delta_in, delta_out)
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p serverbee-server service::traffic::tests -v`
Expected: all 5 tests PASS

- [ ] **Step 4: Write tests for cycle range computation**

Add to test module:
```rust
#[test]
fn test_cycle_range_natural_month() {
    let today = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
    let (start, end) = get_cycle_range("monthly", None, today);
    assert_eq!(start, NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
    assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
}

#[test]
fn test_cycle_range_billing_day_15() {
    let today = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
    let (start, end) = get_cycle_range("monthly", Some(15), today);
    assert_eq!(start, NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());
    assert_eq!(end, NaiveDate::from_ymd_opt(2026, 4, 14).unwrap());
}

#[test]
fn test_cycle_range_billing_day_before_anchor() {
    // Today is Mar 10, anchor is 15 -> previous cycle
    let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
    let (start, end) = get_cycle_range("monthly", Some(15), today);
    assert_eq!(start, NaiveDate::from_ymd_opt(2026, 2, 15).unwrap());
    assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 14).unwrap());
}

#[test]
fn test_cycle_range_quarterly() {
    let today = NaiveDate::from_ymd_opt(2026, 5, 10).unwrap();
    let (start, end) = get_cycle_range("quarterly", Some(1), today);
    assert_eq!(start, NaiveDate::from_ymd_opt(2026, 4, 1).unwrap());
    assert_eq!(end, NaiveDate::from_ymd_opt(2026, 6, 30).unwrap());
}

#[test]
fn test_cycle_range_yearly() {
    let today = NaiveDate::from_ymd_opt(2026, 8, 15).unwrap();
    let (start, end) = get_cycle_range("yearly", Some(1), today);
    assert_eq!(start, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    assert_eq!(end, NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
}

#[test]
fn test_cycle_range_unknown_falls_back_to_monthly() {
    let today = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();
    let (start, end) = get_cycle_range("unknown", None, today);
    assert_eq!(start, NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
    assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
}
```

- [ ] **Step 5: Implement cycle range**

```rust
/// Compute billing cycle date range.
/// Returns (start_date_inclusive, end_date_inclusive).
pub fn get_cycle_range(
    billing_cycle: &str,
    billing_start_day: Option<i32>,
    today: NaiveDate,
) -> (NaiveDate, NaiveDate) {
    let anchor = billing_start_day.unwrap_or(1).clamp(1, 28);

    match billing_cycle {
        "quarterly" => get_quarterly_range(anchor, today),
        "yearly" => get_yearly_range(anchor, today),
        _ => get_monthly_range(anchor, today), // "monthly" or unknown
    }
}

fn get_monthly_range(anchor: i32, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let (y, m) = (today.year(), today.month());

    let cycle_start = if today.day() as i32 >= anchor {
        NaiveDate::from_ymd_opt(y, m, anchor as u32).unwrap()
    } else {
        // Go to previous month
        let prev = today - Duration::days(today.day() as i64);
        NaiveDate::from_ymd_opt(prev.year(), prev.month(), anchor as u32).unwrap()
    };

    // End = day before next anchor
    let next_anchor = if anchor == 1 {
        // Natural month: end is last day of start's month
        let next_month = if cycle_start.month() == 12 {
            NaiveDate::from_ymd_opt(cycle_start.year() + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(cycle_start.year(), cycle_start.month() + 1, 1).unwrap()
        };
        next_month - Duration::days(1)
    } else {
        let next = add_months(cycle_start, 1);
        next - Duration::days(1)
    };

    (cycle_start, next_anchor)
}

fn get_quarterly_range(anchor: i32, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    // Find the most recent quarter start
    let (y, m) = (today.year(), today.month());
    let quarter_start_months = [1, 4, 7, 10];

    let mut cycle_start = None;
    for &qm in quarter_start_months.iter().rev() {
        let candidate = NaiveDate::from_ymd_opt(y, qm, anchor as u32);
        if let Some(c) = candidate {
            if c <= today {
                cycle_start = Some(c);
                break;
            }
        }
    }
    let cycle_start = cycle_start.unwrap_or_else(|| {
        NaiveDate::from_ymd_opt(y - 1, 10, anchor as u32).unwrap()
    });

    let end = add_months(cycle_start, 3) - Duration::days(1);
    (cycle_start, end)
}

fn get_yearly_range(anchor: i32, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = NaiveDate::from_ymd_opt(today.year(), 1, anchor as u32).unwrap();
    if start <= today {
        let end = add_months(start, 12) - Duration::days(1);
        (start, end)
    } else {
        let start = NaiveDate::from_ymd_opt(today.year() - 1, 1, anchor as u32).unwrap();
        let end = add_months(start, 12) - Duration::days(1);
        (start, end)
    }
}

fn add_months(date: NaiveDate, months: u32) -> NaiveDate {
    let total_months = date.year() * 12 + date.month() as i32 - 1 + months as i32;
    let y = total_months / 12;
    let m = (total_months % 12) + 1;
    let d = date.day().min(days_in_month(y, m as u32));
    NaiveDate::from_ymd_opt(y, m as u32, d).unwrap()
}

fn days_in_month(year: i32, month: u32) -> u32 {
    NaiveDate::from_ymd_opt(
        if month == 12 { year + 1 } else { year },
        if month == 12 { 1 } else { month + 1 },
        1,
    )
    .unwrap()
    .pred_opt()
    .unwrap()
    .day()
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p serverbee-server service::traffic::tests -v`
Expected: all 11 tests PASS

- [ ] **Step 7: Register module**

In `crates/server/src/service/mod.rs`, append:
```rust
pub mod traffic;
```

- [ ] **Step 8: Commit**

```bash
git add crates/server/src/service/traffic.rs crates/server/src/service/mod.rs
git commit -m "feat: add TrafficService with delta calculation and cycle range"
```

---

## Task 6: TrafficService — prediction algorithm

**Files:**
- Modify: `crates/server/src/service/traffic.rs`

- [ ] **Step 1: Write prediction tests**

```rust
#[test]
fn test_prediction_normal() {
    let p = compute_prediction(60_000_000_000, 7, 10, Some(100_000_000_000), "sum");
    assert!(p.is_some());
    let p = p.unwrap();
    // daily_avg = 60B / 7 = ~8.57B, remaining = 10 days
    // estimated = 60B + 8.57B * 10 = ~145.7B
    assert!(p.estimated_total > 60_000_000_000);
    assert!(p.will_exceed); // 145.7B > 100B limit
}

#[test]
fn test_prediction_too_early() {
    let p = compute_prediction(5_000_000_000, 2, 28, Some(100_000_000_000), "sum");
    assert!(p.is_none()); // days_elapsed < 3
}

#[test]
fn test_prediction_no_limit() {
    let p = compute_prediction(60_000_000_000, 7, 10, None, "sum");
    assert!(p.is_none());
}
```

- [ ] **Step 2: Implement prediction**

```rust
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct TrafficPrediction {
    pub estimated_total: i64,
    pub estimated_percent: f64,
    pub will_exceed: bool,
}

/// Returns None if days_elapsed < 3 or no traffic_limit set.
pub fn compute_prediction(
    recent_sum: i64,
    days_elapsed: i64,
    days_remaining: i64,
    traffic_limit: Option<i64>,
    traffic_limit_type: &str,
) -> Option<TrafficPrediction> {
    if days_elapsed < 3 {
        return None;
    }
    let traffic_limit = traffic_limit?;
    if traffic_limit <= 0 {
        return None;
    }

    let daily_avg = recent_sum as f64 / days_elapsed as f64;
    let estimated_total = recent_sum + (daily_avg * days_remaining as f64) as i64;
    let estimated_percent = estimated_total as f64 / traffic_limit as f64 * 100.0;
    let will_exceed = estimated_total > traffic_limit;

    Some(TrafficPrediction {
        estimated_total,
        estimated_percent,
        will_exceed,
    })
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p serverbee-server service::traffic::tests -v`
Expected: all 14 tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/traffic.rs
git commit -m "feat: add traffic prediction algorithm"
```

---

## Task 7: TrafficService — DB operations (upsert + load cache)

**Files:**
- Modify: `crates/server/src/service/traffic.rs`

- [ ] **Step 1: Write tests for DB operations**

Add to test module (uses in-memory SQLite):
```rust
#[tokio::test]
async fn test_upsert_traffic_hourly_accumulates() {
    let db = setup_test_db().await;
    let hour = Utc::now().date_naive().and_hms_opt(10, 0, 0).unwrap().and_utc();
    TrafficService::upsert_hourly(&db, "srv-1", hour, 100, 200).await.unwrap();
    TrafficService::upsert_hourly(&db, "srv-1", hour, 50, 30).await.unwrap();
    // Second upsert should accumulate: 100+50=150, 200+30=230
    let row = traffic_hourly::Entity::find()
        .filter(traffic_hourly::Column::ServerId.eq("srv-1"))
        .one(&db).await.unwrap().unwrap();
    assert_eq!(row.bytes_in, 150);
    assert_eq!(row.bytes_out, 230);
}

#[tokio::test]
async fn test_load_transfer_cache_from_traffic_state() {
    let db = setup_test_db().await;
    TrafficService::upsert_state(&db, "srv-1", 1000, 2000).await.unwrap();
    let cache = TrafficService::load_transfer_cache(&db).await.unwrap();
    assert_eq!(cache.get("srv-1"), Some(&(1000i64, 2000i64)));
}
```

- [ ] **Step 2: Implement DB operations**

In `service/traffic.rs`, add:
- `pub async fn upsert_hourly(db, server_id, hour, delta_in, delta_out)` — raw SQL UPSERT with ON CONFLICT accumulation
- `pub async fn upsert_state(db, server_id, last_in, last_out)` — upsert traffic_state
- `pub async fn load_transfer_cache(db) -> HashMap<String, (i64, i64)>` — load all traffic_state rows into cache

- [ ] **Step 3: Run tests**

Run: `cargo test -p serverbee-server service::traffic::tests -v`
Expected: all tests PASS

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/traffic.rs
git commit -m "feat: add traffic DB operations (upsert_hourly, upsert_state, load_cache)"
```

---

## Task 8: record_writer — delta calculation integration

**Files:**
- Modify: `crates/server/src/task/record_writer.rs`

- [ ] **Step 1: Add delta calculation to record_writer loop**

The record_writer currently iterates `all_latest_reports()` and calls `save_report`. Add traffic delta logic:
- Add a `HashMap<String, (i64, i64)>` for previous transfer cache
- On first iteration: call `TrafficService::load_transfer_cache(&state.db)` to initialize from `traffic_state` table
- Each iteration: call `compute_delta()`, then `TrafficService::upsert_hourly()` and `TrafficService::upsert_state()`

Refer to spec section 1.3 for the exact pseudocode.

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p serverbee-server`
Expected: compiles with no errors

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/task/record_writer.rs
git commit -m "feat: integrate traffic delta calculation into record_writer"
```

---

## Task 9: Aggregator and cleanup

**Files:**
- Modify: `crates/server/src/service/traffic.rs`
- Modify: `crates/server/src/task/aggregator.rs`
- Modify: `crates/server/src/task/cleanup.rs`

- [ ] **Step 1: Write test for timezone-aware daily aggregation**

```rust
#[tokio::test]
async fn test_aggregate_daily_timezone_bucketing() {
    let db = setup_test_db().await;
    // Insert hourly data spanning a timezone day boundary
    // For Asia/Shanghai (UTC+8): Mar 17 local = Mar 16 16:00 UTC to Mar 17 15:59 UTC
    let h1 = NaiveDate::from_ymd_opt(2026, 3, 16).unwrap()
        .and_hms_opt(20, 0, 0).unwrap().and_utc(); // Mar 17 04:00 CST
    let h2 = NaiveDate::from_ymd_opt(2026, 3, 17).unwrap()
        .and_hms_opt(2, 0, 0).unwrap().and_utc();  // Mar 17 10:00 CST
    TrafficService::upsert_hourly(&db, "srv-1", h1, 100, 200).await.unwrap();
    TrafficService::upsert_hourly(&db, "srv-1", h2, 300, 400).await.unwrap();

    TrafficService::aggregate_daily(&db, "Asia/Shanghai").await.unwrap();

    // Both hours fall on Mar 17 in Asia/Shanghai
    let daily = traffic_daily::Entity::find()
        .filter(traffic_daily::Column::ServerId.eq("srv-1"))
        .all(&db).await.unwrap();
    assert_eq!(daily.len(), 1);
    assert_eq!(daily[0].bytes_in, 400);  // 100 + 300
    assert_eq!(daily[0].bytes_out, 600); // 200 + 400
}
```

- [ ] **Step 2: Implement aggregate_daily and cleanup functions**

In `service/traffic.rs`, add:
- `pub async fn aggregate_daily(db, timezone: &str) -> Result<u64, AppError>` — parse timezone via `chrono_tz`, bucket hourly rows by local date for yesterday and today, INSERT OR REPLACE into traffic_daily
- `pub async fn cleanup_hourly(db, days: u32) -> Result<u64, AppError>`
- `pub async fn cleanup_daily(db, days: u32) -> Result<u64, AppError>`
- `pub async fn cleanup_task_results(db, days: u32) -> Result<u64, AppError>`

- [ ] **Step 3: Run tests**

Run: `cargo test -p serverbee-server service::traffic::tests -v`
Expected: all tests PASS

- [ ] **Step 4: Integrate into aggregator and cleanup tasks**

In `aggregator.rs`: add `TrafficService::aggregate_daily(&state.db, &state.config.scheduler.timezone)` call after existing aggregation calls.

In `cleanup.rs`: add cleanup calls for traffic_hourly, traffic_daily, and task_results using retention config values from `state.config.retention`.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p serverbee-server`
Expected: compiles with no errors

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/service/traffic.rs crates/server/src/task/aggregator.rs crates/server/src/task/cleanup.rs
git commit -m "feat: add timezone-aware traffic aggregation and cleanup"
```

---

## Task 10: Alert system fix

**Files:**
- Modify: `crates/server/src/service/alert.rs`

- [ ] **Step 1: Write test for refactored check_transfer_cycle**

Add test that verifies the new implementation queries `traffic_hourly` instead of `records` and supports the `"billing"` cycle_interval. Use an in-memory SQLite DB with test data.

- [ ] **Step 2: Refactor check_transfer_cycle**

Replace the `check_transfer_cycle` function body to:
- For `hour/day/week/month/year`: query `traffic_hourly` with relative time window from `now` (preserving existing semantics)
- For `"billing"`: use `get_cycle_range()` with server's billing config, query `traffic_daily` for completed days + `traffic_hourly` for today
- Return `SUM(bytes_in)`, `SUM(bytes_out)`, or both depending on rule_type

- [ ] **Step 3: Run existing alert tests + new test**

Run: `cargo test -p serverbee-server service::alert -v`
Expected: all tests PASS (existing + new)

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/alert.rs
git commit -m "fix: refactor transfer cycle alerts to use traffic tables"
```

---

## Task 11: Server entity updates + traffic API

**Files:**
- Modify: `crates/server/src/service/server.rs`
- Create: `crates/server/src/router/api/traffic.rs`
- Modify: `crates/server/src/router/api/mod.rs`
- Modify: `crates/server/src/openapi.rs`

- [ ] **Step 1: Add billing_start_day to UpdateServerInput**

In `service/server.rs`, add to `UpdateServerInput`:
```rust
pub billing_start_day: Option<Option<i32>>,
```
And handle it in `update_server()`.

- [ ] **Step 2: Create traffic API route**

Create `router/api/traffic.rs` with `GET /api/servers/{id}/traffic`:
- Accept optional `cycle_start` query param
- Call `TrafficService` functions for cycle range, totals (daily + hourly for today), prediction
- Return JSON response per spec section 1.6
- Add `#[utoipa::path]` annotation

- [ ] **Step 3: Register route in mod.rs**

In `router/api/mod.rs`, add `pub mod traffic;` and merge `traffic::read_router()` into the read layer.

- [ ] **Step 4: Register in openapi.rs**

Add the traffic handler path and response schemas.

- [ ] **Step 5: Verify it compiles**

Run: `cargo check -p serverbee-server`
Expected: compiles with no errors

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/service/server.rs crates/server/src/router/api/traffic.rs crates/server/src/router/api/mod.rs crates/server/src/openapi.rs
git commit -m "feat: add traffic query API and billing_start_day support"
```

---

## Task 12: Integration tests

**Files:**
- Modify: `crates/server/tests/integration.rs`

- [ ] **Step 1: Add regression test for existing one-shot task creation**

Verify that creating a one-shot task still works after migration adds new NOT NULL columns with defaults (`task_type DEFAULT 'oneshot'`, `enabled DEFAULT true`, etc.). This ensures the new schema is backward-compatible.

- [ ] **Step 2: Add traffic delta integration test**

Test: simulate Agent reports with a restart (cumulative drop), verify `traffic_hourly` has correct deltas. Also test single-direction restart.

- [ ] **Step 3: Add traffic API integration test**

Test: set up a server with `traffic_limit` and `billing_cycle`, insert traffic data, query API, verify response structure and totals.

- [ ] **Step 4: Run integration tests**

Run: `cargo test -p serverbee-server --test integration -v`
Expected: all tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/server/tests/integration.rs
git commit -m "test: add traffic stats integration tests and one-shot regression test"
```

---

## Task 13: Frontend — use-traffic hook

**Files:**
- Create: `apps/web/src/hooks/use-traffic.ts`

- [ ] **Step 1: Create the hook**

```typescript
import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'

export interface TrafficData {
  cycle_start: string
  cycle_end: string
  bytes_in: number
  bytes_out: number
  bytes_total: number
  traffic_limit: number | null
  traffic_limit_type: string | null
  usage_percent: number | null
  prediction: {
    estimated_total: number
    estimated_percent: number
    will_exceed: boolean
  } | null
  daily: Array<{ date: string; bytes_in: number; bytes_out: number }>
  hourly: Array<{ hour: string; bytes_in: number; bytes_out: number }>
}

export function useTraffic(serverId: string) {
  return useQuery({
    queryKey: ['servers', serverId, 'traffic'],
    queryFn: () => api.get<TrafficData>(`/api/servers/${serverId}/traffic`),
    staleTime: 60_000,
    enabled: !!serverId,
  })
}
```

- [ ] **Step 2: Commit**

```bash
git add apps/web/src/hooks/use-traffic.ts
git commit -m "feat: add useTraffic hook for traffic API"
```

---

## Task 14: Frontend — BillingInfoBar progress bar

**Files:**
- Create: `apps/web/src/components/server/traffic-progress.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`

- [ ] **Step 1: Create TrafficProgress component**

A progress bar component that:
- Shows `{used} / {limit} [{bar}] {percent}%`
- Colors: 0-70% green, 70-90% yellow, 90%+ red
- Optional dashed prediction marker
- Handles `traffic_limit_type` (sum/up/down)

- [ ] **Step 2: Integrate into BillingInfoBar**

In `$id.tsx`, import `useTraffic` and render `TrafficProgress` in the `BillingInfoBar` section when `traffic_limit` is set.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/server/traffic-progress.tsx apps/web/src/routes/_authed/servers/\$id.tsx
git commit -m "feat: add traffic progress bar to BillingInfoBar"
```

---

## Task 15: Frontend — Traffic detail card

**Files:**
- Create: `apps/web/src/components/server/traffic-card.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`

- [ ] **Step 1: Create TrafficCard component**

Collapsible card with:
- Collapsed: summary line
- Expanded: daily BarChart (Recharts, stacked in/out), hourly LineChart (day selector), prediction dashed line, cycle info

- [ ] **Step 2: Integrate into server detail page**

Add `TrafficCard` below existing metrics charts.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/server/traffic-card.tsx apps/web/src/routes/_authed/servers/\$id.tsx
git commit -m "feat: add collapsible traffic detail card with charts"
```

---

## Task 16: Frontend — Edit dialog + i18n

**Files:**
- Modify: `apps/web/src/components/server/server-edit-dialog.tsx`

- [ ] **Step 1: Add billing_start_day field**

In the billing section of the edit dialog, add a number input (1-28) for `billing_start_day` with placeholder "Leave empty for natural month (1st)".

- [ ] **Step 2: Add i18n keys**

Add translation keys for traffic-related UI text in the relevant namespace files.

- [ ] **Step 3: Commit**

```bash
git add apps/web/src/components/server/server-edit-dialog.tsx
git commit -m "feat: add billing_start_day to server edit dialog"
```

---

## Task 17: Frontend tests

**Files:**
- Create: `apps/web/src/hooks/use-traffic.test.ts`

Tests should live alongside source files following existing convention (e.g., `use-servers-ws.test.ts` is in `apps/web/src/hooks/`).

- [ ] **Step 1: Write tests for useTraffic hook**

Test query key, staleTime, enabled behavior.

- [ ] **Step 2: Write tests for TrafficProgress**

Test color thresholds (green/yellow/red), limit type handling, prediction marker.

- [ ] **Step 3: Run frontend tests**

Run: `bun run test`
Expected: all tests PASS

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/hooks/use-traffic.test.ts
git commit -m "test: add frontend traffic tests"
```

---

## Task 18: Regenerate API types + lint

**Files:**
- Modify: `apps/web/src/lib/api-types.ts` (auto-generated from OpenAPI spec)
- Modify: `apps/web/src/lib/api-schema.ts` (manually maintained re-exports from api-types.ts — update to export new traffic types)

- [ ] **Step 1: Regenerate OpenAPI types**

Run: `cargo run --example dump_openapi > /tmp/openapi.json && cd apps/web && npx openapi-typescript /tmp/openapi.json -o src/lib/api-types.ts`

- [ ] **Step 2: Update api-schema.ts**

Add re-exports for new traffic-related types (e.g., `TrafficResponse`, `TrafficPrediction`) from `api-types.ts`.

- [ ] **Step 3: Run lint**

Run: `bun x ultracite fix && bun run typecheck`
Expected: no errors

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/lib/api-types.ts apps/web/src/lib/api-schema.ts
git commit -m "chore: regenerate OpenAPI types for traffic API"
```

---

## Task 19: Documentation

**Files:**
- Modify: `ENV.md`
- Modify: `apps/docs/content/docs/{en,cn}/configuration.mdx`
- Modify: `TESTING.md`

- [ ] **Step 1: Update ENV.md**

Add new environment variables:
- `SERVERBEE_RETENTION__TRAFFIC_HOURLY_DAYS`
- `SERVERBEE_RETENTION__TRAFFIC_DAILY_DAYS`
- `SERVERBEE_RETENTION__TASK_RESULTS_DAYS`
- `SERVERBEE_SCHEDULER__TIMEZONE`

- [ ] **Step 2: Update configuration docs**

Add the new config fields to both EN and CN docs.

- [ ] **Step 3: Update TESTING.md**

Update test counts, add new test file locations, update verification checklist.

- [ ] **Step 4: Commit**

```bash
git add ENV.md apps/docs/content/docs/ TESTING.md
git commit -m "docs: document traffic stats config and update test counts"
```

---

## Task 20: Final verification

- [ ] **Step 1: Run all Rust tests**

Run: `cargo test --workspace`
Expected: all tests PASS

- [ ] **Step 2: Run all frontend tests**

Run: `bun run test`
Expected: all tests PASS

- [ ] **Step 3: Run lints**

Run: `cargo clippy --workspace -- -D warnings && bun x ultracite check && bun run typecheck`
Expected: 0 warnings, 0 errors

- [ ] **Step 4: Build check**

Run: `cargo build --workspace && cd apps/web && bun run build`
Expected: builds successfully
