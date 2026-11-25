# WASM Geo Host — Sample Media & Wasmtime Invocation Guide

This directory contains test images containing complete **EXIF**, **IPTC**, and **XMP** metadata across three image formats, along with instructions on running plugins and inspecting components directly via `wasmtime`.

---

## 1. Sample Media Files

All sample files represent an identical media item captured in Berlin, Germany:

| File | Format | Embedded Metadata |
|---|---|---|
| [`sample.jpg`](sample.jpg) | JPEG | EXIF (APP1), IPTC (APP13 / 8BIM), XMP (APP1 RDF) |
| [`sample.png`](sample.png) | PNG | EXIF (`eXIf` chunk), XMP (`iTXt` chunk), IPTC (`tEXt` chunk) |
| [`sample.heic`](sample.heic) | HEIC (ISOBMFF) | EXIF (`idat` box), XMP (RDF XML packet in `idat`), ISOBMFF `meta` |

### Embedded Metadata Attributes
- **EXIF**:
  - `Camera Make`: `Sony`
  - `Camera Model`: `ILCE-7RM4`
  - `Lens`: `FE 24-70mm F2.8 GM`
  - `DateTimeOriginal`: `2026:11:25 10:00:00`
  - `GPS Coordinates`: `52.520008 N, 13.404954 E, Alt: 34.0m` (Berlin, Germany)
- **IPTC**:
  - `Headline / Object Name`: `WASM Geo Sample`
  - `Byline / Creator`: `WASM Geo Platform`
  - `Country`: `Germany`
  - `Caption`: `Testing EXIF, IPTC and XMP metadata extraction`
- **XMP**:
  - `dc:title`: `WASM Geo Sample`
  - `dc:creator`: `WASM Geo Platform`
  - `photoshop:City`: `Berlin`
  - `photoshop:Country`: `Germany`
  - `xmp:CreateDate`: `2026-11-25T10:00:00Z`

---

## 2. Building WASI 0.2 Components

Before invoking plugins with `wasmtime`, compile them for the native WebAssembly Component Model target (`wasm32-wasip2`):

```bash
# Ensure target is installed
rustup target add wasm32-wasip2

# Compile plugins to release components
cargo build --package flagme-plugin-metadata --target wasm32-wasip2 --release
cargo build --package flagme-plugin-thumbnail --target wasm32-wasip2 --release
cargo build --package flagme-plugin-reversegeo --target wasm32-wasip2 --release
cargo build --package flagme-plugin-duplicate --target wasm32-wasip2 --release
```

Compiled components are located in:
`target/wasm32-wasip2/release/`

---

## 3. Direct Wasmtime Invocations

`wasmtime` (version 20+) executes WebAssembly Components directly with capability-based security isolation.

### A. Inspecting the Component WIT Interface

Inspect exports, imports, and WIT interfaces of the compiled component:

```bash
wasmtime component wit target/wasm32-wasip2/release/flagme_plugin_metadata.wasm
```

Expected output:
```wit
package wasmgeohost:media;

world media-plugin {
  import host-capabilities;
  export name: func() -> string;
  export version: func() -> string;
  export process: func(doc: document) -> result<document, string>;
}
```

---

### B. Invoking with JPEG Sample (`sample.jpg`)

Run `wasmtime` granting sandboxed, read-only filesystem access strictly to the `Examples` directory:

```bash
# Grant access only to the Examples directory mapped to /media in guest
wasmtime run \
  --wasm component-model \
  --dir Examples::/media \
  target/wasm32-wasip2/release/flagme_plugin_metadata.wasm /media/sample.jpg
```

---

### C. Invoking with PNG Sample (`sample.png`)

```bash
wasmtime run \
  --wasm component-model \
  --dir Examples::/media \
  target/wasm32-wasip2/release/flagme_plugin_metadata.wasm /media/sample.png
```

---

### D. Invoking with HEIC Sample (`sample.heic`)

```bash
wasmtime run \
  --wasm component-model \
  --dir Examples::/media \
  target/wasm32-wasip2/release/flagme_plugin_metadata.wasm /media/sample.heic
```

---

### E. Capability Isolation Testing (Security Verification)

To test capability denial (verifying that unauthorized filesystem or network operations are blocked by Wasmtime):

```bash
# 1. Block all network access (offline isolation)
wasmtime run \
  --wasm component-model \
  --net=none \
  --dir Examples::/media \
  target/wasm32-wasip2/release/flagme_plugin_countryflag.wasm

# 2. Restrict guest memory to 256MB (matching config.yaml max_memory_bytes)
wasmtime run \
  --wasm component-model \
  --max-wasm-stack 2097152 \
  --dir Examples::/media \
  target/wasm32-wasip2/release/flagme_plugin_thumbnail.wasm /media/sample.jpg
```

---

## 4. End-to-End Execution via Host Engine

To pass these sample files through the complete multi-stage pipeline (Metadata → ReverseGeo → CountryFlag → Thumbnail → AI-Tagging → Duplicate → Timeline → SQLite):

```bash
# 1. Process sample.jpg
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml

# 2. Process sample.png
cargo run --bin flagme-host -- process Examples/sample.png --config config.yaml

# 3. Process sample.heic
cargo run --bin flagme-host -- process Examples/sample.heic --config config.yaml
```

The output will print the enriched JSON document, and all metadata, locations, and thumbnail cache keys will be saved in `media.db`.
