# Podman Deployment Guide — WASM Geo Host

This guide describes how to deploy and operate **WASM Geo Host** using **Podman** in rootless container environments with full SELinux and user namespace support.

---

## 1. Why Podman for WASM Geo Host?

Podman is ideal for running sandboxed WASM hosts:
- **Rootless execution by default**: No root daemon required.
- **Enhanced security boundaries**: Sandboxed WASM components run within an already sandboxed unprivileged container process.
- **Native SELinux integration**: Label enforcement for host directory volumes.
- **Systemd integration (Quadlets)**: Native systemd service management.

---

## 2. Building the Image with Podman

Build the image directly using Podman:

```bash
podman build -t wasm-geo-host -f Deployments/Dockerfile .
```

---

## 3. Running with Podman (Rootless & SELinux)

### Single File Processing

When running rootless with volume mounts on Linux systems with SELinux (RHEL, Fedora, CentOS, Rocky Linux), append the `:Z` volume flag to relabel volumes:

```bash
podman run --rm \
  --userns=keep-id \
  -v "$(pwd)/Examples:/data/input:ro,Z" \
  -v "$(pwd)/data:/data:Z" \
  wasm-geo-host process /data/input/sample.jpg --config /app/config.yaml
```

**Flags Explained:**
- `--userns=keep-id`: Maps your current unprivileged host user UID directly to the container process UID, avoiding file permission mismatches with output databases.
- `:Z`: Configures private unshared SELinux label for mounted volumes.
- `:ro`: Mounts media directory as strictly read-only.

---

## 4. Podman Compose

If using `podman-compose`:

```yaml
version: '3.8'

services:
  geo-host:
    build:
      context: .
      dockerfile: Deployments/Dockerfile
    image: wasm-geo-host:latest
    userns_mode: "keep-id"
    volumes:
      - ./Examples:/data/input:ro,Z
      - ./data:/data:Z
    command: ["process", "/data/input/sample.jpg", "--config", "/app/config.yaml"]
```

Execute:

```bash
podman-compose up
```

---

## 5. Podman Quadlet / Systemd Service

To deploy WASM Geo Host as a managed background service under systemd on Linux:

Create `~/.config/containers/systemd/wasm-geo-host.container`:

```ini
[Unit]
Description=WASM Geo Host Processing Engine
After=network.target

[Container]
Image=wasm-geo-host:latest
Volume=%h/wasm_geo-host/Examples:/data/input:ro,Z
Volume=%h/wasm_geo-host/data:/data:Z
Exec=process /data/input/sample.jpg --config /app/config.yaml
UserNS=keep-id

[Service]
Restart=on-failure
TimeoutStartSec=60

[Install]
WantedBy=default.target
```

Reload and start the systemd service:

```bash
systemctl --user daemon-reload
systemctl --user start wasm-geo-host
systemctl --user status wasm-geo-host
```
