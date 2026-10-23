# The Stacks backup format

A Stacks backup is one JSON document that holds everything an account owns.
Restoring it into an empty account gives back the same data: the same rows,
with the same ids and timestamps. This page describes the document and how to
restore it. [`backup-format.schema.json`](backup-format.schema.json) is its
JSON Schema (draft 2020-12). That schema is generated from the Rust types in
[`core/src/import_export/backup/format.rs`](../core/src/import_export/backup/format.rs)
with `just gen-backup-schema`, and `cargo test` fails when the committed copy
is stale.

## Taking a backup

```sh
curl -H "Authorization: Bearer $STACKS_TOKEN" \
     -o stacks-backup.json \
     https://stacks.example.com/api/v1/exports/backup
```

Any `read` token works; the operation is `exportBackup`.

Notification channels hold credentials: webhook URLs (Slack and Discord
webhook URLs are secrets), signing secrets, and header values. Their
`config` is left out of the backup, and `secrets_included` is `false`,
unless the request asks for them. Only a signed-in session can ask: an API
token sending `include_secrets=true` is refused with `403
secrets-need-session`, so a leaked token never reads back a credential the
channel API itself never returns. Send the session cookie instead:

```sh
curl -b "stacks_session=…" -o stacks-backup.json \
     "https://stacks.example.com/api/v1/exports/backup?include_secrets=true"
```

Treat a backup taken with `include_secrets=true` like a password file.

## The document

```jsonc
{
  "format": "stacks-backup",        // always this
  "format_version": 1,              // this page describes version 1
  "exported_at": "2026-10-20T18:00:00Z",
  "generator": "stacks 0.1.0",
  "secrets_included": false,
  "user": { "id": "…", "display_name": "Ada", "email": "…",
            "oidc_issuer": "…", "oidc_subject": "…", "created_at": "…" },
  "settings": { "score_format": "point_10" },   // users.settings, as stored
  "titles": [ … ],
  "entries": [ … ],
  "lists": [ … ],
  "tags": [ … ],
  "blocklist": [ … ],
  "notification_channels": [ … ]
}
```

| Member | What it holds |
| --- | --- |
| `titles` | Every title an entry, list item or blocklist row refers to. A title has its catalog `id` from the server the backup came from, its `name` and `kind`, and every external id it is known by in `refs`. Exactly one ref is `primary`: the public id (`tmdb:movie:550`, `anilist:show:21`). The others link the same title across providers (`mal`, `imdb`, `tvdb`). |
| `entries` | Every library entry. Soft-deleted ones are included, with `deleted_at` set. An entry has its `runs`, its `events` and its `tags`. |
| `entries[].runs` | Each watch-through, with its dates and their precision (`day`, `month` or `year`). Run 1 is the first watch. A run has every `play`, and voided plays are included with `voided_at` set. A play names its episode by `season` and `number`. A movie play has no episode. |
| `entries[].events` | The entry's append-only event log, oldest first. `before` and `after` are the rows as they were stored, so they keep the members that the release which wrote them had. |
| `lists` | Lists, soft-deleted ones included. Items are in list order, and each has its `position` (a fractional-index key, compared bytewise). |
| `tags`, `entries[].tags` | Tags, and the entries each one is assigned to. |
| `blocklist` | The titles the user never wants suggested. |
| `notification_channels` | Channels with their subscriptions. `config` is `null` unless the backup was taken with secrets. |

The catalog's metadata is not included: no overviews, posters, seasons or
episode titles. The refs are enough to find each title again, and a restore
fetches the rest in the background. The catalog is shared by every account
on the server, so a restore only reads the refs to find a title: it never
writes a link from a backup. A title it adds gets its other ids from its
provider when the refresh job fetches it.

## Restoring

```sh
curl -H "Authorization: Bearer $STACKS_TOKEN" \
     -F file=@stacks-backup.json \
     "https://stacks.example.com/api/v1/imports/backup?mode=merge&dry_run=true"
```

