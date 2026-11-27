//! Host side of the `media-plugin` world (`wit/media-plugin.wit`, ADR 0003).
//!
//! Every enabled plugin of the pipeline is loaded from its component file (`PluginConfig.path`),
//! compiled once and instantiated per call in a fresh [`Store`]. The guest gets no preopened
//! directories, sockets, environment or stdin; all I/O goes through `host-capabilities`, which
//! the [`CapabilityManager`] checks per plugin. Memory is capped by a [`ResourceLimiter`]
//! (`max_memory_bytes`), run time by epoch interruption (`timeout_ms`).
//!
//! Compiled code is kept in wasmtime's own cache (ADR 0004), in a directory that only the host
//! user may write; otherwise components are compiled on every start.

use crate::capabilities::CapabilityManager;
use crate::config::ResourceLimits;
use flagme_sdk::{HostCapabilities, PluginError, PluginResult};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wasm_geo_models as m;
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Cache, CacheConfig, Config, Engine, ResourceLimiter, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

mod bindings {
    wasmtime::component::bindgen!({
        world: "media-plugin",
        path: "../../wit",
    });
}

use bindings::wasmgeohost::media::host_capabilities as host;
use bindings::wasmgeohost::media::types as w;
pub use bindings::MediaPluginPre;

/// Tick of the epoch timer that enforces `timeout_ms`.
const EPOCH_TICK: Duration = Duration::from_millis(10);

/// Caps the linear memory of a guest at `max_memory_bytes` and records a violation.
struct MemoryLimiter {
    max_bytes: usize,
    exceeded: bool,
}

impl ResourceLimiter for MemoryLimiter {
    fn memory_growing(&mut self, _current: usize, desired: usize, _maximum: Option<usize>) -> wasmtime::Result<bool> {
        if desired > self.max_bytes {
            self.exceeded = true;
            wasmtime::bail!("guest memory of {} bytes exceeds max_memory_bytes ({} bytes)", desired, self.max_bytes);
        }
        Ok(true)
    }

    fn table_growing(&mut self, _current: usize, _desired: usize, _maximum: Option<usize>) -> wasmtime::Result<bool> {
        Ok(true)
    }
}

/// Per-call state of a guest: WASI context without ambient authority, the plugin's
/// capability manager and the memory limiter.
pub struct HostState {
    wasi: WasiCtx,
    table: ResourceTable,
    caps: CapabilityManager,
    limiter: MemoryLimiter,
    warned: Arc<Mutex<HashSet<String>>>,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

impl w::Host for HostState {}

impl host::Host for HostState {
    fn fs_read(&mut self, path: String) -> Result<Vec<u8>, w::PluginError> {
        self.caps.fs_read(&path).map_err(Into::into)
    }
    fn fs_write(&mut self, path: String, data: Vec<u8>) -> Result<(), w::PluginError> {
        self.caps.fs_write(&path, &data).map_err(Into::into)
    }
    fn fs_modified(&mut self, path: String) -> Result<u64, w::PluginError> {
        self.caps.fs_modified(&path).map_err(Into::into)
    }
    fn http_get(&mut self, url: String) -> Result<String, w::PluginError> {
        self.caps.http_get(&url).map_err(Into::into)
    }
    fn kv_get(&mut self, key: String) -> Result<Option<String>, w::PluginError> {
        self.caps.kv_get(&key).map_err(Into::into)
    }
    fn kv_set(&mut self, key: String, value: String) -> Result<(), w::PluginError> {
        self.caps.kv_set(&key, &value).map_err(Into::into)
    }
    fn secret_get(&mut self, name: String) -> Result<Option<String>, w::PluginError> {
        self.caps.secret_get(&name).map_err(Into::into)
    }
    fn log_event(&mut self, level: String, message: String) {
        // Each call runs in a fresh instance, so the guest's `SettingsCache` cannot suppress
        // repeated settings warnings; the host keeps them to once per plugin and message.
        if message.starts_with("settings[") {
            let mut warned = match self.warned.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            if !warned.insert(message.clone()) {
                return;
            }
        }
        self.caps.log(&level, &message);
    }
}

/// Hits and misses of the component cache since the engine was created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    /// Components loaded from the cache.
    pub hits: usize,
    /// Components compiled and written to the cache.
    pub misses: usize,
}

/// Shared Wasmtime engine with the epoch timer for `timeout_ms`.
pub struct ComponentEngine {
    engine: Engine,
    linker: Linker<HostState>,
    stop: Arc<AtomicBool>,
    cache: Option<Cache>,
}

