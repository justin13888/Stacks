# ADR 0005: Logs, metrics and probes

- Status: accepted
- Date: 2026-10-23
- Issue: #6

## Context

Under Salvo, request metrics were labelled by the raw request path, so every
title reference a client asked for minted a new time series, and `/metrics`
was a public route in the same document as the API. Stacks is self-hosted:
an operator wants structured logs, a scrape target for Prometheus, and
probes a container runtime can use, without running a collector.

## Decision

### Logs

`tracing` with `tracing-subscriber`'s JSON formatter on stdout, filtered by
`WW_LOG_LEVEL` (an `EnvFilter` directive, default `info`). Two events log
each request, both carrying the matched operation. Stacks' `RequestLog`
observer writes `request received` when it arrives; observers run before
any interceptor, so no identifier exists yet and the event has no
`request_id`. kynos's `Trace` writes `response sent` when the response
leaves, carrying the `X-Request-Id` the `RequestId` interceptor assigned, so
a client-reported id finds the request's outcome.
Problem documents never carry internals: an `Internal` (500) logs its cause
with the call site and answers a fixed body.

### Metrics

The `metrics` facade with `metrics-exporter-prometheus` 0.18.

| Series | Labels | From |
| --- | --- | --- |
| `stacks_http_requests_total` | `operation`, `status` | the `Metrics` observer |
| `stacks_http_request_duration_seconds` | `operation`, `status` | the `Metrics` observer (time to the response head) |
| `stacks_metadata_request_duration_seconds` | `provider`, `operation`, `outcome` | the metadata adapter |
| `stacks_metadata_errors_total` | `provider`, `operation`, `class` | the metadata adapter |
| `stacks_metadata_circuit_state` | `provider` | the circuit breakers (0 closed, 1 half-open, 2 open) |

HTTP series are labelled by the operation's `operationId`, which a kynos
`Observer` receives with every response, and never by the request path: the
label set is the API's own vocabulary and is bounded by it. A request that
matched no operation is `operation="unmatched"`.

`GET /metrics` is served on a separate operator listener, `WW_METRICS_BIND`
(default `0.0.0.0:9095`; empty disables it), from its own router and its own
document tagged `internal`, so the public contract never carries it. When
`WW_METRICS_TOKEN` is set, a scrape must send it as a bearer token (compared
in constant time); unset, anyone who can reach the listener may scrape, which
suits a listener only Prometheus's network can reach.

### Probes

- `GET /healthz`: liveness. Touches no dependency, so a slow database never
  gets the process restarted.
- `GET /readyz`: readiness. 200 when PostgreSQL answers and its schema is
  current (no pending migration); 503 `database-unavailable` or
  `migrations-pending` otherwise. The metadata providers' circuit states are
  reported in the body and never make the service unready, because the
  library is served from the catalog snapshot while they are down.

## Alternatives rejected

- **OpenTelemetry export.** A collector is one more service for a
  self-hosted app with one process; the `tracing` events can be bridged to
  OpenTelemetry later without changing the call sites.
- **`/metrics` on the public listener, excluded from the document.** kynos
  describes every route it serves, by design; hiding one would need its
  `unchecked` escape hatches. A second listener is the supported shape, and
  keeps the scrape target off the internet-facing port.
- **Labelling by path template.** Bounded too, but the operation id is what
  the document, the generated clients and the logs already call each
  operation.

## Consequences

- A deployment that scrapes metrics publishes port 9095 to Prometheus only;
  the API image exposes it beside 8095.
- Dashboards and alerts key on operation ids, which are part of the public
  contract and change only with it.
