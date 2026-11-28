# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking
- **Thumbnails only from redacted images** ([ADR 0012](docs/adr/0012-redacted-avif-thumbnails.md), #76): `thumbnail` now runs **after** `privacy-redact` at the end of the pipeline. While `privacy-redact` is enabled with `redact_faces` on, the host **rejects a configuration** in which `thumbnail` comes before `privacy-redact` or `face-detection` is missing, disabled or comes after it. Custom pipelines must be reordered.
- **Fail closed**: if redaction is required but fails (no face result, unreadable image, write error), the photo gets no thumbnails and a warning.
- **Thumbnail endpoint**: `GET /api/photos/:id/thumbnail/:size` answers `404` without a thumbnail file; it no longer serves the original image or a built-in 1×1 placeholder. `Cache-Control` is now `private, max-age=86400`.
- **Thumbnails are AVIF** by default (`<key>_<size>.avif`, about half the size of JPEG; ≈ 2.2 s more per photo for 128/512/1024 px in the sandbox). `format: jpeg` keeps JPEG. Existing JPEG thumbnails, made from unredacted images, stay until a photo is processed again.
- **`privacy-redact` `output_dir`** defaults to `cache/thumbnails` (was `cache/redacted`).
- **WIT `wasmgeohost:media@0.3.0`**: the document record gained `working-image`. The host loads only components built against 0.3.0; rebuild all plugins (`make components`).
- **Configuration profiles renamed and extended**: `config.docker.yaml` is now `config_2cores_4GB.yaml` (2 cores, 4 GB RAM; maintained again, all plugins enabled, `plugin_dir` as in `config.yaml`), `config.mac-studio.yaml` is now `config_mac-studio_32GB.yaml`. New profiles `config_cpu_8GB.yaml` (CPU-only, 8 GB RAM) and `config_mac-studio_64GB.yaml` (Mac Studio M5 Max, 64 GB). Each profile names the recommended `watch --workers`. Update your `--config` paths.
- **Face and text boxes of rotated JPEGs** now refer to the upright working image (EXIF orientation applied) instead of the stored pixels. Results of earlier runs for such photos no longer match and need processing again.

### Added
- **Working image for all input formats** ([ADR 0011](docs/adr/0011-working-image-pure-rust-decoders.md)): the new plugin `image-convert` runs right after `metadata` and sets `doc.working_image`. It decodes JPEG, PNG, TIFF, BMP, WebP, HEIC/HEIF (`heic-rs` 0.1.5) and AVIF (`oxideav-avif` 0.0.18, 8 and 10 bit) in pure Rust inside its sandbox, applies the EXIF orientation or the HEIF container transforms (`irot`, `imir`, `clap`), lays transparency on white and writes a baseline JPEG (quality 95) with the source's ICC profile but without EXIF, GPS or XMP to `cache/working/<sha256>.working.jpg`. Complete JPEGs without rotation are used as they are. Settings `output_dir`, `quality`, `max_pixels` (default 50 MP); own limits 1 GB / 30 s (`config_mac-studio_32GB.yaml`: 4 GB / 120 s, `config_mac-studio_64GB.yaml`: 6 GB / 120 s, both 100 MP).
- `thumbnail`, `duplicate` (dHash), `ai-tagging`, `ocr-text`, `face-detection` and `privacy-redact` read the working image (`flagme_sdk::pixel_source`), so HEIC, AVIF, WebP, TIFF and BMP photos now get face detection and redaction, OCR, tags, thumbnails and a perceptual hash. `metadata` and the SHA-256 of `duplicate` keep reading the original file.
- The watcher accepts `.bmp` and `.heif` and skips `*.working.jpg`.
- `PrivacyInfo.face_redaction` (`applied`, `no_faces`, `disabled`, `failed`; WIT `enum face-redaction`) records the result of face redaction; `flagme_sdk::display_source` picks the image a display plugin may use. The web UI shows it and a placeholder when a thumbnail is missing.
- `thumbnail` settings `format` (`avif`, `jpeg`) and format-specific `quality` defaults (AVIF 70, JPEG 75). The shipped profiles write AVIF of at most 1024 px.

## [0.4.0] - 2026-11-27

### Breaking
- **`face-detection`, `ocr-text` and `ai-tagging` are enabled by default** and run real models (ADR 0009, ADR 0010). Processing takes noticeably longer per photo (measured via the host: ≈ 0.14 s faces, ≈ 0.75–4 s OCR depending on the amount of text, ≈ 0.85 s CLIP) and needs more memory; the shipped configs set per-plugin limits (`ocr-text` 30 s; `ai-tagging` 512 MB, 30 s). Disable a plugin with `enabled: false` if you do not need it.
- **`ai-tagging` needs a model file**: the release packages contain `models/clip-vit-b32-vision-int8.onnx`; from source run `make models` (network, ≈ 155 MB, SHA-256 checked). Without it the plugin warns and stores nothing. `make test` and `make release` run `make models` first.
- **`ocr-text`**: `min_confidence` defaults to 0.5 (was 0.0), `combined_text` joins lines with newlines (was spaces), `primary_language` stays empty.
- **`ai-tagging`**: `embedding_dim` is fixed to 512; `description` stays empty (CLIP produces no caption).
- **Wasm SIMD**: plugin components and WASI CLI binaries are built with `+simd128` and need a Wasm runtime with SIMD (wasmtime supports it by default).

### Added
- **Real face detection** ([ADR 0009](docs/adr/0009-plugin-inference-rten.md)): `face-detection` now runs the embedded YuNet model (OpenCV Zoo, `face_detection_yunet_2023mar`, MIT) with the pure-Rust engine `rten` inside its Wasm component. It writes normalised boxes, confidences and five landmarks to `doc.faces` (`detection_model = "yunet-2023mar"`), so `privacy-redact` now redacts real faces. The plugin is enabled by default in `config.yaml`, `config.mac-studio.yaml` and `AppConfig::default()`. Measured on a 9 MP photo: ≈ 0.2 s (decode and detection). JPEG and PNG only; truncated JPEGs are rejected.
- **Real text recognition**: `ocr-text` now runs the embedded PaddleOCR PP-OCRv5 mobile models (text detection and Latin recognition, Apache-2.0) with `rten` inside its Wasm component. It writes text lines in reading order with normalised boxes and confidences; umlauts and ß are recognised. The text is searchable via `flagme-host search`. Enabled by default with `plugins.ocr-text.resource_limits.timeout_ms: 30000`. New setting `max_regions` (default 200); `min_confidence` now defaults to 0.5; `combined_text` joins lines with newlines; `primary_language` stays empty. Measured: ≈ 0.6–4.3 s per image depending on the amount of text.
- **Real image tags and embeddings** ([ADR 0010](docs/adr/0010-ai-tagging-clip.md)): `ai-tagging` runs the image encoder of OpenAI CLIP ViT-B/32 (int8, MIT) with `rten` inside its Wasm component. It stores a 512-d embedding (now used by `flagme-host similar`) and up to `max_tags` tags from a fixed list of 278 labels without personal attributes; `description` stays empty. The 89 MB model is a file: `make models` downloads it (fixed revision, SHA-256 checked), the release packages ship it, and the plugin verifies it before loading. New settings `model_path`, `max_tags`, `min_confidence`; `embedding_dim` is fixed to 512. Enabled by default with 512 MB / 30 s. New tool `tools/clip-labels` and `make labels`.
- Plugin components and WASI CLIs are built with Wasm SIMD (`+simd128`, `.cargo/config.toml`).

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

[Unreleased]: https://github.com/Zheng-Bote/wasm_geo-host/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/Zheng-Bote/wasm_geo-host/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/Zheng-Bote/wasm_geo-host/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Zheng-Bote/wasm_geo-host/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/Zheng-Bote/wasm_geo-host/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Zheng-Bote/wasm_geo-host/releases/tag/v0.1.0
