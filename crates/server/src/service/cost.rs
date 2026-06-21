use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, EntityTrait, Statement, Value,
};
use serde::Serialize;

use crate::entity::server;
use crate::error::AppError;
use crate::service::traffic;

const RECORD_LOOKBACK_HOURS: i64 = 24;
const UPTIME_RECENT_DAYS: i64 = 30;
/// Recent 30-day uptime below this ratio surfaces a `LowUptime` advisory.
const LOW_UPTIME_THRESHOLD: f64 = 0.90;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CostInvalidReason {
    MissingPrice,
    MissingBillingCycle,
    InvalidBillingCycle,
    InvalidPrice,
}

/// A cost advisory: an objective, per-server warning surfaced alongside the cost
/// breakdown. Unlike the former composite "value score", each advisory is a
/// single actionable fact computed without any fleet comparison, so it is
/// meaningful even for a single-server deployment. The variants are declared in
/// display-priority order (most urgent first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CostAdvisory {
    /// Billing date (`expired_at`) is in the past — renew before losing the host.
    ExpiredBilling,
    /// Offline with no recent samples while still paying a non-zero bill.
    SleepingMoney,
    /// Resources sit idle (low CPU + memory, no real I/O) while paying a bill.
    IdleBurn,
    /// Recent 30-day uptime is below 90%.
    LowUptime,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    pub advisories: Vec<CostAdvisory>,
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
    pub advisories: Vec<CostAdvisory>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UtilizationStats {
    avg_cpu: Option<f64>,
    avg_memory_percent: Option<f64>,
    has_network_activity: bool,
    has_disk_io_activity: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NormalizedCostConfig {
    pub configured: bool,
    pub invalid_reason: Option<CostInvalidReason>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub billing_cycle: Option<String>,
}

#[derive(Debug, Clone)]
struct ComputedCost {
    server: server::Model,
    config: NormalizedCostConfig,
    burn: Option<CostBurn>,
    resource_value: Option<ResourceValue>,
    utilization_stats: UtilizationStats,
    uptime_ratio: Option<f64>,
    online: bool,
    advisories: Vec<CostAdvisory>,
}

#[derive(Debug, Clone, Default)]
struct UptimeAggregate {
    total_minutes: i64,
    online_minutes: i64,
}

pub struct CostService;

impl CostService {
    pub async fn overview(
        db: &DatabaseConnection,
        agent_manager: &crate::service::agent_manager::AgentManager,
    ) -> Result<CostOverviewResponse, AppError> {
        let servers = server::Entity::find().all(db).await?;
        let computed = compute_costs(db, agent_manager, servers).await?;

        Ok(CostOverviewResponse {
            currencies: currency_summaries(&computed),
            servers: computed.iter().map(map_overview).collect(),
        })
    }

    pub async fn server_insights(
        db: &DatabaseConnection,
        agent_manager: &crate::service::agent_manager::AgentManager,
        server_id: &str,
    ) -> Result<ServerCostInsights, AppError> {
        let servers = server::Entity::find().all(db).await?;
        if !servers.iter().any(|server| server.id == server_id) {
            return Err(AppError::NotFound("Server not found".to_string()));
        }

        let computed = compute_costs(db, agent_manager, servers).await?;
        computed
            .iter()
            .find(|entry| entry.server.id == server_id)
            .map(map_insights)
            .ok_or_else(|| AppError::NotFound("Server not found".to_string()))
    }

    pub(crate) fn normalize_config(
        price: Option<f64>,
        billing_cycle: Option<&str>,
        currency: Option<&str>,
    ) -> NormalizedCostConfig {
        let normalized_currency = Some(normalize_currency(currency));
        let normalized_billing_cycle = billing_cycle
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(String::from);

        let invalid_reason = if price.is_none() {
            Some(CostInvalidReason::MissingPrice)
        } else if price.is_some_and(|value| !value.is_finite() || value < 0.0) {
            Some(CostInvalidReason::InvalidPrice)
        } else if normalized_billing_cycle.is_none() {
            Some(CostInvalidReason::MissingBillingCycle)
        } else if normalized_billing_cycle
            .as_deref()
            .is_some_and(|value| !matches!(value, "monthly" | "quarterly" | "yearly"))
        {
            Some(CostInvalidReason::InvalidBillingCycle)
        } else {
            None
        };

        NormalizedCostConfig {
            configured: invalid_reason.is_none(),
            invalid_reason,
            price,
            currency: normalized_currency,
            billing_cycle: normalized_billing_cycle,
        }
    }

    pub fn compute_burn(
        price: f64,
        billing_cycle: &str,
        billing_start_day: Option<i32>,
        today: NaiveDate,
    ) -> Result<CostBurn, CostInvalidReason> {
        if !price.is_finite() || price < 0.0 {
            return Err(CostInvalidReason::InvalidPrice);
        }
        if !is_valid_billing_cycle(billing_cycle) {
            return Err(CostInvalidReason::InvalidBillingCycle);
        }

        let (cycle_start, cycle_end) =
            traffic::get_cycle_range(billing_cycle, billing_start_day, today);
        let cycle_days = (cycle_end - cycle_start).num_days() + 1;
        let days_elapsed = ((today - cycle_start).num_days() + 1).clamp(0, cycle_days);
        let days_remaining = cycle_days - days_elapsed;
        let cost_per_month_equivalent = cost_per_month_equivalent(price, billing_cycle);

        let (
            cost_per_second,
            cost_per_hour,
            cost_per_day,
            cycle_cost_elapsed,
            cycle_cost_remaining,
            cycle_burn_percent,
        ) = if price == 0.0 {
            (0.0, 0.0, 0.0, 0.0, 0.0, None)
        } else {
            let cost_per_day = price / cycle_days as f64;
            let cost_per_hour = cost_per_day / 24.0;
            let cost_per_second = cost_per_hour / 3600.0;
            let cycle_cost_elapsed = cost_per_day * days_elapsed as f64;
            let cycle_cost_remaining = (price - cycle_cost_elapsed).max(0.0);
            let cycle_burn_percent = Some(cycle_cost_elapsed / price * 100.0);

            (
                cost_per_second,
                cost_per_hour,
                cost_per_day,
                cycle_cost_elapsed,
                cycle_cost_remaining,
                cycle_burn_percent,
            )
        };

        Ok(CostBurn {
            cycle_start: cycle_start.to_string(),
            cycle_end: cycle_end.to_string(),
            cycle_days,
            days_elapsed,
            days_remaining,
            cost_per_second,
            cost_per_hour,
            cost_per_day,
            cost_per_month_equivalent,
            cycle_cost_elapsed,
            cycle_cost_remaining,
            cycle_burn_percent,
        })
    }

    pub fn compute_resource_value(
        cost_per_month_equivalent: f64,
        cpu_cores: Option<i32>,
        mem_total_bytes: Option<i64>,
        disk_total_bytes: Option<i64>,
        traffic_limit_bytes: Option<i64>,
        traffic_limit_type: Option<&str>,
    ) -> ResourceValue {
        let normalized_traffic_limit_type = traffic_limit_type.map(str::to_string);
        let traffic_type_is_valid =
            traffic_limit_type.is_none_or(|value| matches!(value, "sum" | "up" | "down"));

        ResourceValue {
            cost_per_cpu_core: divide_positive(cost_per_month_equivalent, cpu_cores.map(f64::from)),
            cost_per_gb_memory: divide_positive(
                cost_per_month_equivalent,
                mem_total_bytes.map(bytes_to_gib),
            ),
            cost_per_gb_disk: divide_positive(
                cost_per_month_equivalent,
                disk_total_bytes.map(bytes_to_gib),
            ),
            cost_per_tb_traffic_limit: if traffic_type_is_valid {
                divide_positive(
                    cost_per_month_equivalent,
                    traffic_limit_bytes.map(bytes_to_tib),
                )
            } else {
                None
            },
            traffic_limit_type: normalized_traffic_limit_type,
        }
    }
}

async fn compute_costs(
    db: &DatabaseConnection,
    agent_manager: &crate::service::agent_manager::AgentManager,
    servers: Vec<server::Model>,
) -> Result<Vec<ComputedCost>, AppError> {
    let now = Utc::now();
    let today = now.date_naive();
    let server_ids = servers
        .iter()
        .map(|server| server.id.clone())
        .collect::<Vec<_>>();
    let utilization_by_server = load_utilization_stats(db, &server_ids, now).await?;
    let uptime_by_server = load_uptime_aggregates(db, &server_ids, today).await?;

    let mut computed = servers
        .into_iter()
        .map(|server| {
            let config = CostService::normalize_config(
                server.price,
                server.billing_cycle.as_deref(),
                server.currency.as_deref(),
            );

            let burn = if config.configured {
                config.price.and_then(|price| {
                    config.billing_cycle.as_deref().and_then(|billing_cycle| {
                        CostService::compute_burn(
                            price,
                            billing_cycle,
                            server.billing_start_day,
                            today,
                        )
                        .ok()
                    })
                })
            } else {
                None
            };

            let resource_value = burn.as_ref().map(|burn| {
                CostService::compute_resource_value(
                    burn.cost_per_month_equivalent,
                    server.cpu_cores,
                    server.mem_total,
                    server.disk_total,
                    server.traffic_limit,
                    server.traffic_limit_type.as_deref(),
                )
            });

            let utilization_stats = utilization_by_server
                .get(&server.id)
                .cloned()
                .unwrap_or_default();
            let uptime_ratio = uptime_by_server.get(&server.id).and_then(uptime_ratio);
            let online = agent_manager.is_online(&server.id);

            ComputedCost {
                server,
                config,
                burn,
                resource_value,
                utilization_stats,
                uptime_ratio,
                online,
                advisories: Vec::new(),
            }
        })
        .collect::<Vec<_>>();

    for entry in &mut computed {
        entry.advisories = compute_advisories(entry, now);
    }

    computed.sort_by(|left, right| {
        left.server
            .name
            .cmp(&right.server.name)
            .then_with(|| left.server.id.cmp(&right.server.id))
    });

    Ok(computed)
}

async fn load_utilization_stats(
    db: &DatabaseConnection,
    server_ids: &[String],
    now: DateTime<Utc>,
) -> Result<HashMap<String, UtilizationStats>, AppError> {
    if server_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let recent_cutoff = now - chrono::Duration::hours(RECORD_LOOKBACK_HOURS);
    let placeholders = sql_placeholders(server_ids.len());
    // Network activity is gated on the per-window *speed* columns only.
    // `net_*_transfer` is a cumulative since-boot counter (sysinfo
    // `total_received/total_transmitted`), so it is non-zero for any host that
    // has ever moved a byte and would make this flag a constant.
    let sql = format!(
        r#"
        SELECT
            r.server_id,
            AVG(r.cpu) AS avg_cpu,
            AVG(
                CASE
                    WHEN s.mem_total IS NOT NULL AND s.mem_total > 0
                    THEN CAST(r.mem_used AS REAL) / CAST(s.mem_total AS REAL) * 100.0
                    ELSE NULL
                END
            ) AS avg_memory_percent,
            MAX(
                CASE
                    WHEN r.net_in_speed > 0
                        OR r.net_out_speed > 0
                    THEN 1
                    ELSE 0
                END
            ) AS has_network_activity,
            GROUP_CONCAT(r.disk_io_json, char(30)) AS disk_io_samples
        FROM records r
        INNER JOIN servers s ON s.id = r.server_id
        WHERE r.server_id IN ({placeholders})
            AND r.time >= ?
        GROUP BY r.server_id
    "#
    );

    let mut values = server_ids
        .iter()
        .cloned()
        .map(Value::from)
        .collect::<Vec<_>>();
    values.push(recent_cutoff.into());
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            sql,
            values,
        ))
        .await?;

    let mut stats = HashMap::new();
    for row in rows {
        let server_id: String = row.try_get_by_index(0)?;
        let avg_cpu: Option<f64> = row.try_get_by_index(1)?;
        let avg_memory_percent: Option<f64> = row.try_get_by_index(2)?;
        let has_network_activity: i64 = row.try_get_by_index(3).unwrap_or(0);
        let disk_io_samples: Option<String> = row.try_get_by_index(4)?;

        stats.insert(
            server_id,
            UtilizationStats {
                avg_cpu,
                avg_memory_percent,
                has_network_activity: has_network_activity > 0,
                has_disk_io_activity: disk_io_samples
                    .as_deref()
                    .is_some_and(disk_io_samples_have_activity),
            },
        );
    }

    Ok(stats)
}

