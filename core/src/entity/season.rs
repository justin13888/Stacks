use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// A season of a show. Specials are season 0.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "seasons")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub media_id: Uuid,
    pub number: i32,
    pub title: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<Date>,
    pub poster_url: Option<String>,
    pub episode_count: Option<i32>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
