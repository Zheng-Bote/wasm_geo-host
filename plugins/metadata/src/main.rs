use flagme_plugin_metadata::MetadataPlugin;
use flagme_sdk::{
    serde_json, FileInfo, NoopCapabilities, Plugin, PluginContext, UnifiedDocument,
};
use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let plugin = MetadataPlugin::new();
    let ctx = PluginContext::new(Arc::new(NoopCapabilities));

    let doc = if args.len() > 1 && !args[1].starts_with('{') {
        let path_str = &args[1];
        let p = Path::new(path_str);
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let filename = p.file_name().unwrap_or_default().to_string_lossy().to_string();

        UnifiedDocument::new(
            "cli-doc",
            FileInfo {
                path: path_str.clone(),
                filename,
                size_bytes: size,
                mime_type: "image/jpeg".to_string(),
                sha256: None,
            },
        )
    } else {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        serde_json::from_str::<UnifiedDocument>(&buffer)?
    };

    let processed = plugin.process(doc, &ctx)?;
    println!("{}", serde_json::to_string(&processed)?);
    Ok(())
}
