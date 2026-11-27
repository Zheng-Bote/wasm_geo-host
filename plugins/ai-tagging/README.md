# AI Tagging Plugin (`flagme-plugin-ai-tagging`)

> **Experimental — disabled by default.** No vision model is built in yet. Without a model the plugin produces no result ([ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)).

## Overview

The `ai-tagging` plugin writes semantic tags with confidence values, a short scene description and an L2-normalized embedding vector for similarity search (`flagme-host similar`). All of this comes from an exchangeable vision model behind the `VisionModel` trait. The model receives only the image bytes, never the file name or path.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `ai-tagging` |
| Crate | `flagme-plugin-ai-tagging` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_ai_tagging.wasm` — `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-ai-tagging`; loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.3.0` |
| Reads | image bytes of `doc.file.path` |
| Writes | `doc.ai` (`AiInfo`: `tags`, `description`, `embedding`) — only if a model is set |
| Capabilities | `filesystem` |
| Configurable via `settings` | **Yes** (1 effective parameter, see §2.2) |

---

## 1. Key Features

1. **Exchangeable model** — `VisionModel` trait (`model_name`, `analyze(image, embedding_dim)`). `AiTaggingPlugin::new()` has **no** model; `with_model(..)` sets one.
2. **Embedding check** — the plugin L2-normalizes the model's embedding. An embedding whose length differs from `embedding_dim`, or a zero / non-finite vector, is dropped with a warning; tags and description are kept.
3. **Honest failure** — no model, an unreadable file or a model error leaves `doc.ai` unchanged (`None`) and logs a warning.
4. **Dual-Mode Execution** — Wasm component (`media-plugin` world, feature `component`) run in the sandbox of `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  ai-tagging:
    # experimental: no engine built in, produces no results (ADR 0002)
    enabled: false
    capabilities:
      - filesystem     # read image bytes
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `false` in all shipped profiles and `AppConfig::default()` | `false` → stage is skipped; no tags, no embedding. |
| `capabilities` | list | `[filesystem]` | See §4. |
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / values | Default | Effect |
| :--- | :--- | :--- | :--- |
| `embedding_dim` | integer 64–2048 | `512` | Requested embedding length; passed to the model and checked afterwards. |
| `model` | any | — | **Not supported yet** (no built-in model) → warning, ignored. |
| `model_tier` | any | — | **Not supported yet** → warning, ignored. |
| `device` | any | — | **Not supported yet** → warning, ignored. |

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies. Settings are parsed once per configuration.

---

## 3. Pipeline Position

No dependencies on other plugins.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read(doc.file.path)` | Warning `ai-tagging: cannot read …`; `doc.ai` stays `None`. |

---

## 5. Data Model Output (`AiInfo`)

```rust
pub struct AiInfo {
    pub tags: Vec<AiTag>,
    pub description: String,
    pub embedding: Option<Vec<f32>>,   // embedding_dim values, L2-normalized
}

pub struct AiTag {
    pub label: String,
    pub confidence: f32,     // 0.0..1.0
    pub model: String,       // model_name() of the VisionModel
}
```

Warnings (`warn` log level):

| Message | Cause |
| :--- | :--- |
| `ai-tagging: no vision model available (ADR 0002); '<path>' not processed` | plugin created with `new()` |
| `ai-tagging: cannot read '<path>': …` | file not readable / capability missing |
| `ai-tagging: model '<name>' failed on '<path>': …` | model returned an error |
| `ai-tagging: embedding of '<path>' from '<name>' dropped: …` | wrong dimension, zero or non-finite vector |

---

## 6. Usage Examples

### 6.1 Rust API — Plugging in a Model

```rust
use flagme_plugin_ai_tagging::{AiTaggingPlugin, VisionModel};
use flagme_sdk::AiInfo;

struct MyModel;
impl VisionModel for MyModel {
    fn model_name(&self) -> &str {
        "my-model"
    }
    fn analyze(&self, image: &[u8], embedding_dim: usize) -> Result<AiInfo, String> {
        // decode `image`, run your model, map its output to AiInfo
        Err("not implemented".into())
    }
}

let plugin = AiTaggingPlugin::with_model(Box::new(MyModel));
```

### 6.2 Standalone CLI

```bash
cat doc.json | wasmtime run -S cli-exit-with-code --dir . \
  target/wasm32-wasip2/release/flagme-plugin-ai-tagging.wasm
```

The CLI uses `AiTaggingPlugin::new()` and therefore returns the document unchanged.

### 6.3 SQLite

```sql
SELECT photo_id, label, confidence, model FROM ai_tags;
SELECT photo_id, model, length(embedding) FROM photo_embeddings;   -- 2048 bytes = 512 × f32
```

---

## 7. Limitations & Implementation Notes

- **No built-in model.** The former heuristics (byte statistics of the compressed file, file-name keywords, fixed tags per "model", byte-histogram embeddings) were removed ([#23](https://github.com/Zheng-Bote/wasm_geo-host/issues/23), [ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)). A real model (e.g. ONNX via a pure-Rust runtime) follows as a separate feature with its own ADR.
- Without a model, `flagme-host similar` has no embeddings to compare.
- Databases filled by earlier versions may still contain synthetic tags and embeddings until the photos are processed again.
