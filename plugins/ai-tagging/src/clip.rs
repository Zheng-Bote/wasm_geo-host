//! Built-in vision model: the image encoder of OpenAI CLIP ViT-B/32 (int8 ONNX, MIT) run with
//! the pure-Rust inference engine `rten` inside the plugin component (ADR 0009, ADR 0010).
//!
//! The model is a file (`make models`, 89 MB), read by the plugin through `fs-read` and checked
//! against its SHA-256 before it is loaded. Tags come from the fixed label list
//! (`labels/labels.txt`), whose text embeddings are embedded as `labels/labels.bin`.
//! The model produces no caption, so `description` stays empty.

use crate::VisionModel;
use flagme_sdk::{AiInfo, AiTag};
use image::{ImageFormat, RgbImage};
use rten::{Model, NodeId, Value};
use rten_tensor::prelude::*;
use rten_tensor::{NdTensor, Tensor};
use sha2::{Digest, Sha256};

/// SHA-256 of `models/clip-vit-b32-vision-int8.onnx` (`models/SHA256SUMS`).
pub const MODEL_SHA256: &str = "0ab0c1b3ace708e539633af1744d5a95247fe4e14d3e08ff197ef82a6cb9bd93";
/// `AiTag.model` and `VisionModel::model_name`.
pub const MODEL_NAME: &str = "clip-vit-b32";
/// Default of the `model_path` setting (relative to the working directory).
pub const DEFAULT_MODEL_PATH: &str = "models/clip-vit-b32-vision-int8.onnx";
pub const IMAGE_SIZE: u32 = 224;
pub const EMBEDDING_DIM: usize = 512;
pub const LOGIT_SCALE: f32 = 100.0;
const MEAN: [f32; 3] = [0.48145466, 0.4578275, 0.40821073];
const STD: [f32; 3] = [0.268_629_54, 0.261_302_6, 0.275_777_1];

/// Text embeddings of the label list (built by `tools/clip-labels`).
pub static LABELS: &[u8] = include_bytes!("../labels/labels.bin");
pub const LABELS_MAGIC: &[u8; 4] = b"FCLB";
pub const LABELS_VERSION: u16 = 1;
const LABELS_HEADER: usize = 4 + 2 + 2 + 4 + 4 + 32 + 32;

/// Parsed `labels.bin`.
#[derive(Debug, Clone)]
pub struct Labels {
    pub names: Vec<String>,
    /// `names.len() * EMBEDDING_DIM` values, each label vector L2-normalised.
    pub vectors: Vec<f32>,
    pub labels_sha256: [u8; 32],
    pub text_model_sha256: [u8; 32],
}

impl Labels {
    /// Checks magic, version, dimension and lengths.
    pub fn parse(b: &[u8]) -> Result<Self, String> {
        if b.len() < LABELS_HEADER || &b[0..4] != LABELS_MAGIC {
            return Err("labels.bin: not a label file".to_string());
        }
        let u16_at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
        let u32_at = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize;
        let version = u16_at(4);
        if version != LABELS_VERSION {
            return Err(format!("labels.bin: unsupported version {} (expected {})", version, LABELS_VERSION));
        }
        let (count, dim) = (u32_at(8), u32_at(12));
        if dim != EMBEDDING_DIM {
            return Err(format!("labels.bin: dimension {} (expected {})", dim, EMBEDDING_DIM));
        }
        let mut labels_sha256 = [0u8; 32];
        labels_sha256.copy_from_slice(&b[16..48]);
        let mut text_model_sha256 = [0u8; 32];
        text_model_sha256.copy_from_slice(&b[48..80]);
        let vec_end = count
            .checked_mul(dim * 4)
            .and_then(|n| n.checked_add(LABELS_HEADER))
            .filter(|&e| e <= b.len())
            .ok_or("labels.bin: truncated vectors")?;
        let vectors: Vec<f32> = b[LABELS_HEADER..vec_end]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        let mut names = Vec::with_capacity(count);
        let mut pos = vec_end;
        for _ in 0..count {
            let len = b.get(pos..pos + 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize).ok_or("labels.bin: truncated names")?;
            let name = b.get(pos + 2..pos + 2 + len).ok_or("labels.bin: truncated names")?;
            names.push(std::str::from_utf8(name).map_err(|_| "labels.bin: label is not UTF-8")?.to_string());
            pos += 2 + len;
        }
        if pos != b.len() {
            return Err("labels.bin: trailing bytes".to_string());
        }
        Ok(Self { names, vectors, labels_sha256, text_model_sha256 })
    }
}

/// Which tags to keep.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TagParams {
    pub max_tags: usize,
    pub min_confidence: f32,
}

impl Default for TagParams {
    fn default() -> Self {
        Self { max_tags: 5, min_confidence: 0.10 }
    }
}

/// `true` if an end-of-image marker (`FF D9`) follows the first start-of-scan marker (`FF DA`).
/// Copied on purpose: plugins must not depend on each other.
fn jpeg_is_complete(data: &[u8]) -> bool {
    let Some(sos) = data.windows(2).position(|w| w == [0xFF, 0xDA]) else {
        return false;
    };
    data[sos + 2..].windows(2).any(|w| w == [0xFF, 0xD9])
}

