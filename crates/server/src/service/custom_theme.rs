use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QueryOrder, Set, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::entity::{config, custom_theme, status_page};
use crate::error::AppError;
use crate::service::config::ConfigService;
use crate::service::theme_ref::{self, ThemeRef};
use crate::service::theme_validator::{self, VarMap};

const ACTIVE_THEME_KEY: &str = "active_admin_theme";
const DEFAULT_REF: &str = "preset:default";
const MAX_THEME_NAME_LEN: usize = 120;

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
        let name = normalize_theme_name(input.name)?;
        let based_on = normalize_based_on(input.based_on)?;
        theme_validator::validate_var_map(&input.vars_light)?;
        theme_validator::validate_var_map(&input.vars_dark)?;

        let vars_light = encode_vars(&input.vars_light)?;
        let vars_dark = encode_vars(&input.vars_dark)?;
        let now = Utc::now();
        let model = custom_theme::ActiveModel {
            id: sea_orm::NotSet,
            name: Set(name),
            description: Set(input.description),
            based_on: Set(based_on),
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
        let name = normalize_theme_name(input.name)?;
        let based_on = normalize_based_on(input.based_on)?;
        theme_validator::validate_var_map(&input.vars_light)?;
        theme_validator::validate_var_map(&input.vars_dark)?;

        let model = custom_theme::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("theme {id}")))?;

        let mut active: custom_theme::ActiveModel = model.into();
        active.name = Set(name);
        active.description = Set(input.description);
        active.based_on = Set(based_on);
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
                name: duplicate_theme_name(&source.name),
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
        let txn = db.begin().await?;
        let references = list_references_in_connection(&txn, id).await?;
        if references.admin || !references.status_pages.is_empty() {
            return Err(AppError::Conflict(
                "Theme is in use by admin or one or more status pages; unbind it first.".into(),
            ));
        }

        let result = custom_theme::Entity::delete_by_id(id)
            .exec(&txn)
            .await
            .map_err(map_theme_ref_integrity_error)?;
        if result.rows_affected == 0 {
            return Err(AppError::NotFound(format!("theme {id}")));
        }

        txn.commit().await?;
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

        match Self::resolve(db, theme_ref.clone()).await {
            Ok(response) => Ok(response),
            Err(AppError::NotFound(message)) if matches!(theme_ref, ThemeRef::Custom(_)) => {
                warn!(
                    "Stored active custom theme ref is dangling, falling back to default: {message}"
                );
                Self::resolve(db, default_theme_ref()).await
            }
            Err(err) => Err(err),
        }
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
        set_config_value(db, ACTIVE_THEME_KEY, &canonical).await?;

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

async fn set_config_value<C>(db: &C, key: &str, value: &str) -> Result<(), AppError>
where
    C: ConnectionTrait,
{
    let existing = config::Entity::find_by_id(key)
        .one(db)
        .await
        .map_err(map_theme_ref_integrity_error)?;

    match existing {
        Some(model) => {
            let mut active: config::ActiveModel = model.into();
            active.value = Set(value.to_string());
            active
                .update(db)
                .await
                .map_err(map_theme_ref_integrity_error)?;
        }
        None => {
            let new_config = config::ActiveModel {
                key: Set(key.to_string()),
                value: Set(value.to_string()),
            };
            new_config
                .insert(db)
                .await
                .map_err(map_theme_ref_integrity_error)?;
        }
    }

    Ok(())
}

fn map_theme_ref_integrity_error(err: DbErr) -> AppError {
    let message = err.to_string();
    if message.contains("custom_theme_ref_in_use") {
        AppError::Conflict(
            "Theme is in use by admin or one or more status pages; unbind it first.".into(),
        )
    } else if message.contains("custom_theme_ref_missing") {
        AppError::Validation("custom theme reference does not exist".into())
    } else {
        AppError::from(err)
    }
}

fn normalize_theme_name(name: String) -> Result<String, AppError> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation("theme name cannot be empty".into()));
    }
    if name.chars().count() > MAX_THEME_NAME_LEN {
        return Err(AppError::Validation(format!(
            "theme name cannot exceed {MAX_THEME_NAME_LEN} characters"
        )));
    }

    Ok(name)
}

fn normalize_based_on(based_on: Option<String>) -> Result<Option<String>, AppError> {
    let Some(id) = based_on
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
    else {
        return Ok(None);
    };

    if theme_ref::is_preset_id(&id) {
        Ok(Some(id))
    } else {
        Err(AppError::Validation(format!("unknown preset: {id}")))
    }
}

