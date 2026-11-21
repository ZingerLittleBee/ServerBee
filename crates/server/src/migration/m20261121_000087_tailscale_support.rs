use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        for column in [
            "tailscale_ipv4",
            "tailscale_ipv6",
            "tailscale_dns_name",
            "tailscale_tailnet",
        ] {
            db.execute_unprepared(&format!("ALTER TABLE servers ADD COLUMN {column} TEXT"))
                .await?;
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
