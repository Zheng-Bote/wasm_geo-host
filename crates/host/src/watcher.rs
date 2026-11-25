use chrono::Datelike;
use crate::pipeline::Pipeline;
use crate::storage::Storage;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use wasm_geo_models::{FileInfo, UnifiedDocument};

/// Configuration options for DirectoryWatcher.
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    pub watch_dir: PathBuf,
    pub target_dir: Option<PathBuf>,
    pub organize_pattern: String,
    pub copy_only: bool,
    pub workers: usize,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            watch_dir: PathBuf::from("."),
            target_dir: None,
            organize_pattern: "{year}/{month}/{country}/{city}".to_string(),
            copy_only: false,
            workers: 4,
        }
    }
}

/// Helper for organizing ingested media into templated destination paths.
pub struct MediaOrganizer;

impl MediaOrganizer {
    /// Formats a relative destination path based on template pattern and document metadata.
    pub fn format_destination_path(doc: &UnifiedDocument, pattern: &str) -> PathBuf {
        let (year_str, month_str) = if let Some(tl) = &doc.timeline {
            let dt = tl.taken_at.as_ref().unwrap_or(&tl.imported_at);
            (dt.year().to_string(), format!("{:02}", dt.month()))
        } else {
            ("Unknown_Year".to_string(), "00".to_string())
        };

        let country_str = if let Some(loc) = &doc.location {
            loc.country
                .as_deref()
                .or(loc.country_code.as_deref())
                .unwrap_or("Unknown_Country")
        } else {
            "Unknown_Country"
        };

        let city_str = if let Some(loc) = &doc.location {
            loc.city.as_deref().unwrap_or("Unknown_City")
        } else {
            "Unknown_City"
        };

        let sanitized_country = sanitize_path_segment(country_str);
        let sanitized_city = sanitize_path_segment(city_str);

        let replaced = pattern
            .replace("{year}", &year_str)
            .replace("{month}", &month_str)
            .replace("{country}", &sanitized_country)
            .replace("{city}", &sanitized_city);

        let filename = Path::new(&doc.file.filename)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("media.jpg");

        PathBuf::from(replaced).join(filename)
    }

    /// Moves or copies a processed media file into target directory using the template pattern.
    pub fn organize(
        source_path: &Path,
        doc: &UnifiedDocument,
        target_base: &Path,
        pattern: &str,
        copy_only: bool,
    ) -> anyhow::Result<PathBuf> {
        let rel_path = Self::format_destination_path(doc, pattern);
        let dest_path = target_base.join(rel_path);

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if copy_only {
            fs::copy(source_path, &dest_path)?;
        } else {
            // Try rename; fall back to copy + remove if across filesystem mounts
            if fs::rename(source_path, &dest_path).is_err() {
                fs::copy(source_path, &dest_path)?;
                let _ = fs::remove_file(source_path);
            }
        }

        Ok(dest_path)
    }
}

