use std::sync::Arc;

use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use sea_orm::*;
use serde::Serialize;

use crate::entity::{incident, incident_update, maintenance, server, server_group, status_page};
use crate::error::{ok, ApiResponse, AppError};
use crate::service::incident::IncidentService;
use crate::service::maintenance::MaintenanceService;
use crate::service::status_page::{CreateStatusPage, StatusPageService, UpdateStatusPage};
use crate::service::uptime::UptimeService;
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
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ServerStatusInfo {
    pub id: String,
    pub name: String,
    pub region: Option<String>,
    pub country_code: Option<String>,
    pub os: Option<String>,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub online: bool,
    pub uptime_percentage: f64,
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

    // Parse server IDs from the page
    let server_ids: Vec<String> =
        serde_json::from_str(&page.server_ids_json).unwrap_or_default();

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
    let groups: Vec<server_group::Model> =
        server_group::Entity::find().all(&state.db).await?;
    let group_map: std::collections::HashMap<String, String> = groups
        .into_iter()
        .map(|g| (g.id, g.name))
        .collect();

    // Build server status info
    let mut server_infos = Vec::new();
    for s in &servers {
        let online = state.agent_manager.is_online(&s.id);
        let uptime_percentage = UptimeService::get_availability(&state.db, &s.id, 90)
            .await
            .unwrap_or(100.0);
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
            group_name: s.group_id.as_ref().and_then(|gid| group_map.get(gid).cloned()),
            online,
            uptime_percentage,
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

        let updates = IncidentService::list_updates(&state.db, &inc.id).await.unwrap_or_default();

        if inc.status != "resolved" {
            active_incidents.push(IncidentWithUpdates {
                incident: inc,
                updates,
            });
        } else if let Some(resolved_at) = inc.resolved_at {
            if resolved_at >= seven_days_ago {
                recent_incidents.push(IncidentWithUpdates {
                    incident: inc,
                    updates,
                });
            }
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
    };

    ok(PublicStatusPageData {
        page: page_info,
        servers: server_infos,
        active_incidents,
        planned_maintenances,
        recent_incidents,
    })
}

#[utoipa::path(
    get,
    path = "/api/status-pages",
    operation_id = "list_status_pages",
    tag = "status-pages",
    responses(
        (status = 200, description = "List all status pages", body = Vec<status_page::Model>),
    ),
    security(("session_cookie" = []), ("api_key" = []))
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
    security(("session_cookie" = []), ("api_key" = []))
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
    security(("session_cookie" = []), ("api_key" = []))
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
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn delete_status_page(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<&'static str>>, AppError> {
    StatusPageService::delete(&state.db, &id).await?;
    ok("ok")
}
