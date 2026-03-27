use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m013_create_server_library_items"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ServerLibraryItems::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ServerLibraryItems::Id)
                            .text()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ServerLibraryItems::ServerId)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ServerLibraryItems::CameoMediaId)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ServerLibraryItems::ServerInternalId)
                            .text()
                            .not_null(),
                    )
                    .col(ColumnDef::new(ServerLibraryItems::Title).text().not_null())
                    .col(ColumnDef::new(ServerLibraryItems::Quality).text().null())
                    .col(
                        ColumnDef::new(ServerLibraryItems::DeepLink)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ServerLibraryItems::PlaybackLink)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ServerLibraryItems::ExternalIds)
                            .json_binary()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(ServerLibraryItems::LastSeenAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(ServerLibraryItems::Table, ServerLibraryItems::ServerId)
                            .to(StreamingServers::Table, StreamingServers::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(ServerLibraryItems::Table)
                    .col(ServerLibraryItems::ServerId)
                    .col(ServerLibraryItems::ServerInternalId)
                    .unique()
                    .name("uq_lib_items_server_internal")
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(ServerLibraryItems::Table)
                    .col(ServerLibraryItems::CameoMediaId)
                    .name("idx_lib_items_cameo")
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ServerLibraryItems::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum ServerLibraryItems {
    Table,
    Id,
    ServerId,
    CameoMediaId,
    ServerInternalId,
    Title,
    Quality,
    DeepLink,
    PlaybackLink,
    ExternalIds,
    LastSeenAt,
}

#[derive(Iden)]
enum StreamingServers {
    Table,
    Id,
}
