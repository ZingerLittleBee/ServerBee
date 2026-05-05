use chrono::NaiveDate;
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

fn bytes_to_gib(bytes: i64) -> f64 {
    bytes as f64 / 1024_f64.powi(3)
}

fn bytes_to_tib(bytes: i64) -> f64 {
    bytes as f64 / 1024_f64.powi(4)
}

#[cfg(test)]
mod tests {
    use crate::service::cost::{CostInvalidReason, CostService};

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
}
