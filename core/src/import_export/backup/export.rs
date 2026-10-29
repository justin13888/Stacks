//! Writing a [`Backup`] of one account.
//!
//! Every query reads every row the user owns, soft-deleted and voided rows
//! included, in a stable order (ids are UUIDv7, so by creation), inside one
//! repeatable-read transaction so the document is a consistent snapshot.

use std::collections::{BTreeSet, HashMap};

use sea_orm::{
    AccessMode, ColumnTrait, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait,
    IsolationLevel, Order, QueryFilter, QueryOrder, TransactionTrait,
};
use uuid::Uuid;

use crate::entity::{
    blocklist, entry_event, entry_tag, library_entry, list, list_item, media, media_ref,
    notification_channel, notification_subscription, play, tag, user, watch_run,
};
use crate::import_export::error::ImportExportError;
use crate::media::catalog;
use crate::media::filter::FilterQuery;

use super::format::{
    Backup, Blocked, Channel, Entry, EntryTag, EpisodeRef, Event, FORMAT, FORMAT_VERSION, List,
    ListItem, Play, Profile, Run, Subscription, Tag, Title, TitleRef,
};

/// What a backup includes beyond the defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExportOptions {
    /// Include notification channel configurations, which hold credentials.
    pub include_secrets: bool,
}

/// Everything `user_id` owns, as a backup document.
pub async fn export_backup(
    db: &DatabaseConnection,
    user_id: Uuid,
    options: ExportOptions,
) -> Result<Backup, ImportExportError> {
    let txn = db
        .begin_with_config(
            Some(IsolationLevel::RepeatableRead),
            Some(AccessMode::ReadOnly),
        )
        .await?;
    let backup = read(&txn, user_id, options).await?;
    txn.commit().await?;
    Ok(backup)
}

