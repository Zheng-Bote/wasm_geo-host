# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-11-27

### Breaking
- **`reversegeo` results change** (ADR 0006/0008): offline lookups now return the nearest GeoNames place with its own region and country instead of one of 35 reference cities or a country capital, and region names are shorter. Photos already stored keep their old names until they are processed again; the watcher's target folders (`<Country>/<City>/<Year>`) follow the new names for new photos.
- **`reversegeo` `city_radius_km`** now means the maximum distance to the nearest place: default **25** (was 200), range 1–200 (was 1–2000). Explicit values above 200 are clamped with a warning.
- **`http_get` error texts** no longer contain `curl exit <n>`; `curl` is no longer called. Proxies are used through an HTTP `CONNECT` tunnel, also for `http://` URLs. On Linux, HTTPS needs the `ca-certificates` package.

### Changed
- **Shorter region names in `reversegeo`** ([ADR 0008](docs/adr/0008-short-region-names.md), #67): `region` and `local_region` use GeoNames' short form when it occurs as a whole word sequence in the previous name, e.g. `State of Salzburg` → `Salzburg`, `Canton of Geneva` → `Geneva`, `Free and Hanseatic City of Hamburg` → `Hamburg`, `Land Berlin` → `Berlin`. 871 English and 333 local region names change; names without such a short form stay (`State of Berlin`). Counties are unchanged. `places.bin` was rebuilt from the same dumps (2026-11-27).
- **`http` capability without `curl`** ([ADR 0007](docs/adr/0007-http-client-rust.md), #66): outbound requests run in-process with the Rust client `ureq` (rustls, system certificate store) instead of spawning `curl`. Fixed limits: 2 s per request, at most 1 MiB per response (never read beyond), at most 5 redirects. Proxies come from `HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY`/`NO_PROXY`. Error messages keep the form `http_get <url>: <cause>` but no longer contain `curl exit <n>` (e.g. `HTTP status 404`, `timed out after 2 s`, `response larger than 1048576 bytes`). `curl` is no longer a runtime requirement; on Linux the `ca-certificates` package is.
- **`reversegeo` offline lookup uses an embedded GeoNames gazetteer** ([ADR 0006](docs/adr/0006-geonames-gazetteer-names.md), #64): 162,380 populated places from [GeoNames](https://www.geonames.org/) `cities1000` (CC BY 4.0, data of 2026-11-27) with English and local names for place, county, region and country. Photos now resolve offline to the nearest town (e.g. Freising, Moosburg, Garmisch-Partenkirchen) in its own country. `city_radius_km` is now the maximum distance to the nearest place (default **25**, range 1–200; previously a radius around 35 reference cities, default 200). `make geonames` rebuilds `plugins/reversegeo/data/places.bin` with the new `tools/geonames-import`.

### Fixed
- **`reversegeo` no longer invents places** (#62, #63): the country boxes (capital as city for any point in the box) and the nearest-of-35-reference-cities rule (wrong country across borders, e.g. Prague → Dresden, Germany) are removed.

### Added
- **Per-plugin resource limits**: `plugins.<name>.resource_limits` (`max_memory_bytes`, `timeout_ms`) replaces the global `resource_limits` for one plugin, higher or lower; missing fields inherit the global values. Invalid values (0, not an integer) and unknown keys are logged as warnings at start and fall back to the global value. The limits also apply when the host queries `name`/`version` at load time.
- **Component cache** ([ADR 0004](docs/adr/0004-component-cache.md)): compiled plugin components are kept in wasmtime's cache, so later starts skip compilation (measured with 8 plugins: `flagme-host process` 0.8 s → 0.2 s from the second start on). New `config.yaml` keys `component_cache` (default `true`) and `component_cache_dir` (default `cache/components`). The host creates the directory with mode `0700` and uses it only if it is owned by the host user, not writable by group or others and not a symbolic link; otherwise it warns and compiles as before.

## [0.2.0] - 2026-11-26

### Breaking
- Plugins run **only** as Wasm components ([ADR 0003](docs/adr/0003-wasm-component-execution.md)): build them with `make components` (or `cargo build --release --target wasm32-wasip2 --lib --features component -p …`) and point `plugin_dir` in `config.yaml` at them. The host refuses to start if an enabled plugin's component is missing; there is no native fallback.
- `wit/media-plugin.wit` is `wasmgeohost:media@0.2.0` (typed `document`, `plugin-error`, `fs-write`, `process(doc, settings)`); components built for 0.1.x do not load.
- `flagme-host stream` no longer spawns the `wasmtime` CLI; it runs on the embedded runtime like `process`.
- Rust 1.97+ is required (Wasmtime 51).
- Plugins no longer invent data ([ADR 0002](docs/adr/0002-plugins-without-engine-no-output.md)): fields that cannot be determined stay empty (`None`) with a warning. `ai-tagging`, `ocr-text` and `face-detection` have no built-in engine and are disabled by default.

### Added
- **Release build**: `Makefile` with `make release` (sdd gates, tests, host, plugin components, WASI and native plugin CLIs, packages with SHA-256 checksums in `dist/packages`).
- **New plugins**: `weather-enrich` (historical weather from the Open-Meteo archive), `ocr-text`, `face-detection` (engine interfaces, experimental) and `privacy-redact` (face redaction, EXIF serial stripping, GPS fuzzing).
- **Search**: SQLite FTS5 full-text search (`flagme-host search`) and cosine similarity over stored embeddings (`flagme-host similar`).
- **Directory watcher** (`flagme-host watch`): event-driven ingestion with worker pool, duplicate skipping and target organization by capture time and location.
- **Embedded web UI** (`flagme-host serve`): dashboard, map, gallery, REST and GeoJSON API.
- **Plugin settings (#26)**: all plugins read and honour their `settings` from `config.yaml` via the shared SDK module `flagme_sdk::settings` (typed settings, shared parsing rules, warnings for invalid, unsupported and unknown keys). Parameters per plugin are documented in each plugin README §2.2.
- `duplicate`: Hamming threshold `phash_threshold`; `timeline`: `timezone` (IANA via `chrono-tz`) and `source_priority`; `ai-tagging` / `face-detection` / `ocr-text`: model, tier and engine selected per call (superseded by ADR 0002, see *Fixed*).

### Changed
- `privacy-redact`: booleans follow the shared rule — `yes`/`on`/`1` now enable a feature (previously only `true`).
- Host YAML reader keeps `#` inside quoted values.
- Profiles `config.docker.yaml` / `config.mac-studio.yaml`: removed keys without implementation (`device`, `ocr-text.engine`, `max_image_dimension`).

### Fixed
- **No fabricated HTTP responses (#52, [ADR 0002](docs/adr/0002-plugins-without-engine-no-output.md))**: the host capability `http_get` now returns an error naming the cause when a request fails (`curl` missing, unreachable server, timeout, HTTP 4xx/5xx via `curl --fail`, empty or non-UTF-8 response) instead of `Ok` with an invented `{"status": "offline"}` body. `reversegeo` no longer invents a location (`Unknown Country` / `XX` / `Global`) for unresolved coordinates: the name fields stay empty, the coordinates are kept, a warning names the cause and nothing is cached. `weather-enrich` keeps `doc.weather` empty and caches nothing on a failed request.
- **Plugins run as Wasm components (#30, [ADR 0003](docs/adr/0003-wasm-component-execution.md))**: `flagme-host` embeds Wasmtime 51 (Component Model, WASI 0.2) and loads every enabled plugin from its component (`plugins.<name>.path`, else `<plugin_dir>/flagme_plugin_<name>.wasm`; new key `plugin_dir`, default `target/wasm32-wasip2/release`). Previously all plugins were linked natively and the sandbox did not exist at runtime.
  - Every plugin call runs in a fresh store without ambient authority (no preopened directories, environment, stdin or sockets); file, HTTP, KV and secret access goes only through `host-capabilities`, checked per plugin against `config.yaml`.
  - `resource_limits.max_memory_bytes` and `timeout_ms` are enforced (memory limiter, epoch interruption); an exceeded limit fails the call with the limit named, and the next document runs normally.
  - `wit/media-plugin.wit` is now `wasmgeohost:media@0.2.0`: complete typed `document` (all fields of `UnifiedDocument`, `custom` as JSON), `plugin-error`, `fs-write`, and `process(doc, settings)`.
  - SDK: `flagme_sdk::export_plugin!` exports a `Plugin` as component (active with the crate feature `component`); build with `cargo build --release --target wasm32-wasip2 --lib --features component -p …`. The standalone CLI binaries stay plain WASI commands.
  - No native mode and no fallback: the host no longer depends on the plugin crates; a missing component stops the host with the file name. `flagme-host stream` runs on the embedded runtime (the subprocess supervisor and the external `wasmtime` CLI are gone) and applies capabilities, `enabled` and `settings`.
  - `settings[…]` warnings appear once per plugin and message although each document gets a fresh instance.
  - Docker image builds the components and uses `config.docker.yaml` (`plugin_dir: /app/plugins`); Rust 1.97+ is required.
- **Unique face rows in SQLite (#24)**: faces are stored under the row key `<photo_id>/<face.id>`, so photos no longer overwrite each other's faces; a photo's old face rows are deleted before re-inserting (no duplicates on reprocessing), and `persons.representative_face_id` always points at an existing face of that person.
- **No invented capture time (#44)**: `timeline` no longer sets `taken_at` to the import time when no source matches; `taken_at` stays `None`, `resolution_source` is `none` and a warning is logged (`imported_at` still holds the import time). The watcher puts such photos into `Unknown_Year/00/…` instead of the import month, `weather-enrich` sends no request for them, and the web UI shows `unknown`.
- **Capability-only file access (#25)**: new host capability `fs_modified` (SDK `HostCapabilities`, host `CapabilityManager`, `NoopCapabilities`, WIT `fs-modified`). `timeline` reads the file modification time only through it (previously `std::fs::metadata`, even without the `filesystem` grant); the `metadata` CLI gets the file size via `fs_read`. No plugin source uses `std::fs` or `std::process` any more; a test enforces this and checks that no plugin reads or writes files without the grant.
- **No synthetic hashes, weather or thumbnails (#40, [ADR 0002](docs/adr/0002-plugins-without-engine-no-output.md))**:
  - `duplicate`: an unreadable file no longer hashes the path string; `doc.duplicate` and `doc.file.sha256` stay empty with a warning. `phash` is now a real dHash over the decoded pixels (`image` crate) instead of the compressed bytes, and `DuplicateInfo.phash` is `Option<String>` (`None` for undecodable formats such as HEIC; stored as `''` in the `duplicates` table).
  - `weather-enrich`: removed the climatological offline estimate and the fixed substitute date for photos without capture time; without measured data `doc.weather` stays empty with a warning and nothing is cached. `offline_fallback` now warns "not supported".
  - `thumbnail`: real scaling in pure Rust (`image` crate, Lanczos3, EXIF orientation, no upscaling) on every platform and in Wasm; `width`/`height` are the measured sizes; entries exist only for written files. Removed the 1×1 placeholder, the copy-the-original fallback, `sips` and direct `std::fs` access. HEIC/AVIF get no thumbnails (warning).
- **Sample files (#35)**: `Examples/generate_samples.py` now writes valid EXIF (IFD0, Exif IFD, GPS IFD) and a decodable 64×48 JPEG; `sample.jpg`, `sample.png` and `sample.heic` carry camera, lens, exposure and GPS (Berlin) values that `metadata` extracts. Previously the ExifIFD pointer was 0 and the documented values only appeared through the synthetic fallback removed in #23.
- **Streaming docs (#36)**: `docs/pipeline_host_supervisor.md` shows the output of a real `flagme-host stream` run, marks values that are not measured (path hash, thumbnail entries, offline weather estimate) and lists the limits of the streaming mode.
- **Host log output (#34)**: `flagme-host` now initialises a `tracing` subscriber. Host and plugin messages (e.g. `privacy-redact: no face detection result …`) appear on stderr with the plugin name; the level comes from `RUST_LOG` (default `warn`). stdout stays reserved for the command output.
- **No synthetic plugin output (#23, [ADR 0002](docs/adr/0002-plugins-without-engine-no-output.md))**: plugins no longer invent data.
  - `metadata`: removed the demo-value fallback (Sony ILCE-7RM4, GPS Berlin) for files under 128 bytes and for unreadable files; an unreadable file now leaves `doc.metadata` empty and logs a warning.
  - `ocr-text`, `ai-tagging`, `face-detection`: removed all built-in pseudo engines (file-name keyword rules, signature table of sample images, byte statistics, fixed tags, skin-tone heuristic via `djpeg`, byte-derived embeddings). Recognition now runs only through exchangeable engines (`OcrEngine`, `VisionModel`, `FaceDetector`) that receive the image bytes only; without an engine the field stays `None` with a warning. `ai-tagging` checks and L2-normalizes the model's embedding.
  - The three plugins are **experimental and disabled** in `config.yaml`, `config.docker.yaml`, `config.mac-studio.yaml` and `AppConfig::default()`. Settings `model`, `model_tier`, `tier`, `engine`, `device` now warn "not supported yet" (SDK: `SettingsReader::not_supported`).
  - `privacy-redact` warns `no face detection result …; faces NOT redacted` when `doc.faces` is missing.
  - Embeddings without tags are stored with model `unknown` instead of `CLIP`.
- `privacy-redact` (#22): faces are now redacted pixel-accurately inside each `DetectedFace.bbox` on the decoded image (`pixelate` with `block_size` px blocks, Gaussian `blur` with `blur_sigma`, `blackout`) and re-encoded as a valid JPEG (quality 90, no source metadata). Previously a byte range of the encoded file was overwritten. `redacted_<N>_faces_via_<style>` now counts only processed faces; read, decode or write failures log a warning and record no redaction. The `std::fs` write fallback was removed, so writes go through the `filesystem` capability only.

## [0.1.1] - 2026-11-25

### Added
- **Dual-language reverse geocoding (#1, #2)**: `LocationInfo` carries English names (`country`, `region`, `city`) and native names (`local_country`, `local_region`, `local_city`, `local_language`); the `locations` table stores both.

## [0.1.0] - 2026-11-25

### Added
- **Core Platform (`flagme-host`)**:
  - Sandboxed WebAssembly Component Model execution runtime powered by Wasmtime.
  - Granular capability-based security enforcement (`filesystem`, `http`, `sqlite`, `kv-store`, `secrets`).
  - Configurable pipeline orchestrator (`config.yaml`) with structured observability (`tracing`).
  - SQLite persistence layer with relational migrations (`photos`, `metadata`, `locations`, `countries`, `thumbnails`, `ai_tags`, `duplicates`).
- **Unified Document Model (`wasm-geo-models`)**:
  - Strongly typed, serialized pipeline document (`UnifiedDocument`).
- **Plugin SDK (`flagme-sdk`)**:
  - `Plugin` trait and `HostCapabilities` abstractions for guests.
- **WIT Interfaces (`wit/media-plugin.wit`)**:
  - Component Model interfaces for plugins and host imports.
- **7 WASM Plugins (`plugins/`)**:
  - `metadata`: Lossless EXIF, IPTC, and XMP extraction into structured fields and `raw_tags`.
  - `reversegeo`: Dual online and offline geocoding with KV caching.
  - `countryflag`: Offline ISO-3166 database with flag emojis, continent, and region mapping.
  - `thumbnail`: Multi-format deterministic thumbnail generator (128px, 512px, 1024px).
  - `ai-tagging`: Swappable vision model interface supporting MobileNet, CLIP, and OpenCLIP.
  - `duplicate`: Duplicate detection via cryptographic SHA256 and perceptual difference hashing (`phash`).
  - `timeline`: Timestamp normalization across EXIF, XMP, and filesystem with deterministic conflict resolution.
- **Documentation & Specifications**:
  - Spec-Driven Development (SDD) governance artifacts: Verfassung ([constitution.md](constitution.md)), PRD, Tech Spec, and Execution Plan.
  - Comprehensive user, build, and plugin developer guides.

[Unreleased]: https://github.com/Zheng-Bote/wasm_geo-host/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/Zheng-Bote/wasm_geo-host/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Zheng-Bote/wasm_geo-host/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/Zheng-Bote/wasm_geo-host/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Zheng-Bote/wasm_geo-host/releases/tag/v0.1.0
