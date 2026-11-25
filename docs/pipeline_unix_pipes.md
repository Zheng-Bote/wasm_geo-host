# Pipeline Execution Model 2a — Unix CLI Streaming Pipeline

In this execution model, each WASM Geo Host plugin is compiled as an independent, standalone WebAssembly Component (`wasm32-wasip2`) that conforms to the Unix philosophy:
- **Eingabe (`stdin`)**: Reads `UnifiedDocument` JSON (or an initial image file argument for the first plugin).
- **Ausgabe (`stdout`)**: Writes the enriched `UnifiedDocument` JSON.
- **Diagnostics (`stderr`)**: Emits structured logging so stdout remains clean for streaming pipes.

---

## Architecture

```mermaid
flowchart LR
    photo["sample.jpg"] --> P1["flagme-plugin-metadata.wasm"]
    P1 -->|"JSON stdout | stdin"| P2["flagme-plugin-reversegeo.wasm"]
    P2 -->|"JSON stdout | stdin"| P3["flagme-plugin-countryflag.wasm"]
    P3 -->|"JSON stdout | stdin"| P4["flagme-plugin-thumbnail.wasm"]
    P4 -->|"JSON stdout | stdin"| P5["flagme-plugin-ai-tagging.wasm"]
    P5 -->|"JSON stdout | stdin"| P6["flagme-plugin-duplicate.wasm"]
    P6 -->|"JSON stdout | stdin"| P7["flagme-plugin-timeline.wasm"]
    P7 --> JQ["jq . / Output"]
```

---

## 1. Prerequisites & Compilation

Compile all plugins for the WASI 0.2 component target:

```bash
rustup target add wasm32-wasip2

cargo build --package flagme-plugin-metadata \
            --package flagme-plugin-reversegeo \
            --package flagme-plugin-countryflag \
            --package flagme-plugin-thumbnail \
            --package flagme-plugin-ai-tagging \
            --package flagme-plugin-duplicate \
            --package flagme-plugin-timeline \
            --target wasm32-wasip2 --release
```

All binaries are located in:
`target/wasm32-wasip2/release/`

---

## 2. Invocation Examples

### A. Minimal 3-Stage Pipeline (Metadata -> ReverseGeo -> CountryFlag)

Pass an image to `metadata.wasm`, pipe the enriched JSON through `reversegeo.wasm`, and enrich country emojis via `countryflag.wasm`:

```bash
wasmtime run -S cli-exit-with-code --dir Examples::/media \
    target/wasm32-wasip2/release/flagme-plugin-metadata.wasm /media/sample.jpg \
  | wasmtime run -S cli-exit-with-code \
    target/wasm32-wasip2/release/flagme-plugin-reversegeo.wasm \
  | wasmtime run -S cli-exit-with-code \
    target/wasm32-wasip2/release/flagme-plugin-countryflag.wasm \
  | jq .
```

### B. Complete 7-Stage End-to-End Streaming Pipeline

Chain all seven plugins sequentially through standard Unix pipes:

```bash
wasmtime run -S cli-exit-with-code --dir Examples::/media \
    target/wasm32-wasip2/release/flagme-plugin-metadata.wasm /media/sample.jpg \
  | wasmtime run -S cli-exit-with-code \
    target/wasm32-wasip2/release/flagme-plugin-reversegeo.wasm \
  | wasmtime run -S cli-exit-with-code \
    target/wasm32-wasip2/release/flagme-plugin-countryflag.wasm \
  | wasmtime run -S cli-exit-with-code \
    target/wasm32-wasip2/release/flagme-plugin-thumbnail.wasm \
  | wasmtime run -S cli-exit-with-code \
    target/wasm32-wasip2/release/flagme-plugin-ai-tagging.wasm \
  | wasmtime run -S cli-exit-with-code \
    target/wasm32-wasip2/release/flagme-plugin-duplicate.wasm \
  | wasmtime run -S cli-exit-with-code \
    target/wasm32-wasip2/release/flagme-plugin-timeline.wasm \
  > processed_media.json
```

---

## 3. Advantages & Trade-Offs

- **Advantages**:
  - Maximum decoupling: Any plugin can be substituted, tested, or debugged individually.
  - Polyglot: A single stage can be rewritten in Zig, Go, or C as long as it speaks JSON on stdin/stdout.
  - OS-Level Isolation: If one plugin crashes, the pipe terminates predictably without affecting a host daemon.
- **Trade-Offs**:
  - Process startup latency: Wasmtime initializes a new instance per pipeline stage.
  - JSON serialization overhead across pipe boundaries.
