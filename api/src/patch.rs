//! A nullable member of a merge-patch body.
//!
//! `PATCH` bodies have merge semantics: a member left out leaves the stored
//! value alone, and an explicit `null` clears a nullable one. `Option<T>`
//! cannot say both, so such members are a [`Nullable<T>`] carrying
//! `#[serde(default)]`, which also keeps them out of the schema's `required`.

use kynos::openapi::Schema as OpenApiSchema;
use kynos::schema::{Schema, registry::Registry};
use serde::{Deserialize, Deserializer};

/// Absent, `null`, or a value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Nullable<T> {
    /// Not mentioned: leave the stored value alone.
    #[default]
    Absent,
    /// `null`: clear the stored value.
    Null,
    /// Set the stored value.
    Value(T),
}

impl<T> Nullable<T> {
    pub fn into_patch(self) -> stacks_core::Patch<T> {
        match self {
            Self::Absent => stacks_core::Patch::Absent,
            Self::Null => stacks_core::Patch::Null,
            Self::Value(v) => stacks_core::Patch::Value(v),
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Nullable<U> {
        match self {
            Self::Absent => Nullable::Absent,
            Self::Null => Nullable::Null,
            Self::Value(v) => Nullable::Value(f(v)),
        }
    }

    pub fn as_value(&self) -> Option<&T> {
        match self {
            Self::Value(v) => Some(v),
            Self::Absent | Self::Null => None,
        }
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Nullable<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Reached only for a member that is present; an absent one takes the
        // `#[serde(default)]`.
        Ok(match Option::<T>::deserialize(deserializer)? {
            None => Self::Null,
            Some(v) => Self::Value(v),
        })
    }
}

/// Described as the member's type or `null`, exactly as an `Option<T>` is.
impl<T: Schema> Schema for Nullable<T> {
    fn schema(registry: &mut Registry) -> OpenApiSchema {
        <Option<T> as Schema>::schema(registry)
    }

    fn check_constraints(
        &self,
        at: kynos::schema::constraints::Pointer<'_>,
        violations: &mut kynos::schema::constraints::Violations,
    ) {
        if let Self::Value(v) = self {
            v.check_constraints(at, violations);
        }
    }
}

// A `#[schema(...)]` bound on a clearable member applies to its value;
// `null` and absent pass.

impl<T: kynos::schema::constraints::Numeric> kynos::schema::constraints::Numeric for Nullable<T> {
    fn number(&self) -> Option<f64> {
        self.as_value().and_then(T::number)
    }
}

impl<T: kynos::schema::constraints::Textual> kynos::schema::constraints::Textual for Nullable<T> {
    fn text(&self) -> Option<&str> {
        self.as_value().and_then(T::text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize)]
    struct Body {
        #[serde(default)]
        notes: Nullable<String>,
    }

    #[test]
    fn absent_null_and_value_are_three_things() {
        let parse = |s: &str| serde_json::from_str::<Body>(s).map(|b| b.notes).ok();
        assert_eq!(parse("{}"), Some(Nullable::Absent));
        assert_eq!(parse(r#"{"notes":null}"#), Some(Nullable::Null));
        assert_eq!(parse(r#"{"notes":"x"}"#), Some(Nullable::Value("x".into())));
    }
}
