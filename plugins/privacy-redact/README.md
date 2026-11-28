# Privacy Redaction & Sanitizing Plugin (`flagme-plugin-privacy-redact`)

## Overview

The `privacy-redact` plugin sanitizes personal data (GDPR / DSGVO) before media files are indexed, shared or exported. It obfuscates image regions for detected faces, reduces GPS precision and strips hardware serial numbers. It runs as the **last** pipeline stage, after `metadata`, `reversegeo` and `face-detection`.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `privacy-redact` |
| Crate | `flagme-plugin-privacy-redact` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_privacy_redact.wasm` — `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-privacy-redact`; loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.4.0` |
| Reads | the working image (`flagme_sdk::pixel_source`: `doc.working_image.path` from `image-convert`, otherwise `doc.file.path`; [ADR 0011](../../docs/adr/0011-working-image-pure-rust-decoders.md)), `doc.faces`, `doc.location`, `doc.metadata.raw_tags` |
| Writes | `doc.privacy`; modifies `doc.location.latitude/longitude` and `doc.metadata.raw_tags` |
| Capabilities | `filesystem` |
| Configurable via `settings` | **Yes** (8 parameters, see §2) |

---

## 1. Key Features

1. **Face Region Obfuscation** — if `doc.faces` contains at least one face, a redacted copy of the source file is written to `<output_dir>/<doc.id>.redacted.jpg`. The source is decoded (JPEG only), every `DetectedFace.bbox` is converted to a pixel rectangle and only that rectangle is obfuscated; the result is re-encoded as JPEG (quality 90, same dimensions, **no** EXIF/XMP/IPTC of the original). Three styles: `pixelate` (default), `blur`, `blackout`.
2. **GPS Coordinate Fuzzing** — rounds `doc.location.latitude` / `longitude` to `gps_decimal_places` decimals, keeping city/district context for maps.
3. **Hardware Serial Stripping** (on request, the only permitted change to `raw_tags`, see [ADR 0001](../../docs/adr/0001-raw-tags-original-serial-stripping.md)) — removes every `raw_tags` key containing `serialnumber` or `serial_number` (case-insensitive), e.g. `CameraSerialNumber`, `LensSerialNumber`, `Exif.BodySerialNumber`.
4. **Audit Trail** — every applied rule is recorded in `doc.privacy.applied_rules` and persisted to SQLite.
5. **Dual-Mode Execution** — Wasm component (`media-plugin` world, feature `component`) run in the sandbox of `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  privacy-redact:
    enabled: true                  # false → host skips the plugin, document passes unchanged
    capabilities:
      - filesystem                 # required to read the source image and write the redacted copy
    settings:
      style: "pixelate"            # "pixelate" | "blur" | "blackout"
      block_size: "16"             # 2..128, only used by style "pixelate"
      blur_sigma: "24.0"           # 1.0..100.0, only used by style "blur"
      redact_faces: "true"
      fuzz_gps: "true"
      gps_decimal_places: "2"      # 0..6
      strip_serial_numbers: "true"
      output_dir: "cache/thumbnails"
```

All `settings` values are passed to the plugin as **strings** (`HashMap<String, String>`). Quoting is optional; the host strips `"` and `'`.

### 2.2 `settings` Reference

| Key *(aliases)* | Rust field (`PrivacySettings`) | Type | Default | Valid values / range | Effect |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `style` *(`redaction_style`)* | `style: RedactionStyle` | enum | `pixelate` | `pixelate` (`mosaic`), `blur` (`gaussian`), `blackout` (`solid`, `mask`) — case-insensitive | Obfuscation style for the redacted image. Adds rule `redacted_<N>_faces_via_<style>`. |
| `block_size` | `block_size: u32` | integer | `16` | `2`–`128` (clamped) | Edge length in pixels of the uniform mosaic blocks for `pixelate` (block colour = mean of its pixels). Larger = coarser. |
| `blur_sigma` | `blur_sigma: f32` | float | `24.0` | `1.0`–`100.0` (clamped) | Gaussian sigma in pixels for `blur`. Larger = stronger blur. |
| `redact_faces` | `redact_faces: bool` | bool | `true` | `true` / `false` | `true` → redact every face in `doc.faces` and write the redacted copy. `false` → no image is written, `faces_redacted_count = 0`. |
| `fuzz_gps` *(`fuzz_gps_coordinates`)* | `fuzz_gps: bool` | bool | `true` | `true` / `false` | `true` → round `latitude`/`longitude` (only if both are present). Sets `original_gps_fuzzed = true` and adds rule `fuzzed_gps_precision_<d>_decimals`. |
| `gps_decimal_places` | `gps_decimal_places: u32` | integer | `2` | `0`–`6` (values > 6 become 6) | Decimals kept: `round(coord × 10^d) / 10^d`. |
| `strip_serial_numbers` *(`strip_serials`)* | `strip_serial_numbers: bool` | bool | `true` | `true` / `false` | `true` → remove serial-number keys from `raw_tags`. Adds rule `stripped_hardware_serial_numbers` only if at least one key was removed. |
| `output_dir` | `output_dir: String` | path | `cache/thumbnails` (next to the thumbnails made from it, ADR 0012) | relative or absolute path without `..` | Target folder for `<output_dir>/<doc.id>.redacted.jpg` (`/` in `doc.id` is replaced by `_`). A trailing `/` is removed. |

