use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260430_000021_custom_theme_ref_integrity"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TRIGGER IF NOT EXISTS trg_custom_theme_config_insert_ref_exists
            BEFORE INSERT ON configs
            WHEN NEW.key = 'active_admin_theme'
                AND NEW.value LIKE 'custom:%'
                AND NOT EXISTS (
                    SELECT 1 FROM custom_theme
                    WHERE 'custom:' || id = NEW.value
                )
            BEGIN
                SELECT RAISE(ABORT, 'custom_theme_ref_missing');
            END",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TRIGGER IF NOT EXISTS trg_custom_theme_config_update_ref_exists
            BEFORE UPDATE ON configs
            WHEN NEW.key = 'active_admin_theme'
                AND NEW.value LIKE 'custom:%'
                AND NOT EXISTS (
                    SELECT 1 FROM custom_theme
                    WHERE 'custom:' || id = NEW.value
                )
            BEGIN
                SELECT RAISE(ABORT, 'custom_theme_ref_missing');
            END",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TRIGGER IF NOT EXISTS trg_custom_theme_status_page_insert_ref_exists
            BEFORE INSERT ON status_page
            WHEN NEW.theme_ref LIKE 'custom:%'
                AND NOT EXISTS (
                    SELECT 1 FROM custom_theme
                    WHERE 'custom:' || id = NEW.theme_ref
                )
            BEGIN
                SELECT RAISE(ABORT, 'custom_theme_ref_missing');
            END",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TRIGGER IF NOT EXISTS trg_custom_theme_status_page_update_ref_exists
            BEFORE UPDATE ON status_page
            WHEN NEW.theme_ref LIKE 'custom:%'
                AND NOT EXISTS (
                    SELECT 1 FROM custom_theme
                    WHERE 'custom:' || id = NEW.theme_ref
                )
            BEGIN
                SELECT RAISE(ABORT, 'custom_theme_ref_missing');
            END",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TRIGGER IF NOT EXISTS trg_custom_theme_delete_no_active_config_ref
            BEFORE DELETE ON custom_theme
            WHEN EXISTS (
                SELECT 1 FROM configs
                WHERE key = 'active_admin_theme'
                    AND value = 'custom:' || OLD.id
            )
            BEGIN
                SELECT RAISE(ABORT, 'custom_theme_ref_in_use');
            END",
        )
        .await?;
        db.execute_unprepared(
            "CREATE TRIGGER IF NOT EXISTS trg_custom_theme_delete_no_status_page_ref
            BEFORE DELETE ON custom_theme
            WHEN EXISTS (
                SELECT 1 FROM status_page
                WHERE theme_ref = 'custom:' || OLD.id
            )
            BEGIN
                SELECT RAISE(ABORT, 'custom_theme_ref_in_use');
            END",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
