use chrono::Utc;
use rust_embed::Embed;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::entity::widget_module::{self, Entity as WidgetModuleEntity, SourceType};

/// Reads files from `apps/web/dist/builtin-widgets/` at compile time.
#[derive(Embed)]
#[folder = "../../apps/web/dist/builtin-widgets"]
struct BuiltinAssets;

#[derive(Debug, Deserialize)]
struct ManifestEntry {
    id: String,
    version: String,
    entry_path: String,
    manifest: serde_json::Value,
}

pub async fn register_all(db: &DatabaseConnection) -> anyhow::Result<()> {
    let Some(raw) = BuiltinAssets::get("manifest.json") else {
        tracing::warn!(
            "builtin widgets manifest.json missing — run `cd apps/web && bun run build` to generate it"
        );
        return Ok(());
    };

    let entries: Vec<ManifestEntry> = serde_json::from_slice(raw.data.as_ref())?;

    for entry in &entries {
        let Some(code) = BuiltinAssets::get(&entry.entry_path) else {
            tracing::warn!(
                "builtin widget {} references missing entry_path {}",
                entry.id,
                entry.entry_path
            );
            continue;
        };
        let sha = {
            let mut h = Sha256::new();
            h.update(code.data.as_ref());
            format!("{:x}", h.finalize())
        };

        let active = widget_module::ActiveModel {
            id: Set(entry.id.clone()),
            version: Set(entry.version.clone()),
            source_type: Set(SourceType::Builtin),
            source_url: Set(None),
            bundled_by_theme_id: Set(None),
            manifest_json: Set(serde_json::to_string(&entry.manifest)?),
            code_sha256: Set(sha),
            entry_path: Set(entry.entry_path.clone()),
            package_blob: Set(None),
            installed_by: Set(None),
            installed_at: Set(Utc::now()),
            enabled: Set(true),
        };

        WidgetModuleEntity::insert(active)
            .on_conflict(
                OnConflict::column(widget_module::Column::Id)
                    .update_columns([
                        widget_module::Column::Version,
                        widget_module::Column::ManifestJson,
                        widget_module::Column::CodeSha256,
                        widget_module::Column::EntryPath,
                        widget_module::Column::InstalledAt,
                        widget_module::Column::Enabled,
                    ])
                    .to_owned(),
            )
            .exec(db)
            .await?;
    }

    // Delete any stale Builtin rows not present in the current manifest.
    let active_ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    let mut delete = WidgetModuleEntity::delete_many()
        .filter(widget_module::Column::SourceType.eq(SourceType::Builtin));
    if !active_ids.is_empty() {
        delete = delete.filter(widget_module::Column::Id.is_not_in(active_ids));
    }
    delete.exec(db).await?;

    Ok(())
}

pub fn builtin_asset_bytes(entry_path: &str) -> Option<Vec<u8>> {
    BuiltinAssets::get(entry_path).map(|f| f.data.into_owned())
}
