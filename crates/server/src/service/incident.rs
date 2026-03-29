use chrono::Utc;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{incident, incident_update};
use crate::error::AppError;

pub struct IncidentService;

const VALID_STATUSES: &[&str] = &["investigating", "identified", "monitoring", "resolved"];
const VALID_SEVERITIES: &[&str] = &["minor", "major", "critical"];

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CreateIncident {
    pub title: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default = "default_severity")]
    pub severity: String,
    pub server_ids_json: Option<Vec<String>>,
    pub status_page_ids_json: Option<Vec<String>>,
}

fn default_status() -> String {
    "investigating".to_string()
}

fn default_severity() -> String {
    "minor".to_string()
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct UpdateIncident {
    pub title: Option<String>,
    pub status: Option<String>,
    pub severity: Option<String>,
    pub server_ids_json: Option<Option<Vec<String>>>,
    pub status_page_ids_json: Option<Option<Vec<String>>>,
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CreateIncidentUpdate {
    pub status: String,
    pub message: String,
}

impl IncidentService {
    pub async fn list(
        db: &DatabaseConnection,
        status_filter: Option<&str>,
    ) -> Result<Vec<incident::Model>, AppError> {
        let mut query = incident::Entity::find();
        if let Some(s) = status_filter {
            query = query.filter(incident::Column::Status.eq(s));
        }
        Ok(query
            .order_by_desc(incident::Column::CreatedAt)
            .all(db)
            .await?)
    }

    pub async fn get(db: &DatabaseConnection, id: &str) -> Result<incident::Model, AppError> {
        incident::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Incident {id} not found")))
    }

    pub async fn create(
        db: &DatabaseConnection,
        input: CreateIncident,
    ) -> Result<incident::Model, AppError> {
        if !VALID_STATUSES.contains(&input.status.as_str()) {
            return Err(AppError::Validation(format!(
                "status must be one of: {}",
                VALID_STATUSES.join(", ")
            )));
        }
        if !VALID_SEVERITIES.contains(&input.severity.as_str()) {
            return Err(AppError::Validation(format!(
                "severity must be one of: {}",
                VALID_SEVERITIES.join(", ")
            )));
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
        let resolved_at = if input.status == "resolved" {
            Some(now)
        } else {
            None
        };

        let model = incident::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            title: Set(input.title),
            status: Set(input.status),
            severity: Set(input.severity),
            server_ids_json: Set(server_ids_json),
            status_page_ids_json: Set(status_page_ids_json),
            created_at: Set(now),
            updated_at: Set(now),
            resolved_at: Set(resolved_at),
        };

        Ok(model.insert(db).await?)
    }

    pub async fn update(
        db: &DatabaseConnection,
        id: &str,
        input: UpdateIncident,
    ) -> Result<incident::Model, AppError> {
        let existing = Self::get(db, id).await?;
        let mut model: incident::ActiveModel = existing.clone().into();

        if let Some(title) = input.title {
            model.title = Set(title);
        }
        if let Some(status) = input.status {
            if !VALID_STATUSES.contains(&status.as_str()) {
                return Err(AppError::Validation(format!(
                    "status must be one of: {}",
                    VALID_STATUSES.join(", ")
                )));
            }
            if status == "resolved" && existing.resolved_at.is_none() {
                model.resolved_at = Set(Some(Utc::now()));
            }
            model.status = Set(status);
        }
        if let Some(severity) = input.severity {
            if !VALID_SEVERITIES.contains(&severity.as_str()) {
                return Err(AppError::Validation(format!(
                    "severity must be one of: {}",
                    VALID_SEVERITIES.join(", ")
                )));
            }
            model.severity = Set(severity);
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
        model.updated_at = Set(Utc::now());

        Ok(model.update(db).await?)
    }

    pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let result = incident::Entity::delete_by_id(id).exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!("Incident {id} not found")));
        }
        // Cascade delete updates
        incident_update::Entity::delete_many()
            .filter(incident_update::Column::IncidentId.eq(id))
            .exec(db)
            .await?;
        Ok(())
    }

    pub async fn add_update(
        db: &DatabaseConnection,
        incident_id: &str,
        input: CreateIncidentUpdate,
    ) -> Result<incident_update::Model, AppError> {
        // Verify incident exists
        let existing = Self::get(db, incident_id).await?;

        if !VALID_STATUSES.contains(&input.status.as_str()) {
            return Err(AppError::Validation(format!(
                "status must be one of: {}",
                VALID_STATUSES.join(", ")
            )));
        }

        let now = Utc::now();
        let update_model = incident_update::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            incident_id: Set(incident_id.to_string()),
            status: Set(input.status.clone()),
            message: Set(input.message),
            created_at: Set(now),
        };

        let result = update_model.insert(db).await?;

        // Also update the incident's status
        let mut incident_model: incident::ActiveModel = existing.into();
        if input.status == "resolved" {
            incident_model.resolved_at = Set(Some(now));
        }
        incident_model.status = Set(input.status);
        incident_model.updated_at = Set(now);
        incident_model.update(db).await?;

        Ok(result)
    }

    pub async fn list_updates(
        db: &DatabaseConnection,
        incident_id: &str,
    ) -> Result<Vec<incident_update::Model>, AppError> {
        Ok(incident_update::Entity::find()
            .filter(incident_update::Column::IncidentId.eq(incident_id))
            .order_by_asc(incident_update::Column::CreatedAt)
            .all(db)
            .await?)
    }
}
