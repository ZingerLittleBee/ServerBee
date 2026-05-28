use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use serde::Serialize;
use utoipa::ToSchema;

use super::{WidgetModuleError, package::UnpackedPackage};
use crate::entity::widget_module::{self, Entity as WidgetModuleEntity};

#[derive(Debug, Serialize, ToSchema)]
pub struct WidgetModuleListEntry {
    pub id: String,
    pub version: String,
    pub source_type: String,
    pub entry_path: String,
    pub code_sha256: String,
    #[schema(value_type = Object)]
    pub manifest: serde_json::Value,
    pub enabled: bool,
}

pub struct WidgetModuleService;

impl WidgetModuleService {
    pub async fn list(
        db: &DatabaseConnection,
    ) -> Result<Vec<WidgetModuleListEntry>, WidgetModuleError> {
        let rows = WidgetModuleEntity::find()
            .filter(widget_module::Column::Enabled.eq(true))
            .order_by_asc(widget_module::Column::Id)
            .all(db)
            .await?;
        rows.into_iter()
            .map(|r| {
                let manifest: serde_json::Value =
                    serde_json::from_str(&r.manifest_json).map_err(|e| {
                        WidgetModuleError::ManifestValidation(format!(
                            "stored manifest invalid: {e}"
                        ))
                    })?;
                Ok(WidgetModuleListEntry {
                    id: r.id,
                    version: r.version,
                    source_type: format!("{:?}", r.source_type),
                    entry_path: r.entry_path,
                    code_sha256: r.code_sha256,
                    manifest,
                    enabled: r.enabled,
                })
            })
            .collect()
    }

    pub async fn get(
        db: &DatabaseConnection,
        id: &str,
    ) -> Result<widget_module::Model, WidgetModuleError> {
        WidgetModuleEntity::find_by_id(id.to_string())
            .one(db)
            .await?
            .ok_or_else(|| WidgetModuleError::NotFound(id.to_string()))
    }

    /// Loads the package and returns bytes + mime for a requested asset path.
    pub async fn serve_asset(
        db: &DatabaseConnection,
        id: &str,
        requested: &str,
    ) -> Result<(Vec<u8>, String), WidgetModuleError> {
        let row = Self::get(db, id).await?;
        let blob = row
            .package_blob
            .ok_or_else(|| WidgetModuleError::NotFound(format!("{id}: no blob")))?;

        let package = if blob.starts_with(b"PK\x03\x04") {
            UnpackedPackage::from_zip(&blob)?
        } else {
            UnpackedPackage::from_single_file(&row.entry_path, blob)
        };

        let bytes = package
            .get(requested)
            .ok_or(WidgetModuleError::InvalidAssetPath)?
            .to_vec();
        let mime = mime_for(requested);
        Ok((bytes, mime))
    }
}

fn mime_for(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "text/javascript; charset=utf-8"
    } else if lower.ends_with(".json") {
        "application/json"
    } else if lower.ends_with(".css") {
        "text/css"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "application/octet-stream"
    }
    .to_string()
}