impl ComponentEngine {
    /// Creates the engine. With `cache_dir`, compiled components are cached there if the
    /// directory passes the checks of ADR 0004; otherwise the host warns and compiles on every
    /// start.
    pub fn new(cache_dir: Option<&Path>) -> anyhow::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.epoch_interruption(true);
        let cache = match cache_dir {
            None => {
                tracing::debug!("Component cache disabled by configuration");
                None
            }
            Some(dir) => match open_cache(dir) {
                Ok(cache) => {
                    tracing::debug!(dir = %cache.directory().display(), "Component cache enabled");
                    Some(cache)
                }
                Err(reason) => {
                    tracing::warn!("component cache disabled: '{}': {}", dir.display(), reason);
                    None
                }
            },
        };
        config.cache(cache.clone());
        let engine = Engine::new(&config)?;

        let mut linker = Linker::<HostState>::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        bindings::MediaPlugin::add_to_linker::<HostState, HasSelf<HostState>>(&mut linker, |s| s)?;

        let stop = Arc::new(AtomicBool::new(false));
        let (ticker_engine, ticker_stop) = (engine.clone(), stop.clone());
        std::thread::Builder::new()
            .name("flagme-epoch".to_string())
            .spawn(move || {
                while !ticker_stop.load(Ordering::Relaxed) {
                    std::thread::sleep(EPOCH_TICK);
                    ticker_engine.increment_epoch();
                }
            })?;

        Ok(Self { engine, linker, stop, cache })
    }

    /// Cache hits and misses, or `None` if the cache is off.
    pub fn cache_stats(&self) -> Option<CacheStats> {
        self.cache.as_ref().map(|c| CacheStats { hits: c.cache_hits(), misses: c.cache_misses() })
    }

    /// Compiles a component file and checks that it implements `media-plugin`.
    pub fn load(&self, path: &Path) -> anyhow::Result<MediaPluginPre<HostState>> {
        let component = Component::from_file(&self.engine, path)
            .map_err(|e| anyhow::anyhow!("cannot load component '{}': {}", path.display(), e))?;
        let pre = self.linker.instantiate_pre(&component).map_err(|e| {
            anyhow::anyhow!("component '{}' does not match the media-plugin world: {}", path.display(), e)
        })?;
        MediaPluginPre::new(pre).map_err(|e| {
            anyhow::anyhow!(
                "component '{}' does not export the media-plugin world ({}); build it with \
                 `cargo build --release --target wasm32-wasip2 --lib --features component`",
                path.display(),
                e
            )
        })
    }

    fn store(&self, caps: CapabilityManager, limits: &ResourceLimits, warned: Arc<Mutex<HashSet<String>>>) -> Store<HostState> {
        let wasi = WasiCtxBuilder::new()
            .inherit_stderr()
            .allow_tcp(false)
            .allow_udp(false)
            .allow_ip_name_lookup(false)
            .build();
        let memory = usize::try_from(limits.max_memory_bytes).unwrap_or(usize::MAX);
        let state = HostState {
            wasi,
            table: ResourceTable::new(),
            caps,
            limiter: MemoryLimiter { max_bytes: memory, exceeded: false },
            warned,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|s| &mut s.limiter);
        let ticks = limits.timeout_ms.div_ceil(EPOCH_TICK.as_millis() as u64).max(1);
        store.set_epoch_deadline(ticks);
        store.epoch_deadline_trap();
        store
    }

    /// Returns the `name` and `version` exports of a component.
    pub fn component_identity(
        &self,
        pre: &MediaPluginPre<HostState>,
        limits: &ResourceLimits,
    ) -> anyhow::Result<(String, String)> {
        let caps = CapabilityManager::new("<load>", Vec::new(), Default::default());
        let mut store = self.store(caps, limits, Default::default());
        let plugin = pre.instantiate(&mut store).map_err(|e| anyhow::anyhow!("{:#}", e))?;
        let name = plugin.call_name(&mut store).map_err(|e| anyhow::anyhow!("{:#}", e))?;
        let version = plugin.call_version(&mut store).map_err(|e| anyhow::anyhow!("{:#}", e))?;
        Ok((name, version))
    }

    /// Runs `process` of a component in a fresh store.
    pub fn process(
        &self,
        pre: &MediaPluginPre<HostState>,
        caps: CapabilityManager,
        limits: &ResourceLimits,
        warned: Arc<Mutex<HashSet<String>>>,
        doc: m::UnifiedDocument,
        settings: &HashMap<String, String>,
    ) -> PluginResult<m::UnifiedDocument> {
        let mut store = self.store(caps, limits, warned);
        let plugin = pre
            .instantiate(&mut store)
            .map_err(|e| PluginError::ProcessingFailed(format!("instantiation failed: {}", e)))?;
        let mut settings: Vec<(String, String)> = settings.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        settings.sort();
        let result = plugin
            .call_process(&mut store, &document_to_wit(doc), &settings)
            .map_err(|e| PluginError::ProcessingFailed(guest_failure(&e, store.data().limiter.exceeded, limits)))?;
        document_from_wit(result.map_err(PluginError::from)?)
    }
}

