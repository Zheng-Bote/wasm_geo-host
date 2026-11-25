# Build Instructions — WASM Geo Host

## Prerequisites

- **Rust toolchain**: 1.80+ (`rustup default stable`)
- **WebAssembly target**: `rustup target add wasm32-wasip2`
- **Wasm Tools** (optional for Component Model inspection/composition): `cargo install wasm-tools`

## Building the Workspace

To compile all host crates and plugins:

```bash
cargo build --all
```

To run all tests:

```bash
cargo test --all
```

## Building WASM Components (WASI 0.2 / Component Model)

To build each plugin as a native WebAssembly Component (`wasm32-wasip2`) for sandboxed execution:

```bash
# Compile metadata plugin to WASI 0.2 Component
cargo build --package flagme-plugin-metadata --target wasm32-wasip2 --release

# Compile reversegeo plugin to WASI 0.2 Component
cargo build --package flagme-plugin-reversegeo --target wasm32-wasip2 --release

# Compile countryflag plugin to WASI 0.2 Component
cargo build --package flagme-plugin-countryflag --target wasm32-wasip2 --release

# Compile thumbnail plugin to WASI 0.2 Component
cargo build --package flagme-plugin-thumbnail --target wasm32-wasip2 --release

# Compile ai-tagging plugin to WASI 0.2 Component
cargo build --package flagme-plugin-ai-tagging --target wasm32-wasip2 --release

# Compile duplicate plugin to WASI 0.2 Component
cargo build --package flagme-plugin-duplicate --target wasm32-wasip2 --release

# Compile timeline plugin to WASI 0.2 Component
cargo build --package flagme-plugin-timeline --target wasm32-wasip2 --release
```
