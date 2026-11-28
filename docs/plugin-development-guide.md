# Plugin Development Guide — WASM Geo Host

## Creating a New Plugin

Every WASM Geo Host plugin is an isolated component depending exclusively on `flagme-sdk`.

### 1. Define Cargo.toml

```toml
[package]
name = "flagme-plugin-mycustom"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[features]
# Export the `media-plugin` component (ADR 0003); build with `--lib --features component`.
component = []

[dependencies]
flagme-sdk = { path = "../../crates/sdk" }
```

### 2. Implement the Plugin Trait

```rust
use flagme_sdk::{Plugin, PluginContext, PluginResult, UnifiedDocument};

pub struct MyCustomPlugin;

impl Plugin for MyCustomPlugin {
    fn name(&self) -> &'static str {
        "mycustom"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        ctx.capabilities.log("info", "Processing in custom plugin");
        // Enrich document fields here...
        Ok(doc)
    }
}
```

Export it as a Wasm component at the end of `src/lib.rs`:

```rust
flagme_sdk::export_plugin!(MyCustomPlugin, || MyCustomPlugin);
```

The macro is active only on `wasm32` with the crate's `component` feature. It implements the `media-plugin` world of [`wit/media-plugin.wit`](../wit/media-plugin.wit): it converts the WIT `document` to `UnifiedDocument` and back, passes the `settings` from `config.yaml` as `ctx.settings` and routes `ctx.capabilities` to the host imports. Build and check:

```bash
cargo build --release --target wasm32-wasip2 --lib --features component -p flagme-plugin-mycustom
wasm-tools component wit target/wasm32-wasip2/release/flagme_plugin_mycustom.wasm
```

`name()` must return the plugin's key in `config.yaml`; the host rejects a component whose `name` differs. A new field in `UnifiedDocument` needs its counterpart in `wit/media-plugin.wit` and in both conversions (`crates/sdk/src/component.rs`, `crates/host/src/component.rs`); `crates/host/tests/component_runtime_test.rs` (AC-6) round-trips a fully populated document.

### 3. Request Host Capabilities in config.yaml

Register the plugin in `config.yaml`:

```yaml
plugins:
  mycustom:
    enabled: true
    capabilities:
      - filesystem
      - kv
```

Plugins access system resources **only** through `ctx.capabilities` (constitution *Security* 1–2): `fs_read`, `fs_write`, `fs_modified` (`filesystem`), `http_get` (`http`), `kv_get`/`kv_set` (`kv`), `secret_get`, `log`. Never use `std::fs`, `std::process` or external programs in a plugin; `crates/host/tests/capability_io_test.rs` checks this for every plugin under `plugins/*/src`. If a capability is denied, leave the affected field empty and log a warning ([ADR 0002](adr/0002-plugins-without-engine-no-output.md)).

### 4. Read pixels from the working image

A plugin that processes pixels reads the **working image**, not `doc.file.path` ([ADR 0011](adr/0011-working-image-pure-rust-decoders.md)):

```rust
let source = flagme_sdk::pixel_source(&doc); // working_image.path if set, otherwise file.path
let bytes = ctx.capabilities.fs_read(source)?;
```

`image-convert` runs right after `metadata` and turns JPEG, PNG, TIFF, BMP, WebP, HEIC/HEIF and AVIF into an upright baseline JPEG (orientation applied, ICC profile kept, no EXIF or GPS). A pixel plugin therefore only needs to decode JPEG; boxes and coordinates it writes refer to the working image. Without a working image (plugin disabled or conversion failed) `pixel_source` returns the original file, and the plugin handles only the formats it decodes itself. File-related values stay with the original: `metadata` reads EXIF and `duplicate` hashes the file bytes from `doc.file.path`.
