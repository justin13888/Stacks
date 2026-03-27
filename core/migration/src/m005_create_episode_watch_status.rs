use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m005_create_episode_watch_status"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(EpisodeWatchStatus::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(EpisodeWatchStatus::UserId).text().not_null())
                    .col(
                        ColumnDef::new(EpisodeWatchStatus::CameoEpisodeId)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EpisodeWatchStatus::CameoMediaId)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EpisodeWatchStatus::SeasonNumber)
                            .integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(EpisodeWatchStatus::EpisodeNumber)
                            .integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(EpisodeWatchStatus::Watched)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(EpisodeWatchStatus::WatchedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(EpisodeWatchStatus::ProgressSeconds)
                            .integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(EpisodeWatchStatus::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .primary_key(
                        Index::create()
                            .col(EpisodeWatchStatus::UserId)
                            .col(EpisodeWatchStatus::CameoEpisodeId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(EpisodeWatchStatus::Table, EpisodeWatchStatus::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(EpisodeWatchStatus::Table)
                    .col(EpisodeWatchStatus::UserId)
                    .col(EpisodeWatchStatus::CameoMediaId)
                    .name("idx_ep_status_media")
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(EpisodeWatchStatus::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum EpisodeWatchStatus {
    Table,
    UserId,
    CameoEpisodeId,
    CameoMediaId,
    SeasonNumber,
    EpisodeNumber,
    Watched,
    WatchedAt,
    ProgressSeconds,
    UpdatedAt,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
