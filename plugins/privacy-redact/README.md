# Privacy Redaction & Sanitizing Plugin (`flagme-plugin-privacy-redact`)

## Overview
The `privacy-redact` plugin provides automated visual pixel obfuscation and metadata sanitization to protect personal data (GDPR / DSGVO compliance) before media files are indexed, shared, or exported. It operates as the final stage of the media processing pipeline after `metadata`, `reversegeo`, and `face-detection`.

---

## 1. Key Features

1. **Destructive Visual Obfuscation**:
   - Reads detected facial bounding boxes from `UnifiedDocument.faces` (`DetectedFace.bbox`) and permanently obfuscates the corresponding image regions.
   - Supports three configurable redaction algorithms (`style`):
     - `pixelate` (Default): Downscales the target bounding box into coarse mosaic blocks (`block_size`).
     - `blur`: Applies a high-radius Gaussian smoothing filter (`blur_sigma`).
     - `blackout`: Irreversibly overwrites the bounding box with a solid opaque black mask.
2. **Geospatial Coordinate Fuzzing (GPS Truncation)**:
   - Reduces the decimal precision of `UnifiedDocument.location.latitude` and `longitude` to prevent pinpointing private residences while preserving city- or neighborhood-level context for map views and reverse geocoding.
3. **Hardware & Owner Identifier Stripping**:
   - Purges unique camera and lens hardware serial numbers (e.g., `CameraSerialNumber`, `LensSerialNumber`, `BodySerialNumber`, `SerialNumber`, `serial_number`) from `UnifiedDocument.metadata.raw_tags`.
4. **Audit Trail & Relational Persistence**:
   - Records all executed privacy transformations in `UnifiedDocument.privacy` (`PrivacyInfo`) and persists them into SQLite (`photos.is_redacted`, `photos.redacted_file_path`, `locations.is_gps_fuzzed`, and the `privacy_redactions` table).
5. **Dual-Mode Execution**:
   - **Library Component (`cdylib`)**: Loaded into `flagme-host` capability sandbox (`wasm32-wasip2`).
   - **Standalone CLI Binary (`[[bin]]`)**: Usable in Unix pipelines (`stdin` $\to$ `stdout`).

---

## 2. Configuration & Parameters (`config.yaml` / `PrivacySettings`)

The plugin can be configured declaratively via `plugins.privacy-redact.settings` in `config.yaml` (or programmatically in Rust via `PrivacyRedactPlugin::with_settings(PrivacySettings { ... })`).

### YAML Configuration Example

```yaml
plugins:
  privacy-redact:
    enabled: true
    capabilities:
      - filesystem
    settings:
      style: "pixelate"              # Options: "pixelate" | "blur" | "blackout"
      block_size: "16"               # Mosaic block size in px (2..128) when style="pixelate"
      blur_sigma: "24.0"             # Gaussian blur sigma (1.0..100.0) when style="blur"
      redact_faces: "true"           # Redact all faces detected in doc.faces ("true" | "false")
      fuzz_gps: "true"               # Round GPS coordinates in doc.location ("true" | "false")
      gps_decimal_places: "2"        # Decimal places to retain (0..6, default: 2 ≈ 1.1 km)
      strip_serial_numbers: "true"   # Remove hardware serial numbers from raw_tags ("true" | "false")
      output_dir: "cache/redacted"   # Target directory for exported *.redacted.jpg artifacts
```

### Parameter Reference Table

| Parameter (`config.yaml`) | Rust Field (`PrivacySettings`) | Type | Default | Valid Values / Range | Description |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `style` *(alias: `redaction_style`)* | `style: RedactionStyle` | `String` / `enum` | `"pixelate"` (`RedactionStyle::Pixelate`) | `"pixelate"` (`"mosaic"`), `"blur"` (`"gaussian"`), `"blackout"` (`"solid"`, `"mask"`) | Visual obfuscation method applied to detected bounding boxes. Adds rule `redacted_<N>_faces_via_<style>`. |
| `block_size` | `block_size: u32` | `u32` | `16` | `2` – `128` (clamped) | Mosaic block size in pixels when `style: "pixelate"` is active. Larger values produce coarser pixelation. |
| `blur_sigma` | `blur_sigma: f32` | `f32` | `24.0` | `1.0` – `100.0` (clamped) | Standard deviation ($\sigma$) of the Gaussian blur kernel when `style: "blur"` is active. |
| `redact_faces` | `redact_faces: bool` | `bool` | `true` | `"true"` / `"false"` | If `true`, iterates over all entries in `doc.faces.faces`, obfuscates their bounding boxes, and writes a redacted image file to `output_dir`. |
| `fuzz_gps` *(alias: `fuzz_gps_coordinates`)* | `fuzz_gps: bool` | `bool` | `true` | `"true"` / `"false"` | If `true`, rounds `doc.location.latitude` and `doc.location.longitude` to `gps_decimal_places` and sets `original_gps_fuzzed = true`. |
| `gps_decimal_places` | `gps_decimal_places: u32` | `u32` | `2` | `0` – `6` (clamped) | Number of decimal places retained during GPS fuzzing: $\text{round}(\text{coord} \times 10^d) / 10^d$. Adds rule `fuzzed_gps_precision_<d>_decimals`. |
| `strip_serial_numbers` *(alias: `strip_serials`)* | `strip_serial_numbers: bool` | `bool` | `true` | `"true"` / `"false"` | If `true`, removes all keys containing `serialnumber` or `serial_number` (case-insensitive) from `doc.metadata.raw_tags`. Adds rule `stripped_hardware_serial_numbers`. |
| `output_dir` | `output_dir: String` | `String` | `"cache/redacted"` | Relative directory path (no `..` traversal) | Output folder where redacted image copies (`<output_dir>/<doc.id>.redacted.jpg`) are written when `faces_redacted_count > 0`. |

