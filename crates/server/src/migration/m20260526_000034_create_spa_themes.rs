use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum SpaThemes {
    Table,
    Id,
    Uuid,
    ManifestId,
    Name,
    Version,
    Author,
    Description,
    ManifestJson,
    PackageData,
    PreviewData,
    PreviewMime,
    SizeBytes,
    UploadedBy,
    UploadedAt,
    IsSuperseded,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(SpaThemes::Table)
                    .col(ColumnDef::new(SpaThemes::Id).integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(SpaThemes::Uuid).text().not_null().unique_key())
                    .col(ColumnDef::new(SpaThemes::ManifestId).text().not_null())
                    .col(ColumnDef::new(SpaThemes::Name).text().not_null())
                    .col(ColumnDef::new(SpaThemes::Version).text().not_null())
                    .col(ColumnDef::new(SpaThemes::Author).text())
                    .col(ColumnDef::new(SpaThemes::Description).text())
                    .col(ColumnDef::new(SpaThemes::ManifestJson).text().not_null())
                    .col(ColumnDef::new(SpaThemes::PackageData).blob().not_null())
                    .col(ColumnDef::new(SpaThemes::PreviewData).blob())
                    .col(ColumnDef::new(SpaThemes::PreviewMime).text())
                    .col(ColumnDef::new(SpaThemes::SizeBytes).big_integer().not_null())
                    .col(ColumnDef::new(SpaThemes::UploadedBy).text().not_null())
                    .col(
                        ColumnDef::new(SpaThemes::UploadedAt)
                            .timestamp()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(SpaThemes::IsSuperseded).integer().not_null().default(0))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_spa_themes_manifest_id_version")
                    .table(SpaThemes::Table)
                    .col(SpaThemes::ManifestId)
                    .col(SpaThemes::Version)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_spa_themes_uploaded_at")
                    .table(SpaThemes::Table)
                    .col(SpaThemes::UploadedAt)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
