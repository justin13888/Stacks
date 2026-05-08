use serde::{Deserialize, Serialize};

use crate::import_export::error::ImportExportError;
use crate::import_export::models::{ExternalIds, ImportEntry};

/// Raw entry as it appears in a MyAnimeList export XML/JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MalEntry {
    pub mal_id: u64,
    pub title: String,
    pub status: Option<String>,
    pub score: Option<i32>,
    pub episodes_watched: Option<i32>,
}

/// Parse a MAL XML export file (bytes) into a list of `ImportEntry` values.
///
/// MAL exports look like:
/// ```xml
/// <myanimelist>
///   <anime>
///     <series_animedb_id>1</series_animedb_id>
///     <series_title>Cowboy Bebop</series_title>
///     <my_status>Completed</my_status>
///     <my_score>9</my_score>
///   </anime>
/// </myanimelist>
/// ```
pub fn parse_mal_xml(data: &[u8]) -> Result<Vec<ImportEntry>, ImportExportError> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_reader(data);
    reader.config_mut().trim_text(true);

    let mut entries = Vec::new();
    let mut buf = Vec::new();
    let mut current: Option<MalEntry> = None;
    let mut current_tag: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if name == "anime" || name == "manga" {
                    current = Some(MalEntry {
                        mal_id: 0,
                        title: String::new(),
                        status: None,
                        score: None,
                        episodes_watched: None,
                    });
                }
                current_tag = Some(name);
            }
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default().into_owned();
                if let (Some(tag), Some(item)) = (&current_tag, current.as_mut()) {
                    match tag.as_str() {
                        "series_animedb_id" | "manga_mangadb_id" => {
                            item.mal_id = text.trim().parse().unwrap_or(0);
                        }
                        "series_title" | "manga_title" => {
                            item.title = text;
                        }
                        "my_status" => {
                            item.status = Some(text.to_lowercase());
                        }
                        "my_score" => {
                            item.score = text.trim().parse().ok();
                        }
                        "my_watched_episodes" | "my_read_chapters" => {
                            item.episodes_watched = text.trim().parse().ok();
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if (name == "anime" || name == "manga") && current.is_some() {
                    if let Some(item) = current.take() {
                        entries.push(map_entry(item));
                    }
                }
                current_tag = None;
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ImportExportError::ParseError(format!("xml: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(entries)
}

/// Parse a MAL JSON export into a list of `ImportEntry` values.
pub fn parse_mal_json(data: &[u8]) -> Result<Vec<ImportEntry>, ImportExportError> {
    let raw: Vec<MalEntry> =
        serde_json::from_slice(data).map_err(|e| ImportExportError::ParseError(e.to_string()))?;
    Ok(raw.into_iter().map(map_entry).collect())
}

/// Map a `MalEntry` to a generic `ImportEntry`.
pub fn map_entry(entry: MalEntry) -> ImportEntry {
    ImportEntry {
        external_ids: ExternalIds {
            mal_id: Some(entry.mal_id.to_string()),
            ..Default::default()
        },
        status: entry.status,
        score: entry.score,
        progress: entry.episodes_watched,
        started_at: None,
        completed_at: None,
        source: "mal".to_string(),
    }
}
