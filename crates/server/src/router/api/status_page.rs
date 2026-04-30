use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use sea_orm::*;
use serde::Serialize;

use crate::entity::{incident, incident_update, maintenance, server, server_group, status_page};
use crate::error::{ApiResponse, AppError, ok};
use crate::service::custom_theme::{CustomThemeService, ThemeResolved};
use crate::service::incident::IncidentService;
use crate::service::maintenance::MaintenanceService;
use crate::service::status_page::{CreateStatusPage, StatusPageService, UpdateStatusPage};
use crate::service::theme_ref::ThemeRef;
use crate::service::uptime::{UptimeDailyEntry, UptimeService};
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Public response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StatusPageInfo {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub description: Option<String>,
    pub group_by_server_group: bool,
    pub show_values: bool,
    pub custom_css: Option<String>,
    pub uptime_yellow_threshold: f64,
    pub uptime_red_threshold: f64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ServerStatusInfo {
    #[serde(rename = "server_id")]
    pub id: String,
    #[serde(rename = "server_name")]
    pub name: String,
    pub region: Option<String>,
    pub country_code: Option<String>,
    pub os: Option<String>,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub online: bool,
    #[serde(rename = "uptime_percent")]
    pub uptime_percentage: Option<f64>,
    pub uptime_daily: Vec<UptimeDailyEntry>,
    pub in_maintenance: bool,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct IncidentWithUpdates {
    #[serde(flatten)]
    pub incident: incident::Model,
    pub updates: Vec<incident_update::Model>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct PublicStatusPageData {
    pub page: StatusPageInfo,
    pub theme: ThemeResolved,
    pub servers: Vec<ServerStatusInfo>,
    pub active_incidents: Vec<IncidentWithUpdates>,
    pub planned_maintenances: Vec<maintenance::Model>,
    pub recent_incidents: Vec<IncidentWithUpdates>,
}

// ---------------------------------------------------------------------------
// Routers
// ---------------------------------------------------------------------------

/// Public route (no auth required).
pub fn public_router() -> Router<Arc<AppState>> {
    Router::new().route("/status/{slug}", get(get_public_status_page))
}

/// Read routes accessible to all authenticated users.
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new().route("/status-pages", get(list_status_pages))
}

/// Write routes restricted to admin users only.
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/status-pages", post(create_status_page))
        .route("/status-pages/{id}", put(update_status_page))
        .route("/status-pages/{id}", delete(delete_status_page))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/status/{slug}",
    operation_id = "get_public_status_page",
    tag = "status-pages",
    params(("slug" = String, Path, description = "Status page URL slug")),
    responses(
        (status = 200, description = "Public status page data", body = PublicStatusPageData),
        (status = 404, description = "Not found"),
    )
)]
pub async fn get_public_status_page(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> Result<Json<ApiResponse<PublicStatusPageData>>, AppError> {
    let page = StatusPageService::get_by_slug(&state.db, &slug).await?;

    if !page.enabled {
        return Err(AppError::NotFound(format!(
            "Status page with slug '{slug}' not found"
        )));
    }

    let page_id = page.id.clone();
    let theme = resolve_public_status_page_theme(
        &state.db,
        page.theme_ref.as_deref(),
        state.config.feature.custom_themes,
    )
    .await?;

    // Parse server IDs from the page
    let server_ids: Vec<String> = serde_json::from_str(&page.server_ids_json).unwrap_or_default();

    // Fetch servers
    let servers = if server_ids.is_empty() {
        vec![]
    } else {
        server::Entity::find()
            .filter(server::Column::Id.is_in(server_ids))
            .order_by_desc(server::Column::Weight)
            .order_by_desc(server::Column::CreatedAt)
            .all(&state.db)
            .await?
    };

    // Fetch groups for labeling
    let groups: Vec<server_group::Model> = server_group::Entity::find().all(&state.db).await?;
    let group_map: std::collections::HashMap<String, String> =
        groups.into_iter().map(|g| (g.id, g.name)).collect();

    // Build server status info
    let mut server_infos = Vec::new();
    for s in &servers {
        let online = state.agent_manager.is_online(&s.id);
        let daily = UptimeService::get_daily_filled(&state.db, &s.id, 90).await?;

        // Compute uptime percentage from daily data
        let total_minutes: i64 = daily.iter().map(|d| d.total_minutes as i64).sum();
        let online_minutes: i64 = daily.iter().map(|d| d.online_minutes as i64).sum();
        let uptime_percentage = if total_minutes == 0 {
            None
        } else {
            Some((online_minutes as f64 / total_minutes as f64) * 100.0)
        };

        let in_maintenance = MaintenanceService::is_in_maintenance(&state.db, &s.id)
            .await
            .unwrap_or(false);

        server_infos.push(ServerStatusInfo {
            id: s.id.clone(),
            name: s.name.clone(),
            region: s.region.clone(),
            country_code: s.country_code.clone(),
            os: s.os.clone(),
            group_id: s.group_id.clone(),
            group_name: s
                .group_id
                .as_ref()
                .and_then(|gid| group_map.get(gid).cloned()),
            online,
            uptime_percentage,
            uptime_daily: daily,
            in_maintenance,
        });
    }

    // Fetch active incidents (not resolved) linked to this status page
    let all_incidents = IncidentService::list(&state.db, None).await?;
    let now = Utc::now();
    let seven_days_ago = now - Duration::days(7);

    let mut active_incidents = Vec::new();
    let mut recent_incidents = Vec::new();

    for inc in all_incidents {
        // Check if incident is linked to this status page
        let linked = match &inc.status_page_ids_json {
            Some(json) => {
                let ids: Vec<String> = serde_json::from_str(json).unwrap_or_default();
                ids.is_empty() || ids.iter().any(|id| id == &page_id)
            }
            None => true, // No filter means applies to all pages
        };

        if !linked {
            continue;
        }

        let updates = IncidentService::list_updates(&state.db, &inc.id)
            .await
            .unwrap_or_default();

        if inc.status != "resolved" {
            active_incidents.push(IncidentWithUpdates {
                incident: inc,
                updates,
            });
        } else if let Some(resolved_at) = inc.resolved_at
            && resolved_at >= seven_days_ago
        {
            recent_incidents.push(IncidentWithUpdates {
                incident: inc,
                updates,
            });
        }
    }

    // Fetch planned maintenances (active, end_at >= now) linked to this page
    let all_maintenances = MaintenanceService::list(&state.db).await?;
    let planned_maintenances: Vec<maintenance::Model> = all_maintenances
        .into_iter()
        .filter(|m| {
            if !m.active || m.end_at < now {
                return false;
            }
            match &m.status_page_ids_json {
                Some(json) => {
                    let ids: Vec<String> = serde_json::from_str(json).unwrap_or_default();
                    ids.is_empty() || ids.iter().any(|id| id == &page_id)
                }
                None => true,
            }
        })
        .collect();

    let page_info = StatusPageInfo {
        id: page.id,
        title: page.title,
        slug: page.slug,
        description: page.description,
        group_by_server_group: page.group_by_server_group,
        show_values: page.show_values,
        custom_css: page.custom_css,
        uptime_yellow_threshold: page.uptime_yellow_threshold,
        uptime_red_threshold: page.uptime_red_threshold,
    };

    ok(PublicStatusPageData {
        page: page_info,
        theme,
        servers: server_infos,
        active_incidents,
        planned_maintenances,
        recent_incidents,
    })
}

async fn resolve_public_status_page_theme(
    db: &DatabaseConnection,
    page_theme_ref: Option<&str>,
    feature_enabled: bool,
) -> Result<ThemeResolved, AppError> {
    let Some(raw_ref) = page_theme_ref else {
        return Ok(CustomThemeService::active_theme(db, feature_enabled)
            .await?
            .theme);
    };

    let parsed = match ThemeRef::parse(raw_ref) {
        Ok(parsed) => parsed,
        Err(_) => return default_theme(db).await,
    };
    let resolved_ref = if !feature_enabled && matches!(parsed, ThemeRef::Custom(_)) {
        ThemeRef::Preset("default".to_string())
    } else {
        parsed
    };

    match CustomThemeService::resolve(db, resolved_ref).await {
        Ok(response) => Ok(response.theme),
        Err(AppError::NotFound(_)) => default_theme(db).await,
        Err(err) => Err(err),
    }
}

async fn default_theme(db: &DatabaseConnection) -> Result<ThemeResolved, AppError> {
    Ok(
        CustomThemeService::resolve(db, ThemeRef::Preset("default".to_string()))
            .await?
            .theme,
    )
}

#[utoipa::path(
    get,
    path = "/api/status-pages",
    operation_id = "list_status_pages",
    tag = "status-pages",
    responses(
        (status = 200, description = "List all status pages", body = Vec<status_page::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list_status_pages(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<status_page::Model>>>, AppError> {
    let pages = StatusPageService::list(&state.db).await?;
    ok(pages)
}

#[utoipa::path(
    post,
    path = "/api/status-pages",
    operation_id = "create_status_page",
    tag = "status-pages",
    request_body = CreateStatusPage,
    responses(
        (status = 200, description = "Status page created", body = status_page::Model),
        (status = 409, description = "Slug conflict"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn create_status_page(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateStatusPage>,
) -> Result<Json<ApiResponse<status_page::Model>>, AppError> {
    let page = StatusPageService::create(&state.db, input).await?;
    ok(page)
}

#[utoipa::path(
    put,
    path = "/api/status-pages/{id}",
    operation_id = "update_status_page",
    tag = "status-pages",
    params(("id" = String, Path, description = "Status page ID")),
    request_body = UpdateStatusPage,
    responses(
        (status = 200, description = "Status page updated", body = status_page::Model),
        (status = 404, description = "Not found"),
        (status = 409, description = "Slug conflict"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn update_status_page(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<UpdateStatusPage>,
) -> Result<Json<ApiResponse<status_page::Model>>, AppError> {
    let page = StatusPageService::update(&state.db, &id, input).await?;
    ok(page)
}

#[utoipa::path(
    delete,
    path = "/api/status-pages/{id}",
    operation_id = "delete_status_page",
    tag = "status-pages",
    params(("id" = String, Path, description = "Status page ID")),
    responses(
        (status = 200, description = "Status page deleted"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn delete_status_page(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    StatusPageService::delete(&state.db, &id).await?;
    ok("ok")
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, Set};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::config::AppConfig;
    use crate::entity::status_page;
    use crate::router::create_router;
    use crate::service::custom_theme::{CreateThemeInput, CustomThemeService};
    use crate::service::status_page::{CreateStatusPage, StatusPageService, UpdateStatusPage};
    use crate::service::theme_validator::{REQUIRED_VARS, VarMap};
    use crate::state::AppState;
    use crate::test_utils::setup_test_db;

    use super::*;

    fn valid_vars() -> VarMap {
        REQUIRED_VARS
            .iter()
            .map(|key| ((*key).to_string(), "oklch(0.5 0.1 180)".to_string()))
            .collect()
    }

    fn create_page_input(slug: &str) -> CreateStatusPage {
        CreateStatusPage {
            title: "Status".to_string(),
            slug: slug.to_string(),
            description: None,
            server_ids_json: Vec::new(),
            group_by_server_group: None,
            show_values: None,
            custom_css: None,
            enabled: None,
            uptime_yellow_threshold: None,
            uptime_red_threshold: None,
        }
    }

    fn empty_update() -> UpdateStatusPage {
        UpdateStatusPage {
            title: None,
            slug: None,
            description: None,
            server_ids_json: None,
            group_by_server_group: None,
            show_values: None,
            custom_css: None,
            enabled: None,
            uptime_yellow_threshold: None,
            uptime_red_threshold: None,
            theme_ref: None,
        }
    }

    async fn create_theme(db: &DatabaseConnection, name: &str) -> i32 {
        CustomThemeService::create(
            db,
            CreateThemeInput {
                name: name.to_string(),
                description: None,
                based_on: Some("default".to_string()),
                vars_light: valid_vars(),
                vars_dark: valid_vars(),
            },
            "status-page-router-test",
        )
        .await
        .expect("theme should be created")
        .id
    }

    async fn build_app(db: DatabaseConnection, config: AppConfig) -> axum::Router {
        let state = AppState::new(db, config)
            .await
            .expect("app state should initialize");
        create_router(state)
    }

    async fn get_json(app: axum::Router, uri: &str) -> (StatusCode, Value) {
        let response = app
            .oneshot(
                Request::builder()
                    .method(axum::http::Method::GET)
                    .uri(uri)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let body = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));

        (status, body)
    }

    async fn set_status_page_theme_ref_direct(
        db: &DatabaseConnection,
        page_id: &str,
        theme_ref: &str,
    ) {
        let page = status_page::Entity::find_by_id(page_id)
            .one(db)
            .await
            .expect("status page lookup should succeed")
            .expect("status page should exist");
        let mut active: status_page::ActiveModel = page.into();
        active.theme_ref = Set(Some(theme_ref.to_string()));
        active
            .update(db)
            .await
            .expect("status page theme ref should update directly");
    }

    #[tokio::test]
    async fn public_status_page_returns_page_specific_custom_theme() {
        let (db, _tmp) = setup_test_db().await;
        let page = StatusPageService::create(&db, create_page_input("page-specific"))
            .await
            .expect("status page should be created");
        let theme_id = create_theme(&db, "Ocean").await;
        let mut input = empty_update();
        input.theme_ref = Some(Some(format!("custom:{theme_id}")));
        StatusPageService::update(&db, &page.id, input)
            .await
            .expect("status page theme ref should update");
        let app = build_app(db, AppConfig::default()).await;

        let (status, body) = get_json(app, "/api/status/page-specific").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["data"]["theme"],
            json!({
                "kind": "custom",
                "id": theme_id,
                "name": "Ocean",
                "vars_light": valid_vars(),
                "vars_dark": valid_vars(),
                "updated_at": body["data"]["theme"]["updated_at"],
            })
        );
    }

    #[tokio::test]
    async fn public_status_page_falls_back_to_global_active_theme_when_page_theme_ref_is_null() {
        let (db, _tmp) = setup_test_db().await;
        StatusPageService::create(&db, create_page_input("global-theme"))
            .await
            .expect("status page should be created");
        let theme_id = create_theme(&db, "Global").await;
        CustomThemeService::set_active_theme(&db, &format!("custom:{theme_id}"), true)
            .await
            .expect("active theme should update");
        let app = build_app(db, AppConfig::default()).await;

        let (status, body) = get_json(app, "/api/status/global-theme").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"]["theme"]["kind"], "custom");
        assert_eq!(body["data"]["theme"]["id"], theme_id);
        assert_eq!(body["data"]["theme"]["name"], "Global");
    }

    #[tokio::test]
    async fn public_status_page_falls_back_to_default_for_dangling_and_malformed_page_refs() {
        let (db, _tmp) = setup_test_db().await;
        db.execute_unprepared(
            "DROP TRIGGER IF EXISTS trg_custom_theme_status_page_update_ref_exists",
        )
        .await
        .expect("status page update trigger should be dropped");
        let dangling = StatusPageService::create(&db, create_page_input("dangling-theme"))
            .await
            .expect("status page should be created");
        let malformed = StatusPageService::create(&db, create_page_input("malformed-theme"))
            .await
            .expect("status page should be created");
        set_status_page_theme_ref_direct(&db, &dangling.id, "custom:999").await;
        set_status_page_theme_ref_direct(&db, &malformed.id, "theme:nope").await;
        let app = build_app(db, AppConfig::default()).await;

        for uri in ["/api/status/dangling-theme", "/api/status/malformed-theme"] {
            let (status, body) = get_json(app.clone(), uri).await;
            assert_eq!(status, StatusCode::OK, "{uri}");
            assert_eq!(
                body["data"]["theme"],
                json!({
                    "kind": "preset",
                    "id": "default",
                }),
                "{uri}"
            );
        }
    }

    #[tokio::test]
    async fn public_status_page_coerces_custom_theme_to_default_when_feature_is_disabled() {
        let (db, _tmp) = setup_test_db().await;
        let page = StatusPageService::create(&db, create_page_input("disabled-feature"))
            .await
            .expect("status page should be created");
        let theme_id = create_theme(&db, "Disabled").await;
        let mut input = empty_update();
        input.theme_ref = Some(Some(format!("custom:{theme_id}")));
        StatusPageService::update(&db, &page.id, input)
            .await
            .expect("status page theme ref should update");
        let mut config = AppConfig::default();
        config.feature.custom_themes = false;
        let app = build_app(db, config).await;

        let (status, body) = get_json(app, "/api/status/disabled-feature").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["data"]["theme"],
            json!({
                "kind": "preset",
                "id": "default",
            })
        );
    }
}