impl Drop for ComponentEngine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Checks the cache directory and builds wasmtime's cache on it.
fn open_cache(dir: &Path) -> Result<Cache, String> {
    let dir = prepare_cache_dir(dir)?;
    let mut config = CacheConfig::new();
    config.with_directory(dir);
    Cache::new(config).map_err(|e| format!("{:#}", e))
}

/// Makes `dir` absolute, creates it with mode `0700` if missing and accepts it only if it is a
/// real directory owned by the host user that neither group nor others can write (ADR 0004).
#[cfg(unix)]
fn prepare_cache_dir(dir: &Path) -> Result<PathBuf, String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};

    // Not canonicalized: a symbolic link must not be resolved before it is rejected.
    let dir = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        std::env::current_dir().map_err(|e| format!("no working directory: {}", e))?.join(dir)
    };
    let meta = match std::fs::symlink_metadata(&dir) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = dir.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("cannot create parent directory: {}", e))?;
            }
            match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("cannot create: {}", e)),
            }
            std::fs::symlink_metadata(&dir).map_err(|e| format!("cannot inspect: {}", e))?
        }
        Err(e) => return Err(format!("cannot inspect: {}", e)),
    };
    if meta.file_type().is_symlink() {
        return Err("is a symbolic link".to_string());
    }
    if !meta.is_dir() {
        return Err("is not a directory".to_string());
    }
    let host_uid = rustix::process::geteuid().as_raw();
    if meta.uid() != host_uid {
        return Err(format!("is owned by uid {}, not by the host user (uid {})", meta.uid(), host_uid));
    }
    if meta.mode() & 0o022 != 0 {
        return Err(format!("is writable by group or others (mode {:o})", meta.mode() & 0o7777));
    }
    Ok(dir)
}

#[cfg(not(unix))]
fn prepare_cache_dir(_dir: &Path) -> Result<PathBuf, String> {
    Err("owner and permissions cannot be checked on this platform".to_string())
}

/// Describes a failed guest call, naming the exceeded limit if there is one.
fn guest_failure(e: &wasmtime::Error, memory_exceeded: bool, limits: &ResourceLimits) -> String {
    if memory_exceeded {
        format!("guest exceeded max_memory_bytes ({} bytes)", limits.max_memory_bytes)
    } else if matches!(e.downcast_ref::<wasmtime::Trap>(), Some(wasmtime::Trap::Interrupt)) {
        format!("guest exceeded timeout_ms ({} ms)", limits.timeout_ms)
    } else {
        format!("guest trapped: {:#}", e)
    }
}

// ---------------------------------------------------------------------------------------
// Errors

impl From<w::PluginError> for PluginError {
    fn from(e: w::PluginError) -> Self {
        match e {
            w::PluginError::Serialization(s) => {
                PluginError::Serialization(serde_json::Error::io(std::io::Error::other(s)))
            }
            w::PluginError::CapabilityDenied(s) => PluginError::CapabilityDenied(s),
            w::PluginError::Io(s) => PluginError::Io(s),
            w::PluginError::ProcessingFailed(s) => PluginError::ProcessingFailed(s),
            w::PluginError::Config(s) => PluginError::Config(s),
        }
    }
}

