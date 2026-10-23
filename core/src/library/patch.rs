//! Merge-semantics patches for entries and runs.

use chrono::NaiveDate;
use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
use serde::{Deserialize, Serialize};

use crate::entity::enums::{DatePrecision, EntryStatus};
use crate::entity::{library_entry, watch_run};
use crate::patch::Patch;

use super::LibraryError;

/// A change to a library entry. Absent fields are left exactly as stored.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct EntryPatch {
    /// Not nullable: absent means unchanged.
    #[serde(default)]
    pub status: Option<EntryStatus>,
    #[serde(default)]
    pub score: Patch<i16>,
    #[serde(default)]
    pub favourite: Option<bool>,
    #[serde(default)]
    pub notes: Patch<String>,
}

impl EntryPatch {
    pub fn is_empty(&self) -> bool {
        self.status.is_none()
            && self.score.is_absent()
            && self.favourite.is_none()
            && self.notes.is_absent()
    }

    pub fn validate(&self) -> Result<(), LibraryError> {
        if let Patch::Value(s) = self.score
            && !(0..=100).contains(&s)
        {
            return Err(LibraryError::InvalidScore);
        }
        Ok(())
    }

    /// The `UPDATE` for this patch: the primary key plus exactly the fields
    /// the patch mentions whose value actually changes. Everything else is
    /// `NotSet` and so is not written at all. `None` when nothing changes.
    pub fn to_update(&self, current: &library_entry::Model) -> Option<library_entry::ActiveModel> {
        let mut am = library_entry::ActiveModel {
            id: Unchanged(current.id),
            user_id: NotSet,
            media_id: NotSet,
            status: NotSet,
            score: NotSet,
            favourite: NotSet,
            notes: NotSet,
            deleted_at: NotSet,
            created_at: NotSet,
            updated_at: NotSet,
        };
        let mut changed = false;
        if let Some(s) = self.status.filter(|s| *s != current.status) {
            am.status = Set(s);
            changed = true;
        }
        if let Some(v) = self.score.into_change().filter(|v| *v != current.score) {
            am.score = Set(v);
            changed = true;
        }
        if let Some(f) = self.favourite.filter(|f| *f != current.favourite) {
            am.favourite = Set(f);
            changed = true;
        }
        if let Some(v) = self
            .notes
            .clone()
            .into_change()
            .filter(|v| *v != current.notes)
        {
            am.notes = Set(v);
            changed = true;
        }
        changed.then_some(am)
    }

    /// What the entry looks like after this patch (pure; used by tests and
    /// to reason about side effects before writing).
    pub fn applied_to(&self, current: &library_entry::Model) -> library_entry::Model {
        library_entry::Model {
            status: self.status.unwrap_or(current.status),
            score: self.score.apply(current.score),
            favourite: self.favourite.unwrap_or(current.favourite),
            notes: self.notes.clone().apply(current.notes.clone()),
            ..current.clone()
        }
    }
}

/// A run date together with how precisely it is known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct RunDate {
    pub date: NaiveDate,
    pub precision: DatePrecision,
}

/// A change to a watch run's dates. Absent fields are left as stored.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct RunPatch {
    #[serde(default)]
    pub started: Patch<RunDate>,
    #[serde(default)]
    pub completed: Patch<RunDate>,
}

