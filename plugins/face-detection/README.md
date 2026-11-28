# Face Detection Plugin (`flagme-plugin-face-detection`)

## Overview

The `face-detection` plugin detects faces with the embedded **YuNet** model ([OpenCV Zoo](https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet), `face_detection_yunet_2023mar`, MIT). The model runs with the pure-Rust inference engine [`rten`](https://github.com/robertknight/rten) inside the plugin's Wasm component ([ADR 0009](../../docs/adr/0009-plugin-inference-rten.md)). For every face, the plugin writes a normalised bounding box, a confidence and five landmarks to `doc.faces`. `privacy-redact` uses these boxes to obfuscate faces.

The detector receives only the image bytes, never the file name or path ([ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)). It finds faces, it does not recognise persons: `embedding` and `person_id` stay empty.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `face-detection` |
| Crate | `flagme-plugin-face-detection` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_face_detection.wasm`, built with `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-face-detection` (Wasm SIMD via `.cargo/config.toml`); loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.5.0` |
| Model | YuNet 2023mar, 232,589 bytes, MIT, embedded (`models/`, see [`models/SOURCE.md`](models/SOURCE.md)) |
| Reads | image bytes of the working image (`flagme_sdk::pixel_source`: `doc.working_image.path` from `image-convert`, otherwise `doc.file.path`; [ADR 0011](../../docs/adr/0011-working-image-pure-rust-decoders.md)); JPEG, PNG |
| Writes | `doc.faces` (`FacesInfo`, `detection_model = "yunet-2023mar"`) |
| Capabilities | `filesystem` |
| Configurable via `settings` | **Yes** (1 effective parameter, see §2.2) |

---

## 1. Key Features

1. **Built-in YuNet detector** (`yunet::YuNetDetector`): faces with box, confidence and landmarks (eyes, nose tip, mouth corners), all normalised to 0.0–1.0 of the image.
2. **Deterministic output**: IDs `face-1` … `face-n` in reading order (by top, then left edge of the box); the same image always gives the same result.
3. **Confidence filter**: faces below `min_confidence` are dropped.
4. **Clear semantics**: `doc.faces = None` means "not determined"; `Some` with an empty list means "checked, no face found".
5. **Honest failure**: an unreadable file, an unsupported format (HEIC, AVIF, …), a truncated JPEG or a model error leave `doc.faces` unchanged (`None`) and log a warning with the path and the cause.
6. **Exchangeable detector**: the `FaceDetector` trait (`model_name`, `detect(image)`); `FaceDetectionPlugin::with_detector(..)` replaces the built-in one, e.g. in tests.

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  face-detection:
    # built-in YuNet detector (ADR 0009); privacy-redact uses its boxes
    enabled: true
    capabilities:
      - filesystem     # read image bytes
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` in every shipped profile (`config*.yaml`) and `AppConfig::default()` | `false` → stage is skipped; `privacy-redact` then warns that no face result exists. |
| `capabilities` | list | `[filesystem]` | See §4. |
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / values | Default | Effect |
| :--- | :--- | :--- | :--- |
| `min_confidence` | float 0.0–1.0 | `0.0` | Faces with a lower confidence are dropped. The detector itself keeps faces with a score of at least 0.6. |
| `model`, `tier`, `device` | any | — | **Not supported**: only the built-in YuNet detector exists. A warning is logged and the key is ignored. |

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies.

### 2.3 Built-in Parameters (not configurable)

| Parameter | Value |
| :--- | :--- |
| Input | image scaled to fit 640 × 640 (aspect ratio kept, zero padding), BGR, 0–255 |
| Score | `sqrt(cls · obj)` per cell of the stride-8/16/32 outputs; candidates below 0.6 are dropped |
| Duplicates | non-maximum suppression, IoU 0.3 |
| Orientation | the EXIF orientation is **not** applied (same as `privacy-redact`, so boxes match the redacted pixels) |

### 2.4 Performance (macOS, Apple Silicon, release build, Wasm SIMD)

| Image | Decode | Detection |
| :--- | --: | --: |
| 1600 × 1257 (test image) | ≈ 18 ms | ≈ 95 ms |
| 4032 × 2268 (9 MP photo) | ≈ 80 ms | ≈ 120 ms |

Model loading takes ≈ 2.5 ms per call (a fresh instance per document). A 4000 × 3000 image runs within the default limits (256 MB, 10 s).

---

## 3. Pipeline Dependencies & Ordering

1. **`face-detection`**: no input dependencies.
2. **`privacy-redact`** (after) → uses `doc.faces`. It logs `privacy-redact: no face detection result for '<path>'; faces NOT redacted` if `doc.faces` is `None`.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(pixel_source(doc))` | Warning `face-detection: cannot read …`; `doc.faces` stays `None`. |

---

## 5. Data Model Output (`FacesInfo`)

```rust
pub struct FacesInfo {
    pub faces: Vec<DetectedFace>,
    pub detection_model: String,             // "yunet-2023mar"
}

pub struct DetectedFace {
    pub id: String,                          // "face-1", "face-2", … in reading order
    pub bbox: BoundingBox,                   // normalized 0.0..1.0
    pub confidence: f32,                     // 0.6..1.0
    pub landmarks: Option<FaceLandmarks>,    // eyes, nose, mouth corners (left = smaller x)
    pub embedding: Option<Vec<f32>>,         // always None (no person recognition)
    pub person_id: Option<String>,           // always None
}
```

Warnings (`warn` log level):

| Message | Cause |
| :--- | :--- |
| `face-detection: built-in detector unavailable: …; '<path>' not processed` | the embedded model could not be loaded |
| `face-detection: cannot read '<path>': …` | file not readable / capability missing |
| `face-detection: detector 'yunet-2023mar' failed on '<path>': …` | unsupported format, truncated or undecodable image, model error |

---

## 6. Usage Examples

### 6.1 Rust API

```rust
use flagme_plugin_face_detection::{FaceDetectionPlugin, FaceDetector};
use flagme_plugin_face_detection::yunet::YuNetDetector;

let plugin = FaceDetectionPlugin::new();                 // built-in YuNet
let faces = YuNetDetector::new()?.detect(&jpeg_bytes)?;  // detector on its own
```

### 6.2 Standalone CLI

```bash
cat doc.json | wasmtime run -S cli-exit-with-code --dir . \
  target/wasm32-wasip2/release/flagme-plugin-face-detection.wasm
```

Empty input prints `Plugin face-detection v0.4.0`. The CLI runs with `NoopCapabilities` (no file access), so it cannot read the image and leaves `doc.faces` empty with a warning.

### 6.3 SQLite

```sql
SELECT id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h, confidence, person_id FROM faces;
SELECT id, name, representative_face_id FROM persons;
```

---

## 7. Limitations & Implementation Notes

- **Small faces**: the model works on a 640 × 640 input. Very small faces in large group photos (a few dozen pixels in the original) can be missed; tiling or a larger input would be a later extension.
- **No person recognition**: no face embeddings, no `person_id` (separate future feature).
- **Formats**: the plugin itself decodes JPEG and PNG only. With `image-convert` in the pipeline every format arrives as a JPEG working image; without it HEIC/AVIF give `None` with a warning. Boxes refer to the image read, i.e. the upright working image for rotated photos. A JPEG counts as complete only if an end-of-image marker follows its scan data; the decoder would otherwise fill a cut-off file with grey.
- **EXIF orientation is not applied** (see §2.3).
- **Face IDs only need to be unique per photo.** The host stores each face under the row key `<photo_id>/<face.id>` in the `faces` table and deletes a photo's old face rows before inserting the new ones ([#24](https://github.com/Zheng-Bote/wasm_geo-host/issues/24)).
- Databases filled by versions before 0.2.0 may still contain synthetic faces until the photos are processed again.

## 8. Model and Licence

YuNet (`face_detection_yunet_2023mar.onnx`), © 2020 Shiqi Yu, from the [OpenCV Zoo](https://github.com/opencv/opencv_zoo), licensed under the MIT License ([`models/LICENSE`](models/LICENSE)). Source, size and SHA-256: [`models/SOURCE.md`](models/SOURCE.md).
