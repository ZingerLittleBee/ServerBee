use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};

use crate::error::{ApiResponse, AppError, ok};
use crate::service::alert::{
    AlertEventResponse, AlertService, AlertStateResponse, CreateAlertRule, UpdateAlertRule,
};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/alert-rules", get(list_rules))
        .route("/alert-rules", post(create_rule))
        .route("/alert-rules/{id}", get(get_rule))
        .route("/alert-rules/{id}", put(update_rule))
        .route("/alert-rules/{id}", delete(delete_rule))
        .route("/alert-rules/{id}/states", get(list_states))
}

#[utoipa::path(
    get,
    path = "/api/alert-rules",
    tag = "alert-rules",
    responses(
        (status = 200, description = "List all alert rules", body = Vec<crate::entity::alert_rule::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn list_rules(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<crate::entity::alert_rule::Model>>>, AppError> {
    let rules = AlertService::list(&state.db).await?;
    ok(rules)
}

#[utoipa::path(
    get,
    path = "/api/alert-rules/{id}",
    tag = "alert-rules",
    params(("id" = String, Path, description = "Alert rule ID")),
    responses(
        (status = 200, description = "Alert rule details", body = crate::entity::alert_rule::Model),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<crate::entity::alert_rule::Model>>, AppError> {
    let rule = AlertService::get(&state.db, &id).await?;
    ok(rule)
}

#[utoipa::path(
    post,
    path = "/api/alert-rules",
    tag = "alert-rules",
    request_body = CreateAlertRule,
    responses(
        (status = 200, description = "Alert rule created", body = crate::entity::alert_rule::Model),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn create_rule(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateAlertRule>,
) -> Result<Json<ApiResponse<crate::entity::alert_rule::Model>>, AppError> {
    let rule = AlertService::create(&state.db, input).await?;
    ok(rule)
}

#[utoipa::path(
    put,
    path = "/api/alert-rules/{id}",
    tag = "alert-rules",
    params(("id" = String, Path, description = "Alert rule ID")),
    request_body = UpdateAlertRule,
    responses(
        (status = 200, description = "Alert rule updated", body = crate::entity::alert_rule::Model),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn update_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateAlertRule>,
) -> Result<Json<ApiResponse<crate::entity::alert_rule::Model>>, AppError> {
    let rule = AlertService::update(&state.db, &id, input).await?;
    ok(rule)
}

#[utoipa::path(
    delete,
    path = "/api/alert-rules/{id}",
    tag = "alert-rules",
    params(("id" = String, Path, description = "Alert rule ID")),
    responses(
        (status = 200, description = "Alert rule deleted"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn delete_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    AlertService::delete(&state.db, &id).await?;
    ok("ok")
}

#[utoipa::path(
    get,
    path = "/api/alert-rules/{id}/states",
    tag = "alert-rules",
    params(("id" = String, Path, description = "Alert rule ID")),
    responses(
        (status = 200, description = "Alert states for this rule", body = Vec<AlertStateResponse>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn list_states(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<AlertStateResponse>>>, AppError> {
    let states = AlertService::list_states(&state.db, &id).await?;
    ok(states)
}

// ── Alert Events (read-only, all authenticated users) ──

/// Read-only router for alert events, accessible to all authenticated users.
pub fn alert_events_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/alert-events", get(list_alert_events))
        .route("/alert-events/{alert_key}", get(get_alert_event_detail))
}

// ── Alert Event Detail ──

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AlertEventDetailResponse {
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

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct AlertEventsQuery {
    #[serde(default = "default_events_limit")]
    pub limit: u64,
}

fn default_events_limit() -> u64 {
    20
}

#[utoipa::path(
    get,
    path = "/api/alert-events",
    tag = "alert-rules",
    params(AlertEventsQuery),
    responses(
        (status = 200, description = "Recent alert events", body = Vec<AlertEventResponse>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list_alert_events(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AlertEventsQuery>,
) -> Result<Json<ApiResponse<Vec<AlertEventResponse>>>, AppError> {
    let events = AlertService::list_events(&state.db, q.limit).await?;
    ok(events)
}

#[utoipa::path(
    get,
    path = "/api/alert-events/{alert_key}",
    tag = "alert-rules",
    params(("alert_key" = String, Path, description = "Alert key in the format `rule_id:server_id`")),
    responses(
        (status = 200, description = "Alert event detail", body = AlertEventDetailResponse),
        (status = 400, description = "Invalid alert_key format"),
        (status = 404, description = "Alert state or rule not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_alert_event_detail(
    State(state): State<Arc<AppState>>,
    Path(alert_key): Path<String>,
) -> Result<Json<ApiResponse<AlertEventDetailResponse>>, AppError> {
    // Parse alert_key: "rule_id:server_id"
    let (rule_id, server_id) = alert_key
        .split_once(':')
        .ok_or_else(|| AppError::BadRequest("alert_key must be in the format rule_id:server_id".to_string()))?;
    let alert_key_owned = alert_key.clone();

    // Find the alert_state row
    let alert_state = crate::entity::alert_state::Entity::find()
        .filter(crate::entity::alert_state::Column::RuleId.eq(rule_id))
        .filter(crate::entity::alert_state::Column::ServerId.eq(server_id))
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Alert state for key {alert_key} not found")))?;

    // Find the alert_rule row
    let rule = crate::entity::alert_rule::Entity::find_by_id(rule_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Alert rule {rule_id} not found")))?;

    // Find the server row (for name)
    let server_name = crate::entity::server::Entity::find_by_id(server_id)
        .one(&state.db)
        .await?
        .map(|s| s.name)
        .unwrap_or_else(|| "Unknown".to_string());

    let status = if alert_state.resolved { "resolved" } else { "firing" };
    let message = if alert_state.resolved {
        format!("Alert resolved after {} trigger(s)", alert_state.count)
    } else {
        format!("Alert firing — triggered {} time(s)", alert_state.count)
    };

    ok(AlertEventDetailResponse {
        alert_key: alert_key_owned,
        rule_id: rule_id.to_string(),
        rule_name: rule.name,
        server_id: server_id.to_string(),
        server_name,
        status: status.to_string(),
        message,
        trigger_count: alert_state.count,
        first_triggered_at: alert_state.first_triggered_at.to_rfc3339(),
        resolved_at: alert_state.resolved_at.map(|t| t.to_rfc3339()),
        rule_enabled: rule.enabled,
        rule_trigger_mode: rule.trigger_mode,
    })
}
