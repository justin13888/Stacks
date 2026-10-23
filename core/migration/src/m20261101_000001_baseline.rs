//! The v0.1.0 baseline schema.
//!
//! MIGRATIONS ARE APPEND-ONLY AFTER v0.1.0. Once a release has shipped this
//! file, it is never edited again: every later schema change is a new
//! `mYYYYMMDD_NNNNNN_<name>.rs` migration registered after it in `lib.rs`.
//! Editing a migration that has already run leaves every existing database
//! silently out of step with the code.
//!
//! The schema is written as SQL rather than through the SeaQuery builder
//! because most of what makes it lossless — CHECK constraints, `text[]`,
//! partial unique indexes, the `C` collation on list positions, the derived
//! `entry_progress` view and the append-only triggers — is plain Postgres that
//! the builder would only obscure. It targets PostgreSQL 18 (`uuidv7()`).
//!
//! Data-safety rules the schema enforces on its own, whatever the code does:
//!
//! - every primary key is a UUIDv7 (`uuidv7()` default; Rust also sets it);
//! - every enum-like column has a CHECK constraint;
//! - every foreign key is `ON DELETE RESTRICT` — there is no cascade anywhere,
//!   so no delete can silently take user data with it;
//! - `plays` and `entry_events` are append-only: a play may only gain a
//!   `voided_at`, an event never changes, and neither can be deleted;
//! - users, library entries, runs and lists cannot be hard-deleted.
//!
//! The one sanctioned way past the delete guards is the explicit admin purge
//! (`stacks_core::user::purge`), which sets `stacks.allow_purge` for its own
//! transaction only.

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261101_000001_baseline"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(UP).await?;
        db.execute_unprepared(&updated_at_triggers()).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(DOWN).await?;
        Ok(())
    }
}

/// Tables that carry an `updated_at` column maintained by trigger.
const UPDATED_AT_TABLES: &[&str] = &[
    "users",
    "api_tokens",
    "media",
    "media_refs",
    "seasons",
    "episodes",
    "library_entries",
    "watch_runs",
    "plays",
    "lists",
    "list_items",
    "tags",
    "entry_tags",
    "blocklist",
    "imports",
    "import_items",
    "jobs",
    "notification_channels",
    "notification_subscriptions",
];

const UP: &str = r#"
-- ---------------------------------------------------------------- functions

CREATE FUNCTION stacks_set_updated_at() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END
$$;

-- Hard deletes are refused unless the explicit admin purge opted in for its
-- own transaction with `SET LOCAL stacks.allow_purge = 'on'`.
CREATE FUNCTION stacks_forbid_delete() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF current_setting('stacks.allow_purge', true) = 'on' THEN
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'rows in % cannot be deleted; use the soft-delete or void path', TG_TABLE_NAME
        USING ERRCODE = 'restrict_violation';
END
$$;

CREATE FUNCTION stacks_forbid_update() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'rows in % are immutable', TG_TABLE_NAME
        USING ERRCODE = 'restrict_violation';
END
$$;

-- A play is append-only: the only permitted change is setting `voided_at`
-- once. Undo voids a play; it is never edited or deleted.
CREATE FUNCTION stacks_plays_void_only() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.id <> OLD.id
        OR NEW.run_id <> OLD.run_id
        OR NEW.episode_id IS DISTINCT FROM OLD.episode_id
        OR NEW.watched_at IS DISTINCT FROM OLD.watched_at
        OR NEW.source <> OLD.source
        OR NEW.import_id IS DISTINCT FROM OLD.import_id
        OR NEW.created_at <> OLD.created_at
    THEN
        RAISE EXCEPTION 'plays are append-only: only voided_at may be set'
            USING ERRCODE = 'restrict_violation';
    END IF;
    IF OLD.voided_at IS NOT NULL AND NEW.voided_at IS DISTINCT FROM OLD.voided_at THEN
        RAISE EXCEPTION 'a voided play stays voided'
            USING ERRCODE = 'restrict_violation';
    END IF;
    RETURN NEW;
END
$$;

-- ---------------------------------------------------------------- accounts

