//! Server-owned billing calendar contract. Clients submit dates, never UTC boundaries.
use crate::{entity::server, error::AppError};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenewalState {
    #[serde(default)]
    pub enabled: bool,
    pub billing_timezone: String,
    pub confirmed_expired_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeadlineOrigin {
    Confirmed,
    Projected,
    Frozen,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RenewalProjection {
    pub enabled: bool,
    pub billing_timezone: String,
    pub expiry_date: Option<String>,
    pub confirmed_expired_at: Option<DateTime<Utc>>,
    pub deadline_origin: DeadlineOrigin,
    pub occurrence_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RenewalInput {
    pub enabled: Option<bool>,
    #[serde(
        default,
        deserialize_with = "super::server::deserialize_optional_nullable"
    )]
    pub billing_timezone: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "super::server::deserialize_optional_nullable"
    )]
    pub expiry_date: Option<Option<String>>,
}

impl RenewalState {
    pub fn from_server(model: &server::Model) -> Self {
        model
            .renewal_state
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| Self {
                enabled: false,
                billing_timezone: "UTC".into(),
                confirmed_expired_at: model.expired_at,
            })
    }
    pub fn projection(&self, deadline: Option<DateTime<Utc>>) -> RenewalProjection {
        RenewalProjection {
            enabled: self.enabled,
            billing_timezone: self.billing_timezone.clone(),
            expiry_date: selected_date(deadline, &self.billing_timezone).map(|d| d.to_string()),
            confirmed_expired_at: self.confirmed_expired_at,
            deadline_origin: DeadlineOrigin::Confirmed,
            occurrence_id: None,
        }
    }
}

pub fn selected_date(deadline: Option<DateTime<Utc>>, timezone: &str) -> Option<NaiveDate> {
    let tz: Tz = timezone.parse().ok()?;
    deadline.map(|instant| instant.with_timezone(&tz).date_naive())
}

pub fn date_boundary(date: NaiveDate, timezone: &str) -> Result<DateTime<Utc>, AppError> {
    let tz: Tz = timezone.parse().map_err(|_| {
        AppError::Validation("billing_timezone must be a valid IANA timezone".into())
    })?;
    let start = date.and_hms_opt(0, 0, 0).unwrap();
    if !(0..1440).any(|minute| {
        tz.from_local_datetime(&(start + Duration::minutes(minute)))
            .earliest()
            .is_some()
    }) {
        return Err(AppError::Validation(
            "expiry_date does not exist in billing_timezone".into(),
        ));
    }
    let next = date
        .succ_opt()
        .ok_or_else(|| AppError::Validation("expiry_date is out of range".into()))?;
    // A few IANA zones transition at midnight. The boundary is the first instant
    // of the following local date, choosing the earlier occurrence on overlap.
    let midnight = next.and_hms_opt(0, 0, 0).unwrap();
    for minutes in 0..=1440 {
        if let Some(boundary) = tz
            .from_local_datetime(&(midnight + Duration::minutes(minutes)))
            .earliest()
        {
            return Ok(boundary.with_timezone(&Utc) - Duration::nanoseconds(1));
        }
    }
    Err(AppError::Validation(
        "expiry_date has no valid local boundary".into(),
    ))
}

pub fn apply_edit(
    model: &server::Model,
    input: Option<&RenewalInput>,
    legacy: Option<Option<DateTime<Utc>>>,
    billing_cycle: Option<&str>,
) -> Result<(RenewalState, Option<DateTime<Utc>>), AppError> {
    let result = apply_calendar(
        RenewalState::from_server(model),
        model.expired_at,
        input,
        legacy,
    )?;
    validate_enabled(&result.0, result.1, billing_cycle)?;
    Ok(result)
}

