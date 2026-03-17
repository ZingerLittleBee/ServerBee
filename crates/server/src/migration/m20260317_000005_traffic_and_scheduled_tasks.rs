use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 1. traffic_hourly
        manager
            .create_table(
                Table::create()
                    .table(TrafficHourly::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(TrafficHourly::Id).big_integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(TrafficHourly::ServerId).string().not_null())
                    .col(ColumnDef::new(TrafficHourly::Hour).timestamp_with_time_zone().not_null())
                    .col(ColumnDef::new(TrafficHourly::BytesIn).big_integer().not_null().default(0))
                    .col(ColumnDef::new(TrafficHourly::BytesOut).big_integer().not_null().default(0))
                    .foreign_key(
                        ForeignKey::create()
                            .from(TrafficHourly::Table, TrafficHourly::ServerId)
                            .to(Servers::Table, Servers::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_traffic_hourly_unique")
                    .table(TrafficHourly::Table)
                    .col(TrafficHourly::ServerId)
                    .col(TrafficHourly::Hour)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // 2. traffic_daily
        manager
            .create_table(
                Table::create()
                    .table(TrafficDaily::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(TrafficDaily::Id).big_integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(TrafficDaily::ServerId).string().not_null())
                    .col(ColumnDef::new(TrafficDaily::Date).date().not_null())
                    .col(ColumnDef::new(TrafficDaily::BytesIn).big_integer().not_null().default(0))
                    .col(ColumnDef::new(TrafficDaily::BytesOut).big_integer().not_null().default(0))
                    .foreign_key(
                        ForeignKey::create()
                            .from(TrafficDaily::Table, TrafficDaily::ServerId)
                            .to(Servers::Table, Servers::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_traffic_daily_unique")
                    .table(TrafficDaily::Table)
                    .col(TrafficDaily::ServerId)
                    .col(TrafficDaily::Date)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // 3. traffic_state
        manager
            .create_table(
                Table::create()
                    .table(TrafficState::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(TrafficState::ServerId).string().not_null().primary_key())
                    .col(ColumnDef::new(TrafficState::LastIn).big_integer().not_null().default(0))
                    .col(ColumnDef::new(TrafficState::LastOut).big_integer().not_null().default(0))
                    .col(ColumnDef::new(TrafficState::UpdatedAt).timestamp_with_time_zone().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(TrafficState::Table, TrafficState::ServerId)
                            .to(Servers::Table, Servers::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // 4. servers: add billing_start_day
        manager
            .alter_table(
                Table::alter()
                    .table(Servers::Table)
                    .add_column(ColumnDef::new(Servers::BillingStartDay).integer().null())
                    .to_owned(),
            )
            .await?;

        // CHECK constraint for billing_start_day (SQLite raw SQL)
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TRIGGER IF NOT EXISTS check_billing_start_day \
                 BEFORE INSERT ON servers \
                 FOR EACH ROW \
                 WHEN NEW.billing_start_day IS NOT NULL AND (NEW.billing_start_day < 1 OR NEW.billing_start_day > 28) \
                 BEGIN SELECT RAISE(ABORT, 'billing_start_day must be between 1 and 28'); END;"
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TRIGGER IF NOT EXISTS check_billing_start_day_update \
                 BEFORE UPDATE ON servers \
                 FOR EACH ROW \
                 WHEN NEW.billing_start_day IS NOT NULL AND (NEW.billing_start_day < 1 OR NEW.billing_start_day > 28) \
                 BEGIN SELECT RAISE(ABORT, 'billing_start_day must be between 1 and 28'); END;"
            )
            .await?;

        // 5. tasks: add scheduled task columns
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::TaskType).string().not_null().default("oneshot"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::Name).string().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::CronExpression).string().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::Enabled).boolean().not_null().default(true))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::Timeout).integer().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::RetryCount).integer().not_null().default(0))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::RetryInterval).integer().not_null().default(60))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::LastRunAt).timestamp_with_time_zone().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Tasks::Table)
                    .add_column(ColumnDef::new(Tasks::NextRunAt).timestamp_with_time_zone().null())
                    .to_owned(),
            )
            .await?;

        // 6. task_results: add run_id, attempt, started_at
        manager
            .alter_table(
                Table::alter()
                    .table(TaskResults::Table)
                    .add_column(ColumnDef::new(TaskResults::RunId).string().null())
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(TaskResults::Table)
                    .add_column(ColumnDef::new(TaskResults::Attempt).integer().not_null().default(1))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(TaskResults::Table)
                    .add_column(ColumnDef::new(TaskResults::StartedAt).timestamp_with_time_zone().null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(TrafficHourly::Table).to_owned()).await?;
        manager.drop_table(Table::drop().table(TrafficDaily::Table).to_owned()).await?;
        manager.drop_table(Table::drop().table(TrafficState::Table).to_owned()).await?;
        // Note: SQLite does not support DROP COLUMN; reverting column additions requires table rebuild.
        // For development, dropping and recreating the DB is acceptable.
        Ok(())
    }
}

// --- Iden enums ---

#[derive(Iden)]
enum TrafficHourly {
    Table,
    Id,
    ServerId,
    Hour,
    BytesIn,
    BytesOut,
}

#[derive(Iden)]
enum TrafficDaily {
    Table,
    Id,
    ServerId,
    Date,
    BytesIn,
    BytesOut,
}

#[derive(Iden)]
enum TrafficState {
    Table,
    ServerId,
    LastIn,
    LastOut,
    UpdatedAt,
}

#[derive(Iden)]
enum Servers {
    Table,
    Id,
    BillingStartDay,
}

#[derive(Iden)]
enum Tasks {
    Table,
    TaskType,
    Name,
    CronExpression,
    Enabled,
    Timeout,
    RetryCount,
    RetryInterval,
    LastRunAt,
    NextRunAt,
}

#[derive(Iden)]
enum TaskResults {
    Table,
    RunId,
    Attempt,
    StartedAt,
}