async fn read<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    options: ExportOptions,
) -> Result<Backup, ImportExportError> {
    let owner = user::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .ok_or_else(|| ImportExportError::Internal(format!("user {user_id} not found")))?;
    let settings = match owner.settings {
        serde_json::Value::Object(map) => map,
        // The schema's CHECK constraint keeps settings an object.
        _ => serde_json::Map::new(),
    };

    let entries = library_entry::Entity::find()
        .filter(library_entry::Column::UserId.eq(user_id))
        .order_by(library_entry::Column::Id, Order::Asc)
        .all(db)
        .await?;
    let entry_ids: Vec<Uuid> = entries.iter().map(|e| e.id).collect();
    let runs = in_ids(db, &entry_ids, |ids| {
        watch_run::Entity::find()
            .filter(watch_run::Column::EntryId.is_in(ids))
            .order_by(watch_run::Column::RunNumber, Order::Asc)
    })
    .await?;
    let run_ids: Vec<Uuid> = runs.iter().map(|r| r.id).collect();
    let plays = in_ids(db, &run_ids, |ids| {
        play::Entity::find()
            .filter(play::Column::RunId.is_in(ids))
            .order_by(play::Column::Id, Order::Asc)
    })
    .await?;
    let events = entry_event::Entity::find()
        .filter(entry_event::Column::UserId.eq(user_id))
        .order_by(entry_event::Column::Id, Order::Asc)
        .all(db)
        .await?;
    let entry_tags = in_ids(db, &entry_ids, |ids| {
        entry_tag::Entity::find()
            .filter(entry_tag::Column::EntryId.is_in(ids))
            .order_by(entry_tag::Column::Id, Order::Asc)
    })
    .await?;
    let tags = tag::Entity::find()
        .filter(tag::Column::UserId.eq(user_id))
        .order_by(tag::Column::Id, Order::Asc)
        .all(db)
        .await?;
    let lists = list::Entity::find()
        .filter(list::Column::UserId.eq(user_id))
        .order_by(list::Column::Id, Order::Asc)
        .all(db)
        .await?;
    let list_ids: Vec<Uuid> = lists.iter().map(|l| l.id).collect();
    let items = in_ids(db, &list_ids, |ids| {
        list_item::Entity::find()
            .filter(list_item::Column::ListId.is_in(ids))
            .order_by(list_item::Column::Position, Order::Asc)
            .order_by(list_item::Column::Id, Order::Asc)
    })
    .await?;
    let blocked = blocklist::Entity::find()
        .filter(blocklist::Column::UserId.eq(user_id))
        .order_by(blocklist::Column::Id, Order::Asc)
        .all(db)
        .await?;
    let channels = notification_channel::Entity::find()
        .filter(notification_channel::Column::UserId.eq(user_id))
        .order_by(notification_channel::Column::Id, Order::Asc)
        .all(db)
        .await?;
    let subscriptions = notification_subscription::Entity::find()
        .filter(notification_subscription::Column::UserId.eq(user_id))
        .order_by(notification_subscription::Column::Id, Order::Asc)
        .all(db)
        .await?;

    let media_ids: BTreeSet<Uuid> = entries
        .iter()
        .map(|e| e.media_id)
        .chain(items.iter().map(|i| i.media_id))
        .chain(blocked.iter().map(|b| b.media_id))
        .collect();
    let media_ids: Vec<Uuid> = media_ids.into_iter().collect();
    let titles = titles(db, &media_ids).await?;
    let episode_ids: Vec<Uuid> = plays.iter().filter_map(|p| p.episode_id).collect();
    let episodes = catalog::episode_numbers(db, &episode_ids).await?;

    let mut plays_by_run: HashMap<Uuid, Vec<Play>> = HashMap::new();
    for p in plays {
        let episode = match p.episode_id {
            Some(id) => {
                let (season, number) = episodes.get(&id).copied().ok_or_else(|| {
                    ImportExportError::Internal(format!("episode {id} of play {} is gone", p.id))
                })?;
                Some(EpisodeRef { id, season, number })
            }
            None => None,
        };
        plays_by_run.entry(p.run_id).or_default().push(Play {
            id: p.id,
            episode,
            watched_at: p.watched_at,
            source: p.source,
            import_id: p.import_id,
            voided_at: p.voided_at,
            created_at: p.created_at,
            updated_at: p.updated_at,
        });
    }
    let mut runs_by_entry: HashMap<Uuid, Vec<Run>> = HashMap::new();
    for r in runs {
        runs_by_entry.entry(r.entry_id).or_default().push(Run {
            plays: plays_by_run.remove(&r.id).unwrap_or_default(),
            id: r.id,
            number: r.run_number,
            started_on: r.started_on,
            started_precision: r.started_precision,
            completed_on: r.completed_on,
            completed_precision: r.completed_precision,
            created_at: r.created_at,
            updated_at: r.updated_at,
        });
    }
    let mut events_by_entry: HashMap<Uuid, Vec<Event>> = HashMap::new();
    for e in events {
        events_by_entry.entry(e.entry_id).or_default().push(Event {
            id: e.id,
            kind: e.kind,
            before: e.before,
            after: e.after,
            created_at: e.created_at,
        });
    }
    let mut tags_by_entry: HashMap<Uuid, Vec<EntryTag>> = HashMap::new();
    for t in entry_tags {
        tags_by_entry.entry(t.entry_id).or_default().push(EntryTag {
            id: t.id,
            tag_id: t.tag_id,
            created_at: t.created_at,
            updated_at: t.updated_at,
        });
    }
    let mut items_by_list: HashMap<Uuid, Vec<ListItem>> = HashMap::new();
    for i in items {
        items_by_list.entry(i.list_id).or_default().push(ListItem {
            id: i.id,
            title_id: i.media_id,
            position: i.position,
            notes: i.notes,
            created_at: i.created_at,
            updated_at: i.updated_at,
        });
    }
    let lists: Vec<List> = lists
        .into_iter()
        .map(|l| {
            // A row an earlier restore stored unchecked may hold members in
            // a shape that never narrowed the list; they are left out
            // rather than refusing the whole backup.
            let filter = l.filter_query.as_ref().and_then(FilterQuery::from_stored);
            List {
                items: items_by_list.remove(&l.id).unwrap_or_default(),
                id: l.id,
                name: l.name,
                description: l.description,
                smart: l.is_smart,
                filter,
                deleted_at: l.deleted_at,
                created_at: l.created_at,
                updated_at: l.updated_at,
            }
        })
        .collect();
    let mut subs_by_channel: HashMap<Uuid, Vec<Subscription>> = HashMap::new();
    for s in subscriptions {
        subs_by_channel
            .entry(s.channel_id)
            .or_default()
            .push(Subscription {
                id: s.id,
                event_type: s.event_type,
                enabled: s.enabled,
                config: s.config.and_then(object),
                created_at: s.created_at,
                updated_at: s.updated_at,
            });
    }

    Ok(Backup {
        format: FORMAT.to_owned(),
        format_version: FORMAT_VERSION,
        exported_at: chrono::Utc::now(),
        generator: concat!("stacks ", env!("CARGO_PKG_VERSION")).to_owned(),
        secrets_included: options.include_secrets,
        user: Some(Profile {
            id: owner.id,
            display_name: owner.display_name,
            email: owner.email,
            oidc_issuer: owner.oidc_issuer,
            oidc_subject: owner.oidc_subject,
            created_at: owner.created_at,
        }),
        settings,
        titles,
        entries: entries
            .into_iter()
            .map(|e| Entry {
                runs: runs_by_entry.remove(&e.id).unwrap_or_default(),
                events: events_by_entry.remove(&e.id).unwrap_or_default(),
                tags: tags_by_entry.remove(&e.id).unwrap_or_default(),
                id: e.id,
                title_id: e.media_id,
                status: e.status,
                score: e.score,
                favourite: e.favourite,
                notes: e.notes,
                deleted_at: e.deleted_at,
                created_at: e.created_at,
                updated_at: e.updated_at,
            })
            .collect(),
        lists,
        tags: tags
            .into_iter()
            .map(|t| Tag {
                id: t.id,
                name: t.name,
                color: t.color,
                created_at: t.created_at,
                updated_at: t.updated_at,
            })
            .collect(),
        blocklist: blocked
            .into_iter()
            .map(|b| Blocked {
                id: b.id,
                title_id: b.media_id,
                created_at: b.created_at,
                updated_at: b.updated_at,
            })
            .collect(),
        notification_channels: channels
            .into_iter()
            .map(|c| Channel {
                subscriptions: subs_by_channel.remove(&c.id).unwrap_or_default(),
                id: c.id,
                channel_type: c.channel_type,
                name: c.name,
                enabled: c.enabled,
                config: if options.include_secrets {
                    object(c.config)
                } else {
                    None
                },
                created_at: c.created_at,
                updated_at: c.updated_at,
            })
            .collect(),
    })
}

