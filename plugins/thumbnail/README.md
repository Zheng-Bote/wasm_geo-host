# Thumbnail Plugin (`flagme-plugin-thumbnail`)

## Overview

The `thumbnail` plugin creates three cached preview variants (`small`, `medium`, `large`) per image for grid views, previews and the lightbox, as **AVIF** by default ([ADR 0012](../../docs/adr/0012-redacted-avif-thumbnails.md)). When faces were redacted it scales the **redacted image**; when redaction was required but failed it writes **no thumbnails** (fail closed). File names are derived deterministically from the source path.

| Property | Value |
| :--- | :--- |
| Plugin name (`config.yaml`) | `thumbnail` |
| Crate | `flagme-plugin-thumbnail` (`cdylib` + `rlib` + `[[bin]]`) |
| Component | `flagme_plugin_thumbnail.wasm` — `cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-thumbnail`; loaded by `flagme-host` from `plugin_dir` ([ADR 0003](../../docs/adr/0003-wasm-component-execution.md)) |
| Version | `0.5.0` |
| Reads | `flagme_sdk::display_source(doc)`: the redacted image (`doc.privacy.redacted_image_path`) if `doc.privacy.face_redaction` is `applied`; nothing if it is `failed`; otherwise the working image (`doc.working_image.path`, else `doc.file.path`; [ADR 0011](../../docs/adr/0011-working-image-pure-rust-decoders.md)). The cache key comes from `doc.file.path`. |
| Writes | `doc.thumbnails` (`Vec<ThumbnailEntry>`, replaces existing entries); files under `output_dir` (default `cache/thumbnails/`) |
| Capabilities | `filesystem` |
| Configurable via `settings` | **Yes** (5 parameters, see §2.2) |

---

## 1. Key Features

