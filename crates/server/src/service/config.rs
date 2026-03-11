use sea_orm::*;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::entity::config;
use crate::error::AppError;

pub struct ConfigService;

impl ConfigService {
    /// Get a config value by key.
    pub async fn get(db: &DatabaseConnection, key: &str) -> Result<Option<String>, AppError> {
        let result = config::Entity::find_by_id(key).one(db).await?;
        Ok(result.map(|m| m.value))
    }

    /// Set a config value by key (upsert).
    pub async fn set(db: &DatabaseConnection, key: &str, value: &str) -> Result<(), AppError> {
        let existing = config::Entity::find_by_id(key).one(db).await?;

        match existing {
            Some(model) => {
                let mut active: config::ActiveModel = model.into();
                active.value = Set(value.to_string());
                active.update(db).await?;
            }
            None => {
                let new_config = config::ActiveModel {
                    key: Set(key.to_string()),
                    value: Set(value.to_string()),
                };
                new_config.insert(db).await?;
            }
        }

        Ok(())
    }

    /// Get a config value and deserialize it from JSON.
    pub async fn get_typed<T: DeserializeOwned>(
        db: &DatabaseConnection,
        key: &str,
    ) -> Result<Option<T>, AppError> {
        let value = Self::get(db, key).await?;
        match value {
            Some(v) => {
                let parsed: T = serde_json::from_str(&v)
                    .map_err(|e| AppError::Internal(format!("Failed to deserialize config: {e}")))?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }

    /// Serialize a value to JSON and store it as a config entry.
    pub async fn set_typed<T: Serialize>(
        db: &DatabaseConnection,
        key: &str,
        value: &T,
    ) -> Result<(), AppError> {
        let json = serde_json::to_string(value)
            .map_err(|e| AppError::Internal(format!("Failed to serialize config: {e}")))?;
        Self::set(db, key, &json).await
    }
}
