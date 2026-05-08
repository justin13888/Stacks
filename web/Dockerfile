# syntax=docker/dockerfile:1.7
# ---------- Builder ----------
FROM oven/bun:1 AS builder

WORKDIR /build

COPY web/package.json web/bun.lock* web/bun.lockb* ./web/
WORKDIR /build/web

RUN bun install --frozen-lockfile || bun install

COPY web ./

# OpenAPI spec is provided externally — copy it in if present so the
# generated React Query hooks land in src/api/. Otherwise the build still
# works against placeholder pages.
RUN if [ -f openapi.json ]; then bun run codegen; fi

RUN bun run build

# ---------- Runner ----------
FROM nginx:1-alpine AS runtime

COPY --from=builder /build/web/dist /usr/share/nginx/html

# SPA fallback: route every unknown path to index.html.
RUN printf 'server {\n\
    listen       3000;\n\
    server_name  _;\n\
    root         /usr/share/nginx/html;\n\
    index        index.html;\n\
    location / {\n\
        try_files $uri $uri/ /index.html;\n\
    }\n\
}\n' > /etc/nginx/conf.d/default.conf \
    && rm -f /etc/nginx/conf.d/default.conf.bak

EXPOSE 3000

CMD ["nginx", "-g", "daemon off;"]
