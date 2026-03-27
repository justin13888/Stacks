# stacks

Self-hosted media tracking service.

## Tech Stack

- **API:** Rust — tracing, tracing-subscriber, thiserror, eyre, color-eyre, tokio (full)
- **CLI:** Rust — tracing, tracing-subscriber, thiserror, eyre, color-eyre
- **Web:** TypeScript / Bun + Biome (formatter + linter)
- **Task runner:** just (root Justfile)
- **Git hooks:** Lefthook

## Project Structure

```
stacks/
├── api/               # Rust backend API
│   ├── src/main.rs
│   └── Cargo.toml
├── cli/               # Rust CLI tool
│   ├── src/main.rs
│   └── Cargo.toml
├── web/               # TypeScript/Bun frontend
│   ├── index.ts
│   ├── index.test.ts
│   ├── biome.json
│   └── package.json
├── Cargo.toml         # Workspace (api + cli)
├── rust-toolchain.toml
├── Justfile
├── lefthook.yml
└── .github/workflows/ci.yml
```

## Development

### Setup

```bash
cd web && bun install && cd ..
```

### Run

```bash
# API
cd api && cargo run

# CLI
cd cli && cargo run

# Web
cd web && bun dev
```

### Test

```bash
just test          # all packages
just test-api      # api only
just test-cli      # cli only
just test-web      # web only
```

### Coverage

```bash
just coverage      # api + cli (80% threshold)
just coverage-api  # api only
just coverage-cli  # cli only
```

> Requires `cargo-llvm-cov`: `cargo install cargo-llvm-cov`

### Format

```bash
just format
```

### Lint

```bash
just lint
```

### Lint Fix

```bash
just lint-fix
```

## Conventions

- Use strict TypeScript — no `any` types
- Write tests for all new functionality
- Use conventional commits (type: description)
- Keep functions small and focused
- Rust: prefer `thiserror` for library errors, `eyre`/`color-eyre` for application errors
- Rust: initialize `color_eyre::install()` and `tracing_subscriber` in `main()`

## Architecture

Three-package monorepo: a Rust API server (`api/`), a Rust CLI tool (`cli/`), and a TypeScript/Bun web frontend (`web/`). The API and CLI share no code yet but live in a Cargo workspace for unified tooling. The web frontend communicates with the API over HTTP.
