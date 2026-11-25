# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
