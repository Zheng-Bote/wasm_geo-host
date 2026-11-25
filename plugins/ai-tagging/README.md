# AI Tagging Plugin (`ai-tagging.wasm`)

## Purpose
Provides automated image tagging and visual description via swappable vision models.

## Supported Model Architectures
- **MobileNet**: Lightweight low-latency object classification.
- **CLIP**: Zero-shot open-vocabulary multimodal tagging.
- **OpenCLIP**: High-capacity foundational vision models.

## Architecture
Models are dynamically interchangeable via the `VisionModel` abstraction without hardcoding dependencies inside the plugin boundary.
