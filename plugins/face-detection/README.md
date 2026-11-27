# Face Detection Plugin (`flagme-plugin-face-detection`)

> **Experimental — disabled by default.** No face detector is built in yet. Without a detector the plugin produces no result, and `privacy-redact` cannot redact faces ([ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)).

## Overview

The `face-detection` plugin writes detected faces with bounding boxes, landmarks and optional identity embeddings to `doc.faces`. `privacy-redact` uses these boxes to obfuscate faces. Detection comes from an exchangeable detector behind the `FaceDetector` trait. The detector receives only the image bytes, never the file name or path.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `face-detection` |
| Crate | `flagme-plugin-face-detection` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_face_detection.wasm` — `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-face-detection`; loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.3.0` |
| Reads | image bytes of `doc.file.path` |
| Writes | `doc.faces` (`FacesInfo`) — only if a detector is set |
| Capabilities | `filesystem` |
| Configurable via `settings` | **Yes** (1 effective parameter, see §2.2) |

---

## 1. Key Features

1. **Exchangeable detector** — `FaceDetector` trait (`model_name`, `detect(image)`). `FaceDetectionPlugin::new()` has **no** detector; `with_detector(..)` sets one.
2. **Confidence filter** — faces below `min_confidence` are dropped.
3. **Clear semantics** — `doc.faces = None` means "not determined"; `Some` with an empty list means "determined, no faces".
4. **Honest failure** — no detector, an unreadable file or a detector error leaves `doc.faces` unchanged (`None`) and logs a warning.
5. **Dual-Mode Execution** — Wasm component (`media-plugin` world, feature `component`) run in the sandbox of `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  face-detection:
    # experimental: no engine built in, produces no results (ADR 0002)
    enabled: false
    capabilities:
      - filesystem     # read image bytes
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `false` in all shipped profiles and `AppConfig::default()` | `false` → stage is skipped; `privacy-redact` warns that no face result exists. |
| `capabilities` | list | `[filesystem]` | See §4. |
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / values | Default | Effect |
| :--- | :--- | :--- | :--- |
| `min_confidence` | float 0.0–1.0 | `0.0` | Faces with a lower confidence are dropped. |
| `tier` | any | — | **Not supported yet** (no built-in detector) → warning, ignored. |
| `device` | any | — | **Not supported yet** → warning, ignored. |

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies.

---

## 3. Pipeline Dependencies & Ordering

1. **`face-detection`** — no input dependencies.
2. **`privacy-redact`** (after) → uses `doc.faces`; logs `privacy-redact: no face detection result for '<path>'; faces NOT redacted` if `doc.faces` is `None`.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(doc.file.path)` | Warning `face-detection: cannot read …`; `doc.faces` stays `None`. |

---

## 5. Data Model Output (`FacesInfo`)

```rust
pub struct FacesInfo {
    pub faces: Vec<DetectedFace>,
    pub detection_model: String,             // model_name() of the FaceDetector
}

pub struct DetectedFace {
    pub id: String,
    pub bbox: BoundingBox,                   // normalized 0.0..1.0
    pub confidence: f32,
    pub landmarks: Option<FaceLandmarks>,
    pub embedding: Option<Vec<f32>>,
    pub person_id: Option<String>,
}
```

Warnings (`warn` log level):

| Message | Cause |
| :--- | :--- |
| `face-detection: no face detector available (ADR 0002); '<path>' not processed` | plugin created with `new()` |
| `face-detection: cannot read '<path>': …` | file not readable / capability missing |
| `face-detection: detector '<name>' failed on '<path>': …` | detector returned an error |

---

## 6. Usage Examples

### 6.1 Rust API — Plugging in a Detector

```rust
use flagme_plugin_face_detection::{FaceDetectionPlugin, FaceDetector};
use flagme_sdk::DetectedFace;

struct MyDetector;
impl FaceDetector for MyDetector {
    fn model_name(&self) -> &str {
        "my-detector"
    }
    fn detect(&self, image: &[u8]) -> Result<Vec<DetectedFace>, String> {
        // decode `image`, run your detector, map its boxes to DetectedFace
        Err("not implemented".into())
    }
}

let plugin = FaceDetectionPlugin::with_detector(Box::new(MyDetector));
```

### 6.2 Standalone CLI

```bash
cat doc.json | wasmtime run -S cli-exit-with-code --dir . \
  target/wasm32-wasip2/release/flagme-plugin-face-detection.wasm
```

Empty input prints `Plugin face-detection v0.3.0`. The CLI uses `FaceDetectionPlugin::new()` and therefore returns the document unchanged.

### 6.3 SQLite

```sql
SELECT id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h, confidence, person_id FROM faces;
SELECT id, name, representative_face_id FROM persons;
```

---

## 7. Limitations & Implementation Notes

- **No built-in detector.** The former detectors were removed ([#23](https://github.com/Zheng-Bote/wasm_geo-host/issues/23), [ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)): the signature table of sample images, the file-name rules, the skin-tone heuristic (via the external program `djpeg`) and the byte-derived embeddings. The model tiers `lightweight` / `heavy` no longer exist. A real detector follows as a separate feature with its own ADR.
- **Face IDs only need to be unique per photo.** The host stores each face under the row key `<photo_id>/<face.id>` in the `faces` table, deletes a photo's old face rows before inserting the new ones and keeps `persons.representative_face_id` pointing at an existing face of that person ([#24](https://github.com/Zheng-Bote/wasm_geo-host/issues/24)). The document JSON keeps the detector's IDs.
- Databases filled by earlier versions may still contain synthetic faces until the photos are processed again.
