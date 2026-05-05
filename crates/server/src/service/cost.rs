use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

use crate::service::traffic;

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

#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct ValueScore {
    pub score: f64,
    pub grade: ValueGrade,
    pub reasons: Vec<ValueReason>,
    pub confidence: ValueConfidence,
}

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

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub(crate) struct UtilizationStats {
    avg_cpu: Option<f64>,
    avg_memory_percent: Option<f64>,
    has_network_activity: bool,
    has_disk_io_activity: bool,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub(crate) struct NormalizedCostConfig {
    pub configured: bool,
    pub invalid_reason: Option<CostInvalidReason>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub billing_cycle: Option<String>,
}

pub struct CostService;

impl CostService {
    #[allow(dead_code)]
    pub(crate) fn grade_for_score(score: f64) -> ValueGrade {
        if score >= 90.0 {
            ValueGrade::Excellent
        } else if score >= 75.0 {
            ValueGrade::Good
        } else if score >= 60.0 {
            ValueGrade::Okay
        } else if score >= 40.0 {
            ValueGrade::Poor
        } else {
            ValueGrade::Waste
        }
    }

    #[allow(dead_code)]
    pub(crate) fn prioritize_reasons(reasons: Vec<ValueReason>) -> Vec<ValueReason> {
        let priority = [
            ValueReason::SleepingMoney,
            ValueReason::IdleBurn,
            ValueReason::ExpensiveCpu,
            ValueReason::LowUptime,
            ValueReason::InsufficientData,
            ValueReason::ExpiredBilling,
            ValueReason::FreeOrZeroPrice,
            ValueReason::HealthyUptime,
            ValueReason::GoodMemoryValue,
            ValueReason::GoodDiskValue,
        ];
        let mut prioritized = Vec::new();

        for reason in priority {
            if reasons.contains(&reason) {
                prioritized.push(reason);
                if prioritized.len() == 3 {
                    return prioritized;
                }
            }
        }

        for reason in reasons {
            if !prioritized.contains(&reason) {
                prioritized.push(reason);
                if prioritized.len() == 3 {
                    break;
                }
            }
        }

        prioritized
    }

    #[allow(dead_code)]
    pub(crate) fn resource_percentile_score(value: f64, comparable_values: &[f64]) -> f64 {
        let mut values = finite_positive_values(comparable_values);
        if values.len() < 2 || !is_finite_positive(value) {
            return 0.5;
        }

        values.sort_by(f64::total_cmp);
        let lowest = values[0];
        let highest = values[values.len() - 1];
        if lowest == highest {
            return 0.5;
        }

        let lower_count = values
            .iter()
            .filter(|candidate| **candidate < value)
            .count();
        let equal_count = values
            .iter()
            .filter(|candidate| **candidate == value)
            .count();
        let rank = if equal_count == 0 {
            lower_count as f64
        } else {
            lower_count as f64 + (equal_count.saturating_sub(1) as f64 / 2.0)
        };

        (1.0 - (rank / (values.len() - 1) as f64)).clamp(0.0, 1.0)
    }

    #[allow(dead_code)]
    pub(crate) fn compute_utilization_score(
        stats: &UtilizationStats,
        monthly_cost: f64,
        fleet_monthly_costs: &[f64],
    ) -> (f64, Vec<ValueReason>, ValueConfidence) {
        let mut reasons = Vec::new();
        let confidence = utilization_confidence(stats);

        if confidence != ValueConfidence::High {
            reasons.push(ValueReason::InsufficientData);
        }

        let low_utilization = is_low_utilization(stats);
        let high_monthly_cost = is_high_monthly_cost(monthly_cost, fleet_monthly_costs);
        let over_utilized = stats.avg_cpu.is_some_and(|value| value > 85.0)
            || stats.avg_memory_percent.is_some_and(|value| value > 90.0);

        let score = if low_utilization && high_monthly_cost {
            reasons.push(ValueReason::IdleBurn);
            7.0
        } else if over_utilized {
            35.0 * 0.7
        } else if is_moderate_stable_utilization(stats) {
            31.5
        } else if confidence == ValueConfidence::Low {
            17.5
        } else {
            22.0
        };

        (score, Self::prioritize_reasons(reasons), confidence)
    }

    #[allow(dead_code)]
    pub(crate) fn compute_reliability_score(
        uptime_ratio: Option<f64>,
        online: bool,
        expired_at: Option<DateTime<Utc>>,
    ) -> (f64, Vec<ValueReason>, ValueConfidence) {
        let mut reasons = Vec::new();
        let expired = expired_at.is_some_and(|value| value < Utc::now());
        if expired {
            reasons.push(ValueReason::ExpiredBilling);
        }

        let (score, confidence) = match uptime_ratio.filter(|value| value.is_finite()) {
            Some(value) => {
                let uptime = value.clamp(0.0, 1.0);
                if uptime >= 0.99 && !expired {
                    reasons.push(ValueReason::HealthyUptime);
                } else if uptime < 0.90 {
                    reasons.push(ValueReason::LowUptime);
                }
                (uptime * 25.0, ValueConfidence::High)
            }
            None if online => {
                reasons.push(ValueReason::InsufficientData);
                (18.75, ValueConfidence::Low)
            }
            None => {
                reasons.push(ValueReason::SleepingMoney);
                reasons.push(ValueReason::InsufficientData);
                (0.0, ValueConfidence::Low)
            }
        };

        (score, Self::prioritize_reasons(reasons), confidence)
    }

    #[allow(dead_code)]
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

#[allow(dead_code)]
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

fn finite_positive_values(values: &[f64]) -> Vec<f64> {
    values
        .iter()
        .copied()
        .filter(|value| is_finite_positive(*value))
        .collect()
}

fn is_finite_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn utilization_confidence(stats: &UtilizationStats) -> ValueConfidence {
    match (
        stats.avg_cpu.filter(|value| value.is_finite()),
        stats.avg_memory_percent.filter(|value| value.is_finite()),
    ) {
        (Some(_), Some(_)) => ValueConfidence::High,
        (Some(_), None) | (None, Some(_)) => ValueConfidence::Medium,
        (None, None) => ValueConfidence::Low,
    }
}

fn is_low_utilization(stats: &UtilizationStats) -> bool {
    let cpu_is_low = stats
        .avg_cpu
        .is_some_and(|value| value.is_finite() && value < 5.0);
    let memory_is_low = stats
        .avg_memory_percent
        .is_some_and(|value| value.is_finite() && value < 20.0);
    let io_is_quiet = !stats.has_network_activity && !stats.has_disk_io_activity;

    cpu_is_low && memory_is_low && io_is_quiet
}

fn is_moderate_stable_utilization(stats: &UtilizationStats) -> bool {
    let cpu_is_moderate = stats
        .avg_cpu
        .is_some_and(|value| value.is_finite() && (10.0..=70.0).contains(&value));
    let memory_is_moderate = stats
        .avg_memory_percent
        .is_some_and(|value| value.is_finite() && (30.0..=80.0).contains(&value));
    let has_activity = stats.has_network_activity || stats.has_disk_io_activity;

    has_activity && (cpu_is_moderate || memory_is_moderate)
}

fn is_high_monthly_cost(monthly_cost: f64, fleet_monthly_costs: &[f64]) -> bool {
    if !is_finite_positive(monthly_cost) {
        return false;
    }

    let mut costs = finite_positive_values(fleet_monthly_costs);
    if costs.is_empty() {
        return false;
    }

    costs.sort_by(f64::total_cmp);
    let threshold = if costs.len() < 4 {
        median(&costs)
    } else {
        percentile_nearest_rank(&costs, 0.75)
    };

    monthly_cost >= threshold
}

fn median(sorted_values: &[f64]) -> f64 {
    let middle = sorted_values.len() / 2;
    if sorted_values.len() % 2 == 0 {
        (sorted_values[middle - 1] + sorted_values[middle]) / 2.0
    } else {
        sorted_values[middle]
    }
}

fn percentile_nearest_rank(sorted_values: &[f64], percentile: f64) -> f64 {
    let rank = (percentile.clamp(0.0, 1.0) * sorted_values.len() as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted_values.len() - 1);
    sorted_values[index]
}

fn bytes_to_gib(bytes: i64) -> f64 {
    bytes as f64 / 1024_f64.powi(3)
}

fn bytes_to_tib(bytes: i64) -> f64 {
    bytes as f64 / 1024_f64.powi(4)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use crate::service::cost::{
        CostInvalidReason, CostService, UtilizationStats, ValueConfidence, ValueGrade, ValueReason,
    };

    fn assert_near(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {actual} to be within 0.0001 of {expected}"
        );
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
            ValueReason::IdleBurn,
        ]);

        assert_eq!(
            reasons,
            vec![
                ValueReason::SleepingMoney,
                ValueReason::IdleBurn,
                ValueReason::ExpensiveCpu
            ]
        );
    }

    #[test]
    fn single_server_resource_metric_uses_neutral_score() {
        let score = CostService::resource_percentile_score(5.0, &[5.0]);

        assert_eq!(score, 0.5);
    }

    #[test]
    fn lower_unit_cost_gets_higher_percentile_score_than_higher_unit_cost() {
        let comparable_values = [2.0, 5.0, 10.0, 20.0];

        let cheap_score = CostService::resource_percentile_score(2.0, &comparable_values);
        let expensive_score = CostService::resource_percentile_score(20.0, &comparable_values);

        assert!(cheap_score > expensive_score);
    }

    #[test]
    fn invalid_comparable_values_do_not_produce_non_finite_score() {
        let score = CostService::resource_percentile_score(
            f64::INFINITY,
            &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0],
        );

        assert!(score.is_finite());
        assert!((0.0..=1.0).contains(&score));
    }

    #[test]
    fn expired_billing_adds_reason_without_lowering_perfect_uptime_score() {
        let expired_at = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();

        let (score, reasons, confidence) =
            CostService::compute_reliability_score(Some(1.0), true, Some(expired_at));

        assert_eq!(score, 25.0);
        assert_eq!(reasons, vec![ValueReason::ExpiredBilling]);
        assert_eq!(confidence, ValueConfidence::High);
    }

    #[test]
    fn low_utilization_with_high_monthly_cost_yields_idle_burn() {
        let stats = UtilizationStats {
            avg_cpu: Some(2.0),
            avg_memory_percent: Some(10.0),
            has_network_activity: false,
            has_disk_io_activity: false,
        };

        let (_score, reasons, _confidence) =
            CostService::compute_utilization_score(&stats, 20.0, &[5.0, 10.0, 20.0]);

        assert!(reasons.contains(&ValueReason::IdleBurn));
    }

    #[test]
    fn over_utilization_caps_score_below_stable_moderate_utilization() {
        let stable = UtilizationStats {
            avg_cpu: Some(35.0),
            avg_memory_percent: Some(50.0),
            has_network_activity: true,
            has_disk_io_activity: false,
        };
        let overloaded = UtilizationStats {
            avg_cpu: Some(90.0),
            avg_memory_percent: Some(70.0),
            has_network_activity: true,
            has_disk_io_activity: true,
        };

        let (stable_score, _stable_reasons, stable_confidence) =
            CostService::compute_utilization_score(&stable, 10.0, &[5.0, 10.0, 20.0]);
        let (overloaded_score, _overloaded_reasons, overloaded_confidence) =
            CostService::compute_utilization_score(&overloaded, 10.0, &[5.0, 10.0, 20.0]);

        assert!(overloaded_score <= 35.0 * 0.7);
        assert!(overloaded_score < stable_score);
        assert_eq!(stable_confidence, ValueConfidence::High);
        assert_eq!(overloaded_confidence, ValueConfidence::High);
    }
}
