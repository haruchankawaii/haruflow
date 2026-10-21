FROM oven/bun:1.3.14-alpine AS web
WORKDIR /source/apps/web
COPY apps/web/package.json apps/web/bun.lock ./
RUN bun install --frozen-lockfile
COPY apps/web/ ./
RUN bun run build

FROM rust:1.98.1-slim-bookworm AS core
WORKDIR /source
ARG HARUFLOW_BUILD_COMMIT=unknown
ENV HARUFLOW_BUILD_COMMIT=${HARUFLOW_BUILD_COMMIT}
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY migrations/ migrations/
RUN cargo build --release --locked

FROM debian:bookworm-slim
ARG HARUFLOW_BUILD_COMMIT=unknown
LABEL org.opencontainers.image.source="https://github.com/haruchankawaii/haruflow" \
      org.opencontainers.image.revision=${HARUFLOW_BUILD_COMMIT}
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
