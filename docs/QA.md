# Manual QA runbook

Bring up the full stack and exercise every primary user-facing flow.

Everything here uses the bundled development identity provider (Dex) and the
mock Beam server, so no external accounts are needed beyond a TMDB token.

## 1. Prerequisites

- Podman or Docker with compose support
- [`just`](https://github.com/casey/just)
- A **TMDB v4 read-access token** (<https://www.themoviedb.org/settings/api>) —
  the only secret you must supply

## 2. Bring up the stack

```bash
just setup            # copies api/.env.example to api/.env
$EDITOR api/.env      # set WW_TMDB_API_KEY
just up-beam          # Postgres + Dex + mock Beam + API + web
```

`just up` omits the mock Beam server; use it if you are not testing streaming.
Migrations run automatically on API startup.

| Service | URL |
|---|---|
| Web app | <http://localhost:3000> |
| API | <http://localhost:8095> |
| Dex | <http://localhost:5556/dex> |
| Mock Beam | <http://localhost:9494> |

Sanity check before starting: `curl http://localhost:8095/health` → `{"status":"ok"}`.

## 3. Smoke checklist

### Auth
- [ ] Open <http://localhost:3000>. You are redirected to `/login`.
- [ ] Click **Login** → Dex. Sign in as `dev@stacks.local` / `password`.
- [ ] You land back on `/` authenticated.
- [ ] **Reload the tab — you stay signed in.** (The refresh token is persisted;
      the access token is silently re-minted before the router mounts.)
- [ ] Click **Logout** → back to `/login`, and further API calls 401.
- [ ] Sign in again and confirm `GET /api/v1/users/me` reports `"role": "admin"`
      (`dev@stacks.local` is in `WW_ADMIN_EMAILS`).

### API tokens
- [ ] `POST /api/v1/auth/tokens` with `{"label":"qa","scopes":["read"]}` returns
      a `stk_…` token, shown once.
- [ ] That token authenticates a request in place of the JWT.
- [ ] `DELETE /api/v1/auth/tokens/{id}` — subsequent calls with it 401.

### Watch tracking & Library
- [ ] Search for "Dune". Results render with posters.
- [ ] Open the detail page. Seasons/episodes render for a TV show.
- [ ] Set status **Watching**, score 88, mark favourite.
- [ ] **Library** shows it. Filter by status, and by favourites — both narrow
      the grid correctly.
- [ ] Set another title to **Plan to Watch**; it appears under that filter.

### Lists
- [ ] Create a list "Backlog", add items from a detail page, drag to reorder.
- [ ] Rename it, then delete it.

### Tags
- [ ] Create tags "rewatch" and "owned", apply both to a title, filter by tag.

### Blocklist
- [ ] Block an item; it disappears from search/discover. Unblock; it returns.

### Stats
- [ ] Visit `/stats`. Counts, heatmap, and genre breakdown render.

### Import / export
- [ ] Settings → Import a small MAL XML or TMDB JSON export.
- [ ] Poll `GET /api/v1/users/me/import/{job_id}` until `completed`.
- [ ] Imported titles appear in Library.
- [ ] Settings → Export downloads JSON, then CSV.

### Notifications
- [ ] Add a webhook channel pointing at `https://webhook.site/<id>`.
- [ ] Enable the `import_completed` event on it.
- [ ] Run another import — webhook.site receives the delivery.

### Streaming (requires `just up-beam`)
- [ ] As an admin, register the mock server:
      ```bash
      curl -X POST http://localhost:8095/api/v1/admin/servers \
        -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
        -d '{"name":"Mock Beam","server_type":"beam","base_url":"http://mock-beam:9494",
             "api_key":"dev-key","webhook_secret":"whsec-dev","enabled":true}'
      ```
- [ ] Confirm the key is encrypted at rest:
      `select encode(api_key_enc,'hex') from streaming_servers;` — ciphertext,
      not `dev-key`.
- [ ] `POST /api/v1/admin/servers/{id}/sync` returns a `job_id`.
- [ ] `GET /api/v1/admin/jobs` shows `beam_sync` transition to `completed`
      (the runner polls every 10s).
- [ ] `GET /api/v1/media/tmdb:movie:438631/availability` reports `available:
      true` with deep links.
- [ ] Retry a job via `POST /api/v1/admin/jobs/{id}/retry`.

### Webhook signature
```bash
BODY='{"event":"library.changed","item_id":"beam-dune-2021"}'
SIG=$(printf '%s' "$BODY" | openssl dgst -sha256 -hmac 'whsec-dev' -hex | sed 's/.*= //')
curl -X POST "http://localhost:8095/api/v1/webhooks/beam/$SERVER_ID" \
  -H 'Content-Type: application/json' -H "X-Beam-Signature: $SIG" --data-binary "$BODY"
```
- [ ] Valid signature → `{"status":"ok"}`.
- [ ] Tampered signature → 401 `INVALID_SIGNATURE`.

## 4. Troubleshooting

- **Login loops back to `/login`** — check the API logs (`just logs stacks-api`).
  A `nonce mismatch` or `unknown or already-used state` means the authorization
  request expired (10 min) or was replayed; start again from the login button.
- **TMDB 401** — `WW_TMDB_API_KEY` must be the v4 read-access token (a long
  Bearer-style string), not the v3 API key. Login and Library still work
  without it; only metadata lookups fail.
- **403 on `/api/v1/admin/*`** — add your email to `WW_ADMIN_EMAILS` and sign in
  again, or have an admin call `PATCH /api/v1/users/{id}/role`.
- **`/scalar` is 404** — the OpenAPI routes are debug-build only and are absent
  from the release image compose builds. Use `just gen-openapi` instead.
- **Start over from scratch** — `just nuke` drops the volumes, including the
  database.
