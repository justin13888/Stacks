//! Finding the title an import row is.
//!
//! In order, the first that answers wins:
//!
//! 1. **A known external id**: a `media_refs` row already links the row's
//!    MAL, IMDb or TVDB id to a catalog title. Only a provider writes these
//!    links (a fetched title's own ids, AniList's `idMal`), so they hold
//!    for every account.
//! 2. **The user's own earlier match**: a row of one of this user's earlier
//!    imports with the same MAL id, matched or resolved by hand, names the
//!    title again with the confidence it had then. A match one user makes
//!    is never applied to another user's rows.
//! 3. **A TMDB id**: `tmdb:<kind>:<id>` names the title outright (the kind
//!    comes from the row, or from asking TMDB which it is).
//! 4. **A MAL id**: AniList's own link (`idMal`), through
//!    [`MetadataProvider::anilist_by_mal_id`].
//! 5. **The title**: a search of the row's provider (AniList for MAL rows,
//!    TMDB for TMDB rows), each result scored for [`confidence`]. The best
//!    one is matched when it reaches [`AUTO_MATCH`] and no other comes
//!    within [`MARGIN`] of it; otherwise the row is unmatched and keeps the
//!    best [`CANDIDATES`] results for resolving it by hand.
//!
//! A title matched by AniList's link records the row's MAL id as one of its
//! `media_refs`, so the next import of it, by anyone, is step 1. A title
//! matched by search or by hand is a judgement, not a provider's link: it
//! stays with the user's import rows, and their next import of it is step 2.
//!
//! A provider that is down or rate-limiting makes the row
//! [`Resolution::Retry`], never unmatched or failed: the import job comes
//! back to it.

use std::sync::Arc;

use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, ExprTrait, Order, QueryFilter, QueryOrder,
};
use serde::{Deserialize, Serialize};

use crate::entity::enums::{ImportOutcome, MediaKind, RefProvider};
use crate::entity::{import, import_item, media};
use crate::import_export::models::ImportEntry;
use crate::media::models::MediaWithUserState;
use crate::media::{
    MediaError, MediaId, MediaService, MetadataError, MetadataErrorKind, MetadataProvider,
    Provider, catalog,
};
use uuid::Uuid;

/// The confidence at or above which a title search matches on its own: an
/// exact title needs at least one fact (kind, year, episode count) to agree
/// and none to disagree.
pub const AUTO_MATCH: f32 = 0.88;

/// How far ahead of the runner-up the best result must be to match.
pub const MARGIN: f32 = 0.03;

/// How many results an unmatched row keeps as candidates.
pub const CANDIDATES: usize = 5;

/// A title an unmatched row might be.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    /// The title's public id (`anilist:show:21`).
    #[serde(rename = "ref")]
    pub reference: String,
    pub title: String,
    pub year: Option<i32>,
    pub kind: MediaKind,
    /// 0–1.
    pub confidence: f32,
}

/// What finding a row's title came to.
#[derive(Debug)]
pub enum Resolution {
    Matched {
        media: Box<media::Model>,
        /// 1 for an id match; the search score otherwise.
        confidence: f32,
    },
    /// No title is certain; the candidates are the best guesses, best first.
    Unmatched(Vec<Candidate>),
    /// The provider could not answer now; try the row again later.
    Retry(String),
    /// The row cannot be matched for a reason retrying will not change.
    Failed(String),
}

/// A metadata failure worth retrying: the provider is down, slow or
/// rate-limiting.
fn transient(e: &MetadataError) -> bool {
    matches!(
        e.kind,
        MetadataErrorKind::Unavailable(_) | MetadataErrorKind::RateLimited { .. }
    )
}

/// Matches the rows of one user's imports.
pub struct Matcher {
    db: DatabaseConnection,
    metadata: Arc<dyn MetadataProvider>,
    user_id: Uuid,
}

impl Matcher {
    pub fn new(db: DatabaseConnection, metadata: Arc<dyn MetadataProvider>, user_id: Uuid) -> Self {
        Self {
            db,
            metadata,
            user_id,
        }
    }

    fn media(&self) -> MediaService {
        MediaService::new(self.db.clone(), self.metadata.clone())
    }

