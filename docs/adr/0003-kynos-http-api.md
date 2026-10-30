# ADR 0003: The HTTP API on kynos

- Status: accepted
- Date: 2026-10-23
- Issue: #6

## Context

The API ran on Salvo 0.77. Its OpenAPI document was assembled from a second,
hand-maintained router (`build_router_stub`) and then patched
(`inject_path_params`), and request bodies and query parameters were missing
from it. About forty-five error types hand-implemented Salvo's `Writer` and
`EndpointOutRegister`; handlers fetched their state from a string-keyed
`Depot` with more than a hundred `unwrap`/`expect` calls; malformed JSON was a
500; and API-token scopes were parsed but never enforced.

The maintainer asked to "re-define entire thing with kynos and openapi 3.1
(avoid 3.2 unless very necessary by some feature)", for "all api code [to] be
idiomatic and not use hacks", and allowed the endpoints to be redesigned "to
be as idiomatic as possible". ADR 0002 established that a kynos 3.1 document
survives orval 8 and strict TypeScript with configuration alone.

## Decision

**kynos 0.4 from crates.io**, features `openapi31, macros, server, http1,
http2, json, multipart, cookie, compression, trace, uuid, time-chrono, docs`
(and `test-util` in tests). `openapi32` stays off, so a construct only 3.2 can
describe does not compile, and `gen-openapi` exports with
`Router::openapi_as(SpecVersion::V3_1)`, which refuses rather than downgrades.

### The document is the router

`api::app::router` is the only description of the public API. The document
`gen-openapi` writes to `web/openapi.json`, the routes `main` serves and every
test are built from that one value, and nothing edits the document afterwards.
`web/openapi.json` is committed; `api/tests/router.rs` and CI's
`mise run check:openapi` fail when it differs from what the router emits, so the
web client and the e2e client always build from the real contract.

### Shape

- Everything is under `/api/v1`, except `/healthz`, `/readyz` and the
  reference at `/api/docs` (with `/api/openapi.json`).
- One module per area (`sessions`, `account`, `catalog`, `library`, `lists`,
  `tags`, `blocklist`, `imports`, `exports`, `stats`, `notifications`,
  `admin`, `ops`), each holding its tag, its DTOs, its error types and its
  handlers, and returning a `Group`.
- State is typed: `App` derives `Provider`, and handlers take
  `Inject<DatabaseConnection>`, `Inject<Arc<dyn MetadataProvider>>` and so on.
  A handler asking for something the context lacks does not compile; nothing
  is looked up at run time, and clippy denies `unwrap_used`/`expect_used` in
  the crate.
- Collections answer `Page<T> { items, next_cursor }`. Cursors are opaque
  keyset positions; one the server did not issue is a 400, never a silent
  first page.
- Titles are named by their public reference (`tmdb:movie:550`), a
  `TitleRef` parameter type; Stacks' row ids never appear where a reference
  applies.
- `PATCH` has merge semantics (`Nullable<T>` tells "absent" from `null`),
  `PUT` creates or replaces, and deletes are soft where the data model is.
- Errors are RFC 9457 problem documents, per operation (ADR 0006).
- Guards are types: `Reader`, `Writer`, `Administrator`, `SessionOnly`
  (ADR 0004).

### Interceptors

Each one declares what it can answer, so its statuses appear on exactly the
operations it covers:

| Interceptor | Scope | Why |
| --- | --- | --- |
| `SecurityHeaders` (`deny_framing`) | everything | `nosniff`, `no-referrer`, `no-store`, `X-Frame-Options: DENY` |
| `Cors` | everything | the configured origins only, with credentials; none by default, since the web app reaches the API through its own origin |
| `RequestId` | everything | an `X-Request-Id` per response, correlated in the trace |
| `Compression` | everything | gzip/brotli/zstd by `Accept-Encoding`; declares the 406 a client refusing every coding gets |
| `Csrf` | `/api/v1` | refuses a cross-site unsafe request by Fetch Metadata, before it reaches the session cookie |
| `RateLimit` | `/api/v1` | per client address (behind `WW_TRUSTED_PROXY_HOPS` proxies), `WW_RATE_LIMIT_PER_MINUTE`; counters in memory, since Stacks is one process |
| `BodySize` | `createImport` only | uploads may exceed kynos's 2 MiB default; every other body keeps it |

Not mounted, deliberately:

- **`catch_panics`.** It would add a 500 to every operation, including those
  that cannot fail, which no test could then exercise. Panics are ruled out by
  construction instead (no `unwrap`/`expect`, every failure a declared
  response); a panic that still happened would end one connection, not the
  process.
