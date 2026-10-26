//! Bursts of same-kind events written at one moment (a bulk "mark up to"),
//! and how a restore into an account whose ids are all taken keeps them.
//! Split out of `backup.rs` to keep that file within its length limit.

use sea_orm::ConnectionTrait;
use stacks_core::import_export::backup::{self, Backup, RestoreOptions};
use uuid::Uuid;

use super::{bytes, common, export, fixed_dataset};

/// Every entry's events as (title, kind, time), sorted: what a restore into
/// another account reproduces, whatever ids it has to assign.
pub fn event_log(b: &Backup) -> Vec<(Uuid, String, chrono::DateTime<chrono::FixedOffset>)> {
    let mut out: Vec<_> = b
        .entries
        .iter()
        .flat_map(|e| {
            e.events
                .iter()
                .map(|ev| (e.title_id, format!("{:?}", ev.kind), ev.created_at))
        })
        .collect();
    out.sort();
    out
}

/// An account restored into before every burst member was kept holds one
/// copy of each burst. Restoring the same backup again adds the members it
/// is missing, rather than letting that one copy stand for all of them.
#[tokio::test]
async fn restoring_again_fills_in_a_burst_an_earlier_restore_kept_one_copy_of() {
    let t = crate::test_db!();
    let ada = common::user(&t.db).await;
    fixed_dataset(&t.db, ada.id).await;
    let original = export(&t.db, ada.id).await;
    let eve = common::user(&t.db).await;
    backup::restore(&t.db, eve.id, &bytes(&original), RestoreOptions::default())
        .await
        .expect("the first restore succeeds");

    // Leave eve one copy of each same-kind, same-moment burst, as a restore
    // that matched every member to the first copy would have.
    let full = common::count(&t.db, "entry_events").await;
    t.db.execute_unprepared(&format!(
        "BEGIN; SET LOCAL stacks.allow_purge = 'on'; \
         DELETE FROM entry_events e WHERE e.user_id = '{}' AND EXISTS ( \
           SELECT 1 FROM entry_events o WHERE o.entry_id = e.entry_id \
           AND o.kind = e.kind AND o.created_at = e.created_at AND o.id < e.id); \
         COMMIT;",
        eve.id
    ))
    .await
    .expect("trim eve's bursts");
    let trimmed = common::count(&t.db, "entry_events").await;
    assert!(trimmed < full, "the dataset holds a burst to trim");
    assert_ne!(
        event_log(&export(&t.db, eve.id).await),
        event_log(&original)
    );

    let report = backup::restore(&t.db, eve.id, &bytes(&original), RestoreOptions::default())
        .await
        .expect("the second restore succeeds");
    assert_eq!(report.events.inserted as i64, full - trimmed);
    assert_eq!(
        event_log(&export(&t.db, eve.id).await),
        event_log(&original)
    );
    assert_eq!(common::count(&t.db, "entry_events").await, full);
}
