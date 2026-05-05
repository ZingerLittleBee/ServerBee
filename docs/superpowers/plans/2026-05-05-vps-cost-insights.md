# VPS Cost Insights Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add VPS cost insights so configured server pricing drives cost burn, value scoring, list/card signals, and detail-page breakdowns.

**Architecture:** Add a dedicated Rust `CostService` that owns all cost normalization, resource unit costs, utilization/reliability scoring, and overview projection. Expose read-only Axum endpoints for overview and per-server details, then consume those endpoints from focused React hooks/components without duplicating formulas in UI code.

**Tech Stack:** Rust, Axum, SeaORM, Utoipa/OpenAPI, SQLite, React 19, TanStack Query/Table, Vitest, Ultracite/Biome.

---

## Reference

- Spec: `docs/superpowers/specs/2026-05-05-vps-cost-insights-design.md`
- Existing cycle math: `crates/server/src/service/traffic.rs`
- Existing server update DTO: `crates/server/src/service/server.rs`
- Existing route registration: `crates/server/src/router/api/mod.rs`
- Existing OpenAPI registration: `crates/server/src/openapi.rs`
- Existing detail billing UI: `apps/web/src/routes/_authed/servers/$id.tsx`
- Existing server list cells: `apps/web/src/routes/_authed/servers/index.cells.tsx`
- Existing server cards: `apps/web/src/components/server/server-card.tsx`
- Existing locales: `apps/web/src/locales/{en,zh}/servers.json`

## File Map

Backend:

- Create `crates/server/src/service/cost.rs`: DTOs, validation enums, cost normalization, resource values, value scoring, batched overview implementation, unit tests.
- Modify `crates/server/src/service/mod.rs`: export `cost`.
- Modify `crates/server/src/service/server.rs`: validate `price`, `billing_cycle`, `traffic_limit_type`, and `billing_start_day` during update.
- Create `crates/server/src/router/api/cost.rs`: `GET /cost/overview` and `GET /servers/{id}/cost-insights`.
- Modify `crates/server/src/router/api/mod.rs`: register cost read router.
- Modify `crates/server/src/openapi.rs`: register paths and schemas.
- Create `crates/server/tests/cost_integration.rs`: endpoint and RBAC integration tests.

Frontend:

- Create `apps/web/src/hooks/use-cost.ts`: cost overview/detail React Query hooks and temporary type exports if generated OpenAPI types are not available yet.
- Create `apps/web/src/lib/cost.ts`: amount formatting, grade class mapping, reason label mapping.
- Create `apps/web/src/lib/cost.test.ts`: pure formatting and mapping tests.
- Create `apps/web/src/components/server/cost-cell.tsx`: table cell for cost overview.
- Create `apps/web/src/components/server/cost-footnote.tsx`: compact grid-card cost signal.
- Create `apps/web/src/components/server/cost-insight-bar.tsx`: detail-page summary/breakdown.
- Modify `apps/web/src/routes/_authed/servers/index.tsx`: fetch cost overview, add Cost column.
- Modify `apps/web/src/components/server/server-card.tsx`: fetch cost overview and render cost footnote.
- Modify `apps/web/src/routes/_authed/servers/$id.tsx`: replace/augment `BillingInfoBar` with `CostInsightBar`.
- Modify `apps/web/src/locales/en/servers.json` and `apps/web/src/locales/zh/servers.json`: cost labels, grades, reasons.
- Modify tests under `apps/web/src/components/server/` and `apps/web/src/routes/_authed/servers/`.

Generated:

- Modify `apps/web/openapi.json` and `apps/web/src/lib/api-types.ts` via `bun run generate:api-types` from `apps/web` after backend OpenAPI compiles.

---

## Chunk 1: Backend Cost Domain

### Task 1: Add `CostService` DTO skeleton and configuration validation

**Files:**

- Create: `crates/server/src/service/cost.rs`
- Modify: `crates/server/src/service/mod.rs`
- Test: `crates/server/src/service/cost.rs`

- [ ] **Step 1: Write failing unit tests for cost config validation**

Add a `#[cfg(test)] mod tests` in `crates/server/src/service/cost.rs` with tests covering:

```rust
#[test]
fn cost_config_requires_price_and_billing_cycle() {
    assert_eq!(
        CostService::normalize_config(None, Some("monthly"), None).invalid_reason,
        Some(CostInvalidReason::MissingPrice)
    );
    assert_eq!(
        CostService::normalize_config(Some(5.0), None, None).invalid_reason,
        Some(CostInvalidReason::MissingBillingCycle)
    );
}

#[test]
fn cost_config_rejects_unknown_billing_cycle_before_cycle_math() {
    assert_eq!(
        CostService::normalize_config(Some(5.0), Some("weekly"), None).invalid_reason,
        Some(CostInvalidReason::InvalidBillingCycle)
    );
}

#[test]
fn cost_config_defaults_missing_currency_to_usd() {
    let normalized = CostService::normalize_config(Some(5.0), Some("monthly"), None);
    assert_eq!(normalized.currency.as_deref(), Some("USD"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p serverbee-server service::cost::tests::cost_config -- --nocapture`

