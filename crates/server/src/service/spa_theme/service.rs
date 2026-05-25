use sea_orm::prelude::Expr;
use sea_orm::{FromQueryResult, QuerySelect};
use sea_orm::*;
use uuid::Uuid;

use crate::entity::spa_theme;
use crate::error::AppError;
use crate::service::config::ConfigService;
use crate::service::spa_theme::error::SpaThemeError;
use crate::service::spa_theme::extractor;
use crate::service::spa_theme::loaded::LoadedTheme;
use crate::service::spa_theme::manifest::ThemeManifest;

pub const ACTIVE_SPA_THEME_KEY: &str = "active_spa_theme_uuid";

pub struct SpaThemeService;

#[derive(FromQueryResult)]
struct SummaryRow {
    uuid: String,
    manifest_id: String,
    name: String,
    version: String,
    author: Option<String>,
    description: Option<String>,
    size_bytes: i64,
    uploaded_by: String,
    uploaded_at: chrono::DateTime<chrono::Utc>,
    is_superseded: i32,
    preview_present: i32, // SQLite-returned boolean
}

#[derive(FromQueryResult)]
struct VersionRow {
    uuid: String,
    #[allow(dead_code)]
    manifest_id: String,
    version: String,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct SpaThemeSummary {
    pub uuid: String,
    pub manifest_id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub size_bytes: i64,
    pub uploaded_by: String,
    pub uploaded_at: String,
    pub is_active: bool,
    pub is_superseded: bool,
    pub has_preview: bool,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct UploadResult {
    pub uuid: String,
    pub manifest: serde_json::Value,
    pub size_bytes: i64,
    pub preview_url: Option<String>,
    pub is_upgrade_of: Option<UpgradeOf>,
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct UpgradeOf {
    pub previous_uuid: String,
    pub previous_version: String,
}

impl SpaThemeService {
    fn running_version() -> semver::Version {
        let s = env!("CARGO_PKG_VERSION");
        semver::Version::parse(s).unwrap_or_else(|_| semver::Version::new(0, 0, 0))
    }

    pub async fn list(db: &DatabaseConnection) -> Result<Vec<SpaThemeSummary>, AppError> {
        let active = ConfigService::get(db, ACTIVE_SPA_THEME_KEY).await?.unwrap_or_default();
        let rows: Vec<SummaryRow> = spa_theme::Entity::find()
            .select_only()
            .column(spa_theme::Column::Uuid)
            .column(spa_theme::Column::ManifestId)
            .column(spa_theme::Column::Name)
            .column(spa_theme::Column::Version)
            .column(spa_theme::Column::Author)
            .column(spa_theme::Column::Description)
            .column(spa_theme::Column::SizeBytes)
            .column(spa_theme::Column::UploadedBy)
            .column(spa_theme::Column::UploadedAt)
            .column(spa_theme::Column::IsSuperseded)
            .column_as(
                Expr::cust("preview_data IS NOT NULL"),
                "preview_present",
            )
            .order_by_desc(spa_theme::Column::UploadedAt)
            .into_model::<SummaryRow>()
            .all(db)
            .await?;
        Ok(rows.into_iter().map(|r| SpaThemeSummary {
            is_active: !active.is_empty() && r.uuid == active,
            is_superseded: r.is_superseded != 0,
            has_preview: r.preview_present != 0,
            uuid: r.uuid,
            manifest_id: r.manifest_id,
            name: r.name,
            version: r.version,
            author: r.author,
            description: r.description,
            size_bytes: r.size_bytes,
            uploaded_by: r.uploaded_by,
            uploaded_at: r.uploaded_at.to_rfc3339(),
        }).collect())
    }

    pub async fn get(db: &DatabaseConnection, uuid: &str) -> Result<spa_theme::Model, AppError> {
        spa_theme::Entity::find()
            .filter(spa_theme::Column::Uuid.eq(uuid))
            .one(db)
            .await?
            .ok_or_else(|| SpaThemeError::ThemeNotFound { uuid: uuid.to_string() }.into())
    }

    pub async fn delete(db: &DatabaseConnection, uuid: &str) -> Result<(), AppError> {
        let active = ConfigService::get(db, ACTIVE_SPA_THEME_KEY).await?.unwrap_or_default();
        if active == uuid {
            return Err(SpaThemeError::ThemeInUse { uuid: uuid.to_string() }.into());
        }
        let res = spa_theme::Entity::delete_many()
            .filter(spa_theme::Column::Uuid.eq(uuid))
            .exec(db)
            .await?;
        if res.rows_affected == 0 {
            return Err(SpaThemeError::ThemeNotFound { uuid: uuid.to_string() }.into());
        }
        Ok(())
    }

    /// Validate + persist a new theme. Returns the inserted row + upgrade info.
    pub async fn upload(
        db: &DatabaseConnection,
        zip_bytes: Vec<u8>,
        uploader_user_id: &str,
    ) -> Result<(spa_theme::Model, Option<UpgradeOf>), AppError> {
        let extracted = extractor::extract(&zip_bytes).map_err(AppError::from)?;
        let file_paths: std::collections::HashSet<String> = extracted.files.keys().cloned().collect();
        let manifest = ThemeManifest::parse_and_validate(
            &extracted.manifest_bytes,
            &Self::running_version(),
            &file_paths,
        ).map_err(AppError::from)?;

        // Version policy
        let upgrade_of = Self::check_version_policy(db, &manifest).await?;

        // Preview locate
        let preview = if let Some(p) = &manifest.preview {
            extractor::locate_preview(&extracted.files, p).map_err(AppError::from)?
        } else { None };

        let new_uuid = Uuid::new_v4().to_string();
        let manifest_json = serde_json::to_string(&manifest).map_err(|e| AppError::Internal(e.to_string()))?;
        let am = spa_theme::ActiveModel {
            id: NotSet,
            uuid: Set(new_uuid.clone()),
            manifest_id: Set(manifest.id.clone()),
            name: Set(manifest.name.clone()),
            version: Set(manifest.version.clone()),
            author: Set(manifest.author.clone()),
            description: Set(manifest.description.clone()),
            manifest_json: Set(manifest_json),
            package_data: Set(zip_bytes),
            preview_data: Set(preview.as_ref().map(|(_, b, _)| b.clone())),
            preview_mime: Set(preview.as_ref().map(|(_, _, m)| m.clone())),
            size_bytes: Set(extracted.total_bytes as i64),
            uploaded_by: Set(uploader_user_id.to_string()),
            uploaded_at: Set(chrono::Utc::now()),
            is_superseded: Set(0),
        };

        // Transaction: mark older rows of same manifest_id as superseded, insert new.
        let txn = db.begin().await?;
        if upgrade_of.is_some() {
            spa_theme::Entity::update_many()
                .col_expr(spa_theme::Column::IsSuperseded, Expr::value(1))
                .filter(spa_theme::Column::ManifestId.eq(manifest.id.clone()))
                .exec(&txn).await?;
        }
        let inserted = am.insert(&txn).await?;
        txn.commit().await?;

        Ok((inserted, upgrade_of))
    }

    async fn check_version_policy(
        db: &DatabaseConnection,
        manifest: &ThemeManifest,
    ) -> Result<Option<UpgradeOf>, AppError> {
        let rows: Vec<VersionRow> = spa_theme::Entity::find()
            .select_only()
            .column(spa_theme::Column::Uuid)
            .column(spa_theme::Column::ManifestId)
            .column(spa_theme::Column::Version)
            .filter(spa_theme::Column::ManifestId.eq(manifest.id.clone()))
            .order_by_desc(spa_theme::Column::UploadedAt)
            .into_model::<VersionRow>()
            .all(db)
            .await?;
        if rows.is_empty() { return Ok(None); }
        let uploaded = semver::Version::parse(&manifest.version)
            .map_err(|_| SpaThemeError::InvalidManifest { field: "version", reason: "invalid semver".into() })?;
        let mut best: Option<(semver::Version, VersionRow)> = None;
        for r in rows {
            if let Ok(v) = semver::Version::parse(&r.version)
                && best.as_ref().map(|(b, _)| &v > b).unwrap_or(true)
            {
                best = Some((v, r));
            }
        }
        let (latest_v, latest_row) = match best { Some(b) => b, None => return Ok(None) };
        if uploaded < latest_v {
            return Err(SpaThemeError::NoDowngrade { uploaded: uploaded.to_string(), existing: latest_v.to_string() }.into());
        }
        if uploaded == latest_v {
            return Err(SpaThemeError::VersionExists { manifest_id: manifest.id.clone(), version: uploaded.to_string() }.into());
        }
        Ok(Some(UpgradeOf { previous_uuid: latest_row.uuid, previous_version: latest_v.to_string() }))
    }

    /// Set the active theme (uuid). None deactivates.
    pub async fn set_active(
        db: &DatabaseConnection,
        slot: &crate::service::spa_theme::loaded::ActiveSpaThemeSlot,
        uuid: Option<&str>,
    ) -> Result<Option<String>, AppError> {
        match uuid {
            None => {
                ConfigService::set(db, ACTIVE_SPA_THEME_KEY, "").await?;
                slot.store(std::sync::Arc::new(None));
                Ok(None)
            }
            Some(u) => {
                let row = Self::get(db, u).await?;
                let loaded = Self::load_row(&row)?;
                ConfigService::set(db, ACTIVE_SPA_THEME_KEY, u).await?;
                slot.store(std::sync::Arc::new(Some(loaded)));
                Ok(Some(u.to_string()))
            }
        }
    }

    pub async fn active_uuid(db: &DatabaseConnection) -> Result<Option<String>, AppError> {
        Ok(ConfigService::get(db, ACTIVE_SPA_THEME_KEY)
            .await?
            .filter(|s| !s.is_empty()))
    }

    pub fn load_row(row: &spa_theme::Model) -> Result<LoadedTheme, AppError> {
        let extracted = crate::service::spa_theme::extractor::extract(&row.package_data)
            .map_err(|e| AppError::Internal(format!("re-extract stored theme: {e}")))?;
        let manifest: ThemeManifest = serde_json::from_str(&row.manifest_json)
            .map_err(|e| AppError::Internal(format!("manifest json: {e}")))?;
        Ok(LoadedTheme::from_extracted(row.uuid.clone(), manifest, extracted.files))
    }

    /// Called on server startup. If active uuid stored but row missing or zip broken,
    /// log warning and leave slot empty (fall back to default SPA).
    pub async fn load_on_startup(
        db: &DatabaseConnection,
        slot: &crate::service::spa_theme::loaded::ActiveSpaThemeSlot,
    ) {
        match Self::active_uuid(db).await {
            Ok(Some(u)) => match Self::get(db, &u).await {
                Ok(row) => match Self::load_row(&row) {
                    Ok(loaded) => slot.store(std::sync::Arc::new(Some(loaded))),
                    Err(e) => tracing::warn!("active SPA theme {u} failed to load: {e}; falling back to default"),
                },
                Err(AppError::Domain { code: "THEME_NOT_FOUND", .. }) => {
                    tracing::warn!("active SPA theme {u} not found; falling back to default");
                }
                Err(e) => {
                    tracing::warn!("active SPA theme {u} lookup failed: {e}; falling back to default");
                }
            },
            Ok(None) => {}
            Err(e) => tracing::warn!("read active spa theme key failed: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::Migrator;
    use sea_orm_migration::MigratorTrait;

    async fn db() -> DatabaseConnection {
        let conn = Database::connect("sqlite::memory:").await.unwrap();
        Migrator::up(&conn, None).await.unwrap();
        conn
    }

    fn zip_of(manifest: serde_json::Value) -> Vec<u8> {
        use std::io::Write;
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::SimpleFileOptions = Default::default();
            w.start_file("manifest.json", opts).unwrap();
            w.write_all(manifest.to_string().as_bytes()).unwrap();
            w.start_file("index.html", opts).unwrap();
            w.write_all(b"<html></html>").unwrap();
            w.finish().unwrap();
        }
        buf
    }

    fn manifest_json(id: &str, v: &str) -> serde_json::Value {
        // Pad id to satisfy ID_REGEX: ^[a-z][a-z0-9-]{2,63}$  (min 3 chars)
        let padded = if id.len() < 3 { format!("{id}xx") } else { id.to_string() };
        serde_json::json!({"schema_version":1,"id":padded,"name":id,"version":v})
    }

    async fn ensure_user(db: &DatabaseConnection, id: &str) {
        use crate::entity::user;
        if user::Entity::find_by_id(id.to_string()).one(db).await.unwrap().is_none() {
            let _ = user::ActiveModel {
                id: Set(id.to_string()),
                username: Set(id.to_string()),
                password_hash: Set("x".into()),
                role: Set("admin".into()),
                totp_secret: Set(None),
                must_change_password: Set(false),
                created_at: Set(chrono::Utc::now()),
                updated_at: Set(chrono::Utc::now()),
            }.insert(db).await;
        }
    }

    #[tokio::test]
    async fn upload_succeeds_and_lists() {
        let db = db().await;
        ensure_user(&db, "u1").await;
        let zip = zip_of(manifest_json("acme", "1.0.0"));
        let (m, up) = SpaThemeService::upload(&db, zip, "u1").await.unwrap();
        assert!(up.is_none());
        let list = SpaThemeService::list(&db).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].uuid, m.uuid);
        assert!(!list[0].is_active);
    }

    #[tokio::test]
    async fn rejects_downgrade() {
        let db = db().await; ensure_user(&db, "u1").await;
        SpaThemeService::upload(&db, zip_of(manifest_json("a", "1.1.0")), "u1").await.unwrap();
        let err = SpaThemeService::upload(&db, zip_of(manifest_json("a", "1.0.0")), "u1").await.unwrap_err();
        if let AppError::Domain { code, .. } = err { assert_eq!(code, "NO_DOWNGRADE"); } else { panic!() }
    }

    #[tokio::test]
    async fn rejects_same_version() {
        let db = db().await; ensure_user(&db, "u1").await;
        SpaThemeService::upload(&db, zip_of(manifest_json("a", "1.0.0")), "u1").await.unwrap();
        let err = SpaThemeService::upload(&db, zip_of(manifest_json("a", "1.0.0")), "u1").await.unwrap_err();
        if let AppError::Domain { code, .. } = err { assert_eq!(code, "VERSION_EXISTS"); } else { panic!() }
    }

    #[tokio::test]
    async fn upgrade_marks_superseded() {
        let db = db().await; ensure_user(&db, "u1").await;
        SpaThemeService::upload(&db, zip_of(manifest_json("a", "1.0.0")), "u1").await.unwrap();
        let (_, up) = SpaThemeService::upload(&db, zip_of(manifest_json("a", "1.1.0")), "u1").await.unwrap();
        assert!(up.is_some());
        let list = SpaThemeService::list(&db).await.unwrap();
        assert_eq!(list.len(), 2);
        let superseded: Vec<_> = list.iter().filter(|s| s.is_superseded).collect();
        assert_eq!(superseded.len(), 1);
        assert_eq!(superseded[0].version, "1.0.0");
    }

    #[tokio::test]
    async fn delete_active_rejected() {
        let db = db().await; ensure_user(&db, "u1").await;
        let (m, _) = SpaThemeService::upload(&db, zip_of(manifest_json("a", "1.0.0")), "u1").await.unwrap();
        ConfigService::set(&db, ACTIVE_SPA_THEME_KEY, &m.uuid).await.unwrap();
        let err = SpaThemeService::delete(&db, &m.uuid).await.unwrap_err();
        if let AppError::Domain { code, .. } = err { assert_eq!(code, "THEME_IN_USE"); } else { panic!() }
    }

    #[tokio::test]
    async fn set_active_loads_into_slot() {
        let db = db().await; ensure_user(&db, "u1").await;
        let (m, _) = SpaThemeService::upload(&db, zip_of(manifest_json("a", "1.0.0")), "u1").await.unwrap();
        let slot = crate::service::spa_theme::loaded::new_slot();
        SpaThemeService::set_active(&db, &slot, Some(&m.uuid)).await.unwrap();
        let loaded = slot.load();
        assert!(loaded.as_ref().is_some());
        let theme = loaded.as_ref().as_ref().unwrap();
        assert_eq!(theme.uuid, m.uuid);
        assert!(theme.entry_html().is_some());
    }

    #[tokio::test]
    async fn set_none_deactivates() {
        let db = db().await; ensure_user(&db, "u1").await;
        let (m, _) = SpaThemeService::upload(&db, zip_of(manifest_json("a", "1.0.0")), "u1").await.unwrap();
        let slot = crate::service::spa_theme::loaded::new_slot();
        SpaThemeService::set_active(&db, &slot, Some(&m.uuid)).await.unwrap();
        SpaThemeService::set_active(&db, &slot, None).await.unwrap();
        assert!(slot.load().is_none());
    }

    #[tokio::test]
    async fn startup_load_with_dangling_key_falls_back() {
        let db = db().await;
        ConfigService::set(&db, ACTIVE_SPA_THEME_KEY, "does-not-exist").await.unwrap();
        let slot = crate::service::spa_theme::loaded::new_slot();
        SpaThemeService::load_on_startup(&db, &slot).await;
        assert!(slot.load().is_none());
    }

    #[tokio::test]
    async fn startup_load_with_corrupt_blob_falls_back() {
        let db = db().await;
        ensure_user(&db, "u1").await;
        spa_theme::ActiveModel {
            id: NotSet,
            uuid: Set("corrupt-uuid".into()),
            manifest_id: Set("test-id".into()),
            name: Set("Test".into()),
            version: Set("1.0.0".into()),
            author: Set(None),
            description: Set(None),
            manifest_json: Set("{}".into()),
            package_data: Set(b"not a zip".to_vec()),
            preview_data: Set(None),
            preview_mime: Set(None),
            size_bytes: Set(9),
            uploaded_by: Set("u1".into()),
            uploaded_at: Set(chrono::Utc::now()),
            is_superseded: Set(0),
        }.insert(&db).await.unwrap();
        ConfigService::set(&db, ACTIVE_SPA_THEME_KEY, "corrupt-uuid").await.unwrap();
        let slot = crate::service::spa_theme::loaded::new_slot();
        SpaThemeService::load_on_startup(&db, &slot).await;
        assert!(slot.load().is_none());
    }
}
