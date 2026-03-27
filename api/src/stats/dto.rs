use salvo::oapi::ToSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StatsResponse {
    pub total_watch_time_minutes: i64,
    pub genre_breakdown: Vec<serde_json::Value>,
    pub yearly_counts: Vec<serde_json::Value>,
    pub monthly_heatmap: Vec<serde_json::Value>,
}
