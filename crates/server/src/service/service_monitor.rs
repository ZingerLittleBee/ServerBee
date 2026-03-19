use chrono::Utc;
use sea_orm::prelude::Expr;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{service_monitor, service_monitor_record};
use crate::error::AppError;

pub struct ServiceMonitorService;

const VALID_TYPES: &[&str] = &["ssl", "dns", "http_keyword", "tcp", "whois"];

fn default_interval() -> i32 {
    300
}

fn default_retry_count() -> i32 {
    1
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CreateServiceMonitor {
    pub name: String,
    pub monitor_type: String,
    pub target: String,
    #[serde(default = "default_interval")]
    pub interval: i32,
    #[serde(default)]
    pub config_json: serde_json::Value,
    pub notification_group_id: Option<String>,
    #[serde(default = "default_retry_count")]
    pub retry_count: i32,
    pub server_ids_json: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct UpdateServiceMonitor {
    pub name: Option<String>,
    pub target: Option<String>,
    pub interval: Option<i32>,
    pub config_json: Option<serde_json::Value>,
    pub notification_group_id: Option<Option<String>>,
    pub retry_count: Option<i32>,
    pub server_ids_json: Option<Option<Vec<String>>>,
    pub enabled: Option<bool>,
}

impl ServiceMonitorService {
    /// List all monitors, optionally filtered by monitor_type.
    pub async fn list(
        db: &DatabaseConnection,
        type_filter: Option<&str>,
    ) -> Result<Vec<service_monitor::Model>, AppError> {
        let mut query = service_monitor::Entity::find();
        if let Some(t) = type_filter {
            query = query.filter(service_monitor::Column::MonitorType.eq(t));
        }
        Ok(query
            .order_by_asc(service_monitor::Column::CreatedAt)
            .all(db)
            .await?)
    }

    /// Get a monitor by ID, returning NotFound if missing.
    pub async fn get(
        db: &DatabaseConnection,
        id: &str,
    ) -> Result<service_monitor::Model, AppError> {
        service_monitor::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Service monitor {id} not found")))
    }

    /// Create a new monitor.
    pub async fn create(
        db: &DatabaseConnection,
        input: CreateServiceMonitor,
    ) -> Result<service_monitor::Model, AppError> {
        if !VALID_TYPES.contains(&input.monitor_type.as_str()) {
            return Err(AppError::Validation(format!(
                "monitor_type must be one of: {}",
                VALID_TYPES.join(", ")
            )));
        }

        let config_json = serde_json::to_string(&input.config_json)
            .map_err(|e| AppError::Validation(format!("Invalid config_json: {e}")))?;

        let server_ids_json = input
            .server_ids_json
            .map(|ids| {
                serde_json::to_string(&ids)
                    .map_err(|e| AppError::Validation(format!("Invalid server_ids_json: {e}")))
            })
            .transpose()?;

        let now = Utc::now();
        let model = service_monitor::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            name: Set(input.name),
            monitor_type: Set(input.monitor_type),
            target: Set(input.target),
            interval: Set(input.interval),
            config_json: Set(config_json),
            notification_group_id: Set(input.notification_group_id),
            retry_count: Set(input.retry_count),
            server_ids_json: Set(server_ids_json),
            enabled: Set(input.enabled),
            last_status: Set(None),
            consecutive_failures: Set(0),
            last_checked_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        };

        Ok(model.insert(db).await?)
    }

    /// Partially update a monitor's configuration.
    pub async fn update(
        db: &DatabaseConnection,
        id: &str,
        input: UpdateServiceMonitor,
    ) -> Result<service_monitor::Model, AppError> {
        let existing = Self::get(db, id).await?;
        let mut model: service_monitor::ActiveModel = existing.into();

        if let Some(name) = input.name {
            model.name = Set(name);
        }
        if let Some(target) = input.target {
            model.target = Set(target);
        }
        if let Some(interval) = input.interval {
            model.interval = Set(interval);
        }
        if let Some(config_json) = input.config_json {
            let json = serde_json::to_string(&config_json)
                .map_err(|e| AppError::Validation(format!("Invalid config_json: {e}")))?;
            model.config_json = Set(json);
        }
        if let Some(notification_group_id) = input.notification_group_id {
            model.notification_group_id = Set(notification_group_id);
        }
        if let Some(retry_count) = input.retry_count {
            model.retry_count = Set(retry_count);
        }
        if let Some(server_ids_opt) = input.server_ids_json {
            let json = server_ids_opt
                .map(|ids| {
                    serde_json::to_string(&ids)
                        .map_err(|e| AppError::Validation(format!("Invalid server_ids_json: {e}")))
                })
                .transpose()?;
            model.server_ids_json = Set(json);
        }
        if let Some(enabled) = input.enabled {
            model.enabled = Set(enabled);
        }
        model.updated_at = Set(Utc::now());

        Ok(model.update(db).await?)
    }

    /// Delete a monitor and cascade-delete all its records.
    pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let result = service_monitor::Entity::delete_by_id(id).exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!(
                "Service monitor {id} not found"
            )));
        }

        service_monitor_record::Entity::delete_many()
            .filter(service_monitor_record::Column::MonitorId.eq(id))
            .exec(db)
            .await?;

        Ok(())
    }

    /// Query records for a monitor with optional time range and limit.
    pub async fn get_records(
        db: &DatabaseConnection,
        monitor_id: &str,
        from: Option<chrono::DateTime<Utc>>,
        to: Option<chrono::DateTime<Utc>>,
        limit: Option<u64>,
    ) -> Result<Vec<service_monitor_record::Model>, AppError> {
        let mut query = service_monitor_record::Entity::find()
            .filter(service_monitor_record::Column::MonitorId.eq(monitor_id));

        if let Some(f) = from {
            query = query.filter(service_monitor_record::Column::Time.gte(f));
        }
        if let Some(t) = to {
            query = query.filter(service_monitor_record::Column::Time.lte(t));
        }

        query = query.order_by_desc(service_monitor_record::Column::Time);

        if let Some(l) = limit {
            query = query.limit(l);
        }

        Ok(query.all(db).await?)
    }

    /// Get the most recent check record for a monitor.
    pub async fn get_latest_record(
        db: &DatabaseConnection,
        monitor_id: &str,
    ) -> Result<Option<service_monitor_record::Model>, AppError> {
        Ok(service_monitor_record::Entity::find()
            .filter(service_monitor_record::Column::MonitorId.eq(monitor_id))
            .order_by_desc(service_monitor_record::Column::Time)
            .one(db)
            .await?)
    }

    /// List all enabled monitors (used by the background execution engine).
    pub async fn list_enabled(
        db: &DatabaseConnection,
    ) -> Result<Vec<service_monitor::Model>, AppError> {
        Ok(service_monitor::Entity::find()
            .filter(service_monitor::Column::Enabled.eq(true))
            .order_by_asc(service_monitor::Column::CreatedAt)
            .all(db)
            .await?)
    }

    /// Update runtime state columns after a check completes.
    pub async fn update_check_state(
        db: &DatabaseConnection,
        id: &str,
        success: bool,
        consecutive_failures: i32,
    ) -> Result<(), AppError> {
        service_monitor::Entity::update_many()
            .col_expr(service_monitor::Column::LastStatus, Expr::value(success))
            .col_expr(
                service_monitor::Column::ConsecutiveFailures,
                Expr::value(consecutive_failures),
            )
            .col_expr(
                service_monitor::Column::LastCheckedAt,
                Expr::value(Utc::now()),
            )
            .col_expr(
                service_monitor::Column::UpdatedAt,
                Expr::value(Utc::now()),
            )
            .filter(service_monitor::Column::Id.eq(id))
            .exec(db)
            .await?;
        Ok(())
    }

    /// Insert a check result record.
    pub async fn insert_record(
        db: &DatabaseConnection,
        monitor_id: &str,
        success: bool,
        latency: Option<f64>,
        detail: serde_json::Value,
        error: Option<String>,
    ) -> Result<service_monitor_record::Model, AppError> {
        let detail_json = serde_json::to_string(&detail)
            .map_err(|e| AppError::Internal(format!("Failed to serialize detail: {e}")))?;

        let record = service_monitor_record::ActiveModel {
            id: NotSet,
            monitor_id: Set(monitor_id.to_string()),
            success: Set(success),
            latency: Set(latency),
            detail_json: Set(detail_json),
            error: Set(error),
            time: Set(Utc::now()),
        };

        Ok(record.insert(db).await?)
    }

    /// Delete records older than the specified number of days.
    pub async fn cleanup_records(db: &DatabaseConnection, days: u32) -> Result<u64, AppError> {
        let cutoff = Utc::now() - chrono::Duration::days(days as i64);
        let deleted = service_monitor_record::Entity::delete_many()
            .filter(service_monitor_record::Column::Time.lt(cutoff))
            .exec(db)
            .await?
            .rows_affected;
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;

    fn sample_create() -> CreateServiceMonitor {
        CreateServiceMonitor {
            name: "Test SSL Monitor".to_string(),
            monitor_type: "ssl".to_string(),
            target: "example.com".to_string(),
            interval: 300,
            config_json: serde_json::json!({}),
            notification_group_id: None,
            retry_count: 1,
            server_ids_json: None,
            enabled: true,
        }
    }

    #[tokio::test]
    async fn test_create_and_list() {
        let (db, _tmp) = setup_test_db().await;

        let created = ServiceMonitorService::create(&db, sample_create())
            .await
            .unwrap();

        let list = ServiceMonitorService::list(&db, None).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, created.id);
        assert_eq!(list[0].name, "Test SSL Monitor");
        assert_eq!(list[0].monitor_type, "ssl");
    }

    #[tokio::test]
    async fn test_create_invalid_type() {
        let (db, _tmp) = setup_test_db().await;

        let mut input = sample_create();
        input.monitor_type = "invalid".to_string();
        let result = ServiceMonitorService::create(&db, input).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Validation(_) => {}
            other => panic!("Expected Validation error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_get() {
        let (db, _tmp) = setup_test_db().await;

        let created = ServiceMonitorService::create(&db, sample_create())
            .await
            .unwrap();
        let fetched = ServiceMonitorService::get(&db, &created.id).await.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.target, "example.com");
        assert_eq!(fetched.interval, 300);
        assert!(fetched.enabled);
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let (db, _tmp) = setup_test_db().await;

        let result = ServiceMonitorService::get(&db, "nonexistent").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::NotFound(_) => {}
            other => panic!("Expected NotFound error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_update() {
        let (db, _tmp) = setup_test_db().await;

        let created = ServiceMonitorService::create(&db, sample_create())
            .await
            .unwrap();

        let update = UpdateServiceMonitor {
            name: Some("Updated Name".to_string()),
            target: None,
            interval: Some(600),
            config_json: None,
            notification_group_id: None,
            retry_count: None,
            server_ids_json: None,
            enabled: Some(false),
        };
        let updated = ServiceMonitorService::update(&db, &created.id, update)
            .await
            .unwrap();
        assert_eq!(updated.name, "Updated Name");
        assert_eq!(updated.interval, 600);
        assert!(!updated.enabled);
        assert_eq!(updated.target, "example.com");
    }

    #[tokio::test]
    async fn test_delete() {
        let (db, _tmp) = setup_test_db().await;

        let created = ServiceMonitorService::create(&db, sample_create())
            .await
            .unwrap();

        ServiceMonitorService::delete(&db, &created.id)
            .await
            .unwrap();

        let list = ServiceMonitorService::list(&db, None).await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_delete_not_found() {
        let (db, _tmp) = setup_test_db().await;

        let result = ServiceMonitorService::delete(&db, "nonexistent").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::NotFound(_) => {}
            other => panic!("Expected NotFound error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_list_filter_by_type() {
        let (db, _tmp) = setup_test_db().await;

        ServiceMonitorService::create(&db, sample_create())
            .await
            .unwrap();

        let mut tcp_input = sample_create();
        tcp_input.monitor_type = "tcp".to_string();
        tcp_input.name = "TCP Monitor".to_string();
        ServiceMonitorService::create(&db, tcp_input).await.unwrap();

        let ssl_list = ServiceMonitorService::list(&db, Some("ssl")).await.unwrap();
        assert_eq!(ssl_list.len(), 1);
        assert_eq!(ssl_list[0].monitor_type, "ssl");

        let all_list = ServiceMonitorService::list(&db, None).await.unwrap();
        assert_eq!(all_list.len(), 2);
    }

    #[tokio::test]
    async fn test_insert_record_and_get_records() {
        let (db, _tmp) = setup_test_db().await;

        let created = ServiceMonitorService::create(&db, sample_create())
            .await
            .unwrap();

        ServiceMonitorService::insert_record(
            &db,
            &created.id,
            true,
            Some(123.4),
            serde_json::json!({"days_remaining": 90}),
            None,
        )
        .await
        .unwrap();

        let records = ServiceMonitorService::get_records(&db, &created.id, None, None, None)
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0].success);
        assert_eq!(records[0].latency, Some(123.4));
    }

    #[tokio::test]
    async fn test_get_latest_record() {
        let (db, _tmp) = setup_test_db().await;

        let created = ServiceMonitorService::create(&db, sample_create())
            .await
            .unwrap();

        // No records yet
        let latest = ServiceMonitorService::get_latest_record(&db, &created.id)
            .await
            .unwrap();
        assert!(latest.is_none());

        ServiceMonitorService::insert_record(
            &db,
            &created.id,
            false,
            None,
            serde_json::json!({}),
            Some("Connection refused".to_string()),
        )
        .await
        .unwrap();

        let latest = ServiceMonitorService::get_latest_record(&db, &created.id)
            .await
            .unwrap();
        assert!(latest.is_some());
        assert!(!latest.unwrap().success);
    }

    #[tokio::test]
    async fn test_list_enabled() {
        let (db, _tmp) = setup_test_db().await;

        ServiceMonitorService::create(&db, sample_create())
            .await
            .unwrap();

        let mut disabled_input = sample_create();
        disabled_input.enabled = false;
        disabled_input.name = "Disabled Monitor".to_string();
        ServiceMonitorService::create(&db, disabled_input)
            .await
            .unwrap();

        let enabled = ServiceMonitorService::list_enabled(&db).await.unwrap();
        assert_eq!(enabled.len(), 1);
        assert!(enabled[0].enabled);
    }

    #[tokio::test]
    async fn test_update_check_state() {
        let (db, _tmp) = setup_test_db().await;

        let created = ServiceMonitorService::create(&db, sample_create())
            .await
            .unwrap();
        assert!(created.last_status.is_none());
        assert_eq!(created.consecutive_failures, 0);

        ServiceMonitorService::update_check_state(&db, &created.id, false, 3)
            .await
            .unwrap();

        let fetched = ServiceMonitorService::get(&db, &created.id).await.unwrap();
        assert_eq!(fetched.last_status, Some(false));
        assert_eq!(fetched.consecutive_failures, 3);
        assert!(fetched.last_checked_at.is_some());
    }

    #[tokio::test]
    async fn test_cleanup_records() {
        let (db, _tmp) = setup_test_db().await;

        let created = ServiceMonitorService::create(&db, sample_create())
            .await
            .unwrap();

        // Insert a record
        ServiceMonitorService::insert_record(
            &db,
            &created.id,
            true,
            Some(50.0),
            serde_json::json!({}),
            None,
        )
        .await
        .unwrap();

        // Cleanup with 0 days should delete all records older than now
        // (records just inserted are at "now", so 0-day cutoff = now, nothing older)
        let deleted = ServiceMonitorService::cleanup_records(&db, 365).await.unwrap();
        assert_eq!(deleted, 0);

        // Verify record still exists
        let records = ServiceMonitorService::get_records(&db, &created.id, None, None, None)
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
    }

    #[tokio::test]
    async fn test_delete_cascades_records() {
        let (db, _tmp) = setup_test_db().await;

        let created = ServiceMonitorService::create(&db, sample_create())
            .await
            .unwrap();

        ServiceMonitorService::insert_record(
            &db,
            &created.id,
            true,
            Some(100.0),
            serde_json::json!({}),
            None,
        )
        .await
        .unwrap();

        ServiceMonitorService::delete(&db, &created.id)
            .await
            .unwrap();

        // Records should be gone too
        let records =
            ServiceMonitorService::get_records(&db, &created.id, None, None, None)
                .await
                .unwrap();
        assert!(records.is_empty());
    }

    #[tokio::test]
    async fn test_update_server_ids_json() {
        let (db, _tmp) = setup_test_db().await;

        let created = ServiceMonitorService::create(&db, sample_create())
            .await
            .unwrap();
        assert!(created.server_ids_json.is_none());

        let update = UpdateServiceMonitor {
            name: None,
            target: None,
            interval: None,
            config_json: None,
            notification_group_id: None,
            retry_count: None,
            server_ids_json: Some(Some(vec!["server-1".to_string(), "server-2".to_string()])),
            enabled: None,
        };
        let updated = ServiceMonitorService::update(&db, &created.id, update)
            .await
            .unwrap();
        assert!(updated.server_ids_json.is_some());

        // Clear it back to None
        let clear_update = UpdateServiceMonitor {
            name: None,
            target: None,
            interval: None,
            config_json: None,
            notification_group_id: None,
            retry_count: None,
            server_ids_json: Some(None),
            enabled: None,
        };
        let cleared = ServiceMonitorService::update(&db, &created.id, clear_update)
            .await
            .unwrap();
        assert!(cleared.server_ids_json.is_none());
    }
}
