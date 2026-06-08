use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post, put};
use axum::{Extension, Json, Router};

use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::utils::extract_client_ip;
use crate::service::audit::AuditService;
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
    Extension(actor): Extension<CurrentUser>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<CreateUserInput>,
) -> Result<Json<ApiResponse<UserResponse>>, AppError> {
    let user =
        UserService::create_user(&state.db, &body.username, &body.password, &body.role).await?;

    // Audit: account creation (a member or a stealth admin) is privilege-sensitive.
    let caller_ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &actor.user_id,
        "user.create",
        Some(&format!(
            "id={} username={} role={}",
            user.id, user.username, user.role
        )),
        &caller_ip,
    )
    .await;

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
    Extension(actor): Extension<CurrentUser>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpdateUserInput>,
) -> Result<Json<ApiResponse<UserResponse>>, AppError> {
    let password_reset = body.password.is_some();
    let old_role = UserService::get_user(&state.db, &id).await?.role;
    let user = UserService::update_user(&state.db, &id, body).await?;

    // Audit: role promotion/demotion and password resets are privilege-sensitive.
    let role_change = if old_role == user.role {
        format!("role={}", user.role)
    } else {
        format!("role {old_role}->{}", user.role)
    };
    let caller_ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &actor.user_id,
        "user.update",
        Some(&format!(
            "id={} username={} {role_change} password_reset={password_reset}",
            user.id, user.username
        )),
        &caller_ip,
    )
    .await;

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
    Extension(actor): Extension<CurrentUser>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    // Capture the target's identity before deletion so the audit entry is meaningful.
    let target = UserService::get_user(&state.db, &id).await?;
    UserService::delete_user(&state.db, &id).await?;

    let caller_ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &actor.user_id,
        "user.delete",
        Some(&format!(
            "id={} username={} role={}",
            target.id, target.username, target.role
        )),
        &caller_ip,
    )
    .await;

    ok("ok")
}
