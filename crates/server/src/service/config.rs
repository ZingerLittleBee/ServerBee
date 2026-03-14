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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;

    #[tokio::test]
    async fn test_get_set_config() {
        let (db, _tmp) = setup_test_db().await;

        // Set a key-value pair
        ConfigService::set(&db, "test_key", "test_value")
            .await
            .expect("set should succeed");

        // Get it back and verify
        let value = ConfigService::get(&db, "test_key")
            .await
            .expect("get should succeed");

        assert_eq!(value, Some("test_value".to_string()), "Retrieved value should match what was set");
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let (db, _tmp) = setup_test_db().await;

        // Get a key that was never set
        let value = ConfigService::get(&db, "nonexistent_key")
            .await
            .expect("get for missing key should succeed (not error)");

        assert_eq!(value, None, "Missing key should return None");
    }

    #[tokio::test]
    async fn test_set_upsert() {
        let (db, _tmp) = setup_test_db().await;

        // Set initial value
        ConfigService::set(&db, "upsert_key", "initial")
            .await
            .expect("first set should succeed");

        // Overwrite with new value
        ConfigService::set(&db, "upsert_key", "updated")
            .await
            .expect("second set (upsert) should succeed");

        let value = ConfigService::get(&db, "upsert_key")
            .await
            .expect("get after upsert should succeed");

        assert_eq!(value, Some("updated".to_string()), "Value should be updated after upsert");
    }

    #[tokio::test]
    async fn test_get_typed() {
        let (db, _tmp) = setup_test_db().await;

        // Store a JSON-serializable typed value
        let original: Vec<u32> = vec![1, 2, 3];
        ConfigService::set_typed(&db, "typed_key", &original)
            .await
            .expect("set_typed should succeed");

        // Retrieve and deserialize
        let retrieved: Option<Vec<u32>> = ConfigService::get_typed(&db, "typed_key")
            .await
            .expect("get_typed should succeed");

        assert_eq!(retrieved, Some(vec![1, 2, 3]), "Typed value should round-trip correctly");
    }

    #[tokio::test]
    async fn test_get_typed_nonexistent() {
        let (db, _tmp) = setup_test_db().await;

        let result: Option<u64> = ConfigService::get_typed(&db, "no_such_key")
            .await
            .expect("get_typed for missing key should return None, not error");

        assert_eq!(result, None, "Missing key should return None for get_typed");
    }
}
