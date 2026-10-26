# syntax=docker/dockerfile:1.7
#
# Multi-stage build for the `stacks-api` binary.
# Final image: Debian slim with the stripped binary, TLS roots and libssl3.
#
# Build: docker build -t stacks-api -f api/Dockerfile .
# (the build context is the workspace root, not the api/ directory)

# Base images are pinned to the digest of their multi-arch index (so the amd64
# and arm64 release builds each resolve their own platform), with the tag kept
# for readers and Dependabot, which refreshes the digests.

# --- builder ---
# The Rust tag matches rust-toolchain.toml; bump the two together.
FROM docker.io/library/rust:1.99.1-bookworm@sha256:48b7f435b8fc6378ce43b44f15e3a47fd8919b2825f835c39bd4bd3da50af5b8 AS builder
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
FROM docker.io/library/debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8 AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --create-home --uid 10001 stacks

USER stacks
WORKDIR /home/stacks

COPY --from=builder /build/target/release/stacks-api /usr/local/bin/stacks-api

ENV WW_BIND=0.0.0.0:8095 \
    WW_METRICS_BIND=0.0.0.0:9095 \
    RUST_BACKTRACE=1

# 8095: the API. 9095: the operator listener (/metrics), for Prometheus only.
EXPOSE 8095 9095

ENTRYPOINT ["/usr/local/bin/stacks-api"]
