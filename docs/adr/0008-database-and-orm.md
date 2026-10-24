# ADR 0008: PostgreSQL 18 and SeaORM 2

- Status: accepted
- Date: 2026-10-24
- Issue: #12 (records the verdict taken in #5)

## Context

Stacks is the system of record for what someone has watched. The maintainer
wants to "start using it and never lose data", so the store has to enforce
its own invariants (status values, score range, one entry per title per
user), keep history append-only, survive upgrades through migrations that
run unattended at startup, and be backed up with ordinary tools.

The prototype ran on PostgreSQL through SeaORM 1, with eighteen migrations
grown during development, no CHECK constraints and hard cascading deletes.

## Decision

- **PostgreSQL 18** is the only supported database. The compose stack ships
  `postgres:18`; CI's Rust and coverage jobs test against the same image.
- **SeaORM 2.0** (`sqlx-postgres`, `runtime-tokio-rustls`) for entities and
  queries, with `sea-orm-migration` for the schema. Repositories stay thin:
  the SeaORM query and nothing else; transactions are opened by services so
  an entry change, its `entry_events` row, runs and plays commit together.
- **One baseline migration for v0.1.0**
  (`core/migration/src/m20261101_000001_baseline.rs`). Every enum column has
  a CHECK constraint, every timestamp is `timestamptz`, user content carries
  `created_at`/`updated_at`, plays and events are append-only (undo voids, it
  never deletes), and deletes of entries and lists are soft.
- **After v0.1.0 migrations are append-only**: a released migration is never
  edited, and each release's migrations run idempotently on API start
  (`Migrator::up`). `/readyz` reports 503 while any is pending.
- Hand-written SQL is used where SeaORM cannot say it, notably the job
  queue's `FOR UPDATE SKIP LOCKED` claim (ADR 0011) and the snapshot
  staleness query.

## Alternatives rejected

- **sqlx directly.** Compile-time checked SQL is attractive, but every query
  becomes hand-written SQL plus a `DATABASE_URL` (or offline data) at build
  time, and the entity model would have to be rebuilt by hand. SeaORM sits
  on sqlx anyway, so dropping to raw SQL where it pays is always available.
  The entity-first model is also what the sibling Beam service uses.
- **Diesel.** Mature and fast, but synchronous at its core (async through
  `diesel-async`), with a DSL that fights the dynamic filters the library and
  search listings build at run time, and its own migration runner.
- **SQLite.** Simpler to host, but a single writer, no `SKIP LOCKED`, weaker
  type enforcement, and `rusqlite`'s `libsqlite3-sys` collides with the copy
  sqlx links (the reason cameo's `cache` feature is off). PostgreSQL also
  gives `pg_dump`, which the backup sidecar and the restore check rely on.
- **Keep the eighteen development migrations.** They encoded the prototype's
  data-loss bugs and would have had to be replayed by every new install for
  no benefit; v0.1.0 has no existing installs to migrate.

## Consequences

- The schema test suite (`core/tests/schema.rs`) applies the baseline to an
  empty PostgreSQL 18 and asserts its constraints.
- Operators back up with `pg_dump` (or the `backup` compose profile) and can
  verify a dump restores with `just backup-verify` (docs/DEPLOY.md).
- Upgrading PostgreSQL across major versions is a dump and restore, as for
  any PostgreSQL deployment.
