pub mod paddle;

use flagme_sdk::{
    OcrInfo, Plugin, PluginContext, PluginResult, PluginSettings, SettingsCache, SettingsReader,
    UnifiedDocument,
};

/// Swappable OCR engine (text detection + recognition).
///
/// An engine only receives the image bytes, never the file name or path (ADR 0002).
pub trait OcrEngine: Send + Sync {
    fn name(&self) -> &str;
    fn recognize(&self, image: &[u8]) -> Result<OcrInfo, String>;

    /// Like `recognize`, but reads at most `max_regions` text lines. Returns the number of lines
    /// found if it exceeded the limit. Engines without a limit use the default.
    fn recognize_limited(&self, image: &[u8], max_regions: usize) -> Result<(OcrInfo, Option<usize>), String> {
        let _ = max_regions;
        self.recognize(image).map(|info| (info, None))
    }
}

/// Configurable parameters of the OCR plugin (`config.yaml` `settings`).
#[derive(Debug, Clone, PartialEq)]
pub struct OcrSettings {
    /// Regions with a lower confidence are dropped.
    pub min_confidence: f32,
    /// At most this many text lines are read per image.
    pub max_regions: u32,
}

impl Default for OcrSettings {
    fn default() -> Self {
        Self { min_confidence: 0.5, max_regions: 200 }
    }
}

impl PluginSettings for OcrSettings {
    const PLUGIN: &'static str = "ocr-text";

    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        r.not_supported("engine", &[], "only the built-in PaddleOCR engine is available, see ADR 0009");
        Self {
            min_confidence: r.f32("min_confidence", &[], 0.0..=1.0, base.min_confidence),
            max_regions: r.u32("max_regions", &[], 1..=1000, base.max_regions),
        }
    }
}

/// Drops regions below `min_confidence` and rebuilds `combined_text` (lines joined by `\n`).
pub fn filter_by_confidence(mut info: OcrInfo, min_confidence: f32) -> OcrInfo {
    if min_confidence <= 0.0 {
        return info;
    }
    info.regions.retain(|r| r.confidence >= min_confidence);
    info.combined_text = info
        .regions
        .iter()
        .map(|r| r.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if info.regions.is_empty() {
        info.primary_language = None;
    }
    info
}

/// OCR Text Recognition Plugin for WASM Geo Host.
///
/// Without an engine the plugin leaves `doc.ocr` untouched and logs a warning (ADR 0002).
pub struct OcrTextPlugin {
    engine: Option<Box<dyn OcrEngine>>,
    /// Why the built-in engine could not be loaded, reported in `process`.
    unavailable: Option<String>,
    settings: SettingsCache<OcrSettings>,
}

impl Default for OcrTextPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl OcrTextPlugin {
    /// Plugin with the built-in PaddleOCR engine (ADR 0009).
    pub fn new() -> Self {
        match paddle::PaddleOcrEngine::new() {
            Ok(engine) => Self::with_engine(Box::new(engine)),
            Err(e) => Self {
                engine: None,
                unavailable: Some(e),
                settings: SettingsCache::new(OcrSettings::default()),
            },
        }
    }

    /// Plugin with the given engine (used by tests).
    pub fn with_engine(engine: Box<dyn OcrEngine>) -> Self {
        Self {
            engine: Some(engine),
            unavailable: None,
            settings: SettingsCache::new(OcrSettings::default()),
        }
    }
}

impl Plugin for OcrTextPlugin {
    fn name(&self) -> &'static str {
        "ocr-text"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let settings = self.settings.get_or_parse(ctx);
        let warn = |msg: String| ctx.capabilities.log("warn", &format!("ocr-text: {}", msg));

        let Some(engine) = self.engine.as_deref() else {
            match &self.unavailable {
                Some(e) => warn(format!("built-in OCR engine unavailable: {}; '{}' not processed", e, doc.file.path)),
                None => warn(format!("no OCR engine available (ADR 0002); '{}' not processed", doc.file.path)),
            }
            return Ok(doc);
        };
        // Working image if set (ADR 0011), otherwise the original file.
        let source = flagme_sdk::pixel_source(&doc).to_string();
        let bytes = match ctx.capabilities.fs_read(&source) {
            Ok(bytes) => bytes,
            Err(e) => {
                warn(format!("cannot read '{}': {}", source, e));
                return Ok(doc);
            }
        };
        ctx.capabilities.log(
            "info",
            &format!("Running OCR engine '{}' on '{}'", engine.name(), source),
        );
        match engine.recognize_limited(&bytes, settings.max_regions as usize) {
            Ok((info, truncated)) => {
                if let Some(found) = truncated {
                    warn(format!(
                        "'{}': {} text lines found, read the {} most confident (max_regions)",
                        source, found, settings.max_regions
                    ));
                }
                doc.ocr = Some(filter_by_confidence(info, settings.min_confidence));
            }
            Err(e) => warn(format!("engine '{}' failed on '{}': {}", engine.name(), source, e)),
        }
        Ok(doc)
    }
}

flagme_sdk::export_plugin!(OcrTextPlugin, OcrTextPlugin::new);
