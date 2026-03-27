# Stacks — root task runner

# --- Aggregate ---

format: format-api format-cli format-web

lint: lint-api lint-cli lint-web

lint-fix: lint-fix-api lint-fix-cli lint-fix-web

test: test-api test-cli test-web

coverage: coverage-api coverage-cli

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

coverage-cli:
    cargo llvm-cov --manifest-path cli/Cargo.toml --fail-under-lines 80

# --- web ---

format-web:
    cd web && bun run format

lint-web:
    cd web && bun run lint

lint-fix-web:
    cd web && bun run lint:fix

test-web:
    cd web && bun test
