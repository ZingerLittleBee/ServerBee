use std::sync::Arc;

use axum::extract::{ConnectInfo, Query, State};
use axum::http::HeaderMap;
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::router::utils::extract_client_ip;
use crate::service::audit::{AuditListFilters, AuditService};
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct AuditListParams {
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
}

fn default_limit() -> u64 {
    50
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuditLogEntry {
    pub id: i64,
    pub user_id: String,
    pub action: String,
    pub detail: Option<String>,
    pub ip: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuditListResponse {
    pub entries: Vec<AuditLogEntry>,
    pub total: u64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuditUserOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuditOptionsResponse {
    pub actions: Vec<String>,
    pub users: Vec<AuditUserOption>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuditClearResponse {
    pub deleted: u64,
}

/// Admin-only audit log routes.
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/audit-logs", get(list_audit_logs).delete(clear_audit_logs))
        .route("/audit-logs/options", get(list_audit_options))
}

#[utoipa::path(
    get,
    path = "/api/audit-logs",
    tag = "audit",
    params(AuditListParams),
    responses(
        (status = 200, description = "Audit log entries", body = AuditListResponse),
        (status = 403, description = "Forbidden — admin only"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list_audit_logs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuditListParams>,
) -> Result<Json<ApiResponse<AuditListResponse>>, AppError> {
    let limit = params.limit.min(200);
    let filters = AuditListFilters {
        action: params.action,
        user_id: params.user_id,
    };
    let (entries, total) = AuditService::list(&state.db, limit, params.offset, filters).await?;

    let entries: Vec<AuditLogEntry> = entries
        .into_iter()
        .map(|e| AuditLogEntry {
            id: e.id,
            user_id: e.user_id,
            action: e.action,
            detail: e.detail,
            ip: e.ip,
            created_at: e.created_at.to_rfc3339(),
        })
        .collect();

    ok(AuditListResponse { entries, total })
}

#[utoipa::path(
    get,
    path = "/api/audit-logs/options",
    tag = "audit",
    responses(
        (status = 200, description = "Filter options for the audit log", body = AuditOptionsResponse),
        (status = 403, description = "Forbidden — admin only"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn list_audit_options(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<AuditOptionsResponse>>, AppError> {
    let actions = AuditService::distinct_actions(&state.db).await?;
    let users = AuditService::distinct_users(&state.db)
        .await?
        .into_iter()
        .map(|u| AuditUserOption {
            id: u.id,
            label: u.label,
        })
        .collect();
    ok(AuditOptionsResponse { actions, users })
}

#[utoipa::path(
    delete,
    path = "/api/audit-logs",
    tag = "audit",
    responses(
        (status = 200, description = "Number of audit log entries removed", body = AuditClearResponse),
        (status = 403, description = "Forbidden — admin only"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
pub async fn clear_audit_logs(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(current_user): Extension<CurrentUser>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<AuditClearResponse>>, AppError> {
    let ip = extract_client_ip(
        &ConnectInfo(addr),
        &headers,
        &state.config.server.trusted_proxies,
    )
    .to_string();

    let deleted = AuditService::clear_all(&state.db).await?;

    // Record the clear action itself so the admin who cleared the table is auditable.
    let detail = serde_json::json!({ "deleted": deleted }).to_string();
    let _ = AuditService::log(
        &state.db,
        &current_user.user_id,
        "audit_log_clear",
        Some(&detail),
        &ip,
    )
    .await;

    ok(AuditClearResponse { deleted })
}
