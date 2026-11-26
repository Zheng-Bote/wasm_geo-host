# Multi-stage Containerfile/Dockerfile for WASM Geo Host

# Stage 1: Build Host and WASM Component Plugins
FROM rust:1.97-bookworm AS builder

WORKDIR /build

# Install WASI 0.2 component model target
RUN rustup target add wasm32-wasip2

# Copy source tree
COPY . .

# Build host engine
RUN cargo build --release --bin flagme-host

# Build all 11 plugins as Wasm components (media-plugin world, ADR 0003)
RUN cargo build --release --target wasm32-wasip2 --lib --features component \
    -p flagme-plugin-metadata -p flagme-plugin-reversegeo -p flagme-plugin-countryflag \
    -p flagme-plugin-thumbnail -p flagme-plugin-ai-tagging -p flagme-plugin-duplicate \
    -p flagme-plugin-timeline -p flagme-plugin-weather-enrich -p flagme-plugin-ocr-text \
    -p flagme-plugin-face-detection -p flagme-plugin-privacy-redact

# Stage 2: Minimal Production Runtime
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates sqlite3 curl && \
    rm -rf /var/lib/apt/lists/*

# Create application user and directories
RUN useradd -u 10001 -U -m appuser && \
    mkdir -p /app/plugins /app/cache /data && \
    chown -R appuser:appuser /app /data

WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/target/release/flagme-host /usr/local/bin/flagme-host

# Copy the plugin components (plugin_dir in config.docker.yaml)
COPY --from=builder /build/target/wasm32-wasip2/release/flagme_plugin_*.wasm /app/plugins/

# Container profile with plugin_dir: /app/plugins
COPY config.docker.yaml /app/config.yaml

USER appuser

ENV RUST_LOG=info
VOLUME ["/data"]

ENTRYPOINT ["flagme-host"]
CMD ["--help"]
