use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m004_create_watch_status"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(WatchStatus::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(WatchStatus::UserId).text().not_null())
                    .col(ColumnDef::new(WatchStatus::CameoMediaId).text().not_null())
                    .col(ColumnDef::new(WatchStatus::Status).text().not_null())
                    .col(ColumnDef::new(WatchStatus::Score).integer().null())
                    .col(
                        ColumnDef::new(WatchStatus::ScoreDate)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WatchStatus::StartedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WatchStatus::CompletedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WatchStatus::RepeatCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(WatchStatus::Notes).text().null())
                    .col(
                        ColumnDef::new(WatchStatus::IsFavorite)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(WatchStatus::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .primary_key(
                        Index::create()
                            .col(WatchStatus::UserId)
                            .col(WatchStatus::CameoMediaId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(WatchStatus::Table, WatchStatus::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(WatchStatus::Table)
                    .col(WatchStatus::UserId)
                    .col(WatchStatus::Status)
                    .name("idx_watch_status_user_status")
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(WatchStatus::Table)
                    .col(WatchStatus::UserId)
                    .col(WatchStatus::IsFavorite)
                    .name("idx_watch_status_user_favorite")
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WatchStatus::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum WatchStatus {
    Table,
    UserId,
    CameoMediaId,
    Status,
    Score,
    ScoreDate,
    StartedAt,
    CompletedAt,
    RepeatCount,
    Notes,
    IsFavorite,
    UpdatedAt,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