Expected: compile failure because `service::cost` and `CostService` do not exist.

- [ ] **Step 3: Add skeleton types and validation**

Create `crates/server/src/service/cost.rs` with:

```rust
use chrono::{DateTime, NaiveDate, Utc};
use sea_orm::{ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CostInvalidReason {
    MissingPrice,
    MissingBillingCycle,
    InvalidBillingCycle,
    InvalidPrice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueGrade {
    Excellent,
    Good,
    Okay,
    Poor,
    Waste,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueReason {
    IdleBurn,
    SleepingMoney,
    GoodMemoryValue,
    GoodDiskValue,
    ExpensiveCpu,
    HealthyUptime,
    LowUptime,
    ExpiredBilling,
    NoPriceCycle,
    InsufficientData,
    FreeOrZeroPrice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueConfidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq)]
struct NormalizedCostConfig {
    configured: bool,
    invalid_reason: Option<CostInvalidReason>,
    price: Option<f64>,
    currency: Option<String>,
    billing_cycle: Option<String>,
}

pub struct CostService;

impl CostService {
    pub(crate) fn normalize_config(
        price: Option<f64>,
        billing_cycle: Option<&str>,
        currency: Option<&str>,
    ) -> NormalizedCostConfig {
        // Implement exactly per spec:
        // missing price -> MissingPrice
        // price < 0 -> InvalidPrice
        // missing/empty billing cycle -> MissingBillingCycle
        // cycle outside monthly/quarterly/yearly -> InvalidBillingCycle
        // currency None/empty -> USD
        todo!("implement in task")
    }
}
```

Add `pub mod cost;` to `crates/server/src/service/mod.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p serverbee-server service::cost::tests::cost_config -- --nocapture`

Expected: all new config validation tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/cost.rs crates/server/src/service/mod.rs
git commit -m "feat(server): add cost insight config validation"
```

### Task 2: Implement cycle cost math and resource unit costs

**Files:**

- Modify: `crates/server/src/service/cost.rs`
- Test: `crates/server/src/service/cost.rs`

- [ ] **Step 1: Write failing unit tests for cost math**

Add tests covering:

```rust
#[test]
fn monthly_cost_uses_real_cycle_days() {
    let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
    let burn = CostService::compute_burn(31.0, "monthly", None, today).unwrap();
    assert_eq!(burn.cycle_days, 31);
    assert_eq!(burn.days_elapsed, 5);
    assert!((burn.cost_per_day - 1.0).abs() < 0.0001);
    assert!((burn.cycle_cost_elapsed - 5.0).abs() < 0.0001);
}

#[test]
fn zero_price_has_no_burn_percent() {
    let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
    let burn = CostService::compute_burn(0.0, "monthly", None, today).unwrap();
    assert_eq!(burn.cost_per_second, 0.0);
    assert_eq!(burn.cycle_burn_percent, None);
}

