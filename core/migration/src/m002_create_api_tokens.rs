use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m002_create_api_tokens"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ApiTokens::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(ApiTokens::Id).text().not_null().primary_key())
                    .col(ColumnDef::new(ApiTokens::UserId).text().not_null())
                    .col(ColumnDef::new(ApiTokens::TokenHash).text().not_null())
                    .col(ColumnDef::new(ApiTokens::Label).text().not_null())
                    .col(
                        ColumnDef::new(ApiTokens::Scopes)
                            .json_binary()
                            .not_null()
                            .default(r#"["read"]"#),
                    )
                    .col(
                        ColumnDef::new(ApiTokens::ExpiresAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ApiTokens::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(ApiTokens::Table, ApiTokens::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ApiTokens::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum ApiTokens {
    Table,
    Id,
    UserId,
    TokenHash,
    Label,
    Scopes,
    ExpiresAt,
    CreatedAt,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
