# ADR 0011: The job queue is a PostgreSQL table claimed with SKIP LOCKED

- Status: accepted
- Date: 2026-10-24
- Issue: #12 (records the verdict taken in #5)

## Context

Some work must not run inside a request: imports that resolve hundreds of
titles against rate-limited providers, notification delivery with retries,
and the hourly refresh of catalog snapshots. It has to survive restarts,
retry with backoff, and never run twice at once, on a deployment that is
usually one API process and one database.

## Decision

- **A `jobs` table** ([`core/src/entity/job.rs`](../../core/src/entity/job.rs))
  holds each job's type, JSON payload, status, attempts, `max_attempts` and
  `run_at`.
- **The runner** ([`core/src/jobs/runner.rs`](../../core/src/jobs/runner.rs))
  runs inside the API process. Every poll it claims a batch of due jobs with
  `SELECT … FOR UPDATE SKIP LOCKED`, so any number of runners never share a
  job; a failure is rescheduled with exponential backoff (30 s doubling,
  capped at an hour) until `max_attempts`, then marked failed. A job left
  `running` by a crashed process is reclaimed after a long lease. A deferred
  run that made progress returns without spending an attempt.
- Enqueueing is an insert in the caller's transaction, so a job exists if
  and only if the change that needed it committed.
- Admins see and retry jobs through `/api/v1/admin/jobs`.

## Alternatives rejected

- **An external broker (Redis + a queue library, RabbitMQ, NATS).** Another
  service to deploy, secure and back up, and a second source of truth that
  can disagree with the database: a job enqueued for a transaction that
  rolled back, or lost when the broker restarts. Nothing in Stacks needs the
  throughput.
- **A crate built on the same idea (apalis, sqlxmq, graphile-worker
  ports).** They would add their own schema and runtime conventions for a
  runner that is a few hundred lines and fully tested here.
- **`tokio::spawn` in the request.** Lost on restart, unbounded, and
  invisible to operators.

## Consequences

- `core/tests/jobs.rs` covers concurrent claimers, due-only claiming,
  reclaiming abandoned runs and admin retries against PostgreSQL.
- Running more than one API replica is safe for the queue; the in-memory rate
  limiter is the part that is per process (docs/DEPLOY.md).
