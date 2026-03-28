use sea_orm::*;

use crate::entity::{alert_rule, alert_state, server};
use crate::error::AppError;

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct MobileAlertEvent {
    pub alert_key: String,
    pub rule_id: String,
    pub rule_name: String,
    pub server_id: String,
    pub server_name: String,
    pub status: String,
    pub message: String,
    pub trigger_count: i32,
    pub first_triggered_at: String,
    pub last_notified_at: String,
    pub resolved_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct MobileAlertDetail {
    pub alert_key: String,
    pub rule_id: String,
    pub rule_name: String,
    pub server_id: String,
    pub server_name: String,
    pub status: String,
    pub message: String,
    pub trigger_count: i32,
    pub first_triggered_at: String,
    pub resolved_at: Option<String>,
    pub rule_enabled: bool,
    pub rule_trigger_mode: String,
}

pub struct MobileAlertService;

impl MobileAlertService {
    /// List recent alert events, ordered by updated_at desc.
    pub async fn list_events(
        db: &DatabaseConnection,
        limit: u64,
    ) -> Result<Vec<MobileAlertEvent>, AppError> {
        let states = alert_state::Entity::find()
            .order_by_desc(alert_state::Column::UpdatedAt)
            .limit(limit)
            .all(db)
            .await?;

        let mut events = Vec::with_capacity(states.len());
        for state in states {
            let rule = alert_rule::Entity::find_by_id(&state.rule_id)
                .one(db)
                .await?;
            let srv = server::Entity::find_by_id(&state.server_id)
                .one(db)
                .await?;

            let rule_name = rule.as_ref().map(|r| r.name.clone()).unwrap_or_default();
            let server_name = srv.as_ref().map(|s| s.name.clone()).unwrap_or_default();
            let is_resolved = state.resolved;

            events.push(MobileAlertEvent {
                alert_key: format!("{}:{}", state.rule_id, state.server_id),
                rule_id: state.rule_id,
                rule_name,
                server_id: state.server_id,
                server_name,
                status: if is_resolved {
                    "resolved".to_string()
                } else {
                    "firing".to_string()
                },
                message: String::new(),
                trigger_count: state.count,
                first_triggered_at: state.first_triggered_at.to_rfc3339(),
                last_notified_at: state.last_notified_at.to_rfc3339(),
                resolved_at: state.resolved_at.map(|t| t.to_rfc3339()),
                updated_at: state.updated_at.to_rfc3339(),
            });
        }

        Ok(events)
    }

    /// Get alert detail by alert_key (rule_id:server_id).
    pub async fn get_detail(
        db: &DatabaseConnection,
        alert_key: &str,
    ) -> Result<MobileAlertDetail, AppError> {
        let (rule_id, server_id) = parse_alert_key(alert_key)?;

        let state = alert_state::Entity::find()
            .filter(alert_state::Column::RuleId.eq(rule_id))
            .filter(alert_state::Column::ServerId.eq(server_id))
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound("Alert not found".to_string()))?;

        let rule = alert_rule::Entity::find_by_id(rule_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound("Alert rule not found".to_string()))?;

        let srv = server::Entity::find_by_id(server_id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound("Server not found".to_string()))?;

        let is_resolved = state.resolved;

        Ok(MobileAlertDetail {
            alert_key: alert_key.to_string(),
            rule_id: state.rule_id,
            rule_name: rule.name,
            server_id: state.server_id,
            server_name: srv.name,
            status: if is_resolved {
                "resolved".to_string()
            } else {
                "firing".to_string()
            },
            message: String::new(),
            trigger_count: state.count,
            first_triggered_at: state.first_triggered_at.to_rfc3339(),
            resolved_at: state.resolved_at.map(|t| t.to_rfc3339()),
            rule_enabled: rule.enabled,
            rule_trigger_mode: rule.trigger_mode,
        })
    }
}

fn parse_alert_key(key: &str) -> Result<(&str, &str), AppError> {
    key.split_once(':')
        .ok_or_else(|| AppError::Validation("Invalid alert_key format".to_string()))
}
