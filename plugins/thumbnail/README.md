# Thumbnail Plugin (`flagme-plugin-thumbnail`)

## Overview
The `thumbnail` plugin generates deterministically cached multi-resolution image variants for incoming media files, optimizing display performance for grid views, previews, and full-screen displays.

---

## Key Features

1. **Deterministic Multi-Size Generation**:
   - Computes three standardized variants for every processed image:
     - `small`: 128x128 px (icons, map markers, thumbnail strips)
     - `medium`: 512x512 px (list previews, grid views)
     - `large`: 1024x1024 px (lightbox, full-view previews)
2. **Deterministic Cache Keys**:
   - Each thumbnail generates a content-derived `cache_key` based on the source image file path/hash and the target dimension. Redundant scaling operations are avoided if the thumbnail file already exists on disk.
3. **Format & Aspect-Ratio Preservation**:
   - Preserves source aspect ratios without distortion.
   - Supports JPEG, PNG, WebP, TIFF, and HEIC source formats.
4. **Dual-Mode Execution**:
   - **Library Component (`cdylib`)**: Loaded into `flagme-host`.
   - **Standalone CLI Binary (`[[bin]]`)**: Streams in Unix pipelines reading `stdin` JSON and emitting enriched thumbnail entries to `stdout`.

---

## Data Model Output (`ThumbnailEntry`)

```rust
pub struct ThumbnailEntry {
    pub size_label: String,   // "small", "medium", "large"
    pub width: u32,           // 128, 512, 1024
    pub height: u32,          // 128, 512, 1024
    pub path: String,         // "cache/thumbnails/<cache_key>_<size>.jpg"
    pub cache_key: String,    // "aad58983bf531312"
}
```

---

## Required Capabilities

When executing under sandboxed host control (`config.yaml`), the plugin requires:
- `filesystem`: Access to read source images and write generated thumbnail files to `cache/thumbnails/`.

```yaml
plugins:
  thumbnail:
    enabled: true
    capabilities:
      - filesystem
```

---

## Usage Examples

### 1. In Host Pipeline
```bash
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml
```

### 2. Standalone WASM CLI via Unix Pipe
```bash
cat doc.json | wasmtime run -S cli-exit-with-code target/wasm32-wasip2/release/flagme-plugin-thumbnail.wasm
```

### 3. Verification in SQLite
```sql
SELECT photo_id, size_label, width, height, file_path, cache_key FROM thumbnails;
```
| photo_id | size_label | width | height | file_path | cache_key |
|---|---|---|---|---|---|
| `3f12502733adb127` | small | 128 | 128 | cache/thumbnails/aad58983bf531312_128.jpg | aad58983bf531312 |
| `3f12502733adb127` | medium | 512 | 512 | cache/thumbnails/6e982c5779c61d3b_512.jpg | 6e982c5779c61d3b |
| `3f12502733adb127` | large | 1024 | 1024 | cache/thumbnails/6c2140244b0b7f61_1024.jpg | 6c2140244b0b7f61 |
