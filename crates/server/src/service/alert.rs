use chrono::{Duration, Utc};
use dashmap::DashMap;
use sea_orm::prelude::Expr;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{alert_rule, alert_state, network_probe_record, record, server};
use crate::error::AppError;
use crate::service::agent_manager::AgentManager;
use crate::service::maintenance::MaintenanceService;
use crate::service::notification::{NotificationService, NotifyContext};

/// Rule types that are event-driven (not evaluated on a polling interval).
const EVENT_DRIVEN_RULE_TYPES: &[&str] = &[
    "ip_changed",
    "ssh_brute_force_detected",
    "ssh_new_ip_login",
    "port_scan_detected",
    "capability_grant_detected",
];

/// Security-typed alert rules. They must not be mixed with non-security items
/// (AND semantics across different event types is meaningless) and at most one
/// security item is allowed per rule.
pub const SECURITY_RULE_TYPES: &[&str] = &[
    "ssh_brute_force_detected",
    "ssh_new_ip_login",
    "port_scan_detected",
];

/// Cover-type discriminants shared across alert_rule and block_list.
pub const COVER_TYPE_ALL: &str = "all";
pub const COVER_TYPE_INCLUDE: &str = "include";
pub const COVER_TYPE_EXCLUDE: &str = "exclude";
/// All accepted `cover_type` values for alert_rule and block_list inputs.
pub const VALID_COVER_TYPES: &[&str] =
    &[COVER_TYPE_ALL, COVER_TYPE_INCLUDE, COVER_TYPE_EXCLUDE];

/// `origin` discriminants for block_list rows.
pub const ORIGIN_MANUAL: &str = "manual";
pub const ORIGIN_AUTO: &str = "auto";

/// Security rule types whose payload carries a `source_ip` and may attach a
/// `block_source_ip` action.
pub const SOURCE_IP_RULE_TYPES: &[&str] =
    &["ssh_brute_force_detected", "port_scan_detected"];

// ── Alert Rule Types ──

#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AlertRuleItem {
    pub rule_type: String,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub duration: Option<u32>,
    #[serde(default)]
    pub cycle_interval: Option<String>,
    #[serde(default)]
    pub cycle_limit: Option<i64>,
    /// Parameters for security-typed rules (`ssh_brute_force_detected`,
    /// `ssh_new_ip_login`, `port_scan_detected`). Ignored for metric rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security: Option<SecurityRuleParams>,
}

/// Side-effect action attached to an alert rule. Currently only
/// `block_source_ip` is supported, which is restricted to security rules whose
/// payload carries a `source_ip` (`ssh_brute_force_detected` /
/// `port_scan_detected`).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlertRuleAction {
    /// Auto-block the `source_ip` from the triggering security event.
    /// Only valid on `ssh_brute_force_detected` / `port_scan_detected` rules.
    BlockSourceIp {
        #[serde(default = "default_action_cover_type")]
        cover_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        server_ids_json: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comment: Option<String>,
    },
}

