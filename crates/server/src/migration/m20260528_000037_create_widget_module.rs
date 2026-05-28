use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS widget_module (
                id                    TEXT NOT NULL PRIMARY KEY,
                version               TEXT NOT NULL,
                source_type           TEXT NOT NULL,
                source_url            TEXT,
                bundled_by_theme_id   TEXT,
                manifest_json         TEXT NOT NULL,
                code_sha256           TEXT NOT NULL,
                entry_path            TEXT NOT NULL,
                package_blob          BLOB,
                installed_by          INTEGER,
                installed_at          TEXT NOT NULL,
                enabled               INTEGER NOT NULL DEFAULT 1
            )
            "#,
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_widget_module_source_type ON widget_module(source_type)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_widget_module_theme ON widget_module(bundled_by_theme_id) WHERE bundled_by_theme_id IS NOT NULL",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