#[test]
fn resource_values_use_month_equivalent_cost() {
    let values = CostService::compute_resource_value(
        5.0,
        Some(2),
        Some(8 * 1024_i64.pow(3)),
        Some(80 * 1024_i64.pow(3)),
        Some(1024_i64.pow(4)),
        Some("sum"),
    );
    assert_eq!(values.cost_per_cpu_core, Some(2.5));
    assert_eq!(values.cost_per_gb_memory, Some(0.625));
    assert_eq!(values.cost_per_gb_disk, Some(0.0625));
    assert_eq!(values.cost_per_tb_traffic_limit, Some(5.0));
}
```

Also test yearly leap-year cycle:

```rust
#[test]
fn yearly_leap_year_uses_366_days() {
    let today = chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
    let burn = CostService::compute_burn(366.0, "yearly", Some(1), today).unwrap();
    assert_eq!(burn.cycle_days, 366);
    assert!((burn.cost_per_day - 1.0).abs() < 0.0001);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p serverbee-server service::cost::tests -- --nocapture`

Expected: compile failure for missing `compute_burn` and `compute_resource_value`.

- [ ] **Step 3: Add burn/resource structs and implementations**

Implement in `crates/server/src/service/cost.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct CostBurn {
    pub cycle_start: String,
    pub cycle_end: String,
    pub cycle_days: i64,
    pub days_elapsed: i64,
    pub days_remaining: i64,
    pub cost_per_second: f64,
    pub cost_per_hour: f64,
    pub cost_per_day: f64,
    pub cost_per_month_equivalent: f64,
    pub cycle_cost_elapsed: f64,
    pub cycle_cost_remaining: f64,
    pub cycle_burn_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct ResourceValue {
    pub cost_per_cpu_core: Option<f64>,
    pub cost_per_gb_memory: Option<f64>,
    pub cost_per_gb_disk: Option<f64>,
    pub cost_per_tb_traffic_limit: Option<f64>,
    pub traffic_limit_type: Option<String>,
}
```

Implementation notes:

- Validate cycle before calling `traffic::get_cycle_range`.
- Use `cycle_days = (cycle_end - cycle_start).num_days() + 1`.
- Use `days_elapsed = ((today - cycle_start).num_days() + 1).clamp(0, cycle_days)`.
- `price == 0.0` returns `cycle_burn_percent = None`.
- `cost_per_month_equivalent = price`, `price / 3.0`, or `price / 12.0`.
- Use bytes divided by `1024_f64.powi(3)` or `1024_f64.powi(4)`.
- Return `None` for traffic unit cost when `traffic_limit_type` is not `sum/up/down/None`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p serverbee-server service::cost::tests -- --nocapture`

Expected: all cost math tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/cost.rs
git commit -m "feat(server): compute normalized vps cost metrics"
```

### Task 3: Implement value scoring

**Files:**

- Modify: `crates/server/src/service/cost.rs`
- Test: `crates/server/src/service/cost.rs`

- [ ] **Step 1: Write failing unit tests for score edges**

Add tests covering:

```rust
#[test]
fn grade_boundaries_are_half_open() {
    assert_eq!(CostService::grade_for_score(90.0), ValueGrade::Excellent);
    assert_eq!(CostService::grade_for_score(75.0), ValueGrade::Good);
    assert_eq!(CostService::grade_for_score(60.0), ValueGrade::Okay);
    assert_eq!(CostService::grade_for_score(40.0), ValueGrade::Poor);
    assert_eq!(CostService::grade_for_score(39.9), ValueGrade::Waste);
}

#[test]
fn reasons_are_limited_and_prioritized() {
    let reasons = CostService::prioritize_reasons(vec![
        ValueReason::HealthyUptime,
        ValueReason::IdleBurn,
        ValueReason::SleepingMoney,
        ValueReason::ExpensiveCpu,
    ]);
    assert_eq!(
        reasons,
        vec![ValueReason::SleepingMoney, ValueReason::IdleBurn, ValueReason::ExpensiveCpu]
    );
}
```

Add at least one test for single-server resource scoring:

```rust
#[test]
fn single_server_resource_metric_uses_neutral_score() {
    let score = CostService::resource_percentile_score(5.0, &[5.0]);
    assert_eq!(score, 0.5);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p serverbee-server service::cost::tests -- --nocapture`

Expected: compile failure for missing scoring helpers.

- [ ] **Step 3: Implement score structs and helpers**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct ValueScore {
    pub score: f64,
    pub grade: ValueGrade,
    pub reasons: Vec<ValueReason>,
    pub confidence: ValueConfidence,
}

#[derive(Debug, Clone, Default)]
struct UtilizationStats {
    avg_cpu: Option<f64>,
    avg_memory_percent: Option<f64>,
    has_network_activity: bool,
    has_disk_io_activity: bool,
}
```

Implement:

- `grade_for_score(score: f64) -> ValueGrade`
- `prioritize_reasons(Vec<ValueReason>) -> Vec<ValueReason>`
- `resource_percentile_score(value: f64, comparable_values: &[f64]) -> f64`
- internal `compute_utilization_score(stats, monthly_cost, fleet_monthly_costs)`
- internal `compute_reliability_score(uptime_ratio, online, expired_at)`

Keep helpers small and testable. Do not query the DB in these helpers.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p serverbee-server service::cost::tests -- --nocapture`

Expected: scoring tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/cost.rs
git commit -m "feat(server): score vps cost value"
```

### Task 4: Implement DB-backed cost overview/detail methods

**Files:**

- Modify: `crates/server/src/service/cost.rs`
- Test: `crates/server/src/service/cost.rs`

- [ ] **Step 1: Write failing async service tests**

Use `crate::test_utils::setup_test_db()` and insert `server::ActiveModel`, `record::ActiveModel`, and `uptime_daily::ActiveModel`.

Test cases:

- `overview_groups_currency_and_defaults_null_currency_to_usd`
- `detail_returns_missing_price_without_error`
- `expired_at_adds_expired_billing_reason_without_truncating_burn`
- `overview_uses_batch_inputs_for_multiple_servers`

Example assertion:

```rust
#[tokio::test]
async fn detail_returns_missing_price_without_error() {
    let (db, _tmp) = crate::test_utils::setup_test_db().await;
    insert_test_server(&db, "srv-1", None, Some("monthly")).await;
    let agent_manager = test_agent_manager();

    let insights = CostService::server_insights(&db, &agent_manager, "srv-1").await.unwrap();

    assert!(!insights.configured);
    assert_eq!(insights.invalid_reason, Some(CostInvalidReason::MissingPrice));
    assert!(insights.value_score.is_none());
}
```

Add this test helper instead of trying to use `Default` for `AgentManager`:

```rust
fn test_agent_manager() -> crate::service::agent_manager::AgentManager {
    let (browser_tx, _rx) = tokio::sync::broadcast::channel(16);
    crate::service::agent_manager::AgentManager::new(browser_tx)
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p serverbee-server service::cost::tests -- --nocapture`

Expected: compile failure for missing DB methods and test helpers.

- [ ] **Step 3: Implement DTOs and DB methods**

Add DTOs:

```rust
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct CostOverviewResponse {
    pub currencies: Vec<CurrencyCostSummary>,
    pub servers: Vec<ServerCostOverview>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct CurrencyCostSummary {
    pub currency: String,
    pub configured_server_count: u32,
    pub monthly_equivalent_total: f64,
    pub daily_total: f64,
    pub cycle_elapsed_total: f64,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ServerCostOverview {
    pub server_id: String,
    pub name: String,
    pub configured: bool,
    pub invalid_reason: Option<CostInvalidReason>,
    pub currency: Option<String>,
    pub billing_cycle: Option<String>,
    pub cost_per_second: Option<f64>,
    pub cost_per_hour: Option<f64>,
    pub cost_per_day: Option<f64>,
    pub cost_per_month_equivalent: Option<f64>,
    pub cycle_cost_elapsed: Option<f64>,
    pub cycle_burn_percent: Option<f64>,
    pub days_remaining: Option<i64>,
    pub value_score: Option<ValueScore>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ServerCostInsights {
    pub server_id: String,
    pub configured: bool,
    pub invalid_reason: Option<CostInvalidReason>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub billing_cycle: Option<String>,
    pub cycle_start: Option<String>,
    pub cycle_end: Option<String>,
    pub cycle_days: Option<i64>,
    pub days_elapsed: Option<i64>,
    pub days_remaining: Option<i64>,
    pub cost_per_second: Option<f64>,
    pub cost_per_hour: Option<f64>,
    pub cost_per_day: Option<f64>,
    pub cost_per_month_equivalent: Option<f64>,
    pub cycle_cost_elapsed: Option<f64>,
    pub cycle_cost_remaining: Option<f64>,
    pub cycle_burn_percent: Option<f64>,
    pub resource_value: Option<ResourceValue>,
    pub value_score: Option<ValueScore>,
}
```

Implement public methods:

```rust
pub async fn overview(
    db: &DatabaseConnection,
    agent_manager: &crate::service::agent_manager::AgentManager,
) -> Result<CostOverviewResponse, AppError>

pub async fn server_insights(
    db: &DatabaseConnection,
    agent_manager: &crate::service::agent_manager::AgentManager,
    server_id: &str,
) -> Result<ServerCostInsights, AppError>
```

Implementation constraints:

- Fetch all servers once for overview.
- Query recent record aggregation in one grouped SQL query for all server IDs.
- Query uptime_daily aggregation in one grouped SQL query for all server IDs.
- Build resource comparable values by currency in memory.
- Map one internal computed struct to both overview and detail DTO.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p serverbee-server service::cost::tests -- --nocapture`

Expected: all cost service tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/cost.rs
git commit -m "feat(server): build cost overview and insights"
```

### Task 5: Add server update validation

**Files:**

- Modify: `crates/server/src/service/server.rs`
- Test: `crates/server/src/service/server.rs`

- [ ] **Step 1: Write failing validation tests**

Add tests under `#[cfg(test)] mod tests` in `crates/server/src/service/server.rs`, or extend existing test module if present:

```rust
#[tokio::test]
async fn update_server_rejects_negative_price() {
    let (db, _tmp) = crate::test_utils::setup_test_db().await;
    insert_server(&db, "srv-1").await;
    let err = ServerService::update_server(
        &db,
        "srv-1",
        UpdateServerInput {
            price: Some(Some(-1.0)),
            ..Default::default()
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Validation(_)));
}
```

Also test invalid `billing_cycle`, invalid `traffic_limit_type`, and invalid `billing_start_day`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p serverbee-server service::server::tests::update_server_rejects -- --nocapture`

Expected: tests fail because validation is missing and `UpdateServerInput` may need `Default`.

- [ ] **Step 3: Implement validation**

In `UpdateServerInput`, derive or implement `Default` if needed for tests.

In `ServerService::update_server()` before mutating the ActiveModel:

```rust
fn validate_billing_cycle(value: &Option<String>) -> Result<(), AppError> {
    match value.as_deref() {
        None | Some("monthly" | "quarterly" | "yearly") => Ok(()),
        Some(_) => Err(AppError::Validation("billing_cycle must be monthly, quarterly, or yearly".into())),
    }
}
```

Add equivalent validation for:

- `price >= 0.0`
- `traffic_limit_type in {sum, up, down}`
- `billing_start_day in 1..=28`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p serverbee-server service::server::tests -- --nocapture`

Expected: server service validation tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service/server.rs
git commit -m "fix(server): validate billing cost inputs"
```

---

## Chunk 2: Backend API and OpenAPI

### Task 6: Add cost API routes

**Files:**

- Create: `crates/server/src/router/api/cost.rs`
- Modify: `crates/server/src/router/api/mod.rs`
- Test: `crates/server/tests/cost_integration.rs`

- [ ] **Step 1: Write failing integration tests**

Create `crates/server/tests/cost_integration.rs` with local helpers copied from `integration.rs` as needed:

- `start_test_server()`
- `http_client()`
- `login_admin()`
- `register_agent()` or direct server insert helpers

Test:

```rust
#[tokio::test]
async fn cost_overview_requires_auth_and_returns_configured_servers() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();

    let unauth = client.get(format!("{}/api/cost/overview", base_url)).send().await.unwrap();
    assert_eq!(unauth.status(), 401);

    login_admin(&client, &base_url).await;
    let resp = client.get(format!("{}/api/cost/overview", base_url)).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["data"]["servers"].is_array());
}
```

Add tests for:

- `GET /api/servers/{id}/cost-insights`
- unconfigured price returns 200 with `configured = false`
- response does not include `token_hash` or `token_prefix`
- `/api/traffic/overview` response does not include cost fields

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p serverbee-server --test cost_integration -- --nocapture`

Expected: 404 for cost endpoints or compile failure for missing test helpers.

- [ ] **Step 3: Implement router**

Create `crates/server/src/router/api/cost.rs`:

```rust
use std::sync::Arc;
use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};

use crate::error::{ApiResponse, AppError, ok};
use crate::service::cost::{CostOverviewResponse, CostService, ServerCostInsights};
use crate::state::AppState;

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/cost/overview", get(get_cost_overview))
        .route("/servers/{id}/cost-insights", get(get_server_cost_insights))
}

#[utoipa::path(
    get,
    path = "/api/cost/overview",
    responses((status = 200, description = "Cost overview", body = ApiResponse<CostOverviewResponse>)),
    tag = "cost",
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_cost_overview(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<CostOverviewResponse>>, AppError> {
    ok(CostService::overview(&state.db, &state.agent_manager).await?)
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/cost-insights",
    params(("id" = String, Path, description = "Server ID")),
    responses((status = 200, description = "Server cost insights", body = ApiResponse<ServerCostInsights>)),
    tag = "cost",
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_server_cost_insights(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ServerCostInsights>>, AppError> {
    ok(CostService::server_insights(&state.db, &state.agent_manager, &id).await?)
}
```

Modify `crates/server/src/router/api/mod.rs`:

- Add `pub mod cost;`
- Merge `.merge(cost::read_router())` in the authenticated read-only router.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p serverbee-server --test cost_integration -- --nocapture`

Expected: cost integration tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/api/cost.rs crates/server/src/router/api/mod.rs crates/server/tests/cost_integration.rs
git commit -m "feat(server): expose cost insights api"
```

### Task 7: Register OpenAPI schemas and regenerate frontend API types

**Files:**

- Modify: `crates/server/src/openapi.rs`
- Generated: `apps/web/openapi.json`
- Generated: `apps/web/src/lib/api-types.ts`
- Modify: `apps/web/src/lib/api-schema.ts`

- [ ] **Step 1: Add cost OpenAPI registration**

Modify `crates/server/src/openapi.rs`:

- Add paths:
  - `crate::router::api::cost::get_cost_overview`
  - `crate::router::api::cost::get_server_cost_insights`
- Add schemas:
  - `crate::service::cost::CostOverviewResponse`
  - `crate::service::cost::CurrencyCostSummary`
  - `crate::service::cost::ServerCostOverview`
  - `crate::service::cost::ServerCostInsights`
  - `crate::service::cost::ResourceValue`
  - `crate::service::cost::ValueScore`
  - `crate::service::cost::CostInvalidReason`
  - `crate::service::cost::ValueGrade`
  - `crate::service::cost::ValueReason`
  - `crate::service::cost::ValueConfidence`

- [ ] **Step 2: Verify OpenAPI dump compiles**

Run: `cargo run --example dump_openapi`

Expected: JSON emits to stdout and exits 0.

- [ ] **Step 3: Regenerate frontend API types**

Run from repo root:

```bash
cd apps/web
bun run generate:api-types
```

Expected:

- `apps/web/openapi.json` changes.
- `apps/web/src/lib/api-types.ts` changes.
- New cost schemas appear in generated types.

- [ ] **Step 4: Add api-schema re-exports**

Modify `apps/web/src/lib/api-schema.ts`:

```ts
export type CostOverviewResponse = S['CostOverviewResponse']
export type CurrencyCostSummary = S['CurrencyCostSummary']
export type ServerCostOverview = S['ServerCostOverview']
export type ServerCostInsights = S['ServerCostInsights']
export type ResourceValue = S['ResourceValue']
export type ValueScore = S['ValueScore']
export type CostInvalidReason = S['CostInvalidReason']
export type ValueGrade = S['ValueGrade']
export type ValueReason = S['ValueReason']
export type ValueConfidence = S['ValueConfidence']
```

- [ ] **Step 5: Run type generation-related checks**

Run:

```bash
cargo check -p serverbee-server
cd apps/web
bun run typecheck
```

Expected: both pass. If `bun run typecheck` fails due to frontend code not yet consuming new types, note the failure and re-run after frontend tasks.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/openapi.rs apps/web/openapi.json apps/web/src/lib/api-types.ts apps/web/src/lib/api-schema.ts
git commit -m "feat(api): document cost insights endpoints"
```

---

## Chunk 3: Frontend Cost UI

### Task 8: Add cost hooks and formatting utilities

**Files:**

- Create: `apps/web/src/hooks/use-cost.ts`
- Create: `apps/web/src/lib/cost.ts`
- Create: `apps/web/src/lib/cost.test.ts`

- [ ] **Step 1: Write failing pure utility tests**

Create `apps/web/src/lib/cost.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { formatCostAmount, getCostGradeClassName, getCostReasonKey } from './cost'

describe('cost utilities', () => {
  it('formats tiny per-second costs without rounding to zero', () => {
    expect(formatCostAmount(0.0000019, 'USD', { maximumFractionDigits: 8 })).toContain('0.0000019')
  })

  it('maps waste grade to destructive style', () => {
    expect(getCostGradeClassName('waste')).toContain('text-red')
  })

  it('maps known reasons to translation keys', () => {
    expect(getCostReasonKey('sleeping_money')).toBe('cost_reason_sleeping_money')
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd apps/web
bun run test src/lib/cost.test.ts
```

Expected: module not found.

- [ ] **Step 3: Implement utilities and hooks**

Create `apps/web/src/lib/cost.ts`:

- `formatCostAmount(amount, currency, options?)`
- `formatCostRate(amount, currency, unit)`
- `getCostGradeClassName(grade)`
- `getCostReasonKey(reason)`
- `getCostInvalidReasonKey(reason)`

Create `apps/web/src/hooks/use-cost.ts`:

```ts
import { useQuery } from '@tanstack/react-query'
import { api } from '@/lib/api-client'
import type { CostOverviewResponse, ServerCostInsights } from '@/lib/api-schema'

export function useCostOverview() {
  return useQuery<CostOverviewResponse>({
    queryKey: ['cost', 'overview'],
    queryFn: () => api.get<CostOverviewResponse>('/api/cost/overview'),
    staleTime: 60_000
  })
}

export function useCostInsights(serverId: string) {
  return useQuery<ServerCostInsights>({
    queryKey: ['servers', serverId, 'cost-insights'],
    queryFn: () => api.get<ServerCostInsights>(`/api/servers/${serverId}/cost-insights`),
    enabled: serverId.length > 0,
    staleTime: 60_000
  })
}
```

- [ ] **Step 4: Run tests**

Run:

```bash
cd apps/web
bun run test src/lib/cost.test.ts
```

Expected: tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/hooks/use-cost.ts apps/web/src/lib/cost.ts apps/web/src/lib/cost.test.ts
git commit -m "feat(web): add cost insight data helpers"
```

### Task 9: Add CostCell to server table

**Files:**

- Create: `apps/web/src/components/server/cost-cell.tsx`
- Modify: `apps/web/src/routes/_authed/servers/index.tsx`
- Test: `apps/web/src/components/server/cost-cell.test.tsx`

- [ ] **Step 1: Write failing component tests**

Create `apps/web/src/components/server/cost-cell.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { CostCell } from './cost-cell'

vi.mock('react-i18next', () => ({ useTranslation: () => ({ t: (key: string) => key }) }))

describe('CostCell', () => {
  it('renders not set for missing price', () => {
    render(<CostCell entry={{ server_id: 'srv-1', name: 'srv', configured: false, invalid_reason: 'missing_price' }} />)
    expect(screen.getByText('cost_not_set')).toBeDefined()
  })

  it('renders price only for missing cycle', () => {
    render(<CostCell entry={{ server_id: 'srv-1', name: 'srv', configured: false, invalid_reason: 'missing_billing_cycle', currency: 'USD' }} />)
    expect(screen.getByText('cost_price_only')).toBeDefined()
  })
})
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cd apps/web
bun run test src/components/server/cost-cell.test.tsx
```

Expected: module not found.

- [ ] **Step 3: Implement CostCell**

Create `apps/web/src/components/server/cost-cell.tsx`:

- Accept `entry?: ServerCostOverview`.
- Render muted fallback when missing.
- Main value: month equivalent or daily cost.
- Secondary value: `score N · grade`.
- Use `getCostGradeClassName()`.

- [ ] **Step 4: Wire into server list**

Modify `apps/web/src/routes/_authed/servers/index.tsx`:

- Import `CostCell`.
- Import/use `useCostOverview()`.
- Build `costByServerId`.
- Add column after `network`:

```tsx
{
  id: 'cost',
  accessorFn: (row) => costByServerId.get(row.id)?.cost_per_month_equivalent ?? -1,
  header: ({ column }) => <DataTableColumnHeader column={column} label={t('col_cost')} />,
  cell: ({ row }) => <CostCell entry={costByServerId.get(row.original.id)} />,
  size: 150,
  meta: { className: 'hidden xl:table-cell xl:w-[150px]', cellClassName: 'xl:align-top', label: t('col_cost') }
}
```

- [ ] **Step 5: Run tests**

Run:

```bash
cd apps/web
bun run test src/components/server/cost-cell.test.tsx src/routes/_authed/servers/index.cells.test.tsx
```

Expected: tests pass.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/server/cost-cell.tsx apps/web/src/components/server/cost-cell.test.tsx apps/web/src/routes/_authed/servers/index.tsx
git commit -m "feat(web): show cost in server table"
```

### Task 10: Add server card cost footnote

**Files:**

- Create: `apps/web/src/components/server/cost-footnote.tsx`
- Modify: `apps/web/src/components/server/server-card.tsx`
- Test: `apps/web/src/components/server/server-card.test.tsx`

- [ ] **Step 1: Write failing test**

In `server-card.test.tsx`, mock `useCostOverview`:

```tsx
const mockCostOverview = vi.fn()
vi.mock('@/hooks/use-cost', () => ({
  useCostOverview: (...args: unknown[]) => mockCostOverview(...args)
}))
```

Add:

```tsx
it('renders compact cost footnote when cost overview is available', () => {
  mockCostOverview.mockReturnValue({
    data: {
      currencies: [],
      servers: [{
        server_id: 'srv-1',
        name: 'test-server',
        configured: true,
        currency: 'USD',
        cost_per_hour: 0.01,
        value_score: { score: 82, grade: 'good', reasons: [], confidence: 'high' }
      }]
    }
  })
  render(<ServerCard server={makeServer()} />)
  expect(screen.getByText(/good/)).toBeDefined()
})
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cd apps/web
bun run test src/components/server/server-card.test.tsx
```

Expected: cost footnote text not found.

- [ ] **Step 3: Implement CostFootnote**

Create `apps/web/src/components/server/cost-footnote.tsx`:

- Accept `entry?: ServerCostOverview`.
- Render nothing if no entry.
- Render `cost_not_set` or `cost_price_only` for unconfigured states.
- Render `formatCostRate(entry.cost_per_hour, currency, 'h') · grade`.
- Use existing tooltip component if available; otherwise keep plain compact text in first pass.

- [ ] **Step 4: Wire into ServerCard**

Modify `apps/web/src/components/server/server-card.tsx`:

- Import `useCostOverview`.
- Import `CostFootnote`.
- Add `const { data: costOverview } = useCostOverview()`.
- Find `costEntry` by `server.id`.
- Add `<CostFootnote entry={costEntry} />` near existing footnote metrics without changing card density.

- [ ] **Step 5: Run tests**

Run:

```bash
cd apps/web
bun run test src/components/server/server-card.test.tsx
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/server/cost-footnote.tsx apps/web/src/components/server/server-card.tsx apps/web/src/components/server/server-card.test.tsx
git commit -m "feat(web): add server card cost signal"
```

### Task 11: Add detail-page CostInsightBar

**Files:**

- Create: `apps/web/src/components/server/cost-insight-bar.tsx`
- Modify: `apps/web/src/routes/_authed/servers/$id.tsx`
- Test: `apps/web/src/routes/_authed/servers/$id.test.tsx`

- [ ] **Step 1: Write failing route/component test**

In `$id.test.tsx`, mock `useCostInsights` and assert detail cost text appears when configured:

```tsx
vi.mock('@/hooks/use-cost', () => ({
  useCostInsights: () => ({
    data: {
      configured: true,
      price: 5,
      currency: 'USD',
      billing_cycle: 'monthly',
      cost_per_day: 0.16,
      cost_per_hour: 0.0068,
      cost_per_second: 0.0000019,
      cycle_cost_elapsed: 2.71,
      cycle_burn_percent: 54.2,
      days_remaining: 14,
      value_score: { score: 82, grade: 'good', reasons: ['healthy_uptime'], confidence: 'high' },
      resource_value: {}
    }
  })
}))
```

Assert `cost_value_score` or `82` appears.

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cd apps/web
bun run test 'src/routes/_authed/servers/$id.test.tsx'
```

Expected: cost insight UI not found.

- [ ] **Step 3: Implement CostInsightBar**

Create `apps/web/src/components/server/cost-insight-bar.tsx`:

- Accept `serverId` and basic server billing fallback props.
- Use `useCostInsights(serverId)`.
- Summary:
  - price/cycle
  - day/hour/second rates
  - cycle burned/days remaining
  - score/grade
- Breakdown:
  - CPU core, GB memory, GB disk, TB traffic costs
  - reasons, including `expired_billing` text stating it is an operational reminder and not part of value score.
- On API failure or missing insight data, fallback to existing billing display behavior.

- [ ] **Step 4: Replace BillingInfoBar usage**

Modify `apps/web/src/routes/_authed/servers/$id.tsx`:

- Import `CostInsightBar`.
- Replace `{hasBilling && <BillingInfoBar ... />}` with `CostInsightBar`.
- Delete `BillingInfoBar` only if no longer used.
- Keep `TrafficProgress` behavior if the new bar does not include it yet.

- [ ] **Step 5: Run tests**

Run:

```bash
cd apps/web
bun run test 'src/routes/_authed/servers/$id.test.tsx'
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/components/server/cost-insight-bar.tsx 'apps/web/src/routes/_authed/servers/$id.tsx' 'apps/web/src/routes/_authed/servers/$id.test.tsx'
git commit -m "feat(web): show server cost insights"
```

### Task 12: Add i18n labels and reason copy

**Files:**

- Modify: `apps/web/src/locales/en/servers.json`
- Modify: `apps/web/src/locales/zh/servers.json`
- Test: frontend tests from Tasks 9-11

- [ ] **Step 1: Add English keys**

Add keys near existing server table/detail labels:

```json
"col_cost": "Cost",
"cost_not_set": "not set",
"cost_price_only": "price only",
"cost_invalid": "invalid",
"cost_month_equivalent": "{{amount}}/mo eq.",
"cost_per_day": "{{amount}}/day",
"cost_per_hour": "{{amount}}/h",
"cost_per_second": "{{amount}}/s",
"cost_burned": "burned {{amount}}",
"cost_days_left": "{{count}}d left",
"cost_value_score": "Value score",
"cost_grade_excellent": "excellent",
"cost_grade_good": "good",
"cost_grade_okay": "okay",
"cost_grade_poor": "poor",
"cost_grade_waste": "waste",
"cost_reason_idle_burn": "Low usage while still burning budget.",
"cost_reason_sleeping_money": "Offline while the bill keeps running.",
"cost_reason_good_memory_value": "Memory value is strong for this currency group.",
"cost_reason_good_disk_value": "Disk value is strong for this currency group.",
"cost_reason_expensive_cpu": "CPU cost is high for this currency group.",
"cost_reason_healthy_uptime": "Uptime is healthy.",
"cost_reason_low_uptime": "Recent uptime is weak.",
"cost_reason_expired_billing": "Billing date is expired; this is an operational reminder, not part of the value score.",
"cost_reason_insufficient_data": "Not enough data; score confidence is lower.",
"cost_reason_free_or_zero_price": "Zero price is excluded from fleet value comparisons."
```

- [ ] **Step 2: Add Chinese keys**

Add equivalent Chinese translations in `apps/web/src/locales/zh/servers.json`.

- [ ] **Step 3: Run affected frontend tests**

Run:

```bash
cd apps/web
bun run test src/components/server/cost-cell.test.tsx src/components/server/server-card.test.tsx 'src/routes/_authed/servers/$id.test.tsx'
```

Expected: pass.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/locales/en/servers.json apps/web/src/locales/zh/servers.json
git commit -m "feat(web): add cost insight copy"
```

---

## Chunk 4: Verification and Cleanup

### Task 13: Run targeted backend verification

**Files:**

- Verify only.

- [ ] **Step 1: Format Rust**

Run: `cargo fmt`

Expected: exits 0.

- [ ] **Step 2: Run targeted Rust tests**

Run:

```bash
cargo test -p serverbee-server service::cost::tests -- --nocapture
cargo test -p serverbee-server service::server::tests -- --nocapture
cargo test -p serverbee-server --test cost_integration -- --nocapture
```

Expected: all pass.

- [ ] **Step 3: Run Rust clippy required by repo rules**

Run: `cargo clippy --all --benches --tests --examples --all-features`

Expected: exits 0 with no warnings. Fix warnings before continuing.

- [ ] **Step 4: Commit any verification fixes**

If formatting/clippy changed files:

```bash
git add <changed files>
git commit -m "fix(cost): address verification feedback"
```

### Task 14: Run targeted frontend verification

**Files:**

- Verify only.

- [ ] **Step 1: Run focused Vitest suite**

Run:

```bash
cd apps/web
bun run test src/lib/cost.test.ts src/components/server/cost-cell.test.tsx src/components/server/server-card.test.tsx 'src/routes/_authed/servers/$id.test.tsx'
```

Expected: all pass.

- [ ] **Step 2: Run frontend typecheck**

Run:

```bash
cd apps/web
bun run typecheck
```

Expected: exits 0.

- [ ] **Step 3: Run frontend lint/format check**

Run:

```bash
cd apps/web
bun x ultracite check
```

Expected: exits 0. If it reports fixable formatting, run `bun x ultracite fix`, inspect diff, then re-run `bun x ultracite check`.

- [ ] **Step 4: Commit any verification fixes**

If frontend verification changed files:

```bash
git add <changed files>
git commit -m "fix(web): address cost insight verification"
```

### Task 15: Final review checklist

**Files:**

- Verify only.

- [ ] **Step 1: Inspect final diff**

Run:

```bash
git status --short
git log --oneline -8
```

Expected: worktree clean after final commit; recent commits show the cost feature sequence.

- [ ] **Step 2: Confirm acceptance criteria**

Check each item manually against code/tests:

- Cost formulas live in `CostService`, not frontend components.
- Unknown billing cycle returns `invalid_billing_cycle` and does not call `get_cycle_range()`.
- `price = 0` cannot emit NaN/inf.
- `ResourceValue` uses month-equivalent costs.
- `traffic_limit_type` uses `sum/up/down`.
- `CostOverview` is batched and does not N+1 over servers.
- Member and Admin can read cost APIs, matching existing `/api/servers` visibility.
- `/api/traffic/overview` has no cost fields.
- Frontend list, card, and detail page use hooks/components, not duplicated formulas.

- [ ] **Step 3: Prepare handoff summary**

Include:

- Commit hashes.
- Tests run and pass/fail status.
- Any skipped verification and why.
- Any intentional non-goals left for follow-up.