The operation is `restoreBackup`, and it needs a `write` token. It answers
with a report: how many titles were matched in the catalog or created, a
tally for every kind of row (`inserted`, `updated`, `unchanged`, `skipped`),
each entry's outcome, and warnings for anything that could not be restored
as it was.

The restore procedure:

1. **Dry run first.** Send the backup with `dry_run=true`. The server does
   the whole restore inside a transaction, reports it, and rolls it back.
   The report is exactly what the real run would do.
2. **Restore.** Send it again without `dry_run`. The restore is one
   transaction: it applies everything or nothing. A failure part-way
   through leaves the account as it was.
3. **Re-enter channel secrets.** A channel backed up without its
   configuration is skipped and named in `warnings`. Create it again. A
   channel whose configuration or name the channel API would refuse (a
   URL it does not allow, a header Stacks sets itself, a blank name) is
   not written either, in any mode, and is named in `warnings`.
4. **Let the catalog fill in.** A title the server did not have is created
   with only its name and ids, never fetched. The hourly `metadata_refresh`
   job fetches never-fetched titles first. Until then the library shows it
   by name.

Restoring the same backup twice is harmless: the second run finds every
row and adds nothing.

### Modes

`mode` says what happens to rows the account already has. A row is the
same row when it has the same id. Ids are also matched by their natural
key: an entry by its title, a run by its number, a tag by its name, a
list item by its title, a blocklist row by its title, and a subscription
by its channel and event.

| Mode | An existing row | Plays, events, tag assignments |
| --- | --- | --- |
| `merge` (default) | An incoming value fills a field that is empty locally: a score, notes, a run date, a tag colour or a list description. A field that is set on both sides keeps the local value. | Unioned. A play the backup has voided is voided here too. |
| `skip` | Left untouched, with everything under it. | Added only under rows the restore adds. |
| `overwrite` | The backup's values replace the local ones. | Unioned. |

No mode deletes anything or un-voids a play. Every change a restore makes
to a row that already existed is written to that entry's event log, so
history shows it like any other edit.

### Ids

Each row keeps the id it has in the backup, unless that id is already taken
by another row on this server. That happens when you restore into a second
account on the server the backup came from. A row with a taken id gets a
new id, and every reference to it follows, including the ids inside event
payloads. A later restore of the same backup still finds these rows: a play
by its run, episode and logging time, an event by its kind and time, and a
list or channel by its name and creation time.

A play's `import_id` is kept only when that import belongs to the restoring
account. Otherwise the play keeps its `import` source and loses the link.

### Versions

A newer Stacks can always read an older backup. Each older `format_version`
is upgraded one version at a time by explicit migrations in
[`core/src/import_export/backup/upgrade.rs`](../core/src/import_export/backup/upgrade.rs).
A backup with a newer `format_version` than the server knows is refused
with a 422 `unsupported-backup-version`. Restore it with a newer Stacks.

| Version | Written by | Notes |
| --- | --- | --- |
| 0 | pre-release builds (`{"version": 1, "entries": […]}`) | One row per live entry: status, score and the first run's dates. Upgrades to version 1 with those fields. Its `progress` count named no episodes, so it is dropped. |
| 1 | Stacks 0.1.0 and later | This page. |

### Refusals

| Status | Problem type | When |
| --- | --- | --- |
| 422 | `unreadable-backup` | Not JSON, no `"format": "stacks-backup"`, or not a valid document of its version. |
| 422 | `unsupported-backup-version` | Written by a newer Stacks. |
| 422 | `invalid-backup` | The document contradicts itself, or breaks a rule the database holds every row to. Examples: an entry that names a title the document lacks, a score outside 0–100, a run date without its precision, a blank list or tag name, a list filter that is not an object, an `import_id` on a play whose source is not `import`, or settings that no release would write. |
| 413 | | Larger than the deployment's import size limit (`WW_IMPORT_MAX_BYTES`). |

Nothing is written in any of these cases.

