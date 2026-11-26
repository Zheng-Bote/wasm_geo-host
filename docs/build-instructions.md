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

## Release Build (`make`)

| Target | Result |
| :--- | :--- |
| `make release` | `check` + `test` + `binaries` + `verify` + `dist` |
| `make binaries` | `host`, `components`, `wasm-cli`, `native-cli` |
| `make host` | `target/release/flagme-host` |
| `make components` | `target/wasm32-wasip2/release/flagme_plugin_<name>.wasm` (feature `component`) |
| `make wasm-cli` | `target/wasm-cli/wasm32-wasip2/release/flagme-plugin-<name>.wasm` (own target directory, so the CLI build does not overwrite the components) |
| `make native-cli` | `target/release/flagme-plugin-<name>` |
| `make verify` | every component exports `process`, no CLI binary imports `host-capabilities` (needs `wasm-tools`) |
| `make dist` | `dist/v<version>/…` and `dist/packages/*.tar.gz` with `SHA256SUMS-v<version>` |

The version comes from `crates/host/Cargo.toml`. In the packages, `config.yaml` uses `plugin_dir: "plugins"` (relative to the working directory, so start `bin/flagme-host` from the unpacked directory).

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