### 2.3 Parsing Rules

The plugin uses the shared SDK rules (`flagme_sdk::settings`):

- **Canonical key wins**: if both `style` and `redaction_style` are set, `style` is used and the alias is reported (same for all aliases).
- **Invalid values** (e.g. `block_size: "abc"`, `gps_decimal_places: "-1"`, `style: "sepia"`, `output_dir: "../x"`) produce a warning `settings[privacy-redact]: invalid value …` and the default stays active.
- **Booleans**: `true`/`yes`/`on`/`1` and `false`/`no`/`off`/`0` (case-insensitive); any other value → warning and default.
- **Out-of-range numbers are clamped** with a warning (`block_size: "500"` → `128`).
- **Unknown keys** produce a warning `settings[privacy-redact]: unknown key '<key>' ignored`.
- **`#` in values** must be quoted (`"a#b"`); unquoted, it starts a comment.

### 2.4 GPS Precision Reference (`gps_decimal_places`)

| `gps_decimal_places` | Example (`52.518624, 13.376198`) | Resolution (at equator) | Typical privacy level |
| :---: | :---: | :---: | :--- |
| `0` | `53.0, 13.0` | ≈ 111 km | Country / state |
| `1` | `52.5, 13.4` | ≈ 11.1 km | Metropolitan area / city |
| **`2` (default)** | **`52.52, 13.38`** | **≈ 1.11 km** | **District / neighborhood (recommended)** |
| `3` | `52.519, 13.376` | ≈ 111 m | Street / block |
| `4` | `52.5186, 13.3762` | ≈ 11.1 m | Building / parcel |
| `5`–`6` | `52.51862, 13.3762` … | ≈ 1.1 m – 0.11 m | Practically no protection |

### 2.5 Profiles Shipped with the Repository

| Key | `config.yaml` | `config_2cores_4GB.yaml`, `config_cpu_8GB.yaml` | `config_mac-studio_32GB.yaml`, `config_mac-studio_64GB.yaml` |
| :--- | :--- | :--- | :--- |
| `style` | *(default `pixelate`)* | `pixelate` | `blackout` |
| `block_size` | *(default `16`)* | `16` | *(default)* |
| `blur_sigma` | *(default)* | *(default)* | `24.0` (no effect with `blackout`) |
| `fuzz_gps` | *(default `true`)* | `true` | `true` |
| `gps_decimal_places` | *(default `2`)* | `2` | `4` |
| `strip_serials` | *(default `true`)* | `true` | `true` |

---

## 3. Pipeline Dependencies & Ordering

`privacy-redact` must run **after** these stages:

1. **`metadata`** → fills `doc.location` (raw GPS) and `doc.metadata.raw_tags` (serial numbers).
2. **`reversegeo`** → resolves city/country from the *exact* coordinates before they are rounded.
3. **`weather-enrich`** → queries the weather with the exact coordinates.
4. **`face-detection`** → fills `doc.faces`.
5. **`privacy-redact`** → redacts, rounds and strips.
6. **`thumbnail`** → runs **after** `privacy-redact` and scales the redacted image ([ADR 0012](../../docs/adr/0012-redacted-avif-thumbnails.md)).

While `privacy-redact` is enabled with `redact_faces` on, the host rejects a pipeline in which `face-detection` is missing, disabled or comes after `privacy-redact`, or in which `thumbnail` comes before it.

