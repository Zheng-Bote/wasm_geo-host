# AI Tagging Plugin (`flagme-plugin-ai-tagging`)

## Overview

The `ai-tagging` plugin assigns semantic tags with confidence values to images, creates a short scene description and a normalized 512-dimensional embedding vector for similarity search (`flagme-host similar`). The model sits behind the swappable `VisionModel` trait.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `ai-tagging` |
| Crate | `flagme-plugin-ai-tagging` (`cdylib` + `rlib` + `[[bin]]`) |
| Version | `0.1.0` |
| Reads | `doc.file.path` (file bytes and file name) |
| Writes | `doc.ai` (`AiInfo`: `tags`, `description`, `embedding`) |
| Capabilities | `filesystem` |
| Configurable via `settings` | **Yes** (4 parameters, see §2.2) |

---

## 1. Key Features

1. **Swappable Model Abstraction** — `VisionModel` trait; three built-in implementations:

   | Rust type | `model_name()` (stored in `AiTag.model`) | Used by the host |
   | :--- | :--- | :--- |
   | `ClipModel` | `CLIP-ViT-B/32` | **yes (default)** |
   | `MobileNetModel` | `MobileNetV3` | no |
   | `OpenClipModel` | `OpenCLIP-ConvNeXt-Large` | no |

2. **Feature Analysis** — brightness, contrast, edge density, color balance and a 64-bin histogram from the image bytes; dimensions from the PNG/JPEG header.
3. **512-D Embedding** — L2-normalized vector, stored in SQLite as `photo_embeddings`.
4. **Dual-Mode Execution** — library component for `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  ai-tagging:
    enabled: true
    capabilities:
      - filesystem     # read image bytes
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` | `false` → no tags, no embedding; `similar` search finds nothing for this photo. |
| `capabilities` | list | `[filesystem]` | See §4. |
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / values | Default | Effect |
| :--- | :--- | :--- | :--- |
| `model` | `CLIP-ViT-B/32` (`clip`), `MobileNetV3` (`mobilenet`), `OpenCLIP-ConvNeXt-Large` (`openclip`) | model of the constructor (`new()` → CLIP) | Built-in model, selected per call. |
| `model_tier` | `lightweight` → MobileNetV3, `heavy` → OpenCLIP | — | Only used if `model` is not set. |
| `embedding_dim` | integer 64–2048 | `512` | Length of `AiInfo.embedding`. |
| `device` | `cpu` | `cpu` | `mps`, `cuda`, `metal`, `gpu` are known but not supported yet → warning, `cpu` is used. |

```yaml
    settings:
      model: "MobileNetV3"
      embedding_dim: "512"
```

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): booleans accept `true`/`yes`/`on`/`1` and `false`/`no`/`off`/`0`; numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies. Settings are parsed once per configuration.

### 2.3 Built-in Parameters (not configurable)

| Parameter | Value | Effect |
| :--- | :--- | :--- |
| Sampling | max. ≈ 8192 samples, header offset `min(len/10, 512)` | basis for the features |
| Fallback dimensions | `1024 × 768` | if no PNG/JPEG header is found |

### 2.4 Tag Rules (with image bytes)

Each document gets one tag per category — **three** tags, or **four** if `daylight` applies:

| Category | Condition | Tag | Confidence |
| :--- | :--- | :--- | :--- |
| Orientation | `height > width` | `portrait orientation` | 0.93 |
| | otherwise | `landscape` | 0.92 |
| Lighting | `brightness > 0.56` | `daylight` **and** `blue sky` | `min(0.75 + brightness × 0.2, 0.98)` / 0.88 |
| | `brightness < 0.35` | `low light` | 0.89 |
| | otherwise | `outdoor` | 0.87 |
| Structure | file name contains `stromzaehler`, `meter` or `doc` | `technical instrument` | 0.94 |
| | `edge_density > 0.48` | `architecture` | `min(0.70 + edge × 0.25, 0.96)` |
| | otherwise | `mountain peak` | 0.91 |

Without image bytes the fixed `predict()` output of the model is used (e.g. CLIP: `mountain peak` 0.91, `blue sky` 0.88).

Description format: `"<w>x<h> scene (<tags>) analyzed with <model> (brightness <b>%, contrast <c>%)"`.

---

## 3. Pipeline Position

No dependencies on other plugins.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(doc.file.path)` | Empty bytes → fixed model output and an embedding derived only from the file path. |

---

## 5. Data Model Output (`AiInfo`)

```rust
pub struct AiInfo {
    pub tags: Vec<AiTag>,
    pub description: String,
    pub embedding: Option<Vec<f32>>,   // 512 values, L2-normalized
}

pub struct AiTag {
    pub label: String,       // e.g. "landscape", "daylight"
    pub confidence: f32,     // 0.0..1.0
    pub model: String,       // e.g. "CLIP-ViT-B/32"
}
```

---

## 6. Usage Examples

### 6.1 Host Pipeline / Container

```bash
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml

docker run --rm -v $(pwd)/Examples:/data:ro \
  wasm-geo-host process /data/sample.jpg --config /app/config.yaml
```

### 6.2 Standalone CLI

```bash
cat doc.json | wasmtime run -S cli-exit-with-code --dir . \
  target/wasm32-wasip2/release/flagme-plugin-ai-tagging.wasm
```

### 6.3 Rust API — Choosing a Model

```rust
use flagme_plugin_ai_tagging::{AiTaggingPlugin, MobileNetModel};

let plugin = AiTaggingPlugin::with_model(Box::new(MobileNetModel));
```

Your own model: implement `VisionModel` (`model_name`, `predict`; optionally override `predict_from_bytes_with_dim`). The constructor model is the default; `settings.model` / `settings.model_tier` replace it per call.

### 6.4 SQLite

```sql
SELECT photo_id, label, confidence, model FROM ai_tags;
SELECT photo_id, model, length(embedding) FROM photo_embeddings;   -- 2048 bytes = 512 × f32
```

---

## 7. Limitations & Implementation Notes

- **No neural inference yet.** All three models share the same heuristic over raw (compressed) file bytes; the model only changes the `model` string and the fallback output. The labels are therefore rough estimates, not real classification. Tracked in [#23](https://github.com/Zheng-Bote/wasm_geo-host/issues/23).
- The embedding is derived from byte statistics and the file path; similarity search finds byte-statistically similar files, not semantically similar images.
- No WASI-NN / GPU support is wired up; only `device: cpu` is accepted.
