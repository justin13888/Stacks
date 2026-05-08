use std::collections::HashMap;
use std::sync::Arc;

use cameo::CameoClient;
use cameo::unified::DetailProvider;
use sea_orm::DatabaseConnection;

use crate::cameo::tmdb_numeric_id;

use super::models::UserStats;
use super::repo::{self, StatsError};

pub struct StatsService {
    pub db: DatabaseConnection,
    pub cameo: Arc<CameoClient>,
}

impl StatsService {
    pub fn new(db: DatabaseConnection, cameo: Arc<CameoClient>) -> Self {
        Self { db, cameo }
    }

    pub async fn get_user_stats(&self, user_id: &str) -> Result<UserStats, StatsError> {
        let mut stats = repo::compute_user_stats(&self.db, user_id).await?;
        stats.genre_breakdown = self.compute_genre_breakdown(user_id).await?;
        Ok(stats)
    }

    async fn compute_genre_breakdown(
        &self,
        user_id: &str,
    ) -> Result<Vec<(String, u32)>, StatsError> {
        let media_ids = repo::distinct_user_media_ids(&self.db, user_id).await?;
        let mut counts: HashMap<String, u32> = HashMap::new();
        for cameo_id in media_ids.iter().take(200) {
            let Some(tmdb) = tmdb_numeric_id(cameo_id) else {
                continue;
            };
            // Best-effort: ignore lookup failures so a single bad ID doesn't
            // poison the whole stats response.
            if let Ok(details) = self.cameo.movie_details(tmdb).await {
                for g in details.movie.genres {
                    *counts.entry(g.name().to_string()).or_insert(0) += 1;
                }
                continue;
            }
            if let Ok(details) = self.cameo.tv_show_details(tmdb).await {
                for g in details.show.genres {
                    *counts.entry(g.name().to_string()).or_insert(0) += 1;
                }
            }
        }
        let mut breakdown: Vec<(String, u32)> = counts.into_iter().collect();
        breakdown.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(breakdown)
    }
}
