# Thumbnail Plugin (`flagme-plugin-thumbnail`)

## Overview

The `thumbnail` plugin creates three cached preview variants (`small`, `medium`, `large`) per image for grid views, previews and the lightbox. File names are derived deterministically from the source path, so existing thumbnails are reused.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `thumbnail` |
| Crate | `flagme-plugin-thumbnail` (`cdylib` + `rlib` + `[[bin]]`) |
| Version | `0.1.0` |
| Reads | `doc.file.path` |
| Writes | `doc.thumbnails` (`Vec<ThumbnailEntry>`, replaces existing entries); files under `output_dir` (default `cache/thumbnails/`) |
| Capabilities | `filesystem` |
| Configurable via `settings` | **Yes** (4 parameters, see §2.2) |

---

## 1. Key Features

1. **Three Fixed Variants** — `small` (128 px), `medium` (512 px), `large` (1024 px) longest side.
2. **Deterministic Cache Keys** — `cache_key` = 64-bit hash over *source path + size*; the file is only generated if it does not exist yet.
3. **Real Scaling on macOS** — via `sips -s format jpeg -Z <size>`; aspect ratio is preserved.
4. **Always a Valid JPEG** — if scaling is not possible, the original bytes or a 1×1 JPEG are written so the web UI always gets an image.
5. **Dual-Mode Execution** — library component for `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

---

## 2. Configuration & Parameters

### 2.1 Plugin Entry (`config.yaml`)

```yaml
plugins:
  thumbnail:
    enabled: true
    capabilities:
      - filesystem     # read source, write <output_dir>/*
```

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `enabled` | bool | `true` | `false` → no thumbnails; the web UI has no previews. |
| `capabilities` | list | `[filesystem]` | See §4. |
| `settings` | map | `{}` | See §2.2. |

### 2.2 `settings`

| Key | Type / range | Default | Effect |
| :--- | :--- | :--- | :--- |
| `variants` | comma-separated integers 16–8192 | `128,512,1024` | Target sizes (longest side, px); sorted, duplicates removed. Labels: see §2.4. |
| `max_dimension` | integer 16–8192 | *(no limit)* | Variants above the limit are dropped; if none is left, exactly `max_dimension` is used. |
| `quality` | integer 1–100 | *(encoder default)* | JPEG quality passed to `sips -s formatOptions` (macOS only, see §7). |
| `output_dir` | path without `..` | `cache/thumbnails` | Target directory of `<cache_key>_<size>.jpg`. |

```yaml
    settings:
      variants: "128,256,512,1024,2048"
      quality: "95"
```

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): booleans accept `true`/`yes`/`on`/`1` and `false`/`no`/`off`/`0`; numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies. Settings are parsed once per configuration.

### 2.3 Built-in Parameters (not configurable)

| Parameter | Value | Effect |
| :--- | :--- | :--- |
| Entry size | `width` = `height` = variant size | **nominal** box size, not the real pixel size |
| File name | `<cache_key>_<size>.jpg` | e.g. `aad58983bf531312_128.jpg` |
| `cache_key` | `DefaultHasher(path, size)` as 16 hex digits | Changes if the file is moved/renamed, **not** if its content changes. |
| Output format | JPEG | |
| Supported extensions | `jpg`, `jpeg`, `png`, `webp`, `tiff`, `tif`, `heic`, `avif` (case-insensitive), plus `.dat` | Other extensions: warning, document unchanged. |

### 2.4 Size Labels

Sizes are sorted ascending. The smallest gets `small`, the largest `large`, the remaining size closest to 512 `medium`; every other size is labelled `<px>px`. One variant → only `small`; two → `small`, `large`.

| `variants` | Labels |
| :--- | :--- |
| `128,512,1024` (default) | `small`, `medium`, `large` |
| `128,256,512,1024,2048` | `small`, `256px`, `medium`, `1024px`, `large` |
| `256,1024` | `small`, `large` |

The web UI requests `/api/photos/:id/thumbnail/small` and falls back to the first entry.

### 2.5 Generation Strategy (per variant)

1. Target file already exists → reuse it (no regeneration).
2. macOS: `sips` scales to the longest side `<size>` (with `quality`, if set).
3. Otherwise: copy the original bytes unchanged (no scaling).
4. Source not readable: write a 1×1 fallback JPEG.

---

## 3. Pipeline Position

No dependencies on other plugins. Only `doc.file.path` is used.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read` (source) and `fs_write` (fallback copy) | In native host mode `sips` and `std::fs` are used directly; inside a Wasm sandbox no file is written. |

---

## 5. Data Model Output (`ThumbnailEntry`)

```rust
pub struct ThumbnailEntry {
    pub size_label: String,   // "small" | "medium" | "large"
    pub width: u32,           // 128 | 512 | 1024 (nominal)
    pub height: u32,          // 128 | 512 | 1024 (nominal)
    pub path: String,         // "cache/thumbnails/<cache_key>_<size>.jpg"
    pub cache_key: String,    // "aad58983bf531312"
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
  target/wasm32-wasip2/release/flagme-plugin-thumbnail.wasm
```

### 6.3 SQLite

```sql
SELECT photo_id, size_label, width, height, file_path, cache_key FROM thumbnails;
```

| photo_id | size_label | width | height | file_path | cache_key |
|---|---|---|---|---|---|
| `3f12502733adb127` | small | 128 | 128 | cache/thumbnails/aad58983bf531312_128.jpg | aad58983bf531312 |
| `3f12502733adb127` | medium | 512 | 512 | cache/thumbnails/6e982c5779c61d3b_512.jpg | 6e982c5779c61d3b |
| `3f12502733adb127` | large | 1024 | 1024 | cache/thumbnails/6c2140244b0b7f61_1024.jpg | 6c2140244b0b7f61 |

---

## 7. Limitations & Implementation Notes

- **Real scaling only on macOS** (`sips`). On Linux/Docker and in Wasm the original is copied — the "thumbnail" then has the full resolution and file size. Tracked in [#25](https://github.com/Zheng-Bote/wasm_geo-host/issues/25).
- The cache is never invalidated: if the source changes at the same path, the old thumbnail stays. Delete `output_dir` to rebuild; changing `quality` does not regenerate existing files.
- `DefaultHasher` is not guaranteed to be stable across Rust versions; cache keys can change after a toolchain update.
