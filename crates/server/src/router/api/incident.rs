use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Deserialize;

use crate::entity::{incident, incident_update};
use crate::error::{ApiResponse, AppError, ok};
use crate::service::incident::{
    CreateIncident, CreateIncidentUpdate, IncidentService, UpdateIncident,
};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/incidents", get(list_incidents))
        .route("/incidents", post(create_incident))
        .route("/incidents/{id}", put(update_incident))
        .route("/incidents/{id}", delete(delete_incident))
        .route("/incidents/{id}/updates", post(add_incident_update))
}

#[derive(Deserialize, utoipa::IntoParams)]
pub struct IncidentListQuery {
    /// Filter by status (investigating, identified, monitoring, resolved).
    pub status: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/incidents",
    operation_id = "list_incidents",
    tag = "incidents",
    params(IncidentListQuery),
    responses(
        (status = 200, description = "List all incidents", body = Vec<incident::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn list_incidents(
    State(state): State<Arc<AppState>>,
    Query(q): Query<IncidentListQuery>,
) -> Result<Json<ApiResponse<Vec<incident::Model>>>, AppError> {
    let incidents = IncidentService::list(&state.db, q.status.as_deref()).await?;
    ok(incidents)
}

#[utoipa::path(
    post,
    path = "/api/incidents",
    operation_id = "create_incident",
    tag = "incidents",
    request_body = CreateIncident,
    responses(
        (status = 200, description = "Incident created", body = incident::Model),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn create_incident(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateIncident>,
) -> Result<Json<ApiResponse<incident::Model>>, AppError> {
    let incident = IncidentService::create(&state.db, input).await?;
    ok(incident)
}

#[utoipa::path(
    put,
    path = "/api/incidents/{id}",
    operation_id = "update_incident",
    tag = "incidents",
    params(("id" = String, Path, description = "Incident ID")),
    request_body = UpdateIncident,
    responses(
        (status = 200, description = "Incident updated", body = incident::Model),
        (status = 404, description = "Not found"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn update_incident(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateIncident>,
) -> Result<Json<ApiResponse<incident::Model>>, AppError> {
    let incident = IncidentService::update(&state.db, &id, input).await?;
    ok(incident)
}

#[utoipa::path(
    delete,
    path = "/api/incidents/{id}",
    operation_id = "delete_incident",
    tag = "incidents",
    params(("id" = String, Path, description = "Incident ID")),
    responses(
        (status = 200, description = "Incident deleted"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn delete_incident(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    IncidentService::delete(&state.db, &id).await?;
    ok("ok")
}

#[utoipa::path(
    post,
    path = "/api/incidents/{id}/updates",
    operation_id = "add_incident_update",
    tag = "incidents",
    params(("id" = String, Path, description = "Incident ID")),
    request_body = CreateIncidentUpdate,
    responses(
        (status = 200, description = "Incident update added", body = incident_update::Model),
        (status = 404, description = "Incident not found"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn add_incident_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<CreateIncidentUpdate>,
) -> Result<Json<ApiResponse<incident_update::Model>>, AppError> {
    let update = IncidentService::add_update(&state.db, &id, input).await?;
    ok(update)
}
