use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilterQuery {
    pub media_type: Option<Vec<String>>,
    pub genre: Option<Vec<String>>,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub status: Option<Vec<String>>,
    pub available_on_beam: Option<bool>,
    pub score_min: Option<i32>,
    pub score_max: Option<i32>,
    pub sort: Option<String>,
}

impl FilterQuery {
    pub fn is_empty(&self) -> bool {
        self.media_type.is_none()
            && self.genre.is_none()
            && self.year_min.is_none()
            && self.year_max.is_none()
            && self.status.is_none()
            && self.available_on_beam.is_none()
            && self.score_min.is_none()
            && self.score_max.is_none()
    }
}
