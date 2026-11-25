use flagme_host::{AppConfig, Pipeline, PluginRuntime, Storage, StreamSupervisor};
use flagme_plugin_ai_tagging::AiTaggingPlugin;
use flagme_plugin_countryflag::CountryFlagPlugin;
use flagme_plugin_duplicate::DuplicatePlugin;
use flagme_plugin_metadata::MetadataPlugin;
use flagme_plugin_reversegeo::ReverseGeoPlugin;
use flagme_plugin_thumbnail::ThumbnailPlugin;
use flagme_plugin_timeline::TimelinePlugin;
use wasm_geo_models::{FileInfo, UnifiedDocument};
use std::path::Path;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: flagme-host [process|stream|search|similar] <FILE|QUERY|PHOTO_ID> [--config <CONFIG>] [--limit <N>]");
        std::process::exit(1);
    }

    let is_similar = args[1] == "similar";
    let is_search = args[1] == "search";
    let is_stream = args[1] == "stream";
    let is_process = args[1] == "process";

    if is_similar {
        if args.len() < 3 {
            eprintln!("Usage: flagme-host similar <PHOTO_ID> [--limit <N>] [--config <CONFIG>]");
            std::process::exit(1);
        }
        let photo_id = &args[2];
        let config_path = args
            .iter()
            .position(|arg| arg == "--config")
            .and_then(|idx| args.get(idx + 1))
            .map(|s| s.as_str())
            .unwrap_or("config.yaml");

        let limit: usize = args
            .iter()
            .position(|arg| arg == "--limit")
            .and_then(|idx| args.get(idx + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        let config = if Path::new(config_path).exists() {
            AppConfig::load_from_file(config_path)?
        } else {
            AppConfig::default()
        };

        let storage = Storage::new(&config.database_path);
        storage.init_schema()?;

        let results = storage.find_similar(photo_id, limit)?;
        println!("Vector Similarity Results for '{}': {} match(es)", photo_id, results.len());
        for (idx, res) in results.iter().enumerate() {
            println!("  [{}] photo_id: {} | cosine_score: {:.4}", idx + 1, res.photo_id, res.score);
        }
        return Ok(());
    }

    if is_search {
        if args.len() < 3 {
            eprintln!("Usage: flagme-host search <QUERY> [--config <CONFIG>]");
            std::process::exit(1);
        }
        let query = &args[2];
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

        let storage = Storage::new(&config.database_path);
        storage.init_schema()?;

        let results = storage.search(query)?;
        println!("FTS5 Search Results for '{}': {} match(es)", query, results.len());
        for (idx, res) in results.iter().enumerate() {
            println!("  [{}] photo_id: {} | filename: {} | bm25_rank: {:.4}", idx + 1, res.photo_id, res.filename, res.rank);
        }
        return Ok(());
    }

    let file_path = if is_process || is_stream {
        if args.len() < 3 {
            eprintln!("Usage: flagme-host [process|stream|search] <FILE|QUERY> [--config <CONFIG>]");
            std::process::exit(1);
        }
        &args[2]
    } else {
        &args[1]
    };

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

    if is_stream {
        let supervisor = StreamSupervisor::default_pipeline();
        let final_json = supervisor.execute(file_path)?;
        let doc: UnifiedDocument = serde_json::from_str(&final_json)?;
        storage.save_document(&doc)?;
        println!("{}", serde_json::to_string_pretty(&doc)?);
        return Ok(());
    }

    let mut runtime = PluginRuntime::new(config.clone());
    runtime.register_plugin(Arc::new(MetadataPlugin::new()));
    runtime.register_plugin(Arc::new(ReverseGeoPlugin::new()));
    runtime.register_plugin(Arc::new(CountryFlagPlugin::new()));
    runtime.register_plugin(Arc::new(ThumbnailPlugin::new()));
    runtime.register_plugin(Arc::new(AiTaggingPlugin::new()));
    runtime.register_plugin(Arc::new(DuplicatePlugin::new()));
    runtime.register_plugin(Arc::new(TimelinePlugin::new()));

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
