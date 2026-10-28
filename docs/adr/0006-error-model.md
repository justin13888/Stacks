# ADR 0006: The error model

- Status: accepted
- Date: 2026-10-23
- Issue: #6

## Context

Under Salvo every module hand-wrote its HTTP mapping, errors were a
Stacks-specific `{ "error": { "code", "message" } }` envelope, the document
listed responses by hand (and missed most of them), and malformed JSON was a
500. A client could neither tell failures apart reliably nor trust the
document to say which ones an operation can produce.

## Decision

### RFC 9457 everywhere

Every error is an `application/problem+json` document: `type` (a URI naming
the class of failure), `title`, `status`, `detail` (a sentence for a human),
and occasionally a header such as `Retry-After`. A client branches on
`status` and `type`, never on `detail`.

- Stacks' own types are `https://github.com/justin13888/Stacks/blob/master/docs/adr/0006-error-model.md#<slug>`,
  so each one links to its entry below (`api::problem::BASE`).
- Failures kynos itself detects carry `type: about:blank`: an unparseable
  path, query or header parameter (400), a malformed JSON body (400), a body
  that does not match its schema (422, with an `errors` list of JSON
  Pointers), an unsupported media type (415), a body over its limit (413),
  an `Accept-Encoding` refusing every coding (406), a missing or invalid
  credential (401, with `WWW-Authenticate`), a cross-site unsafe request
  (403), and the rate limit (429, with `Retry-After` and `X-RateLimit-*`).

### One error type per operation

Each operation returns `Result<_, E>` where `E` is a
`#[derive(ApiError)]` type declaring exactly the statuses that operation can
produce; failures many operations share (`Internal`, `TitleNotFound`,
`Upstream`, `InvalidCursor`) are held with `#[problem(transparent)]`. There
is no catch-all error type and no hand-written response list, so the
document cannot advertise a status an operation never sends: every area's
suite asserts that each declared response was observed
(`assert_declared_responses_covered`) and that each observed response
matched its declaration (`assert_conformance`).

### What a problem may say

A problem says only what the request determined. Database and provider
failures are logged where they happen (with the call site) and answered
with a fixed sentence; nothing from a SQL error, a stack trace or an
upstream body reaches a client.

## Problem types

### internal

500. Stacks failed in a way the client could not have caused (most often the
database). The cause is in the server log, not the response. Retrying may
help if the cause was transient.

### invalid-cursor

400. The `cursor` was not one this server issued (or was issued for another
listing or sort). Restart from the first page.

### title-not-found

404. The title reference names nothing the metadata provider has, or a
provider cannot serve this part of it (a movie has no seasons, an AniList show
no season but its first).

### upstream-unavailable

503. The metadata provider (TMDB or AniList) is unreachable, timing out,
answering with 5xx, or its circuit breaker is open, and no stored snapshot
could stand in. Try again later.

### upstream-rate-limited

503 with `Retry-After`. The metadata provider is rate-limiting Stacks. Wait
that many seconds.

### upstream-invalid-response

502. The metadata provider answered with something Stacks cannot use
(malformed JSON, an explicit `null` in a required field, GraphQL errors).
Retrying may help once the provider recovers.

### upstream-misconfigured

502. The metadata provider refused Stacks' request itself: an invalid or
revoked TMDB key (401/403), or another 4xx. Every operation that reads the
provider answers this the same way, and a search that degrades names the
failure's class as `misconfigured`. Retrying will not help; an operator has
to fix the configuration (`WW_TMDB_API_KEY`).

### account-deactivated

403. The credential is genuine, but an administrator deactivated the
account. Signing in again will not help.

### insufficient-scope

403. The API token lacks the scope the operation needs (`read`, `write`,
`admin`), or the account is not an administrator.

### credential-store-unavailable

403. The credential could not be checked because the credential store
(PostgreSQL) failed. Not a statement about the credential: retry. (It
becomes a 503 once kynos releases
[getkono/kynos#993](https://github.com/getkono/kynos/issues/993); see
ADR 0004.)

### sign-in-failed

400. The sign-in could not be completed: the `state` is unknown, expired or
already used, the identity provider refused the code, or the id_token failed
verification. Start a new sign-in.

### identity-provider-unavailable

502. The identity provider could not be reached or answered unusably
(discovery, key set or token endpoint).

### session-not-found

404. The caller has no session with that id.

### invalid-display-name

422. A display name must not be blank.

### api-token-not-found

404. The caller has no live API token with that id.

### invalid-api-token

422. A token needs a non-blank name, at least one scope, and an expiry in
the future (when it has one).

### scope-not-permitted

422. Only an administrator can create a token with the `admin` scope.

### database-unavailable

503 (`/readyz`). PostgreSQL did not answer.

### migrations-pending

503 (`/readyz`). The database schema is behind this release.

### entry-not-found

404. The title is not live in the caller's library (it was never added, or
it is in the trash).

### list-not-found

404. The caller has no list with that id (or none in the trash, for a
restore).

