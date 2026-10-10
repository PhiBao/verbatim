# syntax=docker/dockerfile:1

# One image: the Rust engine and the Next.js reading surface.
#
# The engine owns every /api route; Next proxies /api/* to it, so the browser
# only ever sees one origin and CORS never enters the picture.

# ---------------------------------------------------------------- rust build
FROM rust:1-bookworm AS engine-build
WORKDIR /src

# Workspace manifests first, so the dependency graph is cached across source
# edits. Both members are needed: a workspace member that is missing fails to
# load, even when only one binary is being built.
COPY Cargo.toml ./
COPY crates/engine/Cargo.toml crates/engine/
COPY crates/cli/Cargo.toml crates/cli/

RUN mkdir -p crates/engine/src crates/cli/src
RUN echo "fn main(){}" > crates/engine/src/main.rs && echo "" > crates/engine/src/lib.rs
RUN echo "fn main(){}" > crates/cli/src/main.rs

RUN cargo build --release --bin engine

# Now the real sources. Touching main.rs forces the final binary to relink
# against them rather than reusing the stub.
COPY crates/engine/src crates/engine/src
COPY crates/cli/src crates/cli/src
RUN touch crates/engine/src/main.rs crates/engine/src/lib.rs
RUN cargo build --release --bin engine

# ---------------------------------------------------------------- node build
FROM node:22-bookworm AS web-build
WORKDIR /web
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
ENV NEXT_TELEMETRY_DISABLED=1
# The engine address is baked in for the standalone server, and both processes
# share one container, so it is always loopback.
ENV ENGINE_URL=http://127.0.0.1:8787
RUN pnpm build

# ---------------------------------------------------------------- runtime
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# The web server is a Node process, and the runtime image is slim. Copying the
# binary from the build stage is smaller and more reproducible than adding a repo.
COPY --from=node:22-bookworm /usr/local/bin/node /usr/local/bin/node
WORKDIR /app

COPY --from=engine-build /src/target/release/engine /app/engine
COPY --from=web-build /web/.next/standalone /app/web/
COPY --from=web-build /web/.next/static /app/web/.next/static
COPY --from=web-build /web/public /app/web/public

# The pinned corpus ships in the image: the deployed demo must be reproducible
# rather than dependent on the live web.
COPY corpus /app/corpus

ENV LITELLM_BASE_URL=https://litellm.vision.aivar.app/v1 \
    VERBATIM_BIND=127.0.0.1:8787 \
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
