use chrono::Utc;
use sea_orm::prelude::Expr;
use sea_orm::*;
use serde::{Deserialize, Deserializer};

use crate::entity::server;
use crate::error::AppError;
use serverbee_common::types::SystemInfo;

/// Deserialize a field that distinguishes between absent (None), explicit null (Some(None)),
/// and a present value (Some(Some(v))).
fn deserialize_optional_nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    // If the field is present in JSON (even as null), this function is called.
    // JSON null → Ok(Some(None)), JSON value → Ok(Some(Some(v)))
    Ok(Some(Option::deserialize(deserializer)?))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateServerInput {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub group_id: Option<Option<String>>,
    pub weight: Option<i32>,
    pub hidden: Option<bool>,
    pub remark: Option<String>,
    pub public_remark: Option<String>,
    // Billing fields
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub price: Option<Option<f64>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub billing_cycle: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub currency: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub expired_at: Option<Option<chrono::DateTime<chrono::Utc>>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub traffic_limit: Option<Option<i64>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub traffic_limit_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub billing_start_day: Option<Option<i32>>,
    pub capabilities: Option<i32>,
}

pub struct ServerService;

impl ServerService {
    /// List all servers ordered by weight DESC, then created_at DESC.
    pub async fn list_servers(db: &DatabaseConnection) -> Result<Vec<server::Model>, AppError> {
        let servers = server::Entity::find()
            .order_by_desc(server::Column::Weight)
            .order_by_desc(server::Column::CreatedAt)
            .all(db)
            .await?;
        Ok(servers)
    }

