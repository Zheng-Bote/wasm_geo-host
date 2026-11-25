use flagme_sdk::{
    BoundingBox, DetectedFace, FaceLandmarks, FacesInfo, Plugin, PluginContext, PluginResult,
    UnifiedDocument,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTier {
    Lightweight,
    Heavy,
}

impl Default for ModelTier {
    fn default() -> Self {
        Self::Lightweight
    }
}

/// Trait abstraction for pluggable face detectors and feature extractors.
pub trait FaceDetector: Send + Sync {
    fn tier(&self) -> ModelTier;
    fn model_name(&self) -> &'static str;
    fn detect(&self, path: &str) -> Vec<DetectedFace>;
}

/// Lightweight face detector simulating UltraFace / MobileFaceNet (128-D embeddings).
pub struct LightweightFaceDetector;

impl FaceDetector for LightweightFaceDetector {
    fn tier(&self) -> ModelTier {
        ModelTier::Lightweight
    }

    fn model_name(&self) -> &'static str {
        "UltraFace-Slim-INT8"
    }

    fn detect(&self, path: &str) -> Vec<DetectedFace> {
        let filename = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        let mut faces = Vec::new();

        // Check if image represents portrait or family photo
        if filename.to_lowercase().contains("portrait") || filename.to_lowercase().contains("face") {
            faces.push(DetectedFace {
                id: "face-1".to_string(),
                bbox: BoundingBox {
                    x: 0.30,
                    y: 0.20,
                    width: 0.40,
                    height: 0.45,
                },
                confidence: 0.97,
                landmarks: Some(FaceLandmarks {
                    left_eye: (0.42, 0.35),
                    right_eye: (0.58, 0.35),
                    nose: (0.50, 0.45),
                    mouth_left: (0.44, 0.55),
                    mouth_right: (0.56, 0.55),
                }),
                embedding: Some(vec![0.1f32; 128]),
                person_id: None,
            });
        } else if filename.to_lowercase().contains("group") {
            faces.push(DetectedFace {
                id: "face-g1".to_string(),
                bbox: BoundingBox {
                    x: 0.15,
                    y: 0.25,
                    width: 0.25,
                    height: 0.35,
                },
                confidence: 0.91,
                landmarks: Some(FaceLandmarks {
                    left_eye: (0.22, 0.35),
                    right_eye: (0.32, 0.35),
                    nose: (0.27, 0.42),
                    mouth_left: (0.23, 0.50),
                    mouth_right: (0.31, 0.50),
                }),
                embedding: Some(vec![0.15f32; 128]),
                person_id: None,
            });
            faces.push(DetectedFace {
                id: "face-g2".to_string(),
                bbox: BoundingBox {
                    x: 0.60,
                    y: 0.25,
                    width: 0.25,
                    height: 0.35,
                },
                confidence: 0.93,
                landmarks: Some(FaceLandmarks {
                    left_eye: (0.67, 0.35),
                    right_eye: (0.77, 0.35),
                    nose: (0.72, 0.42),
                    mouth_left: (0.68, 0.50),
                    mouth_right: (0.76, 0.50),
                }),
                embedding: Some(vec![0.25f32; 128]),
                person_id: None,
            });
        }

        faces
    }
}

/// Heavy face detector simulating SCRFD / ArcFace-ResNet100 (512-D embeddings).
pub struct HeavyFaceDetector;

impl FaceDetector for HeavyFaceDetector {
    fn tier(&self) -> ModelTier {
        ModelTier::Heavy
    }

    fn model_name(&self) -> &'static str {
        "SCRFD-ArcFace-ResNet100"
    }

    fn detect(&self, path: &str) -> Vec<DetectedFace> {
        let mut faces = LightweightFaceDetector.detect(path);
        // Heavy tier upgrades embeddings to 512-D high-precision vectors
        for face in &mut faces {
            face.embedding = Some(vec![0.05f32; 512]);
        }
        faces
    }
}

/// Face Detection and Clustering Plugin.
pub struct FaceDetectionPlugin {
    detector: Box<dyn FaceDetector>,
}

impl FaceDetectionPlugin {
    pub fn new() -> Self {
        Self {
            detector: Box::new(LightweightFaceDetector),
        }
    }

    pub fn with_tier(tier: ModelTier) -> Self {
        match tier {
            ModelTier::Lightweight => Self {
                detector: Box::new(LightweightFaceDetector),
            },
            ModelTier::Heavy => Self {
                detector: Box::new(HeavyFaceDetector),
            },
        }
    }

    pub fn with_detector(detector: Box<dyn FaceDetector>) -> Self {
        Self { detector }
    }
}

impl Plugin for FaceDetectionPlugin {
    fn name(&self) -> &'static str {
        "face-detection"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn process(&self, mut doc: UnifiedDocument, ctx: &PluginContext) -> PluginResult<UnifiedDocument> {
        ctx.capabilities.log(
            "info",
            &format!(
                "Running face detection with model '{}' ({:?}) on '{}'",
                self.detector.model_name(),
                self.detector.tier(),
                doc.file.path
            ),
        );

        let faces = self.detector.detect(&doc.file.path);
        let detection_model = self.detector.model_name().to_string();

        doc.faces = Some(FacesInfo {
            faces,
            detection_model,
        });

        Ok(doc)
    }
}
