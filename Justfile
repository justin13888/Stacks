# Stacks — workspace task runner.
#
# Single source of truth for every dev / CI / git-hook command. The CI
# workflow (.github/workflows/ci.yml) and lefthook.yml both call into these
# recipes; keep new commands here rather than inlining them elsewhere.
#
# Naming:
#   format-*       write formatter fixes (cargo fmt / biome format --write)
#   format-check-* read-only formatter check
#   lint-*         read-only lint (clippy -D warnings / biome lint)
#   lint-fix-*     apply lint autofixes
#   test-*         unit + integration tests
#   coverage-*     llvm-cov with a ratchet threshold (see below)
#   coverage-lcov-* llvm-cov emitting lcov.info for CI upload
#
# `just check` is the full pre-push gate (format-check + lint + typecheck + test).

# Load the API dev env (api/.env) into every recipe's environment. just runs
# recipes from the justfile dir (repo root), so the binary's own
# dotenvy::dotenv() — which only searches cwd and parents — never finds
# api/.env on its own. Missing file is a silent no-op (CI / hooks unaffected).
set dotenv-path := "api/.env"

# Container runtime. Podman is preferred (it is what this project is developed
# against); docker is used when podman is absent. Override with
# `just container_engine=docker up`.
#
# Note: some Homebrew podman builds shadow a working /usr/bin/podman with a
# broken one, so the system path is probed first.
container_engine := if path_exists("/usr/bin/podman") == "true" { "/usr/bin/podman" } else if `command -v podman 2>/dev/null || true` != "" { "podman" } else { "docker" }

compose := container_engine + " compose"

# Default target: list every recipe.
default:
    @just --list

# --- Aggregates ---

# Apply formatter fixes across api, core, web.
format: format-api format-core format-web

# Read-only formatter check.
format-check: format-check-api format-check-core format-check-web

# Read-only lint check.
lint: lint-api lint-core lint-web

# Apply lint autofixes.
lint-fix: lint-fix-api lint-fix-core lint-fix-web

# Web TypeScript type check (Rust crates are checked as part of `lint`).
typecheck: typecheck-web

# Run the full test suite.
test: test-api test-core test-web

# Coverage ratchet. The thresholds below are a floor set just under current
# measured coverage, so it cannot regress; raise them as tests are added. They
# are deliberately not 80% — the recipes previously claimed an 80% gate that
# nothing enforced, because CI only ever called the `coverage-lcov-*` recipes.
coverage: coverage-api coverage-core

# Emit lcov files (CI uploads these as artifacts).
coverage-lcov: coverage-lcov-api coverage-lcov-core

# Full pre-push gate. Mirrors what CI runs.
check: format-check lint typecheck test

# --- api ---

format-api:
    cargo fmt --manifest-path api/Cargo.toml

format-check-api:
    cargo fmt --manifest-path api/Cargo.toml --check

lint-api:
    cargo clippy --manifest-path api/Cargo.toml --all-targets -- -D warnings

lint-fix-api:
    cargo clippy --fix --manifest-path api/Cargo.toml --allow-dirty --allow-staged

test-api:
    cargo test --manifest-path api/Cargo.toml

coverage-api:
    cargo llvm-cov --manifest-path api/Cargo.toml --fail-under-lines 35

coverage-lcov-api:
    cargo llvm-cov --manifest-path api/Cargo.toml --lcov --output-path lcov-api.info

# --- core ---

format-core:
    cargo fmt --manifest-path core/Cargo.toml

format-check-core:
    cargo fmt --manifest-path core/Cargo.toml --check

lint-core:
    cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings

lint-fix-core:
    cargo clippy --fix --manifest-path core/Cargo.toml --allow-dirty --allow-staged

test-core:
    cargo test --manifest-path core/Cargo.toml

coverage-core:
    cargo llvm-cov --manifest-path core/Cargo.toml --fail-under-lines 14

coverage-lcov-core:
    cargo llvm-cov --manifest-path core/Cargo.toml --lcov --output-path lcov-core.info

# --- web ---

format-web:
    cd web && bun run format:fix

format-check-web:
    cd web && bun run format

lint-web:
    cd web && bun run lint

lint-fix-web:
    cd web && bun run lint:fix

typecheck-web: codegen
    cd web && bun run check

test-web: codegen
    cd web && bun test

build-web: codegen
    cd web && bun run build

# --- run / codegen ---

# Run the API server in dev mode. Needs env (see api/.env.example).
dev-api:
    cargo run -p stacks-api