fn default_action_cover_type() -> String {
    COVER_TYPE_ALL.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SecurityRuleParams {
    /// Minimum failed attempts to fire (`ssh_brute_force_detected`).
    #[serde(default)]
    pub min_failed_count: Option<u32>,
    /// Minimum distinct ports scanned to fire (`port_scan_detected`).
    #[serde(default)]
    pub min_distinct_ports: Option<u32>,
    /// Usernames excluded from `ssh_new_ip_login`.
    #[serde(default)]
    pub exclude_users: Vec<String>,
    /// CIDRs excluded from `ssh_new_ip_login`.
    #[serde(default)]
    pub exclude_cidrs: Vec<String>,
    /// Notification dedupe window per (rule, server, source_ip).
    #[serde(default = "default_dedupe_secs")]
    pub dedupe_window_seconds: u32,
}

fn default_dedupe_secs() -> u32 {
    600
}

impl Default for SecurityRuleParams {
    fn default() -> Self {
        Self {
            min_failed_count: None,
            min_distinct_ports: None,
            exclude_users: Vec::new(),
            exclude_cidrs: Vec::new(),
            dedupe_window_seconds: default_dedupe_secs(),
        }
    }
}

/// Validate the shape of `AlertRuleItem`s for create/update paths.
///
/// Security rule types are restricted to one item per rule and may not be
/// mixed with other rule types because `check_server` uses AND semantics
/// across items (`crates/server/src/service/alert.rs`).
pub fn validate_alert_rule_items(items: &[AlertRuleItem]) -> Result<(), AppError> {
    let security_count = items
        .iter()
        .filter(|i| SECURITY_RULE_TYPES.contains(&i.rule_type.as_str()))
        .count();
    let non_security_count = items.len() - security_count;
    if security_count > 0 && non_security_count > 0 {
        return Err(AppError::BadRequest(
            "cannot mix security rule types with other items".to_string(),
        ));
    }
    if security_count > 1 {
        return Err(AppError::BadRequest(
            "only one security item per alert_rule is supported".to_string(),
        ));
    }
    Ok(())
}

/// Validate `AlertRuleAction`s. At most one action per rule. `block_source_ip`
/// is only allowed when every item in the rule is one of
/// `ssh_brute_force_detected` / `port_scan_detected` (i.e. payloads that carry
/// a `source_ip`).
pub fn validate_actions(
    rules: &[AlertRuleItem],
    actions: &[AlertRuleAction],
) -> Result<(), AppError> {
    if actions.is_empty() {
        return Ok(());
    }
    if actions.len() > 1 {
        return Err(AppError::Validation(
            "at most one action per alert_rule".to_string(),
        ));
    }
    for a in actions {
        match a {
            AlertRuleAction::BlockSourceIp { cover_type, .. } => {
                if !VALID_COVER_TYPES.contains(&cover_type.as_str()) {
                    return Err(AppError::Validation(format!(
                        "invalid cover_type '{cover_type}' on action"
                    )));
                }
                if rules.is_empty()
                    || !rules
                        .iter()
                        .all(|r| SOURCE_IP_RULE_TYPES.contains(&r.rule_type.as_str()))
                {
                    return Err(AppError::Validation(
                        "block_source_ip is only allowed on \
                         ssh_brute_force_detected / port_scan_detected rules"
                            .to_string(),
                    ));
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateAlertRule {
    pub name: String,
    pub rules: Vec<AlertRuleItem>,
    #[serde(default = "default_trigger_mode")]
    pub trigger_mode: String,
    pub notification_group_id: Option<String>,
    #[serde(default = "default_cover_type")]
    pub cover_type: String,
    pub server_ids: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<AlertRuleAction>,
}

fn default_trigger_mode() -> String {
    "always".to_string()
}

fn default_cover_type() -> String {
    COVER_TYPE_ALL.to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateAlertRule {
    pub name: Option<String>,
    pub rules: Option<Vec<AlertRuleItem>>,
    pub trigger_mode: Option<String>,
    pub notification_group_id: Option<Option<String>>,
    pub cover_type: Option<String>,
    pub server_ids: Option<Option<Vec<String>>>,
    pub enabled: Option<bool>,
    #[serde(default)]
    pub actions: Option<Vec<AlertRuleAction>>,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AlertStateResponse {
    pub server_id: String,
    pub server_name: String,
    pub first_triggered_at: chrono::DateTime<chrono::Utc>,
    pub last_notified_at: chrono::DateTime<chrono::Utc>,
    pub count: i32,
    pub resolved: bool,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ── Alert State (hot cache + DB persistence) ──

#[derive(Debug, Clone)]
pub struct TriggeredInfo {
    #[allow(dead_code)]
    pub first_triggered_at: chrono::DateTime<Utc>,
    pub last_notified_at: chrono::DateTime<Utc>,
    pub count: u32,
}

pub struct AlertStateManager {
    triggered: DashMap<(String, String, String), TriggeredInfo>,
}

impl Default for AlertStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AlertStateManager {
    /// Create an empty `AlertStateManager` with no pre-loaded state.
    pub fn new() -> Self {
        Self {
            triggered: DashMap::new(),
        }
    }

    pub async fn load_from_db(db: &DatabaseConnection) -> Result<Self, AppError> {
        let states = alert_state::Entity::find()
            .filter(alert_state::Column::Resolved.eq(false))
            .all(db)
            .await?;

        let triggered = DashMap::new();
        for s in states {
            triggered.insert(
                (s.rule_id, s.server_id, s.event_key),
                TriggeredInfo {
                    first_triggered_at: s.first_triggered_at,
                    last_notified_at: s.last_notified_at,
                    count: s.count as u32,
                },
            );
        }

        Ok(Self { triggered })
    }

    pub fn is_triggered(&self, rule_id: &str, server_id: &str, event_key: &str) -> bool {
        self.triggered.contains_key(&(
            rule_id.to_string(),
            server_id.to_string(),
            event_key.to_string(),
        ))
    }

    pub fn get_info(
        &self,
        rule_id: &str,
        server_id: &str,
        event_key: &str,
    ) -> Option<TriggeredInfo> {
        self.triggered
            .get(&(
                rule_id.to_string(),
                server_id.to_string(),
                event_key.to_string(),
            ))
            .map(|r| r.clone())
    }

    pub async fn mark_triggered(
        &self,
        db: &DatabaseConnection,
        rule_id: &str,
        server_id: &str,
        event_key: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now();
        let key = (
            rule_id.to_string(),
            server_id.to_string(),
            event_key.to_string(),
        );

        if let Some(mut info) = self.triggered.get_mut(&key) {
            info.count += 1;
            info.last_notified_at = now;

            // Update DB
            alert_state::Entity::update_many()
                .col_expr(
                    alert_state::Column::Count,
                    Expr::col(alert_state::Column::Count).add(1),
                )
                .col_expr(alert_state::Column::LastNotifiedAt, Expr::value(now))
                .col_expr(alert_state::Column::UpdatedAt, Expr::value(now))
                .filter(alert_state::Column::RuleId.eq(rule_id))
                .filter(alert_state::Column::ServerId.eq(server_id))
                .filter(alert_state::Column::EventKey.eq(event_key))
                .filter(alert_state::Column::Resolved.eq(false))
                .exec(db)
                .await?;
        } else {
            self.triggered.insert(
                key,
                TriggeredInfo {
                    first_triggered_at: now,
                    last_notified_at: now,
                    count: 1,
                },
            );

            // Re-arm the existing row if one is left over from a prior
            // resolve cycle, otherwise insert a fresh one. A
            // UNIQUE(rule_id, server_id, event_key) constraint means at most
            // one row can exist per dimension, so a blind INSERT here would
            // fail and abort evaluation forever after the first
            // trigger→resolve.
            let existing = alert_state::Entity::find()
                .filter(alert_state::Column::RuleId.eq(rule_id))
                .filter(alert_state::Column::ServerId.eq(server_id))
                .filter(alert_state::Column::EventKey.eq(event_key))
                .one(db)
                .await?;
            if let Some(row) = existing {
                let mut am: alert_state::ActiveModel = row.into();
                am.first_triggered_at = Set(now);
                am.last_notified_at = Set(now);
                am.count = Set(1);
                am.resolved = Set(false);
                am.resolved_at = Set(None);
                am.updated_at = Set(now);
                am.update(db).await?;
            } else {
                let model = alert_state::ActiveModel {
                    id: NotSet,
                    rule_id: Set(rule_id.to_string()),
                    server_id: Set(server_id.to_string()),
                    event_key: Set(event_key.to_string()),
                    first_triggered_at: Set(now),
                    last_notified_at: Set(now),
                    count: Set(1),
                    resolved: Set(false),
                    resolved_at: Set(None),
                    updated_at: Set(now),
                };
                model.insert(db).await?;
            }
        }

        Ok(())
    }

    pub async fn mark_resolved(
        &self,
        db: &DatabaseConnection,
        rule_id: &str,
        server_id: &str,
        event_key: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now();
        let key = (
            rule_id.to_string(),
            server_id.to_string(),
            event_key.to_string(),
        );

        self.triggered.remove(&key);

        alert_state::Entity::update_many()
            .col_expr(alert_state::Column::Resolved, Expr::value(true))
            .col_expr(alert_state::Column::ResolvedAt, Expr::value(Some(now)))
            .col_expr(alert_state::Column::UpdatedAt, Expr::value(now))
            .filter(alert_state::Column::RuleId.eq(rule_id))
            .filter(alert_state::Column::ServerId.eq(server_id))
            .filter(alert_state::Column::EventKey.eq(event_key))
            .filter(alert_state::Column::Resolved.eq(false))
            .exec(db)
            .await?;

        Ok(())
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AlertEventResponse {
    pub rule_id: String,
    pub rule_name: String,
    pub server_id: String,
    pub server_name: String,
    /// "firing" or "resolved"
    pub status: String,
    /// first_triggered_at for firing, resolved_at for resolved
    pub event_at: String,
    pub resolved_at: Option<String>,
    pub count: i32,
}

// ── Alert Rule CRUD ──

pub struct AlertService;

impl AlertService {
    pub async fn list(db: &DatabaseConnection) -> Result<Vec<alert_rule::Model>, AppError> {
        Ok(alert_rule::Entity::find().all(db).await?)
    }

    pub async fn get(db: &DatabaseConnection, id: &str) -> Result<alert_rule::Model, AppError> {
        alert_rule::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Alert rule {id} not found")))
    }

    pub async fn create(
        db: &DatabaseConnection,
        input: CreateAlertRule,
    ) -> Result<alert_rule::Model, AppError> {
        validate_cover_type(&input.cover_type)?;
        validate_alert_rule_items(&input.rules)?;
        validate_actions(&input.rules, &input.actions)?;
        let rules_json = serde_json::to_string(&input.rules)
            .map_err(|e| AppError::Validation(format!("Invalid rules: {e}")))?;
        let server_ids_json = input
            .server_ids
            .map(|ids| serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string()));
        let actions_json = if input.actions.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&input.actions)
                    .map_err(|e| AppError::Validation(format!("Invalid actions: {e}")))?,
            )
        };
        let now = Utc::now();

        let model = alert_rule::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            name: Set(input.name),
            enabled: Set(input.enabled),
            rules_json: Set(rules_json),
            trigger_mode: Set(input.trigger_mode),
            notification_group_id: Set(input.notification_group_id),
            fail_trigger_tasks: Set(None),
            recover_trigger_tasks: Set(None),
            cover_type: Set(input.cover_type),
            server_ids_json: Set(server_ids_json),
            actions_json: Set(actions_json),
            created_at: Set(now),
            updated_at: Set(now),
        };
        Ok(model.insert(db).await?)
    }

    pub async fn update(
        db: &DatabaseConnection,
        id: &str,
        input: UpdateAlertRule,
    ) -> Result<alert_rule::Model, AppError> {
        let existing = Self::get(db, id).await?;
        let existing_rules: Vec<AlertRuleItem> =
            serde_json::from_str(&existing.rules_json).unwrap_or_default();
        let mut model: alert_rule::ActiveModel = existing.into();

        if let Some(name) = input.name {
            model.name = Set(name);
        }
        let effective_rules: Vec<AlertRuleItem> = if let Some(rules) = input.rules {
            validate_alert_rule_items(&rules)?;
            let rules_json = serde_json::to_string(&rules)
                .map_err(|e| AppError::Validation(format!("Invalid rules: {e}")))?;
            model.rules_json = Set(rules_json);
            rules
        } else {
            existing_rules
        };
        if let Some(trigger_mode) = input.trigger_mode {
            model.trigger_mode = Set(trigger_mode);
        }
        if let Some(notification_group_id) = input.notification_group_id {
            model.notification_group_id = Set(notification_group_id);
        }
        if let Some(cover_type) = input.cover_type {
            validate_cover_type(&cover_type)?;
            model.cover_type = Set(cover_type);
        }
        if let Some(server_ids) = input.server_ids {
            let json = server_ids
                .map(|ids| serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string()));
            model.server_ids_json = Set(json);
        }
        if let Some(enabled) = input.enabled {
            model.enabled = Set(enabled);
        }
        if let Some(actions) = input.actions {
            validate_actions(&effective_rules, &actions)?;
            let actions_json = if actions.is_empty() {
                None
            } else {
                Some(
                    serde_json::to_string(&actions)
                        .map_err(|e| AppError::Validation(format!("Invalid actions: {e}")))?,
                )
            };
            model.actions_json = Set(actions_json);
        }
        model.updated_at = Set(Utc::now());

        Ok(model.update(db).await?)
    }

    pub async fn list_states(
        db: &DatabaseConnection,
        rule_id: &str,
    ) -> Result<Vec<AlertStateResponse>, AppError> {
        let states = alert_state::Entity::find()
            .filter(alert_state::Column::RuleId.eq(rule_id))
            .order_by_desc(alert_state::Column::UpdatedAt)
            .all(db)
            .await
            .map_err(AppError::from)?;

        let mut result = Vec::new();
        for state in states {
            let server_name = server::Entity::find_by_id(&state.server_id)
                .one(db)
                .await
                .map_err(AppError::from)?
                .map(|s| s.name)
                .unwrap_or_else(|| "Unknown".to_string());

            result.push(AlertStateResponse {
                server_id: state.server_id,
                server_name,
                first_triggered_at: state.first_triggered_at,
                last_notified_at: state.last_notified_at,
                count: state.count,
                resolved: state.resolved,
                resolved_at: state.resolved_at,
            });
        }
        Ok(result)
    }

    /// List recent alert events across all rules and servers.
    ///
    /// Joins `alert_state` with `alert_rule` (for rule_name) and `server` (for
    /// server_name). Results are ordered: firing events first, then by event_at
    /// DESC. Capped at `limit`.
    pub async fn list_events(
        db: &DatabaseConnection,
        limit: u64,
    ) -> Result<Vec<AlertEventResponse>, AppError> {
        let states = alert_state::Entity::find()
            .order_by_asc(alert_state::Column::Resolved)
            .order_by_desc(alert_state::Column::UpdatedAt)
            .limit(limit)
            .all(db)
            .await?;

        // Collect unique rule_ids and server_ids for batch lookup
        let rule_ids: Vec<&str> = states.iter().map(|s| s.rule_id.as_str()).collect();
        let server_ids: Vec<&str> = states.iter().map(|s| s.server_id.as_str()).collect();

        let rules = alert_rule::Entity::find()
            .filter(alert_rule::Column::Id.is_in(rule_ids))
            .all(db)
            .await?;
        let rule_map: std::collections::HashMap<String, String> =
            rules.into_iter().map(|r| (r.id, r.name)).collect();

        let servers = server::Entity::find()
            .filter(server::Column::Id.is_in(server_ids))
            .all(db)
            .await?;
        let server_map: std::collections::HashMap<String, String> =
            servers.into_iter().map(|s| (s.id, s.name)).collect();

        let result = states
            .into_iter()
            .map(|s| {
                let status = if s.resolved { "resolved" } else { "firing" };
                let event_at = if s.resolved {
                    s.resolved_at
                        .map(|t| t.to_rfc3339())
                        .unwrap_or_else(|| s.first_triggered_at.to_rfc3339())
                } else {
                    s.first_triggered_at.to_rfc3339()
                };
                let resolved_at = s.resolved_at.map(|t| t.to_rfc3339());
                AlertEventResponse {
                    rule_id: s.rule_id.clone(),
                    rule_name: rule_map
                        .get(&s.rule_id)
                        .cloned()
                        .unwrap_or_else(|| "Unknown".to_string()),
                    server_id: s.server_id.clone(),
                    server_name: server_map
                        .get(&s.server_id)
                        .cloned()
                        .unwrap_or_else(|| "Unknown".to_string()),
                    status: status.to_string(),
                    event_at,
                    resolved_at,
                    count: s.count,
                }
            })
            .collect();

        Ok(result)
    }

    pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let result = alert_rule::Entity::delete_by_id(id).exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!("Alert rule {id} not found")));
        }
        // Clean up alert states for this rule
        alert_state::Entity::delete_many()
            .filter(alert_state::Column::RuleId.eq(id))
            .exec(db)
            .await?;
        Ok(())
    }

    // ── Evaluation ──

    /// Evaluate all enabled alert rules against current data.
    pub async fn evaluate_all(
        db: &DatabaseConnection,
        config: &crate::config::AppConfig,
        agent_manager: &AgentManager,
        state_manager: &AlertStateManager,
    ) -> Result<(), AppError> {
        let rules = alert_rule::Entity::find()
            .filter(alert_rule::Column::Enabled.eq(true))
            .all(db)
            .await?;

        for rule in rules {
            // Skip rules where ALL items are event-driven (e.g. ip_changed).
            // These are dispatched from WS handlers via check_event_rules().
            let items: Vec<AlertRuleItem> =
                serde_json::from_str(&rule.rules_json).unwrap_or_default();
            if !items.is_empty()
                && items
                    .iter()
                    .all(|i| EVENT_DRIVEN_RULE_TYPES.contains(&i.rule_type.as_str()))
            {
                continue;
            }

            if let Err(e) =
                Self::evaluate_rule(db, config, agent_manager, state_manager, &rule).await
            {
                tracing::error!("Error evaluating alert rule '{}': {e}", rule.name);
            }
        }
        Ok(())
    }

    async fn evaluate_rule(
        db: &DatabaseConnection,
        config: &crate::config::AppConfig,
        agent_manager: &AgentManager,
        state_manager: &AlertStateManager,
        rule: &alert_rule::Model,
    ) -> Result<(), AppError> {
        let items: Vec<AlertRuleItem> = serde_json::from_str(&rule.rules_json).unwrap_or_default();
        if items.is_empty() {
            return Ok(());
        }

        let servers = resolve_servers(db, &rule.cover_type, &rule.server_ids_json).await?;

        for srv in &servers {
            let triggered = Self::check_server(db, agent_manager, &items, &srv.id).await;

            if triggered {
                // Skip alerting if the server is in a maintenance window
                if MaintenanceService::is_in_maintenance(db, &srv.id)
                    .await
                    .unwrap_or(false)
                {
                    tracing::debug!(
                        "Skipping alert '{}' for server '{}': in maintenance",
                        rule.name,
                        srv.name
                    );
                    continue;
                }
                Self::handle_triggered(db, config, state_manager, rule, &srv.id, &srv.name).await?;
            } else if state_manager.is_triggered(&rule.id, &srv.id, "") {
                // Recovered
                state_manager
                    .mark_resolved(db, &rule.id, &srv.id, "")
                    .await?;
                tracing::info!("Alert '{}' resolved for server '{}'", rule.name, srv.name);
                Self::handle_resolved(db, config, rule, &srv.id, &srv.name).await;
            }
        }

        Ok(())
    }

    async fn check_server(
        db: &DatabaseConnection,
        agent_manager: &AgentManager,
        items: &[AlertRuleItem],
        server_id: &str,
    ) -> bool {
        for item in items {
            let matched = match item.rule_type.as_str() {
                "offline" => {
                    let duration = item.duration.unwrap_or(60) as u64;
                    !agent_manager.is_online(server_id) && {
                        // Check how long the server has been offline by looking at last record
                        match get_last_record_time(db, server_id).await {
                            Some(last) => {
                                let elapsed = (Utc::now() - last).num_seconds().max(0) as u64;
                                elapsed >= duration
                            }
                            None => true,
                        }
                    }
                }
                "transfer_in_cycle" | "transfer_out_cycle" | "transfer_all_cycle" => {
                    check_transfer_cycle(db, server_id, item).await
                }
                "expiration" => {
                    // Check if server's expired_at is within N days (default 7)
                    check_expiration(db, server_id, item).await
                }
                "network_latency" => check_network_latency(db, server_id, item).await,
                "network_packet_loss" => check_network_packet_loss(db, server_id, item).await,
                _ => {
                    // Resource threshold type: check recent records
                    check_threshold(db, server_id, item).await
                }
            };

            if !matched {
                return false; // AND logic: all items must match
            }
        }
        true
    }

    async fn handle_triggered(
        db: &DatabaseConnection,
        config: &crate::config::AppConfig,
        state_manager: &AlertStateManager,
        rule: &alert_rule::Model,
        server_id: &str,
        server_name: &str,
    ) -> Result<(), AppError> {
        let should_notify = match rule.trigger_mode.as_str() {
            "once" => !state_manager.is_triggered(&rule.id, server_id, ""),
            _ => {
                // "always" — but debounce 5 minutes
                match state_manager.get_info(&rule.id, server_id, "") {
                    Some(info) => {
                        let elapsed = Utc::now() - info.last_notified_at;
                        elapsed >= Duration::minutes(5)
                    }
                    None => true,
                }
            }
        };

        state_manager
            .mark_triggered(db, &rule.id, server_id, "")
            .await?;

        if should_notify && let Some(ref group_id) = rule.notification_group_id {
            let ctx = NotifyContext {
                server_name: server_name.to_string(),
                server_id: server_id.to_string(),
                rule_name: rule.name.clone(),
                rule_id: rule.id.clone(),
                event: "triggered".to_string(),
                message: format!("Alert rule '{}' triggered", rule.name),
                time: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                ..Default::default()
            };
            if let Err(e) = NotificationService::send_group(db, config, group_id, &ctx).await {
                tracing::error!("Failed to send alert notification: {e}");
            }
        }

        Ok(())
    }

    /// Send a recovery notification when an alert transitions back to normal.
    ///
    /// This branch is edge-triggered: `evaluate_rule` only reaches it on the
    /// triggered→recovered transition (and `mark_resolved` clears the state),
    /// so no debounce is needed. Notification failures are logged, never
    /// propagated, so a flaky channel cannot block alert-state bookkeeping.
    async fn handle_resolved(
        db: &DatabaseConnection,
        config: &crate::config::AppConfig,
        rule: &alert_rule::Model,
        server_id: &str,
        server_name: &str,
    ) {
        let Some(ref group_id) = rule.notification_group_id else {
            return;
        };
        let ctx = NotifyContext {
            server_name: server_name.to_string(),
            server_id: server_id.to_string(),
            rule_name: rule.name.clone(),
            rule_id: rule.id.clone(),
            event: "resolved".to_string(),
            message: format!("Alert rule '{}' resolved", rule.name),
            time: Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            ..Default::default()
        };
        if let Err(e) = NotificationService::send_group(db, config, group_id, &ctx).await {
            tracing::error!("Failed to send alert recovery notification: {e}");
        }
    }

    /// Check event-driven rules (e.g. `ip_changed`) — called from WS handler
    /// when an event occurs for a specific server.
    pub async fn check_event_rules(
        db: &DatabaseConnection,
        config: &crate::config::AppConfig,
        state_manager: &AlertStateManager,
        server_id: &str,
        event_type: &str,
    ) -> Result<(), AppError> {
        let rules = alert_rule::Entity::find()
            .filter(alert_rule::Column::Enabled.eq(true))
            .all(db)
            .await?;

        for rule in &rules {
            let items: Vec<AlertRuleItem> =
                serde_json::from_str(&rule.rules_json).unwrap_or_default();

            // Check if any item in this rule matches the event type
            let has_matching_event = items.iter().any(|i| i.rule_type == event_type);
            if !has_matching_event {
                continue;
            }

            // Check if this rule covers the given server
            if !rule_covers_server(&rule.cover_type, &rule.server_ids_json, server_id) {
                continue;
            }

            // Skip if server is in maintenance
            if MaintenanceService::is_in_maintenance(db, server_id)
                .await
                .unwrap_or(false)
            {
                tracing::debug!("Skipping event alert for server {server_id}: in maintenance");
                continue;
            }

            // Resolve server name for notification context
            let server_name = server::Entity::find_by_id(server_id)
                .one(db)
                .await?
                .map(|s| s.name)
                .unwrap_or_else(|| "Unknown".to_string());

            Self::handle_triggered(db, config, state_manager, rule, server_id, &server_name)
                .await?;
        }

        Ok(())
    }
}

// ── Helpers ──

/// Check if a rule's cover_type/server_ids covers a specific server (pure, no DB).
pub(crate) fn rule_covers_server(
    cover_type: &str,
    server_ids_json: &Option<String>,
    server_id: &str,
) -> bool {
    match cover_type {
        "include" => {
            let ids: Vec<String> = server_ids_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            ids.iter().any(|id| id == server_id)
        }
        "exclude" => {
            let ids: Vec<String> = server_ids_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            !ids.iter().any(|id| id == server_id)
        }
        _ => true, // "all" (default)
    }
}

fn validate_cover_type(cover_type: &str) -> Result<(), AppError> {
    if VALID_COVER_TYPES.contains(&cover_type) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "Invalid cover_type '{cover_type}': must be one of 'all', 'include', 'exclude'"
        )))
    }
}

