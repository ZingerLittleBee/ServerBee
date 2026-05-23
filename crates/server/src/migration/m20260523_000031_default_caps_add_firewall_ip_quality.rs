use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260523_000031_default_caps_add_firewall_ip_quality"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // CAP_DEFAULT now adds CAP_FIREWALL_BLOCK (512) and CAP_IP_QUALITY (1024)
        // on top of the previous default mask (316), giving 1852. Promote any
        // server row that still carries exactly the previous default to the new
        // default. Custom masks are left untouched so administrators who opted
        // out of any default bit keep their choice.
        let db = manager.get_connection();
        db.execute_unprepared(
            "UPDATE servers SET capabilities = 1852 WHERE capabilities = 316",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
