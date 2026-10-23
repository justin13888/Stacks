//! Titles the caller never wants suggested.
//!
//! A blocked title is left out of search, recommendations, discovery feeds
//! and smart lists, until it is unblocked.

use std::sync::Arc;

use kynos::prelude::*;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use stacks_core::blocklist::BlocklistService;
use stacks_core::media::{MediaError, MediaService, MetadataProvider, catalog};

use crate::app::App;
use crate::catalog::{TitlePath, TitleSummary, snapshot_summaries};
use crate::page::{self, InvalidCursor, Page, PageParams};
use crate::problem::{Internal, TitleNotFound};
use crate::security::{Caller, Reader, Writer};

/// The caller's blocklist.
#[derive(Tag)]
#[tag(
    name = "blocklist",
    description = "Titles left out of search, recommendations, feeds and smart lists."
)]
pub struct Blocklist;

/// A blocked title.
#[derive(Clone, Debug, PartialEq, Schema, Serialize, Deserialize)]
pub struct BlockedTitle {
    pub title: TitleSummary,
    pub blocked_at: chrono::DateTime<chrono::Utc>,
}

/// What listing blocked titles can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum ListBlockedError {
    #[error(transparent)]
    #[problem(transparent)]
    InvalidCursor(#[from] InvalidCursor),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// What blocking a title can fail with.
#[derive(Debug, thiserror::Error, ApiError)]
pub enum BlockTitleError {
    #[error(transparent)]
    #[problem(transparent)]
    NotFound(#[from] TitleNotFound),
    #[error(transparent)]
    #[problem(transparent)]
    Internal(#[from] Internal),
}

/// List the caller's blocked titles, most recently blocked first.
#[kynos::get("/blocklist", operation_id = "listBlocked")]
async fn list_blocked(
    Auth(caller): Reader,
    Query(params): Query<PageParams>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<Json<Page<BlockedTitle>>, ListBlockedError> {
    let caller = Caller::from(caller);
    let blocked = BlocklistService::new(db.clone())
        .list_blocked_media(caller.id())
        .await
        .map_err(Internal::from)?;
    let page = page::by_id_desc(blocked, |(b, _)| b.id, &params)?;
    let rows: Vec<_> = page.items.iter().map(|(_, m)| m.clone()).collect();
    let mut summaries = snapshot_summaries(&db, caller.id(), &rows).await?;
    Ok(Json(Page::new(
        page.items
            .iter()
            .filter_map(|(b, m)| {
                Some(BlockedTitle {
                    title: summaries.remove(&m.id)?,
                    blocked_at: b.created_at.to_utc(),
                })
            })
            .collect(),
        page.next_cursor,
    )))
}

/// Block a title.
///
/// Blocking a blocked title changes nothing. A title Stacks has not stored
/// yet is fetched from its provider; when the provider is down it is stored
/// as a placeholder and filled in later.
#[kynos::put("/blocklist/{ref}", operation_id = "blockTitle")]
async fn block_title(
    Auth(caller): Writer,
    Path(path): Path<TitlePath>,
    Inject(db): Inject<DatabaseConnection>,
    Inject(metadata): Inject<Arc<dyn MetadataProvider>>,
) -> Result<NoContent, BlockTitleError> {
    let caller = Caller::from(caller);
    let title = MediaService::new(db.clone(), metadata)
        .ensure_in_catalog(path.reference.media_id())
        .await
        .map_err(|e| match e {
            MediaError::NotFound(what) => BlockTitleError::from(TitleNotFound(what)),
            MediaError::Db(e) => Internal::from(e).into(),
            other => Internal::log(other).into(),
        })?;
    BlocklistService::new(db)
        .block(caller.id(), title.id)
        .await
        .map_err(Internal::from)?;
    Ok(NoContent)
}

/// Unblock a title.
///
/// Unblocking a title that is not blocked changes nothing.
#[kynos::delete("/blocklist/{ref}", operation_id = "unblockTitle")]
async fn unblock_title(
    Auth(caller): Writer,
    Path(path): Path<TitlePath>,
    Inject(db): Inject<DatabaseConnection>,
) -> Result<NoContent, Internal> {
    let caller = Caller::from(caller);
    // A title Stacks never stored cannot be blocked.
    if let Some(title) = catalog::find_by_ref(&db, path.reference.media_id()).await? {
        BlocklistService::new(db)
            .unblock(caller.id(), title.id)
            .await?;
    }
    Ok(NoContent)
}

pub(crate) fn group() -> Group<App> {
    Group::new("/").tag::<Blocklist>().mount(kynos::routes![
        list_blocked,
        block_title,
        unblock_title,
    ])
}
