use std::collections::HashMap;

use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseConnection};
use ulid::Ulid;

use crate::crypto;
use crate::entity::{server_library_item, streaming_server};

use super::beam::BeamConnector;
use super::connector::StreamingConnector;
use super::matcher;
use super::models::{ConnectorEvent, ConnectorHealth, ConnectorLibraryItem};
use super::{StreamingError, repo};

/// Manages streaming servers (Beam connectors) and their cached libraries.
///
/// Server API keys are encrypted at rest with AES-256-GCM; the key is derived
/// from the application secret and supplied at construction time.
pub struct StreamingService {
    pub db: DatabaseConnection,
    encryption_key: [u8; 32],
}

impl StreamingService {
    pub fn new(db: DatabaseConnection, encryption_key: [u8; 32]) -> Self {
        Self { db, encryption_key }
    }

    pub async fn register_server(
        &self,
        user_id: Option<&str>,
        name: String,
        server_type: String,
        base_url: String,
        api_key: Option<String>,
        webhook_secret: Option<String>,
    ) -> Result<streaming_server::Model, StreamingError> {
        if server_type != "beam" {
            return Err(StreamingError::UnsupportedServerType(server_type));
        }
        let api_key_enc = match api_key {
            Some(key) if !key.is_empty() => {
                Some(crypto::encrypt(&self.encryption_key, key.as_bytes())?)
            }
            _ => None,
        };
        let model = streaming_server::ActiveModel {
            id: Set(Ulid::new().to_string()),
            name: Set(name),
            server_type: Set(server_type),
            base_url: Set(base_url.trim_end_matches('/').to_string()),
            api_key_enc: Set(api_key_enc),
            user_id: Set(user_id.map(str::to_string)),
            webhook_secret: Set(webhook_secret),
            enabled: Set(true),
            last_synced_at: Set(None),
            created_at: Set(Utc::now().into()),
        };
        repo::create_server(&self.db, model).await
    }

    pub async fn get_server(&self, id: &str) -> Result<streaming_server::Model, StreamingError> {
        repo::find_server_by_id(&self.db, id)
            .await?
            .ok_or(StreamingError::ServerNotFound)
    }

    pub async fn list_servers(
        &self,
        user_id: &str,
    ) -> Result<Vec<streaming_server::Model>, StreamingError> {
        repo::find_servers_for_user(&self.db, user_id).await
    }

    /// Every registered server — for the admin surface.
    pub async fn list_all_servers(&self) -> Result<Vec<streaming_server::Model>, StreamingError> {
        repo::find_all_servers(&self.db).await
    }

    pub async fn update_server(
        &self,
        id: &str,
        name: Option<String>,
        base_url: Option<String>,
        api_key: Option<String>,
        webhook_secret: Option<String>,
        enabled: Option<bool>,
    ) -> Result<streaming_server::Model, StreamingError> {
        let server = self.get_server(id).await?;
        let mut active: streaming_server::ActiveModel = server.into();
        if let Some(name) = name {
            active.name = Set(name);
        }
        if let Some(base_url) = base_url {
            active.base_url = Set(base_url.trim_end_matches('/').to_string());
        }
        if let Some(key) = api_key {
            active.api_key_enc = Set(if key.is_empty() {
                None
            } else {
                Some(crypto::encrypt(&self.encryption_key, key.as_bytes())?)
            });
        }
        if let Some(secret) = webhook_secret {
            active.webhook_secret = Set(if secret.is_empty() {
                None
            } else {
                Some(secret)
            });
        }
        if let Some(enabled) = enabled {
            active.enabled = Set(enabled);
        }
        repo::update_server(&self.db, active).await
    }

    pub async fn delete_server(&self, id: &str) -> Result<(), StreamingError> {
        if repo::delete_server(&self.db, id).await? {
            Ok(())
        } else {
            Err(StreamingError::ServerNotFound)
        }
    }

