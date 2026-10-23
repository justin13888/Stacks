# Deploying stacks in production

This guide covers a single-host Docker Compose deployment behind a TLS
reverse proxy. Everything below assumes a Linux host with Docker / Podman.

## Recommended topology

```
        ┌─────────────────────┐
   TLS  │  Reverse proxy      │  Caddy / Nginx / Traefik
  ────► │  (terminates HTTPS) │  - rate-limit /api/v1/auth/* (optional, see below)
        └─────────┬───────────┘  - forward X-Forwarded-For
                  │
                  ├──── /api/* ──────────────┐
                  │                          ▼
                  │              ┌─────────────────────┐
                  │              │  stacks-api :8095   │  Rust / Salvo
                  │              │  - migrations on    │
                  │              │    startup          │
                  │              │  - job runner +     │
                  │              │    Beam scheduler   │
                  │              └─────────┬───────────┘
                  │                        │
                  └──── / ───────┐         │
                                 ▼         ▼
                       ┌─────────────────────┐    ┌────────────────┐
                       │  stacks-web :3000   │    │  PostgreSQL 18 │
                       │  (Vite static build)│    │  (compose-managed)
                       └─────────────────────┘    └────────────────┘
```

The web container is a static-asset Nginx serving the Vite build; it talks
to the API across the Compose network using `VITE_API_BASE_URL`.

## 1. Generate the application secret

```bash
export WW_SECRET_KEY=$(just gen-secret)   # or: openssl rand -hex 32
```

`WW_SECRET_KEY` does double duty: it signs JWTs and, run through SHA-256
with a domain-separated prefix, becomes the AES-256-GCM key that encrypts
streaming-server API keys at rest
([`core/src/crypto.rs`](../core/src/crypto.rs)). Rotating it invalidates
every active JWT and renders existing encrypted credentials unrecoverable —
plan a rotation by re-registering streaming servers afterwards.

## 2. Set production environment variables

