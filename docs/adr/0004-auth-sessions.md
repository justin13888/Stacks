# ADR 0004: Sign-in, sessions and API tokens

- Status: accepted
- Date: 2026-10-23
- Issue: #6

## Context

Stacks authenticated with a hand-rolled OpenID Connect flow that decoded the
id_token without checking its signature, then issued HS-signed JWT access
tokens and rotating refresh tokens. The web app kept the refresh token in
`localStorage`, where any script on the page can read it. API tokens carried
scopes that nothing enforced. `jsonwebtoken` and `WW_SECRET_KEY` existed only
for this.

Stacks is a self-hosted app with one web client served from its own origin
and, for automation, personal API tokens.

## Decision

### Sign-in: OpenID Connect through `openidconnect`

The authorization code flow with PKCE (S256), through the `openidconnect`
crate:

1. `POST /api/v1/auth/oidc/authorizations` records a pending request
   (`oidc_login_states`: state, nonce, PKCE verifier, ten-minute expiry) and
   returns the provider's authorization URL. Nonce and verifier never leave
   the server.
2. The provider redirects the browser back to the web app with `code` and
   `state`, which it posts to `POST /api/v1/sessions`.
3. The API consumes the pending request (single use: the CSRF and replay
   check), exchanges the code with the verifier, and has `openidconnect`
   verify the id_token: signature against the provider's JWKS, issuer,
   audience, expiry and nonce. Only then is the account found or created by
   `(issuer, subject)`.

Discovery is cached for five minutes. When `WW_OIDC_INTERNAL_ISSUER` is set,
discovery and the token and JWKS endpoints use that address while the
browser keeps the public one and id_tokens must still name the public issuer
(the containerised "split horizon"). `openidconnect` runs on the workspace
reqwest through oauth2's `AsyncHttpClient`, with redirects disabled and a
1 MiB response cap.

Failures are classified: a refused or forged sign-in is a 400
`sign-in-failed` (start again), an unreachable or misbehaving provider a 502
`identity-provider-unavailable`, a deactivated account a 403
`account-deactivated`.

### Sessions: opaque, server-side, in an httpOnly cookie

A sign-in creates a `sessions` row holding the SHA-256 of a 256-bit random
token, and sets the token as the `stacks_session` cookie:

- `HttpOnly` (no script can read it), `SameSite=Lax`, `Path=/api`,
  `Max-Age` of the session's lifetime (`WW_SESSION_TTL_SECS`, 30 days);
- `Secure` whenever the web app is served over HTTPS (read from the OIDC
  redirect URI's scheme).

There are no JWTs and nothing to refresh: a request with the cookie is
looked up by hash, signing out (`DELETE /sessions/current`) deletes the row
and clears the cookie, and `DELETE /sessions/{id}` ends another device's
session. Revocation takes effect on the next request. `last_seen_at` is
refreshed at most every five minutes, so reads do not become writes.

Cross-site request forgery is refused twice: by the cookie's `SameSite=Lax`
in the browser, and by the API's Fetch Metadata check (`Csrf`), which refuses
an unsafe request whose `Sec-Fetch-Site` says it came from another site.

### API tokens: scoped bearer credentials

`stk_` plus 256 random bits, stored as SHA-256 (one indexed lookup per
request; there is nothing to brute-force, so no salt or slow hash is needed),
with scopes `read`, `write` and `admin`, an optional expiry, and soft
revocation. Only an administrator can grant `admin`, and an `admin` token
stops working as such when its owner is demoted. Tokens are created and
revoked only from a signed-in browser (`SessionOnly`), so a leaked token
cannot mint more or end its owner's sessions.

### Guards

| Guard | Accepts | Used by |
| --- | --- | --- |
| `Reader` | a session, or a token holding `read` | every read |
| `Writer` | a session, or a token holding `write` | every change to the caller's data |
| `Administrator` | an administrator's session, or an administrator's token holding `admin` | `/admin/*` |
| `SessionOnly` | a session | credential management |

A token without the scope an operation needs gets 403 `insufficient-scope`;
`api/tests/scopes.rs` checks every operation in the document. A valid
credential of a deactivated account gets 403 `account-deactivated`.

When PostgreSQL cannot be reached to check a credential, kynos 0.4.0 lets an
authenticator answer only 401 or 403
([getkono/kynos#993](https://github.com/getkono/kynos/issues/993), fixed on
kynos `master` since). A 401 would make the web app treat the session as gone
and sign the user out during an outage, so Stacks answers 403
`credential-store-unavailable`, which a client can tell apart and retry. It
moves to the 503 once a kynos release carries it.

## Alternatives rejected

- **Keep JWT access and refresh tokens.** Revocation is either impossible
  (until expiry) or a server-side lookup anyway; a refresh token still has to
  live somewhere the browser can send it, and `localStorage` is readable by
  any injected script. An opaque session is one lookup and is revoked by
  deleting a row.
- **A token-mediating backend in the web container.** Stacks' web container
  is static files behind nginx; the API is already same-origin through it, so
  the API can set the cookie itself.
- **Sessions in Redis.** One more service to run for a single-process app
  whose database is already PostgreSQL.
- **A hand-rolled OIDC client.** That is what was replaced: the previous one
  never verified a signature. `openidconnect` verifies everything the
  specification requires and is maintained.

## Consequences

- `WW_SECRET_KEY`, `WW_ACCESS_TOKEN_TTL_SECS` and `WW_REFRESH_TOKEN_TTL_SECS`
  are gone; `WW_SESSION_TTL_SECS` sets the session lifetime.
- The web app holds no credential: it calls the API with
  `credentials: "include"` and learns who is signed in from
  `GET /api/v1/sessions/current`.
- `rsa` reaches the tree through `openidconnect`, for verifying RS256
  id_tokens; its RUSTSEC-2023-0071 advisory (private-key timing) is ignored in
  `deny.toml` with that reason, since Stacks performs no RSA private-key
  operation.
