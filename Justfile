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
check: format-check lint typecheck test contract

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

# Regenerate everything the web build needs but does not commit: the OpenAPI
# spec, the orval client, and the TanStack route tree. All three are gitignored,
# so a clean clone must run this before `tsc` or `bun test` will work.
codegen:
    cargo run --manifest-path api/Cargo.toml --bin gen-openapi > web/openapi.json
    cd web && bun run codegen

# Print the OpenAPI spec to stdout (CI redirects this to a file for validation).
gen-openapi:
    cargo run --manifest-path api/Cargo.toml --bin gen-openapi

# --- OpenAPI contract harness (docs/adr/0002-openapi-31-kynos-orval.md) ---

# Write the kynos harness's OpenAPI 3.1 document (api/examples/contract.rs) to
# web/contract/openapi.json. `openapi_as(V3_1)` refuses rather than downgrades,
# so anything that needs 3.2 fails here.
contract-spec:
    cargo run --quiet --manifest-path api/Cargo.toml --example contract -- spec > web/contract/openapi.json

# The whole contract check: 3.1 export, orval 8 client, strict tsc over the
# client and its type assertions, then the Vitest round trip against the
# served harness. Needs `bun install --frozen-lockfile` in web/contract first.
contract: contract-spec
    cd web/contract && bun run generate
    cd web/contract && bun run typecheck
    cd web/contract && bun run test

# --- git hooks (invoked from lefthook.yml) ---

# Install lefthook's git hooks. Run once after cloning.
install-hooks:
    lefthook install

# --- commit messages (rule: .versionrc + AGENTS.md "Commits & merging") ---

# Reject a commit message that isn't a valid Conventional Commit: convco checks
# type, scope, and body/footer structure against .versionrc; the header must
# also be at most 72 characters with no trailing period. Called by the
# commit-msg hook with the message file, and by CI's pr-title job with the
# pull request title (the squash commit that lands on master).
commit-lint MSG_FILE:
    #!/usr/bin/env bash
    set -euo pipefail
    convco check --from-stdin --strip < "{{MSG_FILE}}"
    header="$(git stripspace --strip-comments < "{{MSG_FILE}}" | head -n 1)"
    # convco matches the type case-insensitively; the convention is lower-case.
    if ! printf '%s\n' "$header" | grep -qE '^[a-z]+(\([a-z]+\))?!?: '; then
        echo "Commit type and scope must be lower-case: ${header}" >&2
        exit 1
    fi
    if [ "${#header}" -gt 72 ]; then
        echo "Commit header is ${#header} characters; the limit is 72: ${header}" >&2
        exit 1
    fi
    if [ "${header%.}" != "${header}" ]; then
        echo "Commit header must not end with a period: ${header}" >&2
        exit 1
    fi

# Check every non-merge commit in BASE..HEAD with the same rule as commit-lint.
commit-lint-range BASE HEAD:
    #!/usr/bin/env bash
    set -euo pipefail
    status=0
    msg="$(mktemp)"
    trap 'rm -f "$msg"' EXIT
    for sha in $(git rev-list --no-merges --reverse "{{BASE}}..{{HEAD}}"); do
        git log -1 --format=%B "$sha" > "$msg"
        if ! just commit-lint "$msg"; then
            echo "  in commit $(git log -1 --format='%h %s' "$sha")" >&2
            status=1
        fi
    done
    exit "$status"

# Self-test for commit-lint: messages it must accept and reject.
commit-lint-test:
    #!/usr/bin/env bash
    set -uo pipefail
    msg="$(mktemp)"
    trap 'rm -f "$msg"' EXIT
    failed=0
    expect() {
        printf '%b\n' "$2" > "$msg"
        if just commit-lint "$msg" > /dev/null 2>&1; then got=accept; else got=reject; fi
        if [ "$got" != "$1" ]; then
            echo "FAIL: expected $1, got $got: $2" >&2
            failed=1
        fi
    }
    expect accept 'feat(api)!: drop salvo'
    expect accept 'fix: handle null'
    expect accept 'ci: add release workflow'
    expect accept 'revert: feat(web): add theme toggle'
    expect accept 'docs(docs): describe releases\n\nLonger body.\n\nRefs: #2'
    expect accept 'feat(core): rename field\n\nBREAKING CHANGE: the old field is gone'
    expect reject 'update stuff'
    expect reject 'feat(foo): x'
    expect reject 'Feat: x'
    expect reject 'wip: x'
    expect reject 'feat(API): x'
    expect reject 'feat: x.'
    expect reject "feat: $(printf 'a%.0s' $(seq 1 70))"
    if [ "$failed" -ne 0 ]; then exit 1; fi
    echo "commit-lint: all cases behave as expected"

# --- release (invoked from .github/workflows/release.yml) ---

# Build the release assets into OUT_DIR: openapi.json, the compose bundle
# stacks-compose-VERSION.tar.gz, and SHA256SUMS over both. VERSION is the tag,
# e.g. `just release-assets v0.1.0 target/release-assets`.
release-assets VERSION OUT_DIR:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{OUT_DIR}}"
    cargo run --quiet --manifest-path api/Cargo.toml --bin gen-openapi > "{{OUT_DIR}}/openapi.json"
    bundle="stacks-compose-{{VERSION}}"
    # Fixed ordering, owner and mtime so the tarball is reproducible.
    tar --create --sort=name --owner=0 --group=0 --numeric-owner \
        --mtime="@$(git log -1 --format=%ct)" --transform "s|^|${bundle}/|" \
        compose*.yaml deploy .env.example README.md LICENSE docs/DEPLOY.md \
        | gzip -n > "{{OUT_DIR}}/${bundle}.tar.gz"
    cd "{{OUT_DIR}}"
    sha256sum openapi.json "${bundle}.tar.gz" > SHA256SUMS
    cat SHA256SUMS

# Bring Cargo.lock and web/bun.lock in line with the manifests after a version
# bump (the release workflow runs this on the release-please branch).
sync-lockfiles:
    cargo update --workspace
    bun install --cwd web --lockfile-only

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