The exhaustive list lives in
[`core/src/config/env.rs`](../core/src/config/env.rs) and the table in the
[README](../README.md#configuration). The required production-relevant set:

```bash
# Database — the compose file ships a Postgres 18 with these creds
export WW_DATABASE_URL=postgres://stacks:stacks@postgres:5432/stacks

# Secrets
export WW_SECRET_KEY=$(just gen-secret)
export WW_TMDB_API_KEY=...

# OIDC (Authelia / Authentik / Keycloak / Dex / etc.)
export WW_OIDC_ISSUER=https://auth.example.com
export WW_OIDC_CLIENT_ID=stacks
export WW_OIDC_CLIENT_SECRET=...
export WW_OIDC_REDIRECT_URI=https://stacks.example.com/auth/callback

# CORS — must list your actual frontend origin(s)
export WW_CORS_ALLOWED_ORIGINS=https://stacks.example.com

# Optional
export WW_LOG_LEVEL=info
export WW_BEAM_SYNC_INTERVAL_SECS=3600
# Per-attempt TMDB / AniList timeouts (seconds); defaults shown
export WW_METADATA_CONNECT_TIMEOUT_SECS=3
export WW_METADATA_REQUEST_TIMEOUT_SECS=10
```

`WW_CORS_ALLOWED_ORIGINS` is critical: the default localhost dev allowlist
will reject your production frontend. A single `*` value opens CORS to any
origin and logs a warning — only use it during emergency debugging.

## 3. Bring the stack up

```bash
docker compose up -d
```

[`compose.yaml`](../compose.yaml) is a one-liner that `include:`s
[`compose.dependencies.yaml`](../compose.dependencies.yaml) (PostgreSQL 18)
and [`compose.stacks.yaml`](../compose.stacks.yaml) (the API on `:8095` and
the web build on `:3000`). The API container mounts a `stacks_data` volume
at `/data`; the database mounts `postgres_data` at
`/var/lib/postgresql`.

## 4. First-time setup

### Migrations

Migrations run automatically on every API start via
`migration::Migrator::up(&db, None)` ([`api/src/main.rs`](../api/src/main.rs)).
There is no separate migration command to invoke. The migration set
(`m001`..`m016`) lives in [`core/migration/src/`](../core/migration/src/).

### First admin user

stacks has no admin-creation API — it is OIDC-only and provisions users on
their first login with role `user` (default in the `users.role` column,
[`core/migration/src/m001_create_users.rs`](../core/migration/src/m001_create_users.rs)).
Promote the first user to admin manually:

```bash
# 1. Log into the web app once with the user who should become admin. This
#    OIDC-provisions them with role 'user'.
# 2. Find their id (PostgreSQL ULID strings) and flip the role.
docker compose exec postgres psql -U stacks -d stacks \
  -c "UPDATE users SET role = 'admin' WHERE username = 'your-username';"
```

After the role flip, the next access token issued (e.g. by refresh or
re-login) will pass the `AdminGuard` middleware and unlock
`/api/v1/admin/*` and `/api/v1/users` (admin user-management).

## 5. Reverse proxy (TLS)

A minimal Caddy example:

```caddy
stacks.example.com {
    encode zstd gzip

    # Frontend: static SPA on :3000
    handle /api/* {
        reverse_proxy localhost:8095 {
            header_up X-Forwarded-For {remote_host}
        }
    }
    handle /metrics {
        # Lock down the metrics endpoint at the proxy — it's unauthenticated
        # by design (Prometheus scrapers need it open).
        @internal remote_ip 10.0.0.0/8 192.168.0.0/16 172.16.0.0/12
        reverse_proxy @internal localhost:8095
        respond 403
    }
    handle {
        reverse_proxy localhost:3000
    }
}
```

The API trusts `X-Forwarded-For`'s first hop for the rate limiter
([`api/src/middleware/rate_limit.rs`](../api/src/middleware/rate_limit.rs)).
Make sure your proxy sets it.

### Built-in rate limits

The public auth router (`/api/v1/auth/login`, `/api/v1/auth/callback`,
`/api/v1/auth/refresh`) is in-process rate-limited to **30 requests per
minute per source IP**. The limiter is in-memory and single-process — fine
for a single-binary deployment. If you need stricter limits, layer them at
the reverse proxy (Caddy's `rate_limit`, Nginx's `limit_req`, etc.).

## 6. Backups

Two pieces of persistent state to capture:

### PostgreSQL

```bash
docker compose exec postgres pg_dump -U stacks -F c stacks > stacks-$(date +%F).dump

# Restore (into an empty DB):
docker compose exec -T postgres pg_restore -U stacks -d stacks --clean < stacks-2026-05-22.dump
```

This is the only piece holding user data — watch state, lists, history,
encrypted streaming-server credentials, jobs. Schedule it (cron / systemd
timer / your backup tool of choice).

### `WW_DATA_DIR` volume (`stacks_data`)

The API mounts `stacks_data` at `/data`. Currently used for the cameo cache
directory (`${WW_DATA_DIR}/cameo`, see `WW_CAMEO_CACHE_DIR`). Cameo's
sqlite cache is off in the workspace today, so the directory is reserved
for future use and a fresh start is safe; back it up if you want, but loss
is non-destructive.

## 7. Observability

### Logs

The API emits JSON to stdout via `tracing-subscriber`. Filter at the source
with `WW_LOG_LEVEL` (default `info`); the value is whatever
`tracing_subscriber::EnvFilter` accepts — e.g.
`info,stacks_core::jobs=debug`. Ship the container logs to your aggregator
of choice (Loki, Vector → Elasticsearch, OpenTelemetry Collector, …).

Each request emits a log with `method`, `path`, status, and elapsed time
from [`api/src/middleware/logging.rs`](../api/src/middleware/logging.rs).

### Metrics

`GET /metrics` returns the Prometheus text format. Exposed metrics include:

- `http_requests_total{method, path, status}` — counter.
- `http_request_duration_seconds{method, path}` — histogram.

Plus whatever metrics handlers / services emit at runtime. The endpoint is
**unauthenticated** by design; restrict access at the reverse proxy (see
the Caddy example above) when exposing it beyond a trusted scrape network.

Path-label cardinality grows with id-bearing routes
(`/api/v1/media/tmdb:tv:1396`) — fine for self-hosted single-tenant use,
not so fine if you ever multi-tenant it (you'd need route templating). See
the comment in
[`api/src/middleware/metrics.rs`](../api/src/middleware/metrics.rs).

### Health

- `GET /health` — process liveness.
- `GET /health/ready` — readiness; checks the database is reachable.

Wire both to your orchestrator probes.

## 8. OIDC provider tips

The API expects a standard OIDC provider that exposes discovery at
`<issuer>/.well-known/openid-configuration` and supports the
authorization-code flow with `openid email profile` scopes.
[`core/src/auth/oidc.rs`](../core/src/auth/oidc.rs) is the source of truth.

Per-provider notes:

- **Authelia / Authentik / Keycloak / Dex.** Register a new confidential
  client; redirect URI = `https://<your-host>/auth/callback`; grant types
  `authorization_code` + `refresh_token` (refresh is not currently exercised
  upstream — stacks issues its own refresh tokens); scopes `openid email
  profile`.
- **Username.** stacks derives the username from `preferred_username` if
  present, otherwise falls back to `user_<first-8-of-sub>`. Make sure your
  provider includes `preferred_username` in the userinfo response if you
  want stable, human-readable usernames.
- **Account linkage.** Users are identified by `sub` (`oidc_sub` column,
  unique). If you change OIDC providers, existing users won't match —
  either migrate them by updating `oidc_sub` directly or let new accounts
  be auto-provisioned.

## 9. Beam streaming connector setup

stacks can connect to one or more Beam media servers via the admin API:

```bash
# As an admin user
curl -X POST https://stacks.example.com/api/v1/admin/servers \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Living Room Beam",
    "server_type": "beam",
    "base_url": "https://beam.local",
    "api_key": "your-beam-api-key",
    "webhook_secret": "optional-shared-secret-for-webhooks"
  }'
```

Stored fields:

- `api_key` is encrypted with AES-256-GCM
  ([`core/src/streaming/service.rs`](../core/src/streaming/service.rs#L42))
  under a key derived from `WW_SECRET_KEY`. It is decrypted only when
  building a `BeamConnector` for an outbound call.
- `webhook_secret` is stored plaintext (it is itself a verification secret,
  not user-confidential data); use a long random value.

The job runner enqueues a `beam_sync` for every enabled server every
`WW_BEAM_SYNC_INTERVAL_SECS` (default 1h). Trigger an immediate sync with
`POST /api/v1/admin/servers/{id}/sync`.

### Inbound webhooks

Configure Beam to POST events to
`https://stacks.example.com/api/v1/webhooks/beam/{server_id}` with an
HMAC-SHA256 signature of the raw body in `X-Beam-Signature` (or
`X-Hub-Signature-256`), hex-encoded, optionally prefixed `sha256=`. The
secret on Beam's side must match the `webhook_secret` you registered. If
you didn't register one, signature verification is skipped — only do that on
a trusted internal network.

Recognised event names (case- and punctuation-insensitive,
[`core/src/streaming/beam/webhook.rs`](../core/src/streaming/beam/webhook.rs)):

- `playback_start` / `playback_started` / `play`
- `playback_stop` / `playback_stopped` / `stop`
- `playback_pause` / `playback_paused` / `pause`
- `playback_finish` / `playback_finished` / `playback_complete` / `finished`
- `library_changed` / `library_update` / `library_updated` / `scan_complete`

Unknown event names are accepted (logged as `Unknown`) so a provider can
add new events without breaking delivery. Library-changed events trigger a
re-sync; playback events are logged but not yet attributed to a stacks
user (no Beam-user ↔ stacks-user mapping exists today).

## Upgrades

`docker compose pull && docker compose up -d` is the upgrade path.
Migrations apply on the new container's startup before it begins serving;
failed migrations crash startup with the migration error in the logs. Take
a `pg_dump` first.

Releases are cut by release-please; breaking changes are listed in each
release's notes and in [`CHANGELOG.md`](../CHANGELOG.md).
