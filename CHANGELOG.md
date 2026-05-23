# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
No versions have been released yet; everything sits under `[Unreleased]`.

## [Unreleased]

### Added

- **cameo integration.** Media search, detail, season + episode listing,
  recommendations, and discover feeds (trending / popular / top-rated for
  both movies and TV) via the cameo unified TMDB + AniList SDK
  (`/api/v1/media/*`, `/api/v1/discover/*`).
- **Canonical media id.** Self-describing
  `<provider>:<kind>:<numeric>` ids (e.g. `tmdb:movie:550`, `anilist:tv:21`)
  persisted in every `cameo_media_id` column; episode ids extend with
  `:<season>:<episode>`. Parsing recovers the cameo route to call.
- **Smart + custom lists.** Manual lists with fractional-index reordering,
  and smart lists driven by stored filter queries (`/api/v1/users/me/lists`
  with `is_smart` + `filter_query`).
- **Watch tracking.** Per-title status, score, repeat count, favourite
  flag; per-episode watched / progress / `watched_at`; append-only history
  log with bulk-update endpoint.
- **Beam streaming connector.** Library sync against Beam's REST API,
  cameo-id matching by external provider ids, deep-link / playback URL
  generation, and HMAC-SHA256 verified inbound webhooks (`X-Beam-Signature`
  or `X-Hub-Signature-256`). Per-server periodic sync driven by the
  `beam_sync` job (`WW_BEAM_SYNC_INTERVAL_SECS`, default 1h).
- **Admin server management.** CRUD over registered streaming servers
  (`/api/v1/admin/servers`), on-demand sync (`POST .../sync`), recent job
  listing, and one-click retry (`/api/v1/admin/jobs`).
- **Encrypted-at-rest credentials.** Streaming-server API keys are
  AES-256-GCM encrypted with a key derived (SHA-256, domain-separated) from
  `WW_SECRET_KEY`.
- **Background job runner.** PostgreSQL-backed queue with exponential
  backoff (30s base, doubling, capped at 1h), per-job attempt budget,
  pluggable handlers (`beam_sync`, `import`, `notification`). Periodic
  scheduler enqueues `beam_sync` for every enabled server.
- **Import.** MyAnimeList XML, MAL JSON, and TMDB JSON sources via a
  background `import` job (`/api/v1/users/me/import`) with `skip` /
  `overwrite` / `merge` conflict strategies. Per-entry resolution probes
  cameo (movies first, then TV) when the source doesn't already hint a
  kind.
- **Export.** JSON and CSV export of the authenticated user's watch state
  (`/api/v1/users/me/export?format=json|csv`).
- **Stats.** Per-user totals — yearly counts of completed titles, monthly
  activity heatmap, genre breakdown, and an estimated total watch-time
  computed from cameo metadata for up to 250 most-recent completed titles
  (`/api/v1/users/me/stats`).
- **Notifications.** Per-user channels (webhook, Apprise) and per-event-type
  subscription preferences (`new_episode`, `available_on_beam`,
  `upcoming_release`, `import_completed`). Delivery failures are logged but
  do not abort dispatch.
- **OIDC authentication.** Standard authorization-code flow with
  PKCE-style state + nonce. Users are auto-provisioned on first login,
  identified by `oidc_sub`. JWT access tokens (15-min default) plus rotating
  refresh tokens (30-day default).
- **API tokens.** Opaque `stk_…` tokens with `read` / `write` / `admin`
  scopes, argon2id-hashed at rest, plaintext returned exactly once
  (`/api/v1/auth/tokens`).
- **Observability.** Prometheus `/metrics` endpoint
  (`http_requests_total`, `http_request_duration_seconds` with `method`,
  `path`, `status` labels), JSON-formatted `tracing-subscriber` logs filtered
  via `WW_LOG_LEVEL`, liveness + readiness probes (`/health`,
  `/health/ready`).
- **Hardening middleware.** Configurable CORS allowlist
  (`WW_CORS_ALLOWED_ORIGINS`), per-IP token-bucket rate limit on
  `/api/v1/auth/*` (30 req/min, honours `X-Forwarded-For`), security headers
  (`X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`,
  `Content-Security-Policy`, `Referrer-Policy`).
- **Pagination.** Opaque base64url cursors translating to/from cameo's
  page-number API (`?cursor=...&limit=...`, default 50, max 200).
- **Dev-only API docs.** Debug builds expose the OpenAPI 3.1 spec at
  `/api-docs/openapi.json` and a Scalar UI at `/scalar`; both are compiled
  out of release builds.
- **TOML configuration.** Optional `config.toml` (path via `WW_CONFIG_FILE`,
  default `./config.toml`) layered under environment variables.
- **Web UI.** Bun + Vite + React 19 + TanStack Router/Query app with
  full-flow coverage: home, search, library, media detail, lists, stats,
  settings, admin. Uses orval-generated typed API client and the `motion`
  animation library.
- **Docker Compose.** Three-file split — `compose.dependencies.yaml`
  (PostgreSQL), `compose.stacks.yaml` (API + web), `compose.yaml`
  (`include:`s both).

### Changed

- **Database is PostgreSQL.** SeaORM with the `sqlx-postgres` driver. The
  earlier SQLite plan in the PRD is obsolete.
- **No CLI.** The CLI binary referenced in early scaffolding was dropped;
  all operator surface lives behind the HTTP API and the admin endpoints.
- **Auth is OIDC-only.** No local password store. The `users` table carries
  an `oidc_sub` column (unique) rather than a `password_hash`.
- **Core crate renamed to `stacks-core`** to avoid shadowing the Rust
  built-in `core` crate. The library target inside `api` is still `api` for
  brevity.

### Fixed

- `dotenvy::dotenv()` is now called correctly at API startup so `api/.env`
  is loaded in dev.
- Clippy lints addressed across `core` after the cameo integration.
