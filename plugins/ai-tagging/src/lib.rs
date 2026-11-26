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
    pub embedding_dim: usize,
}

impl Default for AiTaggingSettings {
    fn default() -> Self {
        Self {
            embedding_dim: DEFAULT_EMBEDDING_DIM,
        }
    }
}

impl PluginSettings for AiTaggingSettings {
    const PLUGIN: &'static str = "ai-tagging";

    fn from_reader(base: &Self, r: &mut SettingsReader<'_>) -> Self {
        let reason = "no vision model is built in, see ADR 0002";
        r.not_supported("model", &[], reason);
        r.not_supported("model_tier", &[], reason);
        r.not_supported("device", &[], reason);
        Self {
            embedding_dim: r.u32("embedding_dim", &[], 64..=2048, base.embedding_dim as u32) as usize,
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

/// AI tagging plugin with a swappable vision model.
///
/// Without a model the plugin leaves `doc.ai` untouched and logs a warning (ADR 0002).
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
    /// Plugin without a vision model.
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
        "0.1.0"
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        let settings = self.settings.get_or_parse(ctx);
        let warn = |msg: String| ctx.capabilities.log("warn", &format!("ai-tagging: {}", msg));

        let Some(model) = self.model.as_deref() else {
            warn(format!("no vision model available (ADR 0002); '{}' not processed", doc.file.path));
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
            &format!("Running vision model '{}' on '{}'", model.model_name(), doc.file.path),
        );
        match model.analyze(&bytes, settings.embedding_dim) {
            Ok(mut info) => {
                if let Some(embedding) = info.embedding.take() {
                    match normalize_embedding(embedding, settings.embedding_dim) {
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
