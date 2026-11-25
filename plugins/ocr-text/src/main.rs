use flagme_plugin_ocr_text::OcrTextPlugin;
use flagme_sdk::Plugin;

fn main() {
    let plugin = OcrTextPlugin::new();
    println!("Plugin {} v{}", plugin.name(), plugin.version());
}
