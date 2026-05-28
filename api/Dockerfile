# syntax=docker/dockerfile:1.7
#
# Multi-stage build for the `stacks-api` binary.
# Final image: ~80 MB Debian slim with the static binary + TLS roots.
#
# Build: docker build -t stacks-api -f api/Dockerfile .
# (the build context is the workspace root, not the api/ directory)

ARG RUST_VERSION=1.88
ARG DEBIAN_VERSION=bookworm

# --- builder ---
FROM rust:${RUST_VERSION}-${DEBIAN_VERSION} AS builder
WORKDIR /build

# System packages needed to compile the workspace's native deps (ring, etc.).
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config \
 && rm -rf /var/lib/apt/lists/*

# Copy the whole workspace and build the release binary. We deliberately don't
# split into a deps-only "cargo chef" stage — keep the Dockerfile simple; CI
# layer caching plus `Swatinem/rust-cache` already amortise the rebuild cost.
COPY Cargo.toml Cargo.lock ./
COPY api ./api
COPY core ./core
RUN cargo build --release --bin stacks-api \
 && strip target/release/stacks-api

# --- runtime ---
FROM debian:${DEBIAN_VERSION}-slim AS runtime
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
    WW_DATA_DIR=/data \
    WW_LOG_LEVEL=info \
    RUST_BACKTRACE=1

EXPOSE 8095
VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/stacks-api"]