async fn resolve_servers(
    db: &DatabaseConnection,
    cover_type: &str,
    server_ids_json: &Option<String>,
) -> Result<Vec<server::Model>, AppError> {
    match cover_type {
        "include" => {
            let ids: Vec<String> = server_ids_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            if ids.is_empty() {
                return Ok(vec![]);
            }
            Ok(server::Entity::find()
                .filter(server::Column::Id.is_in(ids))
                .all(db)
                .await?)
        }
        "exclude" => {
            let ids: Vec<String> = server_ids_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or_default();
            if ids.is_empty() {
                return Ok(server::Entity::find().all(db).await?);
            }
            Ok(server::Entity::find()
                .filter(server::Column::Id.is_not_in(ids))
                .all(db)
                .await?)
        }
        _ => {
            // "all" (default)
            Ok(server::Entity::find().all(db).await?)
        }
    }
}

async fn get_last_record_time(
    db: &DatabaseConnection,
    server_id: &str,
) -> Option<chrono::DateTime<Utc>> {
    record::Entity::find()
        .filter(record::Column::ServerId.eq(server_id))
        .order_by_desc(record::Column::Time)
        .one(db)
        .await
        .ok()
        .flatten()
        .map(|r| r.time)
}

async fn check_threshold(db: &DatabaseConnection, server_id: &str, item: &AlertRuleItem) -> bool {
    let ten_min_ago = Utc::now() - Duration::minutes(10);

    let records = match record::Entity::find()
        .filter(record::Column::ServerId.eq(server_id))
        .filter(record::Column::Time.gte(ten_min_ago))
        .order_by_desc(record::Column::Time)
        .all(db)
        .await
    {
        Ok(r) => r,
        Err(_) => return false,
    };

    if records.is_empty() {
        return false;
    }

    let mut exceeded_count = 0;
    let total = records.len();

    for rec in &records {
        let value = extract_metric(rec, &item.rule_type);
        let exceeds = match (item.min, item.max) {
            (Some(min), Some(max)) => value >= min && value <= max,
            (Some(min), None) => value >= min,
            (None, Some(max)) => value >= max,
            (None, None) => false,
        };
        if exceeds {
            exceeded_count += 1;
        }
    }

    // 70%+ samples exceeded threshold
    exceeded_count as f64 / total as f64 >= 0.7
}

