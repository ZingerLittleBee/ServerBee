use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use sea_orm::*;
use serde::Serialize;

use crate::entity::{incident, incident_update, maintenance, server, server_group, status_page};
use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::resolve_optional_user;
use crate::service::custom_theme::{CustomThemeService, ThemeResolved};
use crate::service::incident::IncidentService;
use crate::service::ip_quality::{IpQualityService, ServerIpQualityData};
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
    /// Present only when the page has `show_ip_quality = true`.
    /// IPs are masked to `*.*.*.*` for unauthenticated viewers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_quality: Option<Vec<ServerIpQualityData>>,
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
    headers: HeaderMap,
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

    // Parse server IDs from the page exactly once; reused below for both the
    // server query and the optional IP-quality block.
    let server_ids: Vec<String> = serde_json::from_str(&page.server_ids_json).unwrap_or_default();

    // Fetch servers
    let servers = if server_ids.is_empty() {
        vec![]
    } else {
        server::Entity::find()
            .filter(server::Column::Id.is_in(server_ids.iter()))
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

    // Determine if the request is authenticated (optional auth for IP masking).
    let authenticated = resolve_optional_user(&headers, &state).await.is_some();

    // Build the optional IP quality block (only when the page opts in).
    let ip_quality = if page.show_ip_quality {
        // Scope strictly to servers that still exist — a deleted server left
        // dangling in `server_ids_json` must not leak a stale ID publicly.
        let existing_ids: Vec<String> = servers.iter().map(|s| s.id.clone()).collect();
        let scoped_ids: Vec<String> = server_ids
            .iter()
            .filter(|id| existing_ids.iter().any(|e| e == *id))
            .cloned()
            .collect();

        let mut entries = IpQualityService::get_summaries(&state.db, &scoped_ids).await?;
        if !authenticated {
            // Mask the egress IP for unauthenticated viewers.
            for entry in &mut entries {
                if let Some(ref mut snap) = entry.ip_quality {
                    snap.ip = "*.*.*.*".to_string();
                }
            }
        }
        Some(entries)
    } else {
        None
    };

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
        ip_quality,
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
    if !feature_enabled && matches!(parsed, ThemeRef::Custom(_)) {
        return Ok(CustomThemeService::active_theme(db, feature_enabled)
            .await?
            .theme);
    }

    match CustomThemeService::resolve(db, parsed).await {
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
    ensure_status_page_theme_ref_allowed(&input, state.config.feature.custom_themes)?;
    let page = StatusPageService::update(&state.db, &id, input).await?;
    ok(page)
}

fn ensure_status_page_theme_ref_allowed(
    input: &UpdateStatusPage,
    feature_enabled: bool,
) -> Result<(), AppError> {
    let Some(Some(raw_ref)) = input.theme_ref.as_ref() else {
        return Ok(());
    };
    let parsed = ThemeRef::parse(raw_ref)?;
    if !feature_enabled && matches!(parsed, ThemeRef::Custom(_)) {
        return Err(AppError::Validation(
            "custom theme feature disabled".to_string(),
        ));
    }

    Ok(())
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
    use axum::http::{Request, StatusCode, header};
    use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, Set};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::config::AppConfig;
    use crate::entity::status_page;
    use crate::router::create_router;
    use crate::service::auth::{AuthService, LoginParams};
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
            show_ip_quality: None,
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
            show_ip_quality: None,
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

    async fn session_cookie(db: &DatabaseConnection, username: &str) -> String {
        AuthService::create_user(db, username, "password123", "admin")
            .await
            .expect("admin user should be created");
        let (session, _) = AuthService::login(
            db,
            LoginParams {
                username,
                password: "password123",
                totp_code: None,
                ip: "127.0.0.1",
                user_agent: "status-page-router-test",
                session_ttl: 3600,
            },
        )
        .await
        .expect("login should create a session");

        format!("session_token={}", session.token)
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
    async fn public_status_page_follows_global_theme_for_custom_page_ref_when_feature_is_disabled()
    {
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
        CustomThemeService::set_active_theme(&db, "preset:nord", true)
            .await
            .expect("active preset should update");
        let mut config = AppConfig::default();
        config.feature.custom_themes = false;
        let app = build_app(db, config).await;

        let (status, body) = get_json(app, "/api/status/disabled-feature").await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["data"]["theme"],
            json!({
                "kind": "preset",
                "id": "nord",
            })
        );
    }

    // -----------------------------------------------------------------------
    // Task 15: IP quality on public status page + guest masking
    // -----------------------------------------------------------------------

    async fn create_server(db: &DatabaseConnection, id: &str) {
        use crate::service::auth::AuthService;
        use serverbee_common::constants::CAP_DEFAULT;
        let token_hash = AuthService::hash_password("tok").expect("hash");
        let now = chrono::Utc::now();
        crate::entity::server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(token_hash),
            token_prefix: Set("serverbee_test".to_string()),
            name: Set(id.to_string()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert test server");
    }

    async fn insert_snapshot(db: &DatabaseConnection, server_id: &str, ip: &str) {
        use crate::entity::ip_quality_snapshot;
        use uuid::Uuid;
        let now = chrono::Utc::now();
        ip_quality_snapshot::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            server_id: Set(server_id.to_string()),
            ip: Set(ip.to_string()),
            asn: Set(None),
            as_org: Set(None),
            country: Set(Some("US".to_string())),
            region: Set(None),
            city: Set(None),
            ip_type: Set("residential".to_string()),
            is_proxy: Set(false),
            is_vpn: Set(false),
            is_hosting: Set(false),
            risk_score: Set(None),
            risk_level: Set("unknown".to_string()),
            checked_at: Set(now),
        }
        .insert(db)
        .await
        .expect("insert snapshot");
    }

    fn create_page_with_ip_quality(slug: &str, show_ip_quality: bool, server_ids: Vec<String>) -> CreateStatusPage {
        CreateStatusPage {
            title: "Status".to_string(),
            slug: slug.to_string(),
            description: None,
            server_ids_json: server_ids,
            group_by_server_group: None,
            show_values: None,
            custom_css: None,
            enabled: None,
            uptime_yellow_threshold: None,
            uptime_red_threshold: None,
            show_ip_quality: Some(show_ip_quality),
        }
    }

    #[tokio::test]
    async fn status_page_with_show_ip_quality_false_omits_ip_quality_block() {
        let (db, _tmp) = setup_test_db().await;
        create_server(&db, "srv-ip1").await;
        insert_snapshot(&db, "srv-ip1", "1.2.3.4").await;

        StatusPageService::create(
            &db,
            create_page_with_ip_quality("ipq-off", false, vec!["srv-ip1".to_string()]),
        )
        .await
        .expect("status page should be created");
        let app = build_app(db, AppConfig::default()).await;

        let (status, body) = get_json(app, "/api/status/ipq-off").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body["data"].get("ip_quality").is_none()
                || body["data"]["ip_quality"].is_null(),
            "ip_quality block must be absent when show_ip_quality is false; got: {}",
            body["data"]["ip_quality"]
        );
    }

    #[tokio::test]
    async fn status_page_with_show_ip_quality_true_includes_ip_quality_for_page_servers() {
        let (db, _tmp) = setup_test_db().await;
        create_server(&db, "srv-ip2").await;
        create_server(&db, "srv-ip3").await;
        insert_snapshot(&db, "srv-ip2", "2.3.4.5").await;
        insert_snapshot(&db, "srv-ip3", "3.4.5.6").await;

        StatusPageService::create(
            &db,
            create_page_with_ip_quality("ipq-on", true, vec!["srv-ip2".to_string()]),
        )
        .await
        .expect("status page should be created");
        let app = build_app(db, AppConfig::default()).await;

        let (status, body) = get_json(app, "/api/status/ipq-on").await;
        assert_eq!(status, StatusCode::OK);
        let ip_quality = &body["data"]["ip_quality"];
        assert!(
            ip_quality.is_array(),
            "ip_quality block should be present when show_ip_quality is true"
        );
        let arr = ip_quality.as_array().unwrap();
        // Only srv-ip2 is in the page — srv-ip3 must not appear
        assert_eq!(arr.len(), 1, "should only have one entry for srv-ip2");
        let entry = &arr[0];
        assert_eq!(entry["server_id"], "srv-ip2");
        // Unauthenticated — IP must be masked
        let snap = &entry["ip_quality"];
        if !snap.is_null() {
            let ip_val = snap["ip"].as_str().unwrap_or("");
            assert_eq!(ip_val, "*.*.*.*", "unauthenticated IP must be masked");
        }
    }

    #[tokio::test]
    async fn status_page_authenticated_session_shows_real_ip() {
        let (db, _tmp) = setup_test_db().await;
        let cookie = session_cookie(&db, "ipq_admin").await;
        create_server(&db, "srv-ip4").await;
        insert_snapshot(&db, "srv-ip4", "4.5.6.7").await;

        StatusPageService::create(
            &db,
            create_page_with_ip_quality("ipq-auth", true, vec!["srv-ip4".to_string()]),
        )
        .await
        .expect("status page should be created");
        let app = build_app(db, AppConfig::default()).await;

        let (status, body) = request_json(
            app,
            axum::http::Method::GET,
            "/api/status/ipq-auth",
            Some(&cookie),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let ip_quality = &body["data"]["ip_quality"];
        assert!(ip_quality.is_array(), "ip_quality block should be present");
        let arr = ip_quality.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        let snap = &arr[0]["ip_quality"];
        if !snap.is_null() {
            let ip_val = snap["ip"].as_str().unwrap_or("");
            assert_eq!(ip_val, "4.5.6.7", "authenticated session should show real IP");
        }
    }

    #[tokio::test]
    async fn legacy_status_endpoint_never_includes_ip_quality() {
        let (db, _tmp) = setup_test_db().await;
        let app = build_app(db, AppConfig::default()).await;

        let (status, body) = get_json(app, "/api/status").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            body["data"].get("ip_quality").is_none() || body["data"]["ip_quality"].is_null(),
            "legacy /api/status must never include ip_quality"
        );
    }

    #[tokio::test]
    async fn disabled_custom_themes_reject_custom_status_page_update_but_allow_null_and_preset() {
        let (db, _tmp) = setup_test_db().await;
        let cookie = session_cookie(&db, "status_page_disabled_admin").await;
        let page = StatusPageService::create(&db, create_page_input("disabled-update"))
            .await
            .expect("status page should be created");
        let theme_id = create_theme(&db, "Disabled write").await;
        let mut config = AppConfig::default();
        config.feature.custom_themes = false;
        let app = build_app(db, config).await;

        let (custom_status, custom_body) = request_json(
            app.clone(),
            axum::http::Method::PUT,
            &format!("/api/status-pages/{}", page.id),
            Some(&cookie),
            Some(json!({ "theme_ref": format!("custom:{theme_id}") })),
        )
        .await;
        let (preset_status, _) = request_json(
            app.clone(),
            axum::http::Method::PUT,
            &format!("/api/status-pages/{}", page.id),
            Some(&cookie),
            Some(json!({ "theme_ref": "preset:nord" })),
        )
        .await;
        let (null_status, _) = request_json(
            app,
            axum::http::Method::PUT,
            &format!("/api/status-pages/{}", page.id),
            Some(&cookie),
            Some(json!({ "theme_ref": null })),
        )
        .await;

        assert_eq!(custom_status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            custom_body["error"]["message"],
            "Validation error: custom theme feature disabled"
        );
        assert_eq!(preset_status, StatusCode::OK);
        assert_eq!(null_status, StatusCode::OK);
    }
}
