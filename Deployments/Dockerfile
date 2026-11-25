# Multi-stage Containerfile/Dockerfile for WASM Geo Host

# Stage 1: Build Host and WASM Component Plugins
FROM rust:1.80-bookworm AS builder

WORKDIR /build

# Install WASI 0.2 component model target
RUN rustup target add wasm32-wasip2

# Copy source tree
COPY . .

# Build host engine
RUN cargo build --release --bin flagme-host

# Build all 7 WASM plugins to wasm32-wasip2
RUN cargo build --release --package flagme-plugin-metadata --target wasm32-wasip2 && \
    cargo build --release --package flagme-plugin-reversegeo --target wasm32-wasip2 && \
    cargo build --release --package flagme-plugin-countryflag --target wasm32-wasip2 && \
    cargo build --release --package flagme-plugin-thumbnail --target wasm32-wasip2 && \
    cargo build --release --package flagme-plugin-ai-tagging --target wasm32-wasip2 && \
    cargo build --release --package flagme-plugin-duplicate --target wasm32-wasip2 && \
    cargo build --release --package flagme-plugin-timeline --target wasm32-wasip2

# Stage 2: Minimal Production Runtime
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates sqlite3 && \
    rm -rf /var/lib/apt/lists/*

# Create application user and directories
RUN useradd -u 10001 -U -m appuser && \
    mkdir -p /app/plugins /app/cache /data && \
    chown -R appuser:appuser /app /data

WORKDIR /app

# Copy binary from builder
COPY --from=builder /build/target/release/flagme-host /usr/local/bin/flagme-host

# Copy compiled wasip2 plugins
COPY --from=builder /build/target/wasm32-wasip2/release/*.wasm /app/plugins/

# Copy default configuration
COPY config.yaml /app/config.yaml

USER appuser

ENV RUST_LOG=info
VOLUME ["/data"]

ENTRYPOINT ["flagme-host"]
CMD ["--help"]