    /// Ingest connector library items for a server: match each to a canonical
    /// media id, upsert it, then stamp the server's last-synced time. Returns
    /// the number of items processed.
    pub async fn sync_server_library(
        &self,
        server_id: &str,
        items: Vec<ConnectorLibraryItem>,
    ) -> Result<u64, StreamingError> {
        let now = Utc::now();
        let mut count = 0u64;
        for (item, cameo_media_id) in matcher::match_library_items(items) {
            let model = server_library_item::ActiveModel {
                id: Set(Ulid::new().to_string()),
                server_id: Set(server_id.to_string()),
                cameo_media_id: Set(cameo_media_id),
                server_internal_id: Set(item.server_internal_id),
                title: Set(item.title),
                quality: Set(item.quality),
                deep_link: Set(item.deep_link),
                playback_link: Set(item.playback_link),
                external_ids: Set(item.external_ids),
                last_seen_at: Set(now.into()),
            };
            repo::upsert_library_item(&self.db, model).await?;
            count += 1;
        }
        let server = self.get_server(server_id).await?;
        let mut active: streaming_server::ActiveModel = server.into();
        active.last_synced_at = Set(Some(now.into()));
        repo::update_server(&self.db, active).await?;
        Ok(count)
    }

    /// Pull a server's library from its connector and ingest it. Returns the
    /// number of items synced.
    pub async fn sync_server(&self, server_id: &str) -> Result<u64, StreamingError> {
        let server = self.get_server(server_id).await?;
        let connector = self.build_connector(&server)?;
        let items = connector.sync_library().await?;
        self.sync_server_library(server_id, items).await
    }

    pub async fn get_playback_link(
        &self,
        server_id: &str,
        cameo_media_id: &str,
        episode_id: Option<&str>,
    ) -> Result<String, StreamingError> {
        let item = repo::find_library_item_by_cameo_id(&self.db, server_id, cameo_media_id)
            .await?
            .ok_or(StreamingError::ServerNotFound)?;
        let server = self.get_server(server_id).await?;
        let connector = self.build_connector(&server)?;
        match connector
            .playback_link(&item.server_internal_id, episode_id)
            .await
        {
            Ok(url) => Ok(url),
            // Fall back to the link captured at sync time if a live mint fails.
            Err(_) => Ok(item.playback_link),
        }
    }

    /// Verify, parse, and apply a raw webhook delivery for a server.
    pub async fn process_webhook(
        &self,
        server_id: &str,
        payload: &[u8],
        headers: &HashMap<String, String>,
    ) -> Result<(), StreamingError> {
        let server = self.get_server(server_id).await?;
        let connector = self.build_connector(&server)?;
        let events = connector.handle_webhook(payload, headers).await?;
        self.handle_webhook_events(server_id, events).await
    }

    pub async fn handle_webhook_events(
        &self,
        server_id: &str,
        events: Vec<ConnectorEvent>,
    ) -> Result<(), StreamingError> {
        for event in events {
            match event {
                // A library change is a safe re-sync trigger — no user
                // identity is involved.
                ConnectorEvent::LibraryChanged { .. } => {
                    if let Err(e) = self.sync_server(server_id).await {
                        tracing::warn!(server_id, error = %e, "webhook-triggered sync failed");
                    }
                }
                // Playback events carry a Beam-local user id; Stacks has no
                // Beam-user -> Stacks-user mapping, so these are logged for
                // observability but not written to watch history.
                other => {
                    tracing::info!(server_id, event = ?other, "received playback webhook event");
                }
            }
        }
        Ok(())
    }

    pub async fn health_check(&self, server_id: &str) -> Result<ConnectorHealth, StreamingError> {
        let server = self.get_server(server_id).await?;
        let connector = self.build_connector(&server)?;
        connector.health_check().await
    }

    pub async fn get_library_items(
        &self,
        server_id: &str,
    ) -> Result<Vec<server_library_item::Model>, StreamingError> {
        repo::find_library_items_for_server(&self.db, server_id).await
    }

    /// Build a live connector for a stored server, decrypting its API key.
    fn build_connector(
        &self,
        server: &streaming_server::Model,
    ) -> Result<BeamConnector, StreamingError> {
        if server.server_type != "beam" {
            return Err(StreamingError::UnsupportedServerType(
                server.server_type.clone(),
            ));
        }
        let api_key = match &server.api_key_enc {
            Some(enc) => {
                let bytes = crypto::decrypt(&self.encryption_key, enc)?;
                String::from_utf8(bytes)
                    .map_err(|_| StreamingError::Crypto(crypto::CryptoError::DecryptionFailed))?
            }
            None => String::new(),
        };
        Ok(BeamConnector::new(
            server.base_url.clone(),
            api_key,
            server.webhook_secret.clone(),
        ))
    }
}
