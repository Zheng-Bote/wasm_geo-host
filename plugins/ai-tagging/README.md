# AI Tagging Plugin (`flagme-plugin-ai-tagging`)

## Overview

The `ai-tagging` plugin computes an image embedding and semantic tags with the image encoder of **OpenAI CLIP ViT-B/32** (int8 ONNX, MIT), run with the pure-Rust inference engine [`rten`](https://github.com/robertknight/rten) inside the plugin's Wasm component ([ADR 0010](../../docs/adr/0010-ai-tagging-clip.md), [ADR 0009](../../docs/adr/0009-plugin-inference-rten.md)).

- The L2-normalised 512-dimensional embedding powers the similarity search (`flagme-host similar <photo_id>`).
- Tags come from a **fixed label list** ([`labels/labels.txt`](labels/labels.txt), 278 English labels for photo collections) and go into the full-text index.
- The model produces no caption, so `description` stays empty.

The model (89 MB) is a **file**, not embedded: `make models` downloads it into `models/`, and the release packages contain it. The plugin reads it through its `filesystem` grant and checks its SHA-256 before loading it. The vision model receives only the image bytes, never the file name or path ([ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)).

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `ai-tagging` |
| Crate | `flagme-plugin-ai-tagging` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_ai_tagging.wasm`, built with `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-ai-tagging` (Wasm SIMD via `.cargo/config.toml`); loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.4.0` |
| Model | `models/clip-vit-b32-vision-int8.onnx` (88,648,877 bytes, SHA-256 `0ab0c1b3…bd93`), from `Xenova/clip-vit-base-patch32` revision `6b7c5237…`, MIT; downloaded by `make models` |
| Labels | `labels/labels.txt` → `labels/labels.bin` (embedded, 278 × 512 text embeddings, built by `make labels`) |
| Reads | the model file at `model_path`; image bytes of the working image (`flagme_sdk::pixel_source`: `doc.working_image.path` from `image-convert`, otherwise `doc.file.path`; [ADR 0011](../../docs/adr/0011-working-image-pure-rust-decoders.md)); JPEG, PNG |
| Writes | `doc.ai` (`AiInfo`: `tags`, `embedding`; `description` empty) |
| Capabilities | `filesystem` |
| Configurable via `settings` | **Yes** (4 effective parameters, see §2.2) |

---

## 1. Key Features

1. **Built-in CLIP model** (`clip::ClipModel`): center-cropped 224 × 224 input, CLIP normalisation, `image_embeds` L2-normalised.
2. **Tags from a fixed list**: softmax over 100 · cosine against the embedded label vectors. At most `max_tags` labels with a probability ≥ `min_confidence`, sorted descending, with `model = "clip-vit-b32"`. A picture that matches no label well gets no tags.
3. **No personal attributes**: the label list contains no labels about gender, age, origin, skin colour, religion, appearance, health or profession of people; for people only neutral labels ("people", "a group of people", "a crowd", "a portrait", "a selfie", "hands"). A test checks the list against a block list (ADR 0010, decision 5).
4. **Verified model file**: a missing file or a SHA-256 mismatch leaves `doc.ai` unchanged (`None`) and logs a warning with the path, the cause and the hint `make models`.
5. **Honest failure**: an unreadable image, an unsupported format (HEIC, AVIF, …), a truncated JPEG or a model error give `None` with a warning.
6. **Exchangeable model**: the `VisionModel` trait; `AiTaggingPlugin::with_model(..)` injects another model (tests), and then no model file is read.

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  ai-tagging:
    # built-in CLIP ViT-B/32 model file, see `make models` (ADR 0010)
    enabled: true
    capabilities:
      - filesystem     # read the model file and the image bytes
    resource_limits:
      max_memory_bytes: 536870912
      timeout_ms: 30000
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` in every shipped profile (`config*.yaml`) and `AppConfig::default()` | `false` → stage is skipped. |
| `capabilities` | list | `[filesystem]` | See §4. |
| `resource_limits` | map | 512 MB, 30 s | The guest needs ≈ 160–192 MB for a 9 MP photo; 512 MB leaves room for larger photos. |
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / values | Default | Effect |
| :--- | :--- | :--- | :--- |
| `model_path` | path without `..` | `models/clip-vit-b32-vision-int8.onnx` | Model file, relative to the working directory of `flagme-host` (like `plugin_dir`). |
| `max_tags` | integer 1–20 | `5` | At most this many tags per image. |
| `min_confidence` | float 0.0–1.0 | `0.10` | Tags with a lower probability are dropped. With 278 labels the best tag of a clear photo typically reaches 0.2–0.5. |
| `embedding_dim` | integer 64–2048 | `512` | Fixed to 512 by the CLIP model; another value is ignored with a warning. |
| `model`, `model_tier`, `device` | any | — | **Not supported**: only the built-in CLIP model exists. A warning is logged and the key is ignored. |

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies.

### 2.3 Model Files and Labels

```bash
make models   # downloads the vision and text model and the tokenizer into models/, checks models/SHA256SUMS
make labels   # recomputes labels/labels.bin from labels/labels.txt (needs the text model); commit both files
```

Only the vision model is needed at runtime. The text model and the tokenizer are used by `tools/clip-labels` to compute the label vectors.

### 2.4 Performance (macOS, Apple Silicon, release build, Wasm SIMD)

| | |
| :--- | --: |
| Model load (read 89 MB, verify SHA-256, parse) | ≈ 0.1 s |
| Embedding and tags per photo | ≈ 0.4 s |
| 4000 × 3000 photo via the host | ≈ 1.4 s (debug host) |

---

## 3. Pipeline Position

No dependencies on other plugins. The host stores the embedding (`photo_embeddings`) and indexes the tags.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(model_path)`, `fs_read(pixel_source(doc))` | Warning `ai-tagging: CLIP model unavailable …`; `doc.ai` stays `None`. |

---

## 5. Data Model Output (`AiInfo`)

```rust
pub struct AiInfo {
    pub tags: Vec<AiTag>,               // labels from labels.txt, best first
    pub description: String,            // always "" (CLIP produces no caption)
    pub embedding: Option<Vec<f32>>,    // 512 values, L2-normalised
}

pub struct AiTag {
    pub label: String,                  // e.g. "a space suit"
    pub confidence: f32,                // softmax probability over all labels
    pub model: String,                  // "clip-vit-b32"
}
```

Warnings (`warn` log level):

| Message | Cause |
| :--- | :--- |
| `ai-tagging: CLIP model unavailable at '<path>': … (run \`make models\`); '<photo>' not processed` | model file missing, unreadable or with a wrong SHA-256 |
| `ai-tagging: cannot read '<path>': …` | image not readable / capability missing |
| `ai-tagging: model 'clip-vit-b32' failed on '<path>': …` | unsupported format, truncated or undecodable image, model error |
| `ai-tagging: embedding_dim <n> ignored: it is fixed to 512 by the CLIP model` | `embedding_dim` ≠ 512 |

---

## 6. Usage Examples

### 6.1 Similarity Search

```bash
flagme-host process /photos/a.jpg --config config.yaml
flagme-host similar <photo_id> --limit 5 --config config.yaml
```

### 6.2 Standalone CLI

```bash
cat doc.json | wasmtime run -S cli-exit-with-code --dir . \
  target/wasm32-wasip2/release/flagme-plugin-ai-tagging.wasm
```

Empty input prints `Plugin ai-tagging v0.4.0`. The CLI runs with `NoopCapabilities` (no file access), so it cannot read the model or the image and leaves `doc.ai` empty with a warning.

### 6.3 SQLite

```sql
SELECT photo_id, label, confidence, model FROM ai_tags;
SELECT photo_id, model, length(embedding) FROM photo_embeddings;
```

---

## 7. Limitations & Implementation Notes

- **Fixed vocabulary**: only the 278 English labels of `labels.txt` can be assigned; no free-text search against the embeddings.
- **No caption**, no face or person recognition.
- **Numerical differences**: int8 kernels round slightly differently natively (ARM NEON) and in Wasm SIMD (cosine ≈ 0.997 between both); stored results always come from the Wasm component.
- **Formats**: JPEG and PNG only; EXIF orientation is not applied (center crop of the stored pixels).
- **Model file required**: without `make models` (or the release package) the plugin produces no result.

## 8. Model, Licence and Usage Note

OpenAI CLIP ViT-B/32 (<https://github.com/openai/CLIP>), Copyright (c) 2021 OpenAI, MIT License; ONNX conversion by Xenova (`Xenova/clip-vit-base-patch32`, MIT). The file `models/CLIP-LICENSE` is downloaded with the model and shipped in the release packages.

**Usage note from the model card**: OpenAI's model card states that CLIP was developed as a research output, that "any deployed use case of the model … is currently out of scope", and that it should only be used with thorough testing on a fixed class taxonomy, because its behaviour varies with the label set and can be biased, especially for labels about people. This project uses a fixed, tested label list without personal attributes, runs locally and only for sorting and searching one's own photos. Do not use the tags for automated decisions about people. The operator is responsible for the use.
