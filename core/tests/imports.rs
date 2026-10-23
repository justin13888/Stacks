//! Submitting imports: an unreadable file is refused before anything is
//! stored, and listings never carry the uploaded files.

mod common;

use stacks_core::entity::enums::{ConflictStrategy, ImportSource};
use stacks_core::import_export::ImportExportError;
use stacks_core::import_export::import::pipeline;

const TMDB: &[u8] = br#"[{"tmdb_id": 550, "title": "Fight Club", "media_type": "movie"}]"#;

#[tokio::test]
async fn an_unreadable_file_is_refused_before_anything_is_stored() {
    let t = test_db!();
    let user = common::user(&t.db).await;
    let refused = pipeline::submit(
        &t.db,
        user.id,
        ImportSource::MalXml,
        Some("export.json".into()),
        TMDB.to_vec(),
        ConflictStrategy::Skip,
        1024,
    )
    .await;
    assert!(matches!(refused, Err(ImportExportError::ParseError(_))));
    assert_eq!(common::count(&t.db, "imports").await, 0);
    assert_eq!(common::count(&t.db, "jobs").await, 0);

    let too_big = pipeline::submit(
        &t.db,
        user.id,
        ImportSource::TmdbJson,
        None,
        TMDB.to_vec(),
        ConflictStrategy::Skip,
        8,
    )
    .await;
    assert!(matches!(too_big, Err(ImportExportError::TooLarge { .. })));

    pipeline::submit(
        &t.db,
        user.id,
        ImportSource::TmdbJson,
        None,
        TMDB.to_vec(),
        ConflictStrategy::Skip,
        1024,
    )
    .await
    .unwrap();
    assert_eq!(common::count(&t.db, "imports").await, 1);
    assert_eq!(common::count(&t.db, "jobs").await, 1);
}

#[tokio::test]
async fn listings_page_newest_first_without_the_files() {
    let t = test_db!();
    let ada = common::user(&t.db).await;
    let eve = common::user(&t.db).await;
    let mut ids = Vec::new();
    for _ in 0..3 {
        let row = pipeline::submit(
            &t.db,
            ada.id,
            ImportSource::TmdbJson,
            None,
            TMDB.to_vec(),
            ConflictStrategy::Merge,
            1024,
        )
        .await
        .unwrap();
        assert!(row.original_file.is_some(), "the file is kept");
        ids.push(row.id);
    }
    ids.reverse();

    let first = pipeline::list_for_user(&t.db, ada.id, None, 2)
        .await
        .unwrap();
    assert_eq!(first.iter().map(|i| i.id).collect::<Vec<_>>(), ids[..2]);
    assert!(first.iter().all(|i| i.original_file.is_none()));
    assert!(first.iter().all(|i| i.original_size == TMDB.len() as i64));
    let rest = pipeline::list_for_user(&t.db, ada.id, Some(ids[1]), 2)
        .await
        .unwrap();
    assert_eq!(rest.iter().map(|i| i.id).collect::<Vec<_>>(), ids[2..]);
    assert!(
        pipeline::list_for_user(&t.db, eve.id, None, 10)
            .await
            .unwrap()
            .is_empty()
    );

    let one = pipeline::find_for_user(&t.db, ada.id, ids[0])
        .await
        .unwrap()
        .expect("ada's import");
    assert!(one.original_file.is_none());
    assert_eq!(one.conflict_strategy, ConflictStrategy::Merge);
    assert!(
        pipeline::find_for_user(&t.db, eve.id, ids[0])
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        pipeline::find_item(&t.db, ids[0], 1)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        pipeline::items_with_matches(&t.db, ids[0])
            .await
            .unwrap()
            .is_empty()
    );
}
