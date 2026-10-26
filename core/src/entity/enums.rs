//! The closed vocabularies stored in text columns.
//!
//! Each enum mirrors a CHECK constraint in the baseline migration, so an
//! invalid value is rejected twice: by the Rust type when it is built, and by
//! the database if anything bypasses the type (raw SQL, a future bug). The
//! `string_value`s here and the CHECK lists there must stay in step; the
//! schema tests in `tests/schema.rs` insert every variant to prove it.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Generates a text-backed `DeriveActiveEnum` with a matching serde form.
macro_rules! text_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $( $(#[$vmeta:meta])* $variant:ident = $value:literal ),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, DeriveActiveEnum, Serialize,
            Deserialize, schemars::JsonSchema,
        )]
        #[sea_orm(rs_type = "String", db_type = "Text")]
        pub enum $name {
            $(
                $(#[$vmeta])*
                #[sea_orm(string_value = $value)]
                #[serde(rename = $value)]
                $variant,
            )+
        }

        impl $name {
            /// The stored / wire token.
            pub fn as_str(self) -> &'static str {
                match self {
                    $( $name::$variant => $value, )+
                }
            }

            /// Parse a stored / wire token.
            pub fn parse(s: &str) -> Option<Self> {
                match s {
                    $( $value => Some($name::$variant), )+
                    _ => None,
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

text_enum! {
    /// A user's role.
    Role { User = "user", Admin = "admin" }
}

text_enum! {
    /// Whether a title is a single film or an episodic show.
    MediaKind { Movie = "movie", Show = "show" }
}

text_enum! {
    /// The release format of a title (AniList's vocabulary, which subsumes TMDB's).
    MediaFormat {
        Tv = "tv",
        TvShort = "tv_short",
        Movie = "movie",
        Ova = "ova",
        Ona = "ona",
        Special = "special",
        Miniseries = "miniseries",
    }
}

text_enum! {
    /// The browsing category, derived from provider, format, origin and genre
    /// by [`crate::media::category::derive_category`].
    Category {
        Movie = "movie",
        Series = "series",
        Anime = "anime",
        KDrama = "k-drama",
        JDrama = "j-drama",
        CDrama = "c-drama",
        TwDrama = "tw-drama",
        ThDrama = "th-drama",
    }
}

text_enum! {
    /// Normalised release status across providers.
    ReleaseStatus {
        Upcoming = "upcoming",
        Releasing = "releasing",
        Finished = "finished",
        Cancelled = "cancelled",
        Hiatus = "hiatus",
    }
}

text_enum! {
    /// A provider an external id belongs to.
    RefProvider {
        Tmdb = "tmdb",
        Anilist = "anilist",
        Mal = "mal",
        Imdb = "imdb",
        Tvdb = "tvdb",
    }
}

text_enum! {
    /// Where a library entry stands.
    EntryStatus {
        Planning = "planning",
        Watching = "watching",
        Completed = "completed",
        Paused = "paused",
        Dropped = "dropped",
    }
}

text_enum! {
    /// How precisely a run's start or completion date is known.
    DatePrecision { Day = "day", Month = "month", Year = "year" }
}

text_enum! {
    /// How a play was recorded.
    PlaySource { Manual = "manual", Bulk = "bulk", Import = "import" }
}

text_enum! {
    /// What an `entry_events` row records.
    EntryEventKind {
        EntryCreated = "entry_created",
        EntryUpdated = "entry_updated",
        EntryDeleted = "entry_deleted",
        EntryRestored = "entry_restored",
        RunStarted = "run_started",
        RunUpdated = "run_updated",
        PlayLogged = "play_logged",
        PlayVoided = "play_voided",
        PlayRestored = "play_restored",
    }
}

text_enum! {
    /// A supported import file format.
    ImportSource { MalXml = "mal_xml", MalJson = "mal_json", TmdbJson = "tmdb_json" }
}

text_enum! {
    /// Lifecycle of an import (and, with the same vocabulary, of a job).
    ImportStatus {
        Pending = "pending",
        Running = "running",
        Completed = "completed",
        Failed = "failed",
    }
}

text_enum! {
    /// How an import treats a title already in the library.
    #[derive(Default)]
    ConflictStrategy {
        #[default]
        Skip = "skip",
        Overwrite = "overwrite",
        Merge = "merge",
    }
}

text_enum! {
    /// What happened to one row of an import.
    ImportOutcome {
        Pending = "pending",
        /// The metadata provider was unavailable; the import job retries it.
        PendingRetry = "pending_retry",
        Applied = "applied",
        Skipped = "skipped",
        Unmatched = "unmatched",
        Failed = "failed",
    }
}

text_enum! {
    /// A background job's type.
    JobType {
        Import = "import",
        Notification = "notification",
        MetadataRefresh = "metadata_refresh",
    }
}

text_enum! {
    /// A background job's lifecycle.
    JobStatus {
        Pending = "pending",
        Running = "running",
        Completed = "completed",
        Failed = "failed",
    }
}

text_enum! {
    /// A notification delivery channel.
    ChannelType { Webhook = "webhook", Apprise = "apprise" }
}

text_enum! {
    /// A notification event a user can subscribe to.
    NotificationEventType {
        NewEpisode = "new_episode",
        ImportCompleted = "import_completed",
    }
}

text_enum! {
    /// An API-token scope.
    Scope { Read = "read", Write = "write", Admin = "admin" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::Iterable;

    #[test]
    fn every_variant_round_trips_through_its_token() {
        fn check<E: Iterable + Copy + PartialEq + std::fmt::Debug>(
            as_str: fn(E) -> &'static str,
            parse: fn(&str) -> Option<E>,
        ) {
            for v in E::iter() {
                assert_eq!(parse(as_str(v)), Some(v));
            }
            assert_eq!(parse("not-a-token"), None);
        }
        check(EntryStatus::as_str, EntryStatus::parse);
        check(Category::as_str, Category::parse);
        check(RefProvider::as_str, RefProvider::parse);
        check(EntryEventKind::as_str, EntryEventKind::parse);
        check(Scope::as_str, Scope::parse);
    }

    #[test]
    fn serde_uses_the_stored_token() {
        assert_eq!(
            serde_json::to_value(Category::KDrama).unwrap(),
            serde_json::json!("k-drama")
        );
        assert_eq!(
            serde_json::from_value::<EntryStatus>(serde_json::json!("paused")).unwrap(),
            EntryStatus::Paused
        );
    }
}