### GPS Precision Reference (`gps_decimal_places`)

| `gps_decimal_places` | Example Coordinate (`52.518624, 13.376198`) | Spatial Resolution (at Equator) | Typical Privacy Level |
| :---: | :---: | :---: | :--- |
| `0` | `53.0, 13.0` | $\approx 111\text{ km}$ | Country / State level |
| `1` | `52.5, 13.4` | $\approx 11.1\text{ km}$ | Metropolitan area / City level |
| **`2` (Default)** | **`52.52, 13.38`** | **$\approx 1.11\text{ km}$** | **District / Neighborhood level (GDPR recommended)** |
| `3` | `52.519, 13.376` | $\approx 111\text{ m}$ | Street / Block level |
| `4` | `52.5186, 13.3762` | $\approx 11.1\text{ m}$ | Building / Parcel level |

---

## 3. Pipeline Dependencies & Ordering

For full effectiveness, `privacy-redact` must be placed **after** the extraction and detection stages in `config.yaml`:

1. **`metadata`** $\to$ Populates `doc.location` (raw EXIF GPS coordinates) and `doc.metadata.raw_tags` (camera/lens serial numbers).
2. **`reversegeo`** $\to$ Resolves exact city/country names from raw GPS coordinates *before* `privacy-redact` rounds the numeric coordinates.
3. **`face-detection`** $\to$ Populates `doc.faces` with normalized bounding boxes (`x`, `y`, `width`, `height` $\in [0.0, 1.0]$).
4. **`privacy-redact`** $\to$ Redacts detected faces, rounds GPS coordinates, and purges hardware serial numbers.

---

## 4. Required Capabilities

When executing under sandboxed host control (`config.yaml`), the plugin requires:
- `filesystem`: Read access to the source image (`doc.file.path`) and write access to create the sanitized output image (`cache/redacted/<doc.id>.redacted.jpg`).

---

## 5. Data Model Output (`UnifiedDocument.privacy` / `PrivacyInfo`)

### Rust Struct Definition (`crates/models/src/lib.rs`)

```rust
pub struct PrivacyInfo {
    pub is_redacted: bool,
    pub faces_redacted_count: usize,
    pub license_plates_redacted_count: usize,
    pub original_gps_fuzzed: bool,
    pub redacted_image_path: Option<String>,
    pub applied_rules: Vec<String>,
}
```

### Example JSON Output in `UnifiedDocument`

```json
{
  "privacy": {
    "is_redacted": true,
    "faces_redacted_count": 2,
    "license_plates_redacted_count": 0,
    "original_gps_fuzzed": true,
    "redacted_image_path": "cache/redacted/950b7c3d7d46ad02.redacted.jpg",
    "applied_rules": [
      "redacted_2_faces_via_pixelate",
      "fuzzed_gps_precision_2_decimals",
      "stripped_hardware_serial_numbers"
    ]
  }
}
```

---

## 6. Usage Examples

### 1. In Host Pipeline (`flagme-host`)
```bash
cargo run --bin flagme-host -- process Examples/2025/10/Germany/Munich/2025-10-25_094918.jpg --config config.yaml
```

### 2. Programmatic Rust Usage with Custom Parameters
```rust
use flagme_plugin_privacy_redact::{PrivacyRedactPlugin, PrivacySettings, RedactionStyle};

let plugin = PrivacyRedactPlugin::with_settings(PrivacySettings {
    style: RedactionStyle::Blackout,
    block_size: 16,
    blur_sigma: 24.0,
    redact_faces: true,
    fuzz_gps: true,
    gps_decimal_places: 2,
    strip_serial_numbers: true,
    output_dir: "cache/redacted".to_string(),
});
```

### 3. SQLite Verification
```sql
SELECT photo_id, is_redacted, faces_redacted_count, original_gps_fuzzed, redacted_image_path, applied_rules
FROM privacy_redactions;
```
