use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m006_create_watch_history"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(WatchHistory::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WatchHistory::Id)
                            .text()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(WatchHistory::UserId).text().not_null())
                    .col(ColumnDef::new(WatchHistory::CameoMediaId).text().not_null())
                    .col(ColumnDef::new(WatchHistory::CameoEpisodeId).text().null())
                    .col(ColumnDef::new(WatchHistory::Event).text().not_null())
                    .col(
                        ColumnDef::new(WatchHistory::PositionSeconds)
                            .integer()
                            .null(),
                    )
                    .col(ColumnDef::new(WatchHistory::Metadata).json_binary().null())
                    .col(
                        ColumnDef::new(WatchHistory::Source)
                            .text()
                            .not_null()
                            .default("manual"),
                    )
                    .col(
                        ColumnDef::new(WatchHistory::Timestamp)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(WatchHistory::Table, WatchHistory::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(WatchHistory::Table)
                    .col(WatchHistory::UserId)
                    .col(WatchHistory::Timestamp)
                    .name("idx_history_user_time")
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(WatchHistory::Table)
                    .col(WatchHistory::UserId)
                    .col(WatchHistory::CameoMediaId)
                    .name("idx_history_user_media")
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WatchHistory::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum WatchHistory {
    Table,
    Id,
    UserId,
    CameoMediaId,
    CameoEpisodeId,
    Event,
    PositionSeconds,
    Metadata,
    Source,
    Timestamp,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
