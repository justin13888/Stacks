use salvo::oapi::{ToParameters, ToSchema};
use serde::{Deserialize, Serialize};

use crate::media::dto::MediaItem;

/// Query parameters accepted by the discovery endpoints.
#[derive(Debug, Serialize, Deserialize, ToSchema, ToParameters)]
pub struct DiscoverQuery {
    /// Restrict the feed to `movie` or `tv`. Defaults to `movie` when omitted.
    pub media_type: Option<String>,
    /// Opaque pagination cursor.
    pub cursor: Option<String>,
    /// Page size hint (advisory for cameo-backed endpoints).
    pub limit: Option<u32>,
}

/// A page of discovered media items.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DiscoverResponse {
    pub items: Vec<MediaItem>,
    pub next_cursor: Option<String>,
}
