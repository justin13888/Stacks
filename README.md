# stacks

Self-hosted media tracking service. Single Postgres-backed Rust API, a
TypeScript/React web client, and a thin Rust CLI for poking at things.

## Prerequisites

- [Rust (rustup)](https://rustup.rs) — api and cli runtime
- [Bun](https://bun.sh) — web runtime
- [just](https://github.com/casey/just) — command runner
- [Lefthook](https://github.com/evilmartians/lefthook) — git hooks manager
- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) — code coverage tool
- [Docker](https://www.docker.com) — only if you use the compose stack

## Quick Start

```bash
# 1. Configuration — copy and fill in WW_DATABASE_URL, WW_TMDB_API_KEY,
#    WW_OIDC_*, and a 32-byte WW_SECRET_KEY (`openssl rand -hex 32`).
cp .env.example .env
$EDITOR .env

# 2. Bring up Postgres locally.
docker compose -f compose.dependencies.yaml up -d

# 3. Run migrations.
just migrate

# 4. Web deps.
cd web && bun install && cd ..

# 5. Run all three side-by-side (separate terminals):
just dev-api    # API on :8095, Scalar at /scalar
just dev-web    # web on :3000
cargo run --manifest-path cli/Cargo.toml -- health
```

## Development

| Command              | Description                          |
| -------------------- | ------------------------------------ |
| `just format`        | Format all packages                  |
| `just lint`          | Lint all packages                    |
| `just lint-fix`      | Lint and auto-fix all packages       |
| `just test`          | Run tests for all packages           |
| `just coverage`      | Run LLVM coverage for Rust packages  |
| `just test-api`      | Run API tests                        |
| `just test-cli`      | Run CLI tests                        |
| `just test-web`      | Run web tests                        |
| `just migrate`       | Apply pending migrations             |
| `just migrate-fresh` | Drop & re-apply all migrations (dev) |
| `just codegen`       | Regenerate web API client from spec  |
| `just gen-openapi`   | Print the OpenAPI spec to stdout     |

## Configuration

All runtime config comes from environment variables. See
[`.env.example`](.env.example) for the canonical list. The most important:

- `WW_DATABASE_URL` — Postgres DSN
- `WW_SECRET_KEY` — 32-byte random key (encrypts streaming-server API keys + signs JWTs)
- `WW_TMDB_API_KEY` — TheMovieDB v4 token
- `WW_OIDC_*` — OIDC issuer + client credentials (Zitadel, Authentik, Keycloak, …)

## Tech Stack

- **API:** Rust — Salvo + sea-orm + tokio + tracing
- **CLI:** Rust — clap + reqwest
- **Web:** TypeScript — React 19 + TanStack Router/Query + Tailwind, built with Vite/Bun
- **Media metadata:** [Cameo](https://github.com/justin13888/cameo) (TMDB unified facade)
  — currently vendored under `vendor/cameo` while we sit ahead of an
  upstream patch; see `vendor/cameo/STACKS_PATCH.md`.

## Git Hooks

This project uses Lefthook. Pre-commit hooks auto-fix formatting and linting on staged files. Pre-push hooks run format checks, lint checks, and tests across api / cli / core / web.

## CI/CD

GitHub Actions runs format checks, linting, tests, and coverage on pushes to `master` and pull requests.

## Code Coverage

This project uses [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) for LLVM-based coverage. CI publishes lcov reports for `api` and `core`; an 80% threshold is targeted but not currently gated until the test suite catches up.

Install locally:

```bash
cargo install cargo-llvm-cov
# or faster: cargo binstall cargo-llvm-cov
```

Run:

```bash
just coverage
```

## Manual QA

A step-by-step QA runbook, including end-to-end test scenarios, lives in
[`docs/QA.md`](docs/QA.md).

## License

AGPL-3.0 — see [LICENSE](LICENSE) for details.