fn decode(image: &[u8]) -> Result<RgbImage, String> {
    let format = image::guess_format(image).map_err(|e| format!("unknown image format: {}", e))?;
    if !matches!(format, ImageFormat::Jpeg | ImageFormat::Png) {
        return Err(format!("unsupported image format {:?}", format));
    }
    if format == ImageFormat::Jpeg && !jpeg_is_complete(image) {
        return Err("cannot decode Jpeg image: truncated (no end-of-image marker after the scan data)".to_string());
    }
    image::load_from_memory_with_format(image, format)
        .map(|i| i.to_rgb8())
        .map_err(|e| format!("cannot decode {:?} image: {}", format, e))
}

/// CLIP ViT-B/32 image encoder with the embedded label vectors.
pub struct ClipModel {
    model: Model,
    input: NodeId,
    output: NodeId,
    labels: Labels,
}

impl ClipModel {
    /// Checks the SHA-256 of the model file, loads it and the embedded label vectors.
    pub fn from_model_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        let hash = format!("{:x}", Sha256::digest(&bytes));
        if hash != MODEL_SHA256 {
            return Err(format!("model file has SHA-256 {}, expected {}", hash, MODEL_SHA256));
        }
        let model = Model::load(bytes).map_err(|e| format!("cannot load CLIP model: {}", e))?;
        let input = model.node_id("pixel_values").map_err(|e| format!("CLIP model input: {}", e))?;
        let output = model.node_id("image_embeds").map_err(|e| format!("CLIP model output: {}", e))?;
        let labels = Labels::parse(LABELS)?;
        Ok(Self { model, input, output, labels })
    }

    /// The L2-normalised image embedding.
    pub fn embed(&self, img: &RgbImage) -> Result<Vec<f32>, String> {
        let (w, h) = img.dimensions();
        if w == 0 || h == 0 {
            return Err("empty image".to_string());
        }
        let scale = IMAGE_SIZE as f32 / w.min(h) as f32;
        let nw = ((w as f32 * scale).round() as u32).max(IMAGE_SIZE);
        let nh = ((h as f32 * scale).round() as u32).max(IMAGE_SIZE);
        let resized = image::imageops::resize(img, nw, nh, image::imageops::FilterType::CatmullRom);
        let (ox, oy) = ((nw - IMAGE_SIZE) / 2, (nh - IMAGE_SIZE) / 2);
        let n = IMAGE_SIZE as usize;
        let mut input = NdTensor::<f32, 4>::zeros([1, 3, n, n]);
        for y in 0..IMAGE_SIZE {
            for x in 0..IMAGE_SIZE {
                let p = resized.get_pixel(x + ox, y + oy);
                for c in 0..3 {
                    input[[0, c, y as usize, x as usize]] = (p[c] as f32 / 255.0 - MEAN[c]) / STD[c];
                }
            }
        }
        let values = self
            .model
            .run(vec![(self.input, Value::from(input).into())], &[self.output], None)
            .map_err(|e| format!("CLIP inference failed: {}", e))?;
        let t: Tensor<f32> = values
            .into_iter()
            .next()
            .ok_or("CLIP model returned no output")?
            .try_into()
            .map_err(|e| format!("unexpected CLIP output: {:?}", e))?;
        let v = t.to_vec();
        if v.len() != EMBEDDING_DIM {
            return Err(format!("CLIP embedding has dimension {}, expected {}", v.len(), EMBEDDING_DIM));
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if !norm.is_finite() || norm == 0.0 {
            return Err("CLIP embedding is zero or not finite".to_string());
        }
        Ok(v.into_iter().map(|x| x / norm).collect())
    }

    /// Label probabilities (softmax over `LOGIT_SCALE · cosine`), in label order.
    pub fn label_probabilities(&self, embedding: &[f32]) -> Vec<f32> {
        let logits: Vec<f32> = self
            .labels
            .vectors
            .as_chunks::<EMBEDDING_DIM>()
            .0
            .iter()
            .map(|l| LOGIT_SCALE * l.iter().zip(embedding).map(|(a, b)| a * b).sum::<f32>())
            .collect();
        let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let exp: Vec<f32> = logits.iter().map(|l| (l - max).exp()).collect();
        let sum: f32 = exp.iter().sum();
        exp.into_iter().map(|e| e / sum).collect()
    }

    /// Embedding and tags for an image.
    pub fn analyze_with(&self, image: &[u8], params: &TagParams) -> Result<AiInfo, String> {
        let img = decode(image)?;
        let embedding = self.embed(&img)?;
        let probs = self.label_probabilities(&embedding);
        let mut ranked: Vec<(usize, f32)> = probs.into_iter().enumerate().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let tags = ranked
            .into_iter()
            .filter(|(_, p)| *p >= params.min_confidence)
            .take(params.max_tags)
            .map(|(i, p)| AiTag { label: self.labels.names[i].clone(), confidence: p, model: MODEL_NAME.to_string() })
            .collect();
        Ok(AiInfo { tags, description: String::new(), embedding: Some(embedding) })
    }

    /// The labels the model can assign.
    pub fn labels(&self) -> &Labels {
        &self.labels
    }
}

impl VisionModel for ClipModel {
    fn model_name(&self) -> &str {
        MODEL_NAME
    }

    fn analyze(&self, image: &[u8], _embedding_dim: usize) -> Result<AiInfo, String> {
        self.analyze_with(image, &TagParams::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_labels_parse() {
        let l = Labels::parse(LABELS).unwrap();
        assert_eq!(l.vectors.len(), l.names.len() * EMBEDDING_DIM);
        assert!(l.names.len() >= 200, "{}", l.names.len());
    }
}
