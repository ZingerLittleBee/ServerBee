use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
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
            let id = rest
                .parse::<i32>()
                .map_err(|_| AppError::Validation(format!("invalid custom id: {rest}")))?;
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

pub async fn validate_theme_ref(db: &DatabaseConnection, r: &ThemeRef) -> Result<(), AppError> {
    match r {
        ThemeRef::Preset(_) => Ok(()),
        ThemeRef::Custom(id) => {
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
    let urn = ThemeRef::Custom(custom_id).to_urn();
    let active_admin_theme = ConfigService::get(db, "active_admin_theme").await?;
    let status_pages = status_page::Entity::find()
        .filter(status_page::Column::ThemeRef.eq(urn.clone()))
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
    fn rejects_bad_custom_id() {
        let err = ThemeRef::parse("custom:nope").expect_err("bad custom id should fail");
        assert!(
            matches!(err, AppError::Validation(message) if message == "invalid custom id: nope")
        );
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
}
