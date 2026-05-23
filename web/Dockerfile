# syntax=docker/dockerfile:1.7
#
# Multi-stage build for the React web app.
#
# Cross-language by design: stage 1 compiles the Rust `gen-openapi` binary
# (just one bin, not the full backend), stage 2 runs orval against its output
# and builds the Vite bundle, stage 3 serves the static assets with nginx.
#
# Build: docker build -t stacks-web -f web/Dockerfile .
# (the build context is the workspace root)

ARG RUST_VERSION=1.88
ARG BUN_VERSION=1
ARG NGINX_VERSION=1.27-alpine

# --- 1. Produce the OpenAPI spec from the Rust source ---
FROM rust:${RUST_VERSION}-bookworm AS spec
WORKDIR /build
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config \
 && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY api ./api
COPY core ./core
RUN cargo build --release --bin gen-openapi \
 && ./target/release/gen-openapi > /openapi.json

# --- 2. Bun build of the web app, against the freshly generated spec ---
FROM oven/bun:${BUN_VERSION} AS web
WORKDIR /build
COPY web/package.json web/bun.lock* ./
RUN bun install --frozen-lockfile

COPY web ./
COPY --from=spec /openapi.json ./openapi.json
RUN bun run codegen \
 && bun run build

# --- 3. Static-file server ---
FROM nginx:${NGINX_VERSION} AS runtime
COPY --from=web /build/dist /usr/share/nginx/html
COPY web/nginx.conf /etc/nginx/conf.d/default.conf
EXPOSE 3000
