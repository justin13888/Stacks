# Deploying stacks in production

This guide covers a single-host Docker Compose deployment behind a TLS
reverse proxy. Everything below assumes a Linux host with Docker / Podman.

## Recommended topology

```
        ┌─────────────────────┐
   TLS  │  Reverse proxy      │  Caddy / Nginx / Traefik
  ────► │  (terminates HTTPS) │  - sets X-Forwarded-For to the client
        └─────────┬───────────┘
                  │  everything
                  ▼
        ┌─────────────────────┐
        │  stacks-web :3000   │  Nginx: the Vite build, and
        │                     │  /api/* proxied to stacks-api
        └─────────┬───────────┘
                  │  /api/*  (compose network)
                  ▼
        ┌─────────────────────┐        ┌────────────────┐
        │  stacks-api :8095   │ ─────► │  PostgreSQL 18 │
        │  Rust / kynos       │        │ (compose-managed)
        │  - migrations on    │        └────────────────┘
        │    startup          │
        │  - job runner       │
        │  /metrics on :9095  │
        └─────────────────────┘
```

The web container's Nginx serves the SPA and proxies `/api/` to the API
over the Compose network, so the browser talks to one origin and CORS does
not apply. [`compose.stacks.yaml`](../compose.stacks.yaml) publishes the
API's `:8095` on the host's loopback interface only: the API believes the
client address its trusted proxies report in `X-Forwarded-For`, so a client
that could reach the port directly could name any address it liked and
evade the per-client rate limit. Send remote traffic through `stacks-web`.

## 1. Set production environment variables