pub fn apply_initial(
    input: Option<&RenewalInput>,
    legacy: Option<DateTime<Utc>>,
    billing_cycle: Option<&str>,
) -> Result<(RenewalState, Option<DateTime<Utc>>), AppError> {
    if input.is_some_and(|input| input.expiry_date.is_some()) && legacy.is_some() {
        return Err(AppError::Validation(
            "expiry_date and expired_at cannot be submitted together".into(),
        ));
    }
    // Preserve old create payload instants. Calendar input uses the new contract.
    let result = apply_calendar(
        RenewalState {
            enabled: false,
                billing_timezone: "UTC".into(),
            confirmed_expired_at: legacy,
        },
        legacy,
        input,
        None,
    )?;
    validate_enabled(&result.0, result.1, billing_cycle)?;
    Ok(result)
}

fn validate_enabled(state: &RenewalState, deadline: Option<DateTime<Utc>>, cycle: Option<&str>) -> Result<(), AppError> {
    if state.enabled && (deadline.is_none() || !matches!(cycle, Some("monthly" | "quarterly" | "yearly"))) {
        return Err(AppError::Validation("automatic renewal requires an expiry date and monthly, quarterly or yearly billing_cycle".into()));
    }
    Ok(())
}

fn apply_calendar(
    mut state: RenewalState,
    original: Option<DateTime<Utc>>,
    input: Option<&RenewalInput>,
    legacy: Option<Option<DateTime<Utc>>>,
) -> Result<(RenewalState, Option<DateTime<Utc>>), AppError> {
    let old_date = selected_date(original, &state.billing_timezone);
    let mut deadline = original;
    let mut timezone_changed = false;
    let mut date_replaced = false;
    let timezone = input.and_then(|i| i.billing_timezone.as_ref());
    if let Some(zone) = timezone {
        let zone = zone.as_deref().unwrap_or("UTC");
        zone.parse::<Tz>().map_err(|_| {
            AppError::Validation("billing_timezone must be a valid IANA timezone".into())
        })?;
        if zone != state.billing_timezone {
            state.billing_timezone = zone.into();
            timezone_changed = true;
        }
    }
    let date = input.and_then(|i| i.expiry_date.as_ref());
    if date.is_some() && legacy.is_some() {
        return Err(AppError::Validation(
            "expiry_date and expired_at cannot be submitted together".into(),
        ));
    }
    if let Some(value) = date {
        let date = value
            .as_deref()
            .map(|s| {
                let date = NaiveDate::parse_from_str(s, "%Y-%m-%d")
                    .map_err(|_| AppError::Validation("expiry_date must be YYYY-MM-DD".into()))?;
                if date.to_string() != s {
                    return Err(AppError::Validation(
                        "expiry_date must be YYYY-MM-DD".into(),
                    ));
                }
                Ok(date)
            })
            .transpose()?;
        if date != old_date {
            date_replaced = true;
            deadline = date
                .map(|d| date_boundary(d, &state.billing_timezone))
                .transpose()?;
            state.confirmed_expired_at = deadline;
        }
    } else if let Some(value) = legacy {
        // Existing iOS repeats the original instant; old Web reconstructs UTC
        // midnight from its date prefix. Neither is an operator date edit.
        let unchanged = value == original
            || matches!((value, original), (Some(new), Some(old)) if new.date_naive() == old.date_naive() && new.time() == chrono::NaiveTime::MIN);
        if !unchanged {
            date_replaced = true;
            let date = value.and_then(|instant| {
                if instant.time() == chrono::NaiveTime::MIN {
                    Some(instant.date_naive())
                } else {
                    selected_date(Some(instant), &state.billing_timezone)
                }
            });
            deadline = date
                .map(|d| date_boundary(d, &state.billing_timezone))
                .transpose()?;
            state.confirmed_expired_at = deadline;
        }
    }
    if timezone_changed && !date_replaced {
        deadline = old_date
            .map(|d| date_boundary(d, &state.billing_timezone))
            .transpose()?;
    }
    if let Some(enabled) = input.and_then(|i| i.enabled) {
        state.enabled = enabled;
    }
    Ok((state, deadline))
}
