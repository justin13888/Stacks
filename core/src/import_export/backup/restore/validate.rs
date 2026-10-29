//! Checking a backup document before a restore touches the database.

use std::collections::HashSet;

use serde_json::Value;
use uuid::Uuid;

use crate::entity::enums::PlaySource;
use crate::lists::filter::check_filter;
use crate::lists::fractional_index;
use crate::media::Provider;
use crate::user::UserSettings;

use super::super::format::Backup;
use super::RestoreError;

/// Check every reference inside the document, and every rule the schema
/// enforces on what the restore writes, before touching the database: a
/// document that breaks one is [`RestoreError::Invalid`], never a database
/// error.
pub(super) fn validate(b: &Backup) -> Result<(), RestoreError> {
    let invalid = |m: String| Err(RestoreError::Invalid(m));
    let titles: HashSet<Uuid> = b.titles.iter().map(|t| t.id).collect();
    for t in &b.titles {
        let primaries: Vec<_> = t.refs.iter().filter(|r| r.primary).collect();
        let [primary] = primaries.as_slice() else {
            return invalid(format!(
                "title {} has {} primary ids",
                t.id,
                primaries.len()
            ));
        };
        if Provider::from_ref_provider(primary.provider).is_none()
            || primary.external_id.parse::<i32>().is_err()
        {
            return invalid(format!(
                "title {}'s primary id {}:{} is not a TMDB or AniList id",
                t.id, primary.provider, primary.external_id
            ));
        }
    }
    let tags: HashSet<Uuid> = b.tags.iter().map(|t| t.id).collect();
    for e in &b.entries {
        if !titles.contains(&e.title_id) {
            return invalid(format!("entry {} names a title the backup lacks", e.id));
        }
        if let Some(s) = e.score
            && !(0..=100).contains(&s)
        {
            return invalid(format!("entry {} has score {s}, outside 0–100", e.id));
        }
        if let Some(t) = e.tags.iter().find(|t| !tags.contains(&t.tag_id)) {
            return invalid(format!(
                "entry {} has a tag the backup lacks ({})",
                e.id, t.tag_id
            ));
        }
        let mut numbers = HashSet::new();
        for r in &e.runs {
            if r.number < 1 || !numbers.insert(r.number) {
                return invalid(format!(
                    "entry {} has a duplicate or invalid run {}",
                    e.id, r.number
                ));
            }
            if r.started_on.is_some() != r.started_precision.is_some()
                || r.completed_on.is_some() != r.completed_precision.is_some()
            {
                return invalid(format!(
                    "run {} has a date without its precision, or a precision without its date",
                    r.id
                ));
            }
            for p in &r.plays {
                if p.import_id.is_some() && p.source != PlaySource::Import {
                    return invalid(format!(
                        "play {} names an import but its source is {}",
                        p.id, p.source
                    ));
                }
                if let Some(ep) = &p.episode
                    && (ep.season < 0 || ep.number < 0)
                {
                    return invalid(format!("play {} names a negative episode number", p.id));
                }
            }
        }
    }
    for t in &b.titles {
        if let Some(r) = t.refs.iter().find(|r| r.external_id.is_empty()) {
            return invalid(format!("title {} has a blank {} id", t.id, r.provider));
        }
    }
    if let Some(t) = b.tags.iter().find(|t| t.name.trim().is_empty()) {
        return invalid(format!("tag {} has a blank name", t.id));
    }
    for l in &b.lists {
        if l.name.trim().is_empty() {
            return invalid(format!("list {} has a blank name", l.id));
        }
        if let Some(f) = &l.filter
            && let Err(e) = check_filter(f.clone())
        {
            return invalid(format!("list {}: {e}", l.id));
        }
        let mut seen = HashSet::new();
        for i in &l.items {
            if !titles.contains(&i.title_id) {
                return invalid(format!("list item {} names a title the backup lacks", i.id));
            }
            if !fractional_index::is_valid(&i.position) || !seen.insert(i.position.as_str()) {
                return invalid(format!(
                    "list item {} has an invalid or repeated position",
                    i.id
                ));
            }
        }
    }
    if let Some(b) = b.blocklist.iter().find(|b| !titles.contains(&b.title_id)) {
        return invalid(format!(
            "blocklist row {} names a title the backup lacks",
            b.id
        ));
    }
    UserSettings::from_json(&Value::Object(b.settings.clone()))
        .map_err(|e| RestoreError::Invalid(format!("settings: {e}")))?;
    Ok(())
}