## The CSV export

`GET /api/v1/exports/csv` (`exportCsv`) writes one row per live library
entry, for spreadsheets. The columns are `title`, `name`, `kind`,
`category`, `status`, `score`, `favourite`, `progress` (episodes in the
current run), `episodes`, `plays`, `rewatches`, `started_on` and
`completed_on` (the current run's dates), `first_watched_at`,
`last_watched_at`, `tags`, `notes`, `added_at` and `updated_at`. The CSV is
lossy by design and cannot be imported. Use the backup to move data
between servers.

## Importing from MyAnimeList and TMDB

`POST /api/v1/imports` (`createImport`) accepts other trackers' exports. It
checks the file, keeps the original, and queues a background import. The
import is followed with `getImport`.

| `source` | File |
| --- | --- |
| `mal_xml` | MyAnimeList's own export (`animelist_*.xml`), after `gunzip`. |
| `mal_json` | MyAnimeList API v2's anime list (`GET /v2/users/{user}/animelist?fields=list_status,num_episodes,media_type,start_date`): one page, an array of pages, or the bare `data` array. |
| `tmdb_json` | TMDB account lists: an object keyed by `rated/movies`, `rated/tv`, `watchlist/movies`, `watchlist/tv`, `favorite/movies` and `favorite/tv`, each holding that endpoint's v3 or v4 response page (or an array of pages). A response page on its own also works, as does the plain `[{"tmdb_id", "title", "media_type", "status", "rating"}]` array. |

From MAL, the importer reads the status, the score (MAL's 0 means no
score), the episodes watched, and the start and finish dates. A date keeps
its precision: `2024-05-00` is May 2024, and `0000-00-00` is no date. It
also reads the rewatch count, whether a rewatch is under way, and the
comments. From TMDB, a rated title is `completed` with its rating as the
score, a watchlisted one is `planning`, and a favourite gets the favourite
flag.

Each row is matched to a title in this order:

1. an external id the catalog already links (MAL, IMDb, TVDB). Only a
   provider writes these links, so they hold for every account;
2. the user's own earlier match of the same MAL id, from one of their
   earlier imports;
3. a TMDB id, which names the title outright;
4. AniList's own link to the MAL id;
5. a title search, scored by title similarity and by agreement on the
   year, the kind and the episode count. A result matches when it scores at
   least 0.88 and no other result comes within 0.03 of it.

A row that is not matched is kept as `unmatched`, together with its best
candidates. Resolve it with `PATCH /api/v1/imports/{id}/items/{row}`
(`resolveImportItem`). A row matched by search or resolved by hand is
remembered for that user only, so their next import of that title matches
on its own; another account's rows are never matched by it. A row the
import job is working on can be resolved at the same time: it is applied
once, by whichever comes first.

A matched row becomes the entry's status, score, notes and favourite flag.
Run 1 carries the row's dates, and each rewatch adds a run. The current
run gets a play for each watched episode, or for a watched movie. These
plays have `source = import` and point at the import. Importing the same
file again logs nothing twice.

No row is ever dropped. `total` always equals applied + skipped + unmatched
+ failed + pending_retry, and a failed row records its reason. When the
metadata provider is down or rate-limiting, the import stops asking it at
the first refusal: that row and every row after it wait as `pending_retry`,
and the import goes back to `pending`. A run that settled rows comes back
after 30 s without spending one of the job's 30 attempts, so a long list
that keeps advancing is never given up on. A run that settled none spends
one and backs off (30 s, doubling, capped at an hour): about a day of an
outage. When the attempts run out the import is `failed`, its waiting rows
are kept, and an administrator retrying the job resumes it. A stored file
that no longer parses fails the import at once, with no completion
notification.

Importing from Trakt, Letterboxd, AniList or Simkl is future work.

## Whole-server backups

A backup covers one account. To back up a whole server, dump the database
with `pg_dump`; see [DEPLOY.md, "Backups"](DEPLOY.md#5-backups).
