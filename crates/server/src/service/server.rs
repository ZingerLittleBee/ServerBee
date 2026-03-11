use chrono::Utc;
use sea_orm::*;
use serde::Deserialize;

use crate::entity::server;
use crate::error::AppError;
use serverbee_common::types::SystemInfo;

#[derive(Debug, Deserialize)]
pub struct UpdateServerInput {
    pub name: Option<String>,
    pub group_id: Option<Option<String>>,
    pub weight: Option<i32>,
    pub hidden: Option<bool>,
    pub remark: Option<String>,
    pub public_remark: Option<String>,
}

pub struct ServerService;

impl ServerService {
    /// List all servers ordered by weight DESC, then created_at DESC.
    pub async fn list_servers(
        db: &DatabaseConnection,
    ) -> Result<Vec<server::Model>, AppError> {
        let servers = server::Entity::find()
            .order_by_desc(server::Column::Weight)
            .order_by_desc(server::Column::CreatedAt)
            .all(db)
            .await?;
        Ok(servers)
    }

    /// Get a server by ID. Returns 404 if not found.
    pub async fn get_server(
        db: &DatabaseConnection,
        id: &str,
    ) -> Result<server::Model, AppError> {
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

        active.updated_at = Set(Utc::now());
        let updated = active.update(db).await?;
        Ok(updated)
    }

    /// Delete a server by ID.
    pub async fn delete_server(
        db: &DatabaseConnection,
        id: &str,
    ) -> Result<(), AppError> {
        let result = server::Entity::delete_by_id(id).exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound("Server not found".to_string()));
        }
        Ok(())
    }

    /// Batch delete servers by IDs.
    pub async fn batch_delete(
        db: &DatabaseConnection,
        ids: &[String],
    ) -> Result<u64, AppError> {
        if ids.is_empty() {
            return Ok(0);
        }
        let result = server::Entity::delete_many()
            .filter(server::Column::Id.is_in(ids.iter().cloned()))
            .exec(db)
            .await?;
        Ok(result.rows_affected)
    }

    /// Update system info for a server from an agent report.
    pub async fn update_system_info(
        db: &DatabaseConnection,
        server_id: &str,
        info: &SystemInfo,
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
        active.updated_at = Set(Utc::now());

        active.update(db).await?;
        Ok(())
    }
}
