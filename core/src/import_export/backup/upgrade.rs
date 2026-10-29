//! Reading a backup document of any version this release knows.
//!
//! A document is identified by its `format` member and versioned by
//! `format_version`. A version newer than [`FORMAT_VERSION`] is refused: this
//! release cannot know what it holds. An older one is upgraded one version at
//! a time by the explicit migrations in [`MIGRATIONS`], each a function from
//! the JSON of version `n` to the JSON of version `n + 1`, so a document
//! written by any earlier release reads as the current [`Backup`].
//!
//! Version 0 is the pre-release export (`{"version": 1, "entries": [...]}`
//! with one row per live entry), which carried no `format` member.

use serde_json::{Map, Value, json};
use uuid::Uuid;

use super::format::{Backup, FORMAT, FORMAT_VERSION};
use crate::media::MediaId;

/// Why a document cannot be read as a backup.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    /// Not JSON, not a Stacks backup, or not a valid one.
    #[error("{0}")]
    Unreadable(String),
    /// Written by a newer Stacks than this one.
    #[error(
        "the backup is format version {found}; this release reads up to version {supported}. \
         Restore it with a newer Stacks."
    )]
    UnsupportedVersion { found: u64, supported: u32 },
}

/// One upgrade step: the document of version `n` (its index) to `n + 1`.
type Migration = fn(Value) -> Result<Value, ReadError>;

/// Every upgrade step, oldest first. `MIGRATIONS[n]` turns version `n` into
/// `n + 1`, so there are exactly [`FORMAT_VERSION`] of them.
const MIGRATIONS: [Migration; FORMAT_VERSION as usize] = [v0_to_v1, v1_to_v2];

/// A document read and brought up to the current version.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadBackup {
    pub backup: Backup,
    /// The version the document was written in.
    pub read_version: u32,
}

/// Parse a backup file of any supported version.
pub fn read(bytes: &[u8]) -> Result<ReadBackup, ReadError> {
    let doc: Value = serde_json::from_slice(bytes)
        .map_err(|e| ReadError::Unreadable(format!("the backup is not valid JSON: {e}")))?;
    let (mut doc, version) = identify(doc)?;
    for migrate in &MIGRATIONS[version as usize..] {
        doc = migrate(doc)?;
    }
    let backup: Backup = serde_json::from_value(doc).map_err(|e| {
        ReadError::Unreadable(format!(
            "the backup is not a valid version {FORMAT_VERSION} document: {e}"
        ))
    })?;
    Ok(ReadBackup {
        backup,
        read_version: version,
    })
}

/// The document's version, refusing what is not a backup or is too new.
fn identify(doc: Value) -> Result<(Value, u32), ReadError> {
    let Value::Object(map) = &doc else {
        return Err(not_a_backup());
    };
    match map.get("format") {
        Some(Value::String(f)) if f == FORMAT => {}
        Some(_) => return Err(not_a_backup()),
        None if map.contains_key("version") && map.contains_key("entries") => {
            return Ok((doc, 0));
        }
        None => return Err(not_a_backup()),
    }
    let found = map
        .get("format_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            ReadError::Unreadable("the backup has no numeric `format_version`".into())
        })?;
    match u32::try_from(found) {
        Ok(v) if v <= FORMAT_VERSION => Ok((doc, v)),
        _ => Err(ReadError::UnsupportedVersion {
            found,
            supported: FORMAT_VERSION,
        }),
    }
}

fn not_a_backup() -> ReadError {
    ReadError::Unreadable(format!(
        "the file is not a Stacks backup (no `\"format\": \"{FORMAT}\"` member)"
    ))
}

