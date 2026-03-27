pub use sea_orm_migration::prelude::*;

mod m001_create_users;
mod m002_create_api_tokens;
mod m003_create_refresh_tokens;
mod m004_create_watch_status;
mod m005_create_episode_watch_status;
mod m006_create_watch_history;
mod m007_create_user_lists;
mod m008_create_user_list_items;
mod m009_create_user_tags;
mod m010_create_media_tags;
mod m011_create_blocklist;
mod m012_create_streaming_servers;
mod m013_create_server_library_items;
mod m014_create_notification_channels;
mod m015_create_notification_preferences;
mod m016_create_jobs;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m001_create_users::Migration),
            Box::new(m002_create_api_tokens::Migration),
            Box::new(m003_create_refresh_tokens::Migration),
            Box::new(m004_create_watch_status::Migration),
            Box::new(m005_create_episode_watch_status::Migration),
            Box::new(m006_create_watch_history::Migration),
            Box::new(m007_create_user_lists::Migration),
            Box::new(m008_create_user_list_items::Migration),
            Box::new(m009_create_user_tags::Migration),
            Box::new(m010_create_media_tags::Migration),
            Box::new(m011_create_blocklist::Migration),
            Box::new(m012_create_streaming_servers::Migration),
            Box::new(m013_create_server_library_items::Migration),
            Box::new(m014_create_notification_channels::Migration),
            Box::new(m015_create_notification_preferences::Migration),
            Box::new(m016_create_jobs::Migration),
        ]
    }
}
