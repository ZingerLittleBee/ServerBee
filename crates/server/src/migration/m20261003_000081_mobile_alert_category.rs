use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Independent category migrations may run in either integration order.
        // Reuse the shared column without changing existing delivery metadata.
        if !manager.has_column("mobile_push_outbox", "category").await? {
            manager
                .get_connection()
                .execute_unprepared(
                    "ALTER TABLE mobile_push_outbox ADD COLUMN category TEXT NOT NULL DEFAULT 'test'",
                )
                .await?;
        }
        Ok(())
    }
    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::Migrator;
    use sea_orm::{ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, Statement};

    async fn accepted_base() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:").await.expect("SQLite");
        let base_index = Migrator::migrations()
            .iter()
            .position(|migration| migration.name() == "m20261003_000080_mobile_push_outbox")
            .expect("accepted base migration");
        let steps = <u32 as std::convert::TryFrom<usize>>::try_from(base_index + 1)
            .expect("migration count");
        Migrator::up(&db, Some(steps))
            .await
            .expect("migrate through accepted outbox base");
        assert!(
            !SchemaManager::new(&db)
                .has_column("mobile_push_outbox", "category")
                .await
                .expect("inspect base schema")
        );
        db.execute_unprepared(
            "INSERT INTO mobile_push_outbox
            (event_id, installation_id, user_id, mobile_session_id, registration_revision,
             recipient_role, created_at, expires_at, envelope, next_attempt_at)
            VALUES ('existing-event', 'existing-installation', 'existing-user', 'existing-session',
                7, 'member', 100, 1900, 'encrypted-fixture', 100)",
        )
        .await
        .expect("seed existing delivery");
        db
    }

    async fn assert_preserved(db: &DatabaseConnection, category: &str) {
        let row = db.query_one(Statement::from_string(DatabaseBackend::Sqlite,
            "SELECT category, envelope, registration_revision FROM mobile_push_outbox WHERE event_id='existing-event'".to_string()))
            .await.expect("read delivery").expect("existing delivery survives");
        assert_eq!(
            row.try_get::<String>("", "category").expect("category"),
            category
        );
        assert_eq!(
            row.try_get::<String>("", "envelope").expect("ciphertext"),
            "encrypted-fixture"
        );
        assert_eq!(
            row.try_get::<i64>("", "registration_revision")
                .expect("revision"),
            7
        );
        let columns = db
            .query_all(Statement::from_string(
                DatabaseBackend::Sqlite,
                "PRAGMA table_info('mobile_push_outbox')".to_string(),
            ))
            .await
            .expect("inspect final schema");
        let category_columns: Vec<_> = columns
            .iter()
            .filter(|column| {
                column.try_get::<String>("", "name").expect("column name") == "category"
            })
            .collect();
        assert_eq!(category_columns.len(), 1);
        assert_eq!(
            category_columns[0]
                .try_get::<String>("", "type")
                .expect("type"),
            "TEXT"
        );
        assert_eq!(
            category_columns[0]
                .try_get::<i64>("", "notnull")
                .expect("not null"),
            1
        );
        assert_eq!(
            category_columns[0]
                .try_get::<String>("", "dflt_value")
                .expect("default"),
            "'test'"
        );
    }

    #[tokio::test]
    async fn mobile_alert_migration_adds_shared_category_preserving_existing_jobs() {
        let db = accepted_base().await;
        Migrator::up(&db, None)
            .await
            .expect("apply category migration");
        assert_preserved(&db, "test").await;
        Migrator::up(&db, None)
            .await
            .expect("restart migration runner");
        assert_preserved(&db, "test").await;
    }

    #[tokio::test]
    async fn mobile_alert_migration_reuses_existing_category_preserving_values() {
        let db = accepted_base().await;
        // Represent another independently reserved migration's shared column.
        db.execute_unprepared(
            "ALTER TABLE mobile_push_outbox ADD COLUMN category TEXT NOT NULL DEFAULT 'test'",
        )
        .await
        .expect("existing shared column");
        db.execute_unprepared("UPDATE mobile_push_outbox SET category='security'")
            .await
            .expect("existing category value");
        Migrator::up(&db, None)
            .await
            .expect("reuse shared category");
        assert_preserved(&db, "security").await;
        Migration
            .up(&SchemaManager::new(&db))
            .await
            .expect("repeat shared-column admission");
        assert_preserved(&db, "security").await;
    }
}
