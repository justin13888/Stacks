use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m015_create_notification_preferences"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(NotificationPreferences::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(NotificationPreferences::UserId)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(NotificationPreferences::EventType)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(NotificationPreferences::ChannelId)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(NotificationPreferences::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(NotificationPreferences::Config)
                            .json_binary()
                            .null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(NotificationPreferences::UserId)
                            .col(NotificationPreferences::EventType)
                            .col(NotificationPreferences::ChannelId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                NotificationPreferences::Table,
                                NotificationPreferences::ChannelId,
                            )
                            .to(NotificationChannels::Table, NotificationChannels::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(NotificationPreferences::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(Iden)]
enum NotificationPreferences {
    Table,
    UserId,
    EventType,
    ChannelId,
    Enabled,
    Config,
}

#[derive(Iden)]
enum NotificationChannels {
    Table,
    Id,
}
