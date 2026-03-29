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

    /// Delete a server by ID.
    pub async fn delete_server(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let result = server::Entity::delete_by_id(id).exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound("Server not found".to_string()));
        }
        Ok(())
    }

    /// Batch delete servers by IDs.
    pub async fn batch_delete(db: &DatabaseConnection, ids: &[String]) -> Result<u64, AppError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let result = server::Entity::delete_many()
            .filter(server::Column::Id.is_in(ids.iter().cloned()))
            .exec(db)
            .await?;
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
}
