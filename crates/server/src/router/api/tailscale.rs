use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{ConnectInfo, Extension, Path, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;

use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::utils::extract_client_ip;
use crate::router::ws::agent::extract_tailscale_fields;
use crate::service::audit::AuditService;
use crate::service::capability_gate::require_capability;
use crate::service::tailscale_control::{
    TailnetOverviewResponse, TailscaleAdminConfigResponse, TailscaleConnectionTestResponse,
    TestTailscaleConfigRequest, UpdateTailscaleConfigRequest,
};
use crate::state::AppState;
use serverbee_common::constants::CAP_TAILSCALE;
use serverbee_common::protocol::{AgentMessage, ServerMessage};
use serverbee_common::tailscale_types::TailscaleStatusReport;

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ServerTailscaleStatusResponse {
    pub available: bool,
    pub online: bool,
    pub cached: bool,
    pub tailscale_ipv4: Option<String>,
    pub tailscale_ipv6: Option<String>,
    pub tailscale_dns_name: Option<String>,
    pub tailscale_tailnet: Option<String>,
    pub status: Option<TailscaleStatusReport>,
}

/// Read endpoints accessible to all authenticated users (`admin` + `member`).
pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/servers/{id}/tailscale/status",
            get(get_server_tailscale_status),
        )
        .route("/tailscale/overview", get(get_tailnet_overview))
}

/// Admin-only endpoints for global Tailscale / Headscale control-plane config.
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/tailscale/config",
            get(get_tailscale_config).put(update_tailscale_config),
        )
        .route("/tailscale/test", post(test_tailscale_config))
}

