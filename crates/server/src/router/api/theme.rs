use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::service::custom_theme::{
    ActiveThemeResponse, CreateThemeInput, CustomThemeService, Theme, ThemeSummary,
    UpdateThemeInput,
};
use crate::service::theme_ref::{self, ThemeReferences};
use crate::service::theme_validator::VarMap;
use crate::state::AppState;

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/settings/themes", get(list_themes))
        .route("/settings/themes/{id}", get(get_theme))
        .route("/settings/themes/{id}/export", get(export_theme))
        .route("/settings/active-theme", get(get_active_theme))
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/settings/themes", post(create_theme))
        .route("/settings/themes/{id}", put(update_theme))
        .route("/settings/themes/{id}", delete(delete_theme))
        .route("/settings/themes/{id}/references", get(get_references))
        .route("/settings/themes/{id}/duplicate", post(duplicate_theme))
        .route("/settings/themes/import", post(import_theme))
        .route("/settings/active-theme", put(put_active_theme))
}

#[utoipa::path(
    get,
    path = "/api/settings/themes",
    tag = "themes",
    responses(
        (status = 200, description = "List custom themes", body = Vec<ThemeSummary>),
        (status = 401, description = "Unauthenticated"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn list_themes(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<ThemeSummary>>>, AppError> {
    ok(CustomThemeService::list(&state.db).await?)
}

#[utoipa::path(
    get,
    path = "/api/settings/themes/{id}",
    tag = "themes",
    params(("id" = i32, Path, description = "Theme ID")),
    responses(
        (status = 200, description = "Custom theme", body = Theme),
        (status = 401, description = "Unauthenticated"),
        (status = 404, description = "Theme not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_theme(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Theme>>, AppError> {
    ok(CustomThemeService::get(&state.db, id).await?)
}

#[utoipa::path(
    get,
    path = "/api/settings/themes/{id}/export",
    tag = "themes",
    params(("id" = i32, Path, description = "Theme ID")),
    responses(
        (status = 200, description = "Exportable custom theme payload", body = ExportPayload),
        (status = 401, description = "Unauthenticated"),
        (status = 404, description = "Theme not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn export_theme(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<ExportPayload>>, AppError> {
    let theme = CustomThemeService::get(&state.db, id).await?;
    ok(ExportPayload {
        version: 1,
        name: theme.name,
        description: theme.description,
        based_on: theme.based_on,
        vars_light: theme.vars_light,
        vars_dark: theme.vars_dark,
    })
}

#[utoipa::path(
    get,
    path = "/api/settings/active-theme",
    tag = "themes",
    responses(
        (status = 200, description = "Resolved active admin theme", body = ActiveThemeResponse),
        (status = 401, description = "Unauthenticated"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_active_theme(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<ActiveThemeResponse>>, AppError> {
    ok(CustomThemeService::active_theme(&state.db, state.config.feature.custom_themes).await?)
}

#[utoipa::path(
    post,
    path = "/api/settings/themes",
    tag = "themes",
    request_body = CreateThemeInput,
    responses(
        (status = 200, description = "Custom theme created", body = Theme),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden (non-admin)"),
        (status = 422, description = "Validation error or custom themes disabled"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn create_theme(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<CreateThemeInput>,
) -> Result<Json<ApiResponse<Theme>>, AppError> {
    ensure_custom_themes_enabled(&state)?;
    ok(CustomThemeService::create(&state.db, input, &current_user.user_id).await?)
}

#[utoipa::path(
    put,
    path = "/api/settings/themes/{id}",
    tag = "themes",
    params(("id" = i32, Path, description = "Theme ID")),
    request_body = UpdateThemeInput,
    responses(
        (status = 200, description = "Custom theme updated", body = Theme),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden (non-admin)"),
        (status = 404, description = "Theme not found"),
        (status = 422, description = "Validation error or custom themes disabled"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn update_theme(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
    Json(input): Json<UpdateThemeInput>,
) -> Result<Json<ApiResponse<Theme>>, AppError> {
    ensure_custom_themes_enabled(&state)?;
    ok(CustomThemeService::update(&state.db, id, input).await?)
}

#[utoipa::path(
    delete,
    path = "/api/settings/themes/{id}",
    tag = "themes",
    params(("id" = i32, Path, description = "Theme ID")),
    responses(
        (status = 200, description = "Custom theme deleted"),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden (non-admin)"),
        (status = 404, description = "Theme not found"),
        (status = 409, description = "Theme is referenced"),
        (status = 422, description = "Custom themes disabled"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn delete_theme(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    ensure_custom_themes_enabled(&state)?;
    CustomThemeService::delete(&state.db, id).await?;
    ok("ok")
}

#[utoipa::path(
    get,
    path = "/api/settings/themes/{id}/references",
    tag = "themes",
    params(("id" = i32, Path, description = "Theme ID")),
    responses(
        (status = 200, description = "Theme references", body = ThemeReferences),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden (non-admin)"),
        (status = 422, description = "Invalid theme ID"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_references(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<ThemeReferences>>, AppError> {
    ok(theme_ref::list_references(&state.db, id).await?)
}

#[utoipa::path(
    post,
    path = "/api/settings/themes/{id}/duplicate",
    tag = "themes",
    params(("id" = i32, Path, description = "Theme ID")),
    responses(
        (status = 200, description = "Custom theme duplicated", body = Theme),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden (non-admin)"),
        (status = 404, description = "Theme not found"),
        (status = 422, description = "Validation error or custom themes disabled"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn duplicate_theme(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<Theme>>, AppError> {
    ensure_custom_themes_enabled(&state)?;
    ok(CustomThemeService::duplicate(&state.db, id, &current_user.user_id).await?)
}

#[utoipa::path(
    post,
    path = "/api/settings/themes/import",
    tag = "themes",
    request_body = ExportPayload,
    responses(
        (status = 200, description = "Custom theme imported", body = Theme),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden (non-admin)"),
        (status = 422, description = "Validation error or custom themes disabled"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn import_theme(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(input): Json<ExportPayload>,
) -> Result<Json<ApiResponse<Theme>>, AppError> {
    ensure_custom_themes_enabled(&state)?;
    if input.version != 1 {
        return Err(AppError::Validation(format!(
            "unsupported theme export version: {}",
            input.version
        )));
    }

    ok(CustomThemeService::create(
        &state.db,
        CreateThemeInput {
            name: input.name,
            description: input.description,
            based_on: input.based_on,
            vars_light: input.vars_light,
            vars_dark: input.vars_dark,
        },
        &current_user.user_id,
    )
    .await?)
}

#[utoipa::path(
    put,
    path = "/api/settings/active-theme",
    tag = "themes",
    request_body = PutActiveThemeInput,
    responses(
        (status = 200, description = "Resolved active admin theme", body = ActiveThemeResponse),
        (status = 401, description = "Unauthenticated"),
        (status = 403, description = "Forbidden (non-admin)"),
        (status = 422, description = "Validation error or custom themes disabled"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn put_active_theme(
    State(state): State<Arc<AppState>>,
    Json(input): Json<PutActiveThemeInput>,
) -> Result<Json<ApiResponse<ActiveThemeResponse>>, AppError> {
    ok(CustomThemeService::set_active_theme(
        &state.db,
        &input.r#ref,
        state.config.feature.custom_themes,
    )
    .await?)
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PutActiveThemeInput {
    pub r#ref: String,
}

#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExportPayload {
    pub version: u32,
    pub name: String,
    pub description: Option<String>,
    pub based_on: Option<String>,
    pub vars_light: VarMap,
    pub vars_dark: VarMap,
}

fn ensure_custom_themes_enabled(state: &AppState) -> Result<(), AppError> {
    if state.config.feature.custom_themes {
        Ok(())
    } else {
        Err(AppError::Validation(
            "custom theme feature disabled".to_string(),
        ))
    }
}
