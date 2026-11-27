pub mod clip;

use flagme_sdk::{
    AiInfo, Plugin, PluginContext, PluginResult, PluginSettings, SettingsCache, SettingsReader,
    UnifiedDocument,
};

/// Default embedding dimension.
pub const DEFAULT_EMBEDDING_DIM: usize = 512;

/// Swappable vision model producing tags, a description and an optional embedding.
///
/// A model only receives the image bytes, never the file name or path (ADR 0002).
pub trait VisionModel: Send + Sync {
    fn model_name(&self) -> &str;
    fn analyze(&self, image: &[u8], embedding_dim: usize) -> Result<AiInfo, String>;
}

/// Configurable parameters of the AI tagging plugin (`config.yaml` `settings`).
#[derive(Debug, Clone, PartialEq)]
pub struct AiTaggingSettings {
    /// Expected embedding dimension; the built-in CLIP model always produces 512.
    pub embedding_dim: usize,
    /// Path of the CLIP model file (`make models`), read through `fs-read`.
    pub model_path: String,
    /// At most this many tags per image.
    pub max_tags: u32,
    /// Tags with a lower probability are dropped.
    pub min_confidence: f32,
}

impl Default for AiTaggingSettings {
    fn default() -> Self {
        Self {
            embedding_dim: DEFAULT_EMBEDDING_DIM,
            model_path: clip::DEFAULT_MODEL_PATH.to_string(),
            max_tags: 5,
            min_confidence: 0.10,
        }
    }
}

impl PluginSettings for AiTaggingSettings {
    const PLUGIN: &'static str = "ai-tagging";

    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        let reason = "only the built-in CLIP model is available, see ADR 0010";
        r.not_supported("model", &[], reason);
        r.not_supported("model_tier", &[], reason);
        r.not_supported("device", &[], reason);
        Self {
            embedding_dim: r.u32("embedding_dim", &[], 64..=2048, base.embedding_dim as u32) as usize,
            model_path: r.path("model_path", &[], &base.model_path),
            max_tags: r.u32("max_tags", &[], 1..=20, base.max_tags),
            min_confidence: r.f32("min_confidence", &[], 0.0..=1.0, base.min_confidence),
        }
    }
}

/// Checks the embedding dimension and L2-normalises it.
///
/// Returns `Err` with a reason if the dimension does not match or the vector is zero / not finite.
pub fn normalize_embedding(mut embedding: Vec<f32>, expected_dim: usize) -> Result<Vec<f32>, String> {
    if embedding.len() != expected_dim {
        return Err(format!(
            "embedding has dimension {}, expected {}",
            embedding.len(),
            expected_dim
        ));
    }
    let norm = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
    if !norm.is_finite() || norm == 0.0 {
        return Err("embedding is zero or not finite".to_string());
    }
    for v in embedding.iter_mut() {
        *v /= norm;
    }
    Ok(embedding)
}

/// AI tagging plugin. By default it loads the CLIP model file from `model_path` on every call
/// (ADR 0010); `with_model` injects another model (tests).
///
/// Without a usable model the plugin leaves `doc.ai` untouched and logs a warning (ADR 0002).
pub struct AiTaggingPlugin {
    model: Option<Box<dyn VisionModel>>,
    settings: SettingsCache<AiTaggingSettings>,
}

impl Default for AiTaggingPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl AiTaggingPlugin {
    /// Plugin with the built-in CLIP model, loaded from `model_path` in `process`.
    pub fn new() -> Self {
        Self {
            model: None,
            settings: SettingsCache::new(AiTaggingSettings::default()),
        }
    }

    pub fn with_model(model: Box<dyn VisionModel>) -> Self {
        Self {
            model: Some(model),
            ..Self::new()
        }
    }
}

impl Plugin for AiTaggingPlugin {
    fn name(&self) -> &'static str {
        "ai-tagging"
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let settings = self.settings.get_or_parse(ctx);
        let warn = |msg: String| ctx.capabilities.log("warn", &format!("ai-tagging: {}", msg));

        // Built-in CLIP model: read and verify the model file for this call (fresh instance per
        // document, ADR 0003). An injected model takes precedence.
        let engine = match self.model.as_deref() {
            Some(m) => Engine::Injected(m),
            None => {
                let loaded = ctx
                    .capabilities
                    .fs_read(&settings.model_path)
                    .map_err(|e| e.to_string())
                    .and_then(clip::ClipModel::from_model_bytes);
                match loaded {
                    Ok(m) => {
                        if settings.embedding_dim != clip::EMBEDDING_DIM {
                            warn(format!(
                                "embedding_dim {} ignored: it is fixed to {} by the CLIP model",
                                settings.embedding_dim,
                                clip::EMBEDDING_DIM
                            ));
                        }
                        let params = clip::TagParams {
                            max_tags: settings.max_tags as usize,
                            min_confidence: settings.min_confidence,
                        };
                        Engine::Clip(Box::new(m), params)
                    }
                    Err(e) => {
                        warn(format!(
                            "CLIP model unavailable at '{}': {} (run `make models`); '{}' not processed",
                            settings.model_path, e, doc.file.path
                        ));
                        return Ok(doc);
                    }
                }
            }
        };
        let (model, dim): (&dyn VisionModel, usize) = match &engine {
            Engine::Injected(m) => (*m, settings.embedding_dim),
            Engine::Clip(m, _) => (m.as_ref(), clip::EMBEDDING_DIM),
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
            &format!("Running vision model '{}' on '{}'", model.model_name(), doc.file.path),
        );
        let result = match &engine {
            Engine::Injected(m) => m.analyze(&bytes, dim),
            Engine::Clip(m, params) => m.analyze_with(&bytes, params),
        };
        match result {
            Ok(mut info) => {
                if let Some(embedding) = info.embedding.take() {
                    match normalize_embedding(embedding, dim) {
                        Ok(normalized) => info.embedding = Some(normalized),
                        Err(reason) => warn(format!(
                            "embedding of '{}' from '{}' dropped: {}",
                            doc.file.path,
                            model.model_name(),
                            reason
                        )),
                    }
                }
                doc.ai = Some(info);
            }
            Err(e) => warn(format!("model '{}' failed on '{}': {}", model.model_name(), doc.file.path, e)),
        }
        Ok(doc)
    }
}

/// The model used for one call.
enum Engine<'a> {
    Injected(&'a dyn VisionModel),
    Clip(Box<clip::ClipModel>, clip::TagParams),
}

flagme_sdk::export_plugin!(AiTaggingPlugin, AiTaggingPlugin::new);