    pub async fn resolve(&self, entry: &ImportEntry) -> Resolution {
        match self.resolve_inner(entry).await {
            Ok(r) => r,
            Err(e) => Resolution::Failed(e),
        }
    }

    async fn resolve_inner(&self, entry: &ImportEntry) -> Result<Resolution, String> {
        let ids = &entry.external_ids;
        // 1. An id the catalog already links.
        for (provider, raw) in [
            (RefProvider::Mal, ids.mal_id.as_deref()),
            (RefProvider::Imdb, ids.imdb_id.as_deref()),
            (RefProvider::Tvdb, ids.tvdb_id.as_deref()),
        ] {
            if let Some(raw) = raw
                && let Some(found) = catalog::find_by_external(&self.db, provider, raw)
                    .await
                    .map_err(|e| e.to_string())?
            {
                return Ok(matched(found));
            }
        }

        // 2. The user's own earlier match of this MAL id.
        if let Some(mal) = ids.mal_id.as_deref()
            && let Some((found, confidence)) =
                self.earlier_match(mal).await.map_err(|e| e.to_string())?
        {
            return Ok(Resolution::Matched {
                media: Box::new(found),
                confidence,
            });
        }

        // 3. A TMDB id.
        if let Some(tmdb_id) = ids.tmdb_id.as_deref().and_then(|s| s.parse::<i32>().ok()) {
            let kind = match entry.kind {
                Some(k) => k,
                None => match self.probe_tmdb_kind(tmdb_id).await {
                    Ok(Some(k)) => k,
                    Ok(None) => return Ok(Resolution::Unmatched(Vec::new())),
                    Err(e) if transient(&e) => return Ok(Resolution::Retry(e.to_string())),
                    Err(e) => return Err(e.to_string()),
                },
            };
            return self
                .catalog(&MediaId::new(Provider::Tmdb, kind, tmdb_id), 1.0)
                .await;
        }

        // 4. A MAL id, through AniList's link.
        if let Some(mal) = ids.mal_id.as_deref().and_then(|s| s.parse::<i64>().ok()) {
            match self.metadata.anilist_by_mal_id(mal).await {
                Ok(id) => {
                    let resolution = self.catalog(&id, 1.0).await?;
                    if let Resolution::Matched { media, .. } = &resolution {
                        self.link_mal(entry, media).await?;
                    }
                    return Ok(resolution);
                }
                Err(e) if transient(&e) => return Ok(Resolution::Retry(e.to_string())),
                // Not linked, or the provider cannot look it up: try the title.
                Err(e) => tracing::debug!(mal, error = %e, "no AniList link for a MAL id"),
            }
        }

        // 5. The title.
        let Some(title) = entry.title.as_deref().filter(|t| !t.trim().is_empty()) else {
            return Ok(Resolution::Unmatched(Vec::new()));
        };
        let provider = if entry.source == "tmdb" {
            Provider::Tmdb
        } else {
            Provider::AniList
        };
        let page = match self.metadata.search(provider, title, 1).await {
            Ok(p) => p,
            Err(e) if transient(&e) => return Ok(Resolution::Retry(e.to_string())),
            Err(e) => return Err(e.to_string()),
        };
        let mut scored: Vec<(f32, MediaWithUserState)> = page
            .items
            .into_iter()
            .map(|item| (confidence(entry, &item, None), item))
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let candidates = |scored: &[(f32, MediaWithUserState)]| -> Vec<Candidate> {
            scored
                .iter()
                .take(CANDIDATES)
                .map(|(c, item)| Candidate {
                    reference: item.cameo_media_id.clone(),
                    title: item.title.clone(),
                    year: item.year,
                    kind: item.kind,
                    confidence: *c,
                })
                .collect()
        };
        let Some((best, item)) = scored.first() else {
            return Ok(Resolution::Unmatched(Vec::new()));
        };
        let runner_up = scored.get(1).map_or(0.0, |s| s.0);
        // The episode count is only in the title's detail, so only a result
        // that could match is fetched to check it.
        if *best + 0.05 < AUTO_MATCH || *best - runner_up < MARGIN {
            return Ok(Resolution::Unmatched(candidates(&scored)));
        }
        let id = MediaId::parse(&item.cameo_media_id).map_err(|e| e.to_string())?;
        let row = match self.media().ensure_in_catalog(&id).await {
            Ok(row) => row,
            Err(MediaError::NotFound(_)) => return Ok(Resolution::Unmatched(candidates(&scored))),
            Err(e) => return Err(e.to_string()),
        };
        let fin = confidence(entry, item, row.episode_count);
        if fin < AUTO_MATCH || fin - runner_up < MARGIN {
            scored[0].0 = fin;
            scored.sort_by(|a, b| b.0.total_cmp(&a.0));
            return Ok(Resolution::Unmatched(candidates(&scored)));
        }
        Ok(Resolution::Matched {
            media: Box::new(row),
            confidence: fin,
        })
    }

