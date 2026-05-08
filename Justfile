# Stacks — root task runner

# --- Aggregate ---

format: format-api format-cli format-core format-web

lint: lint-api lint-cli lint-core lint-web

lint-fix: lint-fix-api lint-fix-cli lint-fix-core lint-fix-web

test: test-api test-cli test-core test-web

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

# --- cli ---

format-cli:
    cargo fmt --manifest-path cli/Cargo.toml

lint-cli:
    cargo clippy --manifest-path cli/Cargo.toml -- -D warnings

lint-fix-cli:
    cargo clippy --fix --manifest-path cli/Cargo.toml --allow-dirty --allow-staged

test-cli:
    cargo test --manifest-path cli/Cargo.toml

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

# --- migrations ---

# Apply all pending migrations against $WW_DATABASE_URL.
migrate:
    cargo run --manifest-path core/migration/Cargo.toml -- up

# Drop all tables and re-apply (destructive — dev only).
migrate-fresh:
    cargo run --manifest-path core/migration/Cargo.toml -- fresh

# Roll back the last migration.
migrate-down:
    cargo run --manifest-path core/migration/Cargo.toml -- down

# --- run ---

# Run the API in dev mode (auto-reload requires `cargo install cargo-watch`).
dev-api:
    cargo run --manifest-path api/Cargo.toml --bin stacks-api

# Run the web frontend dev server.
dev-web:
    cd web && bun run dev

# --- codegen ---

# Generate OpenAPI spec, patch path parameters, and run orval to regenerate web/src/api/.
codegen:
    cargo run --manifest-path api/Cargo.toml --bin gen-openapi > web/openapi.json
    cd web && bun scripts/fix-openapi-params.ts openapi.json
    cd web && bun run codegen

# Generate OpenAPI spec only (outputs to stdout).
gen-openapi:
    cargo run --manifest-path api/Cargo.toml --bin gen-openapi
