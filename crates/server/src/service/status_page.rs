//! Admin status-page service (singleton).
//!
//! After R1 the `status_page` table holds exactly one row representing the
//! public status page configuration. The admin surface is therefore a single
//! GET / PUT pair — no create / delete — and this service exposes just the
//! two corresponding operations.
//!
//! Public-facing queries live in `crate::service::public_status`; this module
//! is admin-only.

use chrono::Utc;
use sea_orm::*;
use serde::{Deserialize, Serialize};

use crate::entity::status_page;
use crate::error::AppError;
use crate::service::public_status;

pub struct StatusPageService;

/// Partial-update payload for the singleton status_page row.
///
/// Every field is optional so the admin UI can PATCH individual toggles
/// without re-sending the entire config. Matches the prevailing admin
/// update-DTO convention in this codebase (see `UpdateMaintenance`,
/// `UpdateIncident`, `UpdateServerInput`): `Option<T>` = leave alone,
/// `Option<Option<T>>` = leave alone / clear / set for nullable columns.
#[derive(Debug, Default, Deserialize, Serialize, utoipa::ToSchema)]
pub struct UpdateStatusPage {
    pub title: Option<String>,
    /// Nullable in the entity — `Option<Option<String>>` so callers can
    /// distinguish "leave alone" (absent / null at the outer Option) from
    /// "explicitly clear" (`Some(None)`). Mirrors `UpdateMaintenance`.
    pub description: Option<Option<String>>,
    /// Replace the full set of pinned servers. `None` = leave alone,
    /// `Some(vec![])` = explicitly no servers. The service serialises
    /// this to the entity's `server_ids_json` storage column.
    pub server_ids: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub uptime_yellow_threshold: Option<f64>,
    pub uptime_red_threshold: Option<f64>,
    pub show_ip_quality: Option<bool>,
    pub default_layout: Option<String>,
    pub show_server_detail: Option<bool>,
    pub show_network: Option<bool>,
    pub show_incidents: Option<bool>,
    pub show_maintenance: Option<bool>,
}

impl StatusPageService {
    /// Load the singleton status_page row.
    ///
    /// Thin wrapper around `public_status::load_config` so admin callers
    /// don't have to import a "public" namespace for an admin read. The
    /// underlying invariant (exactly one row exists after the R1
    /// migration) is the same for both surfaces.
    pub async fn get_singleton(db: &DatabaseConnection) -> Result<status_page::Model, AppError> {
        public_status::load_config(db).await
    }

    /// Apply the provided field overrides to the singleton row and return
    /// the updated model. The `id` is resolved internally — callers do
    /// not need to know it.
    pub async fn update_singleton(
        db: &DatabaseConnection,
        input: UpdateStatusPage,
    ) -> Result<status_page::Model, AppError> {
        let existing = Self::get_singleton(db).await?;
        let mut model: status_page::ActiveModel = existing.into();

        if let Some(title) = input.title {
            model.title = Set(title);
        }
        if let Some(description) = input.description {
            model.description = Set(description);
        }
        if let Some(server_ids) = input.server_ids {
            let json = serde_json::to_string(&server_ids)
                .map_err(|e| AppError::Validation(format!("Invalid server_ids: {e}")))?;
            model.server_ids_json = Set(json);
        }
        // `group_by_server_group` is intentionally not exposed on the admin
        // update DTO — the spec's "Admin Settings UI" no longer includes the
        // toggle. The entity column is preserved (left untouched here) so its
        // current value is kept for any internal callers; removal can happen
        // as a follow-up migration once nothing reads it.
        if let Some(enabled) = input.enabled {
            model.enabled = Set(enabled);
        }
        if let Some(yellow) = input.uptime_yellow_threshold {
            model.uptime_yellow_threshold = Set(yellow);
        }
        if let Some(red) = input.uptime_red_threshold {
            model.uptime_red_threshold = Set(red);
        }
        if let Some(show_ip_quality) = input.show_ip_quality {
            model.show_ip_quality = Set(show_ip_quality);
        }
        if let Some(layout) = input.default_layout {
            model.default_layout = Set(layout);
        }
        if let Some(show_server_detail) = input.show_server_detail {
            model.show_server_detail = Set(show_server_detail);
        }
        if let Some(show_network) = input.show_network {
            model.show_network = Set(show_network);
        }
        if let Some(show_incidents) = input.show_incidents {
            model.show_incidents = Set(show_incidents);
        }
        if let Some(show_maintenance) = input.show_maintenance {
            model.show_maintenance = Set(show_maintenance);
        }
        model.updated_at = Set(Utc::now());

        Ok(model.update(db).await?)
    }
}

