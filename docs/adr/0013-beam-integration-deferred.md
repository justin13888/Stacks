# ADR 0013: The Beam integration is out of v0.1.0

- Status: accepted
- Date: 2026-10-24
- Issue: #12 (the integration itself is tracked in #13)

## Context

The prototype shipped a connector for Beam, the maintainer's self-hosted
media server: registered servers with AES-GCM-encrypted API keys, a
`beam_sync` job, availability lookups, and HMAC-verified inbound webhooks,
exercised against a mock Beam container (`compose.mock-beam.yaml`).

The connector targeted an API Beam does not have. Real Beam serves `/v1`
with cookie-only sessions for its own web app: there are no service tokens a
second server could hold, no webhooks, no feed of watch events, and its
library items carry no provider ids Stacks could match against. Shipping
the connector would have meant shipping code that cannot work against any
real Beam, plus encryption-at-rest machinery protecting credentials that do
not exist.

## Decision

v0.1.0 ships without Beam. The connector, the webhook routes, the
`streaming_servers` table, the `WW_SECRET_KEY`-derived encryption key and the
mock-Beam compose file are gone. Stacks stands alone as a tracker.

What Beam has to provide before Stacks can integrate with it, so that the
integration needs no workaround on either side:

1. **Service-token authentication.** A long-lived, scoped, revocable token
   an operator issues to Stacks, sent as a bearer credential, separate from
   Beam's browser sessions. Read-only scopes for library and history.
2. **Provider ids on library items.** Each movie, show and episode carries
   its TMDB id and, for anime, its AniList id (and MAL id where known), so
   Stacks can match an item to a `media_refs` row without fuzzy title search.
3. **A cursor-paginated library listing.** `GET /v1/library?cursor=…` with a
   stable order and a change marker (an `updated_at` or a sync token), so a
   sync can resume and fetch only what changed.
4. **A watch-event feed or webhook.** Either a cursor-paginated feed of
   playback events (started, progressed, finished, with the item, the Beam
   user and a timestamp), or outbound webhooks for the same events signed
   with an HMAC over the raw body and a timestamp, with a documented
   signature header and replay window.
5. **User mapping.** A stable Beam user id plus the OIDC issuer and subject
   (or verified email) the user signed in with, so a Beam user maps to a
   Stacks account through the identity provider both already trust, never
   through a hand-entered username.

With those, Stacks can add: availability on title pages, deep links to play,
and plays logged automatically from finished playback (source `beam`).

## Alternatives rejected

- **Keep the connector behind a flag.** Unreachable code still needs tests,
  migrations and documentation, and a flag invites someone to turn it on.
- **Scrape Beam's web API with a user's session cookie.** Ties Stacks to a
  private, cookie-authenticated surface and stores a user's browser session
  on another server.
- **Implement the Beam side now.** Out of this repository; the requirements
  above are recorded so the work can start from them.

## Consequences

- #13 tracks the integration and is the place to pick it up once Beam
  provides the five items above.
- A future integration adds a migration (append-only after v0.1.0, ADR 0008)
  and a new play `source`; nothing in v0.1.0's data model has to change.
