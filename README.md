# stacks

Self-hosted media tracking service.

## Prerequisites

- [Rust (rustup)](https://rustup.rs) — api and cli runtime
- [Bun](https://bun.sh) — web runtime
- [just](https://github.com/casey/just) — command runner
- [Lefthook](https://github.com/evilmartians/lefthook) — git hooks manager
- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) — code coverage tool

## Quick Start

```bash
# Install web dependencies
cd web && bun install && cd ..

# Run the API
cd api && cargo run

# Run the CLI
cd cli && cargo run

# Run the web frontend
cd web && bun dev
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

## Tech Stack

- **API:** Rust (tracing, thiserror, eyre, tokio)
- **CLI:** Rust (tracing, thiserror, eyre)
- **Web:** TypeScript / Bun + Biome

## Git Hooks

This project uses Lefthook. Pre-commit hooks auto-fix formatting and linting on staged files. Pre-push hooks run format checks, lint checks, tests, and coverage.

## CI/CD

GitHub Actions runs format checks, linting, tests, and coverage on pushes to `master` and pull requests.

## Code Coverage

This project uses [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov) for LLVM-based code coverage. CI enforces a minimum of 80% line coverage.

Install locally:

```bash
cargo install cargo-llvm-cov
# or faster: cargo binstall cargo-llvm-cov
```

Run:

```bash
just coverage
```

## License

AGPL-3.0 — see [LICENSE](LICENSE) for details.
