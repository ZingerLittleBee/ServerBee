use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use serde::Serialize;
use utoipa::ToSchema;

use super::{WidgetModuleError, package::UnpackedPackage};
use crate::entity::widget_module::{self, Entity as WidgetModuleEntity};

#[derive(Debug, Clone)]
pub enum InstalledFrom {
    Url(String),
    Upload(String),
}

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

        if requested.contains("..") {
            return Err(WidgetModuleError::InvalidAssetPath);
        }

        if matches!(
            row.source_type,
            crate::entity::widget_module::SourceType::Builtin
        ) {
            // Builtin: the URL `/api/widget-modules/<id>/<requested>` resolves to a path
            // within the embedded directory. `row.entry_path` is the module's main file
            // (e.g. "hello-world/index.js"); we resolve `requested` relative to its folder.
            let folder = row
                .entry_path
                .rsplit_once('/')
                .map(|(d, _)| d)
                .unwrap_or("");
            let full = if folder.is_empty() {
                requested.to_string()
            } else {
                format!("{folder}/{requested}")
            };
            let bytes = crate::service::widget_module::builtin::builtin_asset_bytes(&full)
                .ok_or(WidgetModuleError::InvalidAssetPath)?;
            return Ok((bytes, mime_for(&full)));
        }

        let blob = row
            .package_blob
            .ok_or_else(|| WidgetModuleError::NotFound(format!("{id}: no blob")))?;

        let is_zip = blob.starts_with(b"PK\x03\x04");
        let package = if is_zip {
            UnpackedPackage::from_zip(&blob)?
        } else {
            UnpackedPackage::from_single_file(&row.entry_path, blob)
        };

        // For zip collections, asset requests are resolved relative to the
        // entry's folder inside the zip (mirrors Builtin behavior). For single
        // files there is no folder so we look up the requested path verbatim.
        let lookup_path = if is_zip {
            let folder = row
                .entry_path
                .rsplit_once('/')
                .map(|(d, _)| d)
                .unwrap_or("");
            if folder.is_empty() {
                requested.to_string()
            } else {
                format!("{folder}/{requested}")
            }
        } else {
            requested.to_string()
        };

        let bytes = package
            .get(&lookup_path)
            .ok_or(WidgetModuleError::InvalidAssetPath)?
            .to_vec();
        let mime = mime_for(&lookup_path);
        Ok((bytes, mime))
    }

    /// Install (or upgrade) a single-file JS widget module by extracting the
    /// JSDoc manifest, computing a sha256 fingerprint, and upserting the row.
    pub async fn install_single_file(
        db: &DatabaseConnection,
        code: Vec<u8>,
        from: InstalledFrom,
        installed_by: Option<i64>,
    ) -> Result<crate::entity::widget_module::Model, WidgetModuleError> {
        use chrono::Utc;
        use sea_orm::sea_query::OnConflict;
        use sea_orm::{ActiveValue::Set, EntityTrait};
        use sha2::{Digest, Sha256};

        use crate::entity::widget_module::{self, SourceType};

        let source = std::str::from_utf8(&code).map_err(|e| {
            WidgetModuleError::ManifestValidation(format!("not utf-8: {e}"))
        })?;
        let manifest = super::extractor::extract_manifest(source)?;

        let sha = {
            let mut h = Sha256::new();
            h.update(&code);
            format!("{:x}", h.finalize())
        };

        let (source_type, source_url) = match from {
            InstalledFrom::Url(u) => (SourceType::Url, Some(u)),
            InstalledFrom::Upload(name) => (SourceType::Upload, Some(name)),
        };

        let manifest_json = serde_json::to_string(&manifest).map_err(|e| {
            WidgetModuleError::ManifestValidation(format!("serialize manifest: {e}"))
        })?;
        let id_clone = manifest.id.clone();

        let active = widget_module::ActiveModel {
            id: Set(manifest.id.clone()),
            version: Set(manifest.version.clone()),
            source_type: Set(source_type),
            source_url: Set(source_url),
            bundled_by_theme_id: Set(None),
            manifest_json: Set(manifest_json),
            code_sha256: Set(sha),
            entry_path: Set("index.js".into()),
            package_blob: Set(Some(code)),
            installed_by: Set(installed_by),
            installed_at: Set(Utc::now()),
            enabled: Set(true),
        };

        widget_module::Entity::insert(active)
            .on_conflict(
                OnConflict::column(widget_module::Column::Id)
                    .update_columns([
                        widget_module::Column::Version,
                        widget_module::Column::SourceType,
                        widget_module::Column::SourceUrl,
                        widget_module::Column::ManifestJson,
                        widget_module::Column::CodeSha256,
                        widget_module::Column::PackageBlob,
                        widget_module::Column::InstalledBy,
                        widget_module::Column::InstalledAt,
                        widget_module::Column::Enabled,
                    ])
                    .to_owned(),
            )
            .exec(db)
            .await?;

        Self::get(db, &id_clone).await
    }

    /// Install (or upgrade) a collection of widget modules packaged as a zip
    /// bundle. The zip must contain a top-level `collection.json` listing one
    /// or more entries; each entry must point to a `.js` (or `.mjs`) file with
    /// an `@serverbee-widget` JSDoc manifest. All widgets in the collection
    /// share the same blob storage.
    pub async fn install_collection_from_zip(
        db: &DatabaseConnection,
        blob: Vec<u8>,
        from: InstalledFrom,
        installed_by: Option<i64>,
    ) -> Result<Vec<crate::entity::widget_module::Model>, WidgetModuleError> {
        use std::collections::HashSet;

        use chrono::Utc;
        use sea_orm::sea_query::OnConflict;
        use sea_orm::{ActiveValue::Set, EntityTrait};
        use sha2::{Digest, Sha256};

        use crate::entity::widget_module::{self, SourceType};

        let package = UnpackedPackage::from_zip(&blob)?;

        let manifest_bytes = package.get("collection.json").ok_or_else(|| {
            WidgetModuleError::ManifestExtraction("missing collection.json".into())
        })?;
        let manifest_str = std::str::from_utf8(manifest_bytes).map_err(|e| {
            WidgetModuleError::ManifestExtraction(format!("collection.json not utf-8: {e}"))
        })?;

        #[derive(serde::Deserialize)]
        struct CollectionEntry {
            entry: String,
        }
        #[derive(serde::Deserialize)]
        struct CollectionManifest {
            widgets: Vec<CollectionEntry>,
        }

        let collection: CollectionManifest = serde_json::from_str(manifest_str)
            .map_err(|e| {
                WidgetModuleError::ManifestExtraction(format!("collection.json invalid: {e}"))
            })?;

        if collection.widgets.is_empty() {
            return Err(WidgetModuleError::ManifestValidation(
                "collection.json: widgets array must not be empty".into(),
            ));
        }

        // Validate every entry up front before touching the database.
        let mut prepared: Vec<(
            super::extractor::WidgetManifest,
            String, // entry path inside the zip
            String, // sha256 of this entry's bytes
        )> = Vec::with_capacity(collection.widgets.len());
        let mut seen_ids: HashSet<String> = HashSet::new();

        for entry in collection.widgets.iter() {
            let entry_path = entry.entry.trim_start_matches('/').to_string();
            if entry_path.is_empty()
                || entry_path.contains("..")
                || entry_path.starts_with('/')
            {
                return Err(WidgetModuleError::ManifestValidation(format!(
                    "invalid entry path: {entry_path}"
                )));
            }
            let lower = entry_path.to_ascii_lowercase();
            if !(lower.ends_with(".js") || lower.ends_with(".mjs")) {
                return Err(WidgetModuleError::ManifestValidation(format!(
                    "entry must be .js or .mjs: {entry_path}"
                )));
            }

            let bytes = package.get(&entry_path).ok_or_else(|| {
                WidgetModuleError::ManifestExtraction(format!(
                    "entry not found in zip: {entry_path}"
                ))
            })?;
            let source = std::str::from_utf8(bytes).map_err(|e| {
                WidgetModuleError::ManifestExtraction(format!(
                    "entry {entry_path} not utf-8: {e}"
                ))
            })?;
            let manifest = super::extractor::extract_manifest(source)?;

            if !seen_ids.insert(manifest.id.clone()) {
                return Err(WidgetModuleError::ManifestValidation(format!(
                    "duplicate widget id in collection: {}",
                    manifest.id
                )));
            }

            let sha = {
                let mut h = Sha256::new();
                h.update(bytes);
                format!("{:x}", h.finalize())
            };

            prepared.push((manifest, entry_path, sha));
        }

        let (source_type, source_url) = match from {
            InstalledFrom::Url(u) => (SourceType::Url, Some(u)),
            InstalledFrom::Upload(name) => (SourceType::Upload, Some(name)),
        };

        let now = Utc::now();
        let mut installed: Vec<widget_module::Model> = Vec::with_capacity(prepared.len());

        for (manifest, entry_path, sha) in prepared.into_iter() {
            let manifest_json = serde_json::to_string(&manifest).map_err(|e| {
                WidgetModuleError::ManifestValidation(format!("serialize manifest: {e}"))
            })?;
            let id_clone = manifest.id.clone();
            let version_clone = manifest.version.clone();

            let active = widget_module::ActiveModel {
                id: Set(manifest.id),
                version: Set(version_clone),
                source_type: Set(source_type.clone()),
                source_url: Set(source_url.clone()),
                bundled_by_theme_id: Set(None),
                manifest_json: Set(manifest_json),
                code_sha256: Set(sha),
                entry_path: Set(entry_path),
                package_blob: Set(Some(blob.clone())),
                installed_by: Set(installed_by),
                installed_at: Set(now),
                enabled: Set(true),
            };

            widget_module::Entity::insert(active)
                .on_conflict(
                    OnConflict::column(widget_module::Column::Id)
                        .update_columns([
                            widget_module::Column::Version,
                            widget_module::Column::SourceType,
                            widget_module::Column::SourceUrl,
                            widget_module::Column::ManifestJson,
                            widget_module::Column::CodeSha256,
                            widget_module::Column::EntryPath,
                            widget_module::Column::PackageBlob,
                            widget_module::Column::InstalledBy,
                            widget_module::Column::InstalledAt,
                            widget_module::Column::Enabled,
                        ])
                        .to_owned(),
                )
                .exec(db)
                .await?;

            installed.push(Self::get(db, &id_clone).await?);
        }

        Ok(installed)
    }

    /// Delete a widget module row. Refuses to delete Builtin rows.
    pub async fn uninstall(db: &DatabaseConnection, id: &str) -> Result<(), WidgetModuleError> {
        let row = Self::get(db, id).await?;
        if matches!(
            row.source_type,
            crate::entity::widget_module::SourceType::Builtin
        ) {
            return Err(WidgetModuleError::ManifestValidation(
                "cannot uninstall builtin widget".into(),
            ));
        }
        crate::entity::widget_module::Entity::delete_by_id(id.to_string())
            .exec(db)
            .await?;
        Ok(())
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
