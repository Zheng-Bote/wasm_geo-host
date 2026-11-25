use flagme_plugin_privacy_redact::PrivacyRedactPlugin;
use flagme_sdk::Plugin;

fn main() {
    let plugin = PrivacyRedactPlugin::new();
    println!("Plugin {} v{}", plugin.name(), plugin.version());
}
