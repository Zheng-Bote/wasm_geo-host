# Build Instructions — WASM Geo Host

## Prerequisites

- **Rust toolchain**: 1.80+ (`rustup default stable`)
- **WebAssembly target**: `rustup target add wasm32-wasip1` / `wasm32-wasip2`
- **Wasm Tools** (optional for Component Model composition): `cargo install wasm-tools`

## Building the Workspace

To compile all host crates and plugins:

```bash
cargo build --all
```

To run all tests:

```bash
cargo test --all
```

## Building WASM Components

To build each plugin as a WebAssembly component for sandboxed execution:

```bash
# Compile metadata plugin to WASI
cargo build --package flagme-plugin-metadata --target wasm32-wasip1 --release

# Compile reversegeo plugin to WASI
cargo build --package flagme-plugin-reversegeo --target wasm32-wasip1 --release

# Compile countryflag plugin to WASI
cargo build --package flagme-plugin-countryflag --target wasm32-wasip1 --release

# Compile thumbnail plugin to WASI
cargo build --package flagme-plugin-thumbnail --target wasm32-wasip1 --release

# Compile ai-tagging plugin to WASI
cargo build --package flagme-plugin-ai-tagging --target wasm32-wasip1 --release

# Compile duplicate plugin to WASI
cargo build --package flagme-plugin-duplicate --target wasm32-wasip1 --release

# Compile timeline plugin to WASI
cargo build --package flagme-plugin-timeline --target wasm32-wasip1 --release
```