/// Check transfer cycle alert: compare cumulative traffic within a time cycle against a limit.
/// Uses traffic_hourly table for time-windowed queries and traffic_daily+hourly for billing cycles.
async fn check_transfer_cycle(
    db: &DatabaseConnection,
    server_id: &str,
    item: &AlertRuleItem,
) -> bool {
    use crate::service::traffic::{TrafficService, get_cycle_range};
    use sea_orm::{ConnectionTrait, Statement};

    let cycle_limit = match item.cycle_limit {
        Some(limit) => limit,
        None => return false,
    };

    let cycle_interval = item.cycle_interval.as_deref().unwrap_or("month");
    let now = Utc::now();

    let (bytes_in, bytes_out) = match cycle_interval {
        "billing" => {
            // Use server's billing config for cycle range
            let srv = server::Entity::find_by_id(server_id)
                .one(db)
                .await
                .ok()
                .flatten();
            let Some(srv) = srv else { return false };
            let billing_cycle = srv.billing_cycle.as_deref().unwrap_or("monthly");
            let today = now.date_naive();
            let (start, end) = get_cycle_range(billing_cycle, srv.billing_start_day, today);
            match TrafficService::query_cycle_traffic(db, server_id, start, end).await {
                Ok(totals) => totals,
                Err(_) => return false,
            }
        }
        _ => {
            // Time-windowed query using traffic_hourly
            let cycle_start = match cycle_interval {
                "hour" => now - Duration::hours(1),
                "day" => now - Duration::days(1),
                "week" => now - Duration::weeks(1),
                "month" => now - Duration::days(30),
                "year" => now - Duration::days(365),
                _ => now - Duration::days(30),
            };
            let start_str = cycle_start.format("%Y-%m-%d %H:%M:%S").to_string();
            let result = db
                .query_one(Statement::from_sql_and_values(
                    db.get_database_backend(),
                    "SELECT COALESCE(SUM(bytes_in), 0), COALESCE(SUM(bytes_out), 0) \
                     FROM traffic_hourly \
                     WHERE server_id = $1 AND hour >= $2",
                    [server_id.into(), start_str.into()],
                ))
                .await
                .ok()
                .flatten();
            match result {
                Some(row) => {
                    let b_in: i64 = row.try_get_by_index(0).unwrap_or(0);
                    let b_out: i64 = row.try_get_by_index(1).unwrap_or(0);
                    (b_in, b_out)
                }
                None => return false,
            }
        }
    };

    let transfer = match item.rule_type.as_str() {
        "transfer_in_cycle" => bytes_in,
        "transfer_out_cycle" => bytes_out,
        "transfer_all_cycle" => bytes_in + bytes_out,
        _ => 0,
    };

    transfer >= cycle_limit
}