/// Version 0 → 1: each pre-release row (`title`, `status`, `score`,
/// `started_on`, `completed_on`) becomes an entry with its title and, when
/// either date is known, a first run carrying it at day precision. Version 0
/// held nothing else; its `progress` count named no episodes, so it cannot
/// become plays and is dropped.
fn v0_to_v1(doc: Value) -> Result<Value, ReadError> {
    let bad = |what: &str| ReadError::Unreadable(format!("version 0 backup: {what}"));
    let exported_at = doc
        .get("exported_at")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("no `exported_at`"))?
        .to_owned();
    let rows = doc
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("`entries` is not an array"))?;
    let mut titles = Vec::with_capacity(rows.len());
    let mut entries = Vec::with_capacity(rows.len());
    for (i, row) in rows.iter().enumerate() {
        let at = |what: &str| bad(&format!("entry {i}: {what}"));
        let raw = row
            .get("title")
            .and_then(Value::as_str)
            .ok_or_else(|| at("no `title`"))?;
        let id = MediaId::parse(raw).map_err(|e| at(&e.to_string()))?;
        let title_id = Uuid::now_v7();
        titles.push(json!({
            "id": title_id,
            "name": raw,
            "kind": id.kind,
            "refs": [{
                "provider": id.provider.as_ref_provider(),
                "kind": id.kind,
                "external_id": id.external_id(),
                "primary": true,
            }],
        }));
        let date = |key: &str| row.get(key).cloned().filter(|v| !v.is_null());
        let (started, completed) = (date("started_on"), date("completed_on"));
        let precision = |d: &Option<Value>| d.as_ref().map(|_| json!("day"));
        let runs = if started.is_some() || completed.is_some() {
            vec![json!({
                "id": Uuid::now_v7(),
                "number": 1,
                "started_precision": precision(&started),
                "started_on": started,
                "completed_precision": precision(&completed),
                "completed_on": completed,
                "created_at": exported_at,
                "updated_at": exported_at,
                "plays": [],
            })]
        } else {
            Vec::new()
        };
        entries.push(json!({
            "id": Uuid::now_v7(),
            "title_id": title_id,
            "status": row.get("status").cloned().unwrap_or(Value::Null),
            "score": row.get("score").cloned().unwrap_or(Value::Null),
            "favourite": false,
            "notes": null,
            "deleted_at": null,
            "created_at": exported_at,
            "updated_at": exported_at,
            "runs": runs,
            "events": [],
            "tags": [],
        }));
    }
    Ok(json!({
        "format": FORMAT,
        "format_version": 1,
        "exported_at": exported_at,
        "generator": "stacks (pre-release export, upgraded)",
        "secrets_included": false,
        "user": null,
        "settings": Map::new(),
        "titles": titles,
        "entries": entries,
        "lists": [],
        "tags": [],
        "blocklist": [],
        "notification_channels": [],
    }))
}

/// The members a version 1 smart-list filter could hold.
const V1_FILTER_MEMBERS: [&str; 8] = [
    "media_type",
    "genre",
    "year_min",
    "year_max",
    "status",
    "score_min",
    "score_max",
    "sort",
];

