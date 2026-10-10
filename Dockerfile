# syntax=docker/dockerfile:1

# One image: the Rust engine and the Next.js reading surface.
#
# The engine owns every /api route; Next proxies /api/* to it, so the browser
# only ever sees one origin and CORS never enters the picture.

# ---------------------------------------------------------------- rust build
FROM rust:1-bookworm AS engine-build
WORKDIR /src
RUN cargo init --name engine crates/engine 2>/dev/null || true
COPY Cargo.toml Cargo.toml
COPY crates/engine/Cargo.toml crates/engine/Cargo.toml
# A stub build first, so the dependency graph is cached across source edits.
RUN mkdir -p crates/engine/src && echo "fn main(){}" > crates/engine/src/main.rs
RUN cargo build --release --bin engine
COPY crates/engine/src crates/engine/src
RUN touch crates/engine/src/main.rs && cargo build --release --bin engine

# ---------------------------------------------------------------- node build
FROM node:22-bookworm AS web-build
WORKDIR /web
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
ENV NEXT_TELEMETRY_DISABLED=1
# The engine address is baked in at build time for the standalone server.
ENV ENGINE_URL=http://127.0.0.1:8787
RUN pnpm build

# ---------------------------------------------------------------- runtime
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app

COPY --from=engine-build /src/target/release/engine /app/engine
COPY --from=web-build /web/.next/standalone /app/web/
COPY --from=web-build /web/.next/static /app/web/.next/static
COPY --from=web-build /web/public /app/web/public

# The pinned corpus ships in the image: the demo must be reproducible.
COPY corpus /app/corpus

ENV VERBATIM_BIND=127.0.0.1:8787 \
    VERBATIM_DB=/app/var/verbatim.db \
    VERBATIM_CORPUS=/app/corpus \
    VERBATIM_RETRIEVAL=corpus \
    NEXT_TELEMETRY_DISABLED=1 \
    PORT=3000 \
    HOSTNAME=0.0.0.0

EXPOSE 3000
COPY docker-entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh
ENTRYPOINT ["/app/entrypoint.sh"]
