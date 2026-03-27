use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m012_create_streaming_servers"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(StreamingServers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(StreamingServers::Id)
                            .text()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(StreamingServers::Name).text().not_null())
                    .col(
                        ColumnDef::new(StreamingServers::ServerType)
                            .text()
                            .not_null()
                            .default("beam"),
                    )
                    .col(ColumnDef::new(StreamingServers::BaseUrl).text().not_null())
                    .col(ColumnDef::new(StreamingServers::ApiKeyEnc).binary().null())
                    .col(ColumnDef::new(StreamingServers::UserId).text().null())
                    .col(ColumnDef::new(StreamingServers::WebhookSecret).text().null())
                    .col(
                        ColumnDef::new(StreamingServers::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(StreamingServers::LastSyncedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(StreamingServers::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(StreamingServers::Table, StreamingServers::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(StreamingServers::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum StreamingServers {
    Table,
    Id,
    Name,
    ServerType,
    BaseUrl,
    ApiKeyEnc,
    UserId,
    WebhookSecret,
    Enabled,
    LastSyncedAt,
    CreatedAt,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
