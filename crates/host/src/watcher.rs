use chrono::Datelike;
use crate::pipeline::Pipeline;
use crate::storage::Storage;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::mpsc;
use wasm_geo_models::{FileInfo, UnifiedDocument};

static IN_FLIGHT_HASHES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn in_flight_set() -> &'static Mutex<HashSet<String>> {
    IN_FLIGHT_HASHES.get_or_init(|| Mutex::new(HashSet::new()))
}

struct InFlightGuard {
    sha256: String,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = in_flight_set().lock() {
            set.remove(&self.sha256);
        }
    }
}

/// Configuration options for DirectoryWatcher.
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    pub watch_dir: PathBuf,
    pub target_dir: Option<PathBuf>,
    pub organize_pattern: String,
    pub copy_only: bool,
    pub workers: usize,
    /// Process the existing files once and return instead of watching (`watch --once`).
    pub once: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            watch_dir: PathBuf::from("."),
            target_dir: None,
            organize_pattern: "{year}/{month}/{country}/{city}".to_string(),
            copy_only: false,
            workers: 4,
            once: false,
        }
    }
}

/// Helper for organizing ingested media into templated destination paths.
pub struct MediaOrganizer;

impl MediaOrganizer {
    /// Formats a relative destination path based on template pattern and document metadata.
    pub fn format_destination_path(doc: &UnifiedDocument, pattern: &str) -> PathBuf {
        // Only the capture time counts; the import time is not a capture time (ADR 0002).
        let (year_str, month_str) = match doc.timeline.as_ref().and_then(|tl| tl.taken_at) {
            Some(dt) => (dt.year().to_string(), format!("{:02}", dt.month())),
            None => ("Unknown_Year".to_string(), "00".to_string()),
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

        if source_path == dest_path {
            return Ok(dest_path);
        }

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if copy_only {
            fs::copy(source_path, &dest_path)?;
        } else {
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

/// Result of ingesting one file (issue #71).
#[derive(Debug, Clone)]
pub enum IngestOutcome {
    /// Run through the pipeline (and organized, if a target directory is set).
    Processed(Box<UnifiedDocument>),
    /// A file with the same SHA-256 is already stored; this file was left where it is.
    AlreadyIngested { id: String },
    /// Another worker is processing a file with the same content right now.
    InProgress,
    /// The file vanished between scan and processing, or it is empty.
    Gone,
}

/// Counts of the initial scan, returned by [`DirectoryWatcher::run`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WatchReport {
    pub found: usize,
    pub processed: usize,
    pub already: usize,
    pub ignored: usize,
    pub failed: usize,
}

/// Files below the watch directory, split into ingestible and ignored ones.
struct ScanResult {
    files: Vec<PathBuf>,
    ignored: Vec<PathBuf>,
}

/// A queued file; `initial` marks files of the initial scan.
struct Job {
    path: PathBuf,
    initial: bool,
}

/// Progress of the initial scan, shared by the workers.
struct ScanTally {
    report: WatchReport,
    remaining: usize,
    summary_printed: bool,
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

    /// Processes the existing files once, prints one summary line when they are done and, unless
    /// `once` is set, keeps watching for new files until [`Self::stop`]. Returns the counts of the
    /// initial scan.
    pub async fn run(&self) -> anyhow::Result<WatchReport> {
        let (tx, rx) = mpsc::channel::<Job>(1024);
        let rx = Arc::new(tokio::sync::Mutex::new(rx));

        // 1. Filesystem watcher for new files (not with `once`).
        let _watcher = if self.config.once {
            None
        } else {
            let tx_sync = tx.clone();
            let target_dir_filter = self.config.target_dir.clone();
            let mut watcher = RecommendedWatcher::new(
                move |res: notify::Result<Event>| {
                    if let Ok(event) = res {
                        if let EventKind::Create(_) | EventKind::Modify(_) = event.kind {
                            for path in event.paths {
                                if let Some(ref target) = target_dir_filter {
                                    if path.starts_with(target) {
                                        continue;
                                    }
                                }
                                if is_supported_media_file(&path) {
                                    let _ = tx_sync.blocking_send(Job { path, initial: false });
                                }
                            }
                        }
                    }
                },
                Config::default(),
            )?;
            // A missing directory is an error when watching (only `once` reports 0 files).
            watcher.watch(&self.config.watch_dir, RecursiveMode::Recursive)?;
            tracing::info!(dir = ?self.config.watch_dir, "Started directory watcher");
            Some(watcher)
        };

        // 2. Initial scan, counted before any file is processed.
        let scan = self.scan_existing_files();
        let tally = Arc::new(Mutex::new(ScanTally {
            report: WatchReport {
                found: scan.files.len() + scan.ignored.len(),
                ignored: scan.ignored.len(),
                ..Default::default()
            },
            remaining: scan.files.len(),
            summary_printed: false,
        }));
        if scan.files.is_empty() {
            self.print_summary(&tally);
        }

        // 3. Worker task pool (directory-watcher AC-2).
        let num_workers = self.config.workers.max(1);
        let mut worker_handles = Vec::with_capacity(num_workers);
        for worker_id in 0..num_workers {
            let rx = rx.clone();
            let pipeline = self.pipeline.clone();
            let storage = self.storage.clone();
            let config = self.config.clone();
            let running = self.running.clone();
            let tally = tally.clone();
            let watch_dir = self.config.watch_dir.clone();

            let handle = tokio::spawn(async move {
                tracing::debug!(worker_id, "Worker started");
                while running.load(Ordering::SeqCst) {
                    let job = {
                        let mut locked_rx = rx.lock().await;
                        match tokio::time::timeout(Duration::from_millis(200), locked_rx.recv()).await {
                            Ok(Some(job)) => Some(job),
                            Ok(None) => break, // channel closed and empty (`once`)
                            Err(_) => None,    // timeout, check `running`
                        }
                    };
                    if let Some(job) = job {
                        let result = process_single_file(&job.path, &pipeline, &storage, &config).await;
                        report_result(&job, &result, &tally, &watch_dir, config.once);
                    }
                }
                tracing::debug!(worker_id, "Worker stopped");
            });
            worker_handles.push(handle);
        }

        for path in scan.files {
            let _ = tx.send(Job { path, initial: true }).await;
        }

        if self.config.once {
            // Workers end when the queue is empty and closed.
            drop(tx);
            for handle in worker_handles {
                let _ = handle.await;
            }
        } else {
            while self.running.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            drop(tx);
            for handle in worker_handles {
                let _ = handle.await;
            }
        }

        let report = tally.lock().unwrap().report;
        Ok(report)
    }

    fn print_summary(&self, tally: &Arc<Mutex<ScanTally>>) {
        let mut t = tally.lock().unwrap();
        print_summary_line(&mut t, &self.config.watch_dir, self.config.once);
    }

    /// Lists the files below the watch directory (without the target directory).
    fn scan_existing_files(&self) -> ScanResult {
        let mut result = ScanResult { files: Vec::new(), ignored: Vec::new() };
        if !self.config.watch_dir.exists() {
            tracing::warn!(dir = ?self.config.watch_dir, "Watch directory does not exist; nothing to scan");
            return result;
        }

        let mut dirs = vec![self.config.watch_dir.clone()];
        while let Some(current_dir) = dirs.pop() {
            if let Some(ref target) = self.config.target_dir {
                if current_dir.starts_with(target) {
                    continue;
                }
            }
            if let Ok(entries) = fs::read_dir(current_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        dirs.push(path);
                    } else if is_supported_media_file(&path) {
                        result.files.push(path);
                    } else {
                        tracing::info!(path = ?path, "Ignoring file (hidden, derived or unsupported)");
                        result.ignored.push(path);
                    }
                }
            }
        }
        result.files.sort();
        result
    }
}

/// Prints the line of one file and counts it if it belongs to the initial scan.
fn report_result(
    job: &Job,
    result: &anyhow::Result<IngestOutcome>,
    tally: &Arc<Mutex<ScanTally>>,
    watch_dir: &Path,
    once: bool,
) {
    let name = job
        .path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| job.path.to_string_lossy().to_string());
    match result {
        Ok(IngestOutcome::Processed(doc)) => println!(
            "[watcher] Processed '{}' -> id={} ({})",
            doc.file.filename,
            doc.id,
            doc.location.as_ref().and_then(|l| l.city.as_deref()).unwrap_or("no-gps")
        ),
        Ok(IngestOutcome::AlreadyIngested { id }) => {
            println!("[watcher] Skipped '{}': already ingested as {} (not organized)", name, id)
        }
        Ok(IngestOutcome::InProgress) => println!("[watcher] Skipped '{}': same content is being processed", name),
        Ok(IngestOutcome::Gone) => println!("[watcher] Skipped '{}': file vanished or is empty", name),
        Err(e) => {
            tracing::error!(path = ?job.path, error = %e, "Failed to ingest media file");
            println!("[watcher] Failed '{}': {}", name, e);
        }
    }
    if !job.initial {
        return;
    }
    let mut t = tally.lock().unwrap();
    match result {
        Ok(IngestOutcome::Processed(_)) => t.report.processed += 1,
        Ok(_) => t.report.already += 1,
        Err(_) => t.report.failed += 1,
    }
    t.remaining = t.remaining.saturating_sub(1);
    if t.remaining == 0 {
        print_summary_line(&mut t, watch_dir, once);
    }
}

