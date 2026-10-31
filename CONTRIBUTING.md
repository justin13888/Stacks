# Contributing to stacks

Thanks for your interest. stacks is a small, opinionated codebase; this guide
captures the toolchain, conventions, and review expectations.

## Prerequisites

- **Rust 1.99.1**, pinned exactly by [`rust-toolchain.toml`](rust-toolchain.toml)
  (every crate declares `rust-version = "1.99"`). rustup installs it, with
  `rustfmt`, `clippy`, `rust-analyzer` and `llvm-tools-preview`, the first
  time you run `cargo` in the workspace.
- **mise** for the workspace tasks and the tools they use.
  https://mise.jdx.dev. After cloning, `mise trust` the repository's
  [`mise.toml`](mise.toml), then `mise install` provides the pinned
  versions of everything below except Rust and PostgreSQL:
  - **Bun** for the web app, used as both runtime and package manager.
  - **Lefthook** for the hooks.
  - **convco** for the `commit-msg` hook.
  - **cargo-llvm-cov** for coverage (`mise run coverage`).
  - **cargo-deny** and **cargo-audit** for `mise run audit`, and
    **cargo-mutants** for `mise run mutants`.
- **PostgreSQL >= 18** — the only data store. `compose.dependencies.yaml`
  brings one up on `localhost:5432`. The DB-backed tests in `api` and `core`
  need it (see Tests).

Run `mise run install-hooks` once after cloning to register the lefthook hooks.

## Workspace layout

```
api/         stacks-api crate — kynos HTTP server; its router is the OpenAPI 3.1 document
core/        stacks-core crate — domain library (no HTTP)
  migration/ the SeaORM baseline migration against PostgreSQL
tests/e2e/   black-box journeys through a spargen client of web/openapi.json
web/         Bun + Vite + React 19; orval-generated API client
e2e/         Playwright browser suite against the composed stack
mise.toml    pinned tools + workspace tasks (format / lint / test / codegen / release)
lefthook.yml pre-commit (format-fix + lint-fix), commit-msg, pre-push (full check)
```

`api` depends on `core` and on `migration`, which it runs at startup;
`core` uses `migration` only as a dev-dependency, to build each test
database from the real baseline. `tests/e2e` depends on all three. The
frontend never talks to `core` directly — it generates a typed client from
`web/openapi.json`, the document `api`'s router emits.
[`AGENTS.md`](AGENTS.md) has the module-level layout.

## Local dev loop

[`mise.toml`](mise.toml) is the single source of truth for every dev / CI /
hook command. CI (`.github/workflows/`) and `lefthook.yml` both call into the
same tasks — don't inline `cargo` / `bun` invocations elsewhere, add or
extend a task instead. `mise tasks` shows everything.

```bash
mise run check              # full gate: format-check + lint + typecheck + test + contract + check:openapi + check:env-docs + check:compose + check:file-length (same as CI / pre-push)
mise run format             # write cargo fmt + biome format fixes
mise run lint               # clippy -D warnings + biome lint (read-only)
mise run lint-fix           # apply clippy --fix + biome lint --fix
mise run test               # cargo test (api, core) + Vitest (web)
mise run coverage           # cargo llvm-cov for api + core, 80% line minimum (set DATABASE_URL)
mise run check:file-length  # fail on a file longer than its package's limit
mise run mutants            # cargo-mutants over core (slow; CI runs it weekly in shards)

# Iterating on the API surface:
mise run gen:openapi        # rewrite the committed web/openapi.json from the router
mise run codegen            # orval client + route tree from web/openapi.json (no Rust needed)
```

`mise run codegen` is the canonical way to regenerate `web/src/api/`. That
directory is `.gitignore`d; `typecheck:web` and `test:web` run codegen
first, so a clean clone (after `bun install` in `web/`) needs no separate
codegen step.

Per-package shortcuts (`format:api`, `lint:core`, `test:web`, etc.) exist in
[`mise.toml`](mise.toml) for tighter loops.

### Running locally

See the README's _Development quick start_ section. In short:

```bash
mise run dev:up     # PostgreSQL via compose.dependencies.yaml
mise run setup      # writes api/.env from api/.env.example; then set WW_TMDB_API_KEY
mise run dev:api    # cargo run -p stacks-api
mise run dev:web    # vite dev server (in another terminal)
```

## Git hooks (Lefthook)

After cloning, run `mise run install-hooks` once (equivalent to `lefthook
install`). [`lefthook.yml`](lefthook.yml) routes every command through a
mise task so the hooks, CI, and local invocations stay in lockstep.

- **pre-commit (piped):**
  1. `mise run format:{api,core,web}` — `cargo fmt` for the Rust crates, `biome
     format --write` for `web/`, all in parallel.
  2. `mise run lint-fix:{api,core,web}` — `cargo clippy --fix` and `biome lint
     --fix`, after formatting completes.
  Modified files are auto-staged via `stage_fixed: true`; you should not need
  to hand-format anything that hits a hook.
