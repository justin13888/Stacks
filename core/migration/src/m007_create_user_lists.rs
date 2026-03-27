use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m007_create_user_lists"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserLists::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(UserLists::Id)
                            .text()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(UserLists::UserId).text().not_null())
                    .col(ColumnDef::new(UserLists::Name).text().not_null())
                    .col(ColumnDef::new(UserLists::Description).text().null())
                    .col(
                        ColumnDef::new(UserLists::IsSmart)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(UserLists::FilterQuery).json_binary().null())
                    .col(
                        ColumnDef::new(UserLists::SortOrder)
                            .text()
                            .null()
                            .default("position"),
                    )
                    .col(
                        ColumnDef::new(UserLists::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(UserLists::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserLists::Table, UserLists::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserLists::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum UserLists {
    Table,
    Id,
    UserId,
    Name,
    Description,
    IsSmart,
    FilterQuery,
    SortOrder,
    CreatedAt,
    UpdatedAt,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
