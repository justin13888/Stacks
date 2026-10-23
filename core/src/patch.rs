//! Tri-state field updates for merge-semantics patches.
//!
//! A `PATCH` must leave every field it does not mention exactly as it was, and
//! still be able to clear a nullable field on purpose. `Option<T>` cannot say
//! both, so nullable fields in a patch are a [`Patch<T>`]:
//!
//! | JSON           | `Patch<T>`        | effect            |
//! |----------------|-------------------|-------------------|
//! | field absent   | `Patch::Absent`   | column untouched  |
//! | `null`         | `Patch::Null`     | column cleared    |
//! | a value        | `Patch::Value(v)` | column set to `v` |
//!
//! Use `#[serde(default)]` on the field so an absent key deserialises to
//! `Absent` (serde only calls the deserialiser for keys that are present).

use sea_orm::ActiveValue;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One field of a merge patch. See the [module docs](self).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Patch<T> {
    /// Not mentioned: leave the stored value alone.
    #[default]
    Absent,
    /// Explicit `null`: clear the stored value.
    Null,
    /// Set the stored value.
    Value(T),
}

impl<T> Patch<T> {
    pub fn is_absent(&self) -> bool {
        matches!(self, Patch::Absent)
    }

    /// The new value of a nullable column, or `None` when the field is absent.
    pub fn into_change(self) -> Option<Option<T>> {
        match self {
            Patch::Absent => None,
            Patch::Null => Some(None),
            Patch::Value(v) => Some(Some(v)),
        }
    }

    /// The `ActiveValue` for a nullable column: `NotSet` when absent, so the
    /// column is left out of the `UPDATE` entirely.
    pub fn into_active_value(self) -> ActiveValue<Option<T>>
    where
        T: Into<sea_orm::Value>,
        Option<T>: Into<sea_orm::Value>,
    {
        match self.into_change() {
            None => ActiveValue::NotSet,
            Some(v) => ActiveValue::Set(v),
        }
    }

    /// Apply to a current value, returning what the column would hold after.
    pub fn apply(self, current: Option<T>) -> Option<T> {
        match self {
            Patch::Absent => current,
            Patch::Null => None,
            Patch::Value(v) => Some(v),
        }
    }

    pub fn as_ref(&self) -> Patch<&T> {
        match self {
            Patch::Absent => Patch::Absent,
            Patch::Null => Patch::Null,
            Patch::Value(v) => Patch::Value(v),
        }
    }
}

impl<T> From<Option<Option<T>>> for Patch<T> {
    fn from(v: Option<Option<T>>) -> Self {
        match v {
            None => Patch::Absent,
            Some(None) => Patch::Null,
            Some(Some(v)) => Patch::Value(v),
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Patch<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Reached only when the key is present (absent keys take `Default`).
        Ok(match Option::<T>::deserialize(deserializer)? {
            None => Patch::Null,
            Some(v) => Patch::Value(v),
        })
    }
}

impl<T: Serialize> Serialize for Patch<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Patch::Absent | Patch::Null => serializer.serialize_none(),
            Patch::Value(v) => v.serialize(serializer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize)]
    struct Body {
        #[serde(default)]
        notes: Patch<String>,
    }

    #[test]
    fn json_distinguishes_absent_null_and_value() {
        let absent: Body = serde_json::from_str("{}").unwrap();
        let null: Body = serde_json::from_str(r#"{"notes":null}"#).unwrap();
        let value: Body = serde_json::from_str(r#"{"notes":"hi"}"#).unwrap();
        assert_eq!(absent.notes, Patch::Absent);
        assert_eq!(null.notes, Patch::Null);
        assert_eq!(value.notes, Patch::Value("hi".to_string()));
    }

    #[test]
    fn apply_only_changes_what_was_mentioned() {
        let cur = Some(1);
        assert_eq!(Patch::Absent.apply(cur), Some(1));
        assert_eq!(Patch::<i32>::Null.apply(cur), None);
        assert_eq!(Patch::Value(2).apply(cur), Some(2));
    }

    #[test]
    fn absent_is_not_set_in_the_update() {
        let v: ActiveValue<Option<i32>> = Patch::Absent.into_active_value();
        assert!(matches!(v, ActiveValue::NotSet));
        let v: ActiveValue<Option<i32>> = Patch::Null.into_active_value();
        assert_eq!(v, ActiveValue::Set(None));
    }

    #[test]
    fn option_of_option_converts() {
        assert_eq!(Patch::<u8>::from(None), Patch::Absent);
        assert_eq!(Patch::<u8>::from(Some(None)), Patch::Null);
        assert_eq!(Patch::from(Some(Some(3u8))), Patch::Value(3));
    }
}
