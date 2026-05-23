# Stacks — root task runner

# --- Aggregate ---

format: format-api format-core format-web

lint: lint-api lint-core lint-web

lint-fix: lint-fix-api lint-fix-core lint-fix-web

test: test-api test-core test-web

coverage: coverage-api coverage-core

# --- api ---

format-api:
    cargo fmt --manifest-path api/Cargo.toml

lint-api:
    cargo clippy --manifest-path api/Cargo.toml -- -D warnings

lint-fix-api:
    cargo clippy --fix --manifest-path api/Cargo.toml --allow-dirty --allow-staged

test-api:
    cargo test --manifest-path api/Cargo.toml

coverage-api:
    cargo llvm-cov --manifest-path api/Cargo.toml --fail-under-lines 80

# --- core ---

format-core:
    cargo fmt --manifest-path core/Cargo.toml

lint-core:
    cargo clippy --manifest-path core/Cargo.toml -- -D warnings

lint-fix-core:
    cargo clippy --fix --manifest-path core/Cargo.toml --allow-dirty --allow-staged

test-core:
    cargo test --manifest-path core/Cargo.toml

coverage-core:
    cargo llvm-cov --manifest-path core/Cargo.toml --fail-under-lines 80

# --- web ---

format-web:
    cd web && bun run format

lint-web:
    cd web && bun run lint

lint-fix-web:
    cd web && bun run lint:fix

test-web:
    cd web && bun test

# --- codegen ---

# Generate OpenAPI spec and run orval to regenerate web/src/api/
codegen:
    cargo run --manifest-path api/Cargo.toml --bin gen-openapi > web/openapi.json
    cd web && bun run codegen

# Generate OpenAPI spec only (outputs to stdout)
gen-openapi:
    cargo run --manifest-path api/Cargo.toml --bin gen-openapi

# --- dev infrastructure ---

# Bring up local PostgreSQL for development.
dev-up:
    docker compose -f compose.dependencies.yaml up -d

# Stop and remove local development services (keeps volumes).
dev-down:
    docker compose -f compose.dependencies.yaml down

# Drop the local dev database (DESTROYS DATA).
dev-db-reset:
    docker compose -f compose.dependencies.yaml down -v
    docker compose -f compose.dependencies.yaml up -d

# Apply the dev-data seed (assumes a user has already signed in via OIDC).
dev-seed user_id:
    psql "${WW_DATABASE_URL}" -v user_id="{{user_id}}" -f tools/seed.sql

# --- security ---

audit:
    cargo deny check
    cargo audit || true
