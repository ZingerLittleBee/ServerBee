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
    /// Whether this maintenance window is shown on the public status page.
    #[serde(default)]
    pub is_public: bool,
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
    pub is_public: Option<bool>,
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

        let now = Utc::now();
        let model = maintenance::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            title: Set(input.title),
            description: Set(input.description),
            start_at: Set(input.start_at),
            end_at: Set(input.end_at),
            server_ids_json: Set(server_ids_json),
            is_public: Set(input.is_public),
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
        if let Some(is_public) = input.is_public {
            model.is_public = Set(is_public);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;
    use chrono::Duration;

    /// Build a CreateMaintenance with a window relative to `now`.
    fn make_input(
        title: &str,
        start_offset_secs: i64,
        end_offset_secs: i64,
        server_ids: Option<Vec<String>>,
        active: Option<bool>,
    ) -> CreateMaintenance {
        let now = Utc::now();
        CreateMaintenance {
            title: title.to_string(),
            description: None,
            start_at: now + Duration::seconds(start_offset_secs),
            end_at: now + Duration::seconds(end_offset_secs),
            server_ids_json: server_ids,
            is_public: false,
            active,
        }
    }

    #[tokio::test]
    async fn create_persists_defaults_and_returns_model() {
        let (db, _tmp) = setup_test_db().await;

        // active = None should default to true, is_public stays false.
        let input = make_input("window-a", -60, 3600, None, None);
        let model = MaintenanceService::create(&db, input).await.unwrap();

        assert_eq!(model.title, "window-a");
        assert!(model.active, "active should default to true when None");
        assert!(!model.is_public);
        assert!(model.server_ids_json.is_none());
        assert_eq!(model.description, None);
        assert_eq!(model.created_at, model.updated_at);
        assert!(!model.id.is_empty(), "id should be a generated uuid");
    }

    #[tokio::test]
    async fn create_serializes_server_ids_and_respects_active_false() {
        let (db, _tmp) = setup_test_db().await;

        let ids = vec!["srv-1".to_string(), "srv-2".to_string()];
        let mut input = make_input("window-b", -60, 3600, Some(ids.clone()), Some(false));
        input.description = Some("scheduled".to_string());
        input.is_public = true;
        let model = MaintenanceService::create(&db, input).await.unwrap();

        assert!(!model.active, "active=Some(false) should be honored");
        assert!(model.is_public);
        assert_eq!(model.description, Some("scheduled".to_string()));

        // server_ids_json should be a JSON-serialized array.
        let stored = model.server_ids_json.expect("server_ids_json should be set");
        let parsed: Vec<String> = serde_json::from_str(&stored).unwrap();
        assert_eq!(parsed, ids);
    }

    #[tokio::test]
    async fn create_rejects_end_equal_to_start() {
        let (db, _tmp) = setup_test_db().await;

        // end_at == start_at must be rejected (<=).
        let input = make_input("bad-eq", 100, 100, None, None);
        let err = MaintenanceService::create(&db, input).await.unwrap_err();
        match err {
            AppError::Validation(msg) => {
                assert!(msg.contains("end_at must be after start_at"), "got: {msg}");
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_rejects_end_before_start() {
        let (db, _tmp) = setup_test_db().await;

        let input = make_input("bad-order", 3600, 60, None, None);
        let err = MaintenanceService::create(&db, input).await.unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn get_returns_not_found_for_missing_id() {
        let (db, _tmp) = setup_test_db().await;

        let err = MaintenanceService::get(&db, "does-not-exist")
            .await
            .unwrap_err();
        match err {
            AppError::NotFound(msg) => assert!(msg.contains("does-not-exist"), "got: {msg}"),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_returns_existing_row() {
        let (db, _tmp) = setup_test_db().await;

        let created = MaintenanceService::create(&db, make_input("getme", -60, 3600, None, None))
            .await
            .unwrap();

        let fetched = MaintenanceService::get(&db, &created.id).await.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.title, "getme");
    }

    #[tokio::test]
    async fn list_orders_by_created_at_desc() {
        let (db, _tmp) = setup_test_db().await;

        // Insert two rows with explicitly different created_at so ordering is deterministic.
        let now = Utc::now();
        let older = maintenance::ActiveModel {
            id: Set("older".to_string()),
            title: Set("older".to_string()),
            description: Set(None),
            start_at: Set(now - Duration::seconds(60)),
            end_at: Set(now + Duration::seconds(60)),
            server_ids_json: Set(None),
            is_public: Set(false),
            active: Set(true),
            created_at: Set(now - Duration::hours(2)),
            updated_at: Set(now - Duration::hours(2)),
        };
        older.insert(&db).await.unwrap();

        let newer = maintenance::ActiveModel {
            id: Set("newer".to_string()),
            title: Set("newer".to_string()),
            description: Set(None),
            start_at: Set(now - Duration::seconds(60)),
            end_at: Set(now + Duration::seconds(60)),
            server_ids_json: Set(None),
            is_public: Set(false),
            active: Set(true),
            created_at: Set(now - Duration::hours(1)),
            updated_at: Set(now - Duration::hours(1)),
        };
        newer.insert(&db).await.unwrap();

        let list = MaintenanceService::list(&db).await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "newer", "most recent created_at should be first");
        assert_eq!(list[1].id, "older");
    }

    #[tokio::test]
    async fn list_empty_returns_empty_vec() {
        let (db, _tmp) = setup_test_db().await;
        let list = MaintenanceService::list(&db).await.unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn update_all_fields_changes_row() {
        let (db, _tmp) = setup_test_db().await;

        let created = MaintenanceService::create(
            &db,
            make_input("orig", -60, 3600, Some(vec!["a".to_string()]), Some(true)),
        )
        .await
        .unwrap();
        let original_updated_at = created.updated_at;

        let new_start = Utc::now() + Duration::seconds(10);
        let new_end = Utc::now() + Duration::seconds(7200);
        let input = UpdateMaintenance {
            title: Some("changed".to_string()),
            description: Some(Some("now has desc".to_string())),
            start_at: Some(new_start),
            end_at: Some(new_end),
            server_ids_json: Some(Some(vec!["x".to_string(), "y".to_string()])),
            is_public: Some(true),
            active: Some(false),
        };

        let updated = MaintenanceService::update(&db, &created.id, input)
            .await
            .unwrap();

        assert_eq!(updated.title, "changed");
        assert_eq!(updated.description, Some("now has desc".to_string()));
        assert!(updated.is_public);
        assert!(!updated.active);
        assert!(
            updated.updated_at >= original_updated_at,
            "updated_at should be refreshed"
        );
        let parsed: Vec<String> = serde_json::from_str(updated.server_ids_json.as_ref().unwrap()).unwrap();
        assert_eq!(parsed, vec!["x".to_string(), "y".to_string()]);
    }

    #[tokio::test]
    async fn update_with_all_none_keeps_existing_values() {
        let (db, _tmp) = setup_test_db().await;

        let created = MaintenanceService::create(
            &db,
            make_input("keep", -60, 3600, Some(vec!["s1".to_string()]), Some(true)),
        )
        .await
        .unwrap();

        // All-None update: only updated_at should change.
        let input = UpdateMaintenance {
            title: None,
            description: None,
            start_at: None,
            end_at: None,
            server_ids_json: None,
            is_public: None,
            active: None,
        };
        let updated = MaintenanceService::update(&db, &created.id, input)
            .await
            .unwrap();

        assert_eq!(updated.title, "keep");
        assert!(updated.active);
        assert_eq!(updated.server_ids_json, created.server_ids_json);
    }

    #[tokio::test]
    async fn update_can_clear_server_ids_with_explicit_none() {
        let (db, _tmp) = setup_test_db().await;

        let created = MaintenanceService::create(
            &db,
            make_input("clearme", -60, 3600, Some(vec!["s1".to_string()]), Some(true)),
        )
        .await
        .unwrap();
        assert!(created.server_ids_json.is_some());

        // server_ids_json = Some(None) clears the filter.
        let input = UpdateMaintenance {
            title: None,
            description: None,
            start_at: None,
            end_at: None,
            server_ids_json: Some(None),
            is_public: None,
            active: None,
        };
        let updated = MaintenanceService::update(&db, &created.id, input)
            .await
            .unwrap();
        assert!(updated.server_ids_json.is_none());
    }

    #[tokio::test]
    async fn update_can_clear_description_with_explicit_none() {
        let (db, _tmp) = setup_test_db().await;

        let mut create_input = make_input("desc", -60, 3600, None, Some(true));
        create_input.description = Some("had description".to_string());
        let created = MaintenanceService::create(&db, create_input).await.unwrap();
        assert!(created.description.is_some());

        let input = UpdateMaintenance {
            title: None,
            description: Some(None),
            start_at: None,
            end_at: None,
            server_ids_json: None,
            is_public: None,
            active: None,
        };
        let updated = MaintenanceService::update(&db, &created.id, input)
            .await
            .unwrap();
        assert!(updated.description.is_none());
    }

    #[tokio::test]
    async fn update_missing_id_returns_not_found() {
        let (db, _tmp) = setup_test_db().await;

        let input = UpdateMaintenance {
            title: Some("x".to_string()),
            description: None,
            start_at: None,
            end_at: None,
            server_ids_json: None,
            is_public: None,
            active: None,
        };
        let err = MaintenanceService::update(&db, "missing", input)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_existing_succeeds() {
        let (db, _tmp) = setup_test_db().await;

        let created = MaintenanceService::create(&db, make_input("del", -60, 3600, None, None))
            .await
            .unwrap();

        MaintenanceService::delete(&db, &created.id).await.unwrap();

        // Subsequent get must report NotFound.
        let err = MaintenanceService::get(&db, &created.id).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_missing_returns_not_found() {
        let (db, _tmp) = setup_test_db().await;

        let err = MaintenanceService::delete(&db, "nope").await.unwrap_err();
        match err {
            AppError::NotFound(msg) => assert!(msg.contains("nope"), "got: {msg}"),
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn is_in_maintenance_active_window_no_filter_applies_to_all() {
        let (db, _tmp) = setup_test_db().await;

        // Active window covering now, no server filter → applies to any server.
        MaintenanceService::create(&db, make_input("global", -60, 3600, None, Some(true)))
            .await
            .unwrap();

        assert!(
            MaintenanceService::is_in_maintenance(&db, "any-server")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn is_in_maintenance_future_window_is_not_active() {
        let (db, _tmp) = setup_test_db().await;

        // Window entirely in the future.
        MaintenanceService::create(&db, make_input("future", 3600, 7200, None, Some(true)))
            .await
            .unwrap();

        assert!(
            !MaintenanceService::is_in_maintenance(&db, "srv")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn is_in_maintenance_past_window_is_not_active() {
        let (db, _tmp) = setup_test_db().await;

        // Window entirely in the past.
        MaintenanceService::create(&db, make_input("past", -7200, -3600, None, Some(true)))
            .await
            .unwrap();

        assert!(
            !MaintenanceService::is_in_maintenance(&db, "srv")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn is_in_maintenance_inactive_flag_excluded() {
        let (db, _tmp) = setup_test_db().await;

        // Window covers now but active = false.
        MaintenanceService::create(&db, make_input("disabled", -60, 3600, None, Some(false)))
            .await
            .unwrap();

        assert!(
            !MaintenanceService::is_in_maintenance(&db, "srv")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn is_in_maintenance_matches_listed_server_id() {
        let (db, _tmp) = setup_test_db().await;

        MaintenanceService::create(
            &db,
            make_input(
                "targeted",
                -60,
                3600,
                Some(vec!["srv-a".to_string(), "srv-b".to_string()]),
                Some(true),
            ),
        )
        .await
        .unwrap();

        assert!(
            MaintenanceService::is_in_maintenance(&db, "srv-b")
                .await
                .unwrap(),
            "listed server should be in maintenance"
        );
        assert!(
            !MaintenanceService::is_in_maintenance(&db, "srv-c")
                .await
                .unwrap(),
            "unlisted server should not be in maintenance"
        );
    }

    #[tokio::test]
    async fn is_in_maintenance_empty_id_array_applies_to_all() {
        let (db, _tmp) = setup_test_db().await;

        // server_ids_json present but empty array → treated as applying to all.
        MaintenanceService::create(
            &db,
            make_input("empty-array", -60, 3600, Some(vec![]), Some(true)),
        )
        .await
        .unwrap();

        assert!(
            MaintenanceService::is_in_maintenance(&db, "whatever")
                .await
                .unwrap(),
            "empty id array should apply to all servers"
        );
    }

    #[tokio::test]
    async fn is_in_maintenance_malformed_json_treated_as_empty() {
        let (db, _tmp) = setup_test_db().await;

        // Insert a row directly with invalid JSON in server_ids_json.
        // serde_json::from_str(..).unwrap_or_default() yields an empty vec,
        // which the logic treats as "applies to all".
        let now = Utc::now();
        let bad = maintenance::ActiveModel {
            id: Set("bad-json".to_string()),
            title: Set("bad".to_string()),
            description: Set(None),
            start_at: Set(now - Duration::seconds(60)),
            end_at: Set(now + Duration::seconds(3600)),
            server_ids_json: Set(Some("not-valid-json".to_string())),
            is_public: Set(false),
            active: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
        };
        bad.insert(&db).await.unwrap();

        assert!(
            MaintenanceService::is_in_maintenance(&db, "srv")
                .await
                .unwrap(),
            "malformed json falls back to empty vec → applies to all"
        );
    }

    #[tokio::test]
    async fn is_in_maintenance_no_rows_returns_false() {
        let (db, _tmp) = setup_test_db().await;

        assert!(
            !MaintenanceService::is_in_maintenance(&db, "srv")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn is_in_maintenance_picks_matching_among_multiple() {
        let (db, _tmp) = setup_test_db().await;

        // Inactive window (should be skipped).
        MaintenanceService::create(&db, make_input("off", -60, 3600, None, Some(false)))
            .await
            .unwrap();
        // Active window targeting a different server.
        MaintenanceService::create(
            &db,
            make_input("other", -60, 3600, Some(vec!["other-srv".to_string()]), Some(true)),
        )
        .await
        .unwrap();
        // Active window targeting our server.
        MaintenanceService::create(
            &db,
            make_input("mine", -60, 3600, Some(vec!["my-srv".to_string()]), Some(true)),
        )
        .await
        .unwrap();

        assert!(
            MaintenanceService::is_in_maintenance(&db, "my-srv")
                .await
                .unwrap()
        );
    }
}
