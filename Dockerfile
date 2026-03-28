# Stage 1: Build frontend
FROM oven/bun:latest AS web-builder
WORKDIR /app/web
COPY apps/web/package.json apps/web/bun.lock* ./
RUN bun install --frozen-lockfile
COPY apps/web/ .
RUN bun run build

# Stage 2: Build Rust binaries
FROM rust:1-alpine AS rust-builder
RUN apk add --no-cache musl-dev curl
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY --from=web-builder /app/web/dist apps/web/dist
RUN cargo build --release -p serverbee-server -p serverbee-agent

# Stage 3: Runtime
FROM alpine:3.21
RUN apk add --no-cache ca-certificates
COPY --from=rust-builder /app/target/release/serverbee-server /usr/local/bin/
COPY --from=rust-builder /app/target/release/serverbee-agent /usr/local/bin/

VOLUME /data
ENV SERVERBEE_SERVER__DATA_DIR=/data
ENV SERVERBEE_SERVER__LISTEN=0.0.0.0:9527
EXPOSE 9527

CMD ["serverbee-server"]
