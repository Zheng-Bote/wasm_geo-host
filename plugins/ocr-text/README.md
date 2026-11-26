# OCR Text Recognition Plugin (`flagme-plugin-ocr-text`)

> **Experimental — disabled by default.** No OCR engine is built in yet. Without an engine the plugin produces no result ([ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)).

## Overview

The `ocr-text` plugin writes visible text (signs, documents, posters) as text regions with bounding boxes. The combined text is written into the host's full-text index (`photos_fts`, SQLite FTS5), so photos can be found by the words they show. Recognition comes from an exchangeable engine behind the `OcrEngine` trait. The engine receives only the image bytes, never the file name or path.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `ocr-text` |
| Crate | `flagme-plugin-ocr-text` (`cdylib` + `rlib` + `[[bin]]`) |
| Version | `0.1.0` |
| Reads | image bytes of `doc.file.path` |
| Writes | `doc.ocr` (`OcrInfo`) — only if an engine is set |
| Capabilities | `filesystem` |
| Configurable via `settings` | **Yes** (1 effective parameter, see §2.2) |

---

## 1. Key Features

1. **Exchangeable engine** — `OcrEngine` trait (`name`, `recognize(image)`). `OcrTextPlugin::new()` has **no** engine; `with_engine(..)` sets one.
2. **Confidence filter** — regions below `min_confidence` are dropped; `combined_text` and `primary_language` are rebuilt.
3. **Honest failure** — no engine, an unreadable file or an engine error leaves `doc.ocr` unchanged (`None`) and logs a warning.
4. **Dual-Mode Execution** — library component for `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  ocr-text:
    # experimental: no engine built in, produces no results (ADR 0002)
    enabled: false
    capabilities:
      - filesystem     # read image bytes
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `false` in all shipped profiles and `AppConfig::default()` | `false` → stage is skipped. |
| `capabilities` | list | `[filesystem]` | See §4. |
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / values | Default | Effect |
| :--- | :--- | :--- | :--- |
| `min_confidence` | float 0.0–1.0 | `0.0` | Regions with a lower confidence are dropped. |
| `engine` | any | — | **Not supported yet** (no built-in engine) → warning, ignored. |

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies.

---

## 3. Pipeline Position

No dependencies on other plugins.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(doc.file.path)` | Warning `ocr-text: cannot read …`; `doc.ocr` stays `None`. |

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

Warnings (`warn` log level):

| Message | Cause |
| :--- | :--- |
| `ocr-text: no OCR engine available (ADR 0002); '<path>' not processed` | plugin created with `new()` |
| `ocr-text: cannot read '<path>': …` | file not readable / capability missing |
| `ocr-text: engine '<name>' failed on '<path>': …` | engine returned an error |

---

## 6. Usage Examples

### 6.1 Rust API — Plugging in an Engine

```rust
use flagme_plugin_ocr_text::{OcrEngine, OcrTextPlugin};
use flagme_sdk::OcrInfo;

struct MyEngine;
impl OcrEngine for MyEngine {
    fn name(&self) -> &str {
        "my-engine"
    }
    fn recognize(&self, image: &[u8]) -> Result<OcrInfo, String> {
        // decode `image`, run your OCR backend, map its result to OcrInfo
        Err("not implemented".into())
    }
}

let plugin = OcrTextPlugin::with_engine(Box::new(MyEngine));
```

### 6.2 Standalone CLI

```bash
cat doc.json | wasmtime run -S cli-exit-with-code --dir . \
  target/wasm32-wasip2/release/flagme-plugin-ocr-text.wasm
```

Empty input prints `Plugin ocr-text v0.1.0`. The CLI uses `OcrTextPlugin::new()` and therefore returns the document unchanged.

### 6.3 SQLite

```sql
SELECT photo_id, combined_text, primary_language FROM ocr_texts;
SELECT photo_id, text, confidence, bbox_x, bbox_y, bbox_w, bbox_h FROM ocr_regions;
SELECT photo_id FROM photos_fts WHERE photos_fts MATCH 'ocr_text:Linden';
```

---

## 7. Limitations & Implementation Notes

- **No built-in engine.** The former `DefaultOcrEngine` (file-name keywords, metadata strings, byte-contrast heuristic, fixed bounding boxes) was removed ([#23](https://github.com/Zheng-Bote/wasm_geo-host/issues/23), [ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)). A real engine follows as a separate feature with its own ADR.
- Databases filled by earlier versions may still contain synthetic OCR texts until the photos are processed again.