/// Check if a server's `expired_at` is within N days of now (or already expired).
/// `item.duration` = days threshold (default 7). Triggers if expired_at is set and
/// expires within that many days.
async fn check_expiration(db: &DatabaseConnection, server_id: &str, item: &AlertRuleItem) -> bool {
    let srv = server::Entity::find_by_id(server_id)
        .one(db)
        .await
        .ok()
        .flatten();
    let Some(srv) = srv else {
        return false;
    };
    let Some(expired_at) = srv.expired_at else {
        return false;
    };
    let days_threshold = item.duration.unwrap_or(7) as i64;
    let deadline = Utc::now() + Duration::days(days_threshold);
    expired_at <= deadline
}

/// Check network latency alert: worst (highest) avg_latency across all probe targets in the last
/// 10 minutes must meet the threshold. NULL latency records are skipped.
async fn check_network_latency(
    db: &DatabaseConnection,
    server_id: &str,
    item: &AlertRuleItem,
) -> bool {
    let ten_min_ago = Utc::now() - Duration::minutes(10);

    let records = match network_probe_record::Entity::find()
        .filter(network_probe_record::Column::ServerId.eq(server_id))
        .filter(network_probe_record::Column::Timestamp.gte(ten_min_ago))
        .all(db)
        .await
    {
        Ok(r) => r,
        Err(_) => return false,
    };

    let latencies: Vec<f64> = records.iter().filter_map(|r| r.avg_latency).collect();

    if latencies.is_empty() {
        return false;
    }

    let worst = latencies.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    match (item.min, item.max) {
        (Some(min), Some(max)) => worst >= min && worst <= max,
        (Some(min), None) => worst >= min,
        (None, Some(max)) => worst >= max,
        (None, None) => false,
    }
}

/// Check network packet loss alert: worst (highest) packet_loss percentage across all probe
/// targets in the last 10 minutes must meet the threshold (e.g. 10.0 for 10%).
async fn check_network_packet_loss(
    db: &DatabaseConnection,
    server_id: &str,
    item: &AlertRuleItem,
) -> bool {
    let ten_min_ago = Utc::now() - Duration::minutes(10);

    let records = match network_probe_record::Entity::find()
        .filter(network_probe_record::Column::ServerId.eq(server_id))
        .filter(network_probe_record::Column::Timestamp.gte(ten_min_ago))
        .all(db)
        .await
    {
        Ok(r) => r,
        Err(_) => return false,
    };

    if records.is_empty() {
        return false;
    }

    let worst = records
        .iter()
        .map(|r| r.packet_loss)
        .fold(f64::NEG_INFINITY, f64::max);

    match (item.min, item.max) {
        (Some(min), Some(max)) => worst >= min && worst <= max,
        (Some(min), None) => worst >= min,
        (None, Some(max)) => worst >= max,
        (None, None) => false,
    }
}

fn extract_metric(rec: &record::Model, rule_type: &str) -> f64 {
    match rule_type {
        "cpu" => rec.cpu,
        "memory" => {
            // We need mem_total for percentage but we don't have it in the record.
            // Return raw mem_used as bytes. The threshold should be set accordingly.
            rec.mem_used as f64
        }
        "swap" => rec.swap_used as f64,
        "disk" => rec.disk_used as f64,
        "load1" => rec.load1,
        "load5" => rec.load5,
        "load15" => rec.load15,
        "tcp_conn" => rec.tcp_conn as f64,
        "udp_conn" => rec.udp_conn as f64,
        "process" => rec.process_count as f64,
        "net_in_speed" => rec.net_in_speed as f64,
        "net_out_speed" => rec.net_out_speed as f64,
        "temperature" => rec.temperature.unwrap_or(0.0),
        "gpu" => rec.gpu_usage.unwrap_or(0.0),
        _ => 0.0,
    }
}