CREATE TABLE users (
    id              uuid PRIMARY KEY DEFAULT uuidv7(),
    oidc_issuer     text NOT NULL,
    oidc_subject    text NOT NULL,
    email           text,
    display_name    text NOT NULL CHECK (length(btrim(display_name)) > 0),
    role            text NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin')),
    settings        jsonb NOT NULL DEFAULT '{}'::jsonb,
    deactivated_at  timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT users_oidc_identity_key UNIQUE (oidc_issuer, oidc_subject),
    CONSTRAINT users_settings_object CHECK (jsonb_typeof(settings) = 'object'),
    CONSTRAINT users_settings_score_format CHECK (
        NOT settings ? 'score_format'
        OR settings->>'score_format' IN ('point_100', 'point_10_decimal', 'point_10', 'point_5', 'point_3')),
    CONSTRAINT users_settings_title_language CHECK (
        NOT settings ? 'title_language'
        OR settings->>'title_language' IN ('english', 'romaji', 'native')),
    CONSTRAINT users_settings_spoilers CHECK (
        NOT settings ? 'spoilers'
        OR settings->>'spoilers' IN ('hide', 'show')),
    CONSTRAINT users_settings_theme CHECK (
        NOT settings ? 'theme'
        OR settings->>'theme' IN ('system', 'light', 'dark'))
);

CREATE TABLE sessions (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    token_hash    text NOT NULL UNIQUE CHECK (token_hash ~ '^[0-9a-f]{64}$'),
    user_agent    text,
    ip            text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    last_seen_at  timestamptz NOT NULL DEFAULT now(),
    expires_at    timestamptz NOT NULL
);
CREATE INDEX sessions_user_id_idx ON sessions (user_id);

