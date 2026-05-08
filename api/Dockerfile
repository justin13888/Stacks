# syntax=docker/dockerfile:1.7
# ---------- Builder ----------
FROM rust:1-bookworm AS builder

WORKDIR /build

# Bring in the workspace metadata first so cargo can resolve deps once and
# cache the heavy compilation step until source changes.
COPY Cargo.toml Cargo.lock ./
COPY api/Cargo.toml api/Cargo.toml
COPY cli/Cargo.toml cli/Cargo.toml
COPY core/Cargo.toml core/Cargo.toml
COPY core/migration/Cargo.toml core/migration/Cargo.toml
COPY vendor/cameo/Cargo.toml vendor/cameo/Cargo.toml
COPY vendor/cameo/build.rs vendor/cameo/build.rs

# Copy the rest of the source.
COPY api ./api
COPY cli ./cli
COPY core ./core
COPY vendor ./vendor

RUN cargo build --release --bin stacks-api

# ---------- Runner ----------
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 10001 --create-home --home-dir /app stacks
WORKDIR /app
USER stacks

COPY --from=builder /build/target/release/stacks-api /usr/local/bin/stacks-api

ENV WW_BIND=0.0.0.0:8095
EXPOSE 8095

ENTRYPOINT ["/usr/local/bin/stacks-api"]