    /// The title one of this user's earlier import rows with the same MAL id
    /// was matched or resolved to, most recent first, and the confidence it
    /// was matched with.
    async fn earlier_match(
        &self,
        mal: &str,
    ) -> Result<Option<(media::Model, f32)>, sea_orm::DbErr> {
        let earlier = import_item::Entity::find()
            .filter(
                import_item::Column::ImportId.in_subquery(
                    Query::select()
                        .column(import::Column::Id)
                        .from(import::Entity)
                        .and_where(import::Column::UserId.eq(self.user_id))
                        .to_owned(),
                ),
            )
            .filter(Expr::cust("raw -> 'external_ids' ->> 'mal_id'").eq(mal))
            .filter(
                import_item::Column::Outcome
                    .is_in([ImportOutcome::Applied, ImportOutcome::Skipped]),
            )
            .filter(import_item::Column::MatchedMediaId.is_not_null())
            .order_by(import_item::Column::UpdatedAt, Order::Desc)
            .one(&self.db)
            .await?;
        let Some((media_id, confidence)) =
            earlier.and_then(|r| r.matched_media_id.map(|m| (m, r.confidence)))
        else {
            return Ok(None);
        };
        Ok(media::Entity::find_by_id(media_id)
            .one(&self.db)
            .await?
            .map(|m| (m, confidence.unwrap_or(1.0))))
    }

