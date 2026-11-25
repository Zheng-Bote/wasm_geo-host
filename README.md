# WASM Geo Host

[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org/)
[![WebAssembly Component Model](https://img.shields.io/badge/wasm-component--model-purple.svg)](https://component-model.bytecodealliance.org/)
[![Wasmtime](https://img.shields.io/badge/runtime-wasmtime-blue.svg)](https://wasmtime.dev/)
[![License](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-green.svg)](NOTICE)

**WASM Geo Host** is a production-ready media processing and ingestion platform written in 100% Rust. It executes image enrichment, geolocation, computer vision tagging, duplicate detection, weather context, OCR, face detection, and privacy redaction inside isolated WebAssembly Components using Wasmtime and WASI 0.2.

---

<!-- START doctoc generated TOC please keep comment here to allow auto update -->

<details>
<summary>Table of Contents</summary>

- [Architecture (C4 Model)](#architecture-c4-model)
- [Pipeline Execution Workflow](#pipeline-execution-workflow)
- [Included Plugins (11 WASI Plugins)](#included-plugins-11-wasi-plugins)
- [Setup & Prerequisites](#setup-prerequisites)
  - [macOS](#macos)
  - [Linux (Ubuntu / Debian / RHEL)](#linux-ubuntu-debian-rhel)
- [Build & Test](#build-test)
- [Usage & CLI Commands](#usage-cli-commands)
  - [1. Configure Capabilities and Pipeline (`config.yaml`)](#1-configure-capabilities-and-pipeline-configyaml)
  - [2. Hardware & Environment Profiles (Docker vs. Mac Studio 32GB)](#2-hardware-environment-profiles-docker-vs-mac-studio-32gb)
    - [A. Docker / Container Profile (`config.docker.yaml`)](#a-docker-container-profile-configdockeryaml)
    - [B. Mac Studio M-Series / High-End Workstation Profile (`config.mac-studio.yaml`)](#b-mac-studio-m-series-high-end-workstation-profile-configmac-studioyaml)
  - [3. Single Media Processing (`flagme-host process`)](#3-single-media-processing-flagme-host-process)
  - [3. Subprocess Streaming Supervisor (`flagme-host stream`)](#3-subprocess-streaming-supervisor-flagme-host-stream)
  - [4. Full-Text Search FTS5 (`flagme-host search`)](#4-full-text-search-fts5-flagme-host-search)
  - [5. Vector Similarity Search (`flagme-host similar`)](#5-vector-similarity-search-flagme-host-similar)
  - [6. Directory Watcher & Batch Ingest (`flagme-host watch`)](#6-directory-watcher-batch-ingest-flagme-host-watch)
  - [7. Embedded Web Dashboard & REST/GeoJSON API (`flagme-host serve`)](#7-embedded-web-dashboard-restgeojson-api-flagme-host-serve)
    - [REST & GeoJSON Endpoints:](#rest-geojson-endpoints)
- [Dual-Mode & Advanced Pipeline Execution Models](#dual-mode-advanced-pipeline-execution-models)
  - [Advanced Pipeline Models](#advanced-pipeline-models)
- [Documentation & Spec-Driven Development (SDD)](#documentation-spec-driven-development-sdd)
- [License & Attribution](#license-attribution)

</details>

<!-- END doctoc generated TOC please keep comment here to allow auto update -->

---

## Architecture (C4 Model)

```mermaid
flowchart TD
    user["User / Browser / Ingest Client"]

    subgraph Platform["WASM Geo Host Platform (flagme-host)"]
        cli["CLI Engine\n(process, stream, search, similar, watch, serve)"]
        web["Embedded Axum Web Server\n(Dashboard, REST, GeoJSON)"]
        watcher["Directory Watcher\n(notify + Worker Task Pool)"]
        config["Config Manager\n(config.yaml)"]
        cap_manager["Capability Sandbox\n(Least Privilege Guard)"]
        orchestrator["Pipeline Orchestrator\n(Sequential / Chained)"]
        runtime["WASM Component Runtime\n(Wasmtime + WASI 0.2)"]
        storage["Storage Engine\n(SQLite media.db + FTS5 + Vector Embeddings)"]
    end

    subgraph Plugins["WASM Component Plugins (wit/media-plugin.wit)"]
        p_meta["metadata.wasm"]
        p_geo["reversegeo.wasm"]
        p_flag["countryflag.wasm"]
        p_thumb["thumbnail.wasm"]
        p_ai["ai-tagging.wasm"]
        p_dup["duplicate.wasm"]
        p_time["timeline.wasm"]
        p_weather["weather-enrich.wasm"]
        p_ocr["ocr-text.wasm"]
        p_face["face-detection.wasm"]
        p_privacy["privacy-redact.wasm"]
    end

    subgraph External["Host System & External Services"]
        fs["Host Filesystem\n(Incoming Media / Library)"]
        net["External APIs\n(Nominatim / Open-Meteo)"]
        db[("media.db\n(SQLite + FTS5 + Vectors)")]
    end

    user -->|"CLI / Web / Import"| cli & web
    cli --> watcher
    watcher --> orchestrator
    cli --> orchestrator
    orchestrator -->|"Enforces Limits & Grants"| cap_manager
    cap_manager -->|"Invokes via WIT"| runtime
    runtime --> Plugins

    cap_manager -.->|"Authorized fs_read/fs_write"| fs
    cap_manager -.->|"Authorized http_get"| net
    orchestrator -->|"Persists UnifiedDocument"| storage
    storage --> db
    web --> storage
```

---

## Pipeline Execution Workflow

Every media file is passed as a `UnifiedDocument` through the configured plugin pipeline:

```mermaid
flowchart LR
    photo["Input Photo"] --> meta["1. metadata\n(EXIF, IPTC, XMP)"]
    meta --> geo["2. reversegeo\n(Coordinates to City)"]
    geo --> flag["3. countryflag\n(ISO Code to Flag Emoji)"]
    flag --> thumb["4. thumbnail\n(128px, 512px, 1024px)"]
    thumb --> ai["5. ai-tagging\n(CLIP & MobileNet Tags)"]
    ai --> dup["6. duplicate\n(SHA256 & pHash Indexing)"]
    dup --> time["7. timeline\n(Timestamp Resolution)"]
    time --> weather["8. weather-enrich\n(Open-Meteo Historical)"]
    weather --> ocr["9. ocr-text\n(Signage Transcription)"]
    ocr --> face["10. face-detection\n(ArcFace / UltraFace)"]
    face --> privacy["11. privacy-redact\n(Pixelate / Blur / GPS Fuzz)"]
    privacy --> db[("12. SQLite Persistence\n(FTS5 & Vector Embeddings)")]
```

---

## Included Plugins (11 WASI Plugins)

| Plugin | Capability Requirements | Description |
| :--- | :--- | :--- |
| **`metadata`** | `filesystem` | Lossless extraction of EXIF, IPTC, and XMP tags; preserves all vendor metadata in `raw_tags`. |
| **`reversegeo`** | `http`, `kv` | Resolves lat/lon coordinates to country, region, county, and city via offline Natural Earth and online OSM fallback. |
| **`countryflag`** | _(none)_ | Completely offline ISO-3166 database returning localized country names, Unicode flag emojis (e.g. 🇩🇪), continents, and regions. |
| **`thumbnail`** | `filesystem` | Deterministic generation of 128px, 512px, and 1024px image variants for JPEG, PNG, WebP, TIFF, HEIC, and AVIF. |
| **`ai-tagging`** | `filesystem` | Swappable vision model interface supporting MobileNetV3 and 512-D dense CLIP vector embeddings. |
| **`duplicate`** | `filesystem`, `kv` | Generates cryptographic SHA256 and perceptual difference hashes (`phash`) to detect duplicates. |
| **`timeline`** | `filesystem` | Resolves and normalizes conflicting timestamps (EXIF original vs. XMP authoring vs. filesystem modtime). |
| **`weather-enrich`** | `http`, `kv` | Historical weather enrichment via Open-Meteo API (temperature, WMO weather codes, weather emojis, humidity). |
| **`ocr-text`** | `filesystem` | Optical Character Recognition (OCR) for signage and text transcription indexed into full-text search. |
| **`face-detection`** | `filesystem` | Detects human faces, bounding boxes, facial landmarks, and 512-D identity embeddings (UltraFace / ArcFace). |
| **`privacy-redact`** | `filesystem` | Privacy sanitization: visual obfuscation of detected faces/text (pixelate/blur), EXIF serial stripping, and GPS coordinate fuzzing. |

---

## Setup & Prerequisites

### macOS

1. **Install Rust**:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
2. **Add WASM target & tools**:
   ```bash
   rustup target add wasm32-wasip2
   brew install sqlite
   ```

### Linux (Ubuntu / Debian / RHEL)

1. **Install Rust and build essentials**:
   ```bash
   # Debian / Ubuntu
   sudo apt update && sudo apt install -y build-essential pkg-config libssl-dev sqlite3 curl
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
2. **Add WASM target**:
   ```bash
   rustup target add wasm32-wasip2
   ```

---

## Build & Test

Build the entire workspace:

```bash
cargo build --all
```

Run the complete test suite (48+ unit and integration tests covering all SDD acceptance criteria):

```bash
cargo test --all
```

Compile all WASI plugins to native WebAssembly Components (`wasm32-wasip2`):

```bash
cargo build -p flagme-plugin-metadata --target wasm32-wasip2 --release
cargo build -p flagme-plugin-reversegeo --target wasm32-wasip2 --release
cargo build -p flagme-plugin-countryflag --target wasm32-wasip2 --release
cargo build -p flagme-plugin-thumbnail --target wasm32-wasip2 --release
cargo build -p flagme-plugin-ai-tagging --target wasm32-wasip2 --release
cargo build -p flagme-plugin-duplicate --target wasm32-wasip2 --release
cargo build -p flagme-plugin-timeline --target wasm32-wasip2 --release
cargo build -p flagme-plugin-weather-enrich --target wasm32-wasip2 --release
cargo build -p flagme-plugin-ocr-text --target wasm32-wasip2 --release
cargo build -p flagme-plugin-face-detection --target wasm32-wasip2 --release
cargo build -p flagme-plugin-privacy-redact --target wasm32-wasip2 --release
```

---

## Usage & CLI Commands

### 1. Configure Capabilities and Pipeline (`config.yaml`)

Edit `config.yaml` to specify which plugins are enabled, execution order, and granted sandbox permissions:

```yaml
database_path: 'media.db'

resource_limits:
  max_memory_bytes: 268435456 # 256 MB
  timeout_ms: 10000 # 10s

pipeline:
  - metadata
  - reversegeo
  - countryflag
  - thumbnail
  - ai-tagging
  - duplicate
  - timeline
  - weather-enrich
  - ocr-text
  - face-detection
  - privacy-redact

plugins:
  metadata:
    enabled: true
    capabilities: ['filesystem']
  reversegeo:
    enabled: true
    capabilities: ['http', 'kv']
  countryflag:
    enabled: true
    capabilities: []
  thumbnail:
    enabled: true
    capabilities: ['filesystem']
  ai-tagging:
    enabled: true
    capabilities: ['filesystem']
  duplicate:
    enabled: true
    capabilities: ['filesystem', 'kv']
  timeline:
    enabled: true
    capabilities: ['filesystem']
  weather-enrich:
    enabled: true
    capabilities: ['http', 'kv']
  ocr-text:
    enabled: true
    capabilities: ['filesystem']
  face-detection:
    enabled: true
    capabilities: ['filesystem']
  privacy-redact:
    enabled: true
    capabilities: ['filesystem']
```

---

### 2. Hardware & Environment Profiles (Docker vs. Mac Studio 32GB)

The platform includes production-ready configuration profiles tailored for different hardware tiers:

#### A. Docker / Container Profile (`config.docker.yaml`)
Designed for CPU-constrained container environments (e.g. 512 MB – 2 GB RAM limit per container).
- **Vision Model**: `MobileNetV3` (Quantized INT8 classification).
- **Face Detection**: `UltraFace-Slim-INT8` (Lightweight 128-D embeddings).
- **OCR Engine**: Quantized low-memory text recognition.
- **Memory Limit**: 512 MB per sandbox instance.

```bash
cargo run -p flagme-host -- process /media/photo.jpg --config config.docker.yaml
```

#### B. Mac Studio M-Series / High-End Workstation Profile (`config.mac-studio.yaml`)
Designed for Apple Silicon workstations with 32 GB+ Unified Memory (leveraging Metal / MPS acceleration where available).
- **Vision Foundation Model**: `OpenCLIP-ConvNeXt-Large` (Dense 512-D vectors for semantic search).
- **Face Detection & Recognition**: `SCRFD + ArcFace-ResNet100` (High-precision 512-D identity embeddings).
- **OCR Engine**: High-resolution multi-lingual server-grade OCR engine.
- **Memory Limit**: 4 GB per sandbox instance; supports 2048px high-fidelity thumbnail generation.

```bash
cargo run -p flagme-host -- process /media/photo.jpg --config config.mac-studio.yaml
```

---

### 3. Single Media Processing (`flagme-host process`)

Processes a single media file in-process through the configured pipeline and stores the result in `media.db`:

```bash
cargo run -p flagme-host -- process Examples/sample.jpg --config config.yaml
```

---

### 3. Subprocess Streaming Supervisor (`flagme-host stream`)

Executes the pipeline in subprocess streaming mode via standard pipes:

```bash
cargo run -p flagme-host -- stream Examples/sample.jpg --config config.yaml
```

---

### 4. Full-Text Search FTS5 (`flagme-host search`)

Queries the built-in SQLite FTS5 full-text index across AI tags, locations, country, camera model, description, and OCR text with BM25 ranking:

```bash
cargo run -p flagme-host -- search "Berlin Sony sunset" --config config.yaml
```

---

### 5. Vector Similarity Search (`flagme-host similar`)

Finds visually and semantically similar photos using cosine similarity ranking over stored 512-D CLIP embedding vectors:

```bash
cargo run -p flagme-host -- similar "doc_photo_1" --limit 10 --config config.yaml
```

---

### 6. Directory Watcher & Batch Ingest (`flagme-host watch`)

Runs a real-time event-driven directory ingestion daemon (using `notify`) with concurrent worker task pools, duplicate skipping, and automatic target directory organization:

```bash
cargo run -p flagme-host -- watch /media/import/ \
  --target-dir /media/library/ \
  --organize-by "{year}/{month}/{country}/{city}" \
  --workers 4 \
  --config config.yaml
```

---

### 7. Embedded Web Dashboard & REST/GeoJSON API (`flagme-host serve`)

Launches the embedded HTTP web server powered by Axum with an interactive Leaflet world map, gallery view, full-text search, and REST/GeoJSON endpoints:

```bash
cargo run -p flagme-host -- serve --port 8080 --bind 0.0.0.0 --config config.yaml
```

Open your browser at `http://localhost:8080` to view the media dashboard.

#### REST & GeoJSON Endpoints:
- `GET /api/photos?page=1&limit=50`: Paginated list of unified media documents.
- `GET /api/photos/:id`: Detailed `UnifiedDocument` JSON payload.
- `GET /api/photos/:id/thumbnail/:size`: Cached thumbnail image stream with HTTP `ETag` and `Cache-Control`.
- `GET /api/map/markers`: GeoJSON `FeatureCollection` containing all geotagged photos with GPS Point coordinates and metadata.
- `GET /api/search?q=:query`: FTS5 full-text search API.

---

## Dual-Mode & Advanced Pipeline Execution Models

Every plugin in `plugins/*` supports **Dual-Mode** compilation:

1. **Library Component (`cdylib`/`rlib`)**: Used for high-throughput, in-process embedding inside `flagme-host`.
2. **Standalone WASI CLI Binary (`bin`)**: Executable directly via `wasmtime run -S cli-exit-with-code <plugin>.wasm`.

### Advanced Pipeline Models

- **[Model 2a: Unix CLI Streaming Pipeline](docs/pipeline_unix_pipes.md)** — Pipe plugins together using standard shell pipes (`|`):
  ```bash
  wasmtime run -S cli-exit-with-code --dir Examples::/media \
      target/wasm32-wasip2/release/flagme-plugin-metadata.wasm /media/sample.jpg \
    | wasmtime run -S cli-exit-with-code target/wasm32-wasip2/release/flagme-plugin-reversegeo.wasm \
    | wasmtime run -S cli-exit-with-code target/wasm32-wasip2/release/flagme-plugin-countryflag.wasm \
    | jq .
  ```
- **[Model 2b: Component Composition](docs/pipeline_component_composition.md)** — Statically compose plugins into a single component (`wac compose` or `wasm-tools compose`) for zero-copy memory execution.
- **[Model 2c: Host Streaming Supervisor](docs/pipeline_host_supervisor.md)** — Let `flagme-host stream` supervise and monitor sandboxed child processes.

---

## Documentation & Spec-Driven Development (SDD)

This repository strictly adheres to **Spec-Driven Development (SDD)** with zero-tolerance for untested code or specification drift:

- [Project Constitution](constitution.md)
- [Agent Guidelines (AGENTS.md)](AGENTS.md)
- [Build Instructions](docs/build-instructions.md)
- [Deployment Guide](docs/deployment-guide.md)
- [Plugin Development Guide](docs/plugin-development-guide.md)
- [Pipeline Model 2a: Unix CLI Streaming](docs/pipeline_unix_pipes.md)
- [Pipeline Model 2b: Component Composition](docs/pipeline_component_composition.md)
- [Pipeline Model 2c: Host Streaming Supervisor](docs/pipeline_host_supervisor.md)
- **PRD Specifications**: `docs/prd/*.md`
- **Technical Design Specs**: `docs/design/*.md`
- **Implementation Plans**: `docs/plans/*.md`

---

## License & Attribution

Licensed under the MIT or Apache 2.0 license. See [NOTICE](NOTICE) for third-party component acknowledgments.