/// Prints the summary of the initial scan exactly once.
fn print_summary_line(t: &mut ScanTally, watch_dir: &Path, once: bool) {
    if t.summary_printed {
        return;
    }
    t.summary_printed = true;
    let r = t.report;
    let tail = if once { "done" } else { "watching for new files (Ctrl+C to stop)" };
    println!(
        "[watcher] Initial scan of {:?}: {} files: {} processed, {} already ingested (skipped), {} ignored (hidden/unsupported), {} failed; {}",
        watch_dir, r.found, r.processed, r.already, r.ignored, r.failed, tail
    );
}

/// Ingests a single media file through deduplication, pipeline execution, and file organization.
pub async fn process_single_file(
    file_path: &Path,
    pipeline: &Arc<Pipeline>,
    storage: &Arc<Storage>,
    config: &WatcherConfig,
) -> anyhow::Result<IngestOutcome> {
    if !file_path.exists() || !file_path.is_file() {
        return Ok(IngestOutcome::Gone);
    }

    // Read bytes & compute SHA-256
    let bytes = fs::read(file_path)?;
    if bytes.is_empty() {
        return Ok(IngestOutcome::Gone);
    }

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha256_hex = format!("{:x}", hasher.finalize());

    // Prevent concurrent duplicate processing of the same SHA-256 across worker threads
    {
        let mut in_flight = in_flight_set().lock().unwrap();
        if in_flight.contains(&sha256_hex) {
            return Ok(IngestOutcome::InProgress);
        }
        in_flight.insert(sha256_hex.clone());
    }
    let _guard = InFlightGuard {
        sha256: sha256_hex.clone(),
    };

    // directory-watcher AC-3: deduplication against the database
    if let Some(id) = storage.document_id_for_sha256(&sha256_hex) {
        tracing::info!(
            path = ?file_path,
            sha256 = %sha256_hex,
            id = %id,
            "Skipping duplicate file: already ingested"
        );
        return Ok(IngestOutcome::AlreadyIngested { id });
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
        working_image: None,
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

    // directory-watcher AC-2: execute the pipeline
    let mut processed_doc = pipeline.process(doc)?;
    if processed_doc.file.sha256.as_deref() != Some(&sha256_hex) {
        processed_doc.file.sha256 = Some(sha256_hex);
        let _ = storage.save_document(&processed_doc);
    }

    // directory-watcher AC-4: organize the processed file into the target directory
    if let Some(target_dir) = &config.target_dir {
        let new_path = MediaOrganizer::organize(
            file_path,
            &processed_doc,
            target_dir,
            &config.organize_pattern,
            config.copy_only,
        )?;
        processed_doc.file.path = new_path.to_string_lossy().to_string();
        let _ = storage.save_document(&processed_doc);
        tracing::info!(
            from = ?file_path,
            to = ?new_path,
            "Organized media file into target directory"
        );
    }

    Ok(IngestOutcome::Processed(Box::new(processed_doc)))
}

/// `true` for image files the watcher ingests; derived files (`.redacted.jpg`, `.working.jpg`,
/// anything below a `cache` directory) and hidden files are skipped.
pub fn is_supported_media_file(path: &Path) -> bool {
    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        if file_name.starts_with('.') || file_name.starts_with("._") || file_name.ends_with(".redacted.jpg")
            || file_name.ends_with(".working.jpg")
        {
            return false;
        }
    }

    // Skip cache directory artifacts
    if path.components().any(|c| c.as_os_str() == "cache") {
        return false;
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "webp" | "tiff" | "tif" | "bmp" | "heic" | "heif" | "avif"
    )
}

/// MIME type from the file extension.
pub fn detect_mime_type(path: &Path) -> String {
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
        "bmp" => "image/bmp".to_string(),
        "heic" => "image/heic".to_string(),
        "heif" => "image/heif".to_string(),
        "avif" => "image/avif".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}