async fn load_uptime_aggregates(
    db: &DatabaseConnection,
    server_ids: &[String],
    today: NaiveDate,
) -> Result<HashMap<String, UptimeAggregate>, AppError> {
    if server_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let recent_cutoff = today - chrono::Duration::days(UPTIME_RECENT_DAYS - 1);
    let placeholders = sql_placeholders(server_ids.len());
    let sql = format!(
        r#"
        SELECT
            server_id,
            SUM(total_minutes) AS total_minutes,
            SUM(online_minutes) AS online_minutes
        FROM uptime_daily
        WHERE server_id IN ({placeholders})
            AND date >= ?
        GROUP BY server_id
    "#
    );

    let mut values = server_ids
        .iter()
        .cloned()
        .map(Value::from)
        .collect::<Vec<_>>();
    values.push(recent_cutoff.to_string().into());
    let rows = db
        .query_all(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            sql,
            values,
        ))
        .await?;

    let mut aggregates = HashMap::new();
    for row in rows {
        let server_id: String = row.try_get_by_index(0)?;
        let total_minutes: i64 = row.try_get_by_index(1).unwrap_or(0);
        let online_minutes: i64 = row.try_get_by_index(2).unwrap_or(0);

        aggregates.insert(
            server_id,
            UptimeAggregate {
                total_minutes,
                online_minutes,
            },
        );
    }

    Ok(aggregates)
}

