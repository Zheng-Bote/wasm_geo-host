use flagme_host::{
    run_server, AppConfig, DirectoryWatcher, Pipeline, PluginRuntime, Storage, WatcherConfig,
};
use wasm_geo_models::{FileInfo, UnifiedDocument};
use std::path::Path;
use std::sync::Arc;

/// Emits host and plugin log messages on stderr; the level comes from `RUST_LOG` (default `warn`).
/// stdout stays reserved for the command output (e.g. the document JSON of `process`).
fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .try_init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: flagme-host [process|stream|search|similar] <FILE|QUERY|PHOTO_ID> [--config <CONFIG>] [--limit <N>]");
        std::process::exit(1);
    }

    let is_similar = args[1] == "similar";
    let is_search = args[1] == "search";
    let is_stream = args[1] == "stream";
    let is_process = args[1] == "process";
    let is_watch = args[1] == "watch";
    let is_serve = args[1] == "serve";

    if is_serve {
        let config_path = args
            .iter()
            .position(|arg| arg == "--config")
            .and_then(|idx| args.get(idx + 1))
            .map(|s| s.as_str())
            .unwrap_or("config.yaml");

        let port: u16 = args
            .iter()
            .position(|arg| arg == "--port")
            .and_then(|idx| args.get(idx + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(8080);

        let bind_addr = args
            .iter()
            .position(|arg| arg == "--bind")
            .and_then(|idx| args.get(idx + 1))
            .cloned()
            .unwrap_or_else(|| "0.0.0.0".to_string());

        let config = if Path::new(config_path).exists() {
            AppConfig::load_from_file(config_path)?
        } else {
            AppConfig::default()
        };

        let storage = Arc::new(Storage::new(&config.database_path));
        storage.init_schema()?;

        let socket_addr: std::net::SocketAddr = format!("{}:{}", bind_addr, port).parse()?;
        println!("Starting flagme-host embedded web server at http://{}", socket_addr);
        run_server(socket_addr, storage).await?;
        return Ok(());
    }

    if is_watch {
        if args.len() < 3 {
            eprintln!("Usage: flagme-host watch <DIR> [--target-dir <DIR>] [--config <CONFIG>] [--workers <N>] [--organize-by <PATTERN>] [--copy] [--once]");
            std::process::exit(1);
        }
        let watch_dir = std::path::PathBuf::from(&args[2]);

        let target_dir = args
            .iter()
            .position(|arg| arg == "--target-dir")
            .and_then(|idx| args.get(idx + 1))
            .map(std::path::PathBuf::from);

        let config_path = args
            .iter()
            .position(|arg| arg == "--config")
            .and_then(|idx| args.get(idx + 1))
            .map(|s| s.as_str())
            .unwrap_or("config.yaml");

        let workers: usize = args
            .iter()
            .position(|arg| arg == "--workers")
            .and_then(|idx| args.get(idx + 1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(4);

        let organize_pattern = args
            .iter()
            .position(|arg| arg == "--organize-by")
            .and_then(|idx| args.get(idx + 1))
            .cloned()
            .unwrap_or_else(|| "{year}/{month}/{country}/{city}".to_string());

        let copy_only = args.iter().any(|arg| arg == "--copy");
        // Process the existing files once, print the summary and exit (issue #71).
        let once = args.iter().any(|arg| arg == "--once");

        let config = if Path::new(config_path).exists() {
            AppConfig::load_from_file(config_path)?
        } else {
            AppConfig::default()
        };

        let storage = Arc::new(Storage::new(&config.database_path));
        storage.init_schema()?;

        let runtime = PluginRuntime::new(config.clone())?;

        let pipeline = Arc::new(Pipeline::new(config, runtime, storage.clone()));

        let watcher_config = WatcherConfig {
            watch_dir,
            target_dir,
            organize_pattern,
            copy_only,
            workers,
            once,
        };

        let watcher = DirectoryWatcher::new(watcher_config, pipeline, storage);
        if once {
            println!("Importing {:?} once (workers: {})", args[2], workers);
        } else {
            println!("Starting flagme-host watch daemon on {:?} (workers: {})", args[2], workers);
        }
        let report = watcher.run().await?;
        if once && report.failed > 0 {
            std::process::exit(1);
        }
        return Ok(());
    }

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

    // `stream` runs on the same embedded component runtime as `process`; every stage gets
    // its own store (ADR 0003).
    let runtime = PluginRuntime::new(config.clone())?;

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
