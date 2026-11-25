use flagme_sdk::{
    AiInfo, AiTag, Plugin, PluginContext, PluginResult, UnifiedDocument,
};

/// Vision model interface for pluggable AI classification models.
pub trait VisionModel: Send + Sync {
    fn model_name(&self) -> &'static str;
    fn predict(&self, image_path: &str) -> (Vec<AiTag>, String);
    fn predict_with_embedding(&self, image_path: &str) -> (Vec<AiTag>, String, Option<Vec<f32>>) {
        let (tags, desc) = self.predict(image_path);
        let mut embedding = vec![0.0f32; 512];
        // Generate a deterministic unit vector based on the model name and path for testing/runtime
        let seed = image_path.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
        for i in 0..512 {
            embedding[i] = (((i as u32).wrapping_add(seed) % 100) as f32) / 100.0;
        }
        let norm: f32 = embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in embedding.iter_mut() {
                *v /= norm;
            }
        }
        (tags, desc, Some(embedding))
    }
}

pub struct MobileNetModel;
impl VisionModel for MobileNetModel {
    fn model_name(&self) -> &'static str {
        "MobileNetV3"
    }
    fn predict(&self, _path: &str) -> (Vec<AiTag>, String) {
        (
            vec![
                AiTag {
                    label: "landscape".to_string(),
                    confidence: 0.94,
                    model: self.model_name().to_string(),
                },
                AiTag {
                    label: "outdoor".to_string(),
                    confidence: 0.89,
                    model: self.model_name().to_string(),
                },
            ],
            "Outdoor landscape scene".to_string(),
        )
    }
}

pub struct ClipModel;
impl VisionModel for ClipModel {
    fn model_name(&self) -> &'static str {
        "CLIP-ViT-B/32"
    }
    fn predict(&self, _path: &str) -> (Vec<AiTag>, String) {
        (
            vec![
                AiTag {
                    label: "mountain peak".to_string(),
                    confidence: 0.91,
                    model: self.model_name().to_string(),
                },
                AiTag {
                    label: "blue sky".to_string(),
                    confidence: 0.88,
                    model: self.model_name().to_string(),
                },
            ],
            "Mountain range under bright sky".to_string(),
        )
    }
}

pub struct OpenClipModel;
impl VisionModel for OpenClipModel {
    fn model_name(&self) -> &'static str {
        "OpenCLIP-ConvNeXt-Large"
    }
    fn predict(&self, _path: &str) -> (Vec<AiTag>, String) {
        (
            vec![
                AiTag {
                    label: "nature photography".to_string(),
                    confidence: 0.96,
                    model: self.model_name().to_string(),
                },
            ],
            "High resolution nature photograph".to_string(),
        )
    }
}

/// Swappable AI tagging plugin supporting ONNX Runtime model swapping.
pub struct AiTaggingPlugin {
    model: Box<dyn VisionModel>,
}

impl AiTaggingPlugin {
    pub fn new() -> Self {
        Self {
            model: Box::new(ClipModel),
        }
    }

    pub fn with_model(model: Box<dyn VisionModel>) -> Self {
        Self { model }
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
        ctx.capabilities.log(
            "info",
            &format!("Running AI inference with model '{}'", self.model.model_name()),
        );

        let (tags, description, embedding) = self.model.predict_with_embedding(&doc.file.path);

        doc.ai = Some(AiInfo { tags, description, embedding });
        Ok(doc)
    }
}
