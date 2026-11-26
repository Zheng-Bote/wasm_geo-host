# Metadata Plugin (`flagme-plugin-metadata`)

## Overview

The `metadata` plugin extracts EXIF, IPTC and XMP metadata from media files in pure Rust. Known camera parameters land in typed fields; **every** tag — including vendor-specific and unknown ones — is kept in `raw_tags` (lossless extraction). `raw_tags` is the unmodified original record; downstream plugins must not change it, except for requested serial-number stripping by `privacy-redact` ([ADR 0001](../../docs/adr/0001-raw-tags-original-serial-stripping.md)). It is the **first** pipeline stage and seeds the GPS coordinates for all geo plugins.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `metadata` |
| Crate | `flagme-plugin-metadata` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_metadata.wasm` — `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-metadata`; loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.1.0` |
| Reads | `doc.file.path` (file bytes) |
| Writes | `doc.metadata`; `doc.location.latitude/longitude/altitude` (only if GPS was found) |
| Capabilities | `filesystem` |
| Configurable via `settings` | **Yes** (2 parameters, see §2.2) |

---

## 1. Key Features

1. **Typed Camera Fields** — `camera_make`, `camera_model`, `lens_model`, `iso`, `focal_length`, `f_number`, `exposure_time`.
2. **Lossless `raw_tags`** — each EXIF field as `Exif.<Tag>`, each IPTC dataset as `IPTC.Application2.<Name>`, each XMP property as `XMP.<ns>.<Property>`, plus aggregated JSON objects under the keys `Exif`, `IPTC`, `XMP`.
3. **GPS Extraction** — converts `GPSLatitude`/`GPSLongitude` (degrees/minutes/seconds) to decimal degrees, applies `S`/`W` references and reads `GPSAltitude`.
4. **Format Detection** — JPEG (incl. dimensions), PNG (incl. dimensions, `eXIf` chunk), HEIC/AVIF (`ftyp`), WebP (`RIFF…WEBP`); TIFF via the EXIF container reader.
5. **Dual-Mode Execution** — Wasm component (`media-plugin` world, feature `component`) run in the sandbox of `flagme-host`, or standalone CLI (file path or `stdin` JSON → `stdout` JSON).

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
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / range | Default | Effect |
| :--- | :--- | :--- | :--- |
| `makernote_max_length` | integer 64–65536 | `512` | `Exif.MakerNote` values longer than this are replaced by `<MakerNote <n> bytes>`. |
| `xmp_max_value_length` | integer 64–65536 | `512` | XMP values (attributes and elements) longer than this are dropped. |

```yaml
    settings:
      makernote_max_length: "4096"
      xmp_max_value_length: "2048"
```

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): booleans accept `true`/`yes`/`on`/`1` and `false`/`no`/`off`/`0`; numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies. Settings are parsed once per configuration.

### 2.3 Built-in Parameters (not configurable)

| Parameter | Value | Effect |
| :--- | :--- | :--- |
| XMP namespaces skipped | `x`, `rdf`, `xmlns`, `xml`, `Container` | Structural namespaces are not exported. |
| XMP namespace alias | `Iptc4xmpCore` → `iptc` | e.g. `XMP.iptc.Location`. |
| IPTC record | only record `2` (Application) | Known datasets get names (`Keywords`, `City`, `Caption`, …), others `Dataset<n>`. |

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
| `filesystem` | `fs_read(doc.file.path)` | Warning `metadata: cannot read '<path>': …; metadata left empty`; `doc.metadata` and `doc.location` stay unchanged. |

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

- **No synthetic values:** only tags present in the file are reported. An unreadable file leaves `doc.metadata` empty with a warning; a file without metadata yields no camera, exposure or GPS values ([ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md), [#23](https://github.com/Zheng-Bote/wasm_geo-host/issues/23)).
- The CLI file mode always sets `mime_type = image/jpeg`, independent of the real format.
- HEIC/WebP: format detection only; EXIF is read if an embedded `Exif\0\0` TIFF header is found.
