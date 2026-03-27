use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m009_create_user_tags"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserTags::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserTags::Id)
                            .text()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(UserTags::UserId).text().not_null())
                    .col(ColumnDef::new(UserTags::Name).text().not_null())
                    .col(ColumnDef::new(UserTags::Color).text().null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserTags::Table, UserTags::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(UserTags::Table)
                    .col(UserTags::UserId)
                    .col(UserTags::Name)
                    .unique()
                    .name("uq_user_tags_user_name")
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserTags::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum UserTags {
    Table,
    Id,
    UserId,
    Name,
    Color,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
