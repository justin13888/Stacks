use chrono::Utc;
use sea_orm::{ActiveValue::Set, DatabaseConnection, EntityTrait};
use ulid::Ulid;

use crate::crypto;
use crate::entity::{server_library_item, streaming_server};

use super::StreamingError;
use super::beam::BeamConnector;
use super::connector::StreamingConnector;
use super::models::{ConnectorEvent, ConnectorHealth, ConnectorLibraryItem};
use super::{matcher, repo};

pub struct StreamingService {
    pub db: DatabaseConnection,
    /// 32-byte AES-256 KEK derived from `WW_SECRET_KEY`. Used to encrypt
    /// per-server `api_key` values at rest.
    pub kek: [u8; 32],
}

impl StreamingService {
    pub fn new(db: DatabaseConnection, secret_key: &str) -> Self {
        Self {
            db,
            kek: derive_kek(secret_key),
        }
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
        let api_key_enc = match api_key {
            Some(k) => Some(
                crypto::encrypt(&self.kek, k.as_bytes())
                    .map_err(|e| StreamingError::Upstream(format!("encrypt: {e}")))?,
            ),
            None => None,
        };
        let model = streaming_server::ActiveModel {
            id: Set(Ulid::new().to_string()),
            name: Set(name),
            server_type: Set(server_type),
            base_url: Set(base_url),
            api_key_enc: Set(api_key_enc),
            user_id: Set(user_id.map(|s| s.to_string())),
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

    pub async fn list_all_servers(
        &self,
    ) -> Result<Vec<streaming_server::Model>, StreamingError> {
        repo::list_all_servers(&self.db).await
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
        let existing = self.get_server(id).await?;
        let mut active: streaming_server::ActiveModel = existing.into();
        if let Some(n) = name {
            active.name = Set(n);
        }
        if let Some(u) = base_url {
            active.base_url = Set(u);
        }
        if let Some(k) = api_key {
            let enc = crypto::encrypt(&self.kek, k.as_bytes())
                .map_err(|e| StreamingError::Upstream(format!("encrypt: {e}")))?;
            active.api_key_enc = Set(Some(enc));
        }
        if let Some(s) = webhook_secret {
            active.webhook_secret = Set(Some(s));
        }
        if let Some(e) = enabled {
            active.enabled = Set(e);
        }
        repo::update_server(&self.db, active).await
    }

    pub async fn delete_server(&self, id: &str) -> Result<(), StreamingError> {
        let deleted = repo::delete_server(&self.db, id).await?;
        if !deleted {
            return Err(StreamingError::ServerNotFound);
        }
        Ok(())
    }

    pub async fn sync_server_library(
        &self,
        server_id: &str,
        items: Vec<ConnectorLibraryItem>,
    ) -> Result<u64, StreamingError> {
        let matched = matcher::match_library_items(&items).await?;
        let now = Utc::now();
        let mut count = 0_u64;
        for (item, cameo_id) in matched {
            let model = server_library_item::ActiveModel {
                id: Set(format!("{}::{}", server_id, item.server_internal_id)),
                server_id: Set(server_id.to_string()),
                cameo_media_id: Set(cameo_id),
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
        // Mark the server's last-synced timestamp.
        if let Some(server) = repo::find_server_by_id(&self.db, server_id).await? {
            let mut active: streaming_server::ActiveModel = server.into();
            active.last_synced_at = Set(Some(now.into()));
            let _ = repo::update_server(&self.db, active).await;
        }
        Ok(count)
    }

    pub async fn get_playback_link(
        &self,
        server_id: &str,
        cameo_media_id: &str,
        _episode_id: Option<&str>,
    ) -> Result<String, StreamingError> {
        let item = repo::find_library_item_by_cameo_id(&self.db, server_id, cameo_media_id)
            .await?
            .ok_or(StreamingError::ServerNotFound)?;
        Ok(item.playback_link)
    }

    pub async fn handle_webhook_events(
        &self,
        server_id: &str,
        events: Vec<ConnectorEvent>,
    ) -> Result<(), StreamingError> {
        // Persist events that map to a known server_library_item as
        // watch_history rows. Events without a known item are dropped
        // (logged at debug level).
        use chrono::Utc;
        use sea_orm::{ActiveModelTrait, ActiveValue::Set};

        for event in events {
            let (server_internal_id, user_id_opt, position, event_kind) = match event {
                ConnectorEvent::PlaybackStarted {
                    server_internal_id,
                    user_id,
                    position_seconds,
                } => (server_internal_id, user_id, position_seconds, "play"),
                ConnectorEvent::PlaybackStopped {
                    server_internal_id,
                    user_id,
                    position_seconds,
                } => (server_internal_id, user_id, position_seconds, "stop"),
                ConnectorEvent::PlaybackPaused {
                    server_internal_id,
                    user_id,
                    position_seconds,
                } => (server_internal_id, user_id, position_seconds, "pause"),
                ConnectorEvent::PlaybackFinished {
                    server_internal_id,
                    user_id,
                } => (server_internal_id, user_id, None, "complete"),
                ConnectorEvent::LibraryChanged { .. } | ConnectorEvent::Unknown { .. } => continue,
            };

            // Look up the library item to find the cameo id.
            let library_id = format!("{server_id}::{server_internal_id}");
            let item = crate::entity::server_library_item::Entity::find_by_id(&library_id)
                .one(&self.db)
                .await
                .map_err(StreamingError::Db)?;
            let Some(item) = item else { continue };
            let Some(cameo_media_id) = item.cameo_media_id else {
                continue;
            };
            let Some(user_id) = user_id_opt else { continue };

            let history = crate::entity::watch_history::ActiveModel {
                id: Set(Ulid::new().to_string()),
                user_id: Set(user_id),
                cameo_media_id: Set(cameo_media_id),
                cameo_episode_id: Set(None),
                event: Set(event_kind.to_string()),
                position_seconds: Set(position),
                metadata: Set(None),
                source: Set(format!("server:{server_id}")),
                timestamp: Set(Utc::now().into()),
            };
            history
                .insert(&self.db)
                .await
                .map_err(StreamingError::Db)?;
        }
        Ok(())
    }

    pub async fn health_check(&self, server_id: &str) -> Result<ConnectorHealth, StreamingError> {
        let server = self.get_server(server_id).await?;
        let connector = self.connector_for(&server)?;
        connector.health_check().await
    }

    pub async fn get_library_items(
        &self,
        server_id: &str,
    ) -> Result<Vec<server_library_item::Model>, StreamingError> {
        repo::find_library_items_for_server(&self.db, server_id).await
    }

    pub async fn list_library_items_for_cameo_id(
        &self,
        cameo_media_id: &str,
    ) -> Result<Vec<server_library_item::Model>, StreamingError> {
        repo::find_library_items_by_cameo_id(&self.db, cameo_media_id).await
    }

    pub fn decrypt_api_key(&self, server: &streaming_server::Model) -> Option<String> {
        let enc = server.api_key_enc.as_ref()?;
        crypto::decrypt(&self.kek, enc)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    }

    /// Build a connector instance for a configured server.
    pub fn connector_for(
        &self,
        server: &streaming_server::Model,
    ) -> Result<Box<dyn StreamingConnector>, StreamingError> {
        match server.server_type.as_str() {
            "beam" => {
                let api_key = self.decrypt_api_key(server).unwrap_or_default();
                Ok(Box::new(BeamConnector::new(
                    server.base_url.clone(),
                    api_key,
                    server.webhook_secret.clone(),
                )))
            }
            other => Err(StreamingError::UnsupportedServerType(other.to_string())),
        }
    }
}

/// Derive a 32-byte AES-256 key from the configured `WW_SECRET_KEY` using
/// SHA-256. The secret is operator-provided and should be high-entropy
/// (e.g. `openssl rand -hex 32`); the SHA pass guarantees a uniform 32-byte
/// key regardless of the input format.
pub(crate) fn derive_kek(secret_key: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(secret_key.as_bytes());
    let out = hasher.finalize();
    let mut k = [0u8; 32];
    k.copy_from_slice(&out);
    k
}
