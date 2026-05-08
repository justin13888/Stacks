# Manual QA runbook

Step-by-step instructions for bringing up the full Stacks stack against a
real Zitadel OIDC tenant and exercising every primary user-facing flow.

## 1. Prerequisites

- Postgres 16+ (handled by `compose.dependencies.yaml` if you don't have one)
- A TMDB v4 API token (https://www.themoviedb.org/settings/api)
- A Zitadel (or compatible) OIDC tenant with a registered web application:
  - Application type: Web
  - Auth method: PKCE + Client Secret (or Code + Secret)
  - Redirect URI: `http://localhost:8095/api/v1/auth/callback`
  - Scopes: `openid email profile`

## 2. Configuration

```bash
cp .env.example .env
$EDITOR .env
```

At minimum set:

- `WW_DATABASE_URL=postgresql://stacks:stacks@localhost:5432/stacks`
- `WW_SECRET_KEY=$(openssl rand -hex 32)`
- `WW_TMDB_API_KEY=<your-tmdb-token>`
- `WW_OIDC_ISSUER=https://<your-zitadel-host>`
- `WW_OIDC_CLIENT_ID=<from Zitadel app>`
- `WW_OIDC_CLIENT_SECRET=<from Zitadel app>`
- `WW_OIDC_REDIRECT_URI=http://localhost:8095/api/v1/auth/callback`

## 3. Bring up the stack

Two options.

### Option A — local dev (recommended for iterative QA)

```bash
docker compose -f compose.dependencies.yaml up -d   # Postgres
just migrate                                         # apply schema
just dev-api &                                       # API on :8095
cd web && bun install && bun run codegen && bun run dev   # web on :3000
```

`bun run codegen` regenerates the React Query client from the API's
`/api-docs/openapi.json` — re-run it any time the API surface changes.

### Option B — full docker compose

```bash
docker compose up --build
```

This brings up Postgres + API + web. The web container serves the
production build via nginx on port 3000.

## 4. Smoke checklist

Open `http://localhost:3000` and walk through:

### Auth
- [ ] Click **Login**, get redirected to Zitadel, sign in. Lands back at `/`.
- [ ] Refresh the tab — session survives (refresh-token rotation).
- [ ] Click **Logout** — redirected to `/login`, follow-up requests 401.

### API tokens (CLI / scripting)
- [ ] `POST /api/v1/auth/tokens` with `{ "name": "qa", "scopes": ["read"] }`
      via Scalar UI (`http://localhost:8095/scalar`) returns a token.
- [ ] Set `STACKS_TOKEN` and run `cargo run --manifest-path cli/Cargo.toml -- whoami`.
- [ ] `DELETE /api/v1/auth/tokens/<id>` — subsequent CLI calls 401.

### Watch tracking
- [ ] Search for a movie (e.g. "Dune"). Click into the detail page.
- [ ] Set status to **Watched**, score 9, mark as favorite.
- [ ] Returning to the library page, the entry shows up under "Watched".
- [ ] Add episode progress on a TV show via `POST /media/<id>/episodes/<ep>/status`.

### Lists
- [ ] Create a list "Backlog".
- [ ] Add three items via the detail-page **Add to list** action.
- [ ] Drag-reorder them.
- [ ] Rename the list, then delete it.

### Tags
- [ ] Create tags "rewatch" and "owned".
- [ ] Apply both to a movie.
- [ ] Filter library by tag.

### Blocklist
- [ ] Block an item from the detail page; confirm it disappears from search/discover.
- [ ] Unblock; it returns.

### Notifications
- [ ] Add a webhook channel pointing at `https://webhook.site/<id>`.
- [ ] Update preferences to enable `import_completed` on that channel.
- [ ] Trigger an import (next section) — webhook.site shows the JSON delivery.

### Stats
- [ ] Visit `/stats`. Total watch time, yearly counts, monthly heatmap, and
      genre breakdown all render with non-zero data after the steps above.

### Import
- [ ] Use the Settings → Import page with a tiny TMDB JSON sample (or
      `curl -F` to `/api/v1/users/me/import` with a base64 payload).
- [ ] Poll `/api/v1/users/me/import/<job_id>` — eventually `status: completed`.
- [ ] Imported titles appear in the library.

### Export
- [ ] Settings → Export → JSON downloads a `stacks-export.json`.
- [ ] Same for CSV.
- [ ] Round-trip: import the JSON back into a clean account; counts match.

### Admin
Promote your user to admin (in Postgres):
```sql
UPDATE users SET role = 'admin' WHERE email = '<you>';
```
- [ ] Register a streaming server (e.g. point at a stub Beam endpoint).
      Confirm the API key is encrypted in the `streaming_servers` table.
- [ ] Trigger a sync (`POST /api/v1/admin/servers/<id>/sync`) — a `beam_sync`
      job appears in `/api/v1/admin/jobs` and transitions through running →
      completed (or fails if the upstream server is unreachable, which is
      fine for the QA sanity check).
- [ ] Retry a failed job.

### Webhook
- [ ] `curl -X POST -H "X-Beam-Signature: <hex hmac>" \
       --data-binary @sample.json
       http://localhost:8095/api/v1/webhooks/beam/<server_id>` —
      `watch_history` gains a row matching the playback event.

### CLI sanity
- [ ] `cargo run --manifest-path cli/Cargo.toml -- health` → `ok`
- [ ] `cargo run --manifest-path cli/Cargo.toml -- whoami` (with `STACKS_TOKEN`)
- [ ] `cargo run --manifest-path cli/Cargo.toml -- lists`
- [ ] `cargo run --manifest-path cli/Cargo.toml -- status --cameo-id tmdb:438631`

## 5. Troubleshooting

- `OIDC callback fails`: confirm the Zitadel app's redirect URI matches
  `WW_OIDC_REDIRECT_URI` exactly (including scheme and trailing slash).
- `panic: TMDB 401`: the TMDB API key is wrong or stale. The v4 read access
  token (the long Bearer-style string) is required, not the v3 API key.
- `JobRunner panics`: jobs that hit the cameo SDK during a Postgres-down
  window will retry up to `max_attempts` (3). Restart the API after bringing
  Postgres back.
- `web/openapi.json missing`: run `just codegen` once with the API live.
