# OCR Text Recognition Plugin (`flagme-plugin-ocr-text`)

## Overview

The `ocr-text` plugin reads visible text (signs, documents, posters, meters) with the embedded **PaddleOCR PP-OCRv5** models (text detection plus Latin-script recognition, Apache-2.0). It runs them with the pure-Rust inference engine [`rten`](https://github.com/robertknight/rten) inside the plugin's Wasm component ([ADR 0009](../../docs/adr/0009-plugin-inference-rten.md)). The plugin writes text lines with bounding boxes and confidences. The combined text goes into the host's full-text index (`photos_fts`, SQLite FTS5), so `flagme-host search` finds photos by the text on them. Umlauts and ß are recognised ("Grüße aus München", "Straße").

The engine receives only the image bytes, never the file name or path ([ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)).

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `ocr-text` |
| Crate | `flagme-plugin-ocr-text` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_ocr_text.wasm`, built with `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-ocr-text` (Wasm SIMD via `.cargo/config.toml`); loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.4.0` |
| Models | `PP-OCRv5_mobile_det` (4.8 MB) and `latin_PP-OCRv5_mobile_rec` (8.0 MB, 836 characters), Apache-2.0, embedded (`models/`, see [`models/SOURCE.md`](models/SOURCE.md)) |
| Reads | image bytes of the working image (`flagme_sdk::pixel_source`: `doc.working_image.path` from `image-convert`, otherwise `doc.file.path`; [ADR 0011](../../docs/adr/0011-working-image-pure-rust-decoders.md)); JPEG, PNG |
| Writes | `doc.ocr` (`OcrInfo`) |
| Capabilities | `filesystem` |
| Configurable via `settings` | **Yes** (2 effective parameters, see §2.2) |

---

## 1. Key Features

1. **Built-in PaddleOCR engine** (`paddle::PaddleOcrEngine`, name `paddleocr-v5-mobile-latin`). It finds text areas, merges them into lines and reads each line. The 836-character dictionary includes ä ö ü Ä Ö Ü ß, é è à ç and €.
2. **Lines in reading order** with boxes normalised to 0.0–1.0 and a confidence per line. `combined_text` is the lines joined by newlines.
3. **Confidence filter and line limit**: `min_confidence` (default 0.5) drops noise; `max_regions` (default 200) bounds the work per photo.
4. **Clear semantics**: `doc.ocr = None` means "not determined"; `Some` with no regions means "checked, no text found". `primary_language` is never guessed and stays `None`.
5. **Honest failure**: an unreadable file, an unsupported format (HEIC, AVIF, …), a truncated JPEG or a model error leave `doc.ocr` unchanged (`None`) and log a warning with the path and the cause.
6. **Exchangeable engine**: the `OcrEngine` trait (`name`, `recognize`, `recognize_limited`); `OcrTextPlugin::with_engine(..)` replaces the built-in one, e.g. in tests.

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  ocr-text:
    # built-in PaddleOCR PP-OCRv5 engine for Latin script (ADR 0009)
    enabled: true
    capabilities:
      - filesystem     # read image bytes
    resource_limits:
      timeout_ms: 30000
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` in every shipped profile (`config*.yaml`) and `AppConfig::default()` | `false` → stage is skipped. |
| `capabilities` | list | `[filesystem]` | See §4. |
| `resource_limits` | map | `timeout_ms: 30000` | Own time limit; a text-heavy photo takes a few seconds (§2.4). |
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / values | Default | Effect |
| :--- | :--- | :--- | :--- |
| `min_confidence` | float 0.0–1.0 | `0.5` | Lines with a lower confidence are dropped (below 0.5 they are mostly noise such as single characters). |
| `max_regions` | integer 1–1000 | `200` | At most this many text lines are read per image; if more are found, the most confident ones are read and a warning is logged. |
| `engine` | any | — | **Not supported**: only the built-in PaddleOCR engine exists. A warning is logged and the key is ignored. |

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies.

### 2.3 Built-in Parameters (not configurable)

| Step | Value |
| :--- | :--- |
| Detection input | longest side 960 px (multiples of 32), BGR, ImageNet mean/std |
| Text areas | probability > 0.3, connected areas as axis-aligned boxes, mean probability ≥ 0.6, expanded by the DB unclip ratio 1.5 |
| Lines | boxes overlapping vertically by ≥ 50 % with a horizontal gap ≤ 0.8 line heights are merged |
| Recognition input | height 48 px, width proportional (at least 320), BGR, −1..1; greedy CTC decoding |
| Orientation | the EXIF orientation is **not** applied; rotated or curved text is not straightened |

### 2.4 Performance (macOS, Apple Silicon, release build, Wasm SIMD)

| Image | Time per image |
| :--- | --: |
| 1400 × 420, 4 lines | ≈ 0.6 s |
| 1200 × 1600 book cover, 16 areas | ≈ 1.7 s |
| 2268 × 4032 photo, 55 areas | ≈ 4.3 s |

Time grows with the number of text lines (each line is read separately).

---

## 3. Pipeline Position

No dependencies on other plugins. Its text is indexed by the host (`photos_fts`).

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(pixel_source(doc))` | Warning `ocr-text: cannot read …`; `doc.ocr` stays `None`. |

---

## 5. Data Model Output (`OcrInfo`)

```rust
pub struct OcrInfo {
    pub combined_text: String,              // all lines joined with "\n"
    pub regions: Vec<TextRegion>,           // lines in reading order
    pub primary_language: Option<String>,   // always None (language is not detected)
}

pub struct TextRegion {
    pub text: String,
    pub confidence: f32,                    // mean character probability, 0.0..1.0
    pub bbox: BoundingBox,                  // x, y, width, height in 0.0..1.0
}
```

Warnings (`warn` log level):

| Message | Cause |
| :--- | :--- |
| `ocr-text: built-in OCR engine unavailable: …; '<path>' not processed` | the embedded models could not be loaded |
| `ocr-text: cannot read '<path>': …` | file not readable / capability missing |
| `ocr-text: engine 'paddleocr-v5-mobile-latin' failed on '<path>': …` | unsupported format, truncated or undecodable image, model error |
| `ocr-text: '<path>': <n> text lines found, read the <m> most confident (max_regions)` | more lines than `max_regions` |

---

## 6. Usage Examples

### 6.1 Rust API

```rust
use flagme_plugin_ocr_text::{OcrEngine, OcrTextPlugin};
use flagme_plugin_ocr_text::paddle::PaddleOcrEngine;

let plugin = OcrTextPlugin::new();                          // built-in PaddleOCR
let info = PaddleOcrEngine::new()?.recognize(&png_bytes)?;  // engine on its own
```

### 6.2 Standalone CLI

```bash
cat doc.json | wasmtime run -S cli-exit-with-code --dir . \
  target/wasm32-wasip2/release/flagme-plugin-ocr-text.wasm
```

Empty input prints `Plugin ocr-text v0.4.0`. The CLI runs with `NoopCapabilities` (no file access), so it cannot read the image and leaves `doc.ocr` empty with a warning.

### 6.3 SQLite

```sql
SELECT photo_id, combined_text, primary_language FROM ocr_texts;
SELECT photo_id, text, confidence, bbox_x, bbox_y, bbox_w, bbox_h FROM ocr_regions;
SELECT photo_id FROM photos_fts WHERE photos_fts MATCH 'ocr_text:München';
```

---

## 7. Limitations & Implementation Notes

- **Latin script only**: other scripts (Cyrillic, Greek, CJK) are not recognised. Characters missing from the dictionary (e.g. the en dash "–") are dropped.
- **Horizontal text**: boxes are axis-aligned; rotated, vertical or curved text is read poorly or not at all.
- **Small text** in large photos can be missed (detection works on a 960 px image).
- **Formats**: JPEG and PNG only; a JPEG counts as complete only if an end-of-image marker follows its scan data.
- **No language detection**: `primary_language` stays `None`.
- Databases filled by versions before 0.2.0 may still contain synthetic OCR texts until the photos are processed again.

## 8. Models and Licence

PP-OCRv5 mobile text detection and Latin recognition models by the PaddlePaddle Authors ([PaddleOCR](https://github.com/PaddlePaddle/PaddleOCR)), ONNX versions from Hugging Face (`PaddlePaddle/PP-OCRv5_mobile_det_onnx`, `PaddlePaddle/latin_PP-OCRv5_mobile_rec_onnx`), licensed under the Apache-2.0 License ([`models/LICENSE`](models/LICENSE)). Sources, sizes and SHA-256: [`models/SOURCE.md`](models/SOURCE.md).