CREATE TABLE api_tokens (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    name          text NOT NULL CHECK (length(btrim(name)) > 0),
    token_prefix  text NOT NULL CHECK (token_prefix LIKE 'stk\_%'),
    lookup_hash   text NOT NULL UNIQUE CHECK (lookup_hash ~ '^[0-9a-f]{64}$'),
    scopes        text[] NOT NULL CHECK (
        cardinality(scopes) > 0 AND scopes <@ ARRAY['read', 'write', 'admin']::text[]),
    expires_at    timestamptz,
    last_used_at  timestamptz,
    revoked_at    timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX api_tokens_user_id_idx ON api_tokens (user_id);

CREATE TABLE oidc_login_states (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    state          text NOT NULL UNIQUE,
    nonce          text NOT NULL,
    pkce_verifier  text NOT NULL,
    redirect_to    text,
    expires_at     timestamptz NOT NULL,
    created_at     timestamptz NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------- catalog snapshot

CREATE TABLE media (
    id                 uuid PRIMARY KEY DEFAULT uuidv7(),
    kind               text NOT NULL CHECK (kind IN ('movie', 'show')),
    format             text CHECK (format IN ('tv', 'tv_short', 'movie', 'ova', 'ona', 'special', 'miniseries')),
    category           text NOT NULL CHECK (category IN (
                           'movie', 'series', 'anime', 'k-drama', 'j-drama', 'c-drama', 'tw-drama', 'th-drama')),
    title              text NOT NULL,
    original_title     text,
    romaji_title       text,
    alt_titles         jsonb NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(alt_titles) = 'array'),
    overview           text,
    poster_url         text,
    backdrop_url       text,
    start_date         date,
    end_date           date,
    status             text CHECK (status IN ('upcoming', 'releasing', 'finished', 'cancelled', 'hiatus')),
    runtime_minutes    integer CHECK (runtime_minutes >= 0),
    genres             text[] NOT NULL DEFAULT '{}',
    origin_countries   text[] NOT NULL DEFAULT '{}',
    original_language  text,
    season_count       integer CHECK (season_count >= 0),
    episode_count      integer CHECK (episode_count >= 0),
    raw                jsonb,
    fetched_at         timestamptz,
    refresh_error      text,
    refresh_failures   integer NOT NULL DEFAULT 0 CHECK (refresh_failures >= 0),
    -- When the refresh job last ran new-episode detection; only that job
    -- writes it. NULL: the title's episodes have not been baselined.
    episodes_checked_at timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE media_refs (
    id           uuid PRIMARY KEY DEFAULT uuidv7(),
    media_id     uuid NOT NULL REFERENCES media (id) ON DELETE RESTRICT,
    provider     text NOT NULL CHECK (provider IN ('tmdb', 'anilist', 'mal', 'imdb', 'tvdb')),
    kind         text NOT NULL CHECK (kind IN ('movie', 'show')),
    external_id  text NOT NULL CHECK (length(external_id) > 0),
    is_primary   boolean NOT NULL DEFAULT false,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT media_refs_external_key UNIQUE (provider, kind, external_id)
);
CREATE INDEX media_refs_media_id_idx ON media_refs (media_id);
-- The primary ref is the public, self-describing id (`tmdb:movie:550`).
CREATE UNIQUE INDEX media_refs_one_primary_idx ON media_refs (media_id) WHERE is_primary;

CREATE TABLE seasons (
    id             uuid PRIMARY KEY DEFAULT uuidv7(),
    media_id       uuid NOT NULL REFERENCES media (id) ON DELETE RESTRICT,
    number         integer NOT NULL CHECK (number >= 0),
    title          text,
    overview       text,
    air_date       date,
    poster_url     text,
    episode_count  integer CHECK (episode_count >= 0),
    -- Episodes numbered up to this have been through new-episode detection;
    -- only the refresh job writes it (a user read never moves it).
    announced_through integer CHECK (announced_through >= 0),
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT seasons_media_number_key UNIQUE (media_id, number)
);

CREATE TABLE episodes (
    id               uuid PRIMARY KEY DEFAULT uuidv7(),
    media_id         uuid NOT NULL REFERENCES media (id) ON DELETE RESTRICT,
    season_id        uuid NOT NULL REFERENCES seasons (id) ON DELETE RESTRICT,
    number           integer NOT NULL CHECK (number >= 0),
    absolute_number  integer CHECK (absolute_number >= 1),
    title            text,
    overview         text,
    air_date         date,
    runtime_minutes  integer CHECK (runtime_minutes >= 0),
    still_url        text,
    created_at       timestamptz NOT NULL DEFAULT now(),
    updated_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT episodes_season_number_key UNIQUE (season_id, number)
);
CREATE INDEX episodes_media_id_idx ON episodes (media_id);

-- ---------------------------------------------------------------- imports (referenced by plays)

CREATE TABLE imports (
    id                 uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id            uuid NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    source             text NOT NULL CHECK (source IN ('mal_xml', 'mal_json', 'tmdb_json')),
    status             text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    conflict_strategy  text NOT NULL DEFAULT 'skip' CHECK (conflict_strategy IN ('skip', 'overwrite', 'merge')),
    original_filename  text,
    original_file      bytea,
    original_size      bigint NOT NULL CHECK (original_size >= 0),
    total_count        integer NOT NULL DEFAULT 0 CHECK (total_count >= 0),
    applied_count      integer NOT NULL DEFAULT 0 CHECK (applied_count >= 0),
    skipped_count      integer NOT NULL DEFAULT 0 CHECK (skipped_count >= 0),
    unmatched_count    integer NOT NULL DEFAULT 0 CHECK (unmatched_count >= 0),
    failed_count       integer NOT NULL DEFAULT 0 CHECK (failed_count >= 0),
    error              text,
    started_at         timestamptz,
    completed_at       timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX imports_user_id_idx ON imports (user_id, id DESC);

CREATE TABLE import_items (
    id                uuid PRIMARY KEY DEFAULT uuidv7(),
    import_id         uuid NOT NULL REFERENCES imports (id) ON DELETE RESTRICT,
    row_number        integer NOT NULL CHECK (row_number >= 1),
    raw               jsonb NOT NULL,
    -- `pending_retry`: the metadata provider was unavailable when the row was
    -- matched; the import job retries it rather than failing it.
    outcome           text NOT NULL DEFAULT 'pending' CHECK (outcome IN (
                          'pending', 'pending_retry', 'applied', 'skipped', 'unmatched', 'failed')),
    matched_media_id  uuid REFERENCES media (id) ON DELETE RESTRICT,
    confidence        real CHECK (confidence >= 0 AND confidence <= 1),
    -- The titles an unmatched row might be, best first, for resolving it by
    -- hand: `[{"ref": "anilist:show:21", "title": …, "year": …, "confidence": …}]`.
    candidates        jsonb CHECK (candidates IS NULL OR jsonb_typeof(candidates) = 'array'),
    error             text,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT import_items_row_key UNIQUE (import_id, row_number)
);

-- ---------------------------------------------------------------- tracking

CREATE TABLE library_entries (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    media_id    uuid NOT NULL REFERENCES media (id) ON DELETE RESTRICT,
    status      text NOT NULL CHECK (status IN ('planning', 'watching', 'completed', 'paused', 'dropped')),
    score       smallint CHECK (score BETWEEN 0 AND 100),
    favourite   boolean NOT NULL DEFAULT false,
    notes       text,
    deleted_at  timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT library_entries_user_media_key UNIQUE (user_id, media_id)
);
CREATE INDEX library_entries_user_updated_idx ON library_entries (user_id, updated_at DESC);

CREATE TABLE watch_runs (
    id                   uuid PRIMARY KEY DEFAULT uuidv7(),
    entry_id             uuid NOT NULL REFERENCES library_entries (id) ON DELETE RESTRICT,
    run_number           integer NOT NULL CHECK (run_number >= 1),
    started_on           date,
    started_precision    text CHECK (started_precision IN ('day', 'month', 'year')),
    completed_on         date,
    completed_precision  text CHECK (completed_precision IN ('day', 'month', 'year')),
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT watch_runs_entry_run_key UNIQUE (entry_id, run_number),
    CONSTRAINT watch_runs_started_precision CHECK ((started_on IS NULL) = (started_precision IS NULL)),
    CONSTRAINT watch_runs_completed_precision CHECK ((completed_on IS NULL) = (completed_precision IS NULL))
);

CREATE TABLE plays (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    run_id      uuid NOT NULL REFERENCES watch_runs (id) ON DELETE RESTRICT,
    episode_id  uuid REFERENCES episodes (id) ON DELETE RESTRICT,
    watched_at  timestamptz,
    source      text NOT NULL CHECK (source IN ('manual', 'bulk', 'import')),
    import_id   uuid REFERENCES imports (id) ON DELETE RESTRICT,
    voided_at   timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT plays_import_source CHECK (import_id IS NULL OR source = 'import')
);
CREATE INDEX plays_run_id_idx ON plays (run_id);
CREATE INDEX plays_episode_id_idx ON plays (episode_id);

CREATE TABLE entry_events (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    entry_id    uuid NOT NULL REFERENCES library_entries (id) ON DELETE RESTRICT,
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    kind        text NOT NULL CHECK (kind IN (
                    'entry_created', 'entry_updated', 'entry_deleted', 'entry_restored',
                    'run_started', 'run_updated', 'play_logged', 'play_voided')),
    before      jsonb,
    after       jsonb,
    created_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX entry_events_user_idx ON entry_events (user_id, id DESC);
CREATE INDEX entry_events_entry_idx ON entry_events (entry_id, id DESC);

-- Progress is derived from runs and plays, never stored: voided plays do not
-- count, and "current run" is the highest-numbered run.
CREATE VIEW entry_progress AS
SELECT
    e.id AS entry_id,
    count(DISTINCT r.id)::integer AS run_count,
    greatest(count(DISTINCT r.id) - 1, 0)::integer AS rewatch_count,
    count(p.id) FILTER (WHERE p.voided_at IS NULL) AS play_count,
    count(p.id) FILTER (WHERE p.voided_at IS NULL AND r.run_number = cur.run_number) AS current_run_plays,
    count(DISTINCT p.episode_id) FILTER (WHERE p.voided_at IS NULL AND r.run_number = cur.run_number)
        AS current_run_episodes,
    min(p.watched_at) FILTER (WHERE p.voided_at IS NULL) AS first_watched_at,
    max(p.watched_at) FILTER (WHERE p.voided_at IS NULL) AS last_watched_at
FROM library_entries e
LEFT JOIN LATERAL (
    SELECT max(run_number) AS run_number FROM watch_runs WHERE entry_id = e.id
) cur ON true
LEFT JOIN watch_runs r ON r.entry_id = e.id
LEFT JOIN plays p ON p.run_id = r.id
GROUP BY e.id;

-- ---------------------------------------------------------------- organisation

CREATE TABLE lists (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    name          text NOT NULL CHECK (length(btrim(name)) > 0),
    description   text,
    is_smart      boolean NOT NULL DEFAULT false,
    filter_query  jsonb CHECK (filter_query IS NULL OR jsonb_typeof(filter_query) = 'object'),
    deleted_at    timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX lists_user_id_idx ON lists (user_id);

-- `position` is a base-62 fractional index compared bytewise (collation "C"),
-- so an item can always be placed between two others without renumbering.
CREATE TABLE list_items (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    list_id     uuid NOT NULL REFERENCES lists (id) ON DELETE RESTRICT,
    media_id    uuid NOT NULL REFERENCES media (id) ON DELETE RESTRICT,
    position    text COLLATE "C" NOT NULL CHECK (position ~ '^[0-9A-Za-z]*[1-9A-Za-z]$'),
    notes       text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT list_items_list_media_key UNIQUE (list_id, media_id),
    -- Deferred, so a whole-list reorder can pass through transient duplicates.
    CONSTRAINT list_items_list_position_key UNIQUE (list_id, position) DEFERRABLE INITIALLY DEFERRED
);

CREATE TABLE tags (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    name        text NOT NULL CHECK (length(btrim(name)) > 0),
    color       text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT tags_user_name_key UNIQUE (user_id, name)
);

CREATE TABLE entry_tags (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    entry_id    uuid NOT NULL REFERENCES library_entries (id) ON DELETE RESTRICT,
    tag_id      uuid NOT NULL REFERENCES tags (id) ON DELETE RESTRICT,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT entry_tags_entry_tag_key UNIQUE (entry_id, tag_id)
);
CREATE INDEX entry_tags_tag_id_idx ON entry_tags (tag_id);

CREATE TABLE blocklist (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    media_id    uuid NOT NULL REFERENCES media (id) ON DELETE RESTRICT,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT blocklist_user_media_key UNIQUE (user_id, media_id)
);

-- ---------------------------------------------------------------- ops

CREATE TABLE jobs (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    job_type      text NOT NULL CHECK (job_type IN ('import', 'notification', 'metadata_refresh')),
    payload       jsonb,
    status        text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    attempts      integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts  integer NOT NULL DEFAULT 3 CHECK (max_attempts >= 1),
    run_at        timestamptz NOT NULL DEFAULT now(),
    started_at    timestamptz,
    completed_at  timestamptz,
    error         text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX jobs_claim_idx ON jobs (run_at) WHERE status = 'pending';

CREATE TABLE notification_channels (
    id            uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id       uuid NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    channel_type  text NOT NULL CHECK (channel_type IN ('webhook', 'apprise')),
    name          text NOT NULL CHECK (length(btrim(name)) > 0),
    config        jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(config) = 'object'),
    enabled       boolean NOT NULL DEFAULT true,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX notification_channels_user_id_idx ON notification_channels (user_id);

CREATE TABLE notification_subscriptions (
    id          uuid PRIMARY KEY DEFAULT uuidv7(),
    user_id     uuid NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    channel_id  uuid NOT NULL REFERENCES notification_channels (id) ON DELETE RESTRICT,
    event_type  text NOT NULL CHECK (event_type IN ('new_episode', 'upcoming_release', 'import_completed')),
    enabled     boolean NOT NULL DEFAULT true,
    config      jsonb CHECK (config IS NULL OR jsonb_typeof(config) = 'object'),
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT notification_subscriptions_key UNIQUE (user_id, event_type, channel_id)
);

-- ---------------------------------------------------------------- guards

CREATE TRIGGER users_forbid_delete BEFORE DELETE ON users
    FOR EACH ROW EXECUTE FUNCTION stacks_forbid_delete();
CREATE TRIGGER library_entries_forbid_delete BEFORE DELETE ON library_entries
    FOR EACH ROW EXECUTE FUNCTION stacks_forbid_delete();
CREATE TRIGGER watch_runs_forbid_delete BEFORE DELETE ON watch_runs
    FOR EACH ROW EXECUTE FUNCTION stacks_forbid_delete();
CREATE TRIGGER plays_forbid_delete BEFORE DELETE ON plays
    FOR EACH ROW EXECUTE FUNCTION stacks_forbid_delete();
CREATE TRIGGER plays_void_only BEFORE UPDATE ON plays
    FOR EACH ROW EXECUTE FUNCTION stacks_plays_void_only();
CREATE TRIGGER entry_events_forbid_delete BEFORE DELETE ON entry_events
    FOR EACH ROW EXECUTE FUNCTION stacks_forbid_delete();
CREATE TRIGGER entry_events_forbid_update BEFORE UPDATE ON entry_events
    FOR EACH ROW EXECUTE FUNCTION stacks_forbid_update();
CREATE TRIGGER lists_forbid_delete BEFORE DELETE ON lists
    FOR EACH ROW EXECUTE FUNCTION stacks_forbid_delete();
"#;

const DOWN: &str = r#"
DROP VIEW IF EXISTS entry_progress;
DROP TABLE IF EXISTS
    notification_subscriptions,
    notification_channels,
    jobs,
    blocklist,
    entry_tags,
    tags,
    list_items,
    lists,
    entry_events,
    plays,
    watch_runs,
    library_entries,
    import_items,
    imports,
    episodes,
    seasons,
    media_refs,
    media,
    oidc_login_states,
    api_tokens,
    sessions,
    users;
DROP FUNCTION IF EXISTS stacks_plays_void_only();
DROP FUNCTION IF EXISTS stacks_forbid_update();
DROP FUNCTION IF EXISTS stacks_forbid_delete();
DROP FUNCTION IF EXISTS stacks_set_updated_at();
"#;

/// `updated_at` triggers, generated per table so the list above is the one
/// place a new table opts in.
fn updated_at_triggers() -> String {
    UPDATED_AT_TABLES
        .iter()
        .map(|t| {
            format!(
                "CREATE TRIGGER {t}_set_updated_at BEFORE UPDATE ON {t} \
                 FOR EACH ROW EXECUTE FUNCTION stacks_set_updated_at();\n"
            )
        })
        .collect()
}
