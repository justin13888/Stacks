use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m017_create_oidc_auth_sessions"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(OidcAuthSessions::Table)
                    .if_not_exists()
                    // The `state` parameter doubles as the primary key: it is a
                    // 32-byte CSPRNG value and must be single-use.
                    .col(
                        ColumnDef::new(OidcAuthSessions::State)
                            .text()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(OidcAuthSessions::Nonce).text().not_null())
                    .col(
                        ColumnDef::new(OidcAuthSessions::ExpiresAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(OidcAuthSessions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Supports the best-effort sweep of expired rows on each login.
        manager
            .create_index(
                Index::create()
                    .table(OidcAuthSessions::Table)
                    .col(OidcAuthSessions::ExpiresAt)
                    .name("idx_oidc_auth_sessions_expires_at")
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(OidcAuthSessions::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum OidcAuthSessions {
    Table,
    State,
    Nonce,
    ExpiresAt,
    CreatedAt,
}
