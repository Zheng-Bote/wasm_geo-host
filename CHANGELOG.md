# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Plugin settings (#26)**: all plugins read and honour their `settings` from `config.yaml` via the shared SDK module `flagme_sdk::settings` (typed settings, shared parsing rules, warnings for invalid, unsupported and unknown keys). Parameters per plugin are documented in each plugin README §2.2.
- `duplicate`: Hamming threshold `phash_threshold`; `timeline`: `timezone` (IANA via `chrono-tz`) and `source_priority`; `ai-tagging` / `face-detection` / `ocr-text`: model, tier and engine selected per call (superseded by ADR 0002, see *Fixed*).

### Changed
- `privacy-redact`: booleans follow the shared rule — `yes`/`on`/`1` now enable a feature (previously only `true`).
- Host YAML reader keeps `#` inside quoted values.
- Profiles `config.docker.yaml` / `config.mac-studio.yaml`: removed keys without implementation (`device`, `ocr-text.engine`, `max_image_dimension`).

### Fixed
- **No synthetic plugin output (#23, [ADR 0002](docs/adr/0002-plugins-without-engine-no-output.md))**: plugins no longer invent data.
  - `metadata`: removed the demo-value fallback (Sony ILCE-7RM4, GPS Berlin) for files under 128 bytes and for unreadable files; an unreadable file now leaves `doc.metadata` empty and logs a warning.
  - `ocr-text`, `ai-tagging`, `face-detection`: removed all built-in pseudo engines (file-name keyword rules, signature table of sample images, byte statistics, fixed tags, skin-tone heuristic via `djpeg`, byte-derived embeddings). Recognition now runs only through exchangeable engines (`OcrEngine`, `VisionModel`, `FaceDetector`) that receive the image bytes only; without an engine the field stays `None` with a warning. `ai-tagging` checks and L2-normalizes the model's embedding.
  - The three plugins are **experimental and disabled** in `config.yaml`, `config.docker.yaml`, `config.mac-studio.yaml` and `AppConfig::default()`. Settings `model`, `model_tier`, `tier`, `engine`, `device` now warn "not supported yet" (SDK: `SettingsReader::not_supported`).
  - `privacy-redact` warns `no face detection result …; faces NOT redacted` when `doc.faces` is missing.
  - Embeddings without tags are stored with model `unknown` instead of `CLIP`.
- `privacy-redact` (#22): faces are now redacted pixel-accurately inside each `DetectedFace.bbox` on the decoded image (`pixelate` with `block_size` px blocks, Gaussian `blur` with `blur_sigma`, `blackout`) and re-encoded as a valid JPEG (quality 90, no source metadata). Previously a byte range of the encoded file was overwritten. `redacted_<N>_faces_via_<style>` now counts only processed faces; read, decode or write failures log a warning and record no redaction. The `std::fs` write fallback was removed, so writes go through the `filesystem` capability only.

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