# Start the web dev server (Vite HMR).
dev-web:
    cd web && bun run dev

# Dump the OpenAPI spec to web/openapi.json and regenerate the orval client.
codegen:
    cargo run --manifest-path api/Cargo.toml --bin gen-openapi > web/openapi.json
    cd web && bun run codegen

# Print the OpenAPI spec to stdout (CI redirects this to a file for validation).
gen-openapi:
    cargo run --manifest-path api/Cargo.toml --bin gen-openapi

# --- git hooks (invoked from lefthook.yml) ---

# Install lefthook's git hooks. Run once after cloning.
install-hooks:
    lefthook install

# Reject a commit message that doesn't follow Conventional Commits.
# Called by the commit-msg hook with the message file path.
commit-lint MSG_FILE:
    @grep -qE '^(feat|fix|chore|docs|refactor|perf|test|build|ci|style|revert)(\([^)]+\))?!?: .+' "{{MSG_FILE}}" \
        || { echo "Commit subject must follow Conventional Commits (e.g. 'feat(api): add X')" >&2; exit 1; }

# --- full stack ---

# Write api/.env from the example, generating a fresh WW_SECRET_KEY.
# Never overwrites an existing file. Fill in WW_TMDB_API_KEY afterwards.
setup:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f api/.env ]; then
        echo "api/.env already exists — leaving it alone."
        exit 0
    fi
    sed "s|^WW_SECRET_KEY=.*|WW_SECRET_KEY=$(openssl rand -hex 32)|" \
        api/.env.example > api/.env
    echo "Wrote api/.env with a generated WW_SECRET_KEY."
    echo "Now set WW_TMDB_API_KEY in api/.env — it is the only value you must supply."

# Build and start the whole stack (Postgres + Dex + API + web) on :3000.
up:
    #!/usr/bin/env bash
    set -euo pipefail
    # When api/.env points at the bundled Dex, the API must reach it by its
    # compose-network name while the browser keeps using localhost. Only applied
    # for the bundled Dex — a real provider is reachable at one address from
    # both sides and must not be rewritten.
    if [ "${WW_OIDC_ISSUER:-}" = "http://localhost:5556/dex" ] && [ -z "${WW_OIDC_INTERNAL_ISSUER:-}" ]; then
        export WW_OIDC_INTERNAL_ISSUER="http://dex:5556/dex"
    fi
    exec {{compose}} --profile dev-idp --env-file api/.env up -d --build

# Stop the whole stack (keeps volumes).
down:
    {{compose}} --profile dev-idp down

# Tail logs from every service (or one: `just logs stacks-api`).
logs *SERVICE:
    {{compose}} --profile dev-idp logs -f {{SERVICE}}

# Like `up`, plus the mock Beam server so streaming can be exercised locally.
up-beam:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "${WW_OIDC_ISSUER:-}" = "http://localhost:5556/dex" ] && [ -z "${WW_OIDC_INTERNAL_ISSUER:-}" ]; then
        export WW_OIDC_INTERNAL_ISSUER="http://dex:5556/dex"
    fi
    exec {{compose}} --profile dev-idp --profile dev-beam --env-file api/.env up -d --build

# Stop the stack and DESTROY its volumes, including the database.
nuke:
    {{compose}} --profile dev-idp down -v

# --- dev infrastructure ---

# Bring up local PostgreSQL for development.
dev-up:
    {{compose}} -f compose.dependencies.yaml up -d

# Stop and remove local development services (keeps volumes).
dev-down:
    {{compose}} -f compose.dependencies.yaml down

# Drop the local dev database (DESTROYS DATA).
dev-db-reset:
    {{compose}} -f compose.dependencies.yaml down -v
    {{compose}} -f compose.dependencies.yaml up -d

# Apply the dev-data seed (assumes a user has already signed in via OIDC).
dev-seed user_id:
    psql "${WW_DATABASE_URL}" -v user_id="{{user_id}}" -f tools/seed.sql

# Print a fresh 32-byte hex secret suitable for WW_SECRET_KEY.
gen-secret:
    @openssl rand -hex 32

# --- security ---

# Run both supply-chain checks (cargo-deny + cargo-audit).
audit: audit-deny audit-vuln

# License / banned-dep / source check via deny.toml.
audit-deny:
    cargo deny check

# RustSec advisories — surfaces findings without failing the build (matches CI).
audit-vuln:
    cargo audit || true
