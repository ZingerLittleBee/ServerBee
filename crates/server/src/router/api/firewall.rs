//! REST API for the firewall blocklist.
//!
//! Read routes (`list`, `get`, `stats`) are exposed to any authenticated user.
//! Create / delete are admin-only and live on `write_router`.

use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::block_list;
use crate::error::{ApiResponse, AppError, ok};
use crate::middleware::auth::CurrentUser;
use crate::service::audit::AuditService;
use crate::service::db_error::is_unique_violation;
use crate::service::firewall::FirewallService;
use crate::state::AppState;

const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/firewall/blocks", get(list_blocks))
        .route("/firewall/blocks/{id}", get(get_block))
        .route("/firewall/stats", get(stats))
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/firewall/blocks", post(create_block))
        .route("/firewall/blocks/{id}", delete(delete_block))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateBlockReq {
    /// Bare IP or CIDR. Server canonicalizes to a CIDR (`/32` or `/128` for
    /// bare IPs, network-bits stripped for CIDRs).
    pub target: String,
    #[serde(default = "default_cover")]
    pub cover_type: String,
    #[serde(default)]
    pub server_ids: Option<Vec<String>>,
    #[serde(default)]
    pub comment: Option<String>,
}

fn default_cover() -> String {
    "all".into()
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BlockListItem {
    pub id: String,
    pub target: String,
    pub family: i32,
    pub cover_type: String,
    pub server_ids: Option<Vec<String>>,
    pub comment: Option<String>,
    pub origin: String,
    pub origin_event_id: Option<String>,
    pub origin_rule_id: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
}

impl From<block_list::Model> for BlockListItem {
    fn from(m: block_list::Model) -> Self {
        let server_ids = m
            .server_ids_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        Self {
            id: m.id,
            target: m.target,
            family: m.family,
            cover_type: m.cover_type,
            server_ids,
            comment: m.comment,
            origin: m.origin,
            origin_event_id: m.origin_event_id,
            origin_rule_id: m.origin_rule_id,
            created_by: m.created_by,
            created_at: m.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListQuery {
    /// RFC3339 timestamp from a previous `next_cursor` response.
    pub cursor: Option<String>,
    pub origin: Option<String>,
    pub target_q: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ListResp {
    pub items: Vec<BlockListItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StatsResp {
    pub total: i64,
    pub auto: i64,
    pub manual: i64,
    pub v4: i64,
    pub v6: i64,
}

#[utoipa::path(
    get,
    path = "/api/firewall/blocks",
    tag = "firewall",
    params(ListQuery),
    responses(
        (status = 200, description = "Paginated blocklist", body = ListResp),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn list_blocks(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> Result<Json<ApiResponse<ListResp>>, AppError> {
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let mut find = block_list::Entity::find().order_by_desc(block_list::Column::CreatedAt);
    if let Some(o) = q.origin.as_deref() {
        find = find.filter(block_list::Column::Origin.eq(o));
    }
    if let Some(tq) = q.target_q.as_deref() {
        find = find.filter(block_list::Column::Target.contains(tq));
    }
    if let Some(cursor) = q.cursor.as_deref() {
        let parsed = DateTime::parse_from_rfc3339(cursor)
            .map_err(|_| AppError::BadRequest("invalid cursor".into()))?
            .with_timezone(&Utc);
        find = find.filter(block_list::Column::CreatedAt.lt(parsed));
    }
    let rows = find.limit(limit + 1).all(&state.db).await?;
    let mut items: Vec<BlockListItem> = rows.into_iter().map(BlockListItem::from).collect();
    let next_cursor = if items.len() as u64 > limit {
        items.pop().map(|last| last.created_at)
    } else {
        None
    };
    ok(ListResp { items, next_cursor })
}

#[utoipa::path(
    get,
    path = "/api/firewall/blocks/{id}",
    tag = "firewall",
    params(("id" = String, Path, description = "Block id")),
    responses(
        (status = 200, description = "Block detail", body = BlockListItem),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_block(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<BlockListItem>>, AppError> {
    let row = block_list::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("block {id} not found")))?;
    ok(BlockListItem::from(row))
}

#[utoipa::path(
    post,
    path = "/api/firewall/blocks",
    tag = "firewall",
    request_body = CreateBlockReq,
    responses(
        (status = 200, description = "Created", body = BlockListItem),
        (status = 409, description = "Guardrail rejected or duplicate target"),
        (status = 403, description = "Forbidden — admin only"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn create_block(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(req): Json<CreateBlockReq>,
) -> Result<Json<ApiResponse<BlockListItem>>, AppError> {
    let (target, family) = FirewallService::canonicalize_target(&req.target)?;

    let dynamic_allow = state.firewall.collect_dynamic_allow().await;
    let mut effective_allow: Vec<String> = state.config.firewall.allow_list.clone();
    effective_allow.extend(dynamic_allow);

    if let Some(reason) = FirewallService::is_protected(&target, &effective_allow) {
        AuditService::log(
            &state.db,
            &current_user.user_id,
            "firewall_block_rejected_server",
            Some(&serde_json::json!({ "target": target, "reason": reason }).to_string()),
            "",
        )
        .await
        .ok();
        return Err(AppError::Conflict(reason));
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let server_ids_json = req
        .server_ids
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    let active = block_list::ActiveModel {
        id: Set(id.clone()),
        target: Set(target.clone()),
        family: Set(family as i32),
        cover_type: Set(req.cover_type.clone()),
        server_ids_json: Set(server_ids_json),
        comment: Set(req.comment.clone()),
        origin: Set("manual".into()),
        origin_event_id: Set(None),
        origin_rule_id: Set(None),
        created_by: Set(Some(current_user.user_id.clone())),
        created_at: Set(now),
    };

    let model = match active.insert(&state.db).await {
        Ok(m) => m,
        Err(e) => {
            if is_unique_violation(&e) {
                return Err(AppError::Conflict(format!(
                    "target {target} already blocked"
                )));
            }
            return Err(e.into());
        }
    };

    AuditService::log(
        &state.db,
        &current_user.user_id,
        "firewall_block_created",
        Some(
            &serde_json::json!({ "id": model.id, "target": model.target, "origin": "manual" })
                .to_string(),
        ),
        "",
    )
    .await
    .ok();

    let item: BlockListItem = model.into();
    state.firewall.broadcast_changed_created(&item);
    state.firewall.push_add_to_covered_agents(&item).await;

    ok(item)
}

#[utoipa::path(
    delete,
    path = "/api/firewall/blocks/{id}",
    tag = "firewall",
    params(("id" = String, Path, description = "Block id")),
    responses(
        (status = 200, description = "Deleted"),
        (status = 403, description = "Forbidden — admin only"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn delete_block(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<bool>>, AppError> {
    let row = block_list::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("block {id} not found")))?;

    block_list::Entity::delete_by_id(&id).exec(&state.db).await?;

    AuditService::log(
        &state.db,
        &current_user.user_id,
        "firewall_block_deleted",
        Some(&serde_json::json!({ "id": id, "target": row.target }).to_string()),
        "",
    )
    .await
    .ok();

    state.firewall.broadcast_changed_deleted(&row);
    state.firewall.push_remove_to_covered_agents(&row).await;

    ok(true)
}

#[utoipa::path(
    get,
    path = "/api/firewall/stats",
    tag = "firewall",
    responses(
        (status = 200, description = "Aggregate counts", body = StatsResp),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<StatsResp>>, AppError> {
    let total = block_list::Entity::find().count(&state.db).await? as i64;
    let auto = block_list::Entity::find()
        .filter(block_list::Column::Origin.eq("auto"))
        .count(&state.db)
        .await? as i64;
    let v6 = block_list::Entity::find()
        .filter(block_list::Column::Family.eq(6))
        .count(&state.db)
        .await? as i64;
    ok(StatsResp {
        total,
        auto,
        manual: total - auto,
        v4: total - v6,
        v6,
    })
}
