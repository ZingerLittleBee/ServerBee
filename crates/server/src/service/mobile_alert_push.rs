//! Alert admission reuses rule transitions; only ciphertext enters the durable queue.
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter};
use sha2::{Digest, Sha256};

use crate::{
    entity::{
        alert_rule, alert_state, mobile_push_outbox as outbox,
        mobile_push_registration as registration, user,
    },
    error::AppError,
    service::{
        alert::{AlertRuleItem, SECURITY_RULE_TYPES, alert_detail_key},
        mobile_push_outbox::eligible,
        push_envelope::{AlertPushTarget, PushContent, encrypt},
    },
};

pub(super) async fn enqueue(
    txn: &DatabaseTransaction,
    rule: &alert_rule::Model,
    state: &alert_state::Model,
    server_name: &str,
) -> Result<(), AppError> {
    let items: Vec<AlertRuleItem> = serde_json::from_str(&rule.rules_json)
        .map_err(|_| AppError::Internal("Invalid persisted alert rule".into()))?;
    // Security admission is owned by SecurityService, never general alert fanout.
    if !rule.enabled
        || items
            .iter()
            .any(|item| SECURITY_RULE_TYPES.contains(&item.rule_type.as_str()))
    {
        return Ok(());
    }
    let occurred = if state.resolved {
        state.resolved_at.unwrap_or(state.updated_at)
    } else {
        state.last_notified_at
    };
    let created_at = occurred.timestamp();
    if created_at + 1800 <= Utc::now().timestamp() {
        return Ok(());
    }
    let alert_key = alert_detail_key(state);
    let status = if state.resolved { "resolved" } else { "firing" };
    let repeat = if !state.resolved && rule.trigger_mode != "once" {
        occurred.to_rfc3339()
    } else {
        String::new()
    };
    let logical = serde_json::to_vec(&("alert", &alert_key, status, repeat))
        .map_err(|_| AppError::Internal("Alert identity encoding failed".into()))?;
    let hash = Sha256::digest(logical);
    let mut id = [0_u8; 16];
    id.copy_from_slice(&hash[..16]);
    let event_id = uuid::Uuid::from_bytes(id).to_string();
    let rows = registration::Entity::find()
        .filter(registration::Column::Enabled.eq(true))
        .filter(registration::Column::Alerts.eq(true))
        .all(txn)
        .await?;
    for row in rows {
        let Some(owner) = user::Entity::find_by_id(&row.user_id).one(txn).await? else {
            continue;
        };
        let mut job = outbox::Model {
            event_id: event_id.clone(),
            installation_id: row.installation_id.clone(),
            user_id: row.user_id.clone(),
            mobile_session_id: row.mobile_session_id.clone(),
            registration_revision: row.revision,
            recipient_role: owner.role,
            category: "alert".into(),
            task_run_id: None,
            created_at,
            expires_at: created_at + 1800,
            envelope: None,
            outcome: "pending".into(),
            reason: "Queued".into(),
            attempts: 0,
            next_attempt_at: created_at,
            lease_id: None,
            lease_until: 0,
        };
        if eligible(txn, &job).await?.is_none() {
            continue;
        }
        if outbox::Entity::find_by_id((event_id.clone(), row.installation_id.clone()))
            .one(txn)
            .await?
            .is_some()
        {
            continue;
        }
        let Some((key_id, secret, deployment)) = row
            .content_key_id
            .as_deref()
            .zip(row.content_key.as_deref())
            .zip(row.deployment_id.as_deref())
            .map(|((id, key), deployment)| (id, key, deployment))
        else {
            continue;
        };
        let Ok(secret) = STANDARD.decode(secret) else {
            continue;
        };
        let content = PushContent {
            kind: "alert".into(),
            deployment_id: deployment.into(),
            user_id: row.user_id,
            installation_id: row.installation_id,
            event_id: event_id.clone(),
            created_at,
            expires_at: created_at + 1800,
            server_id: None,
            security_event_id: None,
            security_event_type: None,
            task_run: None,
            alert: Some(AlertPushTarget {
                alert_key: alert_key.clone(),
                status: status.into(),
                rule_name: rule.name.chars().take(120).collect(),
                server_name: server_name.chars().take(120).collect(),
            }),
        };
        // One corrupt/oversized installation must not suppress another recipient.
        let Ok(envelope) = encrypt(key_id, &secret, &content) else {
            continue;
        };
        job.envelope = Some(
            serde_json::to_string(&envelope)
                .map_err(|_| AppError::Internal("Push envelope encoding failed".into()))?,
        );
        let active: outbox::ActiveModel = job.into();
        active.insert(txn).await?;
    }
    Ok(())
}
