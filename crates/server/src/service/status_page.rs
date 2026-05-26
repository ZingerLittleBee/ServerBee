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
/// without re-sending the entire config. Fields that map 1:1 to the
/// entity model use the same name.
#[derive(Debug, Default, Deserialize, Serialize, utoipa::ToSchema)]
pub struct UpdateStatusPage {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub server_ids_json: Option<Vec<String>>,
    pub group_by_server_group: Option<bool>,
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
        if let Some(server_ids) = input.server_ids_json {
            let json = serde_json::to_string(&server_ids)
                .map_err(|e| AppError::Validation(format!("Invalid server_ids_json: {e}")))?;
            model.server_ids_json = Set(json);
        }
        if let Some(group_by) = input.group_by_server_group {
            model.group_by_server_group = Set(group_by);
        }
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
