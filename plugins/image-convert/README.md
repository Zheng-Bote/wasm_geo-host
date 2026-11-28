# Image Convert Plugin (`flagme-plugin-image-convert`)

## Overview

The `image-convert` plugin turns every supported input format into one **working image**: an upright baseline JPEG that all pixel plugins read ([ADR 0011](../../docs/adr/0011-working-image-pure-rust-decoders.md)). It runs right after `metadata`. Thanks to it, `face-detection` and `privacy-redact` work for HEIC photos from smartphones and for WebP or AVIF images from the web, and `thumbnail`, `duplicate` (dHash), `ai-tagging` and `ocr-text` see the same, correctly rotated pixels.

All decoders are pure Rust and run inside the plugin's Wasm sandbox with its own resource limits.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `image-convert` |
| Crate | `flagme-plugin-image-convert` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_image_convert.wasm`, built with `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-image-convert`; loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.4.0` |
| Reads | image bytes of `doc.file.path`; an existing working image in `output_dir` |
| Writes | `doc.working_image` (`WorkingImage`), the file `<output_dir>/<sha256>.working.jpg` |
| Capabilities | `filesystem` |
| Configurable via `settings` | **Yes** (3 parameters, see §2.2) |

---

## 1. Key Features

1. **Formats** (detected from the file signature, not the extension):

   | Format | Decoder | Orientation applied |
   | :--- | :--- | :--- |
   | JPEG | `image` 0.25 (`zune-jpeg`) | EXIF Orientation |
   | PNG, TIFF, WebP | `image` 0.25 | EXIF Orientation |
   | BMP | `image` 0.25 | — |
   | HEIC/HEIF (8/10 bit, grid tiles) | [`heic-rs`](https://github.com/tbraun96/heic-rs) 0.1.5 (MIT OR Apache-2.0, `forbid(unsafe_code)`) | container transforms `irot`, `imir`, `clap` |
   | AVIF (8/10 bit) | [`oxideav-avif`](https://github.com/OxideAV/oxideav-avif) 0.0.18 (MIT, no `unsafe`) | container transforms `irot`, `imir`, `clap` |

   EXIF Orientation inside HEIF files is not applied on top of the container transforms.
2. **No detour for finished JPEGs**: a complete JPEG without rotation (Orientation missing or 1) is used as it is: `working_image.path = file.path`, `converted = false`, no copy and no quality loss.
3. **Working image content**: baseline JPEG, 8-bit RGB, quality `quality`; transparency laid on white; 10/16-bit sources reduced to 8 bit; the source's ICC profile is kept; **no EXIF, GPS, XMP or IPTC**.
4. **Cache**: the file name is the SHA-256 of the source bytes. An existing working image is reused, not written again.
5. **Honest failure** ([ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)): an unreadable file, an unsupported format (e.g. GIF, RAW), a truncated JPEG, HEIC or AVIF, or an image above `max_pixels` leave `doc.working_image = None`, write nothing and log `image-convert: '<path>': <cause>; no working image`. The pixel plugins then read the original file as before.

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
pipeline:
  - metadata
  - image-convert   # right after metadata, before every pixel plugin
  # …

plugins:
  image-convert:
  # working image for all formats (ADR 0011); the pixel plugins read it
    enabled: true
    capabilities:
      - filesystem     # read the source, write the working image
    resource_limits:
      max_memory_bytes: 1073741824
      timeout_ms: 30000
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` in every shipped profile (`config*.yaml`) and `AppConfig::default()` | `false` → no working image; the pixel plugins read the original (only the formats they decode themselves). |
| `capabilities` | list | `[filesystem]` | See §4. |
| `resource_limits` | map | 1 GB, 30 s (`config_2cores_4GB.yaml`: 1 GB, 60 s; `config_cpu_8GB.yaml`: 1 GB, 45 s; `config_mac-studio_32GB.yaml`: 4 GB, 120 s; `config_mac-studio_64GB.yaml`: 6 GB, 120 s) | A 48 MP HEIC needs more than 512 MB. |
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / values | Default | Effect |
| :--- | :--- | :--- | :--- |
| `output_dir` | path | `cache/working` | Directory of the working images (relative to the working directory). |
| `quality` | integer 1–100 (alias `jpeg_quality`) | `95` | JPEG quality of the working image. |
| `max_pixels` | integer 1,000,000–100,000,000 | `50000000` | Larger images are rejected before decoding (header check), e.g. decompression bombs. The two `config_mac-studio_*.yaml` profiles set 100 MP. |

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): numbers outside the range are clamped with a warning; unreadable values and unknown keys produce a `warn` log entry `settings[image-convert]: …` and the default applies.

---

## 3. Output

```json
"working_image": {
  "path": "cache/working/3f0c…9a1e.working.jpg",
  "source_format": "heic",
  "width": 3024,
  "height": 4032,
  "converted": true
}
```

| Field | Meaning |
| :--- | :--- |
| `path` | Image the pixel plugins read (`flagme_sdk::pixel_source`). |
| `source_format` | `jpeg`, `png`, `tiff`, `bmp`, `webp`, `heic` or `avif`. |
| `width`, `height` | Size of the working image, i.e. after rotation. |
| `converted` | `false`: `path` is the original file. |

`working_image` is part of the document (JSON, WIT record `working-image`), not of `media.db`. Face and text boxes of the following plugins refer to the working image.

---

## 4. Capabilities

| Capability | Calls | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(doc.file.path)`, `fs_read`/`fs_write(<output_dir>/<sha256>.working.jpg)` | Warning, `doc.working_image` stays `None`. |

---

## 5. Measurements

Release build, `wasm32-wasip2` with SIMD, macOS (Apple Silicon), prototype of 2026-11-28 (ADR 0011): decoding 12 MP takes ≈ 0.5 s (HEIC) and ≈ 0.7 s (AVIF); a 48 MP HEIC ≈ 2.4 s. See the tech spec `docs/design/image-convert.md` for the values measured via the host.

---

## 6. Limitations

- No RAW (DNG, CR3, NEF …), GIF, JPEG XL, multi-page TIFF or HEIC/AVIF sequences; only the primary image is used.
- No colour conversion: the ICC profile is kept, Display P3 is not converted to sRGB. No HDR: 10-bit sources become 8 bit, gain maps are ignored.
- `oxideav-avif` rejects single AV1 frames above 35,651,584 pixels (AV1 level limit); larger AVIFs must be grid-coded.
- The working image is **not redacted**; it needs the same protection as the original. The cache is not cleaned up when photos are deleted.
- The decoders are young (autumn 2026) and pinned to tested versions; a new version needs a new test against the fixtures in `crates/host/tests/fixtures/formats/`.
