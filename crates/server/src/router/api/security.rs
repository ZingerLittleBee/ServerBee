//! REST API for security events.
//!
//! Read routes are exposed to any authenticated user (admin or member).
//! Delete is admin-only and lives on `write_router`.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::routing::{delete, get};
use axum::{Json, Router};
use base64::Engine;
use chrono::{DateTime, Utc};
use sea_orm::*;
use serde::{Deserialize, Serialize};

use crate::entity::security_event;
use crate::error::{ApiResponse, AppError, ok};
use crate::state::AppState;

const DEFAULT_LIMIT: u64 = 50;
const MAX_LIMIT: u64 = 200;

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/security/events", get(list_events))
        .route("/security/events/{id}", get(get_event))
        .route("/security/stats", get(stats))
}

pub fn write_router() -> Router<Arc<AppState>> {
    Router::new().route("/security/events/{id}", delete(delete_event))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SecurityEventDto {
    pub id: String,
    pub server_id: String,
    pub event_type: String,
    pub severity: String,
    pub source_ip: String,
    pub source_port: Option<i32>,
    pub username: Option<String>,
    pub started_at: String,
    pub ended_at: String,
    pub first_seen: bool,
    pub detector_source: String,
    pub evidence: serde_json::Value,
    pub created_at: String,
}

impl From<security_event::Model> for SecurityEventDto {
    fn from(m: security_event::Model) -> Self {
        let evidence = serde_json::from_str(&m.evidence).unwrap_or(serde_json::Value::Null);
        Self {
            id: m.id,
            server_id: m.server_id,
            event_type: m.event_type,
            severity: m.severity,
            source_ip: m.source_ip,
            source_port: m.source_port,
            username: m.username,
            started_at: m.started_at.to_rfc3339(),
            ended_at: m.ended_at.to_rfc3339(),
            first_seen: m.first_seen,
            detector_source: m.detector_source,
            evidence,
            created_at: m.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SecurityEventList {
    pub items: Vec<SecurityEventDto>,
    /// Opaque cursor for the next page. `None` when there are no more rows.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ListEventsParams {
    pub server_id: Option<String>,
    pub event_type: Option<String>,
    pub source_ip: Option<String>,
    pub severity: Option<String>,
    /// ISO-8601 timestamp (`>=` filter on created_at).
    pub since: Option<DateTime<Utc>>,
    /// ISO-8601 timestamp (`<=` filter on created_at).
    pub until: Option<DateTime<Utc>>,
    /// Opaque cursor from `next_cursor` of the previous response.
    pub cursor: Option<String>,
    pub limit: Option<u64>,
}

/// Cursor encoded as `base64(rfc3339_created_at|id)`. Stable across calls and
/// agnostic to the underlying schema.
fn encode_cursor(created_at: DateTime<Utc>, id: &str) -> String {
    let raw = format!("{}|{id}", created_at.to_rfc3339());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw)
}

fn decode_cursor(c: &str) -> Result<(DateTime<Utc>, String), AppError> {
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(c.as_bytes())
        .map_err(|_| AppError::BadRequest("invalid cursor".to_string()))?;
    let s = String::from_utf8(raw)
        .map_err(|_| AppError::BadRequest("invalid cursor".to_string()))?;
    let (ts, id) = s
        .split_once('|')
        .ok_or_else(|| AppError::BadRequest("invalid cursor".to_string()))?;
    let parsed = DateTime::parse_from_rfc3339(ts)
        .map_err(|_| AppError::BadRequest("invalid cursor timestamp".to_string()))?
        .with_timezone(&Utc);
    Ok((parsed, id.to_string()))
}

#[utoipa::path(
    get,
    path = "/api/security/events",
    tag = "security",
    params(ListEventsParams),
    responses(
        (status = 200, description = "Paginated security events", body = SecurityEventList),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn list_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListEventsParams>,
) -> Result<Json<ApiResponse<SecurityEventList>>, AppError> {
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

    let mut q = security_event::Entity::find();
    if let Some(server_id) = &params.server_id {
        q = q.filter(security_event::Column::ServerId.eq(server_id.clone()));
    }
    if let Some(event_type) = &params.event_type {
        q = q.filter(security_event::Column::EventType.eq(event_type.clone()));
    }
    if let Some(source_ip) = &params.source_ip {
        q = q.filter(security_event::Column::SourceIp.eq(source_ip.clone()));
    }
    if let Some(severity) = &params.severity {
        q = q.filter(security_event::Column::Severity.eq(severity.clone()));
    }
    if let Some(since) = params.since {
        q = q.filter(security_event::Column::CreatedAt.gte(since));
    }
    if let Some(until) = params.until {
        q = q.filter(security_event::Column::CreatedAt.lte(until));
    }

    if let Some(cursor) = &params.cursor {
        let (cur_ts, cur_id) = decode_cursor(cursor)?;
        // Descending order on (created_at, id): the next page starts with rows
        // strictly older than the cursor, with id as the tie-breaker to keep
        // pagination stable when timestamps collide.
        q = q.filter(
            Condition::any()
                .add(security_event::Column::CreatedAt.lt(cur_ts))
                .add(
                    Condition::all()
                        .add(security_event::Column::CreatedAt.eq(cur_ts))
                        .add(security_event::Column::Id.lt(cur_id)),
                ),
        );
    }

    let rows = q
        .order_by_desc(security_event::Column::CreatedAt)
        .order_by_desc(security_event::Column::Id)
        .limit(limit + 1)
        .all(&state.db)
        .await?;

    let mut items: Vec<security_event::Model> = rows.into_iter().collect();
    let next_cursor = if items.len() as u64 > limit {
        let extra = items.pop().expect("len > limit > 0");
        Some(encode_cursor(extra.created_at, &extra.id))
    } else {
        None
    };

    let items = items.into_iter().map(SecurityEventDto::from).collect();
    ok(SecurityEventList { items, next_cursor })
}

#[utoipa::path(
    get,
    path = "/api/security/events/{id}",
    tag = "security",
    params(("id" = String, Path, description = "Security event id")),
    responses(
        (status = 200, description = "Security event", body = SecurityEventDto),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn get_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<SecurityEventDto>>, AppError> {
    let row = security_event::Entity::find_by_id(&id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("security event {id} not found")))?;
    ok(SecurityEventDto::from(row))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct StatsParams {
    pub server_id: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    /// One of `event_type`, `source_ip`, `day`. Defaults to `event_type`.
    pub group_by: Option<String>,
    /// Cap on returned buckets. Defaults to 50, max 500.
    pub limit: Option<u64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StatsBucket {
    pub key: String,
    pub count: i64,
}

#[utoipa::path(
    get,
    path = "/api/security/stats",
    tag = "security",
    params(StatsParams),
    responses(
        (status = 200, description = "Aggregated counts", body = [StatsBucket]),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn stats(
    State(state): State<Arc<AppState>>,
    Query(params): Query<StatsParams>,
) -> Result<Json<ApiResponse<Vec<StatsBucket>>>, AppError> {
    let group_by = params.group_by.as_deref().unwrap_or("event_type");
    let limit = params.limit.unwrap_or(50).min(500);

    let (group_expr, group_label) = match group_by {
        "event_type" => ("event_type", "event_type"),
        "source_ip" => ("source_ip", "source_ip"),
        "day" => ("date(created_at)", "day"),
        other => {
            return Err(AppError::BadRequest(format!(
                "invalid group_by '{other}': expected event_type|source_ip|day"
            )));
        }
    };

    let mut sql = format!(
        "SELECT {group_expr} AS group_key, COUNT(*) AS cnt \
         FROM security_event WHERE 1=1"
    );
    let mut values: Vec<sea_orm::Value> = Vec::new();
    if let Some(server_id) = &params.server_id {
        sql.push_str(&format!(" AND server_id = ${}", values.len() + 1));
        values.push(server_id.clone().into());
    }
    if let Some(since) = params.since {
        sql.push_str(&format!(" AND created_at >= ${}", values.len() + 1));
        values.push(since.to_rfc3339().into());
    }
    if let Some(until) = params.until {
        sql.push_str(&format!(" AND created_at <= ${}", values.len() + 1));
        values.push(until.to_rfc3339().into());
    }
    sql.push_str(&format!(
        " GROUP BY {group_expr} ORDER BY cnt DESC LIMIT {limit}"
    ));

    let rows = state
        .db
        .query_all(Statement::from_sql_and_values(
            state.db.get_database_backend(),
            sql,
            values,
        ))
        .await?;

    let buckets = rows
        .into_iter()
        .map(|r| {
            let key: String = r
                .try_get::<String>("", "group_key")
                .unwrap_or_else(|_| "".to_string());
            let count: i64 = r.try_get::<i64>("", "cnt").unwrap_or(0);
            StatsBucket { key, count }
        })
        .collect::<Vec<_>>();
    let _ = group_label; // currently unused but retained for future response metadata
    ok(buckets)
}

#[utoipa::path(
    delete,
    path = "/api/security/events/{id}",
    tag = "security",
    params(("id" = String, Path, description = "Security event id")),
    responses(
        (status = 200, description = "Deleted"),
        (status = 403, description = "Forbidden — admin only"),
        (status = 404, description = "Not found"),
    ),
    security(("session_cookie" = []), ("api_key" = []), ("bearer_token" = []))
)]
async fn delete_event(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<bool>>, AppError> {
    let res = security_event::Entity::delete_by_id(&id)
        .exec(&state.db)
        .await?;
    if res.rows_affected == 0 {
        return Err(AppError::NotFound(format!("security event {id} not found")));
    }
    ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::server as server_entity;
    use crate::test_utils::setup_test_db;
    use chrono::Duration;

    async fn insert_server(db: &DatabaseConnection, id: &str) {
        let now = Utc::now();
        server_entity::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(Some("hash".into())),
            token_prefix: Set(Some("prefix".into())),
            name: Set(format!("Server {id}")),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(0),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap();
    }

    async fn insert_event(
        db: &DatabaseConnection,
        id: &str,
        server_id: &str,
        event_type: &str,
        source_ip: &str,
        created_at: DateTime<Utc>,
    ) {
        security_event::ActiveModel {
            id: Set(id.to_string()),
            server_id: Set(server_id.to_string()),
            event_type: Set(event_type.to_string()),
            severity: Set("high".to_string()),
            source_ip: Set(source_ip.to_string()),
            source_port: Set(None),
            username: Set(None),
            started_at: Set(created_at),
            ended_at: Set(created_at),
            first_seen: Set(false),
            detector_source: Set("journal".to_string()),
            evidence: Set("{}".to_string()),
            created_at: Set(created_at),
        }
        .insert(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn list_filters_by_server_id() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "srv-1").await;
        insert_server(&db, "srv-2").await;
        let now = Utc::now();
        insert_event(&db, "e1", "srv-1", "ssh_brute_force", "1.1.1.1", now).await;
        insert_event(&db, "e2", "srv-2", "ssh_brute_force", "2.2.2.2", now).await;

        let q = security_event::Entity::find()
            .filter(security_event::Column::ServerId.eq("srv-1"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].id, "e1");
    }

    #[tokio::test]
    async fn cursor_round_trips() {
        let now = Utc::now();
        let c = encode_cursor(now, "evt-123");
        let (ts, id) = decode_cursor(&c).unwrap();
        // RFC3339 keeps microsecond precision; compare seconds.
        assert_eq!(ts.timestamp(), now.timestamp());
        assert_eq!(id, "evt-123");
    }

    #[tokio::test]
    async fn invalid_cursor_rejected() {
        let err = decode_cursor("not-a-cursor").unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[tokio::test]
    async fn list_paginates_with_cursor() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "srv-1").await;
        let base = Utc::now();
        for i in 0..5 {
            insert_event(
                &db,
                &format!("e{i}"),
                "srv-1",
                "ssh_brute_force",
                "1.1.1.1",
                base - Duration::minutes(i),
            )
            .await;
        }

        // Manual call (without the full app router) to verify pagination plumbing.
        // Limit=2 → 2 items + a next_cursor pointing at the 3rd row.
        let rows = security_event::Entity::find()
            .order_by_desc(security_event::Column::CreatedAt)
            .order_by_desc(security_event::Column::Id)
            .limit(3)
            .all(&db)
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);
        let cursor = encode_cursor(rows[2].created_at, &rows[2].id);

        let (cur_ts, cur_id) = decode_cursor(&cursor).unwrap();
        let next = security_event::Entity::find()
            .filter(
                Condition::any()
                    .add(security_event::Column::CreatedAt.lt(cur_ts))
                    .add(
                        Condition::all()
                            .add(security_event::Column::CreatedAt.eq(cur_ts))
                            .add(security_event::Column::Id.lt(cur_id.clone())),
                    ),
            )
            .order_by_desc(security_event::Column::CreatedAt)
            .order_by_desc(security_event::Column::Id)
            .all(&db)
            .await
            .unwrap();
        // The first 3 rows (e0..e2 by recency) preceded the cursor row; remaining
        // pages should contain the rest excluding the cursor row itself.
        assert!(
            next.iter().all(|r| r.id != cur_id),
            "cursor row excluded from next page"
        );
    }

    #[tokio::test]
    async fn delete_event_removes_row() {
        let (db, _tmp) = setup_test_db().await;
        insert_server(&db, "srv-1").await;
        let now = Utc::now();
        insert_event(&db, "e1", "srv-1", "ssh_brute_force", "1.1.1.1", now).await;

        let res = security_event::Entity::delete_by_id("e1")
            .exec(&db)
            .await
            .unwrap();
        assert_eq!(res.rows_affected, 1);

        let q = security_event::Entity::find().all(&db).await.unwrap();
        assert!(q.is_empty());
    }
}