fn duplicate_theme_name(name: &str) -> String {
    const COPY_SUFFIX: &str = " (copy)";
    let max_base_len = MAX_THEME_NAME_LEN.saturating_sub(COPY_SUFFIX.chars().count());
    let base = name.chars().take(max_base_len).collect::<String>();
    format!("{base}{COPY_SUFFIX}")
}

async fn list_references_in_connection<C>(
    db: &C,
    custom_id: i32,
) -> Result<theme_ref::ThemeReferences, AppError>
where
    C: ConnectionTrait,
{
    if custom_id <= 0 {
        return Err(AppError::Validation(format!(
            "invalid custom id: {custom_id}"
        )));
    }

    let urn = ThemeRef::Custom(custom_id).to_urn();
    let active_admin_theme = config::Entity::find_by_id(ACTIVE_THEME_KEY)
        .one(db)
        .await?
        .map(|m| m.value);
    let status_pages = status_page::Entity::find()
        .filter(status_page::Column::ThemeRef.eq(urn.clone()))
        .order_by_asc(status_page::Column::Title)
        .order_by_asc(status_page::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(|m| theme_ref::StatusPageRef {
            id: m.id,
            name: m.title,
        })
        .collect();

    Ok(theme_ref::ThemeReferences {
        admin: active_admin_theme.as_deref() == Some(urn.as_str()),
        status_pages,
    })
}

fn default_theme_ref() -> ThemeRef {
    ThemeRef::Preset(DEFAULT_REF.trim_start_matches("preset:").to_string())
}

#[cfg(test)]
mod tests {
    use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};

    use crate::service::theme_validator::REQUIRED_VARS;
    use crate::test_utils::setup_test_db;

    use super::*;

    fn valid_vars() -> VarMap {
        REQUIRED_VARS
            .iter()
            .map(|key| ((*key).to_string(), "oklch(0.5 0.1 180)".to_string()))
            .collect()
    }

    fn create_input(name: &str) -> CreateThemeInput {
        CreateThemeInput {
            name: name.to_string(),
            description: None,
            based_on: Some("default".to_string()),
            vars_light: valid_vars(),
            vars_dark: valid_vars(),
        }
    }

    fn validation_message(result: Result<Theme, AppError>) -> String {
        match result {
            Err(AppError::Validation(message)) => message,
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    fn update_input(name: &str) -> UpdateThemeInput {
        UpdateThemeInput {
            name: name.to_string(),
            description: None,
            based_on: Some("default".to_string()),
            vars_light: valid_vars(),
            vars_dark: valid_vars(),
        }
    }

    async fn insert_status_page_with_theme_ref(
        db: &DatabaseConnection,
        id: &str,
        theme_ref: Option<String>,
    ) -> Result<status_page::Model, DbErr> {
        let now = Utc::now();
        status_page::ActiveModel {
            id: Set(id.to_string()),
            title: Set("Status".to_string()),
            slug: Set(id.to_string()),
            description: Set(None),
            server_ids_json: Set("[]".to_string()),
            group_by_server_group: Set(true),
            show_values: Set(true),
            custom_css: Set(None),
            enabled: Set(true),
            uptime_yellow_threshold: Set(100.0),
            uptime_red_threshold: Set(95.0),
            theme_ref: Set(theme_ref),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
    }

    async fn update_status_page_theme_ref(
        db: &DatabaseConnection,
        id: &str,
        theme_ref: Option<String>,
    ) -> Result<status_page::Model, DbErr> {
        let page = status_page::Entity::find_by_id(id)
            .one(db)
            .await?
            .expect("status page should exist");
        let mut active: status_page::ActiveModel = page.into();
        active.theme_ref = Set(theme_ref);
        active.update(db).await
    }

    fn assert_db_err_contains(result: Result<impl std::fmt::Debug, DbErr>, needle: &str) {
        let err = result.expect_err("operation should fail");
        assert!(
            err.to_string().contains(needle),
            "expected DB error to contain {needle}, got {err}"
        );
    }

    #[tokio::test]
    async fn active_theme_falls_back_to_default_for_dangling_custom_ref() {
        let (db, _tmp) = setup_test_db().await;
        db.execute_unprepared("DROP TRIGGER IF EXISTS trg_custom_theme_config_insert_ref_exists")
            .await
            .expect("config insert trigger should be dropped");
        db.execute_unprepared("DROP TRIGGER IF EXISTS trg_custom_theme_config_update_ref_exists")
            .await
            .expect("config update trigger should be dropped");
        ConfigService::set(&db, ACTIVE_THEME_KEY, "custom:999")
            .await
            .expect("config should be set");

        let response = CustomThemeService::active_theme(&db, true)
            .await
            .expect("dangling stored custom ref should fall back");

        assert_eq!(response.r#ref, DEFAULT_REF);
        assert!(matches!(
            response.theme,
            ThemeResolved::Preset { ref id } if id == "default"
        ));
    }

    #[tokio::test]
    async fn active_theme_falls_back_to_default_when_config_missing() {
        let (db, _tmp) = setup_test_db().await;

        let response = CustomThemeService::active_theme(&db, true)
            .await
            .expect("missing config should fall back");

        assert_eq!(response.r#ref, DEFAULT_REF);
        assert!(matches!(
            response.theme,
            ThemeResolved::Preset { ref id } if id == "default"
        ));
    }

    #[tokio::test]
    async fn active_theme_falls_back_to_default_for_malformed_stored_ref() {
        let (db, _tmp) = setup_test_db().await;
        ConfigService::set(&db, ACTIVE_THEME_KEY, "not-a-theme-ref")
            .await
            .expect("malformed legacy config should be stored");

        let response = CustomThemeService::active_theme(&db, true)
            .await
            .expect("malformed stored ref should fall back");

        assert_eq!(response.r#ref, DEFAULT_REF);
        assert!(matches!(
            response.theme,
            ThemeResolved::Preset { ref id } if id == "default"
        ));
    }

    #[tokio::test]
    async fn update_rejects_blank_name() {
        let (db, _tmp) = setup_test_db().await;
        let theme = CustomThemeService::create(&db, create_input("Ocean"), "user-1")
            .await
            .expect("valid theme should be created");

        let message =
            validation_message(CustomThemeService::update(&db, theme.id, update_input(" ")).await);

        assert_eq!(message, "theme name cannot be empty");
    }

    #[tokio::test]
    async fn update_rejects_overlong_name() {
        let (db, _tmp) = setup_test_db().await;
        let theme = CustomThemeService::create(&db, create_input("Ocean"), "user-1")
            .await
            .expect("valid theme should be created");

        let message = validation_message(
            CustomThemeService::update(&db, theme.id, update_input(&"x".repeat(121))).await,
        );

        assert_eq!(message, "theme name cannot exceed 120 characters");
    }

    #[tokio::test]
    async fn create_rejects_whitespace_only_name() {
        let (db, _tmp) = setup_test_db().await;

        let message = validation_message(
            CustomThemeService::create(&db, create_input("   "), "user-1").await,
        );

        assert_eq!(message, "theme name cannot be empty");
    }

    #[tokio::test]
    async fn create_trims_valid_name_before_storing() {
        let (db, _tmp) = setup_test_db().await;

        let theme = CustomThemeService::create(&db, create_input("  Ocean  "), "user-1")
            .await
            .expect("valid theme should be created");

        assert_eq!(theme.name, "Ocean");
    }

    #[tokio::test]
    async fn create_rejects_unknown_based_on() {
        let (db, _tmp) = setup_test_db().await;
        let mut input = create_input("Ocean");
        input.based_on = Some("not-real".to_string());

        let message = validation_message(CustomThemeService::create(&db, input, "user-1").await);

        assert_eq!(message, "unknown preset: not-real");
    }

    #[tokio::test]
    async fn create_treats_blank_based_on_as_none() {
        let (db, _tmp) = setup_test_db().await;
        let mut input = create_input("Ocean");
        input.based_on = Some("   ".to_string());

        let theme = CustomThemeService::create(&db, input, "user-1")
            .await
            .expect("blank based_on should be normalized");

        assert_eq!(theme.based_on, None);
    }

    #[tokio::test]
    async fn duplicate_truncates_max_length_name_before_copy_suffix() {
        let (db, _tmp) = setup_test_db().await;
        let source = CustomThemeService::create(&db, create_input(&"x".repeat(120)), "user-1")
            .await
            .expect("max length theme should be created");

        let copy = CustomThemeService::duplicate(&db, source.id, "user-1")
            .await
            .expect("duplicate should fit max name length");

        assert_eq!(copy.name.chars().count(), 120);
        assert!(copy.name.ends_with(" (copy)"));
    }

    #[tokio::test]
    async fn delete_rejects_in_use_active_theme() {
        let (db, _tmp) = setup_test_db().await;
        let theme = CustomThemeService::create(&db, create_input("Ocean"), "user-1")
            .await
            .expect("valid theme should be created");
        ConfigService::set(&db, ACTIVE_THEME_KEY, &format!("custom:{}", theme.id))
            .await
            .expect("config should be set");

        let err = CustomThemeService::delete(&db, theme.id)
            .await
            .expect_err("active theme should not be deleted");

        assert!(
            matches!(err, AppError::Conflict(message) if message == "Theme is in use by admin or one or more status pages; unbind it first.")
        );
    }

    #[tokio::test]
    async fn delete_rejects_in_use_status_page_theme() {
        let (db, _tmp) = setup_test_db().await;
        let theme = CustomThemeService::create(&db, create_input("Ocean"), "user-1")
            .await
            .expect("valid theme should be created");
        insert_status_page_with_theme_ref(
            &db,
            "status-page-1",
            Some(format!("custom:{}", theme.id)),
        )
        .await
        .expect("status page should be inserted");

        let err = CustomThemeService::delete(&db, theme.id)
            .await
            .expect_err("referenced theme should not be deleted");

        assert!(
            matches!(err, AppError::Conflict(message) if message == "Theme is in use by admin or one or more status pages; unbind it first.")
        );
    }

    #[tokio::test]
    async fn delete_trigger_blocks_active_theme_ref_when_service_precheck_is_bypassed() {
        let (db, _tmp) = setup_test_db().await;
        let theme = CustomThemeService::create(&db, create_input("Ocean"), "user-1")
            .await
            .expect("valid theme should be created");
        ConfigService::set(&db, ACTIVE_THEME_KEY, &format!("custom:{}", theme.id))
            .await
            .expect("config should be set");

        let result = custom_theme::Entity::delete_by_id(theme.id).exec(&db).await;

        assert_db_err_contains(result, "custom_theme_ref_in_use");
    }

    #[tokio::test]
    async fn delete_trigger_blocks_status_page_ref_when_service_precheck_is_bypassed() {
        let (db, _tmp) = setup_test_db().await;
        let theme = CustomThemeService::create(&db, create_input("Ocean"), "user-1")
            .await
            .expect("valid theme should be created");
        insert_status_page_with_theme_ref(
            &db,
            "status-page-1",
            Some(format!("custom:{}", theme.id)),
        )
        .await
        .expect("status page should be inserted");

        let result = custom_theme::Entity::delete_by_id(theme.id).exec(&db).await;

        assert_db_err_contains(result, "custom_theme_ref_in_use");
    }

    #[tokio::test]
    async fn config_trigger_blocks_dangling_active_custom_ref() {
        let (db, _tmp) = setup_test_db().await;

        let err = ConfigService::set(&db, ACTIVE_THEME_KEY, "custom:999")
            .await
            .expect_err("dangling active theme ref should be blocked");

        assert!(matches!(err, AppError::Internal(_)));
    }

    #[tokio::test]
    async fn status_page_insert_trigger_blocks_dangling_custom_ref() {
        let (db, _tmp) = setup_test_db().await;

        let result =
            insert_status_page_with_theme_ref(&db, "status-page-1", Some("custom:999".to_string()))
                .await;

        assert_db_err_contains(result, "custom_theme_ref_missing");
    }

    #[tokio::test]
    async fn status_page_update_trigger_blocks_dangling_custom_ref() {
        let (db, _tmp) = setup_test_db().await;
        insert_status_page_with_theme_ref(&db, "status-page-1", None)
            .await
            .expect("status page should be inserted");

        let result =
            update_status_page_theme_ref(&db, "status-page-1", Some("custom:999".to_string()))
                .await;

        assert_db_err_contains(result, "custom_theme_ref_missing");
    }

    #[tokio::test]
    async fn local_config_write_maps_missing_theme_ref_trigger() {
        let (db, _tmp) = setup_test_db().await;

        let err = set_config_value(&db, ACTIVE_THEME_KEY, "custom:999")
            .await
            .expect_err("dangling active theme ref should be mapped");

        assert!(
            matches!(err, AppError::Validation(message) if message == "custom theme reference does not exist")
        );
    }
}
