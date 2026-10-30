# ADR 0014: mise runs the tasks and pins the tools

- Status: accepted
- Date: 2026-10-29

## Context

Every dev, hook, CI and release command lived in a `Justfile`, while the
tools those commands need came from several separate places. Contributors
installed Bun, Lefthook, convco and cargo-llvm-cov by hand, and CI got them
from `oven-sh/setup-bun` (on `latest`), `taiki-e/install-action` and
`extractions/setup-just`. Nothing tied the versions a contributor ran to the
versions CI ran.

## Decision

[`mise.toml`](../../mise.toml) replaces the `Justfile`. It is the single
source of truth for every command and the tools they need:

- **`[tools]`** pins exact versions of Bun, Lefthook, convco,
  cargo-llvm-cov, cargo-deny, cargo-audit and cargo-mutants. `mise install`
  sets up a contributor's machine. Rust stays with rustup and
  `rust-toolchain.toml`, which the Docker images and every crate's
  `rust-version` already follow.
- **`[tasks]`** carries every former recipe, with names namespaced by colon
  (`format:api`, `check:openapi`, `e2e:up`). Aggregates (`check`, `lint`,
  `test`, …) list their dependencies explicitly, not through `foo:*`
  wildcards. mise runs a task's dependencies in parallel.
- **CI installs mise with `jdx/mise-action`**. Each job installs only the
  tools it needs (`install_args`) and sets `MISE_TASK_RUN_AUTO_INSTALL=false`,
  so `mise run` never fetches the rest. Steps call `mise run <task>`, as the
  hooks in `lefthook.yml` do.
- **Bash lives in `scripts/`.** mise renders inline `run` strings as Tera
  templates, so bash syntax such as `${#x}` cannot appear there. A task
  whose body needs more than a short POSIX command calls a script, as the
  `check-*` gates already did. `scripts/compose` picks podman or docker for
  every compose task (`CONTAINER_ENGINE` overrides it), replacing just's
  `container_engine` variable; `scripts/up` holds the `up` task's Dex check.
- **`api/.env` is loaded by `scripts/with-dotenv`** in front of every task
  that runs the API, its tests or compose. As with just's `set
  dotenv-path`, a variable the shell already set wins over the file, so
  `DATABASE_URL=… mise run coverage` reaches the database it names. Its
  parser reads plain `KEY=value` lines (quotes, `export` and comments
  included) but, unlike dotenvy, does not expand `${VAR}`. Tasks that read
  no environment (format, lint, codegen, `dev:web`) skip it. mise's
  own `env._.file` is not used: it lets the file override the shell, and in
  a top-level `[env]` it would also export the file into any shell where
  mise is activated.

## Consequences

- Contributors need mise, and a clone's `mise.toml` must be trusted once
  (`mise trust`). Without mise the git hooks fail.
- Dependabot does not update `mise.toml`, so its pins are bumped by hand.
  The GitHub Actions ecosystem still covers `jdx/mise-action` itself.
- `scripts/check-compose` reads the `e2e:down` command from
  `mise run --dry-run e2e:down`, as it read `just --dry-run` before.
