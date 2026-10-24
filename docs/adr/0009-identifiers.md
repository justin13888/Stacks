# ADR 0009: UUIDv7 row ids and self-describing title refs

- Status: accepted
- Date: 2026-10-24
- Issue: #12 (records the verdict taken in #5)

## Context

Two kinds of identifier cross the API. Rows the user owns (entries, runs,
plays, lists, tags, sessions, imports) need ids that are unguessable enough
to expose, cheap to index, and stable through a backup and restore. Titles
come from external providers that number their catalogues independently:
TMDB movie 550 and TMDB show 550 are different titles, and AniList has its
own numbering.

The prototype used ULIDs for rows and stored a bare `<provider>:<numeric>`
for titles, which lost the movie/show distinction cameo needs to route a
call.

## Decision

- **Rows: UUIDv7** (`uuid` with the `v7` feature, `Uuid::now_v7()`), stored
  as PostgreSQL `uuid`. They are time-ordered, so inserts append to the
  primary-key index instead of scattering like v4, and they sort roughly by
  creation, which keyset pagination uses as a tiebreaker.
- **Titles: the public ref `<provider>:<kind>:<numeric>`**, e.g.
  `tmdb:movie:550`, `tmdb:show:1396`, `anilist:show:21`
  ([`core/src/media/id.rs`](../../core/src/media/id.rs)). It is what URLs
  and the API carry. Internally a title is a `media` row with a UUIDv7, and
  `media_refs (provider, kind, external_id)` maps every known external id
  (TMDB, AniList, MAL, IMDb, TVDB) to it, so the same title found through two
  providers is one row.
- A backup carries row ids and each title's refs, so a restore matches titles
  by ref and rows by id or natural key (docs/backup-format.md).

## Alternatives rejected

- **ULID.** Equivalent ordering, but a text or bytea column in PostgreSQL and
  a non-standard type in every client; UUIDv7 gets the same property in the
  native `uuid` type.
- **UUIDv4.** Random inserts fragment the B-tree and give no order to fall
  back on.
- **bigserial.** Compact, but enumerable through the API and not portable
  between servers, which a backup restored elsewhere needs.
- **A title UUID in URLs.** Opaque and server-specific: a link or an import
  from another tracker could not name a title without a lookup first. The
  ref is meaningful everywhere.

## Consequences

- Route parameters for titles are refs and are validated by
  `MediaId::parse`; a malformed one is a 400 problem, never a provider call.
- A title restored onto a server that never saw it is created from its refs
  alone and filled in by the `metadata_refresh` job.
