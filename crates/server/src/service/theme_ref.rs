use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use serde::Serialize;

use crate::entity::{custom_theme, status_page};
use crate::error::AppError;
use crate::service::config::ConfigService;

const PRESET_IDS: &[&str] = &[
    "default",
    "tokyo-night",
    "nord",
    "catppuccin",
    "dracula",
    "one-dark",
    "solarized",
    "rose-pine",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeRef {
    Preset(String),
    Custom(i32),
}

impl ThemeRef {
    pub fn parse(s: &str) -> Result<Self, AppError> {
        if let Some(id) = s.strip_prefix("preset:") {
            if PRESET_IDS.contains(&id) {
                return Ok(Self::Preset(id.to_string()));
            }

            return Err(AppError::Validation(format!("unknown preset: {id}")));
        }

        if let Some(rest) = s.strip_prefix("custom:") {
            let id = parse_custom_id(rest)?;
            return Ok(Self::Custom(id));
        }

        Err(AppError::Validation(format!("malformed theme ref: {s}")))
    }

    pub fn to_urn(&self) -> String {
        match self {
            Self::Preset(id) => format!("preset:{id}"),
            Self::Custom(id) => format!("custom:{id}"),
        }
    }
}

fn parse_custom_id(rest: &str) -> Result<i32, AppError> {
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) || rest.starts_with('0') {
        return Err(AppError::Validation(format!("invalid custom id: {rest}")));
    }

    let id = rest
        .parse::<i32>()
        .map_err(|_| AppError::Validation(format!("invalid custom id: {rest}")))?;

    if id > 0 {
        Ok(id)
    } else {
        Err(AppError::Validation(format!("invalid custom id: {rest}")))
    }
}

