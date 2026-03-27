use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m010_create_media_tags"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MediaTags::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(MediaTags::UserId).text().not_null())
                    .col(ColumnDef::new(MediaTags::CameoMediaId).text().not_null())
                    .col(ColumnDef::new(MediaTags::TagId).text().not_null())
                    .primary_key(
                        Index::create()
                            .col(MediaTags::UserId)
                            .col(MediaTags::CameoMediaId)
                            .col(MediaTags::TagId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(MediaTags::Table, MediaTags::TagId)
                            .to(UserTags::Table, UserTags::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(MediaTags::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum MediaTags {
    Table,
    UserId,
    CameoMediaId,
    TagId,
}

#[derive(Iden)]
enum UserTags {
    Table,
    Id,
}
