# Face Detection & Clustering Plugin (`flagme-plugin-face-detection`)

## Overview

The `face-detection` plugin finds faces in images and returns for each face a bounding box, a confidence value, 5 facial landmarks and an embedding vector for clustering into persons. Its result is the input for `privacy-redact`.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `face-detection` |
| Crate | `flagme-plugin-face-detection` (`cdylib` + `rlib` + `[[bin]]`) |
| Version | `0.1.0` |
| Reads | `doc.file.path` (file bytes and file name) |
| Writes | `doc.faces` (`FacesInfo`) |
| Capabilities | `filesystem` |
| Configurable via `settings` | **No** — model tier only via the Rust API (§6.3) |

---

## 1. Key Features

1. **Two Model Tiers** (`ModelTier`):

   | Tier | Rust type | `detection_model` | Embedding | Used by the host |
   | :--- | :--- | :--- | :---: | :--- |
   | `Lightweight` | `LightweightFaceDetector` | `UltraFace-Slim-INT8` | 128-D | **yes (default)** |
   | `Heavy` | `HeavyFaceDetector` | `SCRFD-ArcFace-ResNet100` | 512-D | no |

   `Heavy` uses the same detection as `Lightweight` and only computes larger embeddings.
2. **5 Landmarks** — left eye, right eye, nose, left/right mouth corner, as normalized `(x, y)` coordinates.
3. **L2-normalized Embeddings** — per face, stored as BLOB in `faces.embedding`.
4. **Person Assignment** — `person_id` → table `persons`.
5. **Dual-Mode Execution** — library component for `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  face-detection:
    enabled: true
    capabilities:
      - filesystem
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` | `false` → no `doc.faces`; `privacy-redact` then redacts no faces. |
| `capabilities` | list | `[filesystem]` | See §4. |
| `settings` | map | `{}` | **Not evaluated** by this plugin. |

### 2.2 `settings`

