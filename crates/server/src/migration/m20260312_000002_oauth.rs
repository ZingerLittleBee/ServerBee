use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260312_000002_oauth"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(OauthAccounts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(OauthAccounts::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(OauthAccounts::UserId).string().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(OauthAccounts::Table, OauthAccounts::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(ColumnDef::new(OauthAccounts::Provider).string().not_null())
                    .col(
                        ColumnDef::new(OauthAccounts::ProviderUserId)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(OauthAccounts::Email).string().null())
                    .col(ColumnDef::new(OauthAccounts::DisplayName).string().null())
                    .col(
                        ColumnDef::new(OauthAccounts::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        // Unique constraint: (provider, provider_user_id)
        manager
            .create_index(
                Index::create()
                    .name("idx_oauth_provider_user")
                    .table(OauthAccounts::Table)
                    .col(OauthAccounts::Provider)
                    .col(OauthAccounts::ProviderUserId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Index on user_id for lookups
        manager
            .create_index(
                Index::create()
                    .name("idx_oauth_user_id")
                    .table(OauthAccounts::Table)
                    .col(OauthAccounts::UserId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(OauthAccounts::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum OauthAccounts {
    Table,
    Id,
    UserId,
    Provider,
    ProviderUserId,
    Email,
    DisplayName,
    CreatedAt,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
