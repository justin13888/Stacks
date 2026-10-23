pub use sea_orm_migration::prelude::*;

// Append-only after v0.1.0: add new migrations after the baseline, never edit
// one that has shipped. See the header of `m20261101_000001_baseline`.
mod m20261101_000001_baseline;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20261101_000001_baseline::Migration)]
    }
}
