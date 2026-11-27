# Deployment Guide — WASM Geo Host

## Architecture Overview

WASM Geo Host is deployed as a single, self-contained binary (`flagme-host`) accompanied by:
- Configuration file (`config.yaml`)
- Plugin components (`flagme_plugin_<name>.wasm`, see [Build Instructions](build-instructions.md)) in the directory named by `plugin_dir` in `config.yaml`
- SQLite database (`media.db`)

## Production Deployment Steps

1. **Deploy Binary & Config**:
   Place `flagme-host` and `config.yaml` into your application directory (e.g., `/opt/wasm-geo-host/`).

2. **Deploy the Plugin Components**:
   Copy the `flagme_plugin_*.wasm` components into a directory and set `plugin_dir` in `config.yaml` to it (or `path` per plugin). The host refuses to start if an enabled plugin's component is missing; there is no native fallback ([ADR 0003](adr/0003-wasm-component-execution.md)).

3. **Component Cache**:
   Compiled components are cached in `component_cache_dir` (default `cache/components`, relative to the working directory; [ADR 0004](adr/0004-component-cache.md)). Cached entries are loaded as native code into the host, so the directory is a trust boundary: the host creates it with mode `0700` and uses it only if it is a real directory (no symbolic link) owned by the host user and not writable by group or others. Otherwise it logs `component cache disabled: '<dir>': <reason>` and compiles on every start. Point `component_cache_dir` at a directory owned by the service user (e.g. `/var/cache/wasm-geo-host/components`), or set `component_cache: false`. Deleting the directory clears the cache; entries of a changed component or another wasmtime version are never reused.

4. **Configure Security Capabilities and Limits**:
   Edit `config.yaml` to specify exact capability grants per plugin. Follow the principle of least privilege:
   - Only plugins requiring filesystem reads receive the `filesystem` capability.
   - Only plugins requiring geocoding receive the `http` capability.
   - `resource_limits.max_memory_bytes` and `timeout_ms` are enforced for every plugin call.

5. **Database Migration**:
   On first launch, `flagme-host` automatically executes embedded migrations (`migrations/20261125000000_init.sql`).

6. **Running the Pipeline**:
   ```bash
   flagme-host process /path/to/photo.jpg --config config.yaml
   ```