impl RunPatch {
    /// The `UPDATE` for this patch, or `None` when nothing changes.
    pub fn to_update(&self, current: &watch_run::Model) -> Option<watch_run::ActiveModel> {
        let mut am = watch_run::ActiveModel {
            id: Unchanged(current.id),
            entry_id: NotSet,
            run_number: NotSet,
            started_on: NotSet,
            started_precision: NotSet,
            completed_on: NotSet,
            completed_precision: NotSet,
            created_at: NotSet,
            updated_at: NotSet,
        };
        let mut changed = false;
        if let Some(v) = self.started.into_change() {
            let (date, precision) = (v.map(|d| d.date), v.map(|d| d.precision));
            if date != current.started_on || precision != current.started_precision {
                am.started_on = Set(date);
                am.started_precision = Set(precision);
                changed = true;
            }
        }
        if let Some(v) = self.completed.into_change() {
            let (date, precision) = (v.map(|d| d.date), v.map(|d| d.precision));
            if date != current.completed_on || precision != current.completed_precision {
                am.completed_on = Set(date);
                am.completed_precision = Set(precision);
                changed = true;
            }
        }
        changed.then_some(am)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use sea_orm::ActiveValue;
    use uuid::Uuid;

    fn entry(
        status: EntryStatus,
        score: Option<i16>,
        fav: bool,
        notes: Option<String>,
    ) -> library_entry::Model {
        let now = chrono::Utc::now().into();
        library_entry::Model {
            id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            media_id: Uuid::now_v7(),
            status,
            score,
            favourite: fav,
            notes,
            deleted_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn status() -> impl Strategy<Value = EntryStatus> {
        prop_oneof![
            Just(EntryStatus::Planning),
            Just(EntryStatus::Watching),
            Just(EntryStatus::Completed),
            Just(EntryStatus::Paused),
            Just(EntryStatus::Dropped),
        ]
    }

    fn patch_of<T: std::fmt::Debug + Clone + 'static>(
        v: impl Strategy<Value = T> + 'static,
    ) -> impl Strategy<Value = Patch<T>> {
        prop_oneof![
            Just(Patch::Absent),
            Just(Patch::Null),
            v.prop_map(Patch::Value)
        ]
    }

    fn entry_patch() -> impl Strategy<Value = EntryPatch> {
        (
            proptest::option::of(status()),
            patch_of(0i16..=100),
            proptest::option::of(any::<bool>()),
            patch_of("[a-z ]{0,12}"),
        )
            .prop_map(|(status, score, favourite, notes)| EntryPatch {
                status,
                score,
                favourite,
                notes,
            })
    }

    proptest! {
        /// A patch never clears or changes a field it does not mention, and
        /// the generated UPDATE never touches a column the patch left absent.
        #[test]
        fn patch_never_clears_untouched_fields(
            st in status(),
            score in proptest::option::of(0i16..=100),
            fav in any::<bool>(),
            notes in proptest::option::of("[a-z ]{0,12}"),
            p in entry_patch(),
        ) {
            let cur = entry(st, score, fav, notes);
            let after = p.applied_to(&cur);
            if p.status.is_none() { prop_assert_eq!(after.status, cur.status); }
            if p.score.is_absent() { prop_assert_eq!(after.score, cur.score); }
            if p.favourite.is_none() { prop_assert_eq!(after.favourite, cur.favourite); }
            if p.notes.is_absent() { prop_assert_eq!(&after.notes, &cur.notes); }
            prop_assert_eq!(after.id, cur.id);
            prop_assert_eq!(after.created_at, cur.created_at);

            if let Some(am) = p.to_update(&cur) {
                if p.status.is_none() { prop_assert!(matches!(am.status, ActiveValue::NotSet)); }
                if p.score.is_absent() { prop_assert!(matches!(am.score, ActiveValue::NotSet)); }
                if p.favourite.is_none() { prop_assert!(matches!(am.favourite, ActiveValue::NotSet)); }
                if p.notes.is_absent() { prop_assert!(matches!(am.notes, ActiveValue::NotSet)); }
                prop_assert!(matches!(am.user_id, ActiveValue::NotSet));
                prop_assert!(matches!(am.media_id, ActiveValue::NotSet));
                prop_assert!(matches!(am.deleted_at, ActiveValue::NotSet));
                prop_assert!(matches!(am.created_at, ActiveValue::NotSet));
            } else {
                // No UPDATE means the patch changes nothing.
                prop_assert_eq!(after, cur);
            }
        }
    }

    #[test]
    fn score_out_of_range_is_rejected() {
        let p = EntryPatch {
            score: Patch::Value(101),
            ..Default::default()
        };
        assert!(matches!(p.validate(), Err(LibraryError::InvalidScore)));
        assert!(EntryPatch::default().validate().is_ok());
    }

    #[test]
    fn run_patch_sets_date_and_precision_together() {
        let now = chrono::Utc::now().into();
        let run = watch_run::Model {
            id: Uuid::now_v7(),
            entry_id: Uuid::now_v7(),
            run_number: 1,
            started_on: None,
            started_precision: None,
            completed_on: None,
            completed_precision: None,
            created_at: now,
            updated_at: now,
        };
        let d = RunDate {
            date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            precision: DatePrecision::Year,
        };
        let am = RunPatch {
            started: Patch::Value(d),
            ..Default::default()
        }
        .to_update(&run)
        .expect("changes");
        assert_eq!(am.started_on, ActiveValue::Set(Some(d.date)));
        assert_eq!(
            am.started_precision,
            ActiveValue::Set(Some(DatePrecision::Year))
        );
        assert!(matches!(am.completed_on, ActiveValue::NotSet));
        assert!(RunPatch::default().to_update(&run).is_none());
    }
}
