use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::enums::{Category, MediaFormat, MediaKind, ReleaseStatus};

/// The local catalog snapshot of one title.
///
/// The library renders from this row, not from a live provider call, so an
/// upstream outage cannot empty it. `raw` keeps the provider payload the
/// snapshot was built from.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "media")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub kind: MediaKind,
    pub format: Option<MediaFormat>,
    pub category: Category,
    pub title: String,
    pub original_title: Option<String>,
    pub romaji_title: Option<String>,
    pub alt_titles: Json,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
    pub backdrop_url: Option<String>,
    pub start_date: Option<Date>,
    pub end_date: Option<Date>,
    pub status: Option<ReleaseStatus>,
    pub runtime_minutes: Option<i32>,
    pub genres: Vec<String>,
    pub origin_countries: Vec<String>,
    pub original_language: Option<String>,
    pub season_count: Option<i32>,
    pub episode_count: Option<i32>,
    pub raw: Option<Json>,
    pub fetched_at: Option<DateTimeWithTimeZone>,
    pub refresh_error: Option<String>,
    pub refresh_failures: i32,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::media_ref::Entity")]
    Refs,
}

impl Related<super::media_ref::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Refs.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
