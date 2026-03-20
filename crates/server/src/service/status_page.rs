use chrono::Utc;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::status_page;
use crate::error::AppError;

pub struct StatusPageService;

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct CreateStatusPage {
    pub title: String,
    pub slug: String,
    pub description: Option<String>,
    pub server_ids_json: Vec<String>,
    pub group_by_server_group: Option<bool>,
    pub show_values: Option<bool>,
    pub custom_css: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct UpdateStatusPage {
    pub title: Option<String>,
    pub slug: Option<String>,
    pub description: Option<Option<String>>,
    pub server_ids_json: Option<Vec<String>>,
    pub group_by_server_group: Option<bool>,
    pub show_values: Option<bool>,
    pub custom_css: Option<Option<String>>,
    pub enabled: Option<bool>,
}

impl StatusPageService {
    pub async fn list(db: &DatabaseConnection) -> Result<Vec<status_page::Model>, AppError> {
        Ok(status_page::Entity::find()
            .order_by_desc(status_page::Column::CreatedAt)
            .all(db)
            .await?)
    }

    pub async fn get(db: &DatabaseConnection, id: &str) -> Result<status_page::Model, AppError> {
        status_page::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Status page {id} not found")))
    }

    pub async fn get_by_slug(
        db: &DatabaseConnection,
        slug: &str,
    ) -> Result<status_page::Model, AppError> {
        status_page::Entity::find()
            .filter(status_page::Column::Slug.eq(slug))
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Status page with slug '{slug}' not found")))
    }

    pub async fn create(
        db: &DatabaseConnection,
        input: CreateStatusPage,
    ) -> Result<status_page::Model, AppError> {
        // Check slug uniqueness
        let existing = status_page::Entity::find()
            .filter(status_page::Column::Slug.eq(&input.slug))
            .one(db)
            .await?;
        if existing.is_some() {
            return Err(AppError::Conflict(format!(
                "Status page with slug '{}' already exists",
                input.slug
            )));
        }

        let server_ids_str = serde_json::to_string(&input.server_ids_json)
            .map_err(|e| AppError::Validation(format!("Invalid server_ids_json: {e}")))?;

        let now = Utc::now();
        let model = status_page::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            title: Set(input.title),
            slug: Set(input.slug),
            description: Set(input.description),
            server_ids_json: Set(server_ids_str),
            group_by_server_group: Set(input.group_by_server_group.unwrap_or(true)),
            show_values: Set(input.show_values.unwrap_or(true)),
            custom_css: Set(input.custom_css),
            enabled: Set(input.enabled.unwrap_or(true)),
            created_at: Set(now),
            updated_at: Set(now),
        };

        Ok(model.insert(db).await?)
    }

    pub async fn update(
        db: &DatabaseConnection,
        id: &str,
        input: UpdateStatusPage,
    ) -> Result<status_page::Model, AppError> {
        let existing = Self::get(db, id).await?;
        let mut model: status_page::ActiveModel = existing.into();

        if let Some(title) = input.title {
            model.title = Set(title);
        }
        if let Some(slug) = input.slug {
            // Check slug uniqueness (excluding self)
            let conflict = status_page::Entity::find()
                .filter(status_page::Column::Slug.eq(&slug))
                .filter(status_page::Column::Id.ne(id))
                .one(db)
                .await?;
            if conflict.is_some() {
                return Err(AppError::Conflict(format!(
                    "Status page with slug '{slug}' already exists"
                )));
            }
            model.slug = Set(slug);
        }
        if let Some(description) = input.description {
            model.description = Set(description);
        }
        if let Some(server_ids) = input.server_ids_json {
            let json = serde_json::to_string(&server_ids)
                .map_err(|e| AppError::Validation(format!("Invalid server_ids_json: {e}")))?;
            model.server_ids_json = Set(json);
        }
        if let Some(group_by) = input.group_by_server_group {
            model.group_by_server_group = Set(group_by);
        }
        if let Some(show_values) = input.show_values {
            model.show_values = Set(show_values);
        }
        if let Some(custom_css) = input.custom_css {
            model.custom_css = Set(custom_css);
        }
        if let Some(enabled) = input.enabled {
            model.enabled = Set(enabled);
        }
        model.updated_at = Set(Utc::now());

        Ok(model.update(db).await?)
    }

    pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let result = status_page::Entity::delete_by_id(id).exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!("Status page {id} not found")));
        }
        Ok(())
    }
}
