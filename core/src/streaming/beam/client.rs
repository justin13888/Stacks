use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::streaming::StreamingError;

/// One item in a Beam server's library, as returned by `GET /api/library`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeamLibraryItem {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub media_type: String,
    /// External metadata-provider ids (e.g. `{"tmdb": 550}`), used to match the
    /// item back to a canonical Stacks media id.
    pub provider_ids: Option<serde_json::Value>,
}

/// Playback descriptor returned by `GET /api/library/{id}/playback`.
#[derive(Debug, Clone, Deserialize)]
struct BeamPlayback {
    url: String,
}

/// Health descriptor returned by `GET /api/health`.
#[derive(Debug, Clone, Deserialize)]
struct BeamHealth {
    #[serde(default)]
    version: Option<String>,
}

/// HTTP client for a single Beam media server.
///
/// Beam is the user's own self-hosted media server. This client speaks a small
/// REST contract against it:
///
/// - `GET {base_url}/api/library`               — the full library
/// - `GET {base_url}/api/library/{id}/playback` — a playback URL for an item
/// - `GET {base_url}/api/health`                — liveness + version
///
/// Every request authenticates with `Authorization: Bearer {api_key}`.
#[derive(Debug, Clone)]
pub struct BeamClient {
    pub base_url: String,
    pub api_key: String,
    http: Client,
}

impl BeamClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            http,
        }
    }

    /// Build an authenticated GET request for a Beam API path.
    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.api_key)
    }

    /// Fetch the server's full library.
    pub async fn get_library(&self) -> Result<Vec<BeamLibraryItem>, StreamingError> {
        let resp = ensure_ok(self.get("/api/library").send().await?)?;
        Ok(resp.json().await?)
    }

    /// Fetch a playback URL for a library item (optionally a specific episode).
    pub async fn get_playback_url(
        &self,
        item_id: &str,
        episode_id: Option<&str>,
    ) -> Result<String, StreamingError> {
        let mut req = self.get(&format!("/api/library/{item_id}/playback"));
        if let Some(episode) = episode_id {
            req = req.query(&[("episode", episode)]);
        }
        let resp = ensure_ok(req.send().await?)?;
        let playback: BeamPlayback = resp.json().await?;
        Ok(playback.url)
    }

    /// Check that the server is reachable, returning its reported version.
    pub async fn ping(&self) -> Result<String, StreamingError> {
        let resp = ensure_ok(self.get("/api/health").send().await?)?;
        let health: BeamHealth = resp.json().await?;
        Ok(health.version.unwrap_or_else(|| "unknown".to_string()))
    }
}

/// Map a non-2xx response to a typed error; pass 2xx responses through.
fn ensure_ok(resp: reqwest::Response) -> Result<reqwest::Response, StreamingError> {
    let status = resp.status();
    if status.is_success() {
        Ok(resp)
    } else if status == reqwest::StatusCode::UNAUTHORIZED
        || status == reqwest::StatusCode::FORBIDDEN
    {
        Err(StreamingError::AuthFailed)
    } else {
        Err(StreamingError::Upstream(format!("beam responded {status}")))
    }
}
