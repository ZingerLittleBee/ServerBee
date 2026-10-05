//! Durable expiration admission for the current renewal occurrence.
use crate::{
    entity::{alert_rule, alert_state},
    error::AppError,
    service::{
        alert::{AlertRuleItem, AlertStateManager, rule_covers_server},
        maintenance::MaintenanceService,
        mobile_alert_push,
        renewal::{self, DeadlineOrigin, RenewalState},
    },
};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};

/// `None` leaves legacy records without an occurrence on their existing alert path.
/// The writer reservation protects the selected deadline through queue admission.
pub(super) enum Outcome {
    Silent,
    Triggered,
    Resolved,
    Superseded,
}

pub(super) async fn evaluate(
    db: &DatabaseConnection,
    manager: &AlertStateManager,
    rule: &alert_rule::Model,
    server_id: &str,
    items: &[AlertRuleItem],
    other_conditions: bool,
    now: DateTime<Utc>,
) -> Result<Option<Outcome>, AppError> {
    let _admission = manager.admission_lock.lock().await;
    let tx = db.begin().await?;
    let model = renewal::load_for_update(&tx, server_id).await?;
    let (model, _) = renewal::advance_locked(&tx, model, now).await?;
    let renewal = RenewalState::from_server(&model);
    let Some(rule) = alert_rule::Entity::find_by_id(&rule.id)
        .one(&tx)
        .await?
        .filter(|rule| {
            rule.enabled && rule_covers_server(&rule.cover_type, &rule.server_ids_json, server_id)
        })
    else {
        tx.commit().await?;
        return Ok(Some(Outcome::Silent));
    };
    let Some(occurrence) = renewal.occurrence_id else {
        // Explicitly clearing a typed date recovers its retained occurrence.
        // Legacy rows still use the existing empty-event dimension below.
        let previous = if model.expired_at.is_none() {
            alert_state::Entity::find()
                .filter(alert_state::Column::RuleId.eq(&rule.id))
                .filter(alert_state::Column::ServerId.eq(server_id))
                .filter(alert_state::Column::EventKey.like("renewal:%"))
                .filter(alert_state::Column::Resolved.eq(false))
                .all(&tx)
                .await?
        } else {
            Vec::new()
        };
        let mut recovered = Vec::new();
        for previous in previous {
            let mut active: alert_state::ActiveModel = previous.into();
            active.resolved = Set(true);
            active.resolved_at = Set(Some(now));
            active.updated_at = Set(now);
            let row = active.update(&tx).await?;
            mobile_alert_push::enqueue(&tx, &rule, &row, &model.name).await?;
            recovered.push(row);
        }
        tx.commit().await?;
        for row in &recovered {
            manager.publish_state(row);
        }
        return Ok(if recovered.is_empty() {
            None
        } else {
            Some(Outcome::Resolved)
        });
    };
    let event_key = format!("renewal:{occurrence}");
    let matched = other_conditions
        && items
            .iter()
            .filter(|item| item.rule_type == "expiration")
            .all(|item| {
                renewal::selected_date(model.expired_at, &renewal.billing_timezone)
                    .zip(renewal::selected_date(Some(now), &renewal.billing_timezone))
                    .is_some_and(|(date, today)| {
                        (date - today).num_days() <= i64::from(item.duration.unwrap_or(7))
                    })
            });
    let manual_recovery = renewal.deadline_origin == DeadlineOrigin::Confirmed && !matched;
    let previous = alert_state::Entity::find()
        .filter(alert_state::Column::RuleId.eq(&rule.id))
        .filter(alert_state::Column::ServerId.eq(server_id))
        .filter(alert_state::Column::EventKey.like("renewal:%"))
        .filter(alert_state::Column::EventKey.ne(&event_key))
        .filter(alert_state::Column::Resolved.eq(false))
        .all(&tx)
        .await?;
    let mut archived = Vec::new();
    for previous in previous {
        let mut active: alert_state::ActiveModel = previous.into();
        active.resolved = Set(true);
        // No recovery occurred. Keep the original trigger cycle available for
        // notification detail and mark this target as silently superseded.
        active.resolved_at = Set(manual_recovery.then_some(now));
        active.updated_at = Set(now);
        let row = active.update(&tx).await?;
        if manual_recovery {
            mobile_alert_push::enqueue(&tx, &rule, &row, &model.name).await?;
        }
        archived.push(row);
    }
    let existing = alert_state::Entity::find()
        .filter(alert_state::Column::RuleId.eq(&rule.id))
        .filter(alert_state::Column::ServerId.eq(server_id))
        .filter(alert_state::Column::EventKey.eq(&event_key))
        .one(&tx)
        .await?;
    let mut recovered = manual_recovery && !archived.is_empty();
    if !matched {
        if let Some(current) = existing.as_ref().filter(|state| !state.resolved) {
            let mut active: alert_state::ActiveModel = current.clone().into();
            active.resolved = Set(true);
            active.resolved_at = Set(Some(now));
            active.updated_at = Set(now);
            let row = active.update(&tx).await?;
            mobile_alert_push::enqueue(&tx, &rule, &row, &model.name).await?;
            archived.push(row);
            recovered = true;
        }
    }
    if !matched || MaintenanceService::is_in_maintenance_at(&tx, server_id, now).await? {
        tx.commit().await?;
        for row in &archived {
            manager.publish_state(row);
        }
        return Ok(Some(if recovered {
            Outcome::Resolved
        } else if archived.is_empty() {
            Outcome::Silent
        } else {
            Outcome::Superseded
        }));
    }
    let should_notify = existing.as_ref().is_none_or(|state| {
        rule.trigger_mode != "once" && now - state.last_notified_at >= Duration::minutes(5)
    });
    if !should_notify {
        tx.commit().await?;
        for row in &archived {
            manager.publish_state(row);
        }
        return Ok(Some(if archived.is_empty() {
            Outcome::Silent
        } else {
            Outcome::Superseded
        }));
    }
    let row = if let Some(previous) = existing {
        let count = previous.count;
        let mut active: alert_state::ActiveModel = previous.into();
        active.last_notified_at = Set(now);
        active.count = Set(count.saturating_add(1));
        active.resolved = Set(false);
        active.resolved_at = Set(None);
        active.updated_at = Set(now);
        active.update(&tx).await?
    } else {
        alert_state::ActiveModel {
            rule_id: Set(rule.id.clone()),
            server_id: Set(server_id.into()),
            event_key: Set(event_key),
            first_triggered_at: Set(now),
            last_notified_at: Set(now),
            count: Set(1),
            resolved: Set(false),
            resolved_at: Set(None),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&tx)
        .await?
    };
    mobile_alert_push::enqueue(&tx, &rule, &row, &model.name).await?;
    tx.commit().await?;
    for row in &archived {
        manager.publish_state(row);
    }
    manager.publish_state(&row);
    Ok(Some(Outcome::Triggered))
}
