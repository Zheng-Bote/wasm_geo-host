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
