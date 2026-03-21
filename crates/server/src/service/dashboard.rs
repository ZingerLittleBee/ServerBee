use chrono::Utc;
use sea_orm::prelude::Expr;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{dashboard, dashboard_widget};
use crate::error::AppError;

const VALID_WIDGET_TYPES: &[&str] = &[
    "stat-number",
    "server-cards",
    "gauge",
    "line-chart",
    "multi-line",
    "top-n",
    "alert-list",
    "service-status",
    "traffic-bar",
    "disk-io",
    "server-map",
    "markdown",
];

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateDashboardInput {
    pub name: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateDashboardInput {
    pub name: Option<String>,
    pub is_default: Option<bool>,
    pub sort_order: Option<i32>,
    pub widgets: Option<Vec<WidgetInput>>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct WidgetInput {
    pub id: Option<String>,
    pub widget_type: String,
    pub title: Option<String>,
    pub config_json: serde_json::Value,
    pub grid_x: i32,
    pub grid_y: i32,
    pub grid_w: i32,
    pub grid_h: i32,
    pub sort_order: i32,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DashboardWithWidgets {
    #[serde(flatten)]
    pub dashboard: dashboard::Model,
    pub widgets: Vec<dashboard_widget::Model>,
}

pub struct DashboardService;

impl DashboardService {
    /// List all dashboards, ordered by sort_order then created_at.
    pub async fn list(db: &DatabaseConnection) -> Result<Vec<dashboard::Model>, AppError> {
        Ok(dashboard::Entity::find()
            .order_by_asc(dashboard::Column::SortOrder)
            .order_by_asc(dashboard::Column::CreatedAt)
            .all(db)
            .await?)
    }

    /// Get a single dashboard with all its widgets.
    pub async fn get_with_widgets(
        db: &DatabaseConnection,
        id: &str,
    ) -> Result<DashboardWithWidgets, AppError> {
        let dash = dashboard::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound("Dashboard not found".into()))?;
        let widgets = dashboard_widget::Entity::find()
            .filter(dashboard_widget::Column::DashboardId.eq(id))
            .order_by_asc(dashboard_widget::Column::SortOrder)
            .all(db)
            .await?;
        Ok(DashboardWithWidgets {
            dashboard: dash,
            widgets,
        })
    }

    /// Get the default dashboard. Auto-creates one with preset widgets if none exists.
    pub async fn get_default(db: &DatabaseConnection) -> Result<DashboardWithWidgets, AppError> {
        // Try to find existing default
        if let Some(dash) = dashboard::Entity::find()
            .filter(dashboard::Column::IsDefault.eq(true))
            .one(db)
            .await?
        {
            let widgets = dashboard_widget::Entity::find()
                .filter(dashboard_widget::Column::DashboardId.eq(&dash.id))
                .order_by_asc(dashboard_widget::Column::SortOrder)
                .all(db)
                .await?;
            return Ok(DashboardWithWidgets {
                dashboard: dash,
                widgets,
            });
        }
        // Auto-create default
        Self::create_default(db).await
    }

    /// Create a new dashboard. The first dashboard created becomes the default.
    pub async fn create(
        db: &DatabaseConnection,
        input: CreateDashboardInput,
    ) -> Result<dashboard::Model, AppError> {
        let count = dashboard::Entity::find().count(db).await?;
        let is_default = count == 0;
        let now = Utc::now().to_rfc3339();
        let model = dashboard::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            name: Set(input.name),
            is_default: Set(is_default),
            sort_order: Set(0),
            created_at: Set(now.clone()),
            updated_at: Set(now),
        };
        Ok(model.insert(db).await?)
    }

    /// Update a dashboard's metadata and/or widgets.
    ///
    /// Widget diff: widgets with an existing `id` are updated, new widgets (no `id` or unknown `id`)
    /// are inserted, and old widgets not present in the input are deleted.
    pub async fn update(
        db: &DatabaseConnection,
        id: &str,
        input: UpdateDashboardInput,
    ) -> Result<DashboardWithWidgets, AppError> {
        let dash = dashboard::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound("Dashboard not found".into()))?;

        // Default invariant: cannot unset default without setting another
        if input.is_default == Some(false) && dash.is_default {
            return Err(AppError::BadRequest(
                "Cannot unset default. Set another dashboard as default first.".into(),
            ));
        }

        // Validate widget types
        if let Some(ref widgets) = input.widgets {
            for w in widgets {
                if !VALID_WIDGET_TYPES.contains(&w.widget_type.as_str()) {
                    return Err(AppError::BadRequest(format!(
                        "Unknown widget_type: {}",
                        w.widget_type
                    )));
                }
            }
        }

        let txn = db.begin().await?;

        // Update dashboard fields
        let mut model: dashboard::ActiveModel = dash.into();
        if let Some(name) = input.name {
            model.name = Set(name);
        }
        if let Some(true) = input.is_default {
            // Clear other defaults
            dashboard::Entity::update_many()
                .col_expr(dashboard::Column::IsDefault, Expr::value(false))
                .filter(dashboard::Column::Id.ne(id))
                .exec(&txn)
                .await?;
            model.is_default = Set(true);
        }
        if let Some(sort_order) = input.sort_order {
            model.sort_order = Set(sort_order);
        }
        model.updated_at = Set(Utc::now().to_rfc3339());
        model.update(&txn).await?;

        // Widget diff
        if let Some(widgets) = input.widgets {
            let old_widgets = dashboard_widget::Entity::find()
                .filter(dashboard_widget::Column::DashboardId.eq(id))
                .all(&txn)
                .await?;
            let old_ids: std::collections::HashSet<String> =
                old_widgets.iter().map(|w| w.id.clone()).collect();
            let mut new_ids = std::collections::HashSet::new();

            for w in widgets {
                let now = Utc::now().to_rfc3339();
                let config_str = serde_json::to_string(&w.config_json).unwrap_or_default();
                let is_existing = w.id.as_ref().is_some_and(|wid| old_ids.contains(wid));

                if is_existing {
                    // Update existing widget
                    let wid = w.id.unwrap();
                    new_ids.insert(wid.clone());
                    let am = dashboard_widget::ActiveModel {
                        id: Set(wid),
                        dashboard_id: Set(id.to_string()),
                        widget_type: Set(w.widget_type),
                        title: Set(w.title),
                        config_json: Set(config_str),
                        grid_x: Set(w.grid_x),
                        grid_y: Set(w.grid_y),
                        grid_w: Set(w.grid_w),
                        grid_h: Set(w.grid_h),
                        sort_order: Set(w.sort_order),
                        created_at: NotSet,
                    };
                    am.update(&txn).await?;
                } else {
                    // Insert new widget
                    let new_id = Uuid::new_v4().to_string();
                    new_ids.insert(new_id.clone());
                    let am = dashboard_widget::ActiveModel {
                        id: Set(new_id),
                        dashboard_id: Set(id.to_string()),
                        widget_type: Set(w.widget_type),
                        title: Set(w.title),
                        config_json: Set(config_str),
                        grid_x: Set(w.grid_x),
                        grid_y: Set(w.grid_y),
                        grid_w: Set(w.grid_w),
                        grid_h: Set(w.grid_h),
                        sort_order: Set(w.sort_order),
                        created_at: Set(now),
                    };
                    am.insert(&txn).await?;
                }
            }

            // Delete removed widgets
            let to_delete: Vec<String> = old_ids.difference(&new_ids).cloned().collect();
            if !to_delete.is_empty() {
                dashboard_widget::Entity::delete_many()
                    .filter(dashboard_widget::Column::Id.is_in(to_delete))
                    .exec(&txn)
                    .await?;
            }
        }

        txn.commit().await?;
        Self::get_with_widgets(db, id).await
    }

    /// Delete a dashboard. Cannot delete the default or the last remaining dashboard.
    pub async fn delete(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let dash = dashboard::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(AppError::NotFound("Dashboard not found".into()))?;

        if dash.is_default {
            return Err(AppError::BadRequest(
                "Cannot delete default dashboard. Set another as default first.".into(),
            ));
        }

        let count = dashboard::Entity::find().count(db).await?;
        if count <= 1 {
            return Err(AppError::BadRequest(
                "Cannot delete the last dashboard.".into(),
            ));
        }

        dashboard::Entity::delete_by_id(id).exec(db).await?;
        Ok(())
    }

    /// Create the default dashboard with preset widgets.
    async fn create_default(db: &DatabaseConnection) -> Result<DashboardWithWidgets, AppError> {
        let now = Utc::now().to_rfc3339();
        let dash_id = Uuid::new_v4().to_string();
        let dash = dashboard::ActiveModel {
            id: Set(dash_id.clone()),
            name: Set("Dashboard".to_string()),
            is_default: Set(true),
            sort_order: Set(0),
            created_at: Set(now.clone()),
            updated_at: Set(now.clone()),
        };
        let dash = dash.insert(db).await?;

        let presets = vec![
            ("stat-number", r#"{"metric":"server_count"}"#, 0, 0, 2, 2, 0),
            ("stat-number", r#"{"metric":"avg_cpu"}"#, 2, 0, 2, 2, 1),
            ("stat-number", r#"{"metric":"avg_memory"}"#, 4, 0, 2, 2, 2),
            (
                "stat-number",
                r#"{"metric":"total_bandwidth"}"#,
                6,
                0,
                2,
                2,
                3,
            ),
            ("stat-number", r#"{"metric":"health"}"#, 8, 0, 2, 2, 4),
            ("server-cards", r#"{"scope":"all"}"#, 0, 2, 12, 6, 5),
        ];

        let mut widgets = Vec::new();
        for (wtype, config, x, y, w, h, sort) in presets {
            let wm = dashboard_widget::ActiveModel {
                id: Set(Uuid::new_v4().to_string()),
                dashboard_id: Set(dash_id.clone()),
                widget_type: Set(wtype.to_string()),
                title: Set(None),
                config_json: Set(config.to_string()),
                grid_x: Set(x),
                grid_y: Set(y),
                grid_w: Set(w),
                grid_h: Set(h),
                sort_order: Set(sort),
                created_at: Set(now.clone()),
            };
            widgets.push(wm.insert(db).await?);
        }

        Ok(DashboardWithWidgets {
            dashboard: dash,
            widgets,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::setup_test_db;

    /// Enable foreign keys for test DB (needed for CASCADE behavior).
    async fn setup_db_with_fk() -> (DatabaseConnection, tempfile::TempDir) {
        let (db, tmp) = setup_test_db().await;
        db.execute_unprepared("PRAGMA foreign_keys=ON").await.unwrap();
        (db, tmp)
    }

    #[tokio::test]
    async fn test_create_first_is_default() {
        let (db, _tmp) = setup_db_with_fk().await;

        let dash = DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "First".to_string(),
            },
        )
        .await
        .unwrap();

        assert!(dash.is_default);
        assert_eq!(dash.name, "First");
    }

    #[tokio::test]
    async fn test_create_second_not_default() {
        let (db, _tmp) = setup_db_with_fk().await;

        DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "First".to_string(),
            },
        )
        .await
        .unwrap();

        let second = DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "Second".to_string(),
            },
        )
        .await
        .unwrap();

        assert!(!second.is_default);
        assert_eq!(second.name, "Second");
    }

    #[tokio::test]
    async fn test_get_default_auto_creates() {
        let (db, _tmp) = setup_db_with_fk().await;

        let result = DashboardService::get_default(&db).await.unwrap();
        assert!(result.dashboard.is_default);
        assert_eq!(result.dashboard.name, "Dashboard");
        // Should have 6 preset widgets
        assert_eq!(result.widgets.len(), 6);
    }

    #[tokio::test]
    async fn test_get_default_idempotent() {
        let (db, _tmp) = setup_db_with_fk().await;

        let first = DashboardService::get_default(&db).await.unwrap();
        let second = DashboardService::get_default(&db).await.unwrap();

        assert_eq!(first.dashboard.id, second.dashboard.id);
        assert_eq!(first.widgets.len(), second.widgets.len());
    }

    #[tokio::test]
    async fn test_update_widget_diff() {
        let (db, _tmp) = setup_db_with_fk().await;

        // Create a dashboard with one widget
        let dash = DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "Test".to_string(),
            },
        )
        .await
        .unwrap();

        let result = DashboardService::update(
            &db,
            &dash.id,
            UpdateDashboardInput {
                name: None,
                is_default: None,
                sort_order: None,
                widgets: Some(vec![
                    WidgetInput {
                        id: None,
                        widget_type: "gauge".to_string(),
                        title: Some("CPU".to_string()),
                        config_json: serde_json::json!({"metric": "cpu"}),
                        grid_x: 0,
                        grid_y: 0,
                        grid_w: 4,
                        grid_h: 3,
                        sort_order: 0,
                    },
                    WidgetInput {
                        id: None,
                        widget_type: "gauge".to_string(),
                        title: Some("Memory".to_string()),
                        config_json: serde_json::json!({"metric": "memory"}),
                        grid_x: 4,
                        grid_y: 0,
                        grid_w: 4,
                        grid_h: 3,
                        sort_order: 1,
                    },
                ]),
            },
        )
        .await
        .unwrap();

        assert_eq!(result.widgets.len(), 2);

        // Now update: keep first widget (update title), remove second, add new one
        let keep_id = result.widgets[0].id.clone();

        let result2 = DashboardService::update(
            &db,
            &dash.id,
            UpdateDashboardInput {
                name: None,
                is_default: None,
                sort_order: None,
                widgets: Some(vec![
                    WidgetInput {
                        id: Some(keep_id.clone()),
                        widget_type: "gauge".to_string(),
                        title: Some("CPU Updated".to_string()),
                        config_json: serde_json::json!({"metric": "cpu"}),
                        grid_x: 0,
                        grid_y: 0,
                        grid_w: 6,
                        grid_h: 3,
                        sort_order: 0,
                    },
                    WidgetInput {
                        id: None,
                        widget_type: "markdown".to_string(),
                        title: Some("Notes".to_string()),
                        config_json: serde_json::json!({"content": "hello"}),
                        grid_x: 6,
                        grid_y: 0,
                        grid_w: 6,
                        grid_h: 3,
                        sort_order: 1,
                    },
                ]),
            },
        )
        .await
        .unwrap();

        assert_eq!(result2.widgets.len(), 2);
        // The kept widget should retain its id and have updated title
        let kept = result2.widgets.iter().find(|w| w.id == keep_id).unwrap();
        assert_eq!(kept.title, Some("CPU Updated".to_string()));
        assert_eq!(kept.grid_w, 6);
        // The new widget should exist
        let new_widget = result2.widgets.iter().find(|w| w.id != keep_id).unwrap();
        assert_eq!(new_widget.widget_type, "markdown");
    }

    #[tokio::test]
    async fn test_update_set_default_clears_others() {
        let (db, _tmp) = setup_db_with_fk().await;

        let first = DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "First".to_string(),
            },
        )
        .await
        .unwrap();
        assert!(first.is_default);

        let second = DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "Second".to_string(),
            },
        )
        .await
        .unwrap();
        assert!(!second.is_default);

        // Set second as default
        DashboardService::update(
            &db,
            &second.id,
            UpdateDashboardInput {
                name: None,
                is_default: Some(true),
                sort_order: None,
                widgets: None,
            },
        )
        .await
        .unwrap();

        // First should no longer be default
        let first_reloaded = DashboardService::get_with_widgets(&db, &first.id)
            .await
            .unwrap();
        assert!(!first_reloaded.dashboard.is_default);

        let second_reloaded = DashboardService::get_with_widgets(&db, &second.id)
            .await
            .unwrap();
        assert!(second_reloaded.dashboard.is_default);
    }

    #[tokio::test]
    async fn test_update_unset_default_on_current_default_rejected() {
        let (db, _tmp) = setup_db_with_fk().await;

        let dash = DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "Only".to_string(),
            },
        )
        .await
        .unwrap();
        assert!(dash.is_default);

        let result = DashboardService::update(
            &db,
            &dash.id,
            UpdateDashboardInput {
                name: None,
                is_default: Some(false),
                sort_order: None,
                widgets: None,
            },
        )
        .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::BadRequest(msg) => {
                assert!(msg.contains("Cannot unset default"));
            }
            other => panic!("Expected BadRequest, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_update_unknown_widget_type_rejected() {
        let (db, _tmp) = setup_db_with_fk().await;

        let dash = DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "Test".to_string(),
            },
        )
        .await
        .unwrap();

        let result = DashboardService::update(
            &db,
            &dash.id,
            UpdateDashboardInput {
                name: None,
                is_default: None,
                sort_order: None,
                widgets: Some(vec![WidgetInput {
                    id: None,
                    widget_type: "nonexistent-widget".to_string(),
                    title: None,
                    config_json: serde_json::json!({}),
                    grid_x: 0,
                    grid_y: 0,
                    grid_w: 4,
                    grid_h: 3,
                    sort_order: 0,
                }]),
            },
        )
        .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::BadRequest(msg) => {
                assert!(msg.contains("Unknown widget_type"));
            }
            other => panic!("Expected BadRequest, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_delete_last_dashboard_rejected() {
        let (db, _tmp) = setup_db_with_fk().await;

        let dash = DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "Only".to_string(),
            },
        )
        .await
        .unwrap();

        // It's the default AND the last, both should block deletion.
        // Since it's the default, that check triggers first.
        let result = DashboardService::delete(&db, &dash.id).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::BadRequest(msg) => {
                assert!(
                    msg.contains("Cannot delete default") || msg.contains("Cannot delete the last")
                );
            }
            other => panic!("Expected BadRequest, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_delete_default_dashboard_rejected() {
        let (db, _tmp) = setup_db_with_fk().await;

        let first = DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "First".to_string(),
            },
        )
        .await
        .unwrap();
        assert!(first.is_default);

        // Create a second so "last dashboard" check doesn't trigger
        DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "Second".to_string(),
            },
        )
        .await
        .unwrap();

        let result = DashboardService::delete(&db, &first.id).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::BadRequest(msg) => {
                assert!(msg.contains("Cannot delete default"));
            }
            other => panic!("Expected BadRequest, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_delete_non_default_cascades_widgets() {
        let (db, _tmp) = setup_db_with_fk().await;

        // Create default dashboard
        DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "Default".to_string(),
            },
        )
        .await
        .unwrap();

        // Create second (non-default) dashboard with widgets
        let second = DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "Second".to_string(),
            },
        )
        .await
        .unwrap();

        // Add widgets to the second dashboard
        DashboardService::update(
            &db,
            &second.id,
            UpdateDashboardInput {
                name: None,
                is_default: None,
                sort_order: None,
                widgets: Some(vec![WidgetInput {
                    id: None,
                    widget_type: "gauge".to_string(),
                    title: Some("CPU".to_string()),
                    config_json: serde_json::json!({}),
                    grid_x: 0,
                    grid_y: 0,
                    grid_w: 4,
                    grid_h: 3,
                    sort_order: 0,
                }]),
            },
        )
        .await
        .unwrap();

        // Verify widget exists
        let widgets_before = dashboard_widget::Entity::find()
            .filter(dashboard_widget::Column::DashboardId.eq(&second.id))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(widgets_before.len(), 1);

        // Delete the non-default dashboard
        DashboardService::delete(&db, &second.id).await.unwrap();

        // Widgets should be cascade-deleted
        let widgets_after = dashboard_widget::Entity::find()
            .filter(dashboard_widget::Column::DashboardId.eq(&second.id))
            .all(&db)
            .await
            .unwrap();
        assert!(widgets_after.is_empty());
    }

    #[tokio::test]
    async fn test_list_ordered_by_sort_order() {
        let (db, _tmp) = setup_db_with_fk().await;

        let a = DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "A".to_string(),
            },
        )
        .await
        .unwrap();

        let b = DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "B".to_string(),
            },
        )
        .await
        .unwrap();

        let c = DashboardService::create(
            &db,
            CreateDashboardInput {
                name: "C".to_string(),
            },
        )
        .await
        .unwrap();

        // Set sort_order: C=0, A=1, B=2
        DashboardService::update(
            &db,
            &c.id,
            UpdateDashboardInput {
                name: None,
                is_default: None,
                sort_order: Some(0),
                widgets: None,
            },
        )
        .await
        .unwrap();

        DashboardService::update(
            &db,
            &a.id,
            UpdateDashboardInput {
                name: None,
                is_default: None,
                sort_order: Some(1),
                widgets: None,
            },
        )
        .await
        .unwrap();

        DashboardService::update(
            &db,
            &b.id,
            UpdateDashboardInput {
                name: None,
                is_default: None,
                sort_order: Some(2),
                widgets: None,
            },
        )
        .await
        .unwrap();

        let list = DashboardService::list(&db).await.unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "C");
        assert_eq!(list[1].name, "A");
        assert_eq!(list[2].name, "B");
    }
}
