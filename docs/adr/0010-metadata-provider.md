# ADR 0010: Metadata through cameo, behind a resilient `MetadataProvider`

- Status: accepted
- Date: 2026-10-24
- Issue: #12 (records the verdict taken in #7)

## Context

Titles, seasons, episodes, search and discovery come from TMDB and AniList.
The maintainer: "cameo implementation may fail for any reason (e.g. untested
edge cases and internet API changes). any code calling cameo must have
robust error-handling."

The prototype called cameo (a 0.1-era git revision) with no timeouts,
retries or rate limits, turned every upstream error into a 500, sent AniList
ids to TMDB, and kept no local copy, so the library went blank whenever a
provider was down.

## Decision

- **cameo 0.2 from crates.io** (`default-features = false`, `tmdb` +
  `anilist`) is the client. Its `cache` feature stays off (ADR 0008).
- **A Stacks trait, `MetadataProvider`**
  ([`core/src/media/provider.rs`](../../core/src/media/provider.rs)) speaks
  only Stacks types. Its adapter, `CameoProvider`, and `mapping.rs` are the
  only code that may name `cameo::` types; a test in
  `core/tests/metadata_fault_injection.rs` enforces it. Services
  depend on the trait, so tests substitute a fake and the fault-injection
  suite drives the real adapter against wiremock.
- **Every call is bounded and classified.** Per-attempt connect and request
  timeouts (`WW_METADATA_CONNECT_TIMEOUT_SECS`,
  `WW_METADATA_REQUEST_TIMEOUT_SECS`), a 25 s deadline over retries, cameo's
  retry policy, a token-bucket pacer per provider under its documented rate
  limit, and a circuit breaker per provider (five consecutive health
  failures open it; a probe after the cooldown closes it). Failures become a
  `MetadataErrorKind` (`not_found`, `rate_limited` with its Retry-After,
  `unavailable`, `bad_upstream_response`, `misconfigured`, `unsupported`),
  are counted in `stacks_metadata_errors_total{provider, operation, class}`
  and logged, and map to RFC 9457 problems (404, 503 with `Retry-After`,
  502/503 `upstream-*`). Nothing is `?`'d into a 500.
- **The catalog snapshot is the fallback.** Every tracked title has a
  `media` row (plus seasons and episodes). A read that cannot reach the
  provider serves the snapshot marked `stale`, listings name the failed
  provider in `degraded`, and the web app shows a stale banner. The hourly
  `metadata_refresh` job refreshes snapshots with backoff per title.
- **Provider-correct routing.** AniList titles never go to TMDB; operations a
  provider cannot do (AniList recommendations, a movie's seasons) are
  `unsupported` without a request. AniList lists no seasons, so an AniList
  show has one, season 1, of episodes `1..=N`: its total, else the episode
  before the next to air, else an empty season (`core/src/media/anilist.rs`).
- **cameo gaps are worked around in Stacks and tracked** in #14: Stacks
  sends its own AniList GraphQL queries for romaji titles, MAL ids, airing
  anime and MAL-id lookups, and uses cameo's TMDB discover builder for the
  regional-drama feeds.
- **Base URLs are configurable** (`WW_TMDB_BASE_URL`, `WW_ANILIST_BASE_URL`)
  so the composed e2e suite can serve fixtures and break the upstreams on
  purpose.

## Alternatives rejected

- **Call TMDB and AniList directly.** cameo already models both APIs,
  unifies their result types and owns retries; re-implementing that is more
  code to keep current with two upstream APIs. The trait keeps the option
  open if cameo stalls.
- **Fail hard without a snapshot.** Cheaper, but makes the user's own data
  unreadable during someone else's outage, which contradicts "never lose
  data" in spirit.
- **Cache responses with cameo's `cache` feature.** It links a second SQLite
  (`libsqlite3-sys`) that collides with sqlx's; the snapshot is a better
  fallback anyway because it is what the user tracked, not whatever was
  fetched last.
- **A shared external cache (Redis).** Another service to run for a
  single-user or family deployment, with no gain over the snapshot table.

## Consequences

- `core/tests/metadata_fault_injection.rs` covers 500s, 429s, timeouts,
  explicit nulls and malformed JSON on every call path; the e2e drills
  (docs/QA.md) repeat TMDB-down and AniList-429 on the composed stack.
- `/readyz` reports each provider's circuit but never goes unready because
  of one: the library still serves.
