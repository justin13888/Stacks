use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m014_create_notification_channels"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(NotificationChannels::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NotificationChannels::Id)
                            .text()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(NotificationChannels::UserId)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(NotificationChannels::ChannelType)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(NotificationChannels::Name)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(NotificationChannels::Config)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(NotificationChannels::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(NotificationChannels::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(NotificationChannels::Table, NotificationChannels::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(NotificationChannels::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum NotificationChannels {
    Table,
    Id,
    UserId,
    ChannelType,
    Name,
    Config,
    Enabled,
    CreatedAt,
}

#[derive(Iden)]
enum Users {
    Table,
    Id,
}
