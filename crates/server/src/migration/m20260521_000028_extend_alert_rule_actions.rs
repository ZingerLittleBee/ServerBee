use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260521_000028_extend_alert_rule_actions"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        // SQLite: ALTER TABLE … ADD COLUMN is idempotent only via try-then-ignore.
        // The column is nullable so we can add it unconditionally; if it exists
        // we swallow the duplicate-column error.
        let res = db
            .execute_unprepared("ALTER TABLE alert_rules ADD COLUMN actions_json TEXT")
            .await;
        match res {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("duplicate column name") {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
