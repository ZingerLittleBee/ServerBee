use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};

use crate::error::{ApiResponse, AppError, ok};
use crate::service::user::{CreateUserInput, UpdateUserInput, UserResponse, UserService};
use crate::state::AppState;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/users", get(list_users))
        .route("/users", post(create_user))
        .route("/users/{id}", get(get_user))
        .route("/users/{id}", put(update_user))
        .route("/users/{id}", delete(delete_user))
}

#[utoipa::path(
    get,
    path = "/api/users",
    tag = "users",
    responses(
        (status = 200, description = "List all users", body = Vec<UserResponse>),
        (status = 403, description = "Forbidden — admin only"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list_users(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<UserResponse>>>, AppError> {
    let users = UserService::list_users(&state.db).await?;
    let response: Vec<UserResponse> = users.into_iter().map(UserResponse::from).collect();
    ok(response)
}

#[utoipa::path(
    get,
    path = "/api/users/{id}",
    tag = "users",
    params(("id" = String, Path, description = "User ID")),
    responses(
        (status = 200, description = "User details", body = UserResponse),
        (status = 403, description = "Forbidden — admin only"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<UserResponse>>, AppError> {
    let user = UserService::get_user(&state.db, &id).await?;
    ok(UserResponse::from(user))
}

#[utoipa::path(
    post,
    path = "/api/users",
    tag = "users",
    request_body = CreateUserInput,
    responses(
        (status = 200, description = "User created", body = UserResponse),
        (status = 403, description = "Forbidden — admin only"),
        (status = 409, description = "Username already exists"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateUserInput>,
) -> Result<Json<ApiResponse<UserResponse>>, AppError> {
    let user =
        UserService::create_user(&state.db, &body.username, &body.password, &body.role).await?;
    ok(UserResponse::from(user))
}

#[utoipa::path(
    put,
    path = "/api/users/{id}",
    tag = "users",
    params(("id" = String, Path, description = "User ID")),
    request_body = UpdateUserInput,
    responses(
        (status = 200, description = "User updated", body = UserResponse),
        (status = 403, description = "Forbidden — admin only"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn update_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateUserInput>,
) -> Result<Json<ApiResponse<UserResponse>>, AppError> {
    let user = UserService::update_user(&state.db, &id, body).await?;
    ok(UserResponse::from(user))
}

#[utoipa::path(
    delete,
    path = "/api/users/{id}",
    tag = "users",
    params(("id" = String, Path, description = "User ID")),
    responses(
        (status = 200, description = "User deleted"),
        (status = 400, description = "Cannot delete last admin"),
        (status = 403, description = "Forbidden — admin only"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    UserService::delete_user(&state.db, &id).await?;
    ok("ok")
}
