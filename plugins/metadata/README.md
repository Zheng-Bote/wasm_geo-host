# Metadata Plugin (`flagme-plugin-metadata`)

## Overview
The `metadata` plugin extracts EXIF, IPTC, and XMP metadata from incoming media files in pure Rust. It guarantees lossless extraction by retaining every vendor-specific or unrecognized tag in a structured key-value map (`raw_tags`), in strict accordance with the project constitution.

---

## Key Features

1. **Lossless Tag Extraction**:
   - Parses standard technical camera parameters into typed fields (`iso`, `focal_length`, `f_number`, `exposure_time`, `camera_make`, `camera_model`, `lens_model`).
   - Retains all raw, proprietary, or extended tags in `raw_tags: HashMap<String, String>` (e.g. `Exif.Image.Software`, `XMP.dc.format`, `IPTC.Application2.RecordVersion`).
2. **GPS Coordinate Extraction**:
   - Automatically parses GPS latitude and longitude tags from EXIF blocks and seeds the initial `LocationInfo` in `UnifiedDocument.location` for downstream plugins like `reversegeo`.
3. **Format Support**:
   - Full header and segment parsing for JPEG, PNG, TIFF, HEIC, and WebP.
4. **Dual-Mode Execution**:
   - **Library Component (`cdylib`)**: Loaded into `flagme-host` sandbox.
   - **Standalone CLI Binary (`[[bin]]`)**: Serves as the first stage in Unix pipelines (`wasmtime run ... <FILE>`).

---

## Data Model Output (`MetadataInfo`)

```rust
pub struct MetadataInfo {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub iso: Option<u32>,
    pub focal_length: Option<f32>,
    pub f_number: Option<f32>,
    pub exposure_time: Option<String>,
    pub raw_tags: HashMap<String, String>,
}
```

---

## Required Capabilities

When executing under sandboxed host control (`config.yaml`), the plugin requires:
- `filesystem`: Read-only access to inspect the file header and metadata markers.

```yaml
plugins:
  metadata:
    enabled: true
    capabilities:
      - filesystem
```

---

## Usage Examples

### 1. In Host Pipeline (`flagme-host`)
```bash
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml
```

### 2. Standalone WASM CLI via Unix Pipe
```bash
wasmtime run -S cli-exit-with-code \
  --dir Examples::/media \
  target/wasm32-wasip2/release/flagme-plugin-metadata.wasm /media/sample.jpg
```

### 3. Verification in SQLite
```sql
SELECT photo_id, camera_make, camera_model, iso, focal_length, f_number FROM metadata;
```
| photo_id | camera_make | camera_model | iso | focal_length | f_number |
|---|---|---|---|---|---|
| `3f12502733adb127` | Sony | ILCE-7RM4 | 100 | 35.0 | 2.8 |
