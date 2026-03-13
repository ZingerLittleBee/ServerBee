use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};

use crate::error::{ok, ApiResponse, AppError};
use crate::service::notification::{
    CreateNotification, CreateNotificationGroup, NotificationService, UpdateNotification,
    UpdateNotificationGroup,
};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/notifications", get(list_notifications))
        .route("/notifications", post(create_notification))
        .route("/notifications/{id}", get(get_notification))
        .route("/notifications/{id}", put(update_notification))
        .route("/notifications/{id}", delete(delete_notification))
        .route("/notifications/{id}/test", post(test_notification))
        .route("/notification-groups", get(list_groups))
        .route("/notification-groups", post(create_group))
        .route("/notification-groups/{id}", get(get_group))
        .route("/notification-groups/{id}", put(update_group))
        .route("/notification-groups/{id}", delete(delete_group))
}

#[utoipa::path(
    get,
    path = "/api/notifications",
    tag = "notifications",
    responses(
        (status = 200, description = "List all notifications", body = Vec<crate::entity::notification::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_notifications(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<crate::entity::notification::Model>>>, AppError> {
    let list = NotificationService::list(&state.db).await?;
    ok(list)
}

#[utoipa::path(
    get,
    path = "/api/notifications/{id}",
    tag = "notifications",
    params(("id" = String, Path, description = "Notification ID")),
    responses(
        (status = 200, description = "Notification details", body = crate::entity::notification::Model),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_notification(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<crate::entity::notification::Model>>, AppError> {
    let n = NotificationService::get(&state.db, &id).await?;
    ok(n)
}

#[utoipa::path(
    post,
    path = "/api/notifications",
    tag = "notifications",
    request_body = CreateNotification,
    responses(
        (status = 200, description = "Notification created", body = crate::entity::notification::Model),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn create_notification(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateNotification>,
) -> Result<Json<ApiResponse<crate::entity::notification::Model>>, AppError> {
    let n = NotificationService::create(&state.db, input).await?;
    ok(n)
}

#[utoipa::path(
    put,
    path = "/api/notifications/{id}",
    tag = "notifications",
    params(("id" = String, Path, description = "Notification ID")),
    request_body = UpdateNotification,
    responses(
        (status = 200, description = "Notification updated", body = crate::entity::notification::Model),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn update_notification(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateNotification>,
) -> Result<Json<ApiResponse<crate::entity::notification::Model>>, AppError> {
    let n = NotificationService::update(&state.db, &id, input).await?;
    ok(n)
}

#[utoipa::path(
    delete,
    path = "/api/notifications/{id}",
    tag = "notifications",
    params(("id" = String, Path, description = "Notification ID")),
    responses(
        (status = 200, description = "Notification deleted"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn delete_notification(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    NotificationService::delete(&state.db, &id).await?;
    ok("ok")
}

#[utoipa::path(
    post,
    path = "/api/notifications/{id}/test",
    tag = "notifications",
    params(("id" = String, Path, description = "Notification ID")),
    responses(
        (status = 200, description = "Test notification sent"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn test_notification(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    NotificationService::test_notification(&state.db, &id).await?;
    ok("ok")
}

#[utoipa::path(
    get,
    path = "/api/notification-groups",
    operation_id = "list_notification_groups",
    tag = "notification-groups",
    responses(
        (status = 200, description = "List all notification groups", body = Vec<crate::entity::notification_group::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_groups(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<crate::entity::notification_group::Model>>>, AppError> {
    let list = NotificationService::list_groups(&state.db).await?;
    ok(list)
}

#[utoipa::path(
    get,
    path = "/api/notification-groups/{id}",
    tag = "notification-groups",
    params(("id" = String, Path, description = "Notification group ID")),
    responses(
        (status = 200, description = "Notification group details", body = crate::entity::notification_group::Model),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn get_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<crate::entity::notification_group::Model>>, AppError> {
    let g = NotificationService::get_group(&state.db, &id).await?;
    ok(g)
}

#[utoipa::path(
    post,
    path = "/api/notification-groups",
    operation_id = "create_notification_group",
    tag = "notification-groups",
    request_body = CreateNotificationGroup,
    responses(
        (status = 200, description = "Notification group created", body = crate::entity::notification_group::Model),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn create_group(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateNotificationGroup>,
) -> Result<Json<ApiResponse<crate::entity::notification_group::Model>>, AppError> {
    let g = NotificationService::create_group(&state.db, input).await?;
    ok(g)
}

#[utoipa::path(
    put,
    path = "/api/notification-groups/{id}",
    operation_id = "update_notification_group",
    tag = "notification-groups",
    params(("id" = String, Path, description = "Notification group ID")),
    request_body = UpdateNotificationGroup,
    responses(
        (status = 200, description = "Notification group updated", body = crate::entity::notification_group::Model),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn update_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateNotificationGroup>,
) -> Result<Json<ApiResponse<crate::entity::notification_group::Model>>, AppError> {
    let g = NotificationService::update_group(&state.db, &id, input).await?;
    ok(g)
}

#[utoipa::path(
    delete,
    path = "/api/notification-groups/{id}",
    operation_id = "delete_notification_group",
    tag = "notification-groups",
    params(("id" = String, Path, description = "Notification group ID")),
    responses(
        (status = 200, description = "Notification group deleted"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn delete_group(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    NotificationService::delete_group(&state.db, &id).await?;
    ok("ok")
}
