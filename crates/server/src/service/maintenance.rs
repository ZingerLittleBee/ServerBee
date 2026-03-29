use chrono::{DateTime, Utc};
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::maintenance;
use crate::error::AppError;

pub struct MaintenanceService;

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CreateMaintenance {
    pub title: String,
    pub description: Option<String>,
    #[schema(value_type = String, format = DateTime)]
    pub start_at: DateTime<Utc>,
    #[schema(value_type = String, format = DateTime)]
    pub end_at: DateTime<Utc>,
    pub server_ids_json: Option<Vec<String>>,
    pub status_page_ids_json: Option<Vec<String>>,
    pub active: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct UpdateMaintenance {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    #[schema(value_type = Option<String>, format = DateTime)]
    pub start_at: Option<DateTime<Utc>>,
    #[schema(value_type = Option<String>, format = DateTime)]
    pub end_at: Option<DateTime<Utc>>,
    pub server_ids_json: Option<Option<Vec<String>>>,
    pub status_page_ids_json: Option<Option<Vec<String>>>,
    pub active: Option<bool>,
}

impl MaintenanceService {
    pub async fn list(db: &DatabaseConnection) -> Result<Vec<maintenance::Model>, AppError> {
        Ok(maintenance::Entity::find()
            .order_by_desc(maintenance::Column::CreatedAt)
            .all(db)
            .await?)
    }

    pub async fn get(db: &DatabaseConnection, id: &str) -> Result<maintenance::Model, AppError> {
        maintenance::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Maintenance {id} not found")))
    }

    pub async fn create(
        db: &DatabaseConnection,
        input: CreateMaintenance,
    ) -> Result<maintenance::Model, AppError> {
        if input.end_at <= input.start_at {
            return Err(AppError::Validation(
                "end_at must be after start_at".to_string(),
            ));
        }

        let server_ids_json = input
            .server_ids_json
            .map(|ids| {
                serde_json::to_string(&ids)
                    .map_err(|e| AppError::Validation(format!("Invalid server_ids_json: {e}")))
            })
            .transpose()?;

        let status_page_ids_json = input
            .status_page_ids_json
            .map(|ids| {
                serde_json::to_string(&ids)
                    .map_err(|e| AppError::Validation(format!("Invalid status_page_ids_json: {e}")))
            })
            .transpose()?;

        let now = Utc::now();
        let model = maintenance::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            title: Set(input.title),
            description: Set(input.description),
            start_at: Set(input.start_at),
            end_at: Set(input.end_at),
            server_ids_json: Set(server_ids_json),
            status_page_ids_json: Set(status_page_ids_json),
            active: Set(input.active.unwrap_or(true)),
            created_at: Set(now),
            updated_at: Set(now),
        };

        Ok(model.insert(db).await?)
    }

    pub async fn update(
        db: &DatabaseConnection,
        id: &str,
        input: UpdateMaintenance,
    ) -> Result<maintenance::Model, AppError> {
        let existing = Self::get(db, id).await?;
        let mut model: maintenance::ActiveModel = existing.into();

        if let Some(title) = input.title {
            model.title = Set(title);
        }
        if let Some(description) = input.description {
            model.description = Set(description);
        }
        if let Some(start_at) = input.start_at {
            model.start_at = Set(start_at);
        }
        if let Some(end_at) = input.end_at {
            model.end_at = Set(end_at);
        }
        if let Some(server_ids) = input.server_ids_json {
            let json = server_ids
                .map(|ids| {
                    serde_json::to_string(&ids)
                        .map_err(|e| AppError::Validation(format!("Invalid server_ids_json: {e}")))
                })
                .transpose()?;
            model.server_ids_json = Set(json);
        }
        if let Some(page_ids) = input.status_page_ids_json {
            let json = page_ids
                .map(|ids| {
                    serde_json::to_string(&ids).map_err(|e| {
                        AppError::Validation(format!("Invalid status_page_ids_json: {e}"))
                    })
                })
                .transpose()?;
            model.status_page_ids_json = Set(json);
        }
        if let Some(active) = input.active {
            model.active = Set(active);
        }
        model.updated_at = Set(Utc::now());

        Ok(model.update(db).await?)
    }

    pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let result = maintenance::Entity::delete_by_id(id).exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!("Maintenance {id} not found")));
        }
        Ok(())
    }

    /// Check if a server is currently in an active maintenance window.
    /// Returns true if there is at least one maintenance where:
    ///   active = true AND start_at <= now AND end_at >= now AND
    ///   (server_ids_json IS NULL OR server_ids_json contains the server_id)
    pub async fn is_in_maintenance(
        db: &DatabaseConnection,
        server_id: &str,
    ) -> Result<bool, AppError> {
        let now = Utc::now();

        let maintenances = maintenance::Entity::find()
            .filter(maintenance::Column::Active.eq(true))
            .filter(maintenance::Column::StartAt.lte(now))
            .filter(maintenance::Column::EndAt.gte(now))
            .all(db)
            .await?;

        for m in &maintenances {
            match &m.server_ids_json {
                None => {
                    // No server filter — applies to all servers
                    return Ok(true);
                }
                Some(json) => {
                    let ids: Vec<String> = serde_json::from_str(json).unwrap_or_default();
                    if ids.is_empty() || ids.iter().any(|id| id == server_id) {
                        return Ok(true);
                    }
                }
            }
        }

        Ok(false)
    }
}