fn uptime_ratio(aggregate: &UptimeAggregate) -> Option<f64> {
    if aggregate.total_minutes <= 0 {
        return None;
    }

    Some((aggregate.online_minutes as f64 / aggregate.total_minutes as f64).clamp(0.0, 1.0))
}

fn sql_placeholders(count: usize) -> String {
    vec!["?"; count].join(", ")
}

fn disk_io_samples_have_activity(samples: &str) -> bool {
    samples
        .split('\u{1e}')
        .any(disk_io_json_sample_has_activity)
}

fn disk_io_json_sample_has_activity(sample: &str) -> bool {
    let sample = sample.trim();
    if sample.is_empty() {
        return false;
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(sample) else {
        return false;
    };

    match &value {
        serde_json::Value::Array(devices) => devices.iter().any(disk_io_device_has_activity),
        serde_json::Value::Object(_) => disk_io_device_has_activity(&value),
        _ => false,
    }
}

fn disk_io_device_has_activity(device: &serde_json::Value) -> bool {
    positive_json_number(device.get("read_bytes_per_sec"))
        || positive_json_number(device.get("write_bytes_per_sec"))
}

fn positive_json_number(value: Option<&serde_json::Value>) -> bool {
    value
        .and_then(serde_json::Value::as_f64)
        .is_some_and(|value| value.is_finite() && value > 0.0)
}

/// Derive the objective cost advisories for a server. Returns an empty list when
/// billing is unconfigured (no burn). Each advisory is decided per-server with
/// no fleet comparison, so the result is meaningful for single-server setups.
fn compute_advisories(entry: &ComputedCost, now: DateTime<Utc>) -> Vec<CostAdvisory> {
    if entry.burn.is_none() {
        return Vec::new();
    }

    let paying = entry.config.price.is_some_and(|price| price > 0.0);
    let stats = normalize_utilization_stats(&entry.utilization_stats);
    let has_samples = stats.avg_cpu.is_some() || stats.avg_memory_percent.is_some();
    let low_utilization = is_low_utilization(&stats);
    let expired = entry
        .server
        .expired_at
        .is_some_and(|expired_at| expired_at < now);

    advisories_from(
        paying,
        entry.online,
        has_samples,
        low_utilization,
        entry.uptime_ratio,
        expired,
    )
}

/// Pure advisory rules over already-derived primitives. Kept separate from
/// `compute_advisories` so the decision logic is unit-testable without a DB.
///
/// `SleepingMoney` and `IdleBurn` are mutually exclusive by construction:
/// "sleeping" requires no recent samples, while "idle" requires the low-usage
/// samples that prove the host was recently alive.
fn advisories_from(
    paying: bool,
    online: bool,
    has_utilization_samples: bool,
    low_utilization: bool,
    uptime_ratio: Option<f64>,
    expired: bool,
) -> Vec<CostAdvisory> {
    let mut advisories = Vec::new();

    if expired {
        advisories.push(CostAdvisory::ExpiredBilling);
    }
    if paying && !online && !has_utilization_samples {
        advisories.push(CostAdvisory::SleepingMoney);
    }
    if paying && low_utilization {
        advisories.push(CostAdvisory::IdleBurn);
    }
    if uptime_ratio.is_some_and(|ratio| ratio < LOW_UPTIME_THRESHOLD) {
        advisories.push(CostAdvisory::LowUptime);
    }

    advisories
}

fn currency_summaries(computed: &[ComputedCost]) -> Vec<CurrencyCostSummary> {
    let mut summaries = HashMap::<String, CurrencyCostSummary>::new();

    for entry in computed {
        let Some(burn) = entry.burn.as_ref() else {
            continue;
        };
        let currency = entry
            .config
            .currency
            .clone()
            .unwrap_or_else(|| "USD".to_string());
        let summary = summaries
            .entry(currency.clone())
            .or_insert_with(|| CurrencyCostSummary {
                currency,
                configured_server_count: 0,
                monthly_equivalent_total: 0.0,
                daily_total: 0.0,
                cycle_elapsed_total: 0.0,
            });

        summary.configured_server_count += 1;
        summary.monthly_equivalent_total += burn.cost_per_month_equivalent;
        summary.daily_total += burn.cost_per_day;
        summary.cycle_elapsed_total += burn.cycle_cost_elapsed;
    }

    let mut summaries = summaries.into_values().collect::<Vec<_>>();
    summaries.sort_by(|left, right| left.currency.cmp(&right.currency));
    summaries
}

fn map_overview(entry: &ComputedCost) -> ServerCostOverview {
    ServerCostOverview {
        server_id: entry.server.id.clone(),
        name: entry.server.name.clone(),
        configured: entry.config.configured,
        invalid_reason: entry.config.invalid_reason,
        currency: entry.config.currency.clone(),
        billing_cycle: entry.config.billing_cycle.clone(),
        cost_per_second: entry.burn.as_ref().map(|burn| burn.cost_per_second),
        cost_per_hour: entry.burn.as_ref().map(|burn| burn.cost_per_hour),
        cost_per_day: entry.burn.as_ref().map(|burn| burn.cost_per_day),
        cost_per_month_equivalent: entry
            .burn
            .as_ref()
            .map(|burn| burn.cost_per_month_equivalent),
        cycle_cost_elapsed: entry.burn.as_ref().map(|burn| burn.cycle_cost_elapsed),
        cycle_burn_percent: entry.burn.as_ref().and_then(|burn| burn.cycle_burn_percent),
        days_remaining: entry.burn.as_ref().map(|burn| burn.days_remaining),
        advisories: entry.advisories.clone(),
    }
}

fn map_insights(entry: &ComputedCost) -> ServerCostInsights {
    ServerCostInsights {
        server_id: entry.server.id.clone(),
        configured: entry.config.configured,
        invalid_reason: entry.config.invalid_reason,
        price: entry.config.price,
        currency: entry.config.currency.clone(),
        billing_cycle: entry.config.billing_cycle.clone(),
        cycle_start: entry.burn.as_ref().map(|burn| burn.cycle_start.clone()),
        cycle_end: entry.burn.as_ref().map(|burn| burn.cycle_end.clone()),
        cycle_days: entry.burn.as_ref().map(|burn| burn.cycle_days),
        days_elapsed: entry.burn.as_ref().map(|burn| burn.days_elapsed),
        days_remaining: entry.burn.as_ref().map(|burn| burn.days_remaining),
        cost_per_second: entry.burn.as_ref().map(|burn| burn.cost_per_second),
        cost_per_hour: entry.burn.as_ref().map(|burn| burn.cost_per_hour),
        cost_per_day: entry.burn.as_ref().map(|burn| burn.cost_per_day),
        cost_per_month_equivalent: entry
            .burn
            .as_ref()
            .map(|burn| burn.cost_per_month_equivalent),
        cycle_cost_elapsed: entry.burn.as_ref().map(|burn| burn.cycle_cost_elapsed),
        cycle_cost_remaining: entry.burn.as_ref().map(|burn| burn.cycle_cost_remaining),
        cycle_burn_percent: entry.burn.as_ref().and_then(|burn| burn.cycle_burn_percent),
        resource_value: entry.resource_value.clone(),
        advisories: entry.advisories.clone(),
    }
}

fn normalize_currency(currency: Option<&str>) -> String {
    let currency = currency.map(str::trim).filter(|value| !value.is_empty());
    currency.unwrap_or("USD").to_string()
}

fn is_valid_billing_cycle(billing_cycle: &str) -> bool {
    matches!(billing_cycle, "monthly" | "quarterly" | "yearly")
}

fn cost_per_month_equivalent(price: f64, billing_cycle: &str) -> f64 {
    match billing_cycle {
        "monthly" => price,
        "quarterly" => price / 3.0,
        "yearly" => price / 12.0,
        _ => unreachable!("billing cycle must be validated before cost normalization"),
    }
}

fn divide_positive(numerator: f64, denominator: Option<f64>) -> Option<f64> {
    if !is_valid_resource_cost(numerator) {
        return None;
    }

    denominator
        .filter(|value| value.is_finite() && *value > 0.0)
        .map(|value| numerator / value)
}

fn is_valid_resource_cost(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

fn normalize_utilization_stats(stats: &UtilizationStats) -> UtilizationStats {
    UtilizationStats {
        avg_cpu: stats.avg_cpu.filter(|value| value.is_finite()),
        avg_memory_percent: stats.avg_memory_percent.filter(|value| value.is_finite()),
        has_network_activity: stats.has_network_activity,
        has_disk_io_activity: stats.has_disk_io_activity,
    }
}

fn is_low_utilization(stats: &UtilizationStats) -> bool {
    let cpu_is_low = stats.avg_cpu.is_some_and(|value| value < 5.0);
    let memory_is_low = stats.avg_memory_percent.is_some_and(|value| value < 20.0);
    let io_is_quiet = !stats.has_network_activity && !stats.has_disk_io_activity;

    cpu_is_low && memory_is_low && io_is_quiet
}

fn bytes_to_gib(bytes: i64) -> f64 {
    bytes as f64 / 1024_f64.powi(3)
}

fn bytes_to_tib(bytes: i64) -> f64 {
    bytes as f64 / 1024_f64.powi(4)
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, TimeZone, Utc};
    use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
    use serverbee_common::constants::CAP_DEFAULT;

    use crate::entity::{record, server, uptime_daily};
    use crate::service::cost::{CostAdvisory, CostInvalidReason, CostService, UtilizationStats};

    fn assert_near(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {actual} to be within 0.0001 of {expected}"
        );
    }

    fn test_agent_manager() -> crate::service::agent_manager::AgentManager {
        let (browser_tx, _rx) = tokio::sync::broadcast::channel(16);
        crate::service::agent_manager::AgentManager::new(browser_tx)
    }

    async fn insert_test_server(
        db: &DatabaseConnection,
        id: &str,
        name: &str,
        price: Option<f64>,
        billing_cycle: Option<&str>,
        currency: Option<&str>,
        expired_at: Option<chrono::DateTime<Utc>>,
    ) {
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(Some("test_hash".to_string())),
            token_prefix: Set(Some("serverbee_test".to_string())),
            name: Set(name.to_string()),
            cpu_cores: Set(Some(2)),
            mem_total: Set(Some(8 * 1024_i64.pow(3))),
            disk_total: Set(Some(80 * 1024_i64.pow(3))),
            price: Set(price),
            billing_cycle: Set(billing_cycle.map(str::to_string)),
            currency: Set(currency.map(str::to_string)),
            expired_at: Set(expired_at),
            traffic_limit: Set(Some(1024_i64.pow(4))),
            traffic_limit_type: Set(Some("sum".to_string())),
            billing_start_day: Set(None),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            features: Set("[]".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert test server should succeed");
    }

    async fn insert_test_record(
        db: &DatabaseConnection,
        server_id: &str,
        cpu: f64,
        mem_used: i64,
        active_io: bool,
    ) {
        let disk_io_json = if active_io {
            Some(r#"[{"name":"vda","read_bytes_per_sec":1}]"#.to_string())
        } else {
            Some("[]".to_string())
        };
        insert_test_record_with_disk_io_json(db, server_id, cpu, mem_used, active_io, disk_io_json)
            .await;
    }

    async fn insert_test_record_with_disk_io_json(
        db: &DatabaseConnection,
        server_id: &str,
        cpu: f64,
        mem_used: i64,
        active_network: bool,
        disk_io_json: Option<String>,
    ) {
        record::ActiveModel {
            server_id: Set(server_id.to_string()),
            time: Set(Utc::now()),
            cpu: Set(cpu),
            mem_used: Set(mem_used),
            swap_used: Set(0),
            disk_used: Set(0),
            net_in_speed: Set(if active_network { 100 } else { 0 }),
            net_out_speed: Set(0),
            net_in_transfer: Set(if active_network { 1024 } else { 0 }),
            net_out_transfer: Set(0),
            load1: Set(0.0),
            load5: Set(0.0),
            load15: Set(0.0),
            tcp_conn: Set(0),
            udp_conn: Set(0),
            process_count: Set(0),
            disk_io_json: Set(disk_io_json),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert test record should succeed");
    }

    async fn insert_test_uptime(
        db: &DatabaseConnection,
        server_id: &str,
        date: NaiveDate,
        total_minutes: i32,
        online_minutes: i32,
    ) {
        uptime_daily::ActiveModel {
            server_id: Set(server_id.to_string()),
            date: Set(date),
            total_minutes: Set(total_minutes),
            online_minutes: Set(online_minutes),
            downtime_incidents: Set(0),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert test uptime should succeed");
    }

    #[tokio::test]
    async fn overview_groups_currency_and_defaults_null_currency_to_usd() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(
            &db,
            "srv-usd",
            "USD Server",
            Some(12.0),
            Some("monthly"),
            None,
            None,
        )
        .await;
        insert_test_server(
            &db,
            "srv-eur",
            "EUR Server",
            Some(24.0),
            Some("monthly"),
            Some("EUR"),
            None,
        )
        .await;

        let overview = CostService::overview(&db, &test_agent_manager())
            .await
            .expect("overview should succeed");

        let eur = overview
            .currencies
            .iter()
            .find(|summary| summary.currency == "EUR")
            .expect("EUR summary should exist");
        let usd = overview
            .currencies
            .iter()
            .find(|summary| summary.currency == "USD")
            .expect("USD summary should exist");
        assert_eq!(eur.configured_server_count, 1);
        assert_eq!(usd.configured_server_count, 1);
        assert_eq!(
            overview
                .servers
                .iter()
                .find(|entry| entry.server_id == "srv-usd")
                .and_then(|entry| entry.currency.as_deref()),
            Some("USD")
        );
    }

    #[tokio::test]
    async fn detail_returns_missing_price_without_error() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(
            &db,
            "srv-1",
            "Missing Price",
            None,
            Some("monthly"),
            None,
            None,
        )
        .await;

        let insights = CostService::server_insights(&db, &test_agent_manager(), "srv-1")
            .await
            .expect("server insights should succeed");

        assert!(!insights.configured);
        assert_eq!(
            insights.invalid_reason,
            Some(CostInvalidReason::MissingPrice)
        );
        assert!(insights.advisories.is_empty());
    }

    #[tokio::test]
    async fn expired_at_adds_expired_billing_advisory_without_truncating_burn() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let expired_at = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        insert_test_server(
            &db,
            "srv-expired",
            "Expired",
            Some(31.0),
            Some("monthly"),
            Some("USD"),
            Some(expired_at),
        )
        .await;
        insert_test_record(&db, "srv-expired", 35.0, 4 * 1024_i64.pow(3), true).await;
        insert_test_uptime(&db, "srv-expired", Utc::now().date_naive(), 1440, 1440).await;

        let insights = CostService::server_insights(&db, &test_agent_manager(), "srv-expired")
            .await
            .expect("server insights should succeed");
        let expected_burn =
            CostService::compute_burn(31.0, "monthly", None, Utc::now().date_naive()).unwrap();

        assert_eq!(
            insights.cycle_cost_elapsed,
            Some(expected_burn.cycle_cost_elapsed)
        );
        assert!(insights.advisories.contains(&CostAdvisory::ExpiredBilling));
    }

    #[tokio::test]
    async fn overview_uses_batch_inputs_for_multiple_servers() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        let today = Utc::now().date_naive();
        for (id, name, price, cpu) in [
            ("srv-a", "Batch A", 10.0, 25.0),
            ("srv-b", "Batch B", 20.0, 45.0),
            ("srv-c", "Batch C", 30.0, 65.0),
        ] {
            insert_test_server(
                &db,
                id,
                name,
                Some(price),
                Some("monthly"),
                Some("USD"),
                None,
            )
            .await;
            insert_test_record(&db, id, cpu, 4 * 1024_i64.pow(3), true).await;
            insert_test_uptime(&db, id, today, 1440, 1440).await;
        }

        let overview = CostService::overview(&db, &test_agent_manager())
            .await
            .expect("overview should succeed");

        assert_eq!(overview.servers.len(), 3);
        assert!(
            overview
                .servers
                .iter()
                .all(|entry| entry.advisories.is_empty()),
            "healthy, well-utilized servers with full uptime should raise no advisories"
        );
    }

    #[tokio::test]
    async fn zero_valued_non_empty_disk_io_json_still_counts_as_idle() {
        let (db, _tmp) = crate::test_utils::setup_test_db().await;
        insert_test_server(
            &db,
            "srv-idle",
            "Idle",
            Some(100.0),
            Some("monthly"),
            Some("USD"),
            None,
        )
        .await;
        insert_test_record_with_disk_io_json(
            &db,
            "srv-idle",
            1.0,
            512 * 1024_i64.pow(2),
            false,
            Some(r#"[{"name":"vda","read_bytes_per_sec":0,"write_bytes_per_sec":0}]"#.to_string()),
        )
        .await;

        let insights = CostService::server_insights(&db, &test_agent_manager(), "srv-idle")
            .await
            .expect("server insights should succeed");

        assert!(insights.advisories.contains(&CostAdvisory::IdleBurn));
    }

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

    #[test]
    fn cost_config_rejects_negative_price() {
        assert_eq!(
            CostService::normalize_config(Some(-0.01), Some("monthly"), Some("USD")).invalid_reason,
            Some(CostInvalidReason::InvalidPrice)
        );
    }

    #[test]
    fn cost_config_rejects_non_finite_price() {
        for price in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                CostService::normalize_config(Some(price), Some("monthly"), Some("USD"))
                    .invalid_reason,
                Some(CostInvalidReason::InvalidPrice)
            );
        }
    }

    #[test]
    fn monthly_cost_uses_real_cycle_days() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
        let burn = CostService::compute_burn(31.0, "monthly", None, today).unwrap();

        assert_eq!(burn.cycle_days, 31);
        assert_eq!(burn.days_elapsed, 5);
        assert_near(burn.cost_per_day, 1.0);
        assert_near(burn.cycle_cost_elapsed, 5.0);
    }

    #[test]
    fn zero_price_has_no_burn_percent() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();
        let burn = CostService::compute_burn(0.0, "monthly", None, today).unwrap();

        assert_eq!(burn.cost_per_second, 0.0);
        assert_eq!(burn.cost_per_hour, 0.0);
        assert_eq!(burn.cost_per_day, 0.0);
        assert_eq!(burn.cycle_cost_elapsed, 0.0);
        assert_eq!(burn.cycle_cost_remaining, 0.0);
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
        assert_eq!(values.traffic_limit_type.as_deref(), Some("sum"));
    }

    #[test]
    fn invalid_month_equivalent_cost_returns_no_resource_costs() {
        for cost in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            let values = CostService::compute_resource_value(
                cost,
                Some(2),
                Some(8 * 1024_i64.pow(3)),
                Some(80 * 1024_i64.pow(3)),
                Some(1024_i64.pow(4)),
                Some("sum"),
            );

            assert_eq!(values.cost_per_cpu_core, None);
            assert_eq!(values.cost_per_gb_memory, None);
            assert_eq!(values.cost_per_gb_disk, None);
            assert_eq!(values.cost_per_tb_traffic_limit, None);
            assert_eq!(values.traffic_limit_type.as_deref(), Some("sum"));
        }
    }

    #[test]
    fn yearly_leap_year_uses_366_days() {
        let today = chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        let burn = CostService::compute_burn(366.0, "yearly", Some(1), today).unwrap();

        assert_eq!(burn.cycle_days, 366);
        assert_near(burn.cost_per_day, 1.0);
    }

    #[test]
    fn invalid_billing_cycle_does_not_fall_through_to_monthly() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap();

        assert_eq!(
            CostService::compute_burn(31.0, "weekly", None, today).unwrap_err(),
            CostInvalidReason::InvalidBillingCycle
        );
    }

    #[test]
    fn invalid_traffic_limit_type_returns_none_and_preserves_raw_type() {
        let values = CostService::compute_resource_value(
            5.0,
            Some(2),
            Some(8 * 1024_i64.pow(3)),
            Some(80 * 1024_i64.pow(3)),
            Some(1024_i64.pow(4)),
            Some("sideways"),
        );

        assert_eq!(values.cost_per_tb_traffic_limit, None);
        assert_eq!(values.traffic_limit_type.as_deref(), Some("sideways"));
    }

    #[test]
    fn non_positive_resource_values_return_none() {
        let values =
            CostService::compute_resource_value(5.0, Some(0), Some(0), Some(-1), Some(0), None);

        assert_eq!(values.cost_per_cpu_core, None);
        assert_eq!(values.cost_per_gb_memory, None);
        assert_eq!(values.cost_per_gb_disk, None);
        assert_eq!(values.cost_per_tb_traffic_limit, None);
        assert_eq!(values.traffic_limit_type, None);
    }

    #[test]
    fn missing_traffic_limit_type_still_computes_limit_value() {
        let values =
            CostService::compute_resource_value(5.0, None, None, None, Some(1024_i64.pow(4)), None);

        assert_eq!(values.cost_per_tb_traffic_limit, Some(5.0));
        assert_eq!(values.traffic_limit_type, None);
    }

    // -----------------------------------------------------------------------
    // Advisory rules
    // -----------------------------------------------------------------------

    #[test]
    fn advisory_expired_billing_when_date_is_past() {
        let advisories = super::advisories_from(true, true, true, false, Some(1.0), true);
        assert!(advisories.contains(&CostAdvisory::ExpiredBilling));
    }

    #[test]
    fn advisory_sleeping_money_when_offline_paying_without_samples() {
        let advisories = super::advisories_from(true, false, false, false, None, false);
        assert_eq!(advisories, vec![CostAdvisory::SleepingMoney]);
    }

    #[test]
    fn advisory_no_sleeping_money_for_free_server() {
        let advisories = super::advisories_from(false, false, false, false, None, false);
        assert!(advisories.is_empty());
    }

    #[test]
    fn advisory_idle_burn_when_paying_and_low_utilization() {
        let advisories = super::advisories_from(true, true, true, true, Some(1.0), false);
        assert!(advisories.contains(&CostAdvisory::IdleBurn));
    }

    #[test]
    fn advisory_no_idle_burn_for_free_server() {
        let advisories = super::advisories_from(false, true, true, true, Some(1.0), false);
        assert!(!advisories.contains(&CostAdvisory::IdleBurn));
    }

    #[test]
    fn advisory_sleeping_and_idle_are_mutually_exclusive() {
        // Offline but with recent low-usage samples → idle, not sleeping.
        let advisories = super::advisories_from(true, false, true, true, Some(1.0), false);
        assert!(!advisories.contains(&CostAdvisory::SleepingMoney));
        assert!(advisories.contains(&CostAdvisory::IdleBurn));
    }

    #[test]
    fn advisory_low_uptime_threshold_is_exclusive_at_ninety_percent() {
        assert!(
            super::advisories_from(true, true, true, false, Some(0.89), false)
                .contains(&CostAdvisory::LowUptime)
        );
        assert!(
            !super::advisories_from(true, true, true, false, Some(0.90), false)
                .contains(&CostAdvisory::LowUptime)
        );
        assert!(
            !super::advisories_from(true, true, true, false, None, false)
                .contains(&CostAdvisory::LowUptime)
        );
    }

    #[test]
    fn advisories_are_emitted_in_priority_order() {
        // Expired + offline + no samples + weak uptime.
        let advisories = super::advisories_from(true, false, false, false, Some(0.5), true);
        assert_eq!(
            advisories,
            vec![
                CostAdvisory::ExpiredBilling,
                CostAdvisory::SleepingMoney,
                CostAdvisory::LowUptime
            ]
        );
    }

    #[test]
    fn is_low_utilization_requires_idle_cpu_memory_and_quiet_io() {
        let idle = UtilizationStats {
            avg_cpu: Some(2.0),
            avg_memory_percent: Some(10.0),
            has_network_activity: false,
            has_disk_io_activity: false,
        };
        assert!(super::is_low_utilization(&idle));

        let busy_cpu = UtilizationStats {
            avg_cpu: Some(40.0),
            ..idle.clone()
        };
        assert!(!super::is_low_utilization(&busy_cpu));

        let noisy_network = UtilizationStats {
            has_network_activity: true,
            ..idle.clone()
        };
        assert!(!super::is_low_utilization(&noisy_network));

        let no_samples = UtilizationStats {
            avg_cpu: None,
            avg_memory_percent: None,
            has_network_activity: false,
            has_disk_io_activity: false,
        };
        assert!(!super::is_low_utilization(&no_samples));
    }
}