#[cfg(test)]
mod tests {
    // `super::*` re-exports `status_page`, `public_status`, `AppError`,
    // `UpdateStatusPage`, `StatusPageService`, and the `sea_orm::*` glob
    // (Entity/Column/Set traits), so only `setup_test_db` needs importing here.
    use super::*;
    use crate::test_utils::setup_test_db;

    // ---------------------------------------------------------------------
    // get_singleton
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn get_singleton_returns_seeded_defaults() {
        // The migration seeds exactly one row with documented default values.
        let (db, _tmp) = setup_test_db().await;
        let row = StatusPageService::get_singleton(&db)
            .await
            .expect("singleton present");
        assert!(!row.id.is_empty(), "seeded row has a non-empty id");
        assert_eq!(row.title, "Status");
        assert!(row.description.is_none());
        assert_eq!(row.server_ids_json, "[]");
        assert!(!row.enabled, "seed default is disabled");
        assert_eq!(row.uptime_yellow_threshold, 99.0);
        assert_eq!(row.uptime_red_threshold, 95.0);
        assert!(!row.show_ip_quality);
        assert_eq!(row.default_layout, "grid");
        assert!(row.show_server_detail);
        assert!(!row.show_network);
        assert!(row.show_incidents);
        assert!(row.show_maintenance);
    }

    #[tokio::test]
    async fn get_singleton_matches_public_load_config() {
        // get_singleton is a thin wrapper over public_status::load_config and
        // must return the identical row.
        let (db, _tmp) = setup_test_db().await;
        let via_service = StatusPageService::get_singleton(&db).await.unwrap();
        let via_public = public_status::load_config(&db).await.unwrap();
        assert_eq!(via_service.id, via_public.id);
        assert_eq!(via_service.title, via_public.title);
    }

    #[tokio::test]
    async fn get_singleton_not_found_when_row_missing() {
        // Violating the singleton invariant surfaces NotFound, not a panic.
        let (db, _tmp) = setup_test_db().await;
        status_page::Entity::delete_many()
            .exec(&db)
            .await
            .expect("clear singleton");
        let err = StatusPageService::get_singleton(&db).await;
        assert!(matches!(err, Err(AppError::NotFound(_))));
    }

    // ---------------------------------------------------------------------
    // update_singleton — empty payload (every field left alone)
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn update_singleton_empty_payload_preserves_all_fields() {
        // A default (all-None) payload changes nothing except updated_at.
        let (db, _tmp) = setup_test_db().await;
        let before = StatusPageService::get_singleton(&db).await.unwrap();
        let after = StatusPageService::update_singleton(&db, UpdateStatusPage::default())
            .await
            .unwrap();
        assert_eq!(after.id, before.id, "same singleton row");
        assert_eq!(after.title, before.title);
        assert_eq!(after.description, before.description);
        assert_eq!(after.server_ids_json, before.server_ids_json);
        assert_eq!(after.enabled, before.enabled);
        assert_eq!(after.default_layout, before.default_layout);
        assert_eq!(after.show_ip_quality, before.show_ip_quality);
        assert_eq!(after.show_server_detail, before.show_server_detail);
        assert_eq!(after.show_network, before.show_network);
        assert_eq!(after.show_incidents, before.show_incidents);
        assert_eq!(after.show_maintenance, before.show_maintenance);
        assert_eq!(after.uptime_yellow_threshold, before.uptime_yellow_threshold);
        assert_eq!(after.uptime_red_threshold, before.uptime_red_threshold);
    }

    #[tokio::test]
    async fn update_singleton_empty_payload_does_not_create_a_second_row() {
        // The update must mutate the existing row, never insert a new one.
        let (db, _tmp) = setup_test_db().await;
        StatusPageService::update_singleton(&db, UpdateStatusPage::default())
            .await
            .unwrap();
        let count = status_page::Entity::find().all(&db).await.unwrap().len();
        assert_eq!(count, 1, "singleton invariant preserved");
    }

