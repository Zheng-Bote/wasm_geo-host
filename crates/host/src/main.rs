use flagme_host::{AppConfig, Pipeline, PluginRuntime, Storage};
use wasm_geo_models::{FileInfo, UnifiedDocument};
use std::path::Path;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: flagme-host process <FILE> [--config <CONFIG>]");
        std::process::exit(1);
    }

    let file_path = &args[1];
    let config_path = args
        .iter()
        .position(|arg| arg == "--config")
        .and_then(|idx| args.get(idx + 1))
        .map(|s| s.as_str())
        .unwrap_or("config.yaml");

    let config = if Path::new(config_path).exists() {
        AppConfig::load_from_file(config_path)?
    } else {
        AppConfig::default()
    };

    let storage = Arc::new(Storage::new(&config.database_path));
    storage.init_schema()?;

    let runtime = PluginRuntime::new(config.clone());
    let pipeline = Pipeline::new(config, runtime, storage);

    let file_meta = std::fs::metadata(file_path)?;
    let filename = Path::new(file_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let doc = UnifiedDocument::new(
        uuid_like_id(file_path),
        FileInfo {
            path: file_path.to_string(),
            filename,
            size_bytes: file_meta.len(),
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let processed = pipeline.process(doc)?;
    println!("{}", serde_json::to_string_pretty(&processed)?);

    Ok(())
}

fn uuid_like_id(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    hex[0..16].to_string()
}
