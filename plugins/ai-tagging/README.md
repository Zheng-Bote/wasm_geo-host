# AI Tagging Plugin (`flagme-plugin-ai-tagging`)

## Overview
The `ai-tagging` plugin provides automated semantic image classification, scene tagging, and descriptive captions for media files in the [UnifiedDocument](../../crates/models/src/lib.rs) pipeline.

---

## Key Features

1. **Swappable Model Abstraction**:
   - Implements the modular `VisionModel` trait, allowing vision architectures to be swapped dynamically without altering host interfaces.
   - Built-in support for:
     - **MobileNetV3**: Fast, low-latency edge object and scene classification.
     - **CLIP (ViT-B/32)**: Zero-shot open-vocabulary multimodal tagging.
     - **OpenCLIP (ConvNeXt-Large)**: High-capacity foundational vision embeddings.
2. **Deterministic Output Contract**:
   - Produces structured `AiTag` records containing `label`, `confidence` (0.0 to 1.0), and the exact `model` identifier used.
   - Generates human-readable scene descriptions (`description`).
3. **Container Compatibility (Docker / Podman)**:
   - Compiles to pure `wasm32-wasip2` WebAssembly.
   - **CPU-only by default**: Runs out-of-the-box on Linux container hosts (Debian/Alpine), headless servers, and cloud Kubernetes pods without requiring CUDA or GPU drivers.
   - **GPU & Hardware Acceleration**: Easily augmented via WASI-NN (Host-side neural network API) or sidecar container inference (TorchServe / Triton / ONNX FastAPI via HTTP capability).
4. **Dual-Mode Execution**:
   - **Library Component (`cdylib`)**: Loaded into `flagme-host` sandbox.
   - **Standalone CLI Binary (`[[bin]]`)**: Usable in Unix pipelines (`stdin` $\to$ `stdout`).

---

## Data Model Output (`AiInfo`)

```rust
pub struct AiInfo {
    pub tags: Vec<AiTag>,
    pub description: String,
}

pub struct AiTag {
    pub label: String,       // e.g., "mountain peak", "blue sky"
    pub confidence: f32,     // e.g., 0.91
    pub model: String,       // e.g., "CLIP-ViT-B/32"
}
```

---

## Required Capabilities

When executing under sandboxed host control (`config.yaml`), the plugin requires:
- `filesystem`: Read access to decode image pixels for inference.

```yaml
plugins:
  ai-tagging:
    enabled: true
    capabilities:
      - filesystem
```

---

## Container Deployment (Docker & Podman)

The plugin executes seamlessly inside standard containers. Mount media files into the container volume:

```bash
# Docker
docker run --rm \
  -v $(pwd)/Examples:/data:ro \
  wasm-geo-host process /data/sample.jpg --config /app/config.yaml

# Podman (with SELinux :Z flag)
podman run --rm \
  -v $(pwd)/Examples:/data:Z \
  wasm-geo-host process /data/sample.jpg --config /app/config.yaml
```

Output persisted to SQLite database:
```sql
SELECT photo_id, label, confidence, model FROM ai_tags;
```
| photo_id | label | confidence | model |
|---|---|---|---|
| `3f12502733adb127` | mountain peak | 0.91 | CLIP-ViT-B/32 |
| `3f12502733adb127` | blue sky | 0.88 | CLIP-ViT-B/32 |
