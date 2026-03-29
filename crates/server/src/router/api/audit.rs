use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::{ApiResponse, AppError, ok};
use crate::service::audit::AuditService;
use crate::state::AppState;

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct AuditListParams {
    #[serde(default = "default_limit")]
    pub limit: u64,
    #[serde(default)]
    pub offset: u64,
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

/// Admin-only audit log routes.
pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/audit-logs", get(list_audit_logs))
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
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn list_audit_logs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuditListParams>,
) -> Result<Json<ApiResponse<AuditListResponse>>, AppError> {
    let limit = params.limit.min(200);
    let (entries, total) = AuditService::list(&state.db, limit, params.offset).await?;

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
