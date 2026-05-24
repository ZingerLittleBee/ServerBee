// crates/server/src/service/traceroute.rs
use crate::entity::traceroute_record::{self, Model};
use crate::error::AppError;
use sea_orm::*;
use serverbee_common::protocol::RecordedProtocol;
use serverbee_common::types::TracerouteHop;

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct TracerouteRecordSummary {
    pub request_id: String,
    pub target: String,
    pub protocol: RecordedProtocol,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub hop_count: u32,
    pub has_error: bool,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct TracerouteSnapshotResponse {
    pub request_id: String,
    pub target: String,
    pub protocol: RecordedProtocol,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub round: u32,
    pub total_rounds: u32,
    pub completed: bool,
    pub hops: Vec<TracerouteHop>,
    pub error: Option<String>,
}

pub struct NewTracerouteRecord {
    pub id: String,
    pub server_id: String,
    pub target: String,
    pub protocol: RecordedProtocol,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub total_rounds: u32,
    pub completed_rounds: u32,
    pub hops: Vec<TracerouteHop>,
    pub error: Option<String>,
}

pub async fn list_records_for_server(
    db: &DatabaseConnection,
    server_id: &str,
    limit: u64,
    offset: u64,
) -> Result<Vec<TracerouteRecordSummary>, AppError> {
    let rows = traceroute_record::Entity::find()
        .filter(traceroute_record::Column::ServerId.eq(server_id))
        .order_by_desc(traceroute_record::Column::StartedAt)
        .limit(limit)
        .offset(offset)
        .all(db)
        .await
        .map_err(|e| AppError::Internal(format!("DB list: {e}")))?;
    Ok(rows
        .into_iter()
        .map(|m| {
            let hop_count =
                serde_json::from_str::<Vec<TracerouteHop>>(&m.hops_json)
                    .map(|h| h.len() as u32)
                    .unwrap_or(0);
            TracerouteRecordSummary {
                request_id: m.id.clone(),
                target: m.target.clone(),
                protocol: m.protocol_enum(),
                started_at: m.started_at,
                completed_at: m.completed_at,
                hop_count,
                has_error: m.error.is_some(),
            }
        })
        .collect())
}

pub async fn get_record_snapshot(
    db: &DatabaseConnection,
    server_id: &str,
    request_id: &str,
) -> Result<Option<TracerouteSnapshotResponse>, AppError> {
    let row = traceroute_record::Entity::find_by_id(request_id.to_string())
        .filter(traceroute_record::Column::ServerId.eq(server_id))
        .one(db)
        .await
        .map_err(|e| AppError::Internal(format!("DB get: {e}")))?;
    Ok(row.map(|m| model_to_snapshot(&m)))
}

pub fn model_to_snapshot(m: &Model) -> TracerouteSnapshotResponse {
    let hops: Vec<TracerouteHop> =
        serde_json::from_str(&m.hops_json).unwrap_or_default();
    TracerouteSnapshotResponse {
        request_id: m.id.clone(),
        target: m.target.clone(),
        protocol: m.protocol_enum(),
        started_at: m.started_at,
        completed_at: m.completed_at,
        round: m.completed_rounds as u32,
        total_rounds: m.total_rounds as u32,
        completed: m.completed_at.is_some(),
        hops,
        error: m.error.clone(),
    }
}

pub async fn delete_record(
    db: &DatabaseConnection,
    server_id: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let res = traceroute_record::Entity::delete_many()
        .filter(traceroute_record::Column::Id.eq(request_id))
        .filter(traceroute_record::Column::ServerId.eq(server_id))
        .exec(db)
        .await
        .map_err(|e| AppError::Internal(format!("DB delete: {e}")))?;
    if res.rows_affected == 0 {
        return Err(AppError::NotFound(format!(
            "Traceroute record {request_id} not found for server {server_id}"
        )));
    }
    Ok(())
}

pub async fn delete_records_for_server(
    db: &DatabaseConnection,
    server_id: &str,
) -> Result<u64, AppError> {
    let res = traceroute_record::Entity::delete_many()
        .filter(traceroute_record::Column::ServerId.eq(server_id))
        .exec(db)
        .await
        .map_err(|e| AppError::Internal(format!("DB clear: {e}")))?;
    Ok(res.rows_affected)
}

pub async fn insert_completed_record(
    db: &DatabaseConnection,
    record: NewTracerouteRecord,
) -> Result<(), AppError> {
    let hops_json = serde_json::to_string(&record.hops)
        .map_err(|e| AppError::Internal(format!("JSON encode hops: {e}")))?;
    let am = traceroute_record::ActiveModel {
        id: Set(record.id),
        server_id: Set(record.server_id),
        target: Set(record.target),
        protocol: Set(traceroute_record::protocol_to_str(record.protocol).to_string()),
        started_at: Set(record.started_at),
        completed_at: Set(record.completed_at),
        total_rounds: Set(record.total_rounds as i32),
        completed_rounds: Set(record.completed_rounds as i32),
        hops_json: Set(hops_json),
        error: Set(record.error),
    };
    traceroute_record::Entity::insert(am)
        .exec(db)
        .await
        .map_err(|e| AppError::Internal(format!("DB insert: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{Database, DbBackend, Schema};

    async fn fresh_db() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let schema = Schema::new(DbBackend::Sqlite);
        let stmt = schema.create_table_from_entity(traceroute_record::Entity);
        db.execute(db.get_database_backend().build(&stmt)).await.unwrap();
        db
    }

    fn new_record(server_id: &str, request_id: &str, target: &str) -> NewTracerouteRecord {
        NewTracerouteRecord {
            id: request_id.into(),
            server_id: server_id.into(),
            target: target.into(),
            protocol: RecordedProtocol::Icmp,
            started_at: 1_716_500_000_000,
            completed_at: Some(1_716_500_010_000),
            total_rounds: 5,
            completed_rounds: 5,
            hops: vec![],
            error: None,
        }
    }

    #[tokio::test]
    async fn test_insert_and_list_filters_by_server_id() {
        let db = fresh_db().await;
        insert_completed_record(&db, new_record("s1", "r1", "1.1.1.1")).await.unwrap();
        insert_completed_record(&db, new_record("s2", "r2", "8.8.8.8")).await.unwrap();
        let only_s1 = list_records_for_server(&db, "s1", 50, 0).await.unwrap();
        assert_eq!(only_s1.len(), 1);
        assert_eq!(only_s1[0].request_id, "r1");
    }

    #[tokio::test]
    async fn test_get_record_snapshot_rejects_cross_server() {
        let db = fresh_db().await;
        insert_completed_record(&db, new_record("s1", "r1", "1.1.1.1")).await.unwrap();
        let none = get_record_snapshot(&db, "s2", "r1").await.unwrap();
        assert!(none.is_none(), "must not return a record under wrong server scope");
    }

    #[tokio::test]
    async fn test_delete_record_rejects_cross_server() {
        let db = fresh_db().await;
        insert_completed_record(&db, new_record("s1", "r1", "1.1.1.1")).await.unwrap();
        let err = delete_record(&db, "s2", "r1").await.unwrap_err();
        match err {
            AppError::NotFound(_) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
        // Original row still present
        assert!(get_record_snapshot(&db, "s1", "r1").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_delete_records_for_server_returns_count() {
        let db = fresh_db().await;
        insert_completed_record(&db, new_record("s1", "r1", "1.1.1.1")).await.unwrap();
        insert_completed_record(&db, new_record("s1", "r2", "8.8.8.8")).await.unwrap();
        insert_completed_record(&db, new_record("s2", "r3", "9.9.9.9")).await.unwrap();
        let n = delete_records_for_server(&db, "s1").await.unwrap();
        assert_eq!(n, 2);
        let remaining = list_records_for_server(&db, "s1", 50, 0).await.unwrap();
        assert!(remaining.is_empty());
    }
}
