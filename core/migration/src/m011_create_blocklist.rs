use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m011_create_blocklist"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Blocklist::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Blocklist::UserId).text().not_null())
                    .col(ColumnDef::new(Blocklist::CameoMediaId).text().not_null())
                    .col(
                        ColumnDef::new(Blocklist::AddedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .primary_key(
                        Index::create()
                            .col(Blocklist::UserId)
                            .col(Blocklist::CameoMediaId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Blocklist::Table, Blocklist::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Blocklist::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum Blocklist {
    Table,
    UserId,
    CameoMediaId,
    AddedAt,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
