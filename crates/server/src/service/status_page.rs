use chrono::Utc;
use sea_orm::*;
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use crate::entity::status_page;
use crate::error::AppError;
use crate::service::theme_ref::{self, ThemeRef};

fn deserialize_optional_nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

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
    pub uptime_yellow_threshold: Option<f64>,
    pub uptime_red_threshold: Option<f64>,
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
    pub uptime_yellow_threshold: Option<f64>,
    pub uptime_red_threshold: Option<f64>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub theme_ref: Option<Option<String>>,
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
            uptime_yellow_threshold: Set(input.uptime_yellow_threshold.unwrap_or(100.0)),
            uptime_red_threshold: Set(input.uptime_red_threshold.unwrap_or(95.0)),
            theme_ref: Set(None),
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
        if let Some(yellow) = input.uptime_yellow_threshold {
            model.uptime_yellow_threshold = Set(yellow);
        }
        if let Some(red) = input.uptime_red_threshold {
            model.uptime_red_threshold = Set(red);
        }
        if let Some(theme_ref) = input.theme_ref {
            let canonical = match theme_ref {
                Some(value) => {
                    let parsed = ThemeRef::parse(&value)?;
                    theme_ref::validate_theme_ref(db, &parsed).await?;
                    Some(parsed.to_urn())
                }
                None => None,
            };
            model.theme_ref = Set(canonical);
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::service::custom_theme::{CreateThemeInput, CustomThemeService};
    use crate::service::theme_validator::{REQUIRED_VARS, VarMap};
    use crate::test_utils::setup_test_db;

    use super::*;

    fn valid_vars() -> VarMap {
        REQUIRED_VARS
            .iter()
            .map(|key| ((*key).to_string(), "oklch(0.5 0.1 180)".to_string()))
            .collect()
    }

    fn create_page_input(slug: &str) -> CreateStatusPage {
        CreateStatusPage {
            title: "Status".to_string(),
            slug: slug.to_string(),
            description: None,
            server_ids_json: Vec::new(),
            group_by_server_group: None,
            show_values: None,
            custom_css: None,
            enabled: None,
            uptime_yellow_threshold: None,
            uptime_red_threshold: None,
        }
    }

    fn empty_update() -> UpdateStatusPage {
        UpdateStatusPage {
            title: None,
            slug: None,
            description: None,
            server_ids_json: None,
            group_by_server_group: None,
            show_values: None,
            custom_css: None,
            enabled: None,
            uptime_yellow_threshold: None,
            uptime_red_threshold: None,
            theme_ref: None,
        }
    }

    async fn create_theme(db: &DatabaseConnection, name: &str) -> i32 {
        CustomThemeService::create(
            db,
            CreateThemeInput {
                name: name.to_string(),
                description: None,
                based_on: Some("default".to_string()),
                vars_light: valid_vars(),
                vars_dark: valid_vars(),
            },
            "status-page-test",
        )
        .await
        .expect("theme should be created")
        .id
    }

    #[test]
    fn update_deserializes_theme_ref_absent_null_and_value_distinctly() {
        let absent: UpdateStatusPage =
            serde_json::from_value(json!({})).expect("empty update should deserialize");
        let null: UpdateStatusPage = serde_json::from_value(json!({ "theme_ref": null }))
            .expect("null theme_ref should deserialize");
        let value: UpdateStatusPage = serde_json::from_value(json!({ "theme_ref": "custom:42" }))
            .expect("value theme_ref should deserialize");

        assert_eq!(absent.theme_ref, None);
        assert_eq!(null.theme_ref, Some(None));
        assert_eq!(value.theme_ref, Some(Some("custom:42".to_string())));
    }

    #[tokio::test]
    async fn update_sets_canonical_custom_theme_ref_and_clears_it() {
        let (db, _tmp) = setup_test_db().await;
        let page = StatusPageService::create(&db, create_page_input("service-theme"))
            .await
            .expect("status page should be created");
        let theme_id = create_theme(&db, "Ocean").await;

        let mut set_ref = empty_update();
        set_ref.theme_ref = Some(Some(format!("custom:{theme_id}")));
        let updated = StatusPageService::update(&db, &page.id, set_ref)
            .await
            .expect("custom theme ref should update");

        assert_eq!(updated.theme_ref, Some(format!("custom:{theme_id}")));

        let mut clear_ref = empty_update();
        clear_ref.theme_ref = Some(None);
        let updated = StatusPageService::update(&db, &page.id, clear_ref)
            .await
            .expect("custom theme ref should clear");

        assert_eq!(updated.theme_ref, None);
    }

    #[tokio::test]
    async fn update_rejects_missing_custom_theme_ref() {
        let (db, _tmp) = setup_test_db().await;
        let page = StatusPageService::create(&db, create_page_input("missing-theme"))
            .await
            .expect("status page should be created");

        let mut input = empty_update();
        input.theme_ref = Some(Some("custom:999".to_string()));
        let err = StatusPageService::update(&db, &page.id, input)
            .await
            .expect_err("missing custom theme should be rejected");

        assert!(
            matches!(err, AppError::Validation(message) if message == "custom theme 999 not found")
        );
    }
}
