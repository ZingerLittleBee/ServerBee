use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 1. status_page table
        manager
            .create_table(
                Table::create()
                    .table(StatusPage::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(StatusPage::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(StatusPage::Title).string().not_null())
                    .col(ColumnDef::new(StatusPage::Slug).string().not_null())
                    .col(ColumnDef::new(StatusPage::Description).text().null())
                    .col(
                        ColumnDef::new(StatusPage::ServerIdsJson)
                            .text()
                            .not_null()
                            .default("[]"),
                    )
                    .col(
                        ColumnDef::new(StatusPage::GroupByServerGroup)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(StatusPage::ShowValues)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(ColumnDef::new(StatusPage::CustomCss).text().null())
                    .col(
                        ColumnDef::new(StatusPage::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(StatusPage::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(StatusPage::UpdatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // UNIQUE index on slug
        manager
            .create_index(
                Index::create()
                    .name("idx_status_page_slug_unique")
                    .table(StatusPage::Table)
                    .col(StatusPage::Slug)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // 2. incident table
        manager
            .create_table(
                Table::create()
                    .table(Incident::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Incident::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Incident::Title).string().not_null())
                    .col(ColumnDef::new(Incident::Status).string().not_null())
                    .col(ColumnDef::new(Incident::Severity).string().not_null())
                    .col(ColumnDef::new(Incident::ServerIdsJson).text().null())
                    .col(ColumnDef::new(Incident::StatusPageIdsJson).text().null())
                    .col(
                        ColumnDef::new(Incident::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Incident::UpdatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(Incident::ResolvedAt).timestamp().null())
                    .to_owned(),
            )
            .await?;

        // INDEX on status (filter unresolved)
        manager
            .create_index(
                Index::create()
                    .name("idx_incident_status")
                    .table(Incident::Table)
                    .col(Incident::Status)
                    .to_owned(),
            )
            .await?;

        // 3. incident_update table
        manager
            .create_table(
                Table::create()
                    .table(IncidentUpdate::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(IncidentUpdate::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(IncidentUpdate::IncidentId).string().not_null())
                    .col(ColumnDef::new(IncidentUpdate::Status).string().not_null())
                    .col(ColumnDef::new(IncidentUpdate::Message).text().not_null())
                    .col(
                        ColumnDef::new(IncidentUpdate::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(IncidentUpdate::Table, IncidentUpdate::IncidentId)
                            .to(Incident::Table, Incident::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // INDEX on incident_id
        manager
            .create_index(
                Index::create()
                    .name("idx_incident_update_incident_id")
                    .table(IncidentUpdate::Table)
                    .col(IncidentUpdate::IncidentId)
                    .to_owned(),
            )
            .await?;

        // 4. maintenance table
        manager
            .create_table(
                Table::create()
                    .table(Maintenance::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Maintenance::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Maintenance::Title).string().not_null())
                    .col(ColumnDef::new(Maintenance::Description).text().null())
                    .col(ColumnDef::new(Maintenance::StartAt).timestamp().not_null())
                    .col(ColumnDef::new(Maintenance::EndAt).timestamp().not_null())
                    .col(ColumnDef::new(Maintenance::ServerIdsJson).text().null())
                    .col(ColumnDef::new(Maintenance::StatusPageIdsJson).text().null())
                    .col(
                        ColumnDef::new(Maintenance::Active)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Maintenance::CreatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Maintenance::UpdatedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Index on maintenance for is_in_maintenance() query
        manager
            .create_index(
                Index::create()
                    .name("idx_maintenance_active_time")
                    .table(Maintenance::Table)
                    .col(Maintenance::Active)
                    .col(Maintenance::StartAt)
                    .col(Maintenance::EndAt)
                    .to_owned(),
            )
            .await?;

        // 5. uptime_daily table
        manager
            .create_table(
                Table::create()
                    .table(UptimeDaily::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UptimeDaily::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(UptimeDaily::ServerId).string().not_null())
                    .col(ColumnDef::new(UptimeDaily::Date).date().not_null())
                    .col(ColumnDef::new(UptimeDaily::TotalMinutes).integer().not_null())
                    .col(ColumnDef::new(UptimeDaily::OnlineMinutes).integer().not_null())
                    .col(ColumnDef::new(UptimeDaily::DowntimeIncidents).integer().not_null())
                    .to_owned(),
            )
            .await?;

        // UNIQUE index on (server_id, date) for upsert
        manager
            .create_index(
                Index::create()
                    .name("idx_uptime_daily_unique")
                    .table(UptimeDaily::Table)
                    .col(UptimeDaily::ServerId)
                    .col(UptimeDaily::Date)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Not reversible: dropping tables could lose data.
        Ok(())
    }
}

#[derive(Iden)]
enum StatusPage {
    Table,
    Id,
    Title,
    Slug,
    Description,
    ServerIdsJson,
    GroupByServerGroup,
    ShowValues,
    CustomCss,
    Enabled,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum Incident {
    Table,
    Id,
    Title,
    Status,
    Severity,
    ServerIdsJson,
    StatusPageIdsJson,
    CreatedAt,
    UpdatedAt,
    ResolvedAt,
}

#[derive(Iden)]
enum IncidentUpdate {
    Table,
    Id,
    IncidentId,
    Status,
    Message,
    CreatedAt,
}

#[derive(Iden)]
enum Maintenance {
    Table,
    Id,
    Title,
    Description,
    StartAt,
    EndAt,
    ServerIdsJson,
    StatusPageIdsJson,
    Active,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum UptimeDaily {
    Table,
    Id,
    ServerId,
    Date,
    TotalMinutes,
    OnlineMinutes,
    DowntimeIncidents,
}
