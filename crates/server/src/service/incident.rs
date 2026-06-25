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
    /// Whether this incident is shown on the public status page.
    #[serde(default)]
    pub is_public: bool,
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
    pub is_public: Option<bool>,
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
            is_public: Set(input.is_public),
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
        if let Some(is_public) = input.is_public {
            model.is_public = Set(is_public);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;
    use chrono::TimeZone;

    /// Build a minimal CreateIncident with all optional fields at their defaults.
    fn make_input(title: &str) -> CreateIncident {
        CreateIncident {
            title: title.to_string(),
            status: default_status(),
            severity: default_severity(),
            server_ids_json: None,
            is_public: false,
        }
    }

    #[tokio::test]
    async fn create_persists_defaults_and_returns_model() {
        let (db, _tmp) = setup_test_db().await;

        // Default status "investigating" is not "resolved", so resolved_at stays None.
        let model = IncidentService::create(&db, make_input("outage-a"))
            .await
            .unwrap();

        assert_eq!(model.title, "outage-a");
        assert_eq!(model.status, "investigating");
        assert_eq!(model.severity, "minor");
        assert!(!model.is_public);
        assert!(model.server_ids_json.is_none());
        assert!(model.resolved_at.is_none());
        assert_eq!(model.created_at, model.updated_at);
        assert!(!model.id.is_empty(), "id should be a generated uuid");
    }

    #[tokio::test]
    async fn create_serializes_server_ids_and_honors_public() {
        let (db, _tmp) = setup_test_db().await;

        // server_ids_json should be JSON-serialized into the stored column.
        let ids = vec!["srv-1".to_string(), "srv-2".to_string()];
        let mut input = make_input("outage-b");
        input.server_ids_json = Some(ids.clone());
        input.is_public = true;
        let model = IncidentService::create(&db, input).await.unwrap();

        assert!(model.is_public);
        let stored = model.server_ids_json.expect("server_ids_json should be set");
        let parsed: Vec<String> = serde_json::from_str(&stored).unwrap();
        assert_eq!(parsed, ids);
    }

    #[tokio::test]
    async fn create_resolved_status_sets_resolved_at() {
        let (db, _tmp) = setup_test_db().await;

        // Creating directly with status "resolved" should populate resolved_at.
        let mut input = make_input("already-resolved");
        input.status = "resolved".to_string();
        let model = IncidentService::create(&db, input).await.unwrap();

        assert_eq!(model.status, "resolved");
        assert!(model.resolved_at.is_some(), "resolved_at must be set");
    }

    #[tokio::test]
    async fn create_rejects_invalid_status() {
        let (db, _tmp) = setup_test_db().await;

        // An unknown status string must be rejected with a Validation error.
        let mut input = make_input("bad-status");
        input.status = "bogus".to_string();
        let err = IncidentService::create(&db, input)
            .await
            .err()
            .expect("expected validation error");
        match err {
            AppError::Validation(msg) => assert!(msg.contains("status must be one of"), "got: {msg}"),
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_rejects_invalid_severity() {
        let (db, _tmp) = setup_test_db().await;

        // An unknown severity string must be rejected with a Validation error.
        let mut input = make_input("bad-severity");
        input.severity = "catastrophic".to_string();
        let err = IncidentService::create(&db, input)
            .await
            .err()
            .expect("expected validation error");
        match err {
            AppError::Validation(msg) => {
                assert!(msg.contains("severity must be one of"), "got: {msg}")
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_returns_existing_and_not_found() {
        let (db, _tmp) = setup_test_db().await;

        let created = IncidentService::create(&db, make_input("findme"))
            .await
            .unwrap();

        // get on an existing id returns the same row.
        let fetched = IncidentService::get(&db, &created.id).await.unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.title, "findme");

        // get on a missing id returns NotFound carrying the id.
        let err = IncidentService::get(&db, "missing-id")
            .await
            .err()
            .expect("expected not found");
        match err {
            AppError::NotFound(msg) => assert!(msg.contains("missing-id"), "got: {msg}"),
            other => panic!("expected NotFound error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_orders_by_created_at_desc() {
        let (db, _tmp) = setup_test_db().await;

        // Insert two incidents with explicit timestamps so ordering is deterministic.
        let older = incident::ActiveModel {
            id: Set("inc-old".to_string()),
            title: Set("old".to_string()),
            status: Set("investigating".to_string()),
            severity: Set("minor".to_string()),
            server_ids_json: Set(None),
            is_public: Set(false),
            created_at: Set(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()),
            updated_at: Set(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()),
            resolved_at: Set(None),
        };
        let newer = incident::ActiveModel {
            id: Set("inc-new".to_string()),
            title: Set("new".to_string()),
            status: Set("monitoring".to_string()),
            severity: Set("major".to_string()),
            server_ids_json: Set(None),
            is_public: Set(true),
            created_at: Set(Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap()),
            updated_at: Set(Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap()),
            resolved_at: Set(None),
        };
        older.insert(&db).await.unwrap();
        newer.insert(&db).await.unwrap();

        // No filter: newest first.
        let all = IncidentService::list(&db, None).await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "inc-new");
        assert_eq!(all[1].id, "inc-old");
    }

    #[tokio::test]
    async fn list_applies_status_filter() {
        let (db, _tmp) = setup_test_db().await;

        let mut resolved = make_input("done");
        resolved.status = "resolved".to_string();
        IncidentService::create(&db, resolved).await.unwrap();
        IncidentService::create(&db, make_input("ongoing"))
            .await
            .unwrap();

        // Filtering by "resolved" returns only the resolved incident.
        let only_resolved = IncidentService::list(&db, Some("resolved")).await.unwrap();
        assert_eq!(only_resolved.len(), 1);
        assert_eq!(only_resolved[0].title, "done");

        // Filtering by a status with no rows returns an empty vec.
        let none = IncidentService::list(&db, Some("identified")).await.unwrap();
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn list_empty_returns_empty_vec() {
        let (db, _tmp) = setup_test_db().await;

        // No incidents at all yields an empty list.
        let all = IncidentService::list(&db, None).await.unwrap();
        assert!(all.is_empty());
    }

    #[tokio::test]
    async fn update_changes_all_fields() {
        let (db, _tmp) = setup_test_db().await;

        let created = IncidentService::create(&db, make_input("orig")).await.unwrap();

        // Update every mutable field at once.
        let input = UpdateIncident {
            title: Some("renamed".to_string()),
            status: Some("monitoring".to_string()),
            severity: Some("critical".to_string()),
            server_ids_json: Some(Some(vec!["s1".to_string()])),
            is_public: Some(true),
        };
        let updated = IncidentService::update(&db, &created.id, input).await.unwrap();

        assert_eq!(updated.title, "renamed");
        assert_eq!(updated.status, "monitoring");
        assert_eq!(updated.severity, "critical");
        assert!(updated.is_public);
        let stored = updated.server_ids_json.expect("server_ids should be set");
        let parsed: Vec<String> = serde_json::from_str(&stored).unwrap();
        assert_eq!(parsed, vec!["s1".to_string()]);
        // updated_at advances past created_at on update.
        assert!(updated.updated_at >= created.updated_at);
        // Non-resolved status keeps resolved_at None.
        assert!(updated.resolved_at.is_none());
    }

    #[tokio::test]
    async fn update_with_all_none_only_touches_updated_at() {
        let (db, _tmp) = setup_test_db().await;

        let created = IncidentService::create(&db, make_input("untouched"))
            .await
            .unwrap();

        // All None: every field stays the same except updated_at.
        let input = UpdateIncident {
            title: None,
            status: None,
            severity: None,
            server_ids_json: None,
            is_public: None,
        };
        let updated = IncidentService::update(&db, &created.id, input).await.unwrap();

        assert_eq!(updated.title, created.title);
        assert_eq!(updated.status, created.status);
        assert_eq!(updated.severity, created.severity);
        assert_eq!(updated.is_public, created.is_public);
        assert_eq!(updated.server_ids_json, created.server_ids_json);
    }

    #[tokio::test]
    async fn update_to_resolved_sets_resolved_at_once() {
        let (db, _tmp) = setup_test_db().await;

        let created = IncidentService::create(&db, make_input("resolving"))
            .await
            .unwrap();
        assert!(created.resolved_at.is_none());

        // First transition to resolved stamps resolved_at.
        let input = UpdateIncident {
            title: None,
            status: Some("resolved".to_string()),
            severity: None,
            server_ids_json: None,
            is_public: None,
        };
        let resolved = IncidentService::update(&db, &created.id, input).await.unwrap();
        assert_eq!(resolved.status, "resolved");
        let first_resolved_at = resolved.resolved_at.expect("resolved_at must be set");

        // Re-applying resolved must NOT overwrite the original resolved_at.
        let input2 = UpdateIncident {
            title: None,
            status: Some("resolved".to_string()),
            severity: None,
            server_ids_json: None,
            is_public: None,
        };
        let again = IncidentService::update(&db, &created.id, input2).await.unwrap();
        assert_eq!(again.resolved_at, Some(first_resolved_at));
    }

    #[tokio::test]
    async fn update_clears_server_ids_with_inner_none() {
        let (db, _tmp) = setup_test_db().await;

        let mut input = make_input("hasids");
        input.server_ids_json = Some(vec!["a".to_string()]);
        let created = IncidentService::create(&db, input).await.unwrap();
        assert!(created.server_ids_json.is_some());

        // Some(None) explicitly clears the stored server_ids_json.
        let upd = UpdateIncident {
            title: None,
            status: None,
            severity: None,
            server_ids_json: Some(None),
            is_public: None,
        };
        let cleared = IncidentService::update(&db, &created.id, upd).await.unwrap();
        assert!(cleared.server_ids_json.is_none(), "server_ids_json should be cleared");
    }

    #[tokio::test]
    async fn update_rejects_invalid_status_and_severity() {
        let (db, _tmp) = setup_test_db().await;

        let created = IncidentService::create(&db, make_input("validate"))
            .await
            .unwrap();

        // Invalid status is rejected.
        let bad_status = UpdateIncident {
            title: None,
            status: Some("nope".to_string()),
            severity: None,
            server_ids_json: None,
            is_public: None,
        };
        let err = IncidentService::update(&db, &created.id, bad_status)
            .await
            .err()
            .expect("expected validation error");
        assert!(matches!(err, AppError::Validation(_)));

        // Invalid severity is rejected.
        let bad_sev = UpdateIncident {
            title: None,
            status: None,
            severity: Some("apocalyptic".to_string()),
            server_ids_json: None,
            is_public: None,
        };
        let err = IncidentService::update(&db, &created.id, bad_sev)
            .await
            .err()
            .expect("expected validation error");
        match err {
            AppError::Validation(msg) => {
                assert!(msg.contains("severity must be one of"), "got: {msg}")
            }
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn update_missing_incident_is_not_found() {
        let (db, _tmp) = setup_test_db().await;

        // update on a non-existent id propagates NotFound from get().
        let input = UpdateIncident {
            title: Some("x".to_string()),
            status: None,
            severity: None,
            server_ids_json: None,
            is_public: None,
        };
        let err = IncidentService::update(&db, "ghost", input)
            .await
            .err()
            .expect("expected not found");
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_removes_incident_and_cascades_updates() {
        let (db, _tmp) = setup_test_db().await;

        let created = IncidentService::create(&db, make_input("to-delete"))
            .await
            .unwrap();
        IncidentService::add_update(
            &db,
            &created.id,
            CreateIncidentUpdate {
                status: "identified".to_string(),
                message: "found cause".to_string(),
            },
        )
        .await
        .unwrap();

        // Delete should remove the incident and cascade-delete its updates.
        IncidentService::delete(&db, &created.id).await.unwrap();

        let gone = incident::Entity::find_by_id(&created.id)
            .one(&db)
            .await
            .unwrap();
        assert!(gone.is_none(), "incident row should be gone");
        let updates = IncidentService::list_updates(&db, &created.id).await.unwrap();
        assert!(updates.is_empty(), "child updates should be cascaded away");
    }

    #[tokio::test]
    async fn delete_missing_incident_is_not_found() {
        let (db, _tmp) = setup_test_db().await;

        // Deleting a non-existent incident returns NotFound (rows_affected == 0).
        let err = IncidentService::delete(&db, "nope")
            .await
            .err()
            .expect("expected not found");
        match err {
            AppError::NotFound(msg) => assert!(msg.contains("nope"), "got: {msg}"),
            other => panic!("expected NotFound error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn add_update_persists_and_syncs_incident_status() {
        let (db, _tmp) = setup_test_db().await;

        let created = IncidentService::create(&db, make_input("syncme"))
            .await
            .unwrap();
        assert_eq!(created.status, "investigating");

        // Adding an update with a non-resolved status syncs the incident status.
        let upd = IncidentService::add_update(
            &db,
            &created.id,
            CreateIncidentUpdate {
                status: "identified".to_string(),
                message: "root cause located".to_string(),
            },
        )
        .await
        .unwrap();

        assert_eq!(upd.incident_id, created.id);
        assert_eq!(upd.status, "identified");
        assert_eq!(upd.message, "root cause located");
        assert!(!upd.id.is_empty());

        // Parent incident status mirrors the update, resolved_at still None.
        let refreshed = IncidentService::get(&db, &created.id).await.unwrap();
        assert_eq!(refreshed.status, "identified");
        assert!(refreshed.resolved_at.is_none());
    }

    #[tokio::test]
    async fn add_update_resolved_sets_incident_resolved_at() {
        let (db, _tmp) = setup_test_db().await;

        let created = IncidentService::create(&db, make_input("closeme"))
            .await
            .unwrap();

        // A "resolved" update should stamp the incident's resolved_at.
        IncidentService::add_update(
            &db,
            &created.id,
            CreateIncidentUpdate {
                status: "resolved".to_string(),
                message: "fixed".to_string(),
            },
        )
        .await
        .unwrap();

        let refreshed = IncidentService::get(&db, &created.id).await.unwrap();
        assert_eq!(refreshed.status, "resolved");
        assert!(refreshed.resolved_at.is_some());
    }

    #[tokio::test]
    async fn add_update_rejects_invalid_status() {
        let (db, _tmp) = setup_test_db().await;

        let created = IncidentService::create(&db, make_input("guard"))
            .await
            .unwrap();

        // An invalid update status is rejected before persisting anything.
        let err = IncidentService::add_update(
            &db,
            &created.id,
            CreateIncidentUpdate {
                status: "weird".to_string(),
                message: "msg".to_string(),
            },
        )
        .await
        .err()
        .expect("expected validation error");
        match err {
            AppError::Validation(msg) => assert!(msg.contains("status must be one of"), "got: {msg}"),
            other => panic!("expected Validation error, got {other:?}"),
        }

        // No update row should have been written, and incident status is unchanged.
        let updates = IncidentService::list_updates(&db, &created.id).await.unwrap();
        assert!(updates.is_empty());
        let refreshed = IncidentService::get(&db, &created.id).await.unwrap();
        assert_eq!(refreshed.status, "investigating");
    }

    #[tokio::test]
    async fn add_update_missing_incident_is_not_found() {
        let (db, _tmp) = setup_test_db().await;

        // add_update on an unknown incident propagates NotFound from get().
        let err = IncidentService::add_update(
            &db,
            "absent",
            CreateIncidentUpdate {
                status: "investigating".to_string(),
                message: "m".to_string(),
            },
        )
        .await
        .err()
        .expect("expected not found");
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn list_updates_orders_by_created_at_asc() {
        let (db, _tmp) = setup_test_db().await;

        let created = IncidentService::create(&db, make_input("timeline"))
            .await
            .unwrap();

        // Insert two updates with explicit timestamps so ordering is deterministic.
        incident_update::ActiveModel {
            id: Set("upd-late".to_string()),
            incident_id: Set(created.id.clone()),
            status: Set("monitoring".to_string()),
            message: Set("later".to_string()),
            created_at: Set(Utc.with_ymd_and_hms(2026, 3, 2, 0, 0, 0).unwrap()),
        }
        .insert(&db)
        .await
        .unwrap();
        incident_update::ActiveModel {
            id: Set("upd-early".to_string()),
            incident_id: Set(created.id.clone()),
            status: Set("investigating".to_string()),
            message: Set("earlier".to_string()),
            created_at: Set(Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap()),
        }
        .insert(&db)
        .await
        .unwrap();

        // Updates come back oldest-first.
        let updates = IncidentService::list_updates(&db, &created.id).await.unwrap();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].id, "upd-early");
        assert_eq!(updates[1].id, "upd-late");
    }

    #[tokio::test]
    async fn list_updates_empty_and_isolated_per_incident() {
        let (db, _tmp) = setup_test_db().await;

        let a = IncidentService::create(&db, make_input("a")).await.unwrap();
        let b = IncidentService::create(&db, make_input("b")).await.unwrap();

        IncidentService::add_update(
            &db,
            &a.id,
            CreateIncidentUpdate {
                status: "identified".to_string(),
                message: "for a".to_string(),
            },
        )
        .await
        .unwrap();

        // Updates are scoped to their own incident.
        let a_updates = IncidentService::list_updates(&db, &a.id).await.unwrap();
        assert_eq!(a_updates.len(), 1);
        let b_updates = IncidentService::list_updates(&db, &b.id).await.unwrap();
        assert!(b_updates.is_empty());
    }
}
