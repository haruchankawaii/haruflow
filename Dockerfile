FROM oven/bun:1.3.14-alpine AS web
WORKDIR /source/apps/web
COPY apps/web/package.json apps/web/bun.lock ./
RUN bun install --frozen-lockfile
COPY apps/web/ ./
RUN bun run build

FROM rust:1.98.1-slim-bookworm AS core
WORKDIR /source
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY migrations/ migrations/
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home haruflow
WORKDIR /app
COPY --from=core /source/target/release/haruflow /usr/local/bin/haruflow
COPY --from=web /source/apps/web/dist /app/web
ENV HARUFLOW_BIND=0.0.0.0:8080 HARUFLOW_STATIC_DIR=/app/web
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["haruflow"]
CMD ["serve"]
