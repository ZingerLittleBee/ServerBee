use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.has_column("mobile_push_outbox", "category").await? {
            return Ok(());
        }
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE mobile_push_outbox ADD COLUMN category TEXT NOT NULL DEFAULT 'test'",
            )
            .await?;
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
    use sea_orm::{
        Database, DatabaseBackend, DatabaseConnection, EntityTrait, QueryOrder, Statement,
    };

    async fn before_category_migration() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("connect SQLite");
        let accepted_base_count = Migrator::migrations()
            .iter()
            .position(|migration| migration.name() == "m20261003_000080_mobile_push_outbox")
            .expect("accepted outbox base is registered")
            + 1;
        // Keep the missing-column path meaningful after other category
        // migrations are integrated before this one.
        Migrator::up(&db, Some(accepted_base_count as u32))
            .await
            .expect("migrate through the accepted outbox schema");
        db.execute_unprepared(
            "INSERT INTO mobile_push_outbox
             (event_id, installation_id, user_id, mobile_session_id,
              registration_revision, recipient_role, created_at, expires_at,
              envelope, outcome, reason, attempts, next_attempt_at, lease_id, lease_until)
             VALUES
             ('pending-event', 'installation-a', 'user-a', 'session-a', 3, 'admin',
              2000000000, 2000001800, 'encrypted-fixture', 'retryable',
              'NetworkUnavailable', 2, 2000000008, 'active-lease', 2000000030),
             ('accepted-event', 'installation-b', 'user-b', 'session-b', 9, 'member',
              2000000100, 2000001900, NULL, 'accepted',
              'Accepted', 1, 2000000100, NULL, 0)",
        )
        .await
        .expect("persist pre-existing delivery and receipt");
        db
    }

    async fn delivery_snapshots(db: &DatabaseConnection) -> Vec<serde_json::Value> {
        db.query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT * FROM mobile_push_outbox ORDER BY event_id".to_owned(),
        ))
        .await
        .expect("read existing queue values")
        .into_iter()
        .map(|row| {
            // Read only this migration's predecessor columns. Later integrated
            // migrations may extend the entity before their schema is applied.
            serde_json::json!({
                "event_id": row.try_get::<String>("", "event_id").unwrap(),
                "installation_id": row.try_get::<String>("", "installation_id").unwrap(),
                "user_id": row.try_get::<String>("", "user_id").unwrap(),
                "mobile_session_id": row.try_get::<String>("", "mobile_session_id").unwrap(),
                "registration_revision": row.try_get::<i64>("", "registration_revision").unwrap(),
                "recipient_role": row.try_get::<String>("", "recipient_role").unwrap(),
                "created_at": row.try_get::<i64>("", "created_at").unwrap(),
                "expires_at": row.try_get::<i64>("", "expires_at").unwrap(),
                "envelope": row.try_get::<Option<String>>("", "envelope").unwrap(),
                "outcome": row.try_get::<String>("", "outcome").unwrap(),
                "reason": row.try_get::<String>("", "reason").unwrap(),
                "attempts": row.try_get::<i64>("", "attempts").unwrap(),
                "next_attempt_at": row.try_get::<i64>("", "next_attempt_at").unwrap(),
                "lease_id": row.try_get::<Option<String>>("", "lease_id").unwrap(),
                "lease_until": row.try_get::<i64>("", "lease_until").unwrap(),
                "category": row.try_get::<String>("", "category").unwrap(),
            })
        })
        .collect()
    }

    #[tokio::test]
    async fn adds_missing_category_and_preserves_pending_work_and_receipts() {
        use crate::entity::mobile_push_outbox as outbox;
        let db = before_category_migration().await;
        let manager = SchemaManager::new(&db);
        assert!(
            !manager
                .has_column("mobile_push_outbox", "category")
                .await
                .unwrap()
        );
        Migration.up(&manager).await.expect("add category column");
        Migrator::up(&db, None)
            .await
            .expect("apply category migration");
        let rows = outbox::Entity::find()
            .order_by_asc(outbox::Column::EventId)
            .all(&db)
            .await
            .expect("read upgraded deliveries");
        assert_eq!(rows.len(), 2);
        let accepted = &rows[0];
        assert_eq!(accepted.event_id, "accepted-event");
        assert_eq!(accepted.category, "test");
        assert_eq!(accepted.outcome, "accepted");
        assert_eq!(accepted.registration_revision, 9);
        assert!(accepted.envelope.is_none());
        let pending = &rows[1];
        assert_eq!(pending.event_id, "pending-event");
        assert_eq!(pending.category, "test");
        assert_eq!(pending.installation_id, "installation-a");
        assert_eq!(pending.mobile_session_id, "session-a");
        assert_eq!(pending.registration_revision, 3);
        assert_eq!(pending.recipient_role, "admin");
        assert_eq!(pending.envelope.as_deref(), Some("encrypted-fixture"));
        assert_eq!(pending.outcome, "retryable");
        assert_eq!(pending.reason, "NetworkUnavailable");
        assert_eq!(pending.attempts, 2);
        assert_eq!(pending.created_at, 2000000000);
        assert_eq!(pending.expires_at, 2000001800);
        assert_eq!(pending.next_attempt_at, 2000000008);
        assert_eq!(pending.lease_id.as_deref(), Some("active-lease"));
        assert_eq!(pending.lease_until, 2000000030);
    }

    #[tokio::test]
    async fn reuses_existing_category_without_rewriting_deliveries_or_default() {
        use crate::entity::mobile_push_outbox as outbox;
        let db = before_category_migration().await;
        // Model the shared schema already supplied by another independently
        // registered migration, without importing its event logic or source.
        db.execute_unprepared(
            "ALTER TABLE mobile_push_outbox ADD COLUMN category TEXT NOT NULL DEFAULT 'test';
             UPDATE mobile_push_outbox SET category='alert' WHERE event_id='pending-event';
             UPDATE mobile_push_outbox SET category='security' WHERE event_id='accepted-event'",
        )
        .await
        .expect("apply existing shared column with queued categories");
        let before = delivery_snapshots(&db).await;
        Migration
            .up(&SchemaManager::new(&db))
            .await
            .expect("reuse existing category");
        Migrator::up(&db, None)
            .await
            .expect("reuse existing category");
        Migration
            .up(&SchemaManager::new(&db))
            .await
            .expect("safe repeated application");
        let after = delivery_snapshots(&db).await;
        assert_eq!(before, after, "every persisted value remains unchanged");
        db.execute_unprepared(
            "INSERT INTO mobile_push_outbox
             (event_id, installation_id, user_id, mobile_session_id,
              registration_revision, recipient_role, created_at, expires_at, next_attempt_at)
             VALUES ('later-test', 'installation-c', 'user-c', 'session-c', 1, 'member',
                     2000000200, 2000002000, 2000000200)",
        )
        .await
        .expect("enqueue with preserved legacy default");
        let later =
            outbox::Entity::find_by_id(("later-test".to_owned(), "installation-c".to_owned()))
                .one(&db)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(later.category, "test");
    }

    #[tokio::test]
    async fn propagates_missing_table_failure() {
        let db = before_category_migration().await;
        db.execute_unprepared("DROP TABLE mobile_push_outbox")
            .await
            .expect("remove table to exercise a real schema failure");
        assert!(Migration.up(&SchemaManager::new(&db)).await.is_err());
    }
}