This plugin has **no configurable parameters**. The host always registers `FaceDetectionPlugin::new()` (= `Lightweight`). Settings support for all plugins is planned in [#26](https://github.com/Zheng-Bote/wasm_geo-host/issues/26).

> **Note:** the shipped profiles set keys that are **ignored**:
>
> | Profile | Ignored keys |
> | :--- | :--- |
> | `config.docker.yaml` | `tier: "lightweight"`, `min_confidence: "0.85"` |
> | `config.mac-studio.yaml` | `tier: "heavy"`, `min_confidence: "0.80"`, `device: "mps"` |
>
> There is **no confidence filter**: all detected faces are returned (confidence 0.88–0.99).

### 2.3 Detection Strategy (first stage with a result wins)

| Stage | Method | Prerequisite |
| :---: | :--- | :--- |
| 1 | **Calibration table**: FNV-1a signature over the first 4096 bytes + file length → stored boxes (and `person_id`) for known sample images | image bytes readable |
| 2 | **Skin-color heuristic**: decode with `djpeg -scale 1/8`, 16 × 16 grid, YCbCr skin mask, region growing | `djpeg` (libjpeg-turbo) in `PATH`, JPEG input |
| 3 | **File-name keywords**: `portrait`, `face`, `selfie`, `person` → 1 face; `group`, `family`, `crowd` → 2 faces | — |
| — | nothing applies | `faces = []` |

### 2.4 Built-in Parameters of the Skin-Color Heuristic (stage 2)

| Parameter | Value |
| :--- | :--- |
| Minimum image size after scaling | 32 × 32 px |
| Grid | 16 × 16 cells |
| Skin pixel (RGB) | `R > 65`, `G > 40`, `B > 25`, `R > G`, `R > B`, `R − G > 12` |
| Skin pixel (YCbCr) | `Cb` 80–125, `Cr` 135–172 |
| Skin cell | share of skin pixels > 0.45 |
| Abort (no faces) | no skin cell, or > 35 % of all cells are skin |
| Seed / growth threshold | ≥ 0.52 / ≥ 0.45; seed rows 2–10, columns 2–13 |
| Face candidate | 1–9 cells wide and high, ≤ 42 cells, region above darker than the center (`top < center × 0.92`, hair line) |
| Bounding box | `x`, `y` clamped to 0.05–0.85; min. width 0.08, min. height 0.11 |
| Confidence | `0.86 + mean_skin × 0.12`, clamped to 0.88–0.98 |

### 2.5 Landmarks from the Bounding Box

| Landmark | x | y |
| :--- | :---: | :---: |
| `left_eye` | `x + 0.31·w` | `y + 0.36·h` |
| `right_eye` | `x + 0.69·w` | `y + 0.36·h` |
| `nose` | `x + 0.50·w` | `y + 0.56·h` |
| `mouth_left` | `x + 0.35·w` | `y + 0.76·h` |
| `mouth_right` | `x + 0.65·w` | `y + 0.76·h` |

(Stage 3 uses fixed landmark positions.)

---

## 3. Pipeline Dependencies & Ordering

1. **`face-detection`** — no input dependencies.
2. **`privacy-redact`** (after) → uses `doc.faces`.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(doc.file.path)` | Empty bytes → only stage 3 (file name) is possible; embeddings are constant vectors. |

Stage 2 additionally starts the external process `djpeg` with the file path. This happens **outside** the capability model and only works in native host mode. Tracked in [#25](https://github.com/Zheng-Bote/wasm_geo-host/issues/25).

---

## 5. Data Model Output (`FacesInfo`)

```rust
pub struct FacesInfo {
    pub faces: Vec<DetectedFace>,
    pub detection_model: String,             // "UltraFace-Slim-INT8"
}

pub struct DetectedFace {
    pub id: String,                          // "face-1", "face-2", … ("face-g1" in stage 3)
    pub bbox: BoundingBox,                   // normalized 0.0..1.0
    pub confidence: f32,
    pub landmarks: Option<FaceLandmarks>,
    pub embedding: Option<Vec<f32>>,         // 128 or 512 values
    pub person_id: Option<String>,           // only set by stage 1
}
```

```json
{
  "faces": [
    {
      "id": "face-1",
      "bbox": { "x": 0.30, "y": 0.20, "width": 0.40, "height": 0.45 },
      "confidence": 0.97,
      "landmarks": {
        "left_eye": [0.42, 0.35],
        "right_eye": [0.58, 0.35],
        "nose": [0.50, 0.45],
        "mouth_left": [0.44, 0.55],
        "mouth_right": [0.56, 0.55]
      },
      "embedding": [0.1, 0.1, "…"],
      "person_id": null
    }
  ],
  "detection_model": "UltraFace-Slim-INT8"
}
```

---

## 6. Usage Examples

### 6.1 Host Pipeline

```bash
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml
```

### 6.2 Standalone CLI

```bash
cat doc.json | wasmtime run -S cli-exit-with-code --dir . \
  target/wasm32-wasip2/release/flagme-plugin-face-detection.wasm
```

Empty input prints `Plugin face-detection v0.1.0`. Stage 2 (`djpeg`) is not available inside Wasm.

### 6.3 Rust API — Choosing the Tier

```rust
use flagme_plugin_face_detection::{FaceDetectionPlugin, ModelTier};

let plugin = FaceDetectionPlugin::with_tier(ModelTier::Heavy);
// or a custom detector: FaceDetectionPlugin::with_detector(Box::new(MyDetector))
```

### 6.4 SQLite

```sql
SELECT id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h, confidence, person_id FROM faces;
SELECT id, name, representative_face_id FROM persons;
```

---

## 7. Limitations & Implementation Notes

- **No neural network inference.** The model names describe the target architecture; detection currently uses the three stages in §2.3. Stage 1 only works for the stored sample images. Tracked in [#23](https://github.com/Zheng-Bote/wasm_geo-host/issues/23).
- **Embeddings are not identity features.** They are derived from file bytes and the box position; clustering by similarity does not recognize the same person across photos.
- **Face IDs are only unique per photo** (`face-1`, …), but `faces.id` is the primary key in SQLite → a later photo overwrites the face rows of an earlier photo with the same ID. Tracked in [#24](https://github.com/Zheng-Bote/wasm_geo-host/issues/24).
