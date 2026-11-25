# Metadata Plugin (`flagme-plugin-metadata`)

## Overview

The `metadata` plugin extracts EXIF, IPTC and XMP metadata from media files in pure Rust. Known camera parameters land in typed fields; **every** tag — including vendor-specific and unknown ones — is kept in `raw_tags` (lossless extraction). `raw_tags` is the unmodified original record; downstream plugins must not change it, except for requested serial-number stripping by `privacy-redact` ([ADR 0001](../../docs/adr/0001-raw-tags-original-serial-stripping.md)). It is the **first** pipeline stage and seeds the GPS coordinates for all geo plugins.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `metadata` |
| Crate | `flagme-plugin-metadata` (`cdylib` + `rlib` + `[[bin]]`) |
| Version | `0.1.0` |
| Reads | `doc.file.path` (file bytes) |
| Writes | `doc.metadata`; `doc.location.latitude/longitude/altitude` (only if GPS was found) |
| Capabilities | `filesystem` |
| Configurable via `settings` | **No** |

---

## 1. Key Features

1. **Typed Camera Fields** — `camera_make`, `camera_model`, `lens_model`, `iso`, `focal_length`, `f_number`, `exposure_time`.
2. **Lossless `raw_tags`** — each EXIF field as `Exif.<Tag>`, each IPTC dataset as `IPTC.Application2.<Name>`, each XMP property as `XMP.<ns>.<Property>`, plus aggregated JSON objects under the keys `Exif`, `IPTC`, `XMP`.
3. **GPS Extraction** — converts `GPSLatitude`/`GPSLongitude` (degrees/minutes/seconds) to decimal degrees, applies `S`/`W` references and reads `GPSAltitude`.
4. **Format Detection** — JPEG (incl. dimensions), PNG (incl. dimensions, `eXIf` chunk), HEIC/AVIF (`ftyp`), WebP (`RIFF…WEBP`); TIFF via the EXIF container reader.
5. **Dual-Mode Execution** — library component for `flagme-host`, or standalone CLI (file path or `stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  metadata:
    enabled: true          # false → no metadata, no GPS → geo plugins skip as well
    capabilities:
      - filesystem         # required: read the file bytes
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` | `false` → host skips the plugin. |
| `capabilities` | list | `[filesystem]` | See §4. |
| `settings` | map | `{}` | **Not evaluated** by this plugin. |

### 2.2 `settings`

This plugin has **no configurable parameters**. Any keys under `settings` are ignored. Settings support for all plugins is planned in [#26](https://github.com/Zheng-Bote/wasm_geo-host/issues/26).

### 2.3 Built-in Parameters (fixed in code)

| Parameter | Value | Effect |
| :--- | :--- | :--- |
| MakerNote truncation | > 512 characters | `Exif.MakerNote` is replaced by `<MakerNote <n> bytes>`. |
| XMP value limit | ≤ 512 characters | Longer XMP values are dropped. |
| XMP namespaces skipped | `x`, `rdf`, `xmlns`, `xml`, `Container` | Structural namespaces are not exported. |
| XMP namespace alias | `Iptc4xmpCore` → `iptc` | e.g. `XMP.iptc.Location`. |
| IPTC record | only record `2` (Application) | Known datasets get names (`Keywords`, `City`, `Caption`, …), others `Dataset<n>`. |
| Synthetic-stub threshold | input < 128 bytes | See §7. |

### 2.4 `raw_tags` Key Schema

| Source | Key pattern | Example |
| :--- | :--- | :--- |
| Container | `Format`, `Image.Width`, `Image.Height` | `Format = JPEG` |
| EXIF | `Exif.<TagName>` | `Exif.Software`, `Exif.GPSLatitude` |
| EXIF (normalized dates) | `Exif.Photo.DateTimeOriginal`, `Exif.Photo.DateTimeDigitized`, `Exif.Image.DateTime` | used by `timeline` |
| EXIF (serials) | `CameraSerialNumber`, `LensSerialNumber` | removed by `privacy-redact` |
| IPTC | `IPTC.Application2.<Name>` | `IPTC.Application2.Keywords` |
| XMP | `XMP.<ns>.<Property>` | `XMP.xmp.CreateDate`, `XMP.photoshop.City` |
| Aggregates | `Exif`, `IPTC`, `XMP` | JSON object as string |

---

## 3. Pipeline Position

`metadata` must be the **first** plugin. Downstream consumers:

| Consumer | Uses |
| :--- | :--- |
| `reversegeo`, `weather-enrich` | `doc.location.latitude/longitude` |
| `timeline` | `raw_tags["Exif.Photo.DateTimeOriginal"]` etc. |
| `privacy-redact` | `raw_tags` (serial numbers) |
| Host FTS5 index | all `raw_tags` values (column `raw_tags`) |

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(doc.file.path)` | The read fails and the plugin falls back to a 12-byte JPEG stub → **synthetic values** (see §7). |

---

## 5. Data Model Output (`MetadataInfo`)

```rust
pub struct MetadataInfo {
    pub camera_make: Option<String>,   // EXIF Make
    pub camera_model: Option<String>,  // EXIF Model
    pub lens_model: Option<String>,    // EXIF LensModel
    pub iso: Option<u32>,              // PhotographicSensitivity / StandardOutputSensitivity
    pub focal_length: Option<f32>,     // mm
    pub f_number: Option<f32>,
    pub exposure_time: Option<String>, // e.g. "1/250 s"
    pub raw_tags: HashMap<String, String>,
}
```

Additionally `doc.location` is created or updated with `latitude`, `longitude` (decimal degrees, south/west negative) and `altitude` (metres) if at least one coordinate was found.

---

## 6. Usage Examples

### 6.1 Host Pipeline

```bash
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml
```

### 6.2 Standalone CLI

The CLI accepts either a **file path** as first argument (creates a new document with id `cli-doc`) or a `UnifiedDocument` JSON on `stdin`:

```bash
# Native
cargo run -p flagme-plugin-metadata -- Examples/sample.jpg

# Wasm (WASI preopen maps ./Examples to /media)
wasmtime run -S cli-exit-with-code \
  --dir Examples::/media \
  target/wasm32-wasip2/release/flagme-plugin-metadata.wasm /media/sample.jpg
```

### 6.3 SQLite

```sql
SELECT photo_id, camera_make, camera_model, iso, focal_length, f_number FROM metadata;
```

---

## 7. Limitations & Implementation Notes

- **Synthetic fallback:** if the input has fewer than 128 bytes and contains no `Make`/`Model`, the plugin fills demo values (`Sony ILCE-7RM4`, ISO 100, 35 mm, f/2.8, GPS Berlin `52.520008, 13.404954`). This also happens when the file cannot be read (missing `filesystem` capability, wrong path). Tracked in [#23](https://github.com/Zheng-Bote/wasm_geo-host/issues/23).
- The CLI file mode always sets `mime_type = image/jpeg`, independent of the real format.
- HEIC/WebP: format detection only; EXIF is read if an embedded `Exif\0\0` TIFF header is found.
