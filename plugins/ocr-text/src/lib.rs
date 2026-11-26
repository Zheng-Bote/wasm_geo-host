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
}

/// Configurable parameters of the OCR plugin (`config.yaml` `settings`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OcrSettings {
    /// Regions with a lower confidence are dropped.
    pub min_confidence: f32,
}

impl PluginSettings for OcrSettings {
    const PLUGIN: &'static str = "ocr-text";

    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        r.not_supported("engine", &[], "no OCR engine is built in, see ADR 0002");
        Self {
            min_confidence: r.f32("min_confidence", &[], 0.0..=1.0, base.min_confidence),
        }
    }
}

/// Drops regions below `min_confidence` and rebuilds `combined_text` / `primary_language`.
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
        .join(" ");
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
    settings: SettingsCache<OcrSettings>,
}

impl Default for OcrTextPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl OcrTextPlugin {
    /// Plugin without an OCR engine.
    pub fn new() -> Self {
        Self {
            engine: None,
            settings: SettingsCache::new(OcrSettings::default()),
        }
    }

    pub fn with_engine(engine: Box<dyn OcrEngine>) -> Self {
        Self {
            engine: Some(engine),
            ..Self::new()
        }
    }
}

impl Plugin for OcrTextPlugin {
    fn name(&self) -> &'static str {
        "ocr-text"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let settings = self.settings.get_or_parse(ctx);
        let warn = |msg: String| ctx.capabilities.log("warn", &format!("ocr-text: {}", msg));

        let Some(engine) = self.engine.as_deref() else {
            warn(format!("no OCR engine available (ADR 0002); '{}' not processed", doc.file.path));
            return Ok(doc);
        };
        let bytes = match ctx.capabilities.fs_read(&doc.file.path) {
            Ok(bytes) => bytes,
            Err(e) => {
                warn(format!("cannot read '{}': {}", doc.file.path, e));
                return Ok(doc);
            }
        };
        ctx.capabilities.log(
            "info",
            &format!("Running OCR engine '{}' on '{}'", engine.name(), doc.file.path),
        );
        match engine.recognize(&bytes) {
            Ok(info) => doc.ocr = Some(filter_by_confidence(info, settings.min_confidence)),
            Err(e) => warn(format!("engine '{}' failed on '{}': {}", engine.name(), doc.file.path, e)),
        }
        Ok(doc)
    }
}

flagme_sdk::export_plugin!(OcrTextPlugin, OcrTextPlugin::new);