    #[tokio::test]
    async fn update_singleton_bumps_updated_at() {
        // updated_at is always refreshed even when no other field changes.
        let (db, _tmp) = setup_test_db().await;
        let before = StatusPageService::get_singleton(&db).await.unwrap();
        let after = StatusPageService::update_singleton(&db, UpdateStatusPage::default())
            .await
            .unwrap();
        assert!(
            after.updated_at >= before.updated_at,
            "updated_at is refreshed on every update"
        );
    }

    // ---------------------------------------------------------------------
    // update_singleton — individual field branches
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn update_singleton_sets_title() {
        // Some(title) overwrites the title column.
        let (db, _tmp) = setup_test_db().await;
        let updated = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                title: Some("My Page".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.title, "My Page");
        // Persisted, not just returned.
        let reloaded = StatusPageService::get_singleton(&db).await.unwrap();
        assert_eq!(reloaded.title, "My Page");
    }

    #[tokio::test]
    async fn update_singleton_sets_description() {
        // Some(Some(desc)) sets the nullable description column.
        let (db, _tmp) = setup_test_db().await;
        let updated = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                description: Some(Some("a description".to_string())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.description.as_deref(), Some("a description"));
    }

    #[tokio::test]
    async fn update_singleton_clears_description() {
        // Some(None) explicitly clears a previously-set description to NULL.
        let (db, _tmp) = setup_test_db().await;
        // First set a description so clearing is observable.
        StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                description: Some(Some("temporary".to_string())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let cleared = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                description: Some(None),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(cleared.description.is_none(), "description cleared to NULL");
    }

    #[tokio::test]
    async fn update_singleton_leaves_description_alone_when_outer_none() {
        // Outer None on the Option<Option<String>> means "leave alone".
        let (db, _tmp) = setup_test_db().await;
        StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                description: Some(Some("keep me".to_string())),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        // A subsequent update with description: None must not touch it.
        let after = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                title: Some("unrelated".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(after.description.as_deref(), Some("keep me"));
    }

    #[tokio::test]
    async fn update_singleton_serializes_server_ids() {
        // Some(vec) is serialized to JSON into server_ids_json.
        let (db, _tmp) = setup_test_db().await;
        let updated = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                server_ids: Some(vec!["a".to_string(), "b".to_string()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.server_ids_json, r#"["a","b"]"#);
    }

    #[tokio::test]
    async fn update_singleton_server_ids_empty_vec_is_explicit_clear() {
        // Some(vec![]) explicitly stores an empty JSON array.
        let (db, _tmp) = setup_test_db().await;
        // Seed a non-empty list first.
        StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                server_ids: Some(vec!["x".to_string()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let cleared = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                server_ids: Some(vec![]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(cleared.server_ids_json, "[]");
    }

    #[tokio::test]
    async fn update_singleton_toggles_enabled() {
        // Some(bool) flips the enabled flag (seed default is false).
        let (db, _tmp) = setup_test_db().await;
        let updated = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                enabled: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(updated.enabled);
    }

    #[tokio::test]
    async fn update_singleton_sets_uptime_thresholds() {
        // Both threshold floats are independently settable.
        let (db, _tmp) = setup_test_db().await;
        let updated = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                uptime_yellow_threshold: Some(98.5),
                uptime_red_threshold: Some(90.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.uptime_yellow_threshold, 98.5);
        assert_eq!(updated.uptime_red_threshold, 90.0);
    }

    #[tokio::test]
    async fn update_singleton_sets_show_ip_quality() {
        // show_ip_quality toggle (seed default false).
        let (db, _tmp) = setup_test_db().await;
        let updated = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                show_ip_quality: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(updated.show_ip_quality);
    }

    #[tokio::test]
    async fn update_singleton_sets_default_layout() {
        // default_layout string is overwritten.
        let (db, _tmp) = setup_test_db().await;
        let updated = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                default_layout: Some("list".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.default_layout, "list");
    }

    #[tokio::test]
    async fn update_singleton_sets_show_server_detail() {
        // show_server_detail toggle (seed default true => set false).
        let (db, _tmp) = setup_test_db().await;
        let updated = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                show_server_detail: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(!updated.show_server_detail);
    }

    #[tokio::test]
    async fn update_singleton_sets_show_network() {
        // show_network toggle (seed default false => set true).
        let (db, _tmp) = setup_test_db().await;
        let updated = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                show_network: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(updated.show_network);
    }

    #[tokio::test]
    async fn update_singleton_sets_show_incidents() {
        // show_incidents toggle (seed default true => set false).
        let (db, _tmp) = setup_test_db().await;
        let updated = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                show_incidents: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(!updated.show_incidents);
    }

    #[tokio::test]
    async fn update_singleton_sets_show_maintenance() {
        // show_maintenance toggle (seed default true => set false).
        let (db, _tmp) = setup_test_db().await;
        let updated = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                show_maintenance: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(!updated.show_maintenance);
    }

    // ---------------------------------------------------------------------
    // update_singleton — all fields together + persistence
    // ---------------------------------------------------------------------

    #[tokio::test]
    async fn update_singleton_applies_every_field_at_once() {
        // A fully-populated payload sets every column in a single call and the
        // changes are persisted to the DB.
        let (db, _tmp) = setup_test_db().await;
        let input = UpdateStatusPage {
            title: Some("Full".to_string()),
            description: Some(Some("d".to_string())),
            server_ids: Some(vec!["s1".to_string()]),
            enabled: Some(true),
            uptime_yellow_threshold: Some(97.0),
            uptime_red_threshold: Some(80.0),
            show_ip_quality: Some(true),
            default_layout: Some("list".to_string()),
            show_server_detail: Some(false),
            show_network: Some(true),
            show_incidents: Some(false),
            show_maintenance: Some(false),
        };
        StatusPageService::update_singleton(&db, input).await.unwrap();

        // Reload from the DB to prove persistence (not just the returned model).
        let row = StatusPageService::get_singleton(&db).await.unwrap();
        assert_eq!(row.title, "Full");
        assert_eq!(row.description.as_deref(), Some("d"));
        assert_eq!(row.server_ids_json, r#"["s1"]"#);
        assert!(row.enabled);
        assert_eq!(row.uptime_yellow_threshold, 97.0);
        assert_eq!(row.uptime_red_threshold, 80.0);
        assert!(row.show_ip_quality);
        assert_eq!(row.default_layout, "list");
        assert!(!row.show_server_detail);
        assert!(row.show_network);
        assert!(!row.show_incidents);
        assert!(!row.show_maintenance);
    }

    #[tokio::test]
    async fn update_singleton_partial_update_leaves_others_untouched() {
        // Setting only `enabled` must not disturb any sibling column.
        let (db, _tmp) = setup_test_db().await;
        let before = StatusPageService::get_singleton(&db).await.unwrap();
        let after = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                enabled: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(after.enabled, "target field changed");
        // Untouched siblings retain their prior values.
        assert_eq!(after.title, before.title);
        assert_eq!(after.server_ids_json, before.server_ids_json);
        assert_eq!(after.default_layout, before.default_layout);
        assert_eq!(after.show_incidents, before.show_incidents);
        assert_eq!(after.uptime_yellow_threshold, before.uptime_yellow_threshold);
    }

    #[tokio::test]
    async fn update_singleton_preserves_group_by_server_group_column() {
        // group_by_server_group is intentionally not exposed on the DTO and
        // must be left untouched (seed default is true).
        let (db, _tmp) = setup_test_db().await;
        let after = StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                title: Some("x".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(
            after.group_by_server_group,
            "untouched legacy column keeps its seeded value"
        );
    }

    #[tokio::test]
    async fn update_singleton_not_found_when_row_missing() {
        // With no singleton row present, the update fails with NotFound from
        // the internal get_singleton lookup.
        let (db, _tmp) = setup_test_db().await;
        status_page::Entity::delete_many()
            .exec(&db)
            .await
            .expect("clear singleton");
        let err =
            StatusPageService::update_singleton(&db, UpdateStatusPage::default()).await;
        assert!(matches!(err, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn update_singleton_round_trips_through_public_load_config() {
        // After an admin update, the public reader sees the same row, proving
        // both surfaces target the one singleton.
        let (db, _tmp) = setup_test_db().await;
        StatusPageService::update_singleton(
            &db,
            UpdateStatusPage {
                enabled: Some(true),
                title: Some("Shared".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let public = public_status::load_config(&db).await.unwrap();
        assert!(public.enabled);
        assert_eq!(public.title, "Shared");
        // Sanity: still exactly one row reachable by the singleton filter.
        let rows = status_page::Entity::find()
            .filter(status_page::Column::Title.eq("Shared"))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