### list-item-not-found

404. The title, or the `after`/`before` anchor, is not in the list.

### invalid-list

422. A list needs a non-blank name and a usable filter, and only a smart
list has a filter.

### already-in-list

409. The title is already in the list.

### not-a-manual-list

422. A smart list's titles come from its filter; items cannot be added to
it.

### invalid-placement

422. `after` and `before` must name one gap of the list, and not the item
being moved.

### tag-not-found

404. The caller has no tag with that id.

### tag-name-taken

409. The caller already has a tag with that name.

### invalid-tag

422. A tag needs a non-blank name, and a colour is `#RRGGBB`.

### trashed-entry-not-found

404. The title is not in the caller's trash, so there is nothing to
restore.

### entry-live

409. The title is in the caller's library rather than its trash, so there
is nothing to restore; the entry is left as it is.

### entry-exists

412. A `putLibraryEntry` sent with `If-None-Match: *` only adds, and the
title is already in the caller's library; the entry is left exactly as it
was (a title in the trash counts as absent and is brought back).

### watch-run-not-found

404. The library entry has no watch run with that number.

### episode-not-found

404. The show has no episode with that season and number.

### wrong-play-kind

422. The play does not fit the title: episodes were logged for a movie, or
a movie play for a show.

### run-dates-out-of-order

422. The dates would put a watch run out of order: it would finish before
it started, or a rewatch would start before the run it follows finished.
Dates are compared only as far as both are known, so "2019" is not before
"June 2019". Nothing is changed.

### play-not-found

404. The caller has no play with that id.

### play-already-voided

409. The play was voided already; `restorePlay` brings it back.

### play-not-voided

409. The play is not voided, so there is nothing to restore.

### invalid-date-range

422. A calendar's `to` must be on or after its `from`, and at most 62 days
after it.

### import-not-found

404. The caller has no import with that id.

### import-item-not-found

404. The import has no row with that number.

### import-item-not-resolvable

409. The row cannot take that resolution in its current outcome: a match
needs a row the import has read (not one still `pending`), and a skip needs
an unmatched, failed or retrying row. `detail` names the row's outcome.

### unreadable-import

422. The file is not what its `source` says: that source's parser refused
it.

### unreadable-backup

422. The file given to `restoreBackup` is not a Stacks backup: not JSON, no
`format`, or not a valid document of its version.

### unsupported-backup-version

422. The backup was written by a newer Stacks, in a format version this
server cannot read. Restore it on that version or later.

### invalid-backup

422. The backup reads, but contradicts itself: a reference to a title it
does not hold, or a value out of range (such as a score).

### secrets-need-session

403. `exportBackup` was asked for `include_secrets=true` with an API token.
Notification channel credentials leave Stacks only through a signed-in
session (ADR 0004).

### notification-channel-not-found

404. The caller has no notification channel with that id.

### invalid-notification-channel

422. The channel's configuration cannot be delivered: a URL that is not
absolute http(s), a URL whose host is an address Stacks does not deliver to
(loopback, link-local, multicast and the like always; a private network
unless `WW_NOTIFICATION_ALLOWED_HOSTS` allow-lists it), a header Stacks sets
itself, a blank name or tag, or a patch meant for another channel type.

### notification-delivery-failed

502. A test delivery did not reach the receiver, or the receiver refused
it. The `category` member says which, and nothing more: `refused-address`
(the host resolves only to addresses Stacks does not deliver to),
`timeout` (no answer within ten seconds), `http-error` (a status other
than 2xx; redirects are not followed) or `unreachable`. The `detail` is a
fixed sentence per category; the receiver's status and the transport error
go to the server's log, so the answer cannot be used to probe what is
listening on the server's network.

### notification-subscription-not-found

404. The caller has no notification subscription with that id.

### subscription-exists

409. The channel is already subscribed to that event.

### user-not-found

404. No account has that id.

### cannot-change-own-account

409. An administrator cannot demote or deactivate their own account, so a
deployment always keeps one administrator who can sign in.

### cannot-purge-own-account

409. An administrator cannot permanently delete their own account.

### user-not-deactivated

409. An account is purged only once it is deactivated; deactivate it first.

### purge-not-confirmed

422. The purge's `confirm` parameter is not the account's id.

### job-not-found

404. No background job has that id.

### job-not-retryable

409. Only a failed job can be retried; this one is still queued, running,
or done.

## Alternatives rejected

- **A Stacks-specific error envelope.** Every HTTP client and generator
  understands RFC 9457, and kynos's own rejections already use it; a second
  shape would mean two error decoders.
- **One error enum per area.** Simpler to write, but every operation in the
  area would then declare every status any of them can produce, which
  generated clients turn into dead branches, and which the coverage
  assertion rightly refuses.

## Consequences

- Adding a failure means adding a variant to that operation's error type and
  an entry here; the suites fail until a test produces it.
- orval types each operation's error responses as a union narrowed by
  `status`, and the problem `type` of a Stacks failure is a literal there.
