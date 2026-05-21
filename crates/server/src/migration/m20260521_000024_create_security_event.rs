use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260521_000024_create_security_event"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE IF NOT EXISTS security_event (
                id TEXT PRIMARY KEY NOT NULL,
                server_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                severity TEXT NOT NULL,
                source_ip TEXT NOT NULL,
                source_port INTEGER,
                username TEXT,
                started_at TIMESTAMP WITH TIME ZONE NOT NULL,
                ended_at TIMESTAMP WITH TIME ZONE NOT NULL,
                first_seen BOOLEAN NOT NULL DEFAULT 0,
                detector_source TEXT NOT NULL,
                evidence TEXT NOT NULL,
                created_at TIMESTAMP WITH TIME ZONE NOT NULL
            )",
        )
        .await?;

        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_security_event_server_id_created_at
                ON security_event(server_id, created_at)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_security_event_source_ip_created_at
                ON security_event(source_ip, created_at)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_security_event_event_type_created_at
                ON security_event(event_type, created_at)",
        )
        .await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_security_event_dedupe
                ON security_event(server_id, event_type, source_ip, started_at)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
