# stacks-api

The Stacks HTTP API: a kynos router under `/api/v1`, plus the operator
listener that serves `/metrics`.

## The API reference

Every build serves the OpenAPI 3.1 document at `GET /api/openapi.json` and
a Scalar reference over it at `GET /api/docs`. The same document is
committed as [`web/openapi.json`](../web/openapi.json); `just gen-openapi`
rewrites it from the router without a database, and `just check-openapi`
fails when it is stale.

## Running

```sh
just setup     # writes api/.env from api/.env.example; then set WW_TMDB_API_KEY
just dev-up    # PostgreSQL
just dev-api   # cargo run -p stacks-api, reading api/.env
```

The configuration variables are listed in the root
[README](../README.md#configuration) and in
[`docs/DEPLOY.md`](../docs/DEPLOY.md#configuration). The API's design
is in [`AGENTS.md`](../AGENTS.md) and the decision records under
[`docs/adr/`](../docs/adr/).
