//! Drop the legacy `spa_themes` and `custom_theme` tables.
//!
//! These tables backed the now-removed SPA package upload + custom CSS variable
//! theme features. The new widget module system supersedes both. `down()` is a
//! no-op per project policy (see CLAUDE.md): migrations are forward-only.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // Drop dependent triggers first (created in
        // m20260430_000021_custom_theme_ref_integrity) before the tables they
        // reference go away. `m20260526_000036_simplify_status_page` already
        // drops the `status_page` triggers; the `configs` triggers and the
        // `custom_theme` table itself are still around and must go now. The
        // `configs` triggers also have to be dropped explicitly because their
        // WHEN clauses reference `custom_theme`, and SQLite refuses to load a
        // trigger whose body references a missing table the next time the
        // owning table is touched. `IF EXISTS` keeps every step idempotent.
        db.execute_unprepared(
            "DROP TRIGGER IF EXISTS trg_custom_theme_config_insert_ref_exists",
        )
        .await?;
        db.execute_unprepared(
            "DROP TRIGGER IF EXISTS trg_custom_theme_config_update_ref_exists",
        )
        .await?;
        db.execute_unprepared(
            "DROP TRIGGER IF EXISTS trg_custom_theme_status_page_insert_ref_exists",
        )
        .await?;
        db.execute_unprepared(
            "DROP TRIGGER IF EXISTS trg_custom_theme_status_page_update_ref_exists",
        )
        .await?;
        db.execute_unprepared("DROP TRIGGER IF EXISTS trg_custom_theme_delete_no_active_config_ref")
            .await?;
        db.execute_unprepared("DROP TRIGGER IF EXISTS trg_custom_theme_delete_no_status_page_ref")
            .await?;
        db.execute_unprepared("DROP TABLE IF EXISTS spa_themes").await?;
        db.execute_unprepared("DROP TABLE IF EXISTS custom_theme")
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
