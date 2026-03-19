use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create service_monitor table
        manager
            .create_table(
                Table::create()
                    .table(ServiceMonitor::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ServiceMonitor::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ServiceMonitor::Name).string().not_null())
                    .col(ColumnDef::new(ServiceMonitor::MonitorType).string().not_null())
                    .col(ColumnDef::new(ServiceMonitor::Target).string().not_null())
                    .col(
                        ColumnDef::new(ServiceMonitor::Interval)
                            .integer()
                            .not_null()
                            .default(300),
                    )
                    .col(
                        ColumnDef::new(ServiceMonitor::ConfigJson)
                            .text()
                            .not_null()
                            .default("{}"),
                    )
                    .col(ColumnDef::new(ServiceMonitor::NotificationGroupId).string().null())
                    .col(
                        ColumnDef::new(ServiceMonitor::RetryCount)
                            .integer()
                            .not_null()
                            .default(1),
                    )
                    .col(ColumnDef::new(ServiceMonitor::ServerIdsJson).text().null())
                    .col(
                        ColumnDef::new(ServiceMonitor::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(ColumnDef::new(ServiceMonitor::LastStatus).boolean().null())
                    .col(
                        ColumnDef::new(ServiceMonitor::ConsecutiveFailures)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(ServiceMonitor::LastCheckedAt).timestamp().null())
                    .col(
                        ColumnDef::new(ServiceMonitor::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(ServiceMonitor::UpdatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Create index on enabled column for filtering active monitors
        manager
            .create_index(
                Index::create()
                    .name("idx_service_monitor_enabled")
                    .table(ServiceMonitor::Table)
                    .col(ServiceMonitor::Enabled)
                    .to_owned(),
            )
            .await?;

        // Create service_monitor_record table
        manager
            .create_table(
                Table::create()
                    .table(ServiceMonitorRecord::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ServiceMonitorRecord::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ServiceMonitorRecord::MonitorId).string().not_null())
                    .col(ColumnDef::new(ServiceMonitorRecord::Success).boolean().not_null())
                    .col(ColumnDef::new(ServiceMonitorRecord::Latency).double().null())
                    .col(
                        ColumnDef::new(ServiceMonitorRecord::DetailJson)
                            .text()
                            .not_null()
                            .default("{}"),
                    )
                    .col(ColumnDef::new(ServiceMonitorRecord::Error).text().null())
                    .col(
                        ColumnDef::new(ServiceMonitorRecord::Time)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Create composite index on (monitor_id, time) for efficient record queries
        manager
            .create_index(
                Index::create()
                    .name("idx_service_monitor_record_monitor_time")
                    .table(ServiceMonitorRecord::Table)
                    .col(ServiceMonitorRecord::MonitorId)
                    .col(ServiceMonitorRecord::Time)
                    .to_owned(),
            )
            .await?;

        // Add last_remote_addr to servers table (for P11 IP change detection)
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("servers"))
                    .add_column(ColumnDef::new(Alias::new("last_remote_addr")).text().null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Not reversible: dropping tables and columns could lose data.
        Ok(())
    }
}

#[derive(Iden)]
enum ServiceMonitor {
    Table,
    Id,
    Name,
    MonitorType,
    Target,
    Interval,
    ConfigJson,
    NotificationGroupId,
    RetryCount,
    ServerIdsJson,
    Enabled,
    LastStatus,
    ConsecutiveFailures,
    LastCheckedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum ServiceMonitorRecord {
    Table,
    Id,
    MonitorId,
    Success,
    Latency,
    DetailJson,
    Error,
    Time,
}
