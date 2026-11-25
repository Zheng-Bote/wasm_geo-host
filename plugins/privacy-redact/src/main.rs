use flagme_plugin_privacy_redact::PrivacyRedactPlugin;
use flagme_sdk::{serde_json, NoopCapabilities, Plugin, PluginContext, UnifiedDocument};
use std::io::{self, Read};
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;

    if buffer.trim().is_empty() {
        let plugin = PrivacyRedactPlugin::new();
        println!("Plugin {} v{}", plugin.name(), plugin.version());
        return Ok(());
    }

    let doc: UnifiedDocument = serde_json::from_str(&buffer)?;
    let plugin = PrivacyRedactPlugin::new();
    let ctx = PluginContext::new(Arc::new(NoopCapabilities));

    let processed = plugin.process(doc, &ctx)?;
    println!("{}", serde_json::to_string(&processed)?);
    Ok(())
}