If an upstream stage is missing, the respective step is skipped (no coordinates → no fuzzing). If `redact_faces` is on but `doc.faces` is `None` (no face-detection result, e.g. because `face-detection` is disabled — the default, see [ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)), the plugin logs `privacy-redact: no face detection result for '<path>'; faces NOT redacted` and sets `face_redaction = failed`; `thumbnail` then writes no thumbnails (fail closed). `doc.faces` with an empty list stays silent (`no_faces`).

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(pixel_source(doc))`, `fs_write(<output_dir>/…)` | No redacted image is created; a warning is logged and no `redacted_…` rule is recorded. GPS fuzzing and serial stripping still work. |

---

## 5. Data Model Output (`UnifiedDocument.privacy` / `PrivacyInfo`)

```rust
pub struct PrivacyInfo {
    pub is_redacted: bool,                    // true if at least one rule was applied
    pub faces_redacted_count: usize,          // faces actually redacted (valid bbox, image written)
    pub license_plates_redacted_count: usize, // always 0 (not implemented yet)
    pub original_gps_fuzzed: bool,
    pub redacted_image_path: Option<String>,  // Some(..) only if the redacted copy was written
    pub applied_rules: Vec<String>,
    pub face_redaction: Option<FaceRedaction>, // set on every run, see below (ADR 0012)
}
```

`face_redaction` tells the plugins that create images for display (`thumbnail`, via `flagme_sdk::display_source`) which pixels they may use:

| Value | Case | Thumbnails from |
| :--- | :--- | :--- |
| `applied` | faces found, redacted image written (`redacted_image_path`) | the redacted image |
| `no_faces` | face result present but empty | the working image |
| `disabled` | `redact_faces: false` | the working image |
| `failed` | `redact_faces` on, but no face result, image not readable/decodable or not writable | **none** (fail closed) |

Documents of runs before ADR 0012 have no `face_redaction` (`None`); they are treated like `no_faces`.

Possible `applied_rules` entries:

| Rule | Condition |
| :--- | :--- |
| `redacted_<N>_faces_via_<style>` | `redact_faces = true`, the image was decoded and written, and `N > 0` faces had a valid bbox |
| `fuzzed_gps_precision_<d>_decimals` | `fuzz_gps = true` and latitude + longitude present |
| `stripped_hardware_serial_numbers` | `strip_serial_numbers = true` and at least one key removed |

```json
{
  "privacy": {
    "is_redacted": true,
    "faces_redacted_count": 2,
    "license_plates_redacted_count": 0,
    "original_gps_fuzzed": true,
    "redacted_image_path": "cache/thumbnails/950b7c3d7d46ad02.redacted.jpg",
    "applied_rules": [
      "redacted_2_faces_via_pixelate",
      "fuzzed_gps_precision_2_decimals",
      "stripped_hardware_serial_numbers"
    ],
    "face_redaction": "applied"
  }
}
```

---

## 6. Persistence (SQLite)

| Table | Columns |
| :--- | :--- |
| `privacy_redactions` | `photo_id`, `is_redacted`, `faces_redacted_count`, `license_plates_redacted_count`, `original_gps_fuzzed`, `redacted_image_path`, `applied_rules` |
| `photos` | `is_redacted`, `redacted_file_path` |
| `locations` | `is_gps_fuzzed`; `latitude`/`longitude` are stored **already rounded** |

```sql
SELECT photo_id, is_redacted, faces_redacted_count, original_gps_fuzzed, redacted_image_path, applied_rules
FROM privacy_redactions;
```

---

## 7. Usage Examples

### 7.1 Host Pipeline

```bash
cargo run --bin flagme-host -- process Examples/2025/10/Germany/Munich/2025-10-25_094918.jpg --config config.yaml
```

### 7.2 Standalone CLI (stdin → stdout)

```bash
cat doc.json | cargo run -p flagme-plugin-privacy-redact
```

The CLI always uses the **default settings** (`PrivacySettings::default()`); `config.yaml` is not read. Empty input prints `Plugin privacy-redact v0.4.0`.

### 7.3 Rust API with Custom Parameters

```rust
use flagme_plugin_privacy_redact::{PrivacyRedactPlugin, PrivacySettings, RedactionStyle};

let plugin = PrivacyRedactPlugin::with_settings(PrivacySettings {
    style: RedactionStyle::Blackout,
    gps_decimal_places: 3,
    ..PrivacySettings::default()
});
```

`ctx.settings` from `config.yaml` is layered **on top of** these values (`apply_map`): keys that are present override the Rust values.

---

## 8. Limitations & Implementation Notes

- **Image redaction (JPEG only).** Other formats, unreadable files and write failures log `privacy-redact: faces in '<path>' NOT redacted: <reason>` and record no `redacted_…` rule. Bounding boxes are clamped to the image and refer to the stored pixel grid; EXIF orientation is not applied. A box without a valid area (zero size, `NaN`, fully outside) is skipped with a warning and not counted.
- **`raw_tags` keeps the original values (by design).** GPS fuzzing only changes `doc.location`; EXIF tags such as `Exif.GPSLatitude` / `Exif.GPSLongitude` and the aggregated `Exif`, `IPTC`, `XMP` JSON strings in `raw_tags` are left untouched as the lossless original record ([ADR 0001](../../docs/adr/0001-raw-tags-original-serial-stripping.md)).
- **Serial-number stripping is the only permitted change to `raw_tags`** and only when requested (`strip_serial_numbers`, [ADR 0001](../../docs/adr/0001-raw-tags-original-serial-stripping.md)). It currently filters *keys* only; `CameraSerialNumber` / `LensSerialNumber` inside the aggregated `raw_tags["Exif"]` JSON string remain.
- `license_plates_redacted_count` is always `0`.
