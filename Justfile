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
# `just check` is the full pre-push gate (format-check + lint + typecheck + test
# + contract + check-openapi).

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

# Apply formatter fixes across api, core, the e2e crate, web and the browser
# suite.
format: format-api format-core format-e2e format-web format-browser

# Read-only formatter check.
format-check: format-check-api format-check-core format-check-e2e format-check-web format-check-browser

# Read-only lint check.
lint: lint-api lint-core lint-e2e lint-web lint-browser

# Apply lint autofixes.
lint-fix: lint-fix-api lint-fix-core lint-fix-e2e lint-fix-web lint-fix-browser

# TypeScript type checks (Rust crates are checked as part of `lint`).
typecheck: typecheck-web typecheck-browser

# Run the full test suite.
test: test-api test-core test-e2e test-web

# Coverage ratchet. The thresholds below are a floor set just under current
# measured coverage, so it cannot regress; raise them as tests are added. They
# are deliberately not 80% — the recipes previously claimed an 80% gate that
# nothing enforced, because CI only ever called the `coverage-lcov-*` recipes.
coverage: coverage-api coverage-core

# Emit lcov files (CI uploads these as artifacts).
coverage-lcov: coverage-lcov-api coverage-lcov-core

# Full pre-push gate. Mirrors what CI runs.
check: format-check lint typecheck test contract check-openapi check-env-docs

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

# --- e2e (tests/e2e: a spargen client of web/openapi.json driving a real server) ---

format-e2e:
    cargo fmt --manifest-path tests/e2e/Cargo.toml

format-check-e2e:
    cargo fmt --manifest-path tests/e2e/Cargo.toml --check

lint-e2e:
    cargo clippy --manifest-path tests/e2e/Cargo.toml --all-targets -- -D warnings

lint-fix-e2e:
    cargo clippy --fix --manifest-path tests/e2e/Cargo.toml --allow-dirty --allow-staged

test-e2e:
    cargo test --manifest-path tests/e2e/Cargo.toml

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

# Vitest + Testing Library under jsdom (web/vitest.config.ts); Bun only runs
# the script. `bun test` is Bun's own runner, which has no DOM.
test-web: codegen
    cd web && bun run test

build-web: codegen
    cd web && bun run build

# --- browser e2e (e2e/: Playwright against the composed stack, docs/QA.md) ---
#
# `just e2e-install` once, then `just e2e-up`, `just e2e`, `just e2e-down`.
# The stack is compose.yaml + compose.build.yaml + e2e/compose.e2e.yaml under
# its own project name, with the bundled Dex and a metadata fixture server.

e2e_compose := compose + " -p stacks-e2e -f compose.yaml -f compose.build.yaml -f e2e/compose.e2e.yaml --profile dev-idp"

format-browser:
    cd e2e && bun run format:fix

format-check-browser:
    cd e2e && bun run format

lint-browser:
    cd e2e && bun run lint

lint-fix-browser:
    cd e2e && bun run lint:fix

typecheck-browser:
    cd e2e && bun run typecheck

# Install the suite's packages and its Chromium. CI passes `--with-deps` to
# also install Chromium's system libraries.
e2e-install *FLAGS:
    cd e2e && bun install --frozen-lockfile
    cd e2e && bunx playwright install {{FLAGS}} chromium

# Start the e2e stack, building both images from this checkout. CI builds
# them first with its layer cache and passes `--no-build`.
e2e-up BUILD="--build":
    {{e2e_compose}} up -d {{BUILD}}

# Run the browser suite against the e2e stack (`just e2e-up` first). Extra
# arguments go to Playwright, e.g. `just e2e --project journey`.
e2e *ARGS:
    cd e2e && E2E_STOP_DATABASE="cd .. && {{e2e_compose}} stop postgres" E2E_START_DATABASE="cd .. && {{e2e_compose}} start postgres" bun run test {{ARGS}}

# Tail the e2e stack's logs (or one service's).
e2e-logs *SERVICE:
    {{e2e_compose}} logs {{SERVICE}}

# Stop the e2e stack and delete its volumes.
e2e-down:
    {{e2e_compose}} down -v