pub async fn validate_theme_ref(db: &DatabaseConnection, r: &ThemeRef) -> Result<(), AppError> {
    match r {
        ThemeRef::Preset(id) => {
            if PRESET_IDS.contains(&id.as_str()) {
                Ok(())
            } else {
                Err(AppError::Validation(format!("unknown preset: {id}")))
            }
        }
        ThemeRef::Custom(id) => {
            if *id <= 0 {
                return Err(AppError::Validation(format!("invalid custom id: {id}")));
            }

            let exists = custom_theme::Entity::find_by_id(*id)
                .one(db)
                .await?
                .is_some();
            if exists {
                Ok(())
            } else {
                Err(AppError::Validation(format!("custom theme {id} not found")))
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ThemeReferences {
    pub admin: bool,
    pub status_pages: Vec<StatusPageRef>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct StatusPageRef {
    pub id: String,
    pub name: String,
}

pub async fn list_references(
    db: &DatabaseConnection,
    custom_id: i32,
) -> Result<ThemeReferences, AppError> {
    if custom_id <= 0 {
        return Err(AppError::Validation(format!(
            "invalid custom id: {custom_id}"
        )));
    }

    let urn = ThemeRef::Custom(custom_id).to_urn();
    let active_admin_theme = ConfigService::get(db, "active_admin_theme").await?;
    let status_pages = status_page::Entity::find()
        .filter(status_page::Column::ThemeRef.eq(urn.clone()))
        .order_by_asc(status_page::Column::Title)
        .order_by_asc(status_page::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(|m| StatusPageRef {
            id: m.id,
            name: m.title,
        })
        .collect();

    Ok(ThemeReferences {
        admin: active_admin_theme.as_deref() == Some(urn.as_str()),
        status_pages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, Set};

    use crate::entity::status_page;
    use crate::test_utils::setup_test_db;

    #[test]
    fn parses_preset() {
        assert_eq!(
            ThemeRef::parse("preset:tokyo-night").expect("preset should parse"),
            ThemeRef::Preset("tokyo-night".to_string())
        );
    }

    #[test]
    fn parses_custom() {
        assert_eq!(
            ThemeRef::parse("custom:42").expect("custom ref should parse"),
            ThemeRef::Custom(42)
        );
    }

    #[test]
    fn rejects_unknown_preset() {
        let err = ThemeRef::parse("preset:not-real").expect_err("unknown preset should fail");
        assert!(
            matches!(err, AppError::Validation(message) if message == "unknown preset: not-real")
        );
    }

    #[test]
    fn rejects_empty_preset() {
        let err = ThemeRef::parse("preset:").expect_err("empty preset should fail");
        assert!(matches!(err, AppError::Validation(message) if message == "unknown preset: "));
    }

    #[test]
    fn rejects_bad_custom_id() {
        let err = ThemeRef::parse("custom:nope").expect_err("bad custom id should fail");
        assert!(
            matches!(err, AppError::Validation(message) if message == "invalid custom id: nope")
        );
    }

    #[test]
    fn rejects_empty_custom_id() {
        let err = ThemeRef::parse("custom:").expect_err("empty custom id should fail");
        assert!(matches!(err, AppError::Validation(message) if message == "invalid custom id: "));
    }

    #[test]
    fn rejects_zero_custom_id() {
        let err = ThemeRef::parse("custom:0").expect_err("zero custom id should fail");
        assert!(matches!(err, AppError::Validation(message) if message == "invalid custom id: 0"));
    }

    #[test]
    fn rejects_negative_custom_id() {
        let err = ThemeRef::parse("custom:-1").expect_err("negative custom id should fail");
        assert!(matches!(err, AppError::Validation(message) if message == "invalid custom id: -1"));
    }

    #[test]
    fn rejects_plus_custom_id() {
        let err = ThemeRef::parse("custom:+1").expect_err("plus custom id should fail");
        assert!(matches!(err, AppError::Validation(message) if message == "invalid custom id: +1"));
    }

    #[test]
    fn rejects_leading_zero_custom_id() {
        let err = ThemeRef::parse("custom:001").expect_err("leading zero custom id should fail");
        assert!(
            matches!(err, AppError::Validation(message) if message == "invalid custom id: 001")
        );
    }

    #[test]
    fn rejects_overflow_custom_id() {
        let err = ThemeRef::parse("custom:2147483648").expect_err("overflow custom id should fail");
        assert!(
            matches!(err, AppError::Validation(message) if message == "invalid custom id: 2147483648")
        );
    }

    #[test]
    fn rejects_whitespace_wrapped_ref() {
        let err = ThemeRef::parse(" custom:1").expect_err("leading whitespace should fail");
        assert!(
            matches!(err, AppError::Validation(message) if message == "malformed theme ref:  custom:1")
        );

        let err = ThemeRef::parse("custom:1 ").expect_err("trailing whitespace should fail");
        assert!(matches!(err, AppError::Validation(message) if message == "invalid custom id: 1 "));
    }

    #[test]
    fn rejects_unknown_scheme() {
        let err = ThemeRef::parse("theme:default").expect_err("unknown scheme should fail");
        assert!(
            matches!(err, AppError::Validation(message) if message == "malformed theme ref: theme:default")
        );
    }

    #[test]
    fn round_trips_urn() {
        let refs = [ThemeRef::Preset("default".to_string()), ThemeRef::Custom(7)];

        for theme_ref in refs {
            assert_eq!(
                ThemeRef::parse(&theme_ref.to_urn()).expect("serialized ref should parse"),
                theme_ref
            );
        }
    }

    #[tokio::test]
    async fn validate_theme_ref_rejects_directly_constructed_unknown_preset() {
        let (db, _tmp) = setup_test_db().await;

        let err = validate_theme_ref(&db, &ThemeRef::Preset("nonsense".to_string()))
            .await
            .expect_err("unknown preset should fail validation");

        assert!(
            matches!(err, AppError::Validation(message) if message == "unknown preset: nonsense")
        );
    }

    #[tokio::test]
    async fn validate_theme_ref_rejects_directly_constructed_zero_custom_id() {
        let (db, _tmp) = setup_test_db().await;

        let err = validate_theme_ref(&db, &ThemeRef::Custom(0))
            .await
            .expect_err("zero custom id should fail validation");

        assert!(matches!(err, AppError::Validation(message) if message == "invalid custom id: 0"));
    }

    #[tokio::test]
    async fn validate_theme_ref_rejects_directly_constructed_negative_custom_id() {
        let (db, _tmp) = setup_test_db().await;

        let err = validate_theme_ref(&db, &ThemeRef::Custom(-1))
            .await
            .expect_err("negative custom id should fail validation");

        assert!(matches!(err, AppError::Validation(message) if message == "invalid custom id: -1"));
    }

    #[tokio::test]
    async fn list_references_rejects_zero_custom_id() {
        let (db, _tmp) = setup_test_db().await;

        let err = list_references(&db, 0)
            .await
            .expect_err("zero custom id should fail");

        assert!(matches!(err, AppError::Validation(message) if message == "invalid custom id: 0"));
    }

    #[tokio::test]
    async fn list_references_rejects_negative_custom_id() {
        let (db, _tmp) = setup_test_db().await;

        let err = list_references(&db, -1)
            .await
            .expect_err("negative custom id should fail");

        assert!(matches!(err, AppError::Validation(message) if message == "invalid custom id: -1"));
    }

    #[tokio::test]
    async fn list_references_orders_status_pages_by_title_then_id() {
        let (db, _tmp) = setup_test_db().await;

        insert_status_page(&db, "page-zulu", "Zulu", "custom:42").await;
        insert_status_page(&db, "page-alpha-b", "Alpha", "custom:42").await;
        insert_status_page(&db, "page-alpha-a", "Alpha", "custom:42").await;
        insert_status_page(&db, "page-other", "Beta", "custom:43").await;

        let refs = list_references(&db, 42)
            .await
            .expect("references should load");

        let ordered_ids: Vec<String> = refs.status_pages.into_iter().map(|r| r.id).collect();
        assert_eq!(
            ordered_ids,
            vec![
                "page-alpha-a".to_string(),
                "page-alpha-b".to_string(),
                "page-zulu".to_string(),
            ]
        );
    }

    async fn insert_status_page(
        db: &sea_orm::DatabaseConnection,
        id: &str,
        title: &str,
        theme_ref: &str,
    ) {
        let now = Utc::now();
        status_page::ActiveModel {
            id: Set(id.to_string()),
            title: Set(title.to_string()),
            slug: Set(id.to_string()),
            description: Set(None),
            server_ids_json: Set("[]".to_string()),
            group_by_server_group: Set(false),
            show_values: Set(false),
            custom_css: Set(None),
            enabled: Set(true),
            uptime_yellow_threshold: Set(99.0),
            uptime_red_threshold: Set(95.0),
            theme_ref: Set(Some(theme_ref.to_string())),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .expect("status page insert should succeed");
    }
}