    /// The catalog row for an id the row names.
    async fn catalog(&self, id: &MediaId, confidence: f32) -> Result<Resolution, String> {
        match self.media().ensure_in_catalog(id).await {
            Ok(row) => Ok(Resolution::Matched {
                media: Box::new(row),
                confidence,
            }),
            // The provider has no such title: nothing to resolve by id.
            Err(MediaError::NotFound(_)) => Ok(Resolution::Unmatched(Vec::new())),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Record the row's MAL id against the title AniList links it to. An id
    /// already linked to any title is left as it is.
    async fn link_mal(&self, entry: &ImportEntry, row: &media::Model) -> Result<(), String> {
        if let Some(mal) = entry.external_ids.mal_id.as_deref() {
            catalog::insert_extra_ref(&self.db, row.id, RefProvider::Mal, row.kind, mal)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Whether a bare TMDB id is a movie or a show: movie first, then show.
    /// `Ok(None)` when TMDB has neither.
    async fn probe_tmdb_kind(&self, tmdb_id: i32) -> Result<Option<MediaKind>, MetadataError> {
        for kind in [MediaKind::Movie, MediaKind::Show] {
            let id = MediaId::new(Provider::Tmdb, kind, tmdb_id);
            match self.metadata.details(&id).await {
                Ok(_) => return Ok(Some(kind)),
                Err(e) if e.kind == MetadataErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}

fn matched(media: media::Model) -> Resolution {
    Resolution::Matched {
        media: Box::new(media),
        confidence: 1.0,
    }
}

/// How sure a search result is the row's title, 0–1.
///
/// The title counts for 70% (the best normalised similarity against the
/// result's title and original title); agreement on the year, the kind and
/// the episode count for 10% each. What either side does not know counts
/// half, so a perfect title with nothing else known is 0.85 and is not
/// matched on its own.
pub fn confidence(entry: &ImportEntry, item: &MediaWithUserState, episodes: Option<i32>) -> f32 {
    let wanted = entry.title.as_deref().map(normalise).unwrap_or_default();
    let title = [Some(item.title.as_str()), item.original_title.as_deref()]
        .into_iter()
        .flatten()
        .map(|t| similarity(&wanted, &normalise(t)))
        .fold(0.0_f32, f32::max);
    let agree = |known: Option<bool>| match known {
        Some(true) => 1.0,
        Some(false) => 0.0,
        None => 0.5,
    };
    let year = match (entry.year, item.year) {
        (Some(a), Some(b)) if a == b => 1.0,
        (Some(a), Some(b)) if (a - b).abs() == 1 => 0.5,
        (Some(_), Some(_)) => 0.0,
        _ => 0.5,
    };
    let kind = agree(entry.kind.map(|k| k == item.kind));
    let eps = agree(
        entry
            .total_episodes
            .zip(episodes.filter(|n| *n > 0))
            .map(|(a, b)| a == b),
    );
    0.7 * title + 0.1 * year + 0.1 * kind + 0.1 * eps
}

/// Lower-case, every run of non-alphanumerics one space, trimmed.
fn normalise(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut gap = false;
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            out.push(c);
            gap = false;
        } else {
            gap = true;
        }
    }
    out
}

/// 1 − the Levenshtein distance over the longer length, by characters.
fn similarity(a: &str, b: &str) -> f32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 0.0;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j + 1] + 1).min(cur[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    1.0 - prev[b.len()] as f32 / longest as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::enums::Category;

    fn item(title: &str, year: Option<i32>, kind: MediaKind) -> MediaWithUserState {
        MediaWithUserState {
            cameo_media_id: "anilist:show:1".into(),
            title: title.into(),
            original_title: None,
            kind,
            category: Category::Anime,
            year,
            overview: None,
            poster_url: None,
            backdrop_url: None,
            genres: Vec::new(),
            rating: None,
            watch_status: None,
            rewatching: false,
            score: None,
            is_favorite: false,
        }
    }

    fn row(
        title: &str,
        year: Option<i32>,
        kind: Option<MediaKind>,
        eps: Option<i32>,
    ) -> ImportEntry {
        ImportEntry {
            source: "mal".into(),
            title: Some(title.into()),
            year,
            kind,
            total_episodes: eps,
            ..Default::default()
        }
    }

    #[test]
    fn titles_compare_after_normalising() {
        assert_eq!(
            normalise("  Shingeki no Kyojin: Season 2!! "),
            "shingeki no kyojin season 2"
        );
        assert!((similarity("cowboy bebop", "cowboy bebop") - 1.0).abs() < f32::EPSILON);
        assert!(similarity("cowboy bebop", "cowboy bebop tengoku no tobira") < 0.5);
        assert_eq!(similarity("", ""), 0.0);
    }

    #[test]
    fn an_exact_title_with_agreeing_facts_auto_matches() {
        let r = row("Cowboy Bebop", Some(1998), Some(MediaKind::Show), Some(26));
        let c = confidence(
            &r,
            &item("Cowboy Bebop", Some(1998), MediaKind::Show),
            Some(26),
        );
        assert!(c >= AUTO_MATCH, "{c}");
        // Unknown year and episodes: still enough with the kind agreeing.
        let r = row("Cowboy Bebop", None, Some(MediaKind::Show), None);
        assert!(confidence(&r, &item("Cowboy Bebop", None, MediaKind::Show), None) >= AUTO_MATCH);
    }

    #[test]
    fn disagreement_keeps_a_row_unmatched() {
        let r = row("Cowboy Bebop", None, Some(MediaKind::Show), Some(26));
        // The movie of the same name.
        let c = confidence(
            &r,
            &item("Cowboy Bebop", Some(2001), MediaKind::Movie),
            Some(1),
        );
        assert!(c < AUTO_MATCH, "{c}");
        // A different title.
        let c = confidence(&r, &item("Trigun", None, MediaKind::Show), Some(26));
        assert!(c < 0.6, "{c}");
        // Nothing but the title known.
        let bare = row("Cowboy Bebop", None, None, None);
        let c = confidence(&bare, &item("Cowboy Bebop", None, MediaKind::Show), None);
        assert!(c < AUTO_MATCH, "{c}");
    }

    #[test]
    fn the_original_title_counts_too() {
        let r = row("Shingeki no Kyojin", None, Some(MediaKind::Show), None);
        let mut i = item("Attack on Titan", None, MediaKind::Show);
        i.original_title = Some("Shingeki no Kyojin".into());
        assert!(confidence(&r, &i, None) >= AUTO_MATCH);
    }
}
