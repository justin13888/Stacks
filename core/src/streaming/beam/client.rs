use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::streaming::StreamingError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeamLibraryItem {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub media_type: String,
    pub provider_ids: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct BeamClient {
    pub base_url: String,
    pub api_key: String,
    http: Client,
}

impl BeamClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url,
            api_key,
            http: Client::new(),
        }
    }

    pub async fn get_library(&self) -> Result<Vec<BeamLibraryItem>, StreamingError> {
        todo!()
    }

    pub async fn get_playback_url(
        &self,
        item_id: &str,
        episode_id: Option<&str>,
    ) -> Result<String, StreamingError> {
        let _ = (item_id, episode_id);
        todo!()
    }

    pub async fn ping(&self) -> Result<String, StreamingError> {
        todo!()
    }
}
