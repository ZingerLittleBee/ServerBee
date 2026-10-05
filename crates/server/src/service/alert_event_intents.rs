//! Recover one-shot general alerts independently of subsequent Agent events.
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, NotSet, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};

use crate::{
    config::AppConfig,
    entity::{alert_event_intent as intent, alert_rule, alert_state, server},
    error::AppError,
    service::{
        alert::{
            AlertRuleItem, AlertService, AlertStateManager, SECURITY_RULE_TYPES, rule_covers_server,
        },
        maintenance::MaintenanceService,
        mobile_alert_push,
    },
};

/// Called BEFORE a producer consumes its event, in that producer's writer
/// transaction. A failed capture must roll back the source update as well.
pub(crate) async fn capture(
    txn: &DatabaseTransaction,
    server_id: &str,
    event_type: &str,
    occurred_at: DateTime<Utc>,
) -> Result<(), AppError> {
    if MaintenanceService::is_in_maintenance(txn, server_id).await? {
        return Ok(());
    }
    let rules = alert_rule::Entity::find()
        .filter(alert_rule::Column::Enabled.eq(true))
        .all(txn)
        .await?;
    for rule in rules {
        let items: Vec<AlertRuleItem> = serde_json::from_str(&rule.rules_json)
            .map_err(|_| AppError::Internal("Invalid persisted event alert rule".into()))?;
        if !items.iter().any(|item| item.rule_type == event_type)
            || items
                .iter()
                .any(|item| SECURITY_RULE_TYPES.contains(&item.rule_type.as_str()))
            || !rule_covers_server(&rule.cover_type, &rule.server_ids_json, server_id)
        {
            continue;
        }
        // Pending events already reserve their original cycle and suppression.
        // Later source events cannot duplicate a failed once-only admission.
        let pending = intent::Entity::find()
            .filter(intent::Column::RuleId.eq(&rule.id))
            .filter(intent::Column::ServerId.eq(server_id))
            .order_by_desc(intent::Column::Id)
            .one(txn)
            .await?;
        let persisted = alert_state::Entity::find()
            .filter(alert_state::Column::RuleId.eq(&rule.id))
            .filter(alert_state::Column::ServerId.eq(server_id))
            .filter(alert_state::Column::EventKey.eq(""))
            .filter(alert_state::Column::Resolved.eq(false))
            .one(txn)
            .await?;
        let previous = pending
            .map(|state| (state.first_triggered_at, state.occurred_at, state.count))
            .or_else(|| {
                persisted.map(|state| {
                    (
                        state.first_triggered_at,
                        state.last_notified_at,
                        state.count,
                    )
                })
            });
        let should_notify = previous.is_none_or(|(_, last, _)| {
            rule.trigger_mode != "once" && occurred_at - last >= Duration::minutes(5)
        });
        intent::ActiveModel {
            id: NotSet,
            rule_id: Set(rule.id),
            server_id: Set(server_id.to_string()),
            event_type: Set(event_type.to_string()),
            trigger_mode: Set(rule.trigger_mode),
            first_triggered_at: Set(previous.map_or(occurred_at, |(first, _, _)| first)),
            occurred_at: Set(occurred_at),
            count: Set(previous.map_or(1, |(_, _, count)| count.saturating_add(1))),
            should_notify: Set(should_notify),
        }
        .insert(txn)
        .await?;
    }
    Ok(())
}

/// Same entry point for immediate dispatch and the production periodic tick.
/// Failed dimensions remain ordered; another dimension can still make progress.
pub(crate) async fn replay(
    db: &DatabaseConnection,
    config: &AppConfig,
    manager: &AlertStateManager,
) -> Result<(), AppError> {
    let pending = intent::Entity::find()
        .order_by_asc(intent::Column::Id)
        .limit(100)
        .all(db)
        .await?;
    let mut failed = std::collections::HashSet::new();
    for item in pending {
        let dimension = (item.rule_id.clone(), item.server_id.clone());
        if failed.contains(&dimension) {
            continue;
        }
        if let Err(error) = replay_one(db, config, manager, item.id).await {
            failed.insert(dimension);
            tracing::warn!("Event alert intent remains pending: {error}");
        }
    }
    Ok(())
}

async fn replay_one(
    db: &DatabaseConnection,
    config: &AppConfig,
    manager: &AlertStateManager,
    id: i64,
) -> Result<(), AppError> {
    let admission = manager.admission_lock.lock().await;
    let txn = db.begin().await?;
    txn.execute_unprepared("UPDATE alert_event_intents SET count=count WHERE id=-1")
        .await?;
    let Some(item) = intent::Entity::find_by_id(id).one(&txn).await? else {
        txn.commit().await?;
        return Ok(());
    };
    let rule = alert_rule::Entity::find_by_id(&item.rule_id)
        .one(&txn)
        .await?;
    let srv = server::Entity::find_by_id(&item.server_id)
        .one(&txn)
        .await?;
    let Some((mut rule, srv)) = rule.zip(srv).filter(|(rule, _)| rule.enabled) else {
        intent::Entity::delete_by_id(id).exec(&txn).await?;
        txn.commit().await?;
        return Ok(());
    };
    let existing = alert_state::Entity::find()
        .filter(alert_state::Column::RuleId.eq(&item.rule_id))
        .filter(alert_state::Column::ServerId.eq(&item.server_id))
        .filter(alert_state::Column::EventKey.eq(""))
        .one(&txn)
        .await?;
    let mut active: alert_state::ActiveModel = existing.clone().map(Into::into).unwrap_or_default();
    active.rule_id = Set(item.rule_id);
    active.server_id = Set(item.server_id);
    active.event_key = Set(String::new());
    active.first_triggered_at = Set(item.first_triggered_at);
    active.last_notified_at = Set(item.occurred_at);
    active.count = Set(item.count);
    active.resolved = Set(false);
    active.resolved_at = Set(None);
    active.updated_at = Set(item.occurred_at);
    let state = if existing.is_some() {
        active.update(&txn).await?
    } else {
        active.insert(&txn).await?
    };
    // Keep the original trigger mode for logical ID derivation after rule edits.
    rule.trigger_mode = item.trigger_mode;
    let notify = item.should_notify;
    if notify && let Err(error) = mobile_alert_push::enqueue(&txn, &rule, &state, &srv.name).await {
        txn.rollback().await?;
        return Err(error);
    }
    intent::Entity::delete_by_id(id).exec(&txn).await?;
    txn.commit().await?;
    manager.publish_state(&state);
    drop(admission);
    // Existing external channels remain best-effort. Only the transaction that
    // consumes this intent makes the attempt; job retries/restarts do not repeat it.
    if notify {
        AlertService::notify_triggered(
            db,
            config,
            &rule,
            &state.server_id,
            &srv.name,
            item.occurred_at,
        )
        .await;
    }
    Ok(())
}