# Take one backup with the `backup` profile's sidecar and prove it restores
# row for row into a scratch database (deploy/backup/verify.sh). Runs against
# the e2e stack by default; `just backup-verify ""` targets the main stack.
backup-verify PROJECT="stacks-e2e":
    {{compose}} {{ if PROJECT == "" { "" } else { "-p " + PROJECT } }} -f compose.yaml --profile backup run --rm --no-deps backup once
    {{compose}} {{ if PROJECT == "" { "" } else { "-p " + PROJECT } }} -f compose.yaml --profile backup run --rm --no-deps --entrypoint bash backup /opt/stacks/verify.sh

# Fail when a variable the API or the compose files read is missing from the
# tables in README.md and docs/DEPLOY.md (scripts/check-env-docs).
check-env-docs:
    scripts/check-env-docs

# --- run / codegen ---

# Run the API server in dev mode. Needs env (see api/.env.example).
dev-api:
    cargo run -p stacks-api

# Start the web dev server (Vite HMR).
dev-web:
    cd web && bun run dev

# Generate what the web build needs from the committed OpenAPI document: the
# orval client and the TanStack route tree. Both are gitignored, so a clean
# clone runs this before `tsc` or the tests. Needs no Rust toolchain.
codegen:
    cd web && bun run codegen

# Rewrite the committed web/openapi.json from the router. The document is the
# router's own description (`openapi_as(V3_1)`), never edited by hand.
gen-openapi:
    cargo run --quiet --manifest-path api/Cargo.toml --bin gen-openapi > web/openapi.json

# Fail when web/openapi.json differs from what the router emits (CI runs this,
# so the committed contract can never drift from the server).
check-openapi: gen-openapi
    git diff --exit-code -- web/openapi.json

# Rewrite docs/backup-format.schema.json from the backup format's Rust types
# (core/src/import_export/backup/format.rs). `cargo test` fails when the
# committed schema is stale.
gen-backup-schema:
    cargo run --quiet --manifest-path core/Cargo.toml --example backup_schema > docs/backup-format.schema.json

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
# pull request title plus GitHub's " (#N)" suffix (the squash header that
# lands on master).
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
        compose.yaml compose.dependencies.yaml compose.stacks.yaml compose.backup.yaml \
        compose.dex.yaml deploy .env.example README.md LICENSE docs/DEPLOY.md \
        | gzip -n > "{{OUT_DIR}}/${bundle}.tar.gz"
    cd "{{OUT_DIR}}"
    sha256sum openapi.json "${bundle}.tar.gz" > SHA256SUMS
    cat SHA256SUMS

# Bring Cargo.lock, web/bun.lock and web/openapi.json (whose info.version is
# the crate version) in line with the manifests after a version bump (the
# release workflow runs this on the release-please branch).
sync-lockfiles:
    cargo update --workspace
    bun install --cwd web --lockfile-only
    just gen-openapi

# --- full stack ---

# Write api/.env from the example. Never overwrites an existing file.
# Fill in WW_TMDB_API_KEY afterwards.
setup:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f api/.env ]; then
        echo "api/.env already exists — leaving it alone."
        exit 0
    fi
    cp api/.env.example api/.env
    echo "Wrote api/.env."
    echo "Now set WW_TMDB_API_KEY in api/.env — it is the only value you must supply."

# Build and start the whole stack (Postgres + Dex + API + web) on :3000 from
# this checkout (compose.build.yaml over the released-image compose.yaml).
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
    exec {{compose}} -f compose.yaml -f compose.build.yaml --profile dev-idp --env-file api/.env up -d --build

# Stop the whole stack (keeps volumes).
down:
    {{compose}} -f compose.yaml -f compose.build.yaml --profile dev-idp down

# Tail logs from every service (or one: `just logs stacks-api`).
logs *SERVICE:
    {{compose}} -f compose.yaml -f compose.build.yaml --profile dev-idp logs -f {{SERVICE}}

# Stop the stack and DESTROY its volumes, including the database.
nuke:
    {{compose}} -f compose.yaml -f compose.build.yaml --profile dev-idp down -v

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

# --- security ---

# Run both supply-chain checks (cargo-deny + cargo-audit).
audit: audit-deny audit-vuln

# License / banned-dep / source check via deny.toml.
audit-deny:
    cargo deny check

# RustSec advisories — surfaces findings without failing the build (matches CI).
audit-vuln:
    cargo audit || true
