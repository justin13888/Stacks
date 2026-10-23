use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

use super::enums::{JobStatus, JobType};

/// A background job. Claimed with `FOR UPDATE SKIP LOCKED`
/// (see [`crate::jobs::repo::claim_due_jobs`]).
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "jobs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub job_type: JobType,
    pub payload: Option<Json>,
    pub status: JobStatus,
    pub attempts: i32,
    pub max_attempts: i32,
    pub run_at: DateTimeWithTimeZone,
    pub started_at: Option<DateTimeWithTimeZone>,
    pub completed_at: Option<DateTimeWithTimeZone>,
    pub error: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
