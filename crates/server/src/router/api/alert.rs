use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};

use crate::error::{ok, ApiResponse, AppError};
use crate::service::alert::{AlertService, AlertStateResponse, CreateAlertRule, UpdateAlertRule};
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
    security(("session_cookie" = []), ("api_key" = []))
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
    security(("session_cookie" = []), ("api_key" = []))
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
    security(("session_cookie" = []), ("api_key" = []))
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
    security(("session_cookie" = []), ("api_key" = []))
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
    security(("session_cookie" = []), ("api_key" = []))
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
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_states(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<Vec<AlertStateResponse>>>, AppError> {
    let states = AlertService::list_states(&state.db, &id).await?;
    ok(states)
}
