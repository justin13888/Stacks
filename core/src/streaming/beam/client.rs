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

    fn url(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        format!("{base}{path}")
    }

    pub async fn get_library(&self) -> Result<Vec<BeamLibraryItem>, StreamingError> {
        let res = self
            .http
            .get(self.url("/api/v1/library"))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(StreamingError::Upstream(format!(
                "beam library: status {}",
                res.status()
            )));
        }
        let items: Vec<BeamLibraryItem> = res.json().await?;
        Ok(items)
    }

    pub async fn get_playback_url(
        &self,
        item_id: &str,
        episode_id: Option<&str>,
    ) -> Result<String, StreamingError> {
        let path = match episode_id {
            Some(ep) => format!("/api/v1/items/{item_id}/episodes/{ep}/playback"),
            None => format!("/api/v1/items/{item_id}/playback"),
        };
        let res = self
            .http
            .get(self.url(&path))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(StreamingError::Upstream(format!(
                "beam playback: status {}",
                res.status()
            )));
        }
        #[derive(Deserialize)]
        struct PlaybackResponse {
            url: String,
        }
        let body: PlaybackResponse = res.json().await?;
        Ok(body.url)
    }

    pub async fn ping(&self) -> Result<String, StreamingError> {
        let res = self
            .http
            .get(self.url("/api/v1/health"))
            .send()
            .await?;
        if !res.status().is_success() {
            return Err(StreamingError::Upstream(format!(
                "beam ping: status {}",
                res.status()
            )));
        }
        let text = res.text().await.unwrap_or_default();
        Ok(text)
    }
}