fn sanitize_path_segment(segment: &str) -> String {
    segment
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// Directory Watcher and concurrent batch ingestion orchestrator.
pub struct DirectoryWatcher {
    config: WatcherConfig,
    pipeline: Arc<Pipeline>,
    storage: Arc<Storage>,
    running: Arc<AtomicBool>,
}

impl DirectoryWatcher {
    pub fn new(config: WatcherConfig, pipeline: Arc<Pipeline>, storage: Arc<Storage>) -> Self {
        Self {
            config,
            pipeline,
            storage,
            running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Starts watching the configured directory and dispatches events to the worker pool.
    pub async fn run(&self) -> anyhow::Result<()> {
        let (tx, rx) = mpsc::channel::<PathBuf>(1024);
        let rx = Arc::new(tokio::sync::Mutex::new(rx));
        let tx_sync = tx.clone();

        // 1. Setup notify event watcher (AC-1)
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                if let Ok(event) = res {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            for path in event.paths {
                                if is_supported_media_file(&path) {
                                    let _ = tx_sync.blocking_send(path);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            },
            Config::default(),
        )?;

        watcher.watch(&self.config.watch_dir, RecursiveMode::Recursive)?;
        tracing::info!(dir = ?self.config.watch_dir, "Started directory watcher");

        // 2. Also scan existing files in directory initially
        self.scan_existing_files(&tx).await?;

        // 3. Worker task pool (AC-2)
        let num_workers = self.config.workers.max(1);
        let mut worker_handles = Vec::with_capacity(num_workers);

        for worker_id in 0..num_workers {
            let rx = rx.clone();
            let pipeline = self.pipeline.clone();
            let storage = self.storage.clone();
            let config = self.config.clone();
            let running = self.running.clone();

            let handle = tokio::spawn(async move {
                tracing::debug!(worker_id, "Worker started");
                while running.load(Ordering::SeqCst) {
                    let maybe_path = {
                        let mut locked_rx = rx.lock().await;
                        match tokio::time::timeout(Duration::from_millis(200), locked_rx.recv()).await {
                            Ok(Some(p)) => Some(p),
                            Ok(None) => break, // Channel closed
                            Err(_) => None,    // Timeout check
                        }
                    };

                    if let Some(path) = maybe_path {
                        if let Err(e) = process_single_file(&path, &pipeline, &storage, &config).await {
                            tracing::error!(path = ?path, error = %e, "Failed to ingest media file");
                        }
                    }
                }
                tracing::debug!(worker_id, "Worker stopped");
            });
            worker_handles.push(handle);
        }

        // 4. Wait while running
        while self.running.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }

        // Cleanup
        drop(tx);
        for handle in worker_handles {
            let _ = handle.await;
        }

        Ok(())
    }

    /// Scans pre-existing media files in the watch directory upon startup.
    async fn scan_existing_files(&self, tx: &mpsc::Sender<PathBuf>) -> anyhow::Result<()> {
        if !self.config.watch_dir.exists() {
            return Ok(());
        }

        let mut dirs = vec![self.config.watch_dir.clone()];
        while let Some(current_dir) = dirs.pop() {
            if let Ok(entries) = fs::read_dir(current_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        dirs.push(path);
                    } else if is_supported_media_file(&path) {
                        let _ = tx.send(path).await;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Ingests a single media file through deduplication, pipeline execution, and file organization.
pub async fn process_single_file(
    file_path: &Path,
    pipeline: &Arc<Pipeline>,
    storage: &Arc<Storage>,
    config: &WatcherConfig,
) -> anyhow::Result<Option<UnifiedDocument>> {
    if !file_path.exists() || !file_path.is_file() {
        return Ok(None);
    }

    // Read bytes & compute SHA-256
    let bytes = fs::read(file_path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha256_hex = format!("{:x}", hasher.finalize());

    // AC-3: Deduplication check against SQLite database
    if storage.has_sha256(&sha256_hex) {
        tracing::info!(
            path = ?file_path,
            sha256 = %sha256_hex,
            "Skipping duplicate file: already ingested"
        );
        return Ok(None);
    }

    let filename = file_path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("media.jpg")
        .to_string();

    let doc = UnifiedDocument {
        id: format!("doc_{}", &sha256_hex[..12]),
        file: FileInfo {
            path: file_path.to_string_lossy().to_string(),
            filename,
            size_bytes: bytes.len() as u64,
            mime_type: detect_mime_type(file_path),
            sha256: Some(sha256_hex.clone()),
        },
        metadata: None,
        location: None,
        country: None,
        thumbnails: Vec::new(),
        ai: None,
        duplicate: None,
        timeline: None,
        weather: None,
        ocr: None,
        faces: None,
        privacy: None,
        custom: std::collections::HashMap::new(),
    };

    // AC-2: Execute pipeline
    let mut processed_doc = pipeline.process(doc)?;
    // Ensure the true sha256_hex is preserved in the processed doc and storage
    if processed_doc.file.sha256.as_deref() != Some(&sha256_hex) {
        processed_doc.file.sha256 = Some(sha256_hex);
        let _ = storage.save_document(&processed_doc);
    }

    // AC-4: Organize processed file into target directory if configured
    if let Some(target_dir) = &config.target_dir {
        let new_path = MediaOrganizer::organize(
            file_path,
            &processed_doc,
            target_dir,
            &config.organize_pattern,
            config.copy_only,
        )?;
        tracing::info!(
            from = ?file_path,
            to = ?new_path,
            "Organized media file into target directory"
        );
    }

    Ok(Some(processed_doc))
}

fn is_supported_media_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "webp" | "tiff" | "tif" | "heic" | "avif"
    )
}

fn detect_mime_type(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "png" => "image/png".to_string(),
        "webp" => "image/webp".to_string(),
        "tiff" | "tif" => "image/tiff".to_string(),
        "heic" => "image/heic".to_string(),
        "avif" => "image/avif".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}
