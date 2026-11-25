# Deployment Guide — WASM Geo Host

## Architecture Overview

WASM Geo Host is deployed as a single, self-contained binary (`flagme-host`) accompanied by:
- Configuration file (`config.yaml`)
- Plugins directory (`plugins/*.wasm`)
- SQLite database (`media.db`)

## Production Deployment Steps

1. **Deploy Binary & Config**:
   Place `flagme-host` and `config.yaml` into your application directory (e.g., `/opt/wasm-geo-host/`).

2. **Configure Security Capabilities**:
   Edit `config.yaml` to specify exact capability grants per plugin. Follow the principle of least privilege:
   - Only plugins requiring filesystem reads receive the `filesystem` capability.
   - Only plugins requiring geocoding receive the `http` capability.

3. **Database Migration**:
   On first launch, `flagme-host` automatically executes embedded migrations (`migrations/20261125000000_init.sql`).

4. **Running the Pipeline**:
   ```bash
   flagme-host process /path/to/photo.jpg --config config.yaml
   ```
