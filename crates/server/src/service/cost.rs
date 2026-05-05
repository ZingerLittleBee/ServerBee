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
#[allow(dead_code)]
pub(crate) struct NormalizedCostConfig {
    pub configured: bool,
    pub invalid_reason: Option<CostInvalidReason>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub billing_cycle: Option<String>,
}

pub struct CostService;

#[allow(dead_code)]
impl CostService {
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
        } else if price.is_some_and(|value| value < 0.0) {
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
}

#[allow(dead_code)]
fn normalize_currency(currency: Option<&str>) -> String {
    let currency = currency.map(str::trim).filter(|value| !value.is_empty());
    currency.unwrap_or("USD").to_string()
}

#[cfg(test)]
mod tests {
    use crate::service::cost::{CostInvalidReason, CostService};

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
}