#[utoipa::path(
    get,
    path = "/api/servers/{id}/tailscale/status",
    tag = "tailscale",
    params(("id" = String, Path, description = "Server ID")),
    responses(
        (status = 200, description = "Per-server Tailscale node & peer status", body = ServerTailscaleStatusResponse),
        (status = 403, description = "Tailscale capability disabled on agent"),
        (status = 404, description = "Server not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_server_tailscale_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<ServerTailscaleStatusResponse>>, AppError> {
    let server = require_capability(&state, &id, CAP_TAILSCALE).await?;
    let online = state.agent_manager.is_online(&id);

    if online {
        match state
            .agent_manager
            .request(&id, Duration::from_secs(5), |msg_id| {
                ServerMessage::TailscaleGetStatus { msg_id }
            })
            .await
        {
            Ok(AgentMessage::TailscaleStatus { status, .. }) => {
                let extracted = extract_tailscale_fields(&status.summary);
                return ok(ServerTailscaleStatusResponse {
                    available: true,
                    online: true,
                    cached: false,
                    tailscale_ipv4: extracted.ipv4.or(server.tailscale_ipv4),
                    tailscale_ipv6: extracted.ipv6.or(server.tailscale_ipv6),
                    tailscale_dns_name: extracted.dns_name.or(server.tailscale_dns_name),
                    tailscale_tailnet: extracted.tailnet.or(server.tailscale_tailnet),
                    status: Some(status),
                });
            }
            Ok(AgentMessage::TailscaleUnavailable { .. }) => {
                return ok(ServerTailscaleStatusResponse {
                    available: false,
                    online: true,
                    cached: false,
                    tailscale_ipv4: server.tailscale_ipv4,
                    tailscale_ipv6: server.tailscale_ipv6,
                    tailscale_dns_name: server.tailscale_dns_name,
                    tailscale_tailnet: server.tailscale_tailnet,
                    status: None,
                });
            }
            Ok(_) | Err(_) => {
                // Fall back to cached report / persisted summary on timeout or disconnect.
            }
        }
    }

    let cached_status = state.agent_manager.get_tailscale_status(&id);
    let extracted = cached_status
        .as_ref()
        .map(|s| extract_tailscale_fields(&s.summary));

    let tailscale_ipv4 = extracted
        .as_ref()
        .and_then(|e| e.ipv4.clone())
        .or(server.tailscale_ipv4);
    let tailscale_ipv6 = extracted
        .as_ref()
        .and_then(|e| e.ipv6.clone())
        .or(server.tailscale_ipv6);
    let tailscale_dns_name = extracted
        .as_ref()
        .and_then(|e| e.dns_name.clone())
        .or(server.tailscale_dns_name);
    let tailscale_tailnet = extracted
        .as_ref()
        .and_then(|e| e.tailnet.clone())
        .or(server.tailscale_tailnet);

    let available = cached_status.is_some()
        || tailscale_ipv4.is_some()
        || tailscale_ipv6.is_some()
        || tailscale_dns_name.is_some();

    ok(ServerTailscaleStatusResponse {
        available,
        online: state.agent_manager.is_online(&id),
        cached: true,
        tailscale_ipv4,
        tailscale_ipv6,
        tailscale_dns_name,
        tailscale_tailnet,
        status: cached_status,
    })
}

#[utoipa::path(
    get,
    path = "/api/tailscale/overview",
    tag = "tailscale",
    responses(
        (status = 200, description = "Global Tailnet / Headscale overview and linked devices", body = TailnetOverviewResponse),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_tailnet_overview(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<TailnetOverviewResponse>>, AppError> {
    let overview = state.tailscale_control.get_overview(&state.db).await?;
    ok(overview)
}

#[utoipa::path(
    get,
    path = "/api/tailscale/config",
    tag = "tailscale",
    responses(
        (status = 200, description = "Global Tailscale / Headscale admin configuration (API key redacted)", body = TailscaleAdminConfigResponse),
        (status = 403, description = "Admin role required"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn get_tailscale_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<TailscaleAdminConfigResponse>>, AppError> {
    let cfg = state.tailscale_control.get_admin_config(&state.db).await?;
    ok(cfg)
}

#[utoipa::path(
    put,
    path = "/api/tailscale/config",
    tag = "tailscale",
    request_body = UpdateTailscaleConfigRequest,
    responses(
        (status = 200, description = "Updated Tailscale / Headscale admin configuration", body = TailscaleAdminConfigResponse),
        (status = 403, description = "Admin role required"),
        (status = 422, description = "Validation or SSRF guard error"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn update_tailscale_config(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(actor): Extension<CurrentUser>,
    headers: HeaderMap,
    Json(body): Json<UpdateTailscaleConfigRequest>,
) -> Result<Json<ApiResponse<TailscaleAdminConfigResponse>>, AppError> {
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();

    let (response, api_key_modified) = state
        .tailscale_control
        .update_config(&state.db, body)
        .await?;

    let detail = serde_json::json!({
        "enabled": response.enabled,
        "provider": response.provider,
        "api_base_url": response.api_base_url,
        "tailnet": response.tailnet,
        "has_api_key": response.has_api_key,
        "api_key_modified": api_key_modified,
        "preferred_server_url": response.preferred_server_url,
        "auth_enabled": response.auth_enabled,
        "auto_provision_role": response.auto_provision_role,
    })
    .to_string();
    let _ = AuditService::log(
        &state.db,
        &actor.user_id,
        "tailscale.config.update",
        Some(&detail),
        &ip,
    )
    .await;

    ok(response)
}

#[utoipa::path(
    post,
    path = "/api/tailscale/test",
    tag = "tailscale",
    request_body = TestTailscaleConfigRequest,
    responses(
        (status = 200, description = "Upstream Tailscale / Headscale connection test result", body = TailscaleConnectionTestResponse),
        (status = 403, description = "Admin role required"),
        (status = 422, description = "Validation or SSRF guard error"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn test_tailscale_config(
    State(state): State<Arc<AppState>>,
    Json(body): Json<TestTailscaleConfigRequest>,
) -> Result<Json<ApiResponse<TailscaleConnectionTestResponse>>, AppError> {
    let result = state
        .tailscale_control
        .test_connection(&state.db, body)
        .await?;
    ok(result)
}
