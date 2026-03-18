use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Create docker_event table
        manager
            .create_table(
                Table::create()
                    .table(DockerEvent::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(DockerEvent::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(DockerEvent::ServerId).string().not_null())
                    .col(ColumnDef::new(DockerEvent::Timestamp).big_integer().not_null())
                    .col(ColumnDef::new(DockerEvent::EventType).string().not_null())
                    .col(ColumnDef::new(DockerEvent::Action).string().not_null())
                    .col(ColumnDef::new(DockerEvent::ActorId).string().not_null())
                    .col(ColumnDef::new(DockerEvent::ActorName).string().null())
                    .col(ColumnDef::new(DockerEvent::Attributes).text().null())
                    .col(
                        ColumnDef::new(DockerEvent::CreatedAt)
                            .timestamp()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Create index for querying events by server and time
        manager
            .create_index(
                Index::create()
                    .name("idx_docker_event_server_time")
                    .table(DockerEvent::Table)
                    .col(DockerEvent::ServerId)
                    .col(DockerEvent::Timestamp)
                    .to_owned(),
            )
            .await?;

        // Add features column to servers table
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("servers"))
                    .add_column(ColumnDef::new(Alias::new("features")).text().default("[]"))
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
enum DockerEvent {
    Table,
    Id,
    ServerId,
    Timestamp,
    EventType,
    Action,
    ActorId,
    ActorName,
    Attributes,
    CreatedAt,
}
