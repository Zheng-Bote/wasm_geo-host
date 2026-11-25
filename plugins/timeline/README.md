# Timeline Plugin (`flagme-plugin-timeline`)

## Overview
The `timeline` plugin resolves and normalizes temporal timestamps across competing EXIF tags, XMP blocks, and filesystem metadata, creating a unified timeline for media libraries.

---

## Key Features

1. **Deterministic Conflict Resolution Order**:
   Resolves the effective capture time (`taken_at`) using the following strict priority hierarchy:
   1. **EXIF `DateTimeOriginal` / `SubSecTimeOriginal`**: Camera shutter time (highest priority).
   2. **EXIF `CreateDate`**: Digital digitization time.
   3. **XMP `xmp:CreateDate`**: Authoring metadata block.
   4. **Filesystem `Modified`**: File write timestamp.
   5. **Host `Imported`**: Time of arrival in the WASM Geo Host pipeline (fallback).
2. **Provenance & Source Tracking**:
   - Explicitly records `resolution_source` (e.g. `exif_datetime_original`, `imported_timestamp_fallback`), providing full transparency on how the timestamp was determined.
3. **UTC Time Normalization**:
   - Converts all timestamps to UTC ISO-8601 representation via `chrono::DateTime<Utc>`.
4. **Dual-Mode Execution**:
   - **Library Component (`cdylib`)**: Loaded into `flagme-host`.
   - **Standalone CLI Binary (`[[bin]]`)**: Streams in Unix pipelines.

---

## Data Model Output (`TimelineInfo`)

```rust
pub struct TimelineInfo {
    pub taken_at: Option<DateTime<Utc>>,        // 2026-11-25T12:00:00Z
    pub imported_at: DateTime<Utc>,             // 2026-11-25T12:00:52Z
    pub modified_at: Option<DateTime<Utc>>,     // 2026-11-25T10:23:21Z
    pub resolution_source: String,              // "exif_datetime_original"
}
```

---

## Required Capabilities

When executing under sandboxed host control (`config.yaml`), the plugin requires:
- `filesystem`: Access to read file system timestamps when EXIF/XMP tags are absent.

```yaml
plugins:
  timeline:
    enabled: true
    capabilities:
      - filesystem
```

---

## Usage Examples

### 1. In Host Pipeline
```bash
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml
```

### 2. Standalone WASM CLI via Unix Pipe
```bash
cat doc.json | wasmtime run -S cli-exit-with-code target/wasm32-wasip2/release/flagme-plugin-timeline.wasm
```

### 3. Verification in Document JSON
```json
"timeline": {
  "taken_at": "2026-11-25T12:00:52.266045Z",
  "imported_at": "2026-11-25T12:00:52.266045Z",
  "modified_at": "2026-11-25T10:23:21.101882Z",
  "resolution_source": "imported_timestamp_fallback"
}
```
