//! The public title reference, as a parameter and a property.

use std::fmt;
use std::str::FromStr;

use kynos::openapi::model::schema::object::SchemaObject;
use kynos::openapi::model::schema::types::{SchemaType, TypeSet};
use kynos::openapi::{ComponentName, Schema as OpenApiSchema};
use kynos::schema::{ParamValue, Schema, registry::Registry};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use stacks_core::media::{MediaId, MediaIdError};

/// A title's public, self-describing reference: `<provider>:<kind>:<id>`,
/// such as `tmdb:movie:550`, `tmdb:show:1396` or `anilist:show:21`.
///
/// Every title-scoped path and body names a title this way. Stacks' own row
/// ids never appear in the API, so a reference stays meaningful across
/// deployments and in exports.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TitleRef(pub MediaId);

impl TitleRef {
    pub fn media_id(&self) -> &MediaId {
        &self.0
    }
}

impl From<MediaId> for TitleRef {
    fn from(id: MediaId) -> Self {
        Self(id)
    }
}

impl fmt::Display for TitleRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for TitleRef {
    type Err = MediaIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        MediaId::parse(s).map(Self)
    }
}

impl Serialize for TitleRef {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for TitleRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

impl Schema for TitleRef {
    fn schema(_registry: &mut Registry) -> OpenApiSchema {
        OpenApiSchema::Object(Box::new(SchemaObject {
            ty: Some(TypeSet::One(SchemaType::String)),
            description: Some(
                "A title reference, `<provider>:<kind>:<id>`: provider `tmdb` or `anilist`, \
                 kind `movie` or `show`."
                    .to_owned(),
            ),
            examples: Some(vec![
                serde_json::json!("tmdb:movie:550"),
                serde_json::json!("anilist:show:21"),
            ]),
            min_length: Some(1),
            ..SchemaObject::default()
        }))
    }

    fn name() -> Option<ComponentName> {
        ComponentName::sanitized("TitleRef").ok()
    }
}

impl ParamValue for TitleRef {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reference_round_trips_through_text_and_json() {
        let r: TitleRef = "tmdb:show:1396".parse().expect("valid");
        assert_eq!(r.to_string(), "tmdb:show:1396");
        let json = serde_json::to_value(&r).expect("serializes");
        assert_eq!(json, serde_json::json!("tmdb:show:1396"));
        let back: TitleRef = serde_json::from_value(json).expect("deserializes");
        assert_eq!(back, r);
    }

    #[test]
    fn a_malformed_reference_is_refused() {
        assert!("tmdb:550".parse::<TitleRef>().is_err());
        assert!(serde_json::from_value::<TitleRef>(serde_json::json!("imdb:movie:1")).is_err());
    }
}
