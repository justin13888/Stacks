//! Evaluating a smart list: gathering its candidates from a discovery feed
//! or the user's own library, and narrowing them by its filter.

use crate::library::repo as library_repo;
use crate::media::filter::FilterQuery;
use crate::media::id::Provider;
use crate::media::models::Listing;
use crate::tags::repo as tag_repo;

use super::super::filter::Source;
use super::*;

impl ListService {
    /// Evaluate a smart list's filter and return the matching titles, once
    /// each, overlaid with the requesting user's library state. Titles the
    /// user blocked are left out.
    ///
    /// Candidates are drawn from the filter's source (its `sort` field): a
    /// TMDB discovery feed, or the user's library, read from the catalog
    /// snapshot without calling a provider. They are then narrowed by the
    /// filter predicates. The metadata provider is passed per-call so
    /// [`ListService::new`] stays database-only.
    ///
    /// A feed page that fails ends that kind's scan and is reported in the
    /// listing's `degraded`; the list is an error only when nothing at all
    /// could be fetched.
    pub async fn evaluate_smart_list(
        &self,
        metadata: &dyn MetadataProvider,
        user_id: Uuid,
        list_id: Uuid,
    ) -> Result<Listing<MediaWithUserState>, ListError> {
        let list = self.get_list(user_id, list_id).await?;
        if !list.is_smart {
            return Err(ListError::NotSmart);
        }
        let filter = match &list.filter_query {
            Some(raw) => parse_filter(raw)?,
            None => SmartFilter::default(),
        };
        let library = if filter.source == Source::Library || filter.has_tags() {
            Some(self.library(user_id).await?)
        } else {
            None
        };
        let listing = match filter.source {
            Source::Library => Listing {
                items: library.iter().flatten().map(|(m, _)| m.clone()).collect(),
                next_page: None,
                unavailable: Vec::new(),
                degraded: Vec::new(),
            },
            Source::Feed(feed) => self.feed(metadata, &filter.kinds, feed).await?,
        };
        let want = filter.tags.as_deref().unwrap_or_default();
        let tagged: HashSet<String> = library
            .iter()
            .flatten()
            .filter(|(_, tags)| tags.iter().any(|t| want.contains(t)))
            .map(|(m, _)| m.cameo_media_id.clone())
            .collect();

        let blocked = BlocklistService::new(self.db.clone())
            .blocked_refs(user_id)
            .await?;
        let kinds = &filter.kinds;
        let mut seen = HashSet::new();
        let candidates = listing
            .items
            .into_iter()
            .filter(|m| kinds.contains(&m.kind))
            .filter(|m| MediaId::parse(&m.cameo_media_id).map_or(true, |id| !blocked.contains(&id)))
            .filter(|m| seen.insert(m.cameo_media_id.clone()))
            .collect();
        // Library candidates carry their state already; enriching them again
        // reads the same rows.
        let enriched = match filter.source {
            Source::Library => candidates,
            Source::Feed(_) => enrichment::enrich(&self.db, user_id, candidates).await?,
        };
        Ok(Listing {
            items: enriched
                .into_iter()
                .filter(|media| matches_filter(media, &filter, &tagged))
                .collect(),
            next_page: None,
            ..listing
        })
    }

    /// The candidates of a discovery feed, one kind at a time, stopping
    /// early once a feed runs out of pages.
    async fn feed(
        &self,
        metadata: &dyn MetadataProvider,
        kinds: &[crate::media::id::MediaKind],
        feed: crate::media::DiscoverCategory,
    ) -> Result<Listing<MediaWithUserState>, ListError> {
        let mut results = Vec::new();
        for kind in kinds {
            for page in 1..=SMART_LIST_PAGES {
                let result = metadata.feed(Provider::Tmdb, feed, *kind, page).await;
                let stop = !matches!(&result, Ok(p) if p.has_next);
                results.push(result);
                if stop {
                    break;
                }
            }
        }
        Ok(media_service::merge_pages(1, results)?)
    }

    /// The user's live entries, most recently changed first, each as its
    /// snapshot with the user's state and the ids of the tags it carries. An
    /// entry whose title has no public id cannot be shown and is left out.
    async fn library(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<(MediaWithUserState, Vec<Uuid>)>, ListError> {
        let rows = library_repo::all_entries(&self.db, user_id).await?;
        let entry_ids: Vec<Uuid> = rows.iter().map(|(e, _)| e.id).collect();
        let media_ids: Vec<Uuid> = rows.iter().map(|(_, m)| m.id).collect();
        let refs = crate::media::catalog::primary_refs(&self.db, &media_ids).await?;
        let mut tags = library_repo::tags_for_entries(&self.db, &entry_ids).await?;
        let progress: HashMap<Uuid, _> = library_repo::progress_many(&self.db, entry_ids)
            .await?
            .into_iter()
            .map(|p| (p.entry_id, p))
            .collect();
        Ok(rows
            .into_iter()
            .filter_map(|(entry, media)| {
                let mut item = MediaWithUserState::from_snapshot(&media, refs.get(&media.id)?);
                enrichment::apply_entry(&mut item, &entry, progress.get(&entry.id));
                let carried = tags
                    .remove(&entry.id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|t| t.id)
                    .collect();
                Some((item, carried))
            })
            .collect())
    }
}

/// Check a filter for a list of `user_id`'s: the filter's own rules, and
/// that each tag it names is the user's. A tag `kept` already names (one
/// deleted since the filter was written) may stay.
pub(super) async fn check_filter_for<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    raw: &serde_json::Value,
    kept: Option<&serde_json::Value>,
) -> Result<(), ListError> {
    let filter = parse_filter(raw)?;
    let Some(tags) = filter.tags.filter(|t| !t.is_empty()) else {
        return Ok(());
    };
    let kept: HashSet<Uuid> = kept
        .and_then(|k| serde_json::from_value::<FilterQuery>(k.clone()).ok())
        .and_then(|k| k.tag)
        .unwrap_or_default()
        .into_iter()
        .collect();
    let owned: HashSet<Uuid> = tag_repo::tags_for_user(db, user_id)
        .await?
        .into_iter()
        .map(|t| t.id)
        .collect();
    if tags.iter().all(|t| owned.contains(t) || kept.contains(t)) {
        Ok(())
    } else {
        Err(ListError::InvalidFilter(
            "`tag` names a tag that is not yours".into(),
        ))
    }
}