    /// Get a server by ID. Returns 404 if not found.
    pub async fn get_server(db: &DatabaseConnection, id: &str) -> Result<server::Model, AppError> {
        server::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound("Server not found".to_string()))
    }

    /// Update a server's fields.
    pub async fn update_server(
        db: &DatabaseConnection,
        id: &str,
        input: UpdateServerInput,
    ) -> Result<server::Model, AppError> {
        let model = Self::get_server(db, id).await?;
        Self::validate_update_input(&input)?;

        let mut active: server::ActiveModel = model.into();

        if let Some(name) = input.name {
            active.name = Set(name);
        }
        if let Some(group_id) = input.group_id {
            active.group_id = Set(group_id);
        }
        if let Some(weight) = input.weight {
            active.weight = Set(weight);
        }
        if let Some(hidden) = input.hidden {
            active.hidden = Set(hidden);
        }
        if let Some(remark) = input.remark {
            active.remark = Set(Some(remark));
        }
        if let Some(public_remark) = input.public_remark {
            active.public_remark = Set(Some(public_remark));
        }
        if let Some(price) = input.price {
            active.price = Set(price);
        }
        if let Some(billing_cycle) = input.billing_cycle {
            active.billing_cycle = Set(billing_cycle);
        }
        if let Some(currency) = input.currency {
            active.currency = Set(currency);
        }
        if let Some(expired_at) = input.expired_at {
            active.expired_at = Set(expired_at);
        }
        if let Some(traffic_limit) = input.traffic_limit {
            active.traffic_limit = Set(traffic_limit);
        }
        if let Some(traffic_limit_type) = input.traffic_limit_type {
            active.traffic_limit_type = Set(traffic_limit_type);
        }
        if let Some(billing_start_day) = input.billing_start_day {
            active.billing_start_day = Set(billing_start_day);
        }
        if let Some(caps) = input.capabilities {
            let caps_u32 = caps as u32;
            if caps_u32 & !serverbee_common::constants::CAP_VALID_MASK != 0 {
                return Err(AppError::Validation("Invalid capability bits".into()));
            }
            active.capabilities = Set(caps);
        }

        active.updated_at = Set(Utc::now());
        let updated = active.update(db).await?;
        Ok(updated)
    }

    fn validate_update_input(input: &UpdateServerInput) -> Result<(), AppError> {
        if matches!(input.price, Some(Some(price)) if !price.is_finite() || price < 0.0) {
            return Err(AppError::Validation(
                "price must be finite and greater than or equal to 0".into(),
            ));
        }

        if matches!(
            input.billing_cycle.as_ref(),
            Some(Some(billing_cycle))
                if !matches!(billing_cycle.as_str(), "monthly" | "quarterly" | "yearly")
        ) {
            return Err(AppError::Validation(
                "billing_cycle must be monthly, quarterly, or yearly".into(),
            ));
        }

        if matches!(
            input.traffic_limit_type.as_ref(),
            Some(Some(traffic_limit_type))
                if !matches!(traffic_limit_type.as_str(), "sum" | "up" | "down")
        ) {
            return Err(AppError::Validation(
                "traffic_limit_type must be sum, up, or down".into(),
            ));
        }

        if matches!(input.billing_start_day, Some(Some(day)) if !(1..=28).contains(&day)) {
            return Err(AppError::Validation(
                "billing_start_day must be between 1 and 28".into(),
            ));
        }

        Ok(())
    }

    /// Delete every server-scoped row (raw/aggregated metrics, per-server
    /// config, traffic, uptime, etc.) for the given server ids.
    ///
    /// These tables intentionally have no foreign key to `servers`, so a
    /// server delete does not cascade and would otherwise leave orphaned
    /// rows that only age out via the time-based cleanup task. recovery_job
    /// rows referencing the server via target/source are purged too (running
    /// recoveries are blocked separately by [`Self::ensure_no_running_recovery`]).
    /// Multi-server association tables (alert_rules, incident, maintenance,
    /// ping_tasks, service_monitor, status_page, tasks) are deliberately
    /// excluded: their rows stay valid for the other servers they reference
    /// and are not orphans of this delete.
    async fn delete_server_scoped_rows<C: ConnectionTrait>(
        conn: &C,
        ids: &[String],
    ) -> Result<(), AppError> {
        use crate::entity::{
            alert_state, docker_event, gpu_record, network_probe_config, network_probe_record,
            network_probe_record_hourly, ping_record, record, record_hourly, recovery_job,
            task_result, traffic_daily, traffic_hourly, traffic_state, uptime_daily,
        };

        macro_rules! purge {
            ($ent:ident) => {
                $ent::Entity::delete_many()
                    .filter($ent::Column::ServerId.is_in(ids.iter().cloned()))
                    .exec(conn)
                    .await?;
            };
        }

        purge!(alert_state);
        purge!(docker_event);
        purge!(gpu_record);
        purge!(network_probe_config);
        purge!(network_probe_record);
        purge!(network_probe_record_hourly);
        purge!(ping_record);
        purge!(record);
        purge!(record_hourly);
        purge!(task_result);
        purge!(traffic_daily);
        purge!(traffic_hourly);
        purge!(traffic_state);
        purge!(uptime_daily);

        // recovery_job references servers via two columns, not `server_id`.
        recovery_job::Entity::delete_many()
            .filter(
                Condition::any()
                    .add(recovery_job::Column::TargetServerId.is_in(ids.iter().cloned()))
                    .add(recovery_job::Column::SourceServerId.is_in(ids.iter().cloned())),
            )
            .exec(conn)
            .await?;

        Ok(())
    }

    /// Reject deleting a server that is the target or source of a currently
    /// running recovery job: tearing its rows out mid-run would leave the
    /// recovery task's guards and browser sync in a stale active state.
    async fn ensure_no_running_recovery<C: ConnectionTrait>(
        conn: &C,
        ids: &[String],
    ) -> Result<(), AppError> {
        use crate::entity::recovery_job;

        let running = recovery_job::Entity::find()
            .filter(recovery_job::Column::Status.eq("running"))
            .filter(
                Condition::any()
                    .add(recovery_job::Column::TargetServerId.is_in(ids.iter().cloned()))
                    .add(recovery_job::Column::SourceServerId.is_in(ids.iter().cloned())),
            )
            .count(conn)
            .await?;

        if running > 0 {
            return Err(AppError::Conflict(
                "Cannot delete a server that is part of a running recovery job".to_string(),
            ));
        }
        Ok(())
    }

    /// Delete a server by ID, along with all of its server-scoped data.
    pub async fn delete_server(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let ids = [id.to_string()];
        let txn = db.begin().await?;
        Self::ensure_no_running_recovery(&txn, &ids).await?;
        let result = server::Entity::delete_by_id(id).exec(&txn).await?;
        if result.rows_affected == 0 {
            txn.rollback().await?;
            return Err(AppError::NotFound("Server not found".to_string()));
        }
        Self::delete_server_scoped_rows(&txn, &ids).await?;
        txn.commit().await?;
        Ok(())
    }

    /// Batch delete servers by IDs, along with all of their server-scoped data.
    pub async fn batch_delete(db: &DatabaseConnection, ids: &[String]) -> Result<u64, AppError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let txn = db.begin().await?;
        Self::ensure_no_running_recovery(&txn, ids).await?;
        let result = server::Entity::delete_many()
            .filter(server::Column::Id.is_in(ids.iter().cloned()))
            .exec(&txn)
            .await?;
        Self::delete_server_scoped_rows(&txn, ids).await?;
        txn.commit().await?;
        Ok(result.rows_affected)
    }

    /// Update the features list for a server.
    pub async fn update_features(
        db: &DatabaseConnection,
        server_id: &str,
        features: &[String],
    ) -> Result<(), DbErr> {
        let features_json = serde_json::to_string(features).unwrap_or_else(|_| "[]".into());
        server::Entity::update_many()
            .filter(server::Column::Id.eq(server_id))
            .col_expr(server::Column::Features, Expr::value(features_json))
            .exec(db)
            .await?;
        Ok(())
    }

    /// Update system info for a server from an agent report.
    pub async fn update_system_info(
        db: &DatabaseConnection,
        server_id: &str,
        info: &SystemInfo,
        region: Option<String>,
        country_code: Option<String>,
    ) -> Result<(), AppError> {
        let model = Self::get_server(db, server_id).await?;
        let mut active: server::ActiveModel = model.into();

        active.cpu_name = Set(Some(info.cpu_name.clone()));
        active.cpu_cores = Set(Some(info.cpu_cores));
        active.cpu_arch = Set(Some(info.cpu_arch.clone()));
        active.os = Set(Some(info.os.clone()));
        active.kernel_version = Set(Some(info.kernel_version.clone()));
        active.mem_total = Set(Some(info.mem_total));
        active.swap_total = Set(Some(info.swap_total));
        active.disk_total = Set(Some(info.disk_total));
        active.ipv4 = Set(info.ipv4.clone());
        active.ipv6 = Set(info.ipv6.clone());
        active.virtualization = Set(info.virtualization.clone());
        active.agent_version = Set(Some(info.agent_version.clone()));
        active.region = Set(region);
        active.country_code = Set(country_code);
        active.protocol_version = Set(info.protocol_version as i32);
        active.updated_at = Set(Utc::now());

        active.update(db).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;
    use chrono::Utc;
    use sea_orm::ActiveModelTrait;
    use sea_orm::Set;
    use serverbee_common::constants::CAP_DEFAULT;

    async fn insert_test_server(db: &DatabaseConnection, id: &str, name: &str) {
        let token_hash = AuthService::hash_password("test").expect("hash_password should succeed");
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(token_hash),
            token_prefix: Set("serverbee_test".to_string()),
            name: Set(name.to_string()),
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
        .expect("insert test server should succeed");
    }

    fn update_input() -> UpdateServerInput {
        UpdateServerInput {
            name: None,
            group_id: None,
            weight: None,
            hidden: None,
            remark: None,
            public_remark: None,
            price: None,
            billing_cycle: None,
            currency: None,
            expired_at: None,
            traffic_limit: None,
            traffic_limit_type: None,
            billing_start_day: None,
            capabilities: None,
        }
    }

    fn validation_message(result: Result<server::Model, AppError>) -> String {
        match result {
            Err(AppError::Validation(message)) => message,
            Err(error) => panic!("expected validation error, got {error:?}"),
            Ok(_) => panic!("expected validation error, got success"),
        }
    }

    #[tokio::test]
    async fn test_list_servers() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-list-1", "Test Server List").await;

        let servers = ServerService::list_servers(&db)
            .await
            .expect("list_servers should succeed");
        assert!(!servers.is_empty(), "Should return at least one server");
        assert!(
            servers.iter().any(|s| s.id == "srv-list-1"),
            "Inserted server should be in list"
        );
    }

    #[tokio::test]
    async fn test_get_server_found() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-get-1", "Test Server Get").await;

        let server = ServerService::get_server(&db, "srv-get-1")
            .await
            .expect("get_server should succeed");
        assert_eq!(server.id, "srv-get-1");
        assert_eq!(server.name, "Test Server Get");
    }

    #[tokio::test]
    async fn test_get_server_not_found() {
        let (db, _tmp) = setup_test_db().await;

        let result = ServerService::get_server(&db, "nonexistent-id").await;
        assert!(
            result.is_err(),
            "get_server for nonexistent ID should return error"
        );
    }

    #[tokio::test]
    async fn test_delete_server() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-del-1", "Test Server Delete").await;

        ServerService::delete_server(&db, "srv-del-1")
            .await
            .expect("delete_server should succeed");

        let result = ServerService::get_server(&db, "srv-del-1").await;
        assert!(
            result.is_err(),
            "get_server after deletion should return error"
        );
    }

    #[tokio::test]
    async fn test_batch_delete() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-batch-1", "Test Server Batch 1").await;
        insert_test_server(&db, "srv-batch-2", "Test Server Batch 2").await;

        let ids = vec!["srv-batch-1".to_string(), "srv-batch-2".to_string()];
        let rows = ServerService::batch_delete(&db, &ids)
            .await
            .expect("batch_delete should succeed");
        assert_eq!(rows, 2, "Should have deleted 2 rows");

        let result1 = ServerService::get_server(&db, "srv-batch-1").await;
        let result2 = ServerService::get_server(&db, "srv-batch-2").await;
        assert!(result1.is_err(), "First server should be gone");
        assert!(result2.is_err(), "Second server should be gone");
    }

    #[tokio::test]
    async fn update_server_rejects_negative_price() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-price-negative", "Price Negative").await;

        let result = ServerService::update_server(
            &db,
            "srv-price-negative",
            UpdateServerInput {
                price: Some(Some(-0.01)),
                ..update_input()
            },
        )
        .await;

        let message = validation_message(result);
        assert!(
            message.contains("price"),
            "validation message should mention price, got {message}"
        );
    }

    #[tokio::test]
    async fn update_server_rejects_non_finite_price() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-price-non-finite", "Price Non Finite").await;

        for price in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let result = ServerService::update_server(
                &db,
                "srv-price-non-finite",
                UpdateServerInput {
                    price: Some(Some(price)),
                    ..update_input()
                },
            )
            .await;

            let message = validation_message(result);
            assert!(
                message.contains("price"),
                "validation message should mention price, got {message}"
            );
        }
    }

    #[tokio::test]
    async fn update_server_rejects_invalid_billing_cycle() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-billing-cycle-invalid", "Billing Cycle Invalid").await;

        for billing_cycle in ["weekly", ""] {
            let result = ServerService::update_server(
                &db,
                "srv-billing-cycle-invalid",
                UpdateServerInput {
                    billing_cycle: Some(Some(billing_cycle.to_string())),
                    ..update_input()
                },
            )
            .await;

            let message = validation_message(result);
            assert!(
                message.contains("billing_cycle"),
                "validation message should mention billing_cycle, got {message}"
            );
        }
    }

    #[tokio::test]
    async fn update_server_rejects_invalid_traffic_limit_type() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(
            &db,
            "srv-traffic-limit-type-invalid",
            "Traffic Limit Type Invalid",
        )
        .await;

        let result = ServerService::update_server(
            &db,
            "srv-traffic-limit-type-invalid",
            UpdateServerInput {
                traffic_limit_type: Some(Some("total".to_string())),
                ..update_input()
            },
        )
        .await;

        let message = validation_message(result);
        assert!(
            message.contains("traffic_limit_type"),
            "validation message should mention traffic_limit_type, got {message}"
        );
    }

    #[tokio::test]
    async fn update_server_rejects_invalid_billing_start_day() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(
            &db,
            "srv-billing-start-day-invalid",
            "Billing Start Day Invalid",
        )
        .await;

        for billing_start_day in [0, 29] {
            let result = ServerService::update_server(
                &db,
                "srv-billing-start-day-invalid",
                UpdateServerInput {
                    billing_start_day: Some(Some(billing_start_day)),
                    ..update_input()
                },
            )
            .await;

            let message = validation_message(result);
            assert!(
                message.contains("billing_start_day"),
                "validation message should mention billing_start_day, got {message}"
            );
        }
    }

    #[tokio::test]
    async fn update_server_allows_valid_and_cleared_billing_fields() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-billing-valid", "Billing Valid").await;

        for (billing_cycle, traffic_limit_type) in
            [("monthly", "sum"), ("quarterly", "up"), ("yearly", "down")]
        {
            let updated = ServerService::update_server(
                &db,
                "srv-billing-valid",
                UpdateServerInput {
                    price: Some(Some(0.0)),
                    billing_cycle: Some(Some(billing_cycle.to_string())),
                    traffic_limit_type: Some(Some(traffic_limit_type.to_string())),
                    billing_start_day: Some(Some(28)),
                    ..update_input()
                },
            )
            .await
            .expect("valid billing fields should update");

            assert_eq!(updated.price, Some(0.0));
            assert_eq!(updated.billing_cycle.as_deref(), Some(billing_cycle));
            assert_eq!(
                updated.traffic_limit_type.as_deref(),
                Some(traffic_limit_type)
            );
            assert_eq!(updated.billing_start_day, Some(28));
        }

        let updated = ServerService::update_server(
            &db,
            "srv-billing-valid",
            UpdateServerInput {
                price: Some(None),
                billing_cycle: Some(None),
                traffic_limit_type: Some(None),
                billing_start_day: Some(None),
                ..update_input()
            },
        )
        .await
        .expect("explicit null billing fields should clear");

        assert_eq!(updated.price, None);
        assert_eq!(updated.billing_cycle, None);
        assert_eq!(updated.traffic_limit_type, None);
        assert_eq!(updated.billing_start_day, None);
    }
}
