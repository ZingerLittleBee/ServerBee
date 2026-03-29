use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::Utc;
use sea_orm::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::entity::server_group;
use crate::error::{ApiResponse, AppError, ok};
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateGroupRequest {
    name: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateGroupRequest {
    name: Option<String>,
    weight: Option<i32>,
}

/// GET endpoints accessible to all authenticated users (admin + member).
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route("/server-groups", get(list_groups))
}

/// Write endpoints (POST/PUT/DELETE) restricted to admin users only.
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/server-groups", post(create_group))
        .route("/server-groups/{id}", put(update_group))
        .route("/server-groups/{id}", delete(delete_group))
}

#[utoipa::path(
    get,
    path = "/api/server-groups",
    operation_id = "list_server_groups",
    tag = "server-groups",
    responses(
        (status = 200, description = "List all server groups", body = Vec<server_group::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_groups(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<server_group::Model>>>, AppError> {
    let groups = server_group::Entity::find()
        .order_by_desc(server_group::Column::Weight)
        .order_by_asc(server_group::Column::Name)
        .all(&state.db)
        .await?;
    ok(groups)
}

#[utoipa::path(
    post,
    path = "/api/server-groups",
    operation_id = "create_server_group",
    tag = "server-groups",
    request_body = CreateGroupRequest,
    responses(
        (status = 200, description = "Group created", body = server_group::Model),
        (status = 409, description = "Group name already exists"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn create_group(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateGroupRequest>,
) -> Result<Json<ApiResponse<server_group::Model>>, AppError> {
    if body.name.is_empty() {
        return Err(AppError::Validation("Name is required".to_string()));
    }

    // Check for duplicate name
    let existing = server_group::Entity::find()
        .filter(server_group::Column::Name.eq(&body.name))
        .one(&state.db)
        .await?;

    if existing.is_some() {
        return Err(AppError::Conflict(format!(
            "Group '{}' already exists",
            body.name
        )));
    }

    let new_group = server_group::ActiveModel {
        id: Set(Uuid::new_v4().to_string()),
        name: Set(body.name),
        weight: Set(0),
        created_at: Set(Utc::now()),
    };

    let result = new_group.insert(&state.db).await?;
    ok(result)
}

#[utoipa::path(
    put,
    path = "/api/server-groups/{id}",
    operation_id = "update_server_group",
    tag = "server-groups",
    params(("id" = String, Path, description = "Server group ID")),
    request_body = UpdateGroupRequest,
    responses(
        (status = 200, description = "Group updated", body = server_group::Model),
        (status = 404, description = "Group not found"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn update_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateGroupRequest>,
) -> Result<Json<ApiResponse<server_group::Model>>, AppError> {
    let model = server_group::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound("Server group not found".to_string()))?;

    let mut active: server_group::ActiveModel = model.into();

    if let Some(name) = body.name {
        if name.is_empty() {
            return Err(AppError::Validation("Name cannot be empty".to_string()));
        }
        active.name = Set(name);
    }
    if let Some(weight) = body.weight {
        active.weight = Set(weight);
    }

    let updated = active.update(&state.db).await?;
    ok(updated)
}

#[utoipa::path(
    delete,
    path = "/api/server-groups/{id}",
    operation_id = "delete_server_group",
    tag = "server-groups",
    params(("id" = String, Path, description = "Server group ID")),
    responses(
        (status = 200, description = "Group deleted"),
        (status = 404, description = "Group not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn delete_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    let result = server_group::Entity::delete_by_id(&id)
        .exec(&state.db)
        .await?;

    if result.rows_affected == 0 {
        return Err(AppError::NotFound("Server group not found".to_string()));
    }

    ok("ok")
}