- **`Timeout` and `Concurrency`.** Their 408 and 503 would join every
  operation's contract for a failure Stacks bounds elsewhere: metadata calls
  carry their own deadlines and circuit breakers, the database pool its
  acquire timeout, and kynos's server its header-read and request-body idle
  timeouts.

### Two listeners

`/metrics` is an operator endpoint, not part of the public contract. kynos
describes every route it serves, so `/metrics` lives on a second router and
listener (`WW_METRICS_BIND`, default `0.0.0.0:9095`) with a document of its
own, which is what kynos's documentation recommends "where the contract must
not move" (ADR 0005).

## Gaps found, and what Stacks does about each

| Gap | Upstream | In Stacks |
| --- | --- | --- |
| `Scoped<S, R>` takes one scheme, so "a session, or a token with `write`" cannot be a scope set | [getkono/kynos#1000](https://github.com/getkono/kynos/issues/1000) | One scheme type per scope set (`ReadToken`, `WriteToken`, `AdminToken`, `AdminSession`) registered under shared component keys as default scopes |
| Every guard declares a 403, even one whose authenticator cannot forbid | [getkono/kynos#1001](https://github.com/getkono/kynos/issues/1001) | The metrics test leaves that one 403 out of its coverage check, with the issue linked |
| An authenticator can answer only 401 or 403 when its store fails | [getkono/kynos#993](https://github.com/getkono/kynos/issues/993) (fixed on `master` after 0.4.0) | A 403 with its own problem type, `credential-store-unavailable` (ADR 0004) |
| `Option<T>` parameters are described as nullable | [getkono/kynos#992](https://github.com/getkono/kynos/issues/992) (fixed on `master` after 0.4.0) | The web client drops `null` query values (ADR 0002) |
| `#[problem(extension)]` members are not in the response schema | [getkono/kynos#991](https://github.com/getkono/kynos/issues/991) | No Stacks problem relies on an extension member |
| `Json<T>` admits a JSON array for an object whose members all default | [getkono/kynos#1002](https://github.com/getkono/kynos/issues/1002) | Such bodies nest a defaulted member so serde reads only a map; the generic sweep sends `[]` to every body and fails on any 2xx |
| A `Reply` variant holds only a body, so one return type cannot answer both `201` with `Location` and `200` | [getkono/kynos#1004](https://github.com/getkono/kynos/issues/1004) | `putLibraryEntry`'s reply implements `IntoResponse` and `Responses` by hand |
| A multipart field must be `String`, `Bytes` or `FilePart`, not a `ParamValue` enum | [getkono/kynos#1005](https://github.com/getkono/kynos/issues/1005) | The import upload's two enums implement `FromPart` and `IntoPart` by hand |
| spargen 0.5 reads an inline discriminated branch's tag from the name it generated rather than from the branch's `const`, so it cannot decode the `oneOf` kynos emits for an internally tagged enum | [getkono/spargen#1001](https://github.com/getkono/spargen/issues/1001) | The document is correct and orval reads it; the e2e journey's notification-channel round trip is `#[ignore]`d with the issue linked |

None of these needed a git pin: every one has a working 0.4.0 form. When a
kynos release carries #992 and #993, Stacks moves to it (the release
checklist re-checks crates.io).

## Alternatives rejected

- **Keep Salvo.** Its OpenAPI support describes handlers through attributes
  the compiler does not tie to what they read and return, which is how the
  document drifted; and it is the maintainer's explicit reason for the move.
- **axum with utoipa or aide.** The same problem in a different place: the
  description is a parallel annotation, and nothing checks it against traffic.
  kynos refuses a handler it cannot describe and proves the document against
  real responses (`assert_conformance`, `assert_declared_responses_covered`).
- **actix-web with utoipa or paperclip.** Fast and mature, but the same
  parallel-annotation problem as axum, its own actor-flavoured runtime and
  extractor conventions, and no request-level conformance checking.
- **poem-openapi.** Derives the description from types too, but its
  extractors and responses are its own vocabulary rather than RFC 9457
  problems and typed guards, and it emits 3.0.
- **dropshot.** Generates from types, but is built around its own server
  model and emits 3.0.

## Consequences

- Every area has a TestClient suite that drives each declared response and
  checks conformance, and a generic sweep (`api/tests/support/sweep.rs`) that
  drives the statuses every operation shares from the document itself.
- The committed `web/openapi.json` changes with every API change and with
  each release (its `info.version`); the release workflow regenerates it with
  the lockfiles.
- A kynos upgrade is gated by those suites, by the contract harness (ADR
  0002) and by the spargen e2e crate, which is a second, independent consumer
  of the document.
