//! The typed shape of `users.settings`.

use serde::{Deserialize, Serialize};

/// How scores are shown and entered. Stored scores are always 0–100.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreFormat {
    #[default]
    Point100,
    Point10Decimal,
    Point10,
    Point5,
    Point3,
}

/// Which title to show for titles that have several.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleLanguage {
    #[default]
    English,
    Romaji,
    Native,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spoilers {
    #[default]
    Hide,
    Show,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// A user's preferences. Every field has a default, so a stored `{}` (or a
/// document written before a field existed) reads as the defaults; unknown
/// keys are preserved by [`super::UserService::update_settings`]'s merge.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserSettings {
    pub score_format: ScoreFormat,
    pub title_language: TitleLanguage,
    pub spoilers: Spoilers,
    pub theme: Theme,
}

impl UserSettings {
    /// Read the typed settings out of a stored document, ignoring keys this
    /// version does not know.
    pub fn from_json(v: &serde_json::Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(v.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_document_reads_as_defaults() {
        let s = UserSettings::from_json(&serde_json::json!({})).unwrap();
        assert_eq!(s, UserSettings::default());
    }

    #[test]
    fn known_values_round_trip() {
        let v = serde_json::json!({
            "score_format": "point_10_decimal",
            "title_language": "romaji",
            "spoilers": "show",
            "theme": "dark",
        });
        let s = UserSettings::from_json(&v).unwrap();
        assert_eq!(s.score_format, ScoreFormat::Point10Decimal);
        assert_eq!(serde_json::to_value(s).unwrap(), v);
    }

    #[test]
    fn invalid_values_are_rejected() {
        assert!(UserSettings::from_json(&serde_json::json!({"theme": "neon"})).is_err());
    }
}
