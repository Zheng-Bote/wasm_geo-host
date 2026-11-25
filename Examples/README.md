# WASM Geo Host — Sample Media & Execution Guide

This directory contains test images containing complete **EXIF**, **IPTC**, and **XMP** metadata across three image formats, along with execution instructions.

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

## 2. Building Plugins for WASI 0.2 (`wasm32-wasip2`)

All plugins in `plugins/` are WebAssembly Component libraries (`crate-type = ["cdylib", "rlib"]`) implementing the `wit/media-plugin.wit` specification.

To compile a plugin into a `.wasm` component:

```bash
# 1. Install wasip2 target if not already installed
rustup target add wasm32-wasip2

# 2. Build the plugin
cargo build --package flagme-plugin-metadata --target wasm32-wasip2 --release
```

The compiled component is generated at:
```text
target/wasm32-wasip2/release/flagme_plugin_metadata.wasm
```

---

## 3. Running Media Processing via Host Engine (`flagme-host`)

In the WebAssembly Component Model, plugins are **reactor libraries** (they export `process(UnifiedDocument)` and import host capabilities like filesystem, kv, and http). They are loaded and orchestrated by the host engine:

```bash
# Process sample.jpg through the full pipeline
cargo run --bin flagme-host -- process Examples/sample.jpg --config config.yaml

# Process sample.png
cargo run --bin flagme-host -- process Examples/sample.png --config config.yaml

# Process sample.heic
cargo run --bin flagme-host -- process Examples/sample.heic --config config.yaml
```

The command will:
1. Load and parse the media file.
2. Execute each configured plugin in sequence according to `config.yaml`.
3. Print the enriched `UnifiedDocument` JSON output to `stdout`.
4. Persist the records into the SQLite database (`media.db`).

---

## 4. Inspecting Components with Wasmtime & Wasm-Tools

Because these plugins export the custom WIT world `media-plugin` rather than the standalone `wasi:cli/run` command interface, you can inspect their component structure and disassembly using:

```bash
# Explore component disassembly
wasmtime explore target/wasm32-wasip2/release/flagme_plugin_metadata.wasm

# Inspect WIT interface with wasm-tools
wasm-tools component wit target/wasm32-wasip2/release/flagme_plugin_metadata.wasm
```
