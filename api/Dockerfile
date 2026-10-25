# syntax=docker/dockerfile:1.7
#
# Multi-stage build for the `stacks-api` binary.
# Final image: ~80 MB Debian slim with the static binary + TLS roots.
#
# Build: docker build -t stacks-api -f api/Dockerfile .
# (the build context is the workspace root, not the api/ directory)

# Matches rust-toolchain.toml; bump the two together.
ARG RUST_VERSION=1.99.1
ARG DEBIAN_VERSION=bookworm

# --- builder ---
FROM docker.io/library/rust:${RUST_VERSION}-${DEBIAN_VERSION} AS builder
WORKDIR /build

# System packages needed to compile the workspace's native deps (ring, etc.).
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config \
 && rm -rf /var/lib/apt/lists/*

# Copy the whole workspace and build the release binary. We deliberately don't
# split into a deps-only "cargo chef" stage — keep the Dockerfile simple; CI
# layer caching plus `Swatinem/rust-cache` already amortise the rebuild cost.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY api ./api
COPY core ./core
# A workspace member; only its manifest and sources are needed to resolve
# the workspace, and the stacks-api build never compiles it.
COPY tests/e2e ./tests/e2e
RUN cargo build --release --locked --bin stacks-api \
 && strip target/release/stacks-api

# --- runtime ---
FROM docker.io/library/debian:${DEBIAN_VERSION}-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --create-home --uid 10001 stacks \
 && mkdir -p /data \
 && chown stacks:stacks /data

USER stacks
WORKDIR /home/stacks

COPY --from=builder /build/target/release/stacks-api /usr/local/bin/stacks-api

ENV WW_BIND=0.0.0.0:8095 \
    WW_METRICS_BIND=0.0.0.0:9095 \
    WW_LOG_LEVEL=info \
    RUST_BACKTRACE=1

# 8095: the API. 9095: the operator listener (/metrics), for Prometheus only.
EXPOSE 8095 9095
VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/stacks-api"]
