# stacks-api

Rust HTTP API for Stacks.

## Development

### OpenAPI / Scalar UI

In **debug builds** (`cargo run` / `cargo build`), two additional endpoints are available:

| Path | Description |
|------|-------------|
| `GET /api-docs/openapi.json` | OpenAPI 3.1 spec (JSON) |
| `GET /scalar` | Scalar interactive API reference UI |

These endpoints are compiled out in release builds (`--release`) and are never present in production.

### Generating the spec offline

To dump the OpenAPI spec without running the server:

```sh
cargo run --bin gen-openapi > openapi.json
```

## Configuration

Copy `.env.example` to `.env` and fill in the values before running.

## Running

```sh
cargo run
# or via just:
just dev-api
```
