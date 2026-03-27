use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m008_create_user_list_items"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(UserListItems::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(UserListItems::ListId).text().not_null())
                    .col(
                        ColumnDef::new(UserListItems::CameoMediaId)
                            .text()
                            .not_null(),
                    )
                    .col(ColumnDef::new(UserListItems::Position).double().not_null())
                    .col(
                        ColumnDef::new(UserListItems::AddedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(ColumnDef::new(UserListItems::Notes).text().null())
                    .primary_key(
                        Index::create()
                            .col(UserListItems::ListId)
                            .col(UserListItems::CameoMediaId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(UserListItems::Table, UserListItems::ListId)
                            .to(UserLists::Table, UserLists::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(UserListItems::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum UserListItems {
    Table,
    ListId,
    CameoMediaId,
    Position,
    AddedAt,
    Notes,
}

#[derive(Iden)]
enum UserLists {
    Table,
    Id,
}
