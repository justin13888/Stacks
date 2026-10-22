use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m018_add_api_token_lookup"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Argon2 hashes are salted, so they cannot be searched. Without a
        // deterministic index every request had to load *every* token row and
        // argon2-verify each one in turn. This column stores a plain SHA-256 of
        // the token so a single indexed lookup finds the candidate row; the
        // argon2 hash in `token_hash` still performs the actual verification.
        manager
            .alter_table(
                Table::alter()
                    .table(ApiTokens::Table)
                    .add_column(ColumnDef::new(ApiTokens::TokenLookup).text().null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .table(ApiTokens::Table)
                    .col(ApiTokens::TokenLookup)
                    .name("idx_api_tokens_token_lookup")
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .table(ApiTokens::Table)
                    .name("idx_api_tokens_token_lookup")
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ApiTokens::Table)
                    .drop_column(ApiTokens::TokenLookup)
                    .to_owned(),
            )
            .await
    }
}

#[derive(Iden)]
enum ApiTokens {
    Table,
    TokenLookup,
}
