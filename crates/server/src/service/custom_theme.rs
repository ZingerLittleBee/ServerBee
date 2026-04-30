use chrono::{DateTime, Utc};
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, QueryOrder, Set};
use serde::{Deserialize, Serialize};

use crate::entity::custom_theme;
use crate::error::AppError;
use crate::service::config::ConfigService;
use crate::service::theme_ref::{self, ThemeRef};
use crate::service::theme_validator::{self, VarMap};

const ACTIVE_THEME_KEY: &str = "active_admin_theme";
const DEFAULT_REF: &str = "preset:default";

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ThemeSummary {
    pub id: i32,
    pub name: String,
    pub based_on: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Theme {
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
    pub based_on: Option<String>,
    pub vars_light: VarMap,
    pub vars_dark: VarMap,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateThemeInput {
    pub name: String,
    pub description: Option<String>,
    pub based_on: Option<String>,
    pub vars_light: VarMap,
    pub vars_dark: VarMap,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateThemeInput {
    pub name: String,
    pub description: Option<String>,
    pub based_on: Option<String>,
    pub vars_light: VarMap,
    pub vars_dark: VarMap,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThemeResolved {
    Preset {
        id: String,
    },
    Custom {
        id: i32,
        name: String,
        vars_light: VarMap,
        vars_dark: VarMap,
        updated_at: DateTime<Utc>,
    },
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ActiveThemeResponse {
    pub r#ref: String,
    pub theme: ThemeResolved,
}

pub struct CustomThemeService;

impl CustomThemeService {
    pub async fn list(db: &DatabaseConnection) -> Result<Vec<ThemeSummary>, AppError> {
        let themes = custom_theme::Entity::find()
            .order_by_desc(custom_theme::Column::UpdatedAt)
            .all(db)
            .await?;

        Ok(themes
            .into_iter()
            .map(|m| ThemeSummary {
                id: m.id,
                name: m.name,
                based_on: m.based_on,
                updated_at: m.updated_at,
            })
            .collect())
    }

    pub async fn get(db: &DatabaseConnection, id: i32) -> Result<Theme, AppError> {
        let model = custom_theme::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("theme {id}")))?;

        model_to_theme(&model)
    }

    pub async fn create(
        db: &DatabaseConnection,
        input: CreateThemeInput,
        user_id: &str,
    ) -> Result<Theme, AppError> {
        theme_validator::validate_var_map(&input.vars_light)?;
        theme_validator::validate_var_map(&input.vars_dark)?;

        let vars_light = encode_vars(&input.vars_light)?;
        let vars_dark = encode_vars(&input.vars_dark)?;
        let now = Utc::now();
        let model = custom_theme::ActiveModel {
            id: sea_orm::NotSet,
            name: Set(input.name),
            description: Set(input.description),
            based_on: Set(input.based_on),
            vars_light: Set(vars_light),
            vars_dark: Set(vars_dark),
            created_by: Set(user_id.to_string()),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await?;

        model_to_theme(&model)
    }

    pub async fn update(
        db: &DatabaseConnection,
        id: i32,
        input: UpdateThemeInput,
    ) -> Result<Theme, AppError> {
        theme_validator::validate_var_map(&input.vars_light)?;
        theme_validator::validate_var_map(&input.vars_dark)?;

        let model = custom_theme::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("theme {id}")))?;

        let mut active: custom_theme::ActiveModel = model.into();
        active.name = Set(input.name);
        active.description = Set(input.description);
        active.based_on = Set(input.based_on);
        active.vars_light = Set(encode_vars(&input.vars_light)?);
        active.vars_dark = Set(encode_vars(&input.vars_dark)?);
        active.updated_at = Set(Utc::now());

        let updated = active.update(db).await?;
        model_to_theme(&updated)
    }

    pub async fn duplicate(
        db: &DatabaseConnection,
        id: i32,
        user_id: &str,
    ) -> Result<Theme, AppError> {
        let source = Self::get(db, id).await?;
        Self::create(
            db,
            CreateThemeInput {
                name: format!("{} (copy)", source.name),
                description: source.description,
                based_on: source.based_on,
                vars_light: source.vars_light,
                vars_dark: source.vars_dark,
            },
            user_id,
        )
        .await
    }

    pub async fn delete(db: &DatabaseConnection, id: i32) -> Result<(), AppError> {
        let references = theme_ref::list_references(db, id).await?;
        if references.admin || !references.status_pages.is_empty() {
            return Err(AppError::Conflict(
                "Theme is in use by admin or one or more status pages; unbind it first.".into(),
            ));
        }

        let result = custom_theme::Entity::delete_by_id(id).exec(db).await?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!("theme {id}")));
        }

        Ok(())
    }

    pub async fn active_theme(
        db: &DatabaseConnection,
        feature_enabled: bool,
    ) -> Result<ActiveThemeResponse, AppError> {
        let stored = ConfigService::get(db, ACTIVE_THEME_KEY).await?;
        let parsed = stored
            .as_deref()
            .and_then(|value| ThemeRef::parse(value).ok())
            .unwrap_or_else(default_theme_ref);
        let theme_ref = if feature_enabled {
            parsed
        } else {
            match parsed {
                ThemeRef::Preset(_) => parsed,
                ThemeRef::Custom(_) => default_theme_ref(),
            }
        };

        Self::resolve(db, theme_ref).await
    }

    pub async fn set_active_theme(
        db: &DatabaseConnection,
        urn: &str,
        feature_enabled: bool,
    ) -> Result<ActiveThemeResponse, AppError> {
        let parsed = ThemeRef::parse(urn)?;
        if !feature_enabled && matches!(parsed, ThemeRef::Custom(_)) {
            return Err(AppError::Validation(
                "custom theme feature disabled".to_string(),
            ));
        }

        theme_ref::validate_theme_ref(db, &parsed).await?;
        let canonical = parsed.to_urn();
        ConfigService::set(db, ACTIVE_THEME_KEY, &canonical).await?;

        Self::resolve(db, parsed).await
    }

    pub async fn resolve(
        db: &DatabaseConnection,
        r: ThemeRef,
    ) -> Result<ActiveThemeResponse, AppError> {
        match r {
            ThemeRef::Preset(id) => Ok(ActiveThemeResponse {
                r#ref: format!("preset:{id}"),
                theme: ThemeResolved::Preset { id },
            }),
            ThemeRef::Custom(id) => {
                let theme = Self::get(db, id).await?;
                Ok(ActiveThemeResponse {
                    r#ref: format!("custom:{id}"),
                    theme: ThemeResolved::Custom {
                        id: theme.id,
                        name: theme.name,
                        vars_light: theme.vars_light,
                        vars_dark: theme.vars_dark,
                        updated_at: theme.updated_at,
                    },
                })
            }
        }
    }
}

fn model_to_theme(m: &custom_theme::Model) -> Result<Theme, AppError> {
    Ok(Theme {
        id: m.id,
        name: m.name.clone(),
        description: m.description.clone(),
        based_on: m.based_on.clone(),
        vars_light: decode_vars(&m.vars_light)?,
        vars_dark: decode_vars(&m.vars_dark)?,
        created_at: m.created_at,
        updated_at: m.updated_at,
    })
}

fn encode_vars(vars: &VarMap) -> Result<String, AppError> {
    serde_json::to_string(vars)
        .map_err(|e| AppError::Internal(format!("theme JSON encode error: {e}")))
}

fn decode_vars(raw: &str) -> Result<VarMap, AppError> {
    serde_json::from_str(raw)
        .map_err(|e| AppError::Internal(format!("theme JSON decode error: {e}")))
}

fn default_theme_ref() -> ThemeRef {
    ThemeRef::Preset(DEFAULT_REF.trim_start_matches("preset:").to_string())
}
