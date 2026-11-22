//! Stacks' own AniList GraphQL queries, for what cameo's details lack: the
//! extras a title's detail call adds and a show's episode count, both of
//! which need AniList's `nextAiringEpisode`. Each runs under
//! [`call_paced`](CameoProvider::call_paced) and is classified by
//! [`anilist_graphql`](CameoProvider::anilist_graphql).

use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::message::{sanitize, transport_kind};
use super::{CameoProvider, SeasonDetails, status_kind};
use crate::media::anilist::{self, AniListEpisodes, AniListExtras, MediaData};
use crate::media::error::{MetadataError, MetadataErrorKind, Operation};
use crate::media::id::MediaId;

impl CameoProvider {
    /// Stacks' own AniList query for the two fields cameo's details lack:
    /// the MAL id and the next episode to air. One request for both, where
    /// cameo would take one for the MAL id and has none for the next episode
    /// (justin13888/cameo#192, justin13888/cameo#193).
    pub(super) async fn anilist_extras(
        &self,
        numeric: i32,
    ) -> Result<AniListExtras, MetadataErrorKind> {
        const QUERY: &str = "query StacksMediaExtras($id: Int) { Media(id: $id, type: ANIME) { \
            idMal nextAiringEpisode { episode } } }";
        self.anilist_graphql(
            QUERY,
            serde_json::json!({ "id": numeric }),
            |d: MediaData<AniListExtras>| d.media,
        )
        .await?
        .ok_or(MetadataErrorKind::NotFound)
    }

    /// Send one of Stacks' own GraphQL queries to AniList and classify the
    /// response. `field` picks the queried root field out of `data`; the
    /// response is accepted only when that field is present. A missing or
    /// `null` field is classified by the GraphQL `errors` first, then by the
    /// HTTP status, so a partial-error reply or a 5xx is never mistaken for
    /// an empty result. `Ok(None)` is a successful response that holds
    /// neither the field nor an error. Run it only under
    /// [`call_paced`](Self::call_paced), which waits for the `extras_rate`
    /// slot this request spends.
    async fn anilist_graphql<D: DeserializeOwned, T>(
        &self,
        query: &str,
        variables: serde_json::Value,
        field: impl FnOnce(D) -> Option<T>,
    ) -> Result<Option<T>, MetadataErrorKind> {
        #[derive(Deserialize)]
        struct Envelope<D> {
            data: Option<D>,
            #[serde(default)]
            errors: Vec<GqlError>,
        }
        #[derive(Deserialize)]
        struct GqlError {
            message: Option<String>,
            status: Option<i64>,
        }

        let resp = self
            .extras_http
            .post(&self.anilist_url)
            .json(&serde_json::json!({ "query": query, "variables": variables }))
            .send()
            .await
            .map_err(transport_kind)?;
        let status = resp.status().as_u16();
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let body = resp.text().await.map_err(transport_kind)?;
        if status == 429 {
            return Err(MetadataErrorKind::RateLimited { retry_after });
        }
        let envelope: Envelope<D> = match serde_json::from_str(&body) {
            Ok(e) => e,
            Err(e) => {
                return Err(match status_kind(status) {
                    Some(kind) => kind,
                    None => MetadataErrorKind::BadUpstreamResponse(sanitize(&format!(
                        "{e}; body: {body}"
                    ))),
                });
            }
        };
        if envelope.errors.iter().any(|e| e.status == Some(404)) {
            return Err(MetadataErrorKind::NotFound);
        }
        if envelope.errors.iter().any(|e| e.status == Some(429)) {
            return Err(MetadataErrorKind::RateLimited { retry_after });
        }
        match envelope.data.and_then(field) {
            Some(value) => Ok(Some(value)),
            None if !envelope.errors.is_empty() => {
                let messages: Vec<&str> = envelope
                    .errors
                    .iter()
                    .filter_map(|e| e.message.as_deref())
                    .collect();
                Err(MetadataErrorKind::BadUpstreamResponse(sanitize(&format!(
                    "GraphQL errors: {}",
                    messages.join("; ")
                ))))
            }
            None => status_kind(status).map_or(Ok(None), Err),
        }
    }

    /// An AniList show's one season, from Stacks' own episode-count query
    /// (see [`anilist`]). Any other season is not found, and
    /// nothing is sent for it.
    pub(super) async fn anilist_season(
        &self,
        id: &MediaId,
        season: u32,
    ) -> Result<SeasonDetails, MetadataError> {
        if season != anilist::SEASON {
            let err =
                MetadataError::new(id.provider, Operation::Season, MetadataErrorKind::NotFound);
            return Err(err);
        }
        let found = self
            .call_paced(Operation::Season, async {
                self.anilist_graphql(
                    anilist::EPISODES_QUERY,
                    serde_json::json!({ "id": id.numeric }),
                    |d: MediaData<AniListEpisodes>| d.media,
                )
                .await?
                .ok_or(MetadataErrorKind::NotFound)
            })
            .await?;
        Ok(found.season())
    }
}
