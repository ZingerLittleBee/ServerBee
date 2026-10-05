//! Server-owned billing calendar contract. Clients submit dates, never UTC boundaries.
use crate::{entity::server, error::AppError};
use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, DatabaseTransaction, EntityTrait,
    QueryFilter, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RenewalState {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub deadline_origin: DeadlineOrigin,
    #[serde(default)]
    pub anchor_day: Option<u32>,
    #[serde(default)]
    pub occurrence_id: Option<String>,
    pub billing_timezone: String,
    pub confirmed_expired_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeadlineOrigin {
    #[default]
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
                deadline_origin: DeadlineOrigin::Confirmed,
                anchor_day: None,
                occurrence_id: None,
                billing_timezone: "UTC".into(),
                confirmed_expired_at: model.expired_at,
            })
    }
    /// Catch up directly to the first unexpired occurrence, retaining the original day anchor.
    /// No historical occurrence is admitted or represented as a payment.
    pub fn advance(
        &mut self,
        deadline: &mut Option<DateTime<Utc>>,
        cycle: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<bool, AppError> {
        if !self.enabled {
            return Ok(false);
        }
        validate_enabled(self, *deadline, cycle)?;
        let original = deadline.expect("enabled configuration was validated");
        let date = selected_date(Some(original), &self.billing_timezone)
            .ok_or_else(|| AppError::Validation("invalid billing timezone".into()))?;
        let anchor = *self.anchor_day.get_or_insert(date.day());
        if self.occurrence_id.is_none() {
            self.occurrence_id = Some(uuid::Uuid::new_v4().to_string());
        }
        if original >= now {
            return Ok(false);
        }
        let interval: i32 = match cycle {
            Some("monthly") => 1,
            Some("quarterly") => 3,
            Some("yearly") => 12,
            _ => unreachable!(),
        };
        let today = selected_date(Some(now), &self.billing_timezone).expect("validated timezone");
        let difference =
            (today.year() - date.year()) * 12 + today.month() as i32 - date.month() as i32;
        let mut periods = (difference / interval).max(1);
        loop {
            let month_index = date
                .year()
                .checked_mul(12)
                .and_then(|year| year.checked_add(date.month0() as i32))
                .and_then(|month| month.checked_add(periods.checked_mul(interval)?))
                .ok_or_else(|| AppError::Validation("renewal date is out of range".into()))?;
            let year = month_index.div_euclid(12);
            let month = month_index.rem_euclid(12) as u32 + 1;
            let mut day = anchor;
            let candidate = loop {
                if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                    break date;
                }
                day = day
                    .checked_sub(1)
                    .filter(|day| *day > 0)
                    .ok_or_else(|| AppError::Validation("renewal date is out of range".into()))?;
            };
            // A timezone may skip a whole anchored date. It cannot represent an
            // occurrence; retain the anchor and examine the following period.
            let Some(boundary) = representable_date_boundary(candidate, &self.billing_timezone)?
            else {
                periods += 1;
                continue;
            };
            if boundary >= now {
                *deadline = Some(boundary);
                self.deadline_origin = DeadlineOrigin::Projected;
                self.occurrence_id = Some(uuid::Uuid::new_v4().to_string());
                return Ok(true);
            }
            periods += 1;
        }
    }
    pub fn projection(&self, deadline: Option<DateTime<Utc>>) -> RenewalProjection {
        RenewalProjection {
            enabled: self.enabled,
            billing_timezone: self.billing_timezone.clone(),
            expiry_date: selected_date(deadline, &self.billing_timezone).map(|d| d.to_string()),
            confirmed_expired_at: self.confirmed_expired_at,
            deadline_origin: self.deadline_origin.clone(),
            occurrence_id: self.occurrence_id.clone(),
        }
    }
}

pub fn selected_date(deadline: Option<DateTime<Utc>>, timezone: &str) -> Option<NaiveDate> {
    let tz: Tz = timezone.parse().ok()?;
    deadline.map(|instant| instant.with_timezone(&tz).date_naive())
}

pub fn date_boundary(date: NaiveDate, timezone: &str) -> Result<DateTime<Utc>, AppError> {
    representable_date_boundary(date, timezone)?.ok_or_else(|| {
        AppError::Validation("expiry_date does not exist in billing_timezone".into())
    })
}