fn object(v: serde_json::Value) -> Option<serde_json::Map<String, serde_json::Value>> {
    match v {
        serde_json::Value::Object(map) => Some(map),
        _ => None,
    }
}

/// The rows `query` selects for each bounded chunk of `ids`, in order.
async fn in_ids<C, E, F>(db: &C, ids: &[Uuid], query: F) -> Result<Vec<E::Model>, DbErr>
where
    C: ConnectionTrait,
    E: EntityTrait,
    F: Fn(Vec<Uuid>) -> sea_orm::Select<E>,
{
    let mut out = Vec::new();
    for chunk in ids.chunks(crate::db::INSERT_CHUNK) {
        out.extend(query(chunk.to_vec()).all(db).await?);
    }
    Ok(out)
}

/// The titles with every external id each one has.
async fn titles<C: ConnectionTrait>(db: &C, media_ids: &[Uuid]) -> Result<Vec<Title>, DbErr> {
    let rows = in_ids(db, media_ids, |ids| {
        media::Entity::find()
            .filter(media::Column::Id.is_in(ids))
            .order_by(media::Column::Id, Order::Asc)
    })
    .await?;
    let refs = in_ids(db, media_ids, |ids| {
        media_ref::Entity::find()
            .filter(media_ref::Column::MediaId.is_in(ids))
            .order_by(media_ref::Column::IsPrimary, Order::Desc)
            .order_by(media_ref::Column::Provider, Order::Asc)
            .order_by(media_ref::Column::ExternalId, Order::Asc)
    })
    .await?;
    let mut by_media: HashMap<Uuid, Vec<TitleRef>> = HashMap::new();
    for r in refs {
        by_media.entry(r.media_id).or_default().push(TitleRef {
            provider: r.provider,
            kind: r.kind,
            external_id: r.external_id,
            primary: r.is_primary,
        });
    }
    Ok(rows
        .into_iter()
        .map(|m| Title {
            refs: by_media.remove(&m.id).unwrap_or_default(),
            id: m.id,
            name: m.title,
            kind: m.kind,
        })
        .collect())
}