- **commit-msg:** `mise run commit-lint` enforces Conventional Commits (needs
  convco, from `mise install`).
- **pre-push (parallel):** every task `mise run check` runs:
  `format-check:*`, `lint:*`, `typecheck:*`, `test:*`, `contract`,
  `check:openapi`, `check:env-docs` and `check:file-length`. A failing push
  means something broke; do not bypass with `--no-verify` without good reason
  and a follow-up fix.

## Conventional commits

Every commit and every pull request title is a Conventional Commit, checked
by `mise run commit-lint`. Pull requests are squash-merged, so the title is the
commit that lands on `master` and the line `CHANGELOG.md` is generated from.
CI lints the title on the pull request and every commit pushed to `master`;
branch commits are checked locally by the `commit-msg` hook.
The allowed types and scopes, the subject style, breaking changes and the
release flow are in the "Commits & merging" section of
[`AGENTS.md`](AGENTS.md).

## Tests

- **Unit tests live next to the code** (`#[cfg(test)] mod tests` at the
  bottom of the same file). See `core/src/media/id.rs` or
  `core/src/patch.rs` for examples of the preferred shape.
- **DB-backed tests** (`api/tests/`, `core/tests/`) each create an
  ephemeral database on the PostgreSQL 18 server `DATABASE_URL` names (not
  `WW_DATABASE_URL`), migrate it with the real baseline and drop it
  afterwards. Without `DATABASE_URL` they skip with a note; under CI (`CI`
  set) a missing one fails the run.
- **Web tests** are Vitest + Testing Library under jsdom (`mise run test:web`,
  `web/vitest.config.ts`), next to the code they test (`*.test.ts(x)`).
  Mock the API at `fetch` with `web/src/test/utils.tsx`; never call it
  directly from app code.
- Coverage gate is **80% line coverage** on `api` and `core`, enforced in
  `mise run coverage` (and CI). It is measured with the DB-backed tests running,
  so set `DATABASE_URL` to a PostgreSQL 18 server. Add tests for new
  branches before opening a PR.
- **File length** is gated per package (`mise run check:file-length`): no
  tracked file under `api/`, `core/` or `web/` may exceed its package's
  threshold in `scripts/file-length-allowlist.txt`. The files already over
  it are listed there with their length; that list only shrinks, so split a
  file rather than adding it, and lower (or delete) its entry when it
  shrinks.
- **Mutation testing** (`mise run mutants`, cargo-mutants over `core`) runs
  weekly in `.github/workflows/mutants.yml` and blocks nothing; read its
  report for surviving mutants. A mutant is called equivalent only with a
  written argument.

## cameo dependency

`core` pulls in cameo 0.2 **from crates.io**, through the workspace
dependency in the root [`Cargo.toml`](Cargo.toml):

```toml
cameo = { version = "0.2", default-features = false, features = ["tmdb", "anilist"] }
```

Only the metadata adapter (`core/src/media/provider.rs` and `mapping.rs`)
names cameo's types; everything else goes through the `MetadataProvider`
trait (ADR 0010).

The `cache` feature is intentionally disabled because cameo's optional
sqlite-backed cache pulls in `rusqlite` -> `libsqlite3-sys` (`links =
"sqlite3"`), which collides with the copy of `libsqlite3-sys` `sqlx` already
links via SeaORM. The collision shows up as a cryptic link-time
duplicate-symbol error. Leave the feature off unless you also rework the
sqlite linkage; Stacks' catalog snapshot is the durable cache instead. See
the comment in the workspace [`Cargo.toml`](Cargo.toml).

`cargo update -p cameo` takes a new 0.2.x release; a new minor is a
version bump in the root `Cargo.toml`.

## Pull request checklist

Before requesting review:

- [ ] `mise run check` is clean (full gate: format-check + lint -D warnings +
      typecheck + test + contract + check-openapi + check-env-docs +
      check-file-length).
- [ ] `mise run coverage` stays at or above 80% lines on `api` and `core`.
- [ ] If the API surface changed, `mise run gen:openapi` was run and the
      `web/openapi.json` diff is committed and reviewed (`mise run check:openapi`
      fails on drift). The generated `web/src/api/` is `.gitignore`d;
      `mise run codegen` rebuilds it.
- [ ] New endpoints are `#[kynos::get("/path", operation_id = "camelCase")]`
      (or `post`, `put`, …) handlers with a one-line doc summary; DTOs derive
      `Schema` (`QueryParams` / `PathParams` for parameters), and each new
      problem slug has its heading in ADR 0006.
- [ ] User-facing changes (env vars, endpoints, migration steps) are
      reflected in the README and `docs/DEPLOY.md`. `CHANGELOG.md` is
      generated by release-please; never edit it by hand.
- [ ] The PR title and every commit pass `mise run commit-lint`.

PRs target `master`. CI mirrors the pre-push hooks; if pre-push passes
locally CI usually does too.