1. **Configurable Variants** — default `small` (128 px), `medium` (512 px), `large` (1024 px) longest side.
2. **Real Scaling in pure Rust** — decoded and scaled with the `image` crate (Lanczos3) on every platform and inside the Wasm sandbox; the aspect ratio is kept, the EXIF orientation is applied and images are never upscaled.
3. **AVIF output** — encoded with [`oxideav-avif`](https://github.com/OxideAV/oxideav-avif) 0.0.18 (MIT, pure Rust, `EncodeSpeed::Fast`, one thread, 4:2:0, tiles of at most 1024 px); about half the size of JPEG. `format: jpeg` keeps JPEG.
4. **Fail closed** — with `face_redaction = failed` the plugin reads and writes nothing, leaves `doc.thumbnails` empty and warns; it never falls back to the unredacted original or working image (ADR 0012).
5. **Honest Entries** — `width`/`height` are the measured pixel size of the written file; an entry exists only if its file was written.
6. **Capabilities only** — reads via `fs_read`, writes via `fs_write`; no external process, no direct `std::fs`.
7. **Deterministic File Names** — `cache_key` = 64-bit hash over *source path + size*.
8. **Dual-Mode Execution** — Wasm component (`media-plugin` world, feature `component`) run in the sandbox of `flagme-host`, or standalone CLI (`stdin` JSON → `stdout` JSON).

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
| `format` | `avif` \| `jpeg` (`jpg`) | `avif` | Output format. Other values warn and use `avif`. |
| `quality` | integer 1–100 | `70` (AVIF), `75` (JPEG) | Quality of the written thumbnails in the chosen format. |
| `output_dir` | path without `..` | `cache/thumbnails` | Target directory of `<cache_key>_<size>.avif` (or `.jpg`). |

```yaml
    settings:
      variants: "128,256,512,1024,2048"
      quality: "95"
```

All values are strings and follow the shared SDK rules (`flagme_sdk::settings`): booleans accept `true`/`yes`/`on`/`1` and `false`/`no`/`off`/`0`; numbers outside the range are clamped with a warning; invalid values and unknown keys produce a `warn` log entry `settings[<plugin>]: …` and the default applies. Settings are parsed once per configuration.

### 2.3 Built-in Parameters (not configurable)

| Parameter | Value | Effect |
| :--- | :--- | :--- |
| Entry size | measured size of the written JPEG | longer side = `min(variant, original longer side)`; aspect ratio kept |
| File name | `<cache_key>_<size>.<avif\|jpg>` | e.g. `aad58983bf531312_128.avif` |
| `cache_key` | `DefaultHasher(path, size)` as 16 hex digits | Changes if the file is moved/renamed, **not** if its content changes. |
| Output format | AVIF (default) or JPEG (`format`) | AVIF: `EncodeSpeed::Fast`, one thread, 4:2:0, tiles ≤ 1024 px |
| Supported input | JPEG, PNG, WebP, TIFF, GIF — detected from the content, not the extension; with `image-convert` also HEIC, AVIF and BMP via the JPEG working image | Other formats: warning, no thumbnails. |

### 2.4 Size Labels

Sizes are sorted ascending. The smallest gets `small`, the largest `large`, the remaining size closest to 512 `medium`; every other size is labelled `<px>px`. One variant → only `small`; two → `small`, `large`.

| `variants` | Labels |
| :--- | :--- |
| `128,512,1024` (default) | `small`, `medium`, `large` |
| `128,256,512,1024,2048` | `small`, `256px`, `medium`, `1024px`, `large` |
| `256,1024` | `small`, `large` |

The web UI requests `/api/photos/:id/thumbnail/small` and falls back to the first entry.

### 2.5 Generation Strategy (per variant)

1. Determine the source with `display_source`; if there is none (redaction failed), stop without reading or writing.
2. Read the source (`fs_read`) and decode it once; apply the EXIF orientation.
3. Per variant: scale so that the longer side is at most `<size>` (no upscaling), encode as AVIF or JPEG with `quality`.
4. Write `<output_dir>/<cache_key>_<size>.<avif|jpg>` (`fs_write`) and add the entry with the real size.

Measured in the guest (ADR 0012): AVIF 128/512/1024 px takes ≈ 0.08/0.67/1.44 s per photo; 2048 px ≈ 9.4 s (the shipped profiles stop at 1024 px).

Failures produce a `warn` log entry and no entry — never a placeholder:

| Message | Cause |
| :--- | :--- |
| `thumbnail: '<path>': faces could not be redacted; no thumbnails (ADR 0012)` | `doc.privacy.face_redaction = failed` (fail closed) |
| `thumbnail: cannot read '<path>': …; no thumbnails` | file not readable / capability missing |
| `thumbnail: cannot decode '<path>': …; no thumbnails` | unsupported or broken image |
| `thumbnail: cannot write '<thumb>': …` | `fs_write` failed (only this variant is skipped) |

---

## 3. Pipeline Position

`thumbnail` runs **after `privacy-redact`**, at the end of the pipeline. While `privacy-redact` is enabled with `redact_faces` on, the host rejects a configuration in which `thumbnail` comes first (ADR 0012). The plugin does not know other plugins; it only reads `doc.privacy` and `doc.working_image` through `display_source`.

---

## 4. Required Capabilities

| Capability | Used for | Without the grant |
| :--- | :--- | :--- |
| `filesystem` | `fs_read` (source), `fs_write` (thumbnails) | Warning, no thumbnails. |

---

## 5. Data Model Output (`ThumbnailEntry`)

```rust
pub struct ThumbnailEntry {
    pub size_label: String,   // "small" | "medium" | "large"
    pub width: u32,           // measured, e.g. 128 for a 4:3 landscape photo
    pub height: u32,          // measured, e.g. 96
    pub path: String,         // "cache/thumbnails/<cache_key>_<size>.avif"
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
| `3f12502733adb127` | small | 128 | 96 | cache/thumbnails/aad58983bf531312_128.jpg | aad58983bf531312 |
| `3f12502733adb127` | medium | 512 | 384 | cache/thumbnails/6e982c5779c61d3b_512.jpg | 6e982c5779c61d3b |
| `3f12502733adb127` | large | 1024 | 768 | cache/thumbnails/6c2140244b0b7f61_1024.jpg | 6c2140244b0b7f61 |

---

## 7. Limitations & Implementation Notes

- **Older thumbnails:** documents processed before ADR 0012 keep their JPEG thumbnails, made from the unredacted image, until the photo is processed again.
- **HEIC/AVIF only via `image-convert`.** The plugin does not decode them itself; with `image-convert` in the pipeline it scales their JPEG working image ([ADR 0011](../../docs/adr/0011-working-image-pure-rust-decoders.md)). The former `sips` path (macOS only, outside the capability model) was removed ([#40](https://github.com/Zheng-Bote/wasm_geo-host/issues/40), [ADR 0002](../../docs/adr/0002-plugins-without-engine-no-output.md)).
- Thumbnails are regenerated on every run (no reuse of existing files).
- `DefaultHasher` is not guaranteed to be stable across Rust versions; cache keys can change after a toolchain update.
