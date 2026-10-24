# ADR 0012: The test stack

- Status: accepted
- Date: 2026-10-24
- Issue: #12

## Context

v0.1.0 is QA'd by the maintainer directly from the release, and the data it
holds is meant to be kept forever. The tests have to prove the parts a unit
test cannot see: that the OpenAPI document matches what the server does,
that the browser app works against a real identity provider, that the
library survives a provider outage, and that a backup restores losslessly.

## Decision

Each layer has one tool, chosen for what it proves:

| Layer | Tool | What it proves |
| --- | --- | --- |
| Rust units and services | `cargo test`, `proptest` | Domain rules; "PATCH never clears untouched fields" and the backup round trip as properties. |
| Database | `cargo test` against PostgreSQL 18 (an ephemeral database per test) | Constraints, transactions, the job queue. |
| Upstreams | `wiremock` | Every cameo call path under 500s, 429s, timeouts, nulls and malformed JSON (ADR 0010). |
| HTTP contract | kynos `TestClient` with `assert_conformance` and `assert_declared_responses_covered` | Every declared response is produced and matches the document (ADR 0003). |
| Spec consumers | the orval contract harness (ADR 0002) and the spargen client in `tests/e2e` | Two independent generators accept the document and drive a real server. |
| Web components | Vitest + Testing Library + jsdom, with axe | Components and pages, accessibility (ADR 0007). |
| The composed stack | Playwright in `e2e/` | The real images, a real Dex login, the full journey, every screen in both themes, and the outage drills. |

The **browser suite** runs against `compose.yaml` + `compose.build.yaml` +
`e2e/compose.e2e.yaml`: the released stack's compose files with the images
built from the checkout, the bundled Dex (with two static users), and a Bun
fixture server standing in for TMDB and AniList, which the drills flip to
500s or 429s through a control endpoint. It runs in order on one worker:
the journey (search, track, episodes, complete, rewatch, tag, list and
reorder, export, restore into a second account and compare), the screens
pass (screenshots uploaded as a CI artifact), and the drills (TMDB down,
AniList rate-limited, PostgreSQL stopped mid-session). After it, the backup
sidecar's dump is restored into a scratch database and compared row for row.

It runs in CI's `E2E (composed stack)` job on every pull request, and
`ci-ok` requires it. The job also builds both images, replacing the old
image-build job, so its cost is mostly the build that already ran; the
suite itself takes about two minutes.

## Alternatives rejected

- **Cypress.** One browser engine per run in its free tier, runs inside the
  page (cross-origin redirects to Dex need workarounds), and no first-class
  multi-context support for the second account.
- **Real TMDB and AniList in e2e.** Needs a secret in CI, is
  non-deterministic, and cannot be made to fail on demand.
- **An auth bypass for e2e.** Would leave the most fragile flow, the OIDC
  round trip through a proxy and split-horizon issuer, untested.
- **A nightly e2e job instead of per pull request.** A broken journey would
  land on master and be found a day later; with the image build folded in,
  the per-PR cost is small.
- **`bun test` for the web app.** No DOM (ADR 0007).

## Consequences

- `just e2e-up`, `just e2e`, `just e2e-down` reproduce CI locally
  (docs/QA.md). The suite needs a container runtime that can run the
  compose stack.
- A UI change that renames an accessible label the suite uses fails CI, by
  design: the suite reads the page the way a person or a screen reader does.
