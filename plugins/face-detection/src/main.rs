use flagme_plugin_face_detection::FaceDetectionPlugin;
use flagme_sdk::Plugin;

fn main() {
    let plugin = FaceDetectionPlugin::new();
    println!("Plugin {} v{}", plugin.name(), plugin.version());
}
