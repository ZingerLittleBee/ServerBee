use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for (column, sql_type) in [
            ("admission_payload", "TEXT"),
            ("push_intent", "TEXT"),
            ("authority_fingerprint", "TEXT"),
            ("maintenance_at_admission", "BOOLEAN"),
        ] {
            if !manager.has_column("security_event", column).await? {
                db.execute_unprepared(&format!(
                    "ALTER TABLE security_event ADD COLUMN {column} {sql_type}"
                ))
                .await?;
            }
        }
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_security_event_pending_admission
             ON security_event(created_at, id)
             WHERE admission_payload IS NOT NULL OR push_intent IS NOT NULL",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_security_event_pending_key
             ON security_event(server_id, event_type, source_ip, created_at, id)
             WHERE admission_payload IS NOT NULL",
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
    use crate::{entity::security_event, migration::Migrator};
    use sea_orm::{Database, EntityTrait};

    #[tokio::test]
    async fn preserves_raw_history_without_replaying_legacy_events() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        let predecessor_count = Migrator::migrations()
            .iter()
            .position(|m| m.name() == "m20261003_000085_security_event_admission")
            .unwrap();
        Migrator::up(&db, Some(predecessor_count as u32))
            .await
            .unwrap();
        db.execute_unprepared(
            "INSERT INTO security_event
             (id, server_id, event_type, severity, source_ip, started_at,
              ended_at, first_seen, detector_source, evidence, created_at)
             VALUES ('legacy-event', 'server-a', 'ssh_login', 'high', '203.0.113.8',
                     CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1, 'journal',
                     '{\"kind\":\"ssh_login\",\"auth_method\":\"publickey\"}', CURRENT_TIMESTAMP)",
        )
        .await
        .unwrap();
        Migrator::up(&db, None).await.unwrap();
        Migration.up(&SchemaManager::new(&db)).await.unwrap();
        let rows = security_event::Entity::find().all(&db).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "legacy-event");
        assert_eq!(rows[0].source_ip, "203.0.113.8");
        assert!(rows[0].first_seen);
        assert!(rows[0].evidence.contains("publickey"));
        assert!(rows[0].admission_payload.is_none());
        assert!(rows[0].push_intent.is_none());
        assert!(rows[0].authority_fingerprint.is_none());
        assert!(rows[0].maintenance_at_admission.is_none());
        Migration.down(&SchemaManager::new(&db)).await.unwrap();
        assert_eq!(security_event::Entity::find().all(&db).await.unwrap(), rows);
    }
}
