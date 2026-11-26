# Build Instructions — WASM Geo Host

## Prerequisites

- **Rust toolchain**: 1.97+ (`rustup default stable`; required by `wasmtime` 51)
- **WebAssembly target**: `rustup target add wasm32-wasip2`
- **Wasm Tools** (optional for Component Model inspection/composition): `cargo install wasm-tools`

## Building the Workspace

To compile all host crates and plugins:

```bash
cargo build --all
```

To run all tests (the host tests build the plugin components first, see below):

```bash
cargo test --workspace
```

## Building WASM Components (WASI 0.2 / Component Model)

`flagme-host` runs plugins only as Wasm components ([ADR 0003](adr/0003-wasm-component-execution.md)). Build all of them with the `component` feature:

```bash
cargo build --release --target wasm32-wasip2 --lib --features component \
  -p flagme-plugin-metadata -p flagme-plugin-reversegeo -p flagme-plugin-countryflag \
  -p flagme-plugin-thumbnail -p flagme-plugin-ai-tagging -p flagme-plugin-duplicate \
  -p flagme-plugin-timeline -p flagme-plugin-weather-enrich -p flagme-plugin-ocr-text \
  -p flagme-plugin-face-detection -p flagme-plugin-privacy-redact
```

Output: `target/wasm32-wasip2/release/flagme_plugin_<name>.wasm`, the default `plugin_dir` of `config.yaml`. Check a component with `wasm-tools component wit target/wasm32-wasip2/release/flagme_plugin_timeline.wasm` — it exports `name`, `version` and `process`.

The standalone CLI binaries (`flagme-plugin-<name>.wasm`, run with `wasmtime run`) are built **without** the feature, e.g. `cargo build --release --target wasm32-wasip2 -p flagme-plugin-timeline --bin flagme-plugin-timeline`.
