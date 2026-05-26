// The body of `up()` is wrapped in a SQLite transaction (BEGIN/COMMIT, with
// ROLLBACK on error) so a mid-migration failure does not leave the schema
// half-altered. This is safe because every step uses raw SQL via
// `db.execute_unprepared` / `db.execute` — we do not call any
// `SchemaManager::alter_table` / `create_table` helpers, which historically
// have their own connection-management semantics. If you add such a call,
// re-evaluate whether the transaction still wraps it correctly.

use sea_orm::{ConnectionTrait, DbErr, Statement};
use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260526_000035_simplify_status_page"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared("BEGIN TRANSACTION").await?;
        let outcome = Self::run_up(db).await;
        match outcome {
            Ok(()) => {
                db.execute_unprepared("COMMIT").await?;
                Ok(())
            }
            Err(e) => {
                let _ = db.execute_unprepared("ROLLBACK").await;
                Err(e)
            }
        }
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

impl Migration {
    async fn run_up(db: &impl ConnectionTrait) -> Result<(), DbErr> {
        let backend = db.get_database_backend();

        // 1. Resolve the surviving status_page row id.
        //    Prefer the most-recently-updated enabled = true row;
        //    fall back to the most-recently-updated row;
        //    if the table is empty, insert a default row.
        let surviving_id: String = {
            // Try most-recently-updated enabled row first.
            let row = db
                .query_one(Statement::from_string(
                    backend,
                    "SELECT id FROM status_page WHERE enabled = 1 ORDER BY updated_at DESC LIMIT 1"
                        .to_string(),
                ))
                .await?;
            if let Some(row) = row {
                row.try_get::<String>("", "id")?
            } else {
                // Fall back to most-recently-updated row regardless of enabled.
                let row = db
                    .query_one(Statement::from_string(
                        backend,
                        "SELECT id FROM status_page ORDER BY updated_at DESC LIMIT 1".to_string(),
                    ))
                    .await?;
                if let Some(row) = row {
                    row.try_get::<String>("", "id")?
                } else {
                    // Empty: insert a default singleton row.
                    // Column list must match the cumulative `status_page` schema as
                    // it stands at the start of this migration — do not reorder
                    // this step relative to the ALTER TABLE statements below.
                    // Note: `slug` is set to the row id purely to satisfy NOT NULL;
                    // the slug column is dropped later in this migration, so this
                    // value never reaches the post-migration state.
                    let new_id = uuid::Uuid::new_v4().to_string();
                    db.execute(Statement::from_sql_and_values(
                        backend,
                        "INSERT INTO status_page (id, title, slug, description, server_ids_json, group_by_server_group, show_values, custom_css, enabled, uptime_yellow_threshold, uptime_red_threshold, theme_ref, show_ip_quality, created_at, updated_at) VALUES (?, ?, ?, NULL, '[]', 1, 1, NULL, 0, ?, ?, NULL, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
                        [
                            new_id.clone().into(),
                            "Status".into(),
                            new_id.clone().into(),
                            99.0_f64.into(),
                            95.0_f64.into(),
                        ],
                    ))
                    .await?;
                    new_id
                }
            }
        };

        // 2. Drop the slug unique index (created in m20260320_000009_status_page).
        db.execute_unprepared("DROP INDEX IF EXISTS idx_status_page_slug_unique")
            .await?;

        // 3. Drop triggers from m20260430_000021_custom_theme_ref_integrity.rs
        //    that reference status_page.theme_ref.
        db.execute_unprepared(
            "DROP TRIGGER IF EXISTS trg_custom_theme_status_page_insert_ref_exists",
        )
        .await?;
        db.execute_unprepared(
            "DROP TRIGGER IF EXISTS trg_custom_theme_status_page_update_ref_exists",
        )
        .await?;
        db.execute_unprepared("DROP TRIGGER IF EXISTS trg_custom_theme_delete_no_status_page_ref")
            .await?;

        // 4. Extend status_page with new columns.
        db.execute_unprepared(
            "ALTER TABLE status_page ADD COLUMN default_layout TEXT NOT NULL DEFAULT 'grid'",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE status_page ADD COLUMN show_server_detail BOOLEAN NOT NULL DEFAULT 1",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE status_page ADD COLUMN show_network BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE status_page ADD COLUMN show_incidents BOOLEAN NOT NULL DEFAULT 1",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE status_page ADD COLUMN show_maintenance BOOLEAN NOT NULL DEFAULT 1",
        )
        .await?;

        // 5. Add is_public to incident and maintenance.
        db.execute_unprepared(
            "ALTER TABLE incident ADD COLUMN is_public BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE maintenance ADD COLUMN is_public BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;

        // 6. Backfill is_public preserving existing public-visibility semantics.
        //    A row was visible on a public slug page if status_page_ids_json was
        //    NULL, empty [], or contained the surviving page's id.
        //    `status_page_ids_json` is a serde_json-serialised `Vec<String>`, so
        //    ids are always wrapped in `"…"` inside the JSON array literal — the
        //    `\"` in the LIKE pattern brackets that quoting. The id itself is
        //    bound as a parameter so no row-sourced data is interpolated into SQL.
        db.execute(Statement::from_sql_and_values(
            backend,
            "UPDATE incident SET is_public = 1 WHERE status_page_ids_json IS NULL OR status_page_ids_json = '[]' OR status_page_ids_json LIKE '%\"' || ? || '\"%'",
            [surviving_id.clone().into()],
        ))
        .await?;

        db.execute(Statement::from_sql_and_values(
            backend,
            "UPDATE maintenance SET is_public = 1 WHERE status_page_ids_json IS NULL OR status_page_ids_json = '[]' OR status_page_ids_json LIKE '%\"' || ? || '\"%'",
            [surviving_id.clone().into()],
        ))
        .await?;

        // 7. Drop legacy columns.
        db.execute_unprepared("ALTER TABLE incident DROP COLUMN status_page_ids_json")
            .await?;
        db.execute_unprepared("ALTER TABLE maintenance DROP COLUMN status_page_ids_json")
            .await?;
        db.execute_unprepared("ALTER TABLE status_page DROP COLUMN slug")
            .await?;
        db.execute_unprepared("ALTER TABLE status_page DROP COLUMN theme_ref")
            .await?;
        db.execute_unprepared("ALTER TABLE status_page DROP COLUMN custom_css")
            .await?;
        db.execute_unprepared("ALTER TABLE status_page DROP COLUMN show_values")
            .await?;

        // 8. Delete non-surviving status_page rows.
        db.execute(Statement::from_sql_and_values(
            backend,
            "DELETE FROM status_page WHERE id != ?",
            [surviving_id.into()],
        ))
        .await?;

        Ok(())
    }
}
