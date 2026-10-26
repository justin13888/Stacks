//! Unioning a backup entry's events into the local event log.

use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use uuid::Uuid;

use crate::entity::entry_event;

use super::super::format;
use super::{RestoreError, Restorer, remap};

impl Restorer<'_> {
    /// Add `ev` to the entry's log unless the entry already holds it, under
    /// its own id or (restored before under a fresh one) as the entry's
    /// event of the same kind written at the same moment.
    ///
    /// A local event matches at most one backup event per restore, and an
    /// event this restore inserted matches none, so a burst of same-kind
    /// events written at one moment (a bulk "mark up to") keeps every
    /// member, and restoring the burst again pairs each with its own copy.
    pub(super) async fn union_event(
        &mut self,
        ev: &format::Event,
        entry_id: Uuid,
    ) -> Result<(), RestoreError> {
        let existing = entry_event::Entity::find_by_id(ev.id).one(self.txn).await?;
        if existing.as_ref().is_some_and(|x| x.entry_id == entry_id) {
            self.claimed_events.insert(ev.id);
            self.report.events.unchanged += 1;
            return Ok(());
        }
        if existing.is_some() {
            let twin = entry_event::Entity::find()
                .filter(entry_event::Column::EntryId.eq(entry_id))
                .filter(entry_event::Column::Kind.eq(ev.kind))
                .filter(entry_event::Column::CreatedAt.eq(ev.created_at))
                .order_by_asc(entry_event::Column::Id)
                .all(self.txn)
                .await?
                .into_iter()
                .find(|x| !self.claimed_events.contains(&x.id));
            if let Some(twin) = twin {
                self.claimed_events.insert(twin.id);
                self.ids.insert(ev.id, twin.id);
                self.report.events.unchanged += 1;
                return Ok(());
            }
        }
        let id = if existing.is_some() {
            Uuid::now_v7()
        } else {
            ev.id
        };
        self.ids.insert(ev.id, id);
        self.claimed_events.insert(id);
        let mut before = ev.before.clone();
        let mut after = ev.after.clone();
        for side in [&mut before, &mut after].into_iter().flatten() {
            remap(side, &self.ids);
        }
        entry_event::ActiveModel {
            id: Set(id),
            entry_id: Set(entry_id),
            user_id: Set(self.user_id),
            kind: Set(ev.kind),
            before: Set(before),
            after: Set(after),
            created_at: Set(ev.created_at),
        }
        .insert(self.txn)
        .await?;
        self.report.events.inserted += 1;
        Ok(())
    }
}
