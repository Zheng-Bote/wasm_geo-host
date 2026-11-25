# OCR Text Recognition Plugin (`flagme-plugin-ocr-text`)

## Overview

The `ocr-text` plugin extracts visible text (signs, documents, posters) as text regions with bounding boxes. The combined text is written into the host's full-text index (`photos_fts`, SQLite FTS5), so photos can be found by the words they show. The engine sits behind the swappable `OcrEngine` trait.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `ocr-text` |
| Crate | `flagme-plugin-ocr-text` (`cdylib` + `rlib` + `[[bin]]`) |
| Version | `0.1.0` |
| Reads | `doc.file.path` (file bytes and file name) |
| Writes | `doc.ocr` (`OcrInfo`) |
| Capabilities | `filesystem` |
| Configurable via `settings` | **No** — engine selection only via the Rust API (§6.3) |

---

## 1. Key Features

1. **Swappable Engine** — `OcrEngine` trait (`extract_text`, `extract_text_from_bytes`); default `DefaultOcrEngine`.
2. **Text Regions** — each region with `text`, `confidence` and a normalized `bbox` (0.0–1.0).
3. **Language Hint** — `primary_language` (`de`, `ja`) if regions were found.
4. **FTS5 Integration** — `combined_text` → column `ocr_text` in `photos_fts`.
5. **Dual-Mode Execution** — library component for `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  ocr-text:
    enabled: true
    capabilities:
      - filesystem
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` | `false` → no `doc.ocr`, empty `ocr_text` in the search index. |
| `capabilities` | list | `[filesystem]` | See §4. |
| `settings` | map | `{}` | **Not evaluated** by this plugin. |

### 2.2 `settings`

This plugin has **no configurable parameters**. The host always registers `OcrTextPlugin::new()` (= `DefaultOcrEngine`). Settings support for all plugins is planned in [#26](https://github.com/Zheng-Bote/wasm_geo-host/issues/26).

> **Note:** the shipped profiles set keys that are **ignored**:
>
> | Profile | Ignored keys |
> | :--- | :--- |
> | `config.docker.yaml` | `engine: "CRAFT-CRNN-Quantized"`, `max_image_dimension: "1024"` |
> | `config.mac-studio.yaml` | `engine: "PaddleOCR-v4-Server"`, `max_image_dimension: "4096"` |

### 2.3 Detection Rules of `DefaultOcrEngine` (first match wins)

| Prio | Condition | Result | `primary_language` |
| :---: | :--- | :--- | :--- |
| 1 | file name contains `sign` or `berlin` | `Brandenburger Tor` (0.96), `Unter den Linden` (0.92) | `de` |
| 2 | file name contains `metro` or `tokyo` | `Shinjuku Station` (0.98) | `ja` |
| 3 | file name contains `stromzaehler` or `meter` | `kWh Drehstromzähler` (0.94) | `de` |
| 4 | embedded text in `<dc:title>`, `<photoshop:City>` or `IPTC:Caption` (≥ 3 chars, letters/digits/space/`-`/`,`) | one region per hit (0.91) | `de` |
| 5 | contrast score > `0.85` (share of byte jumps > 48) | file name without `.jpg`/`.png`, `_` → space; confidence `min(0.75 + score × 0.2, 0.95)` | `de` |
| — | nothing applies | no regions, `combined_text = ""` | `null` |

---

## 3. Pipeline Position

No dependencies on other plugins.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(doc.file.path)` | Empty bytes → only the file-name rules (Prio 1–3) can match. |

---

## 5. Data Model Output (`OcrInfo`)

```rust
pub struct OcrInfo {
    pub combined_text: String,              // all region texts joined with " "
    pub regions: Vec<TextRegion>,
    pub primary_language: Option<String>,   // None if no regions
}

pub struct TextRegion {
    pub text: String,
    pub confidence: f32,
    pub bbox: BoundingBox,                  // x, y, width, height in 0.0..1.0
}
```

```json
{
  "combined_text": "Brandenburger Tor Unter den Linden",
  "regions": [
    { "text": "Brandenburger Tor", "confidence": 0.96,
      "bbox": { "x": 0.15, "y": 0.20, "width": 0.70, "height": 0.15 } }
  ],
  "primary_language": "de"
}
```

---

## 6. Usage Examples

### 6.1 Host Pipeline & Search

```bash
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml
cargo run --bin flagme-host -- search "Brandenburger" --config config.yaml
```

### 6.2 Standalone CLI

```bash
cat doc.json | wasmtime run -S cli-exit-with-code --dir . \
  target/wasm32-wasip2/release/flagme-plugin-ocr-text.wasm
```

Empty input prints `Plugin ocr-text v0.1.0`.

### 6.3 Rust API — Custom Engine

```rust
use flagme_plugin_ocr_text::{OcrEngine, OcrTextPlugin};
use flagme_sdk::OcrInfo;

struct MyEngine;
impl OcrEngine for MyEngine {
    fn extract_text(&self, _path: &str) -> OcrInfo {
        // call your OCR backend here and map its result to OcrInfo
        OcrInfo::default()
    }
}

let plugin = OcrTextPlugin::with_engine(Box::new(MyEngine));
```

### 6.4 SQLite

```sql
SELECT photo_id, combined_text, primary_language FROM ocr_texts;
SELECT photo_id, text, confidence, bbox_x, bbox_y, bbox_w, bbox_h FROM ocr_regions;
SELECT photo_id FROM photos_fts WHERE photos_fts MATCH 'ocr_text:Linden';
```

---

## 7. Limitations & Implementation Notes

- **No real text recognition yet.** `DefaultOcrEngine` contains no detection/recognition model; results come from file-name keywords, embedded metadata strings or a byte-contrast heuristic. The bounding boxes are fixed positions, not measured. Tracked in [#23](https://github.com/Zheng-Bote/wasm_geo-host/issues/23).
- `primary_language` is a fixed hint, not a language detection.
