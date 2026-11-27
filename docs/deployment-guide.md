# Deployment Guide — WASM Geo Host

## Architecture Overview

WASM Geo Host is deployed as a single, self-contained binary (`flagme-host`) accompanied by:
- Configuration file (`config.yaml`)
- Plugin components (`flagme_plugin_<name>.wasm`, see [Build Instructions](build-instructions.md)) in the directory named by `plugin_dir` in `config.yaml`
- SQLite database (`media.db`)

Runtime requirements on the host system: the `sqlite3` command-line tool (storage) and, for plugins with the `http` grant, the system certificate store (macOS keychain; on Linux the `ca-certificates` package). Outbound HTTP runs in-process with a Rust client ([ADR 0007](adr/0007-http-client-rust.md)); no external HTTP program is needed.

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
   - `plugins.<name>.resource_limits` gives one plugin its own `max_memory_bytes` and/or `timeout_ms` (higher or lower than the global values; missing fields inherit them). Give image-decoding plugins (`thumbnail`, `duplicate`, `privacy-redact`) more and metadata-only plugins less. Invalid values or unknown keys are logged as warnings at start and fall back to the global value; run with `RUST_LOG=flagme_host=debug` to see the effective limits per plugin.

5. **Model file of `ai-tagging`**:
   `ai-tagging` reads the CLIP model from `models/clip-vit-b32-vision-int8.onnx` (setting `model_path`, relative to the working directory; [ADR 0010](adr/0010-ai-tagging-clip.md)). The release packages contain it; from source run `make models`. The plugin checks its SHA-256; without a valid file it logs `CLIP model unavailable … (run \`make models\`)` and stores no tags or embeddings. The shipped configs give the plugin `max_memory_bytes: 536870912` and `timeout_ms: 30000`.

6. **Outbound HTTP (`http` capability)**:
   Requests run in-process (`ureq` with rustls, [ADR 0007](adr/0007-http-client-rust.md)) with fixed limits: **2 s** in total per request, at most **1 MiB** per response and at most **5 redirects** (http/https only). Exceeding a limit fails the request with an error that names it; the plugin then leaves its fields empty and warns. TLS certificates are always verified against the system certificate store (macOS keychain; Linux needs `ca-certificates`); there is no switch to disable verification, and an untrusted certificate fails the request. Proxies are taken from the environment of the host process, as with common command-line tools: `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` and `NO_PROXY` (requests through a proxy use an HTTP `CONNECT` tunnel). Every request sends `User-Agent: flagme-host/<version>`.

7. **Database Migration**:
   On first launch, `flagme-host` automatically executes embedded migrations (`migrations/20261125000000_init.sql`).

8. **Running the Pipeline**:
   ```bash
   flagme-host process /path/to/photo.jpg --config config.yaml
   ```
