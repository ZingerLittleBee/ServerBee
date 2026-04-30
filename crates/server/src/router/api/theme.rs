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
        .route("/settings/themes/import", post(import_theme))
        .route("/settings/themes/{id}", put(update_theme))
        .route("/settings/themes/{id}", delete(delete_theme))
        .route("/settings/themes/{id}/references", get(get_references))
        .route("/settings/themes/{id}/duplicate", post(duplicate_theme))
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
        (status = 404, description = "Theme not found"),
        (status = 422, description = "Invalid theme ID"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_references(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<ThemeReferences>>, AppError> {
    if id > 0 {
        CustomThemeService::get(&state.db, id).await?;
    }

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

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::config::AppConfig;
    use crate::router::create_router;
    use crate::service::auth::{AuthService, LoginParams};
    use crate::service::theme_validator::REQUIRED_VARS;
    use crate::state::AppState;
    use crate::test_utils::setup_test_db;

    fn valid_vars() -> Value {
        REQUIRED_VARS
            .iter()
            .map(|key| ((*key).to_string(), json!("oklch(0.5 0.1 180)")))
            .collect()
    }

    fn valid_theme_body(name: &str) -> Value {
        json!({
            "name": name,
            "description": "Router test theme",
            "based_on": "default",
            "vars_light": valid_vars(),
            "vars_dark": valid_vars(),
        })
    }

    fn import_body(version: u32, name: &str) -> Value {
        let mut body = valid_theme_body(name);
        body.as_object_mut()
            .expect("theme body must be an object")
            .insert("version".to_string(), json!(version));
        body
    }

    async fn build_app(db: sea_orm::DatabaseConnection, config: AppConfig) -> axum::Router {
        let state = AppState::new(db, config)
            .await
            .expect("app state should initialize");
        create_router(state)
    }

    async fn session_cookie(
        db: &sea_orm::DatabaseConnection,
        username: &str,
        role: &str,
    ) -> String {
        AuthService::create_user(db, username, "password123", role)
            .await
            .expect("user should be created");
        let (session, _) = AuthService::login(
            db,
            LoginParams {
                username,
                password: "password123",
                totp_code: None,
                ip: "127.0.0.1",
                user_agent: "theme-router-test",
                session_ttl: 3600,
            },
        )
        .await
        .expect("login should create a session");

        format!("session_token={}", session.token)
    }

    async fn request_json(
        app: axum::Router,
        method: axum::http::Method,
        uri: &str,
        cookie: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }

        let request_body = if let Some(value) = body {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(serde_json::to_vec(&value).expect("body should serialize"))
        } else {
            Body::empty()
        };
        let response = app
            .oneshot(builder.body(request_body).expect("request should build"))
            .await
            .expect("router should respond");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };

        (status, body)
    }

    #[tokio::test]
    async fn unauthenticated_list_themes_returns_401() {
        let (db, _tmp) = setup_test_db().await;
        let app = build_app(db, AppConfig::default()).await;

        let (status, _) = request_json(
            app,
            axum::http::Method::GET,
            "/api/settings/themes",
            None,
            None,
        )
        .await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn member_can_read_themes_but_cannot_create_theme() {
        let (db, _tmp) = setup_test_db().await;
        let cookie = session_cookie(&db, "theme_member", "member").await;
        let app = build_app(db, AppConfig::default()).await;

        let (read_status, read_body) = request_json(
            app.clone(),
            axum::http::Method::GET,
            "/api/settings/themes",
            Some(&cookie),
            None,
        )
        .await;
        let (write_status, _) = request_json(
            app,
            axum::http::Method::POST,
            "/api/settings/themes",
            Some(&cookie),
            Some(valid_theme_body("Member blocked")),
        )
        .await;

        assert_eq!(read_status, StatusCode::OK);
        assert_eq!(read_body["data"], json!([]));
        assert_eq!(write_status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn admin_can_create_theme_through_router() {
        let (db, _tmp) = setup_test_db().await;
        let cookie = session_cookie(&db, "theme_admin", "admin").await;
        let app = build_app(db, AppConfig::default()).await;

        let (status, body) = request_json(
            app,
            axum::http::Method::POST,
            "/api/settings/themes",
            Some(&cookie),
            Some(valid_theme_body("Admin theme")),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["name"], "Admin theme");
        assert!(body["data"]["id"].as_i64().expect("theme id should be set") > 0);
    }

    #[tokio::test]
    async fn disabled_custom_themes_reject_admin_mutations() {
        let (db, _tmp) = setup_test_db().await;
        let cookie = session_cookie(&db, "theme_disabled_admin", "admin").await;
        let enabled_app = build_app(db.clone(), AppConfig::default()).await;
        let (_, created) = request_json(
            enabled_app,
            axum::http::Method::POST,
            "/api/settings/themes",
            Some(&cookie),
            Some(valid_theme_body("Existing theme")),
        )
        .await;
        let id = created["data"]["id"]
            .as_i64()
            .expect("created theme should have id");

        let mut disabled = AppConfig::default();
        disabled.feature.custom_themes = false;
        let app = build_app(db, disabled).await;

        for (method, uri, body) in [
            (
                axum::http::Method::POST,
                "/api/settings/themes".to_string(),
                Some(valid_theme_body("Disabled create")),
            ),
            (
                axum::http::Method::PUT,
                format!("/api/settings/themes/{id}"),
                Some(valid_theme_body("Disabled update")),
            ),
            (
                axum::http::Method::DELETE,
                format!("/api/settings/themes/{id}"),
                None,
            ),
            (
                axum::http::Method::POST,
                format!("/api/settings/themes/{id}/duplicate"),
                None,
            ),
            (
                axum::http::Method::POST,
                "/api/settings/themes/import".to_string(),
                Some(import_body(1, "Disabled import")),
            ),
        ] {
            let (status, response_body) =
                request_json(app.clone(), method, &uri, Some(&cookie), body).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
            assert_eq!(
                response_body["error"]["message"],
                "Validation error: custom theme feature disabled",
                "{uri}"
            );
        }
    }

    #[tokio::test]
    async fn import_static_route_is_not_captured_as_theme_id() {
        let (db, _tmp) = setup_test_db().await;
        let cookie = session_cookie(&db, "theme_import_admin", "admin").await;
        let app = build_app(db, AppConfig::default()).await;

        let (status, body) = request_json(
            app,
            axum::http::Method::POST,
            "/api/settings/themes/import",
            Some(&cookie),
            Some(import_body(2, "Wrong version")),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body["error"]["message"],
            "Validation error: unsupported theme export version: 2"
        );
    }

    #[tokio::test]
    async fn references_for_missing_theme_returns_404() {
        let (db, _tmp) = setup_test_db().await;
        let cookie = session_cookie(&db, "theme_refs_admin", "admin").await;
        let app = build_app(db, AppConfig::default()).await;

        let (status, _) = request_json(
            app,
            axum::http::Method::GET,
            "/api/settings/themes/404/references",
            Some(&cookie),
            None,
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
