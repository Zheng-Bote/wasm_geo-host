# Docker Deployment Guide — WASM Geo Host

This guide describes how to build, configure, and operate **WASM Geo Host** in a production-grade Docker container.

---

## 1. Overview

The containerized architecture packages:
- The compiled `flagme-host` binary.
- All 11 plugins as WebAssembly components (`/app/plugins/flagme_plugin_*.wasm`, `wasm32-wasip2`), run in the embedded Wasmtime sandbox of the host ([ADR 0003](../docs/adr/0003-wasm-component-execution.md)).
- `config.docker.yaml` as `/app/config.yaml` (`plugin_dir: /app/plugins`, 512 MB / 15 s per plugin call).
- A minimal Debian Bookworm runtime with non-root security privileges (`appuser: 10001`).
- Mountable `/data` and `/app/cache` volumes for inputs, thumbnail caching, and SQLite persistence.

---

## 2. Building the Image

Build the container image using the multi-stage Dockerfile:

```bash
docker build -t wasm-geo-host -f Deployments/Dockerfile .
```

*Note: The builder stage installs the `wasm32-wasip2` target and compiles the host binary and all plugin components (`--lib --features component`).*

---

## 3. Running Containerized Processing

### Single File Processing

To process a sample image from your host directory (e.g. `./Examples/sample.jpg`):

```bash
docker run --rm \
  -v "$(pwd)/Examples:/data/input:ro" \
  -v "$(pwd)/data:/data" \
  wasm-geo-host process /data/input/sample.jpg --config /app/config.yaml
```

**Parameters Explained:**
- `-v "$(pwd)/Examples:/data/input:ro"`: Mounts host media directory into `/data/input` (read-only for security).
- `-v "$(pwd)/data:/data"`: Mounts host storage directory for `media.db` and thumbnail caches.
- `--rm`: Automatically cleans up container instance on exit.

---

## 4. Resource Constraints & Hardening

Run with explicit memory and CPU constraints matching your host capacity:

```bash
docker run --rm \
  --memory=512m \
  --cpus=2.0 \
  --pids-limit=100 \
  --security-opt=no-new-privileges:true \
  --cap-drop=ALL \
  -v "$(pwd)/Examples:/data/input:ro" \
  -v "$(pwd)/data:/data" \
  wasm-geo-host process /data/input/sample.jpg --config /app/config.yaml
```

---

## 5. Docker Compose Workflow

Create a `docker-compose.yaml` in your project root or deployment folder:

```yaml
version: '3.8'

services:
  geo-host:
    build:
      context: .
      dockerfile: Deployments/Dockerfile
    image: wasm-geo-host:latest
    container_name: wasm-geo-host
    user: "10001:10001"
    deploy:
      resources:
        limits:
          cpus: '2.0'
          memory: 512M
    volumes:
      - ./Examples:/data/input:ro
      - ./data:/data
    command: ["process", "/data/input/sample.jpg", "--config", "/app/config.yaml"]
```

Run via Compose:

```bash
docker compose up
```
