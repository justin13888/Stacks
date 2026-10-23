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
provider cannot serve this part of it (an AniList title has no seasons).

### upstream-unavailable

503. The metadata provider (TMDB or AniList) is unreachable, timing out,
answering with 5xx, or its circuit breaker is open, and no stored snapshot
could stand in. Try again later.

### upstream-rate-limited

503 with `Retry-After`. The metadata provider is rate-limiting Stacks. Wait
that many seconds.

### upstream-invalid-response

502. The metadata provider answered with something Stacks cannot use, or
refused a request an operator has to fix (credentials).

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
