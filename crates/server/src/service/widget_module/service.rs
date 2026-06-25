use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use serde::Serialize;
use utoipa::ToSchema;

use super::{WidgetModuleError, package::UnpackedPackage};
use crate::entity::widget_module::{self, Entity as WidgetModuleEntity, SourceType};

/// Asset bytes plus the per-row metadata callers need to set caching headers
/// without re-querying the database.
pub struct ServedAsset {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub version: String,
    pub code_sha256: String,
}

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

    /// Loads the package and returns bytes + mime + ETag inputs for a requested
    /// asset path. The route layer wraps these into HTTP cache headers without
    /// needing a second database round-trip.
    pub async fn serve_asset(
        db: &DatabaseConnection,
        id: &str,
        requested: &str,
    ) -> Result<ServedAsset, WidgetModuleError> {
        let row = Self::get(db, id).await?;

        if requested.contains("..") {
            return Err(WidgetModuleError::InvalidAssetPath);
        }

        let version = row.version.clone();
        let code_sha256 = row.code_sha256.clone();

        if matches!(row.source_type, SourceType::Builtin) {
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
                .ok_or_else(|| WidgetModuleError::AssetNotFound(requested.to_string()))?;
            return Ok(ServedAsset {
                bytes,
                mime: mime_for(&full),
                version,
                code_sha256,
            });
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
            .ok_or_else(|| WidgetModuleError::AssetNotFound(requested.to_string()))?
            .to_vec();
        let mime = mime_for(&lookup_path);
        Ok(ServedAsset {
            bytes,
            mime,
            version,
            code_sha256,
        })
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

        // Reject if an existing row owns this id under a different source_type.
        // Spec §3.5: a user-uploaded module must never silently overwrite a
        // builtin (or vice versa) — those are separate trust domains.
        if let Some(existing) =
            widget_module::Entity::find_by_id(manifest.id.clone()).one(db).await?
            && existing.source_type != source_type
        {
            return Err(WidgetModuleError::IdConflict(format!(
                "id {} already installed as {:?}",
                manifest.id, existing.source_type
            )));
        }

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

        // Reject the whole collection up front if any id collides with an
        // existing row owned by a different source_type (e.g. Builtin).
        for (manifest, _, _) in prepared.iter() {
            if let Some(existing) =
                widget_module::Entity::find_by_id(manifest.id.clone()).one(db).await?
                && existing.source_type != source_type
            {
                return Err(WidgetModuleError::IdConflict(format!(
                    "id {} already installed as {:?}",
                    manifest.id, existing.source_type
                )));
            }
        }

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
    } else if lower.ends_with(".wasm") {
        "application/wasm"
    } else if lower.ends_with(".html") || lower.ends_with(".htm") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".txt") {
        "text/plain; charset=utf-8"
    } else if lower.ends_with(".md") {
        "text/markdown; charset=utf-8"
    } else {
        "application/octet-stream"
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use chrono::{TimeZone, Utc};
    use sea_orm::ActiveModelTrait;
    use sea_orm::ActiveValue::Set;
    use serde_json::json;

    use super::*;
    use crate::test_utils::setup_test_db;

    /// Build a single-file widget JS source with the given id baked into the
    /// JSDoc manifest so the extractor accepts it.
    fn widget_js(id: &str) -> String {
        format!(
            r#"/**
 * @serverbee-widget {{
 *   "id": "{id}",
 *   "version": "1.0.0",
 *   "name": "Test {id}",
 *   "category": "Real-time",
 *   "sizing": {{ "defaultW": 3, "defaultH": 3, "minW": 2, "minH": 2, "strategy": "aspect-square" }},
 *   "sdkVersion": "^0.1.0"
 * }}
 */
export default {{ id: "{id}" }};
"#
        )
    }

    /// A fixed installed_at timestamp so seeded rows are deterministic.
    fn fixed_ts() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap()
    }

    /// Insert a widget_module row directly with explicit fields for branch
    /// coverage of the read paths.
    async fn seed_row(
        db: &DatabaseConnection,
        id: &str,
        source_type: SourceType,
        entry_path: &str,
        manifest_json: &str,
        blob: Option<Vec<u8>>,
        enabled: bool,
    ) -> widget_module::Model {
        widget_module::ActiveModel {
            id: Set(id.to_string()),
            version: Set("1.0.0".into()),
            source_type: Set(source_type),
            source_url: Set(None),
            bundled_by_theme_id: Set(None),
            manifest_json: Set(manifest_json.to_string()),
            code_sha256: Set("deadbeef".into()),
            entry_path: Set(entry_path.to_string()),
            package_blob: Set(blob),
            installed_by: Set(None),
            installed_at: Set(fixed_ts()),
            enabled: Set(enabled),
        }
        .insert(db)
        .await
        .expect("seed widget_module row")
    }

    /// Build an in-memory zip from `(path, bytes)` pairs.
    fn build_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        use zip::write::SimpleFileOptions;

        let mut buf: Vec<u8> = Vec::new();
        {
            let cursor = std::io::Cursor::new(&mut buf);
            let mut zw = zip::ZipWriter::new(cursor);
            let opts = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, bytes) in files {
                zw.start_file(*name, opts).unwrap();
                zw.write_all(bytes).unwrap();
            }
            zw.finish().unwrap();
        }
        buf
    }

    /// Build a valid collection zip from `(folder, id)` pairs plus an embedded
    /// collection.json listing every entry.
    fn build_collection_zip(widgets: &[(&str, &str)]) -> Vec<u8> {
        let entries: Vec<serde_json::Value> = widgets
            .iter()
            .map(|(folder, _)| json!({ "entry": format!("{folder}/index.js") }))
            .collect();
        let manifest = json!({ "widgets": entries });
        let manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let mut owned: Vec<(String, Vec<u8>)> =
            vec![("collection.json".to_string(), manifest_bytes)];
        for (folder, id) in widgets {
            owned.push((format!("{folder}/index.js"), widget_js(id).into_bytes()));
        }
        let refs: Vec<(&str, &[u8])> = owned
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_slice()))
            .collect();
        build_zip(&refs)
    }

    // ---------- list ----------

    /// list returns enabled rows parsed into entries, ordered by id ascending.
    #[tokio::test]
    async fn list_returns_enabled_rows_sorted() {
        let (db, _tmp) = setup_test_db().await;
        seed_row(&db, "b.id", SourceType::Upload, "index.js", "{}", None, true).await;
        seed_row(&db, "a.id", SourceType::Url, "index.js", "{}", None, true).await;

        let entries = WidgetModuleService::list(&db).await.unwrap();
        assert_eq!(entries.len(), 2);
        // order_by_asc(Id) places "a.id" before "b.id".
        assert_eq!(entries[0].id, "a.id");
        assert_eq!(entries[1].id, "b.id");
        // source_type is rendered via Debug formatting of the enum.
        assert_eq!(entries[0].source_type, "Url");
        assert_eq!(entries[1].source_type, "Upload");
        assert!(entries[0].enabled);
    }

    /// list filters out disabled rows.
    #[tokio::test]
    async fn list_excludes_disabled_rows() {
        let (db, _tmp) = setup_test_db().await;
        seed_row(&db, "on.id", SourceType::Upload, "index.js", "{}", None, true).await;
        seed_row(&db, "off.id", SourceType::Upload, "index.js", "{}", None, false).await;

        let entries = WidgetModuleService::list(&db).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "on.id");
    }

    /// list returns an empty vec when there are no enabled rows.
    #[tokio::test]
    async fn list_empty_when_no_rows() {
        let (db, _tmp) = setup_test_db().await;
        let entries = WidgetModuleService::list(&db).await.unwrap();
        assert!(entries.is_empty());
    }

    /// list maps a corrupt stored manifest_json to ManifestValidation.
    #[tokio::test]
    async fn list_rejects_invalid_stored_manifest() {
        let (db, _tmp) = setup_test_db().await;
        // "{" is not valid JSON, so serde_json::from_str fails inside list().
        seed_row(&db, "bad.id", SourceType::Upload, "index.js", "{", None, true).await;

        let err = WidgetModuleService::list(&db)
            .await
            .err()
            .expect("list should fail on corrupt manifest");
        assert!(matches!(err, WidgetModuleError::ManifestValidation(_)));
        assert!(err.to_string().contains("stored manifest invalid"));
    }

    // ---------- get ----------

    /// get returns the row when it exists.
    #[tokio::test]
    async fn get_returns_existing_row() {
        let (db, _tmp) = setup_test_db().await;
        seed_row(&db, "x.id", SourceType::Upload, "index.js", "{}", None, true).await;

        let row = WidgetModuleService::get(&db, "x.id").await.unwrap();
        assert_eq!(row.id, "x.id");
        assert_eq!(row.installed_at, fixed_ts());
    }

    /// get maps a missing id to NotFound with the id in the message.
    #[tokio::test]
    async fn get_missing_is_not_found() {
        let (db, _tmp) = setup_test_db().await;
        let err = WidgetModuleService::get(&db, "absent")
            .await
            .err()
            .expect("get should fail for missing id");
        assert!(matches!(err, WidgetModuleError::NotFound(ref m) if m == "absent"));
    }

    // ---------- serve_asset ----------

    /// serve_asset surfaces NotFound when the module row does not exist.
    #[tokio::test]
    async fn serve_asset_missing_module_is_not_found() {
        let (db, _tmp) = setup_test_db().await;
        let err = WidgetModuleService::serve_asset(&db, "nope", "index.js")
            .await
            .err()
            .expect("serve_asset should fail for missing module");
        assert!(matches!(err, WidgetModuleError::NotFound(_)));
    }

    /// serve_asset rejects a requested path containing `..`.
    #[tokio::test]
    async fn serve_asset_rejects_dotdot_path() {
        let (db, _tmp) = setup_test_db().await;
        let blob = widget_js("z.id").into_bytes();
        seed_row(&db, "z.id", SourceType::Upload, "index.js", "{}", Some(blob), true).await;

        let err = WidgetModuleService::serve_asset(&db, "z.id", "../secret")
            .await
            .err()
            .expect("path traversal must be rejected");
        assert!(matches!(err, WidgetModuleError::InvalidAssetPath));
    }

    /// serve_asset on a single-file (non-zip) blob returns the verbatim bytes
    /// and the js mime type.
    #[tokio::test]
    async fn serve_asset_single_file_returns_bytes() {
        let (db, _tmp) = setup_test_db().await;
        let code = widget_js("single.id");
        let blob = code.clone().into_bytes();
        seed_row(&db, "single.id", SourceType::Upload, "index.js", "{}", Some(blob), true).await;

        let served = WidgetModuleService::serve_asset(&db, "single.id", "index.js")
            .await
            .unwrap();
        assert_eq!(served.bytes, code.into_bytes());
        assert!(served.mime.contains("javascript"));
        assert_eq!(served.version, "1.0.0");
        assert_eq!(served.code_sha256, "deadbeef");
    }

    /// serve_asset on a single-file blob maps an unknown path to AssetNotFound.
    #[tokio::test]
    async fn serve_asset_single_file_unknown_path_is_asset_not_found() {
        let (db, _tmp) = setup_test_db().await;
        let blob = widget_js("single2.id").into_bytes();
        seed_row(&db, "single2.id", SourceType::Upload, "index.js", "{}", Some(blob), true).await;

        // Single-file lookup is verbatim; "other.js" != entry_path "index.js".
        let err = WidgetModuleService::serve_asset(&db, "single2.id", "other.js")
            .await
            .err()
            .expect("unknown asset path should fail");
        assert!(matches!(err, WidgetModuleError::AssetNotFound(ref p) if p == "other.js"));
    }

    /// serve_asset returns NotFound when a non-builtin row has no blob stored.
    #[tokio::test]
    async fn serve_asset_no_blob_is_not_found() {
        let (db, _tmp) = setup_test_db().await;
        seed_row(&db, "noblob.id", SourceType::Url, "index.js", "{}", None, true).await;

        let err = WidgetModuleService::serve_asset(&db, "noblob.id", "index.js")
            .await
            .err()
            .expect("missing blob should fail");
        assert!(matches!(err, WidgetModuleError::NotFound(ref m) if m.contains("no blob")));
    }

    /// serve_asset on a zip blob resolves the request relative to the entry's
    /// folder and returns the sibling asset bytes.
    #[tokio::test]
    async fn serve_asset_zip_resolves_folder_relative() {
        let (db, _tmp) = setup_test_db().await;
        let zip = build_zip(&[
            ("weather/index.js", widget_js("zip.id").as_bytes()),
            ("weather/style.css", b".a{color:red}"),
        ]);
        // entry_path "weather/index.js" means requests resolve under "weather/".
        seed_row(&db, "zip.id", SourceType::Upload, "weather/index.js", "{}", Some(zip), true).await;

        let served = WidgetModuleService::serve_asset(&db, "zip.id", "style.css")
            .await
            .unwrap();
        assert_eq!(served.bytes, b".a{color:red}");
        assert_eq!(served.mime, "text/css");
    }

    /// serve_asset on a zip blob maps a missing sibling asset to AssetNotFound.
    #[tokio::test]
    async fn serve_asset_zip_missing_asset_is_asset_not_found() {
        let (db, _tmp) = setup_test_db().await;
        let zip = build_zip(&[("weather/index.js", widget_js("zip2.id").as_bytes())]);
        seed_row(&db, "zip2.id", SourceType::Upload, "weather/index.js", "{}", Some(zip), true).await;

        let err = WidgetModuleService::serve_asset(&db, "zip2.id", "missing.css")
            .await
            .err()
            .expect("missing zip asset should fail");
        assert!(matches!(err, WidgetModuleError::AssetNotFound(ref p) if p == "missing.css"));
    }

    /// serve_asset on a zip whose entry sits at the root resolves the request
    /// verbatim (no folder prefix).
    #[tokio::test]
    async fn serve_asset_zip_root_entry_no_folder_prefix() {
        let (db, _tmp) = setup_test_db().await;
        let zip = build_zip(&[
            ("index.js", widget_js("ziproot.id").as_bytes()),
            ("logo.svg", b"<svg/>"),
        ]);
        // entry_path "index.js" has no folder, so lookup is verbatim.
        seed_row(&db, "ziproot.id", SourceType::Upload, "index.js", "{}", Some(zip), true).await;

        let served = WidgetModuleService::serve_asset(&db, "ziproot.id", "logo.svg")
            .await
            .unwrap();
        assert_eq!(served.bytes, b"<svg/>");
        assert_eq!(served.mime, "image/svg+xml");
    }

    /// serve_asset for a Builtin row whose embedded asset is absent maps to
    /// AssetNotFound (no embedded build artifact required).
    #[tokio::test]
    async fn serve_asset_builtin_missing_embedded_is_asset_not_found() {
        let (db, _tmp) = setup_test_db().await;
        // A bogus builtin entry_path: builtin_asset_bytes will return None.
        seed_row(
            &db,
            "builtin.id",
            SourceType::Builtin,
            "definitely-not-real/index.js",
            "{}",
            None,
            true,
        )
        .await;

        let err = WidgetModuleService::serve_asset(&db, "builtin.id", "nope.js")
            .await
            .err()
            .expect("absent embedded builtin asset should fail");
        assert!(matches!(err, WidgetModuleError::AssetNotFound(ref p) if p == "nope.js"));
    }

    // ---------- install_single_file ----------

    /// install_single_file (Upload) inserts a new row and returns the model.
    #[tokio::test]
    async fn install_single_file_upload_inserts_row() {
        let (db, _tmp) = setup_test_db().await;
        let code = widget_js("com.test.up").into_bytes();

        let model = WidgetModuleService::install_single_file(
            &db,
            code.clone(),
            InstalledFrom::Upload("up.js".into()),
            Some(42),
        )
        .await
        .unwrap();

        assert_eq!(model.id, "com.test.up");
        assert_eq!(model.version, "1.0.0");
        assert!(matches!(model.source_type, SourceType::Upload));
        assert_eq!(model.source_url.as_deref(), Some("up.js"));
        assert_eq!(model.entry_path, "index.js");
        assert_eq!(model.installed_by, Some(42));
        assert_eq!(model.package_blob.as_deref(), Some(code.as_slice()));
        // Persisted in the DB.
        let count = WidgetModuleEntity::find().all(&db).await.unwrap().len();
        assert_eq!(count, 1);
    }

    /// install_single_file (Url) records the Url source_type and the url value.
    #[tokio::test]
    async fn install_single_file_url_records_source_url() {
        let (db, _tmp) = setup_test_db().await;
        let code = widget_js("com.test.url").into_bytes();

        let model = WidgetModuleService::install_single_file(
            &db,
            code,
            InstalledFrom::Url("https://example.com/w.js".into()),
            None,
        )
        .await
        .unwrap();
        assert!(matches!(model.source_type, SourceType::Url));
        assert_eq!(model.source_url.as_deref(), Some("https://example.com/w.js"));
        assert_eq!(model.installed_by, None);
    }

    /// install_single_file rejects non-utf8 bytes with ManifestValidation.
    #[tokio::test]
    async fn install_single_file_rejects_non_utf8() {
        let (db, _tmp) = setup_test_db().await;
        // 0xff is never valid UTF-8.
        let err = WidgetModuleService::install_single_file(
            &db,
            vec![0xff, 0xfe, 0xfd],
            InstalledFrom::Upload("bin.js".into()),
            None,
        )
        .await
        .err()
        .expect("non-utf8 should fail");
        assert!(matches!(err, WidgetModuleError::ManifestValidation(ref m) if m.contains("not utf-8")));
    }

    /// install_single_file propagates extractor errors when no manifest block.
    #[tokio::test]
    async fn install_single_file_rejects_missing_manifest() {
        let (db, _tmp) = setup_test_db().await;
        let err = WidgetModuleService::install_single_file(
            &db,
            b"export default {};".to_vec(),
            InstalledFrom::Upload("bad.js".into()),
            None,
        )
        .await
        .err()
        .expect("missing manifest should fail");
        assert!(matches!(err, WidgetModuleError::ManifestExtraction(_)));
    }

    /// install_single_file upgrades an existing same-source row in place.
    #[tokio::test]
    async fn install_single_file_upgrades_same_source() {
        let (db, _tmp) = setup_test_db().await;
        let v1 = widget_js("com.test.upg").into_bytes();
        WidgetModuleService::install_single_file(
            &db,
            v1,
            InstalledFrom::Upload("v1.js".into()),
            None,
        )
        .await
        .unwrap();

        // Same id, bumped version.
        let v2 = widget_js("com.test.upg").replace("1.0.0", "2.0.0").into_bytes();
        let model = WidgetModuleService::install_single_file(
            &db,
            v2,
            InstalledFrom::Upload("v2.js".into()),
            None,
        )
        .await
        .unwrap();
        assert_eq!(model.version, "2.0.0");
        // Still exactly one row (upsert, not insert).
        let count = WidgetModuleEntity::find().all(&db).await.unwrap().len();
        assert_eq!(count, 1);
    }

    /// install_single_file refuses to overwrite a row owned by a different
    /// source_type (IdConflict).
    #[tokio::test]
    async fn install_single_file_rejects_cross_source_conflict() {
        let (db, _tmp) = setup_test_db().await;
        // Pre-existing Builtin row with the same id the upload declares.
        seed_row(
            &db,
            "com.test.clash",
            SourceType::Builtin,
            "com.test.clash/index.js",
            "{}",
            None,
            true,
        )
        .await;

        let code = widget_js("com.test.clash").into_bytes();
        let err = WidgetModuleService::install_single_file(
            &db,
            code,
            InstalledFrom::Upload("clash.js".into()),
            None,
        )
        .await
        .err()
        .expect("cross-source-type install should conflict");
        assert!(matches!(err, WidgetModuleError::IdConflict(ref m) if m.contains("Builtin")));
    }

    // ---------- install_collection_from_zip ----------

    /// install_collection_from_zip installs every listed widget and returns
    /// the inserted models.
    #[tokio::test]
    async fn install_collection_inserts_all_widgets() {
        let (db, _tmp) = setup_test_db().await;
        let zip = build_collection_zip(&[("a", "com.c.a"), ("b", "com.c.b")]);

        let models = WidgetModuleService::install_collection_from_zip(
            &db,
            zip,
            InstalledFrom::Upload("pack.zip".into()),
            Some(7),
        )
        .await
        .unwrap();
        assert_eq!(models.len(), 2);
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"com.c.a"));
        assert!(ids.contains(&"com.c.b"));
        // Each row stores its own entry_path under its folder.
        let a = models.iter().find(|m| m.id == "com.c.a").unwrap();
        assert_eq!(a.entry_path, "a/index.js");
        assert_eq!(a.installed_by, Some(7));
    }

    /// install_collection_from_zip (Url source) tags rows with Url source_type.
    #[tokio::test]
    async fn install_collection_url_source_type() {
        let (db, _tmp) = setup_test_db().await;
        let zip = build_collection_zip(&[("a", "com.c.url")]);
        let models = WidgetModuleService::install_collection_from_zip(
            &db,
            zip,
            InstalledFrom::Url("https://example.com/p.zip".into()),
            None,
        )
        .await
        .unwrap();
        assert!(matches!(models[0].source_type, SourceType::Url));
        assert_eq!(models[0].source_url.as_deref(), Some("https://example.com/p.zip"));
    }

    /// install_collection_from_zip rejects a non-zip blob (invalid archive).
    #[tokio::test]
    async fn install_collection_rejects_non_zip() {
        let (db, _tmp) = setup_test_db().await;
        let err = WidgetModuleService::install_collection_from_zip(
            &db,
            b"not a zip".to_vec(),
            InstalledFrom::Upload("x.zip".into()),
            None,
        )
        .await
        .err()
        .expect("non-zip should fail");
        assert!(matches!(err, WidgetModuleError::ManifestExtraction(ref m) if m.contains("invalid zip")));
    }

    /// install_collection_from_zip rejects a zip missing collection.json.
    #[tokio::test]
    async fn install_collection_missing_manifest() {
        let (db, _tmp) = setup_test_db().await;
        let zip = build_zip(&[("a/index.js", widget_js("com.c.a").as_bytes())]);
        let err = WidgetModuleService::install_collection_from_zip(
            &db,
            zip,
            InstalledFrom::Upload("x.zip".into()),
            None,
        )
        .await
        .err()
        .expect("missing collection.json should fail");
        assert!(matches!(err, WidgetModuleError::ManifestExtraction(ref m) if m.contains("missing collection.json")));
    }

    /// install_collection_from_zip rejects a syntactically invalid collection.json.
    #[tokio::test]
    async fn install_collection_invalid_manifest_json() {
        let (db, _tmp) = setup_test_db().await;
        let zip = build_zip(&[("collection.json", b"{ not json")]);
        let err = WidgetModuleService::install_collection_from_zip(
            &db,
            zip,
            InstalledFrom::Upload("x.zip".into()),
            None,
        )
        .await
        .err()
        .expect("invalid collection.json should fail");
        assert!(matches!(err, WidgetModuleError::ManifestExtraction(ref m) if m.contains("collection.json invalid")));
    }

    /// install_collection_from_zip rejects an empty widgets array.
    #[tokio::test]
    async fn install_collection_empty_widgets() {
        let (db, _tmp) = setup_test_db().await;
        let manifest = json!({ "widgets": [] }).to_string();
        let zip = build_zip(&[("collection.json", manifest.as_bytes())]);
        let err = WidgetModuleService::install_collection_from_zip(
            &db,
            zip,
            InstalledFrom::Upload("x.zip".into()),
            None,
        )
        .await
        .err()
        .expect("empty widgets must fail");
        assert!(matches!(err, WidgetModuleError::ManifestValidation(ref m) if m.contains("must not be empty")));
    }

    /// install_collection_from_zip rejects an entry path containing `..`.
    #[tokio::test]
    async fn install_collection_rejects_dotdot_entry() {
        let (db, _tmp) = setup_test_db().await;
        let manifest = json!({ "widgets": [{ "entry": "../escape.js" }] }).to_string();
        let zip = build_zip(&[
            ("collection.json", manifest.as_bytes()),
            ("a/index.js", widget_js("com.c.a").as_bytes()),
        ]);
        let err = WidgetModuleService::install_collection_from_zip(
            &db,
            zip,
            InstalledFrom::Upload("x.zip".into()),
            None,
        )
        .await
        .err()
        .expect("dotdot entry must fail");
        assert!(matches!(err, WidgetModuleError::ManifestValidation(ref m) if m.contains("invalid entry path")));
    }

    /// install_collection_from_zip rejects an empty entry path.
    #[tokio::test]
    async fn install_collection_rejects_empty_entry() {
        let (db, _tmp) = setup_test_db().await;
        // A leading "/" trims to an empty path → invalid.
        let manifest = json!({ "widgets": [{ "entry": "/" }] }).to_string();
        let zip = build_zip(&[("collection.json", manifest.as_bytes())]);
        let err = WidgetModuleService::install_collection_from_zip(
            &db,
            zip,
            InstalledFrom::Upload("x.zip".into()),
            None,
        )
        .await
        .err()
        .expect("empty entry must fail");
        assert!(matches!(err, WidgetModuleError::ManifestValidation(ref m) if m.contains("invalid entry path")));
    }

    /// install_collection_from_zip rejects an entry that is not .js/.mjs.
    #[tokio::test]
    async fn install_collection_rejects_non_js_entry() {
        let (db, _tmp) = setup_test_db().await;
        let manifest = json!({ "widgets": [{ "entry": "a/index.ts" }] }).to_string();
        let zip = build_zip(&[
            ("collection.json", manifest.as_bytes()),
            ("a/index.ts", b"export default {};"),
        ]);
        let err = WidgetModuleService::install_collection_from_zip(
            &db,
            zip,
            InstalledFrom::Upload("x.zip".into()),
            None,
        )
        .await
        .err()
        .expect("non-js entry must fail");
        assert!(matches!(err, WidgetModuleError::ManifestValidation(ref m) if m.contains("must be .js or .mjs")));
    }

    /// install_collection_from_zip rejects an entry that is missing from the zip.
    #[tokio::test]
    async fn install_collection_rejects_entry_not_in_zip() {
        let (db, _tmp) = setup_test_db().await;
        let manifest = json!({ "widgets": [{ "entry": "ghost/index.js" }] }).to_string();
        // collection.json present, but the referenced file is not.
        let zip = build_zip(&[("collection.json", manifest.as_bytes())]);
        let err = WidgetModuleService::install_collection_from_zip(
            &db,
            zip,
            InstalledFrom::Upload("x.zip".into()),
            None,
        )
        .await
        .err()
        .expect("entry missing from zip must fail");
        assert!(matches!(err, WidgetModuleError::ManifestExtraction(ref m) if m.contains("entry not found in zip")));
    }

    /// install_collection_from_zip propagates an extractor failure for an entry
    /// without a manifest block.
    #[tokio::test]
    async fn install_collection_entry_without_manifest() {
        let (db, _tmp) = setup_test_db().await;
        let manifest = json!({ "widgets": [{ "entry": "a/index.js" }] }).to_string();
        let zip = build_zip(&[
            ("collection.json", manifest.as_bytes()),
            ("a/index.js", b"export default {};"),
        ]);
        let err = WidgetModuleService::install_collection_from_zip(
            &db,
            zip,
            InstalledFrom::Upload("x.zip".into()),
            None,
        )
        .await
        .err()
        .expect("entry without manifest must fail");
        assert!(matches!(err, WidgetModuleError::ManifestExtraction(_)));
    }

    /// install_collection_from_zip rejects duplicate widget ids across entries.
    #[tokio::test]
    async fn install_collection_rejects_duplicate_ids() {
        let (db, _tmp) = setup_test_db().await;
        let zip = build_collection_zip(&[("a", "com.c.dup"), ("b", "com.c.dup")]);
        let err = WidgetModuleService::install_collection_from_zip(
            &db,
            zip,
            InstalledFrom::Upload("x.zip".into()),
            None,
        )
        .await
        .err()
        .expect("duplicate ids must fail");
        assert!(matches!(err, WidgetModuleError::ManifestValidation(ref m) if m.contains("duplicate widget id")));
    }

    /// install_collection_from_zip refuses ids that collide with a different
    /// source_type before touching the database.
    #[tokio::test]
    async fn install_collection_rejects_cross_source_conflict() {
        let (db, _tmp) = setup_test_db().await;
        seed_row(
            &db,
            "com.c.clash",
            SourceType::Builtin,
            "com.c.clash/index.js",
            "{}",
            None,
            true,
        )
        .await;
        let zip = build_collection_zip(&[("a", "com.c.clash")]);
        let err = WidgetModuleService::install_collection_from_zip(
            &db,
            zip,
            InstalledFrom::Upload("x.zip".into()),
            None,
        )
        .await
        .err()
        .expect("cross-source collision must fail");
        assert!(matches!(err, WidgetModuleError::IdConflict(ref m) if m.contains("Builtin")));
        // No rows added beyond the pre-seeded Builtin one.
        assert_eq!(WidgetModuleEntity::find().all(&db).await.unwrap().len(), 1);
    }

    // ---------- uninstall ----------

    /// uninstall deletes a non-builtin row.
    #[tokio::test]
    async fn uninstall_deletes_row() {
        let (db, _tmp) = setup_test_db().await;
        seed_row(&db, "del.id", SourceType::Upload, "index.js", "{}", None, true).await;

        WidgetModuleService::uninstall(&db, "del.id").await.unwrap();
        assert!(WidgetModuleEntity::find_by_id("del.id".to_string())
            .one(&db)
            .await
            .unwrap()
            .is_none());
    }

    /// uninstall maps a missing id to NotFound.
    #[tokio::test]
    async fn uninstall_missing_is_not_found() {
        let (db, _tmp) = setup_test_db().await;
        let err = WidgetModuleService::uninstall(&db, "ghost")
            .await
            .err()
            .expect("uninstall missing should fail");
        assert!(matches!(err, WidgetModuleError::NotFound(_)));
    }

    /// uninstall refuses to delete a Builtin row.
    #[tokio::test]
    async fn uninstall_refuses_builtin() {
        let (db, _tmp) = setup_test_db().await;
        seed_row(&db, "bi.id", SourceType::Builtin, "bi.id/index.js", "{}", None, true).await;

        let err = WidgetModuleService::uninstall(&db, "bi.id")
            .await
            .err()
            .expect("builtin uninstall should fail");
        assert!(matches!(err, WidgetModuleError::ManifestValidation(ref m) if m.contains("builtin")));
        // The row is still present.
        assert!(WidgetModuleEntity::find_by_id("bi.id".to_string())
            .one(&db)
            .await
            .unwrap()
            .is_some());
    }

    // ---------- mime_for ----------

    /// mime_for maps every recognised extension and falls back to octet-stream.
    #[test]
    fn mime_for_covers_all_extensions() {
        assert_eq!(mime_for("a.js"), "text/javascript; charset=utf-8");
        assert_eq!(mime_for("a.mjs"), "text/javascript; charset=utf-8");
        assert_eq!(mime_for("a.json"), "application/json");
        assert_eq!(mime_for("a.css"), "text/css");
        assert_eq!(mime_for("a.svg"), "image/svg+xml");
        assert_eq!(mime_for("a.png"), "image/png");
        assert_eq!(mime_for("a.jpg"), "image/jpeg");
        assert_eq!(mime_for("a.jpeg"), "image/jpeg");
        assert_eq!(mime_for("a.webp"), "image/webp");
        assert_eq!(mime_for("a.wasm"), "application/wasm");
        assert_eq!(mime_for("a.html"), "text/html; charset=utf-8");
        assert_eq!(mime_for("a.htm"), "text/html; charset=utf-8");
        assert_eq!(mime_for("a.txt"), "text/plain; charset=utf-8");
        assert_eq!(mime_for("a.md"), "text/markdown; charset=utf-8");
        // Unknown extension and case-insensitivity.
        assert_eq!(mime_for("a.bin"), "application/octet-stream");
        assert_eq!(mime_for("A.JS"), "text/javascript; charset=utf-8");
    }
}
