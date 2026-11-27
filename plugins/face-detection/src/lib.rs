pub mod yunet;

use flagme_sdk::{
    DetectedFace, FacesInfo, Plugin, PluginContext, PluginResult, PluginSettings, SettingsCache,
    SettingsReader, UnifiedDocument,
};

/// Swappable face detector (detection, landmarks and optional identity embeddings).
///
/// A detector only receives the image bytes, never the file name or path (ADR 0002).
pub trait FaceDetector: Send + Sync {
    fn model_name(&self) -> &str;
    fn detect(&self, image: &[u8]) -> Result<Vec<DetectedFace>, String>;
}

/// Configurable parameters of the face detection plugin (`config.yaml` `settings`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FaceDetectionSettings {
    /// Faces with a lower confidence are dropped.
    pub min_confidence: f32,
}

impl PluginSettings for FaceDetectionSettings {
    const PLUGIN: &'static str = "face-detection";

    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        let reason = "only the built-in YuNet detector is available, see ADR 0009";
        r.not_supported("model", &[], reason);
        r.not_supported("tier", &[], reason);
        r.not_supported("device", &[], reason);
        Self {
            min_confidence: r.f32("min_confidence", &[], 0.0..=1.0, base.min_confidence),
        }
    }
}

/// Face Detection Plugin with a swappable detector; by default the embedded YuNet model
/// (ADR 0009).
///
/// Without a detector the plugin leaves `doc.faces` untouched and logs a warning (ADR 0002).
pub struct FaceDetectionPlugin {
    detector: Option<Box<dyn FaceDetector>>,
    /// Why the built-in detector could not be loaded, reported in `process`.
    unavailable: Option<String>,
    settings: SettingsCache<FaceDetectionSettings>,
}

impl Default for FaceDetectionPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl FaceDetectionPlugin {
    /// Plugin with the built-in YuNet detector (ADR 0009).
    pub fn new() -> Self {
        match yunet::YuNetDetector::new() {
            Ok(detector) => Self::with_detector(Box::new(detector)),
            Err(e) => Self {
                detector: None,
                unavailable: Some(e),
                settings: SettingsCache::new(FaceDetectionSettings::default()),
            },
        }
    }

    /// Plugin with the given detector (used by tests).
    pub fn with_detector(detector: Box<dyn FaceDetector>) -> Self {
        Self {
            detector: Some(detector),
            unavailable: None,
            settings: SettingsCache::new(FaceDetectionSettings::default()),
        }
    }
}

impl Plugin for FaceDetectionPlugin {
    fn name(&self) -> &'static str {
        "face-detection"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let settings = self.settings.get_or_parse(ctx);
        let warn = |msg: String| ctx.capabilities.log("warn", &format!("face-detection: {}", msg));

        let Some(detector) = self.detector.as_deref() else {
            match &self.unavailable {
                Some(e) => warn(format!("built-in detector unavailable: {}; '{}' not processed", e, doc.file.path)),
                None => warn(format!("no face detector available (ADR 0002); '{}' not processed", doc.file.path)),
            }
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
            &format!("Running face detector '{}' on '{}'", detector.model_name(), doc.file.path),
        );
        match detector.detect(&bytes) {
            Ok(mut faces) => {
                if settings.min_confidence > 0.0 {
                    faces.retain(|f| f.confidence >= settings.min_confidence);
                }
                doc.faces = Some(FacesInfo {
                    faces,
                    detection_model: detector.model_name().to_string(),
                });
            }
            Err(e) => warn(format!("detector '{}' failed on '{}': {}", detector.model_name(), doc.file.path, e)),
        }
        Ok(doc)
    }
}

flagme_sdk::export_plugin!(FaceDetectionPlugin, FaceDetectionPlugin::new);