The exhaustive list lives in
[`core/src/config/env.rs`](../core/src/config/env.rs) and the table in the
[README](../README.md#configuration). The production-relevant set:

```bash
# Database — the compose file ships a Postgres 18 with these creds
export WW_DATABASE_URL=postgres://stacks:stacks@postgres:5432/stacks

# Metadata
export WW_TMDB_API_KEY=...

# OIDC (Authelia / Authentik / Keycloak / Dex / etc.)
export WW_OIDC_ISSUER=https://auth.example.com
export WW_OIDC_CLIENT_ID=stacks
export WW_OIDC_CLIENT_SECRET=...
export WW_OIDC_REDIRECT_URI=https://stacks.example.com/auth/callback

# Accounts provisioned as administrators (verified addresses only)
export WW_ADMIN_EMAILS=you@example.com

# Your reverse proxy plus the web container's Nginx
export WW_TRUSTED_PROXY_HOPS=2

# Optional
export WW_LOG_LEVEL=info
export WW_METRICS_TOKEN=...          # require a bearer token on /metrics
# Per-attempt TMDB / AniList timeouts (seconds); defaults shown
export WW_METADATA_CONNECT_TIMEOUT_SECS=3
export WW_METADATA_REQUEST_TIMEOUT_SECS=10
```

There is no application secret to generate. Sessions are opaque random
tokens stored hashed in PostgreSQL, so nothing is signed with a server key.

The session cookie is marked `Secure` when `WW_OIDC_REDIRECT_URI` is an
`https://` URL, which it must be behind TLS.

`WW_CORS_ALLOWED_ORIGINS` stays empty for this topology. Set it only if you
host the SPA on a different origin from the API; `*` is refused, because
sessions are cookies.

## 2. Bring the stack up

```bash
docker compose up -d
```

[`compose.yaml`](../compose.yaml) is a one-liner that `include:`s
[`compose.dependencies.yaml`](../compose.dependencies.yaml) (PostgreSQL 18)
and [`compose.stacks.yaml`](../compose.stacks.yaml) (the API on loopback
`:8095` and the web build on `:3000`). The API container mounts a
`stacks_data` volume at `/data`; the database mounts `postgres_data` at
`/var/lib/postgresql`.

## 3. First-time setup

### Migrations

Migrations run automatically on every API start via
`migration::Migrator::up(&db, None)` ([`api/src/main.rs`](../api/src/main.rs)).
There is no separate migration command to invoke. The schema is one
baseline migration in [`core/migration/src/`](../core/migration/src/);
later releases add migrations after it.

### First admin user

stacks is OIDC-only and provisions accounts on their first sign-in with
role `user`. An address listed in `WW_ADMIN_EMAILS` is provisioned as
`admin` instead, and an existing account with that address is promoted the
next time it signs in, provided the identity provider reports the address
as verified (`email_verified: true` in the id_token). Removing an address
never demotes anyone.

Without a provider that verifies addresses, promote an account by hand:

```bash
docker compose exec postgres psql -U stacks -d stacks \
  -c "UPDATE users SET role = 'admin' WHERE email = 'you@example.com';"
```

The role is read on every request, so the change takes effect at once and
unlocks `/api/v1/admin/*`.

## 4. Reverse proxy (TLS)

A minimal Caddy example:

```caddy
stacks.example.com {
    encode zstd gzip
    # The SPA and the API, both through the web container's Nginx.
    reverse_proxy localhost:3000
}
```

Caddy sets `X-Forwarded-For` to the client address and the web container's
Nginx appends Caddy's, so the API trusts two hops
(`WW_TRUSTED_PROXY_HOPS=2`). Count every proxy that appends to the header:
too few and every client shares the proxy's rate-limit bucket; too many and
clients can spoof their address.

### Built-in rate limit

Every `/api/v1` operation is rate-limited in process to
`WW_RATE_LIMIT_PER_MINUTE` requests a minute per client address (600 by
default), keyed on the address the trusted proxies report. The counters are
in memory, which suits the single-process deployment. Layer stricter limits
at the reverse proxy if you need them (Caddy's `rate_limit`, Nginx's
`limit_req`).

## 5. Backups

Two pieces of persistent state to capture:

### PostgreSQL

```bash
docker compose exec postgres pg_dump -U stacks -F c stacks > stacks-$(date +%F).dump

# Restore (into an empty DB):
docker compose exec -T postgres pg_restore -U stacks -d stacks --clean < stacks-2026-05-22.dump
```

This is the only piece holding user data: library entries, plays and their
history, lists, tags, notification channels, sessions and jobs. Schedule it
(cron / systemd timer / your backup tool of choice).

### `WW_DATA_DIR` volume (`stacks_data`)

The API mounts `stacks_data` at `/data`. Currently used for the cameo cache
directory (`${WW_DATA_DIR}/cameo`, see `WW_CAMEO_CACHE_DIR`). Cameo's
sqlite cache is off in the workspace today, so the directory is reserved
for future use and a fresh start is safe; back it up if you want, but loss
is non-destructive.

## 6. Observability

### Logs

The API emits JSON to stdout via `tracing-subscriber`. Filter at the source
with `WW_LOG_LEVEL` (default `info`); the value is whatever
`tracing_subscriber::EnvFilter` accepts — e.g.
`info,stacks_core::jobs=debug`. Ship the container logs to your aggregator
of choice (Loki, Vector → Elasticsearch, OpenTelemetry Collector, …).

Each request is traced by kynos's `Trace` observer, and carries a request
id from the `RequestId` interceptor
([`api/src/app.rs`](../api/src/app.rs)).

### Metrics

`GET /metrics` returns the Prometheus text format on the operator listener,
`WW_METRICS_BIND` (default `0.0.0.0:9095`), which the compose file does not
publish. It is not part of the public API document. Set
`WW_METRICS_TOKEN` to require `Authorization: Bearer <token>`, and expose
the port to your scrape network only. Series include:

- `stacks_http_requests_total{operation, status}`: counter.
- `stacks_http_request_duration_seconds{operation, status}`: histogram.
- `stacks_metadata_request_duration_seconds` and
  `stacks_metadata_errors_total`: TMDB / AniList calls.

`operation` is the OpenAPI `operationId` (or `unmatched`), so label
cardinality is bounded by the API's operations, not by the ids in URLs.

### Health

On the API port:

- `GET /healthz`: process liveness; touches no dependency.
- `GET /readyz`: readiness; 503 while the database is unreachable or
  migrations are pending.

Wire both to your orchestrator probes.

## 7. OIDC provider tips

The API expects a standard OIDC provider that exposes discovery at
`<issuer>/.well-known/openid-configuration` and supports the
authorization-code flow with PKCE (S256) and the `openid email profile`
scopes. [`core/src/auth/oidc.rs`](../core/src/auth/oidc.rs) is the source of
truth.

Per-provider notes:

- **Authelia / Authentik / Keycloak / Dex.** Register a confidential client
  with redirect URI `https://<your-host>/auth/callback` and the
  `authorization_code` grant. stacks does not use the provider's refresh
  tokens; its own sessions last `WW_SESSION_TTL_SECS`.
- **Display name.** stacks takes `preferred_username`, then `name`, then the
  local part of the email, and only then `user_<first-8-of-sub>`.
- **Verified email.** Admin promotion through `WW_ADMIN_EMAILS` needs the
  provider to send `email_verified: true`.
- **Account linkage.** Accounts are identified by the issuer and `sub`
  together. If you change OIDC providers, existing accounts won't match;
  either update `oidc_issuer` and `oidc_subject` directly or let new
  accounts be provisioned.

## Upgrades

`docker compose pull && docker compose up -d` is the upgrade path.
Migrations apply on the new container's startup before it begins serving;
failed migrations crash startup with the migration error in the logs. Take
a `pg_dump` first.

Releases are cut by release-please; breaking changes are listed in each
release's notes and in [`CHANGELOG.md`](../CHANGELOG.md).
