# QA runbook for a release

How to check a Stacks release end to end before trusting it with real data.
Part 1 is automated and runs in CI on every pull request; part 2 is the same
journey by hand on the released images, plus what automation cannot judge.

## 1. The automated suite

The Playwright suite in [`e2e/`](../e2e) drives the composed stack in a real
Chromium: the compose files a deployment uses, the images built from the
checkout, the bundled Dex (no auth bypass), and a fixture server standing in
for TMDB and AniList so every run sees the same titles and the drills can
break them ([ADR 0012](adr/0012-test-stack.md)).

```bash
just e2e-install      # once: the suite's packages and Chromium
just e2e-up           # build and start the stack (project stacks-e2e)
just e2e              # run it; `just e2e --project journey` for one part
just backup-verify    # take a sidecar dump and restore it into a scratch DB
just e2e-down         # stop it and delete its volumes
```

It needs a container runtime that can run the stack: Docker with Compose v2,
or Podman through `podman compose` with Docker Compose v2 as its provider
(podman-compose 1.6 applies the `-f` overlays before the `include:`d files,
so it tags the images `ghcr…:latest` and blanks the e2e environment; see
[Compose providers](DEPLOY.md#compose-providers) for the set-up). It
publishes `localhost:3000` (web), and on loopback `5556` (Dex), `8095`
(API), `9095` (metrics) and `8090` (fixtures), so stop a `just up` stack
first. `just e2e-down` also deletes the backup sidecar's volume that
`just backup-verify` fills.

What it covers, in order:

1. **Journey** (`tests/journey.spec.ts`): sign in through Dex as
   `dev@stacks.local`; search "lighthouse" in the ⌘K palette and open *The
   Lighthouse Keepers*; add it as Watching; "+1 episode" (1 of 4), "Mark up to
   S1E3" (3 of 4), "+1" again and Completed; favourite and a note; start a
   rewatch and log an episode of it; create a tag and tag the show; mark
   *Paper Moons* watched; create the list "Weekend picks", add both, move the
   film up and check the order survives a reload; download the Stacks
   backup; sign out, sign in as `friend@stacks.local`, preview and restore
   the backup; then compare the two accounts' backups (entries, runs, plays,
   tags, list order, blocklist, settings; ids and export metadata aside).
   They must be equal.
2. **Screens** (`tests/screens.spec.ts`): every main screen in the light and
   the dark theme, with no error state: sign-in, Up Next, Library, both title
   pages, Discover, Calendar, Lists, a list, History, Stats, Settings,
   Backup & export, Import, Trash, Blocklist, Admin, and the search palette.
   Written to `e2e/artifacts/screenshots/{light,dark}/`; CI uploads them as
   the `e2e-screenshots` artifact.
3. **Outage drills** (`tests/drills.spec.ts`):
   - *TMDB down* (every TMDB call answers 500): the library lists every title,
     the title page renders from its snapshot with the stale banner and the
     episode progress, Stats renders, Discover shows per-rail errors under a
     working page, no API response is a 500, and
     `stacks_metadata_errors_total{provider="tmdb"}` rises while
     `stacks_http_requests_total{status="500"}` does not.
   - *AniList rate-limited* (429 with `Retry-After`): an anime with no
     snapshot shows "A metadata provider is busy … Try again in N s.", the
     `rate_limited` error count rises, and "Try again" recovers once AniList
     answers.
   - *Database restart* (PostgreSQL stopped mid-session): `/readyz` answers
     503, then 200 after the start; the same session still works, every entry
     is unchanged, and a new play is recorded.

CI's `E2E (composed stack)` job additionally fails unless every service
reports healthy (`docker compose up --wait`) and runs `just backup-verify`.
Its `e2e-report` artifact holds the HTML report and a trace for any failure.

## 2. By hand, on the released images

### Set up

Use the release bundle as a self-hoster would (README, "Quick start"), with
`STACKS_VERSION` pinned to the release and the throwaway Dex:

```bash
gh release download vX.Y.Z -R justin13888/Stacks
sha256sum -c SHA256SUMS
gh attestation verify stacks-compose-vX.Y.Z.tar.gz -R justin13888/Stacks
gh attestation verify oci://ghcr.io/justin13888/stacks-api:vX.Y.Z -R justin13888/Stacks
gh attestation verify oci://ghcr.io/justin13888/stacks-web:vX.Y.Z -R justin13888/Stacks
tar -xzf stacks-compose-vX.Y.Z.tar.gz && cd stacks-compose-vX.Y.Z
cp .env.example .env
```

In `.env`: `STACKS_VERSION=vX.Y.Z`, your real `WW_TMDB_API_KEY`,
`WW_OIDC_ISSUER=http://localhost:5556/dex`,
`WW_OIDC_INTERNAL_ISSUER=http://dex:5556/dex`, `WW_OIDC_CLIENT_ID=stacks`,
`WW_OIDC_CLIENT_SECRET=stacks-dev-client-secret`,
`WW_ADMIN_EMAILS=dev@stacks.local`. Then:

```bash
docker compose --profile dev-idp --profile backup up -d
docker compose ps                         # all healthy
curl -s http://127.0.0.1:8095/readyz      # {"status":"ready",...}
```

- [ ] A fresh database migrates on first start (`docker compose logs stacks-api`
      shows the baseline migration, then "serving the API").

### Accounts and sessions

- [ ] <http://localhost:3000> redirects to the sign-in page; **Sign in** goes
      to Dex; `dev@stacks.local` / `password` lands on Up Next.
- [ ] Reloading keeps you signed in; Settings → Sessions lists this browser.
- [ ] The Admin link is there (you are in `WW_ADMIN_EMAILS`).
- [ ] Settings → API tokens: create a `read` token; it is shown once;
      `curl -H "Authorization: Bearer stk_…" http://127.0.0.1:8095/api/v1/library`
      answers 200, and a write with it,
      `curl -X PUT -H "Authorization: Bearer stk_…" -H "Content-Type: application/json" -d '{"status":"planning"}' http://127.0.0.1:8095/api/v1/library/tmdb:movie:550`,
      answers 403 `insufficient-scope`; revoke it and the `GET` answers 401.
- [ ] Sign out from the account menu: back to sign-in, and the old cookie no
      longer works.

### Tracking, with real metadata

- [ ] ⌘K / Ctrl K: search a film, a western show, an anime and a K-drama;
      results carry posters and categories; the category chips narrow them.
- [ ] Open a show: seasons grid, "+1 episode", "Mark up to", completion,
      "Start a rewatch"; the watch history shows each run.
- [ ] Score in every format (Settings → Profile and preferences), favourite, and notes that
      say "Saved".
- [ ] History lists each change; **Undo** reverts a play and a status change.
- [ ] Delete an entry; it is in Settings → Trash; restore it with its score
      and notes intact.
- [ ] Up Next shows the next episode; Calendar shows upcoming airings.
- [ ] Library: grid, list and board views; filter by status, category, tag
      and favourites; sort.
- [ ] Discover: every rail loads (anime, dramas, trending); "Not interested"
      hides a title and Blocklist lists it.
- [ ] Lists: create, add from a title page, drag to reorder, rename, delete
      and restore.
- [ ] Tags: create, colour, assign from a title page, filter the library.
- [ ] Stats: totals, monthly activity, genres for the year.

### Imports, backups and restore

- [ ] Settings → Import: a MyAnimeList XML export imports, low-confidence
      matches appear under "Worth a check" and can be resolved.
- [ ] A TMDB JSON export imports.
- [ ] Settings → Backup & export: download the Stacks backup and the CSV.
- [ ] Sign in as `friend@stacks.local`, preview and restore the backup: the
      library, runs, plays, lists (in order) and tags match the first account.
- [ ] Restoring the same file again changes nothing.
- [ ] `docker compose --profile backup run --rm --no-deps backup once`, then
      the `verify.sh` command from [DEPLOY.md](DEPLOY.md#5-backups-and-restore)
      reports every table matching.

### Resilience

- [ ] Block TMDB for the API (e.g. an `extra_hosts` entry pointing
      `api.themoviedb.org` at `127.0.0.1` in a compose override, then
      `docker compose up -d`): the library still lists everything, a title
      page shows "may be out of date", Discover rails fail one by one, and
      `curl -s 127.0.0.1:9095/metrics | grep stacks_metadata_errors_total`
      counts the failures. Remove the override and it recovers.
- [ ] `docker compose restart postgres` mid-session: the app recovers with
      nothing lost.

### Look and feel

- [ ] Light, dark and system themes from the account menu; the choice
      follows you to another browser.
- [ ] A phone-sized window: bottom navigation, readable title pages.
- [ ] With "reduce motion" set in the OS, transitions are minimal.
- [ ] Keyboard only: the palette, dialogs and menus are usable and focus is
      visible.

### Notifications

- [ ] Settings → Notifications: add a webhook channel (e.g. a
      <https://webhook.site> URL), subscribe it to "An import finished", run an
      import, and see the delivery arrive.

## Starting over

`docker compose --profile dev-idp --profile backup down -v` deletes every
volume, including the database and the dumps.