fn representable_date_boundary(
    date: NaiveDate,
    timezone: &str,
) -> Result<Option<DateTime<Utc>>, AppError> {
    let tz: Tz = timezone.parse().map_err(|_| {
        AppError::Validation("billing_timezone must be a valid IANA timezone".into())
    })?;
    let start = date.and_hms_opt(0, 0, 0).unwrap();
    if !(0..1440).any(|minute| {
        tz.from_local_datetime(&(start + Duration::minutes(minute)))
            .earliest()
            .is_some()
    }) {
        return Ok(None);
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
            return Ok(Some(
                boundary.with_timezone(&Utc) - Duration::nanoseconds(1),
            ));
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
    now: DateTime<Utc>,
) -> Result<(RenewalState, Option<DateTime<Utc>>), AppError> {
    let previous = RenewalState::from_server(model);
    let enabling = !previous.enabled && input.and_then(|i| i.enabled) == Some(true);
    let result = apply_calendar(previous, model.expired_at, input, legacy)?;
    validate_enabled(&result.0, result.1, billing_cycle)?;
    let (mut state, mut deadline) = result;
    if enabling {
        deadline = selected_date(deadline, &state.billing_timezone)
            .map(|date| date_boundary(date, &state.billing_timezone))
            .transpose()?;
    }
    state.advance(&mut deadline, billing_cycle, now)?;
    Ok((state, deadline))
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
            deadline_origin: DeadlineOrigin::Confirmed,
            anchor_day: None,
            occurrence_id: None,
            billing_timezone: "UTC".into(),
            confirmed_expired_at: legacy,
        },
        legacy,
        input,
        None,
    )?;
    validate_enabled(&result.0, result.1, billing_cycle)?;
    let (mut state, mut deadline) = result;
    if state.enabled {
        deadline = selected_date(deadline, &state.billing_timezone)
            .map(|date| date_boundary(date, &state.billing_timezone))
            .transpose()?;
    }
    state.occurrence_id = None;
    Ok((state, deadline))
}

fn validate_enabled(
    state: &RenewalState,
    deadline: Option<DateTime<Utc>>,
    cycle: Option<&str>,
) -> Result<(), AppError> {
    if state.enabled
        && (deadline.is_none() || !matches!(cycle, Some("monthly" | "quarterly" | "yearly")))
    {
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
    let resulting_enabled = input.and_then(|i| i.enabled).unwrap_or(state.enabled);
    if resulting_enabled && input.is_some_and(|i| matches!(i.billing_timezone, Some(None))) {
        return Err(AppError::Validation(
            "automatic renewal requires a billing timezone".into(),
        ));
    }
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
    if date_replaced {
        state.deadline_origin = DeadlineOrigin::Confirmed;
        state.anchor_day = selected_date(deadline, &state.billing_timezone).map(|d| d.day());
        state.occurrence_id = deadline.map(|_| uuid::Uuid::new_v4().to_string());
    }
    if let Some(enabled) = input.and_then(|i| i.enabled) {
        state.enabled = enabled;
    }
    if !state.enabled && state.deadline_origin == DeadlineOrigin::Projected {
        state.deadline_origin = DeadlineOrigin::Frozen;
    }
    if state.enabled {
        state.deadline_origin = DeadlineOrigin::Projected;
        if state.anchor_day.is_none() {
            state.anchor_day = selected_date(deadline, &state.billing_timezone).map(|d| d.day());
        }
        if state.occurrence_id.is_none() {
            state.occurrence_id = deadline.map(|_| uuid::Uuid::new_v4().to_string());
        }
    }
    Ok((state, deadline))
}

/// Reserve the SQLite writer before reading a schedule. CRUD, advancement and
/// reminder admission can share this transaction and cannot overwrite a newer snapshot.
pub async fn load_for_update(
    tx: &DatabaseTransaction,
    id: &str,
) -> Result<server::Model, AppError> {
    server::Entity::update_many()
        .col_expr(
            server::Column::ExpiredAt,
            sea_orm::prelude::Expr::col(server::Column::ExpiredAt).into(),
        )
        .filter(server::Column::Id.eq(id))
        .exec(tx)
        .await?;
    server::Entity::find_by_id(id)
        .one(tx)
        .await?
        .ok_or_else(|| AppError::NotFound("Server not found".into()))
}

/// Production catch-up evaluates each server from a stable, freshly locked schedule.
pub(super) async fn advance_locked(
    tx: &DatabaseTransaction,
    model: server::Model,
    now: DateTime<Utc>,
) -> Result<(server::Model, bool), AppError> {
    let mut state = RenewalState::from_server(&model);
    let mut deadline = model.expired_at;
    if !state.advance(&mut deadline, model.billing_cycle.as_deref(), now)? {
        return Ok((model, false));
    }
    let mut active: server::ActiveModel = model.into();
    active.expired_at = Set(deadline);
    active.renewal_state = Set(Some(
        serde_json::to_string(&state).map_err(|e| AppError::Internal(e.to_string()))?,
    ));
    active.updated_at = Set(now);
    Ok((active.update(tx).await?, true))
}

pub async fn advance_all(
    db: &DatabaseConnection,
    now: DateTime<Utc>,
) -> Result<Vec<String>, AppError> {
    let servers = server::Entity::find().all(db).await?;
    let mut changed = Vec::new();
    for candidate in servers {
        if !RenewalState::from_server(&candidate).enabled {
            continue;
        }
        let tx = db.begin().await?;
        let model = match load_for_update(&tx, &candidate.id).await {
            Ok(model) => model,
            Err(AppError::NotFound(_)) => {
                tx.rollback().await?;
                continue;
            }
            Err(error) => return Err(error),
        };
        let advanced = match advance_locked(&tx, model, now).await {
            Ok((_, advanced)) => advanced,
            Err(AppError::Validation(reason)) => {
                // A damaged calendar configuration cannot stop healthy schedules.
                // Database/transaction errors still propagate to the caller.
                tracing::error!(server_id = %candidate.id, %reason, "Skipping invalid renewal schedule");
                tx.rollback().await?;
                continue;
            }
            Err(error) => return Err(error),
        };
        tx.commit().await?;
        if advanced {
            changed.push(candidate.id);
        }
    }
    Ok(changed)
}