impl From<PluginError> for w::PluginError {
    fn from(e: PluginError) -> Self {
        match e {
            PluginError::Serialization(e) => w::PluginError::Serialization(e.to_string()),
            PluginError::CapabilityDenied(s) => w::PluginError::CapabilityDenied(s),
            PluginError::Io(s) => w::PluginError::Io(s),
            PluginError::ProcessingFailed(s) => w::PluginError::ProcessingFailed(s),
            PluginError::Config(s) => w::PluginError::Config(s),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Document conversion (models <-> WIT). Keep in sync with `crates/sdk/src/component.rs`.

fn time_to_wit(t: &chrono::DateTime<chrono::Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

fn time_from_wit(s: &str) -> PluginResult<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&chrono::Utc))
        .map_err(|e| PluginError::ProcessingFailed(format!("invalid timestamp '{}': {}", s, e)))
}

fn bbox_to_wit(b: m::BoundingBox) -> w::BoundingBox {
    w::BoundingBox { x: b.x, y: b.y, width: b.width, height: b.height }
}

fn bbox_from_wit(b: w::BoundingBox) -> m::BoundingBox {
    m::BoundingBox { x: b.x, y: b.y, width: b.width, height: b.height }
}

/// Converts a models document to the WIT record.
fn document_to_wit(d: m::UnifiedDocument) -> w::Document {
    w::Document {
        id: d.id,
        file: w::FileInfo {
            path: d.file.path,
            filename: d.file.filename,
            size_bytes: d.file.size_bytes,
            mime_type: d.file.mime_type,
            sha256: d.file.sha256,
        },
        metadata: d.metadata.map(|x| w::MetadataInfo {
            camera_make: x.camera_make,
            camera_model: x.camera_model,
            lens_model: x.lens_model,
            iso: x.iso,
            focal_length: x.focal_length,
            f_number: x.f_number,
            exposure_time: x.exposure_time,
            raw_tags: x.raw_tags.into_iter().collect(),
        }),
        location: d.location.map(|x| w::LocationInfo {
            latitude: x.latitude,
            longitude: x.longitude,
            altitude: x.altitude,
            country: x.country,
            country_code: x.country_code,
            region: x.region,
            county: x.county,
            city: x.city,
            local_country: x.local_country,
            local_region: x.local_region,
            local_city: x.local_city,
            local_language: x.local_language,
        }),
        country: d.country.map(|x| w::CountryInfo {
            country_code: x.country_code,
            country_name: x.country_name,
            flag_emoji: x.flag_emoji,
            continent: x.continent,
            region: x.region,
        }),
        thumbnails: d
            .thumbnails
            .into_iter()
            .map(|x| w::ThumbnailEntry {
                size_label: x.size_label,
                width: x.width,
                height: x.height,
                path: x.path,
                cache_key: x.cache_key,
            })
            .collect(),
        ai: d.ai.map(|x| w::AiInfo {
            tags: x
                .tags
                .into_iter()
                .map(|t| w::AiTag { label: t.label, confidence: t.confidence, model: t.model })
                .collect(),
            description: x.description,
            embedding: x.embedding,
        }),
        duplicate: d.duplicate.map(|x| w::DuplicateInfo {
            sha256: x.sha256,
            phash: x.phash,
            duplicates: x.duplicates,
        }),
        timeline: d.timeline.map(|x| w::TimelineInfo {
            taken_at: x.taken_at.as_ref().map(time_to_wit),
            imported_at: time_to_wit(&x.imported_at),
            modified_at: x.modified_at.as_ref().map(time_to_wit),
            resolution_source: x.resolution_source,
        }),
        weather: d.weather.map(|x| w::WeatherInfo {
            temperature: x.temperature,
            condition: x.condition,
            wmo_code: x.wmo_code,
            icon_emoji: x.icon_emoji,
            humidity_percent: x.humidity_percent,
            precipitation_mm: x.precipitation_mm,
            is_day: x.is_day,
        }),
        ocr: d.ocr.map(|x| w::OcrInfo {
            combined_text: x.combined_text,
            regions: x
                .regions
                .into_iter()
                .map(|r| w::TextRegion { text: r.text, confidence: r.confidence, bbox: bbox_to_wit(r.bbox) })
                .collect(),
            primary_language: x.primary_language,
        }),
        faces: d.faces.map(|x| w::FacesInfo {
            faces: x
                .faces
                .into_iter()
                .map(|f| w::DetectedFace {
                    id: f.id,
                    bbox: bbox_to_wit(f.bbox),
                    confidence: f.confidence,
                    landmarks: f.landmarks.map(|l| w::FaceLandmarks {
                        left_eye: l.left_eye,
                        right_eye: l.right_eye,
                        nose: l.nose,
                        mouth_left: l.mouth_left,
                        mouth_right: l.mouth_right,
                    }),
                    embedding: f.embedding,
                    person_id: f.person_id,
                })
                .collect(),
            detection_model: x.detection_model,
        }),
        privacy: d.privacy.map(|x| w::PrivacyInfo {
            is_redacted: x.is_redacted,
            faces_redacted_count: x.faces_redacted_count as u64,
            license_plates_redacted_count: x.license_plates_redacted_count as u64,
            original_gps_fuzzed: x.original_gps_fuzzed,
            redacted_image_path: x.redacted_image_path,
            applied_rules: x.applied_rules,
        }),
        custom_json: serde_json::to_string(&d.custom).unwrap_or_else(|_| "{}".to_string()),
    }
}

/// Converts the WIT record to a models document; fails on invalid timestamps or `custom-json`.
fn document_from_wit(d: w::Document) -> PluginResult<m::UnifiedDocument> {
    let timeline = match d.timeline {
        Some(x) => Some(m::TimelineInfo {
            taken_at: x.taken_at.as_deref().map(time_from_wit).transpose()?,
            imported_at: time_from_wit(&x.imported_at)?,
            modified_at: x.modified_at.as_deref().map(time_from_wit).transpose()?,
            resolution_source: x.resolution_source,
        }),
        None => None,
    };
    let custom: HashMap<String, serde_json::Value> = serde_json::from_str(&d.custom_json)?;
    Ok(m::UnifiedDocument {
        id: d.id,
        file: m::FileInfo {
            path: d.file.path,
            filename: d.file.filename,
            size_bytes: d.file.size_bytes,
            mime_type: d.file.mime_type,
            sha256: d.file.sha256,
        },
        metadata: d.metadata.map(|x| m::MetadataInfo {
            camera_make: x.camera_make,
            camera_model: x.camera_model,
            lens_model: x.lens_model,
            iso: x.iso,
            focal_length: x.focal_length,
            f_number: x.f_number,
            exposure_time: x.exposure_time,
            raw_tags: x.raw_tags.into_iter().collect(),
        }),
        location: d.location.map(|x| m::LocationInfo {
            latitude: x.latitude,
            longitude: x.longitude,
            altitude: x.altitude,
            country: x.country,
            country_code: x.country_code,
            region: x.region,
            county: x.county,
            city: x.city,
            local_country: x.local_country,
            local_region: x.local_region,
            local_city: x.local_city,
            local_language: x.local_language,
        }),
        country: d.country.map(|x| m::CountryInfo {
            country_code: x.country_code,
            country_name: x.country_name,
            flag_emoji: x.flag_emoji,
            continent: x.continent,
            region: x.region,
        }),
        thumbnails: d
            .thumbnails
            .into_iter()
            .map(|x| m::ThumbnailEntry {
                size_label: x.size_label,
                width: x.width,
                height: x.height,
                path: x.path,
                cache_key: x.cache_key,
            })
            .collect(),
        ai: d.ai.map(|x| m::AiInfo {
            tags: x
                .tags
                .into_iter()
                .map(|t| m::AiTag { label: t.label, confidence: t.confidence, model: t.model })
                .collect(),
            description: x.description,
            embedding: x.embedding,
        }),
        duplicate: d.duplicate.map(|x| m::DuplicateInfo {
            sha256: x.sha256,
            phash: x.phash,
            duplicates: x.duplicates,
        }),
        timeline,
        weather: d.weather.map(|x| m::WeatherInfo {
            temperature: x.temperature,
            condition: x.condition,
            wmo_code: x.wmo_code,
            icon_emoji: x.icon_emoji,
            humidity_percent: x.humidity_percent,
            precipitation_mm: x.precipitation_mm,
            is_day: x.is_day,
        }),
        ocr: d.ocr.map(|x| m::OcrInfo {
            combined_text: x.combined_text,
            regions: x
                .regions
                .into_iter()
                .map(|r| m::TextRegion { text: r.text, confidence: r.confidence, bbox: bbox_from_wit(r.bbox) })
                .collect(),
            primary_language: x.primary_language,
        }),
        faces: d.faces.map(|x| m::FacesInfo {
            faces: x
                .faces
                .into_iter()
                .map(|f| m::DetectedFace {
                    id: f.id,
                    bbox: bbox_from_wit(f.bbox),
                    confidence: f.confidence,
                    landmarks: f.landmarks.map(|l| m::FaceLandmarks {
                        left_eye: l.left_eye,
                        right_eye: l.right_eye,
                        nose: l.nose,
                        mouth_left: l.mouth_left,
                        mouth_right: l.mouth_right,
                    }),
                    embedding: f.embedding,
                    person_id: f.person_id,
                })
                .collect(),
            detection_model: x.detection_model,
        }),
        privacy: d.privacy.map(|x| m::PrivacyInfo {
            is_redacted: x.is_redacted,
            faces_redacted_count: x.faces_redacted_count as usize,
            license_plates_redacted_count: x.license_plates_redacted_count as usize,
            original_gps_fuzzed: x.original_gps_fuzzed,
            redacted_image_path: x.redacted_image_path,
            applied_rules: x.applied_rules,
        }),
        custom,
    })
}
