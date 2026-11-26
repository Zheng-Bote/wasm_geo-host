# Plugin Development Guide — WASM Geo Host

## Creating a New Plugin

Every WASM Geo Host plugin is an isolated component depending exclusively on `flagme-sdk`.

### 1. Define Cargo.toml

```toml
[package]
name = "flagme-plugin-mycustom"
version = "0.1.0"
edition = "2021"

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