/// Version 1 → 2: a smart list's `filter`, any object in version 1, becomes
/// the typed filter of version 2, which adds the `category`, `tag` and
/// `user_score_*` members and the `library` source. A member version 1 did
/// not define meant nothing to it and is dropped, as is a `null` one, so the
/// filter selects what it selected. A filter that is not an object is
/// refused.
fn v1_to_v2(mut doc: Value) -> Result<Value, ReadError> {
    let lists = doc.get_mut("lists").and_then(Value::as_array_mut);
    for (i, list) in lists.into_iter().flatten().enumerate() {
        match list.get_mut("filter") {
            None | Some(Value::Null) => {}
            Some(Value::Object(members)) => {
                members.retain(|k, v| V1_FILTER_MEMBERS.contains(&k.as_str()) && !v.is_null());
            }
            Some(_) => {
                return Err(ReadError::Unreadable(format!(
                    "version 1 backup: list {i}'s filter is not an object"
                )));
            }
        }
    }
    if let Value::Object(map) = &mut doc {
        map.insert("format_version".into(), json!(2));
    }
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v1(lists: Value) -> Value {
        json!({
            "format": FORMAT, "format_version": 1,
            "exported_at": "2026-10-01T12:00:00Z", "generator": "stacks 0.1.0",
            "secrets_included": false, "user": null, "settings": {},
            "titles": [], "entries": [], "lists": lists, "tags": [],
            "blocklist": [], "notification_channels": [],
        })
    }

    fn list(filter: Value) -> Value {
        json!({
            "id": Uuid::nil(), "name": "Smart", "description": null,
            "smart": true, "filter": filter, "deleted_at": null,
            "created_at": "2026-10-01T12:00:00Z",
            "updated_at": "2026-10-01T12:00:00Z", "items": [],
        })
    }

    #[test]
    fn a_version_1_filter_keeps_what_it_selected() {
        let doc = v1(json!([
            list(json!({
                "media_type": ["show"], "genre": ["Drama"], "year_min": 2000,
                "year_max": null, "status": ["watching"], "score_min": 7,
                "score_max": 9, "sort": "trending",
                // Not version 1 members, so they never narrowed the list.
                "tag": ["comfort"], "category": "anime", "user_score_min": 80,
            })),
            list(Value::Null),
        ]));
        let read = read(doc.to_string().as_bytes()).expect("upgrades");
        assert_eq!(read.read_version, 1);
        let filter = read.backup.lists[0].filter.clone().expect("a filter");
        assert_eq!(
            filter,
            crate::media::filter::FilterQuery {
                media_type: Some(vec!["show".into()]),
                genre: Some(vec!["Drama".into()]),
                year_min: Some(2000),
                status: Some(vec!["watching".into()]),
                score_min: Some(7),
                score_max: Some(9),
                sort: Some("trending".into()),
                ..Default::default()
            }
        );
        assert_eq!(read.backup.lists[1].filter, None);
    }

    #[test]
    fn a_version_1_filter_that_is_not_an_object_is_unreadable() {
        let doc = v1(json!([list(json!([1, 2]))]));
        assert!(matches!(
            read(doc.to_string().as_bytes()),
            Err(ReadError::Unreadable(why)) if why.contains("filter is not an object")
        ));
    }

    #[test]
    fn a_newer_version_is_refused_by_number() {
        let doc = json!({"format": FORMAT, "format_version": FORMAT_VERSION + 1});
        let err = read(doc.to_string().as_bytes()).unwrap_err();
        assert_eq!(
            err,
            ReadError::UnsupportedVersion {
                found: u64::from(FORMAT_VERSION + 1),
                supported: FORMAT_VERSION
            }
        );
        let huge = json!({"format": FORMAT, "format_version": u64::MAX});
        assert!(matches!(
            read(huge.to_string().as_bytes()),
            Err(ReadError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn what_is_not_a_backup_is_unreadable() {
        for raw in [
            "not json",
            "[]",
            r#"{"format": "something-else", "format_version": 1}"#,
            r#"{"hello": "world"}"#,
            r#"{"format": "stacks-backup"}"#,
            r#"{"format": "stacks-backup", "format_version": 1}"#,
        ] {
            assert!(
                matches!(read(raw.as_bytes()), Err(ReadError::Unreadable(_))),
                "{raw}"
            );
        }
    }

    #[test]
    fn a_version_0_export_upgrades_to_the_current_format() {
        let v0 = json!({
            "version": 1,
            "exported_at": "2026-10-01T12:00:00Z",
            "entries": [
                {"title": "tmdb:movie:550", "status": "completed", "score": 90,
                 "progress": null, "started_on": "2024-01-05", "completed_on": null},
                {"title": "anilist:show:21", "status": "watching", "score": null,
                 "progress": 12, "started_on": null, "completed_on": null}
            ]
        });
        let read = read(v0.to_string().as_bytes()).expect("upgrades");
        assert_eq!(read.read_version, 0);
        let b = read.backup;
        assert_eq!(b.format_version, FORMAT_VERSION);
        assert_eq!(b.titles.len(), 2);
        assert_eq!(b.entries.len(), 2);
        let fight_club = &b.entries[0];
        assert_eq!(fight_club.title_id, b.titles[0].id);
        assert_eq!(fight_club.score, Some(90));
        let [run] = fight_club.runs.as_slice() else {
            panic!("one run")
        };
        assert_eq!(run.started_on, chrono::NaiveDate::from_ymd_opt(2024, 1, 5));
        assert_eq!(
            run.started_precision,
            Some(crate::entity::enums::DatePrecision::Day)
        );
        assert_eq!(run.completed_on, None);
        assert!(b.entries[1].runs.is_empty());
        assert_eq!(b.titles[1].refs[0].external_id, "21");
    }

    #[test]
    fn there_is_one_migration_per_older_version() {
        assert_eq!(MIGRATIONS.len(), FORMAT_VERSION as usize);
    }
}
