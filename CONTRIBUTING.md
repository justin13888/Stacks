# Contributing to stacks

Thanks for your interest. stacks is a small, opinionated codebase; this guide
captures the toolchain, conventions, and review expectations.

## Prerequisites

- **Rust >= 1.88** with the `stable` toolchain. [`rust-toolchain.toml`](rust-toolchain.toml)
  pins the channel and installs `rustfmt`, `clippy`, `rust-analyzer`, and
  `llvm-tools-preview` automatically when you run any `cargo` command from
  the workspace root.
- **Bun** (latest) for the web app — used as both runtime and package
  manager. See https://bun.sh.
- **just** for the workspace task runner. https://github.com/casey/just
- **Lefthook** for git hooks. https://github.com/evilmartians/lefthook
- **PostgreSQL >= 18** — the only data store. `compose.dependencies.yaml`
  brings one up on `localhost:5432`. Required for running API integration
  tests.
- **cargo-llvm-cov** for coverage (`just coverage`). Install with
  `cargo install cargo-llvm-cov` or `cargo binstall cargo-llvm-cov`.

Run `just install-hooks` once after cloning to register the lefthook hooks.

## Workspace layout

```
api/         stacks-api crate — Salvo HTTP server, OpenAPI generation
core/        stacks-core crate — domain library (no HTTP)
  migration/ SeaORM migrations against PostgreSQL
web/         Bun + Vite + React 19; orval-generated API client
Justfile     workspace tasks (format / lint / test / codegen)
lefthook.yml pre-commit (format-fix + lint-fix) and pre-push (full check)
```

`api` depends on `core`; `core` depends on `core/migration` only transitively
through the runtime migration step. The frontend never talks to `core`
directly — it generates a typed client from the OpenAPI spec that `api`
emits.

## Local dev loop

[`Justfile`](Justfile) is the single source of truth for every dev / CI /
git-hook command. CI (`.github/workflows/ci.yml`) and `lefthook.yml` both
call into the same recipes — don't inline `cargo` / `bun` invocations
elsewhere, add or extend a recipe instead. `just --list` shows everything.

```bash
just check    # full gate: format-check + lint + typecheck + test (same as CI / pre-push)
just format   # write cargo fmt + biome format fixes
just lint     # clippy -D warnings + biome lint (read-only)
just lint-fix # apply clippy --fix + biome lint --fix
just test     # cargo test (api, core) + bun test (web)
just coverage # cargo llvm-cov for api + core, 80% line minimum

# Iterating on the API surface:
just codegen  # dumps the OpenAPI spec to web/openapi.json and runs orval

# Spec only, to stdout:
just gen-openapi
```

The codegen step is the canonical way to regenerate `web/src/api/`. That
directory is `.gitignore`d and the web app re-imports it after every spec
change.

Per-package shortcuts (`format-api`, `lint-core`, `test-web`, etc.) exist in
the [Justfile](Justfile) for tighter loops.

### Running locally

See the README's _Quick start — development_ section. In short:

```bash
just dev-up           # PostgreSQL via compose.dependencies.yaml
cp api/.env.example api/.env  # then edit
just dev-api          # cargo run -p stacks-api
just dev-web          # vite dev server (in another terminal)
```

## Git hooks (Lefthook)

After cloning, run `just install-hooks` once (equivalent to `lefthook
install`). [`lefthook.yml`](lefthook.yml) routes every command through a
`just` recipe so the hooks, CI, and local invocations stay in lockstep.

- **pre-commit (piped):**
  1. `just format-{api,core,web}` — `cargo fmt` for the Rust crates, `biome
     format --write` for `web/`, all in parallel.
  2. `just lint-fix-{api,core,web}` — `cargo clippy --fix` and `biome lint
     --fix`, after formatting completes.
  Modified files are auto-staged via `stage_fixed: true`; you should not need
  to hand-format anything that hits a hook.
- **commit-msg:** `just commit-lint` enforces Conventional Commits on the
  subject line.
- **pre-push (parallel):** `just format-check-*`, `just lint-*`, `just
  typecheck-web`, `just test-*` — equivalent to `just check`. A failing push
  means something broke; do not bypass with `--no-verify` without good reason
  and a follow-up fix.

## Conventional commits

The project uses Conventional Commits. Recent examples from `git log
--oneline -20`:

```
feat(web): wire full UI to live API + Phase-5 pages, charts, helpers
feat(api): hardening — metrics, CORS allowlist, rate limit, secure headers
feat: jobs runner, stats, import/export & TOML config
chore(core): lint
docs: add openssl command for secret key generation
fix: use correct dotenvy::dotenv() call in api main
```

Common types: `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `chore`,
`build`, `ci`. Optional scope in parentheses (`feat(api):`, `chore(core):`).
Breaking changes use `!` (e.g. `feat(api)!: rename …`) and a `BREAKING
CHANGE:` footer.

## Tests

- **Unit tests live next to the code** (`#[cfg(test)] mod tests` at the
  bottom of the same file). See `core/src/media/id.rs` or
  `core/src/crypto.rs` for examples of the preferred shape.
- **API integration tests** that exercise handlers or repos require a
  running PostgreSQL. The CI matrix and local `just test` both assume one is
  reachable on the URL in `WW_DATABASE_URL`. Set `WW_DATABASE_URL` before
  running tests if your local DB lives somewhere else.
- **Web tests** are run with `bun test`. The directory currently has only
  smoke-level coverage (e.g. `web/src/app.test.ts`) — feel free to flesh it
  out as you build.
- Coverage gate is **80% line coverage** on `api` and `core`, enforced in
  `just coverage` (and CI). Add tests for new branches before opening a PR.

## cameo dependency

`core` and `api` pull in cameo as a **git dependency on `master`**:

```toml
cameo = { git = "https://github.com/justin13888/cameo", branch = "master", default-features = false, features = ["tmdb", "anilist"] }
```

The `cache` feature is intentionally disabled because cameo's optional
sqlite-backed cache pulls in `rusqlite` -> `libsqlite3-sys` (`links =
"sqlite3"`), which collides with the copy of `libsqlite3-sys` `sqlx` already
links via SeaORM. The collision shows up as a cryptic link-time
duplicate-symbol error. Leave the feature off unless you also rework the
sqlite linkage. See the comment in the workspace [`Cargo.toml`](Cargo.toml).

Because cameo is a git dep, `cargo update -p cameo` is how you take new
upstream changes. Pin to a specific revision in a PR if you depend on
unreleased behaviour.

## Pull request checklist

Before requesting review:

- [ ] `just check` is clean (full gate: format-check + lint -D warnings +
      typecheck + test).
- [ ] `just coverage` stays at or above 80% for any touched crate.
- [ ] If the API surface changed, `just codegen` was run and the resulting
      `web/openapi.json` diff is reviewed. The generated `web/src/api/` is
      `.gitignore`d; orval runs as part of codegen.
- [ ] New endpoints have OpenAPI annotations (`#[salvo::oapi::endpoint(...)]`
      with `tags(...)` and `summary = "..."`) and DTOs derive `ToSchema` /
      `ToParameters`.
- [ ] User-facing changes (env vars, endpoints, migration steps) are
      reflected in the README, `docs/DEPLOY.md`, and `CHANGELOG.md` under
      `## [Unreleased]`.
- [ ] Commits follow Conventional Commits.

PRs target `master`. CI mirrors the pre-push hooks; if pre-push passes
locally CI usually does too.
