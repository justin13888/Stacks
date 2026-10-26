# Deploying Stacks

This guide runs Stacks v0.1.0 on one host with Docker Compose (or Podman
with Docker Compose as its provider; see [Compose providers](#compose-providers))
behind a TLS reverse proxy, from the compose bundle attached to each GitHub
Release.

## Compose providers

The stack is `compose.yaml`, which `include:`s the other files, with
`compose.build.yaml` or the e2e overlay layered on top with `-f`. Use a
provider that applies those overlays after the included files:

- **Docker** with Compose v2 (`docker compose`).
- **Podman** through `podman compose` with Docker Compose v2 as its
  provider: install `docker-compose` v2, start the Podman API socket
  (`systemctl --user enable --now podman.socket`), and set
  `PODMAN_COMPOSE_PROVIDER=docker-compose` when `podman-compose` is also
  installed. `podman compose version` names the provider in use.

podman-compose (1.6 and earlier) is not supported: it merges the
`include:`d files after the `-f` overlays, so `just up` pulls
`ghcr.io/…:latest` instead of building the checkout and the e2e stack loses
its environment.

On SELinux hosts (Fedora, RHEL) every bind mount carries the `z` option,
which relabels the mounted files under `deploy/` (and `e2e/fixtures/`) so
the containers can read them; it is ignored where SELinux is off.

## Topology

```
          ┌──────────────────────┐
   HTTPS  │ Reverse proxy        │  Caddy / nginx / Traefik: terminates TLS,
  ──────► │                      │  sets X-Forwarded-For
          └──────────┬───────────┘
                     │ everything
                     ▼
          ┌──────────────────────┐
          │ stacks-web :3000     │  nginx: the app, and /api/* proxied
          └──────────┬───────────┘  to stacks-api on the compose network
                     │ /api/*
                     ▼
          ┌──────────────────────┐        ┌──────────────────┐
          │ stacks-api :8095     │ ─────► │ PostgreSQL 18    │
          │ migrations on start, │        │ postgres_data    │
          │ background jobs,     │        └────────┬─────────┘
          │ /metrics on :9095    │                 │ pg_dump
          └──────────┬───────────┘        ┌────────┴─────────┐
                     │ HTTPS              │ backup (profile) │
                     ▼                    │ stacks_backups   │
          TMDB · AniList · your OIDC      └──────────────────┘
          provider
```

- The browser talks to one origin: nginx serves the app and proxies `/api`,
  so there is no CORS and no API URL to configure.
- `stacks-api` publishes `:8095` (API and probes) and `:9095` (metrics) on
  the host's loopback only, and PostgreSQL `:5432` likewise. The API trusts
  the client address its proxies report, so a client that reached `:8095`
  directly could choose its own rate-limit bucket: send remote traffic
  through `stacks-web`.
- Everything the user owns is in PostgreSQL. The API container is
  stateless and mounts no volume.

## 1. Get the bundle and verify it

```bash
gh release download v0.1.0 -R justin13888/Stacks
sha256sum -c SHA256SUMS
gh attestation verify stacks-compose-v0.1.0.tar.gz -R justin13888/Stacks
tar -xzf stacks-compose-v0.1.0.tar.gz && cd stacks-compose-v0.1.0
```

The bundle holds `compose.yaml` and the files it includes
(`compose.dependencies.yaml`, `compose.stacks.yaml`, `compose.backup.yaml`,
`compose.dex.yaml`), `deploy/` (the backup scripts and the development Dex
config), `.env.example`, the README and this guide.

The images are `ghcr.io/justin13888/stacks-api` and `stacks-web`, for
`linux/amd64` and `linux/arm64`, tagged `vX.Y.Z`, `X.Y` and `latest`. Each
carries an SBOM and a build-provenance attestation signed by the release
workflow ([ADR 0001](adr/0001-release-signing.md)). Verify the ones you run:

```bash
gh attestation verify oci://ghcr.io/justin13888/stacks-api:v0.1.0 -R justin13888/Stacks
gh attestation verify oci://ghcr.io/justin13888/stacks-web:v0.1.0 -R justin13888/Stacks
```

The release tag points at a commit GitHub signed; the tag itself is not
signed, and the attestations bind each artifact to that commit.

## 2. Register Stacks with your identity provider

Stacks has no local accounts: everyone signs in through OpenID Connect. The
API runs the authorization-code flow with PKCE (S256) and a nonce, verifies
the id_token's signature, issuer and audience against the provider's
discovery document (`<issuer>/.well-known/openid-configuration`), then sets
its own opaque session cookie ([ADR 0004](adr/0004-auth-sessions.md)).

Register a **confidential web client**:

| Setting | Value |
| --- | --- |
| Redirect URI | `https://stacks.example.com/auth/callback` (the web app's route) |
| Grant | authorization code (PKCE S256) |
| Scopes | `openid email profile` |
| Client authentication | client secret (basic or post) |

Then set `WW_OIDC_ISSUER`, `WW_OIDC_CLIENT_ID`, `WW_OIDC_CLIENT_SECRET` and
`WW_OIDC_REDIRECT_URI`. Accounts are identified by issuer and `sub`; the
display name comes from `preferred_username`, then `name`, then the email's
local part. Stacks never uses the provider's refresh tokens: its sessions last
`WW_SESSION_TTL_SECS`.

- **Authelia.** Add a client under `identity_providers.oidc.clients` with
  `redirect_uris: [https://stacks.example.com/auth/callback]`,
  `scopes: [openid, email, profile]`, `grant_types: [authorization_code]`,
  `require_pkce: true`, `pkce_challenge_method: S256` and
  `token_endpoint_auth_method: client_secret_basic`. The issuer is
  Authelia's root URL.
- **Authentik.** Create an OAuth2/OpenID provider (confidential client,
  signing key set, redirect URI as above) and an application using it. The
  issuer is `https://authentik.example.com/application/o/<application-slug>/`
  (keep the trailing slash: it must match the `iss` claim exactly).
- **Keycloak.** In your realm, create an OpenID Connect client with *Client
  authentication* on, *Standard flow* on, the redirect URI above and PKCE
  method S256. The issuer is `https://keycloak.example.com/realms/<realm>`.
  Map an `email_verified` claim if you use `WW_ADMIN_EMAILS`.

**Administrators.** Addresses in `WW_ADMIN_EMAILS` are provisioned, or
promoted at their next sign-in, as admins, when the provider marks the
address verified (`email_verified: true`). Removing an address never demotes
anyone; an admin does that from the Admin page. Without a provider that
verifies addresses:

```bash
docker compose exec postgres psql -U stacks -d stacks \
  -c "UPDATE users SET role = 'admin' WHERE email = 'you@example.com';"
```

**Split horizon.** If the API reaches the provider at a different address
than the browser (a provider on the same compose network, or a proxy),
set `WW_OIDC_INTERNAL_ISSUER` to the API's address: discovery and the token
exchange use it, and the authorization URL handed to the browser is rewritten
back onto `WW_OIDC_ISSUER`.

## 3. Configure and start

```bash
cp .env.example .env
$EDITOR .env
docker compose up -d                     # or: docker compose --profile backup up -d
docker compose ps                        # every service healthy
```

At minimum set `STACKS_VERSION`, `POSTGRES_PASSWORD` (before the first
start: the database keeps the password it was created with; letters, digits
and `-_.~` only, such as `openssl rand -hex 32`, because compose writes it
into the API's database URL unencoded, where `@ / : # %` break it),
`WW_TMDB_API_KEY`, the four `WW_OIDC_*` values, `WW_ADMIN_EMAILS` and
`WW_TRUSTED_PROXY_HOPS=2` (your proxy plus nginx). The full list is under
[Configuration](#configuration).

The API applies pending migrations, then serves. `GET /readyz` on
`127.0.0.1:8095` answers 200 once the database answers and the schema is
current; compose's healthcheck uses it, and `stacks-web` starts only once the
API is healthy.

### Cookies, TLS and the reverse proxy

- The session cookie (`stacks_session`) is `HttpOnly`, `SameSite=Lax`, and
  `Secure` whenever `WW_OIDC_REDIRECT_URI` is `https://`. Serve Stacks over
  HTTPS only; over plain HTTP on anything but `localhost`, browsers drop a
  `Secure` cookie and sign-in loops.
- Unsafe cross-site requests are refused with Fetch Metadata; leave
  `WW_CORS_ALLOWED_ORIGINS` empty unless you host the app on another origin.
- Count every proxy that appends to `X-Forwarded-For` in
  `WW_TRUSTED_PROXY_HOPS`. Too few and every client shares one rate-limit
  bucket; too many and clients can spoof their address.
- Pass `Host` and `X-Forwarded-Proto` through, and allow request bodies of
  at least `WW_IMPORT_MAX_BYTES` plus overhead for imports (nginx in
  `stacks-web` allows 32 MiB).

A minimal Caddyfile:

```caddy
stacks.example.com {
    encode zstd gzip
    reverse_proxy 127.0.0.1:3000
}
```

nginx:

```nginx
server {
    listen 443 ssl http2;
    server_name stacks.example.com;
    # ssl_certificate ...;
    client_max_body_size 32m;
    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

The built-in rate limit (`WW_RATE_LIMIT_PER_MINUTE` per client on `/api/v1`)
keeps its counters in memory, which suits one API process.

## 4. Observability

- **Logs.** JSON on stdout. Filter with `WW_LOG_LEVEL` (any
  `tracing_subscriber::EnvFilter` value, e.g. `info,stacks_core::jobs=debug`).
  Every request carries a request id.
- **Metrics.** `GET /metrics` (Prometheus text) on the operator listener,
  `WW_METRICS_BIND`, published on `127.0.0.1:9095`. Set `WW_METRICS_TOKEN`
  and scrape with `Authorization: Bearer <token>`:

  ```yaml
  scrape_configs:
    - job_name: stacks
      authorization: { credentials: "<WW_METRICS_TOKEN>" }
      static_configs: [{ targets: ["127.0.0.1:9095"] }]
  ```

  Series: `stacks_http_requests_total` and
  `stacks_http_request_duration_seconds` by `operation` (the OpenAPI
  operationId) and `status`; `stacks_metadata_errors_total` by `provider`,
  `operation` and `class` (`unavailable`, `rate_limited`, …);
  `stacks_metadata_request_duration_seconds`; and
  `stacks_metadata_circuit_state` per provider (0 closed, 1 half-open,
  2 open). See [ADR 0005](adr/0005-observability.md).
- **Probes.** `GET /healthz` (the process is up; touches nothing) and
  `GET /readyz` (503 while the database is unreachable or migrations are
  pending; also reports each metadata provider's circuit, which never makes
  the service unready).

When TMDB or AniList is down the library keeps working from its snapshots,
title pages say their details may be out of date, and the failures show up
in `stacks_metadata_errors_total`, never as 500s.

## 5. Backups and restore

There are three layers; use the first two at least.

### Scheduled pg_dump (the `backup` profile)

```bash
docker compose --profile backup up -d
```

A `postgres:18` sidecar ([`deploy/backup/backup.sh`](../deploy/backup/backup.sh))
dumps the database every `STACKS_BACKUP_INTERVAL_SECS` (default daily) in
pg_dump's custom format into the `stacks_backups` volume, checks each dump
reads back, and keeps the newest `STACKS_BACKUP_KEEP` (default 14). Copy them
off the host with your backup tool, or:

```bash
docker compose cp backup:/backups ./stacks-backups
```

Take one now, and prove the newest dump restores row for row into a scratch
database ([`deploy/backup/verify.sh`](../deploy/backup/verify.sh)). It
compares row counts with the live database, so run it while nobody is using
Stacks (a sign-in in between shows up as a `sessions` mismatch):

```bash
docker compose --profile backup run --rm --no-deps backup once
docker compose --profile backup run --rm --no-deps --entrypoint bash backup /opt/stacks/verify.sh
```

Without the sidecar, the same dump by hand:

```bash
docker compose exec -T postgres pg_dump -U stacks -Fc stacks > stacks-$(date +%F).dump
```

### Restore a dump

```bash
docker compose stop stacks-api stacks-web
docker compose --profile backup run --rm --no-deps --entrypoint bash backup \
  /opt/stacks/restore.sh /backups/stacks-20261101T030000Z.dump
docker compose start stacks-api stacks-web
```

`restore.sh` replaces every object in one transaction (`pg_restore --clean
--if-exists --single-transaction`), so a failed restore leaves the database
as it was. For a dump taken outside the volume, `docker compose cp` it into
the `backup` container's `/backups` first, or pipe it to
`docker compose exec -T postgres pg_restore -U stacks -d stacks --clean --if-exists`.

### The Stacks backup (per account)

Settings → Backup & export downloads one JSON document with everything an
account owns (entries, every run and play, events, lists in order, tags,
blocklist, settings), and Settings → Import restores it, after a dry-run
preview, into the same or another account or server. Scripts use
`GET /api/v1/exports/backup` and `POST /api/v1/imports/backup`. The format
and the restore modes are in [backup-format.md](backup-format.md). It is the
copy to keep when moving servers, and it survives a schema change because it
is versioned.

## 6. Upgrades

```bash
$EDITOR .env                       # STACKS_VERSION=v0.1.1
docker compose --profile backup run --rm --no-deps backup once   # a fresh dump first
docker compose pull && docker compose up -d
```

- Migrations run on the new API's start, before it serves; `/readyz` stays
  503 until they finish. A failed migration stops the container with the
  error in its logs; restore the dump and pin the previous version.
- **After v0.1.0 migrations are append-only**: a released migration is never
  edited, and every upgrade applies only the ones it adds
  ([ADR 0008](adr/0008-database-and-orm.md)). Downgrading across a
  migration means restoring a dump taken before it.
- Versions follow semantic versioning; read the release notes
  ([CHANGELOG.md](../CHANGELOG.md)) before a minor upgrade while Stacks is
  0.x.

## Configuration

### The API

The API reads environment variables, layered over an optional TOML file
(`WW_CONFIG_FILE`); [`core/src/config/`](../core/src/config) is the source,
and `just check-env-docs` fails when this table misses one. Compose passes
each of them to `stacks-api` from your `.env` (`just check-compose` fails
when one is not), except `WW_DATABASE_URL`, which it builds from
`POSTGRES_PASSWORD`, and `WW_BIND`, which it fixes at `0.0.0.0:8095`
because the published port, the healthcheck and `stacks-web`'s proxy all
target 8095. `WW_CONFIG_FILE` is a path inside the container, so mount the
file there too. An optional variable that is set but empty counts as unset,
so the file's value still applies; the one exception is `WW_METRICS_BIND`,
where an empty value disables the listener. In the container image the
listen addresses always come from the environment: the image sets
`WW_BIND` and `WW_METRICS_BIND` by default (and compose sets both too), so
the file's `server.bind` and `server.metrics_bind` never apply there. Set
them with `WW_BIND` / `WW_METRICS_BIND` rather than the config file
(under compose only `WW_METRICS_BIND`, since compose fixes `WW_BIND`).

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `WW_DATABASE_URL` | yes | set by compose from `POSTGRES_PASSWORD` | PostgreSQL URL. |
| `WW_TMDB_API_KEY` | yes | — | TMDB credential: a v3 API key or a v4 read access token. |
| `WW_OIDC_ISSUER` | yes | — | The provider's issuer, as the browser reaches it. |
| `WW_OIDC_CLIENT_ID` | yes | — | Stacks' client id at the provider. |
| `WW_OIDC_CLIENT_SECRET` | yes | — | That client's secret. |
| `WW_OIDC_REDIRECT_URI` | yes | `http://localhost:3000/auth/callback` in compose | The web app's `/auth/callback`; `https://` makes the cookie `Secure`. |
| `WW_OIDC_INTERNAL_ISSUER` | no | — | The issuer as the API reaches it, when different (split horizon). |
| `WW_ADMIN_EMAILS` | no | — | Comma-separated verified emails made admins at sign-in. |
| `WW_SESSION_TTL_SECS` | no | `2592000` | Session lifetime (30 days); sign-out and revocation are immediate. |
| `WW_BIND` | no | `0.0.0.0:8095` | API listen address. |
| `WW_METRICS_BIND` | no | `0.0.0.0:9095` | Operator listener for `/metrics`; empty disables it. |
| `WW_METRICS_TOKEN` | no | — | Bearer token `/metrics` requires; unset serves it to anyone who reaches the port. |
| `WW_TRUSTED_PROXY_HOPS` | no | `1` | Proxies appending to `X-Forwarded-For`: nginx, plus your TLS proxy. `0` trusts none. |
| `WW_RATE_LIMIT_PER_MINUTE` | no | `600` | Requests per client per minute on `/api/v1`. |
| `WW_CORS_ALLOWED_ORIGINS` | no | — | Origins allowed with credentials, comma-separated; `*` is refused. |
| `WW_IMPORT_MAX_BYTES` | no | `10485760` | Largest import upload; the original file is kept, so this bounds storage. |
| `WW_JOB_RETENTION_DAYS` | no | `30` | Days a completed background job is kept; the job runner prunes older ones at start and daily. `0` keeps them forever. |
| `WW_FAILED_JOB_RETENTION_DAYS` | no | `90` | Days a failed background job is kept, so its error stays readable and retryable from the admin jobs view; `0` keeps them forever. Pending and running jobs are never pruned. |
| `WW_METADATA_CONNECT_TIMEOUT_SECS` | no | `3` | Connect timeout per TMDB / AniList attempt. |
| `WW_METADATA_REQUEST_TIMEOUT_SECS` | no | `10` | Whole-request timeout per attempt. |
| `WW_TMDB_BASE_URL` | no | `https://api.themoviedb.org` | TMDB endpoint override (tests, mirrors). |
| `WW_ANILIST_BASE_URL` | no | `https://graphql.anilist.co` | AniList GraphQL endpoint override. |
| `WW_LOG_LEVEL` | no | `info` | Log filter. |
| `WW_CONFIG_FILE` | no | `config.toml` | Optional TOML file the environment overrides. |

There is no application secret to generate: sessions and API tokens are
random values stored hashed.

### Compose

| Variable | Default | Purpose |
| --- | --- | --- |
| `STACKS_VERSION` | `latest` | Image tag to run: `v0.1.0`, or `0.1` for the newest patch. Pin it. |
| `STACKS_WEB_PORT` | `3000` | Host port for `stacks-web`. |
| `POSTGRES_PASSWORD` | `stacks` | Database password, used by PostgreSQL, the API and the backup sidecar. Set before the first start. Use only `A-Z a-z 0-9 - _ . ~` (e.g. `openssl rand -hex 32`): it goes into `WW_DATABASE_URL` unencoded. |
| `STACKS_BACKUP_INTERVAL_SECS` | `86400` | Seconds between dumps (`backup` profile), at least 60. |
| `STACKS_BACKUP_KEEP` | `14` | Dumps kept; older ones are removed. |

## The development identity provider

`--profile dev-idp` starts Dex ([`deploy/dex/config.yaml`](../deploy/dex/config.yaml))
on `localhost:5556` (loopback only) with two accounts, `dev@stacks.local` and
`friend@stacks.local`, both with password `password`, and a public client
secret. It is for trying Stacks and for the e2e suite only; never expose it.

## Troubleshooting

- **The API exits with `missing required environment variable`.** Set the
  named variable in `.env`.
- **Sign-in answers 400 `sign-in-failed`.** The attempt expired (10 minutes)
  or was replayed, or the id_token failed verification (the API log says
  which). Start again; check the issuer matches the `iss` claim exactly.
- **Sign-in answers 502 `identity-provider-unavailable`.** The API cannot
  reach the provider; set `WW_OIDC_INTERNAL_ISSUER` if it needs a different
  address than the browser.
- **Sign-in loops back to the sign-in page.** The cookie was dropped: serve
  over HTTPS when `WW_OIDC_REDIRECT_URI` is `https://`.
- **403 on the Admin page.** Add your verified email to `WW_ADMIN_EMAILS` and
  sign in again.
- **Everyone is rate-limited together.** Raise `WW_TRUSTED_PROXY_HOPS` to the
  number of proxies in front of the API.
- **Titles show "may be out of date".** A metadata provider is failing; see
  `stacks_metadata_errors_total` and the API log. The library keeps working.
- **`stacks-api` stays unhealthy.** `docker compose logs stacks-api`; a
  migration error or an unreachable database is reported there, and
  `/readyz` returns the reason as a problem document.