#[cfg(test)]
/// Pure helper: evaluate whether a single metric value exceeds the threshold
/// defined by `min` and `max` bounds, using the same logic as `check_threshold`.
/// Returns `true` when the value falls within the alerting range.
fn evaluate_threshold(value: f64, min: Option<f64>, max: Option<f64>) -> bool {
    match (min, max) {
        (Some(min), Some(max)) => value >= min && value <= max,
        (Some(min), None) => value >= min,
        (None, Some(max)) => value >= max,
        (None, None) => false,
    }
}

#[cfg(test)]
/// Pure helper: given `exceeded_count` out of `total` samples, return whether
/// the 70 % majority threshold is met (same rule used in `check_threshold`).
fn majority_exceeded(exceeded_count: usize, total: usize) -> bool {
    if total == 0 {
        return false;
    }
    exceeded_count as f64 / total as f64 >= 0.7
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    /// Build a minimal `record::Model` with the given field values.
    fn make_record(cpu: f64, mem_used: i64, load1: f64) -> record::Model {
        record::Model {
            id: 1,
            server_id: "srv-1".to_string(),
            time: Utc::now(),
            cpu,
            mem_used,
            swap_used: 0,
            disk_used: 0,
            net_in_speed: 0,
            net_out_speed: 0,
            net_in_transfer: 0,
            net_out_transfer: 0,
            load1,
            load5: 0.0,
            load15: 0.0,
            tcp_conn: 100,
            udp_conn: 50,
            process_count: 200,
            temperature: Some(55.0),
            gpu_usage: Some(40.0),
            disk_io_json: None,
        }
    }

    // ── T2-1: threshold above ──

    #[test]
    fn test_threshold_above() {
        // min = Some(80.0), max = None  =>  triggers when value >= 80
        assert!(
            evaluate_threshold(90.0, Some(80.0), None),
            "90 >= 80 should trigger"
        );
        assert!(
            evaluate_threshold(80.0, Some(80.0), None),
            "80 >= 80 should trigger (boundary)"
        );
        assert!(
            !evaluate_threshold(79.9, Some(80.0), None),
            "79.9 < 80 should NOT trigger"
        );
    }

    // ── T2-2: threshold below ──

    #[test]
    fn test_threshold_below() {
        // min = None, max = Some(20.0)  =>  triggers when value >= 20
        // NOTE: in the codebase, (None, Some(max)) means value >= max,
        // which is "above max". This test verifies that exact semantic.
        assert!(
            evaluate_threshold(25.0, None, Some(20.0)),
            "25 >= 20 should trigger"
        );
        assert!(
            evaluate_threshold(20.0, None, Some(20.0)),
            "20 >= 20 should trigger (boundary)"
        );
        assert!(
            !evaluate_threshold(19.0, None, Some(20.0)),
            "19 < 20 should NOT trigger"
        );
    }

    #[test]
    fn test_threshold_range() {
        // Both min and max set  =>  triggers when min <= value <= max
        assert!(
            evaluate_threshold(50.0, Some(40.0), Some(60.0)),
            "50 in [40, 60] should trigger"
        );
        assert!(
            evaluate_threshold(40.0, Some(40.0), Some(60.0)),
            "40 at lower boundary should trigger"
        );
        assert!(
            evaluate_threshold(60.0, Some(40.0), Some(60.0)),
            "60 at upper boundary should trigger"
        );
        assert!(
            !evaluate_threshold(39.0, Some(40.0), Some(60.0)),
            "39 below range should NOT trigger"
        );
        assert!(
            !evaluate_threshold(61.0, Some(40.0), Some(60.0)),
            "61 above range should NOT trigger"
        );
    }

    #[test]
    fn test_threshold_no_bounds() {
        // Neither min nor max  =>  never triggers
        assert!(
            !evaluate_threshold(50.0, None, None),
            "no bounds should never trigger"
        );
    }

    // ── T2-3: majority calculation ──

    #[test]
    fn test_majority_exceeded() {
        assert!(majority_exceeded(7, 10), "7/10 = 70% should meet threshold");
        assert!(majority_exceeded(8, 10), "8/10 = 80% should meet threshold");
        assert!(
            !majority_exceeded(6, 10),
            "6/10 = 60% should NOT meet threshold"
        );
        assert!(!majority_exceeded(0, 10), "0/10 should NOT meet threshold");
        assert!(
            !majority_exceeded(0, 0),
            "0/0 (no samples) should NOT meet threshold"
        );
        assert!(majority_exceeded(1, 1), "1/1 = 100% should meet threshold");
    }

    // ── T2-4: extract_metric ──

    #[test]
    fn test_extract_metric_cpu() {
        let rec = make_record(85.5, 4_000_000, 1.2);
        assert!((extract_metric(&rec, "cpu") - 85.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_metric_memory() {
        let rec = make_record(50.0, 8_000_000, 0.0);
        assert!((extract_metric(&rec, "memory") - 8_000_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_metric_load() {
        let rec = make_record(0.0, 0, std::f64::consts::PI);
        assert!((extract_metric(&rec, "load1") - std::f64::consts::PI).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_metric_temperature() {
        let rec = make_record(0.0, 0, 0.0);
        assert!((extract_metric(&rec, "temperature") - 55.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_metric_gpu() {
        let rec = make_record(0.0, 0, 0.0);
        assert!((extract_metric(&rec, "gpu") - 40.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_metric_connections() {
        let rec = make_record(0.0, 0, 0.0);
        assert!((extract_metric(&rec, "tcp_conn") - 100.0).abs() < f64::EPSILON);
        assert!((extract_metric(&rec, "udp_conn") - 50.0).abs() < f64::EPSILON);
        assert!((extract_metric(&rec, "process") - 200.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_metric_unknown_type() {
        let rec = make_record(99.0, 0, 0.0);
        assert!(
            (extract_metric(&rec, "nonexistent") - 0.0).abs() < f64::EPSILON,
            "unknown metric type should return 0.0"
        );
    }

    // ── T2-5: AlertRuleItem serialization round-trip ──

    #[test]
    fn test_alert_rule_item_serialization() {
        let item = AlertRuleItem {
            rule_type: "cpu".to_string(),
            min: Some(80.0),
            max: None,
            duration: Some(300),
            cycle_interval: None,
            cycle_limit: None,
            security: None,
        };

        let json = serde_json::to_string(&item).expect("serialize");
        let parsed: AlertRuleItem = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(parsed.rule_type, "cpu");
        assert_eq!(parsed.min, Some(80.0));
        assert_eq!(parsed.max, None);
        assert_eq!(parsed.duration, Some(300));
    }

    // ── T2-6: default helper functions ──

    #[test]
    fn test_default_trigger_mode() {
        assert_eq!(default_trigger_mode(), "always");
    }

    #[test]
    fn test_default_cover_type() {
        assert_eq!(default_cover_type(), "all");
    }

    // ── rule_covers_server ──

    #[test]
    fn test_rule_covers_server_all() {
        assert!(rule_covers_server("all", &None, "srv-1"));
        assert!(rule_covers_server("all", &Some("[]".to_string()), "srv-1"));
    }

    #[test]
    fn test_rule_covers_server_include() {
        let ids = Some(serde_json::to_string(&vec!["srv-1", "srv-2"]).unwrap());
        assert!(rule_covers_server("include", &ids, "srv-1"));
        assert!(rule_covers_server("include", &ids, "srv-2"));
        assert!(!rule_covers_server("include", &ids, "srv-3"));
        // Empty include list covers nothing
        assert!(!rule_covers_server(
            "include",
            &Some("[]".to_string()),
            "srv-1"
        ));
    }

    #[test]
    fn test_rule_covers_server_exclude() {
        let ids = Some(serde_json::to_string(&vec!["srv-1"]).unwrap());
        assert!(!rule_covers_server("exclude", &ids, "srv-1"));
        assert!(rule_covers_server("exclude", &ids, "srv-2"));
        // Empty exclude list covers everything
        assert!(rule_covers_server(
            "exclude",
            &Some("[]".to_string()),
            "srv-1"
        ));
    }

    // ── event-driven rule type detection ──

    #[test]
    fn test_event_driven_rule_types() {
        assert!(EVENT_DRIVEN_RULE_TYPES.contains(&"ip_changed"));
        assert!(EVENT_DRIVEN_RULE_TYPES.contains(&"ssh_brute_force_detected"));
        assert!(EVENT_DRIVEN_RULE_TYPES.contains(&"ssh_new_ip_login"));
        assert!(EVENT_DRIVEN_RULE_TYPES.contains(&"port_scan_detected"));
        assert!(!EVENT_DRIVEN_RULE_TYPES.contains(&"cpu"));
        assert!(!EVENT_DRIVEN_RULE_TYPES.contains(&"offline"));
    }

    #[test]
    fn capability_grant_detected_is_event_driven_only() {
        assert!(EVENT_DRIVEN_RULE_TYPES.contains(&"capability_grant_detected"));
        assert!(!SECURITY_RULE_TYPES.contains(&"capability_grant_detected"));
        assert!(!SOURCE_IP_RULE_TYPES.contains(&"capability_grant_detected"));
    }

    // ── validate_alert_rule_items ──

    fn item(rule_type: &str) -> AlertRuleItem {
        AlertRuleItem {
            rule_type: rule_type.to_string(),
            min: None,
            max: None,
            duration: None,
            cycle_interval: None,
            cycle_limit: None,
            security: None,
        }
    }

    #[test]
    fn test_validate_rejects_mixing_security_and_metric() {
        let items = vec![item("ssh_brute_force_detected"), item("cpu")];
        let err = validate_alert_rule_items(&items).expect_err("should reject mix");
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn test_validate_rejects_multiple_security_items() {
        let items = vec![
            item("ssh_brute_force_detected"),
            item("ssh_new_ip_login"),
        ];
        let err = validate_alert_rule_items(&items).expect_err("should reject multi-security");
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn test_validate_accepts_single_security_item() {
        let items = vec![item("port_scan_detected")];
        validate_alert_rule_items(&items).expect("single security item is valid");
    }

    #[test]
    fn test_validate_accepts_all_metric_items() {
        let items = vec![item("cpu"), item("memory"), item("offline")];
        validate_alert_rule_items(&items).expect("metric-only is valid");
    }

    #[test]
    fn test_validate_accepts_empty() {
        validate_alert_rule_items(&[]).expect("empty list is valid");
    }

    #[test]
    fn test_security_rule_params_default_dedupe() {
        let json = r#"{"min_failed_count": 10}"#;
        let p: SecurityRuleParams = serde_json::from_str(json).expect("parse");
        assert_eq!(p.dedupe_window_seconds, 600);
    }

    #[test]
    fn test_alert_state_manager_new() {
        let mgr = AlertStateManager::new();
        assert!(!mgr.is_triggered("any-rule", "any-server", ""));
        assert!(mgr.get_info("any-rule", "any-server", "").is_none());
    }

    // ── list_events ──

    use crate::test_utils::setup_test_db;
    use sea_orm::{ActiveModelTrait, Set};

    /// Helper: insert a test server into the database.
    async fn insert_test_server(db: &DatabaseConnection, id: &str, name: &str) {
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(Some("hash".to_string())),
            token_prefix: Set(Some("prefix".to_string())),
            name: Set(name.to_string()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(0),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert test server");
    }

    /// Helper: insert an alert rule into the database.
    async fn insert_test_rule(db: &DatabaseConnection, id: &str, name: &str) {
        let now = Utc::now();
        alert_rule::ActiveModel {
            id: Set(id.to_string()),
            name: Set(name.to_string()),
            enabled: Set(true),
            rules_json: Set("[]".to_string()),
            trigger_mode: Set("always".to_string()),
            notification_group_id: Set(None),
            fail_trigger_tasks: Set(None),
            recover_trigger_tasks: Set(None),
            cover_type: Set("all".to_string()),
            server_ids_json: Set(None),
            actions_json: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .expect("insert test rule");
    }

    /// Helper: insert an alert state into the database.
    async fn insert_test_state(
        db: &DatabaseConnection,
        rule_id: &str,
        server_id: &str,
        resolved: bool,
        first_triggered_at: chrono::DateTime<Utc>,
        resolved_at: Option<chrono::DateTime<Utc>>,
        count: i32,
    ) {
        let now = Utc::now();
        alert_state::ActiveModel {
            id: NotSet,
            rule_id: Set(rule_id.to_string()),
            server_id: Set(server_id.to_string()),
            event_key: Set(String::new()),
            first_triggered_at: Set(first_triggered_at),
            last_notified_at: Set(now),
            count: Set(count),
            resolved: Set(resolved),
            resolved_at: Set(resolved_at),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .expect("insert test state");
    }

    #[tokio::test]
    async fn test_list_events_returns_aggregated_states() {
        let (db, _tmp) = setup_test_db().await;

        // Set up test data
        insert_test_server(&db, "srv-1", "Server Alpha").await;
        insert_test_server(&db, "srv-2", "Server Beta").await;
        insert_test_rule(&db, "rule-1", "CPU Alert").await;
        insert_test_rule(&db, "rule-2", "Memory Alert").await;

        let now = Utc::now();
        let earlier = now - Duration::hours(1);

        // Firing state (resolved=false) should appear first
        insert_test_state(&db, "rule-1", "srv-1", false, earlier, None, 3).await;
        // Resolved state should appear after firing
        insert_test_state(&db, "rule-2", "srv-2", true, earlier, Some(now), 1).await;

        let events = AlertService::list_events(&db, 20).await.unwrap();

        assert_eq!(events.len(), 2);

        // Firing events come first (ordered by resolved ASC)
        assert_eq!(events[0].status, "firing");
        assert_eq!(events[0].rule_name, "CPU Alert");
        assert_eq!(events[0].server_name, "Server Alpha");
        assert_eq!(events[0].count, 3);
        assert!(events[0].resolved_at.is_none());
        // event_at should be first_triggered_at for firing
        assert!(
            events[0]
                .event_at
                .contains(&earlier.format("%Y").to_string())
        );

        assert_eq!(events[1].status, "resolved");
        assert_eq!(events[1].rule_name, "Memory Alert");
        assert_eq!(events[1].server_name, "Server Beta");
        assert_eq!(events[1].count, 1);
        assert!(events[1].resolved_at.is_some());
    }

    #[tokio::test]
    async fn test_list_events_respects_limit() {
        let (db, _tmp) = setup_test_db().await;

        // Create 5 distinct servers and 5 rules to avoid UNIQUE(rule_id, server_id) conflict
        for i in 0..5 {
            insert_test_server(&db, &format!("srv-{i}"), &format!("Server {i}")).await;
            insert_test_rule(&db, &format!("rule-{i}"), &format!("Rule {i}")).await;
        }

        let now = Utc::now();

        // Insert 5 states, each with a different (rule_id, server_id) pair
        for i in 0..5 {
            let t = now - Duration::minutes(i as i64);
            insert_test_state(
                &db,
                &format!("rule-{i}"),
                &format!("srv-{i}"),
                false,
                t,
                None,
                1,
            )
            .await;
        }

        // Request only 3
        let events = AlertService::list_events(&db, 3).await.unwrap();
        assert_eq!(events.len(), 3);

        // Request all
        let events_all = AlertService::list_events(&db, 20).await.unwrap();
        assert_eq!(events_all.len(), 5);
    }

    #[tokio::test]
    async fn test_recovery_dispatches_resolved_notification() {
        use crate::entity::{notification, notification_group};
        use tokio::io::AsyncReadExt;

        let (db, _tmp) = setup_test_db().await;

        // Local webhook sink that captures the first inbound request.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind webhook sink");
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                // Loopback delivers the small request in one or two reads.
                for _ in 0..8 {
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(300),
                        socket.read(&mut chunk),
                    )
                    .await
                    {
                        Ok(Ok(0)) => break,
                        Ok(Ok(n)) => {
                            buf.extend_from_slice(&chunk[..n]);
                            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
                let _ = socket
                    .try_write(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
                let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
            }
        });

        insert_test_server(&db, "srv-1", "Server Alpha").await;

        let now = Utc::now();
        notification::ActiveModel {
            id: Set("notif-1".to_string()),
            name: Set("Hook".to_string()),
            notify_type: Set("webhook".to_string()),
            config_json: Set(format!(
                r#"{{"url":"http://127.0.0.1:{port}/","method":"POST"}}"#
            )),
            enabled: Set(true),
            created_at: Set(now),
        }
        .insert(&db)
        .await
        .expect("insert notification");

        notification_group::ActiveModel {
            id: Set("grp-1".to_string()),
            name: Set("Group".to_string()),
            notification_ids_json: Set(r#"["notif-1"]"#.to_string()),
            created_at: Set(now),
        }
        .insert(&db)
        .await
        .expect("insert notification group");

        // Rule with a CPU threshold that cannot match (no records exist),
        // so check_server() is false and the recovery branch is taken.
        let rule = alert_rule::ActiveModel {
            id: Set("rule-1".to_string()),
            name: Set("CPU Alert".to_string()),
            enabled: Set(true),
            rules_json: Set(r#"[{"rule_type":"cpu","max":90.0}]"#.to_string()),
            trigger_mode: Set("always".to_string()),
            notification_group_id: Set(Some("grp-1".to_string())),
            fail_trigger_tasks: Set(None),
            recover_trigger_tasks: Set(None),
            cover_type: Set("all".to_string()),
            server_ids_json: Set(None),
            actions_json: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .expect("insert rule");

        // Seed the state as already triggered so evaluate_rule sees a
        // triggered→recovered transition.
        let state_manager = AlertStateManager::new();
        state_manager
            .mark_triggered(&db, "rule-1", "srv-1", "")
            .await
            .expect("seed triggered state");

        let (browser_tx, _) = tokio::sync::broadcast::channel(16);
        let agent_manager = AgentManager::new(browser_tx);
        let config = crate::config::AppConfig::default();

        AlertService::evaluate_rule(&db, &config, &agent_manager, &state_manager, &rule)
            .await
            .expect("evaluate_rule");

        let payload = tokio::time::timeout(std::time::Duration::from_secs(5), rx)
            .await
            .expect("recovery notification was not dispatched within timeout")
            .expect("webhook sink dropped");

        assert!(
            payload.contains("resolved"),
            "recovery webhook payload should carry the resolved event, got: {payload}"
        );
        assert!(
            !state_manager.is_triggered("rule-1", "srv-1", ""),
            "state should be cleared after recovery"
        );
    }

    #[tokio::test]
    async fn test_alert_rearms_after_resolve_cycle() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-1", "Server Alpha").await;

        let mgr = AlertStateManager::new();

        // First fire.
        mgr.mark_triggered(&db, "rule-1", "srv-1", "")
            .await
            .expect("first trigger");
        assert!(mgr.is_triggered("rule-1", "srv-1", ""));

        // Recover.
        mgr.mark_resolved(&db, "rule-1", "srv-1", "")
            .await
            .expect("resolve");
        assert!(!mgr.is_triggered("rule-1", "srv-1", ""));

        // Re-arm: triggering again must NOT fail on the
        // UNIQUE(rule_id, server_id, event_key) constraint left by the resolved row.
        mgr.mark_triggered(&db, "rule-1", "srv-1", "")
            .await
            .expect("re-trigger after resolve must succeed");
        assert!(mgr.is_triggered("rule-1", "srv-1", ""));

        // Exactly one row, flipped back to firing.
        let rows = alert_state::Entity::find()
            .filter(alert_state::Column::RuleId.eq("rule-1"))
            .filter(alert_state::Column::ServerId.eq("srv-1"))
            .all(&db)
            .await
            .expect("query states");
        assert_eq!(rows.len(), 1, "no duplicate alert_state rows");
        assert!(!rows[0].resolved, "re-armed row should be firing again");
        assert!(rows[0].resolved_at.is_none(), "resolved_at cleared on re-arm");
        assert_eq!(rows[0].count, 1, "count reset on re-arm");
    }

    // ── AlertRuleAction validator ──

    #[test]
    fn validate_actions_forbids_with_metric_rule() {
        let rules = vec![AlertRuleItem {
            rule_type: "cpu".into(),
            min: Some(80.0),
            ..Default::default()
        }];
        let actions = vec![AlertRuleAction::BlockSourceIp {
            cover_type: "all".into(),
            server_ids_json: None,
            comment: None,
        }];
        let err = validate_actions(&rules, &actions).unwrap_err();
        assert!(format!("{err}").contains("ssh_brute_force_detected"));
    }

    #[test]
    fn validate_actions_forbids_ssh_new_ip_login() {
        let rules = vec![AlertRuleItem {
            rule_type: "ssh_new_ip_login".into(),
            ..Default::default()
        }];
        let actions = vec![AlertRuleAction::BlockSourceIp {
            cover_type: "all".into(),
            server_ids_json: None,
            comment: None,
        }];
        assert!(validate_actions(&rules, &actions).is_err());
    }

    #[test]
    fn validate_actions_allows_brute_force() {
        let rules = vec![AlertRuleItem {
            rule_type: "ssh_brute_force_detected".into(),
            ..Default::default()
        }];
        let actions = vec![AlertRuleAction::BlockSourceIp {
            cover_type: "all".into(),
            server_ids_json: None,
            comment: None,
        }];
        assert!(validate_actions(&rules, &actions).is_ok());
    }

    #[test]
    fn validate_actions_rejects_more_than_one() {
        let rules = vec![AlertRuleItem {
            rule_type: "ssh_brute_force_detected".into(),
            ..Default::default()
        }];
        let actions = vec![
            AlertRuleAction::BlockSourceIp {
                cover_type: "all".into(),
                server_ids_json: None,
                comment: None,
            },
            AlertRuleAction::BlockSourceIp {
                cover_type: "all".into(),
                server_ids_json: None,
                comment: None,
            },
        ];
        assert!(validate_actions(&rules, &actions).is_err());
    }

    #[test]
    fn validate_actions_empty_actions_is_ok() {
        // No actions should always validate, regardless of rules — including
        // empty rules (the early-return branch must not require security rules
        // just to allow zero actions).
        assert!(validate_actions(&[], &[]).is_ok());
        let rules = vec![AlertRuleItem {
            rule_type: "cpu".into(),
            min: Some(80.0),
            ..Default::default()
        }];
        assert!(validate_actions(&rules, &[]).is_ok());
    }

    #[test]
    fn validate_actions_rejects_invalid_cover_type() {
        let rules = vec![AlertRuleItem {
            rule_type: "ssh_brute_force_detected".into(),
            ..Default::default()
        }];
        let actions = vec![AlertRuleAction::BlockSourceIp {
            cover_type: "everyone".into(),
            server_ids_json: None,
            comment: None,
        }];
        let err = validate_actions(&rules, &actions).unwrap_err();
        assert!(format!("{err}").contains("invalid cover_type"));
    }
}
