//! Built-in face detector: YuNet (OpenCV Zoo, `face_detection_yunet_2023mar`, MIT) run with the
//! pure-Rust inference engine `rten` inside the plugin component (ADR 0009).
//!
//! The model is embedded; loading it takes a few milliseconds, so a fresh component instance per
//! document (ADR 0003) is cheap. Images are decoded without applying the EXIF orientation, like
//! `privacy-redact` does, so that the normalised boxes match the pixels that get redacted.

use crate::FaceDetector;
use flagme_sdk::{BoundingBox, DetectedFace, FaceLandmarks};
use image::{ImageFormat, RgbImage};
use rten::{Model, NodeId, Value};
use rten_tensor::prelude::*;
use rten_tensor::{NdTensor, Tensor};

/// The embedded ONNX model (see `models/SOURCE.md`).
pub static MODEL: &[u8] = include_bytes!("../models/face_detection_yunet_2023mar.onnx");
/// Value of `FacesInfo.detection_model`.
pub const MODEL_NAME: &str = "yunet-2023mar";
/// Side length of the square model input.
pub const INPUT_SIZE: usize = 640;
/// Candidates below this score are dropped before non-maximum suppression.
pub const SCORE_THRESHOLD: f32 = 0.6;
/// A candidate overlapping a kept face by this IoU or more is dropped.
pub const NMS_IOU: f32 = 0.3;
/// Feature-map strides of the model outputs.
pub const STRIDES: [usize; 3] = [8, 16, 32];

/// Output nodes of one stride: class score, objectness, box and keypoints.
struct StrideOutputs {
    stride: usize,
    cls: NodeId,
    obj: NodeId,
    bbox: NodeId,
    kps: NodeId,
}

/// A candidate in model-input pixels.
#[derive(Debug, Clone)]
struct Candidate {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    score: f32,
    points: [(f32, f32); 5],
}

/// YuNet face detector.
pub struct YuNetDetector {
    model: Model,
    input: NodeId,
    outputs: Vec<StrideOutputs>,
}

impl YuNetDetector {
    /// Loads the embedded model and checks that it has the expected outputs.
    pub fn new() -> Result<Self, String> {
        let model = Model::load_static_slice(MODEL).map_err(|e| format!("cannot load YuNet model: {}", e))?;
        let input = *model.input_ids().first().ok_or("YuNet model has no input")?;
        let node = |name: String| model.node_id(&name).map_err(|e| format!("YuNet output '{}': {}", name, e));
        let mut outputs = Vec::with_capacity(STRIDES.len());
        for stride in STRIDES {
            outputs.push(StrideOutputs {
                stride,
                cls: node(format!("cls_{stride}"))?,
                obj: node(format!("obj_{stride}"))?,
                bbox: node(format!("bbox_{stride}"))?,
                kps: node(format!("kps_{stride}"))?,
            });
        }
        Ok(Self { model, input, outputs })
    }

    /// Decodes JPEG or PNG; any other format is an error.
    fn decode(image: &[u8]) -> Result<RgbImage, String> {
        let format = image::guess_format(image).map_err(|e| format!("unknown image format: {}", e))?;
        if !matches!(format, ImageFormat::Jpeg | ImageFormat::Png) {
            return Err(format!("unsupported image format {:?}", format));
        }
        if format == ImageFormat::Jpeg && !jpeg_is_complete(image) {
            // The JPEG decoder fills missing scan data with grey instead of failing; a cut-off
            // file must not be analysed as if it were complete.
            return Err("cannot decode Jpeg image: truncated (no end-of-image marker after the scan data)".to_string());
        }
        let decoded = image::load_from_memory_with_format(image, format)
            .map_err(|e| format!("cannot decode {:?} image: {}", format, e))?;
        Ok(decoded.to_rgb8())
    }

    /// Detects faces in an RGB image; boxes and points are normalised to 0..1.
    pub fn detect_rgb(&self, img: &RgbImage) -> Result<Vec<DetectedFace>, String> {
        let (width, height) = (img.width(), img.height());
        if width == 0 || height == 0 {
            return Err("empty image".to_string());
        }
        let scale = INPUT_SIZE as f32 / width.max(height) as f32;
        let new_w = ((width as f32 * scale).round() as u32).clamp(1, INPUT_SIZE as u32);
        let new_h = ((height as f32 * scale).round() as u32).clamp(1, INPUT_SIZE as u32);
        let resized = image::imageops::resize(img, new_w, new_h, image::imageops::FilterType::Triangle);

        // BGR, 0..255, image in the top-left corner, zero padding.
        let mut input = NdTensor::<f32, 4>::zeros([1, 3, INPUT_SIZE, INPUT_SIZE]);
        for (x, y, p) in resized.enumerate_pixels() {
            let (x, y) = (x as usize, y as usize);
            input[[0, 0, y, x]] = p[2] as f32;
            input[[0, 1, y, x]] = p[1] as f32;
            input[[0, 2, y, x]] = p[0] as f32;
        }

        let ids: Vec<NodeId> = self.outputs.iter().flat_map(|o| [o.cls, o.obj, o.bbox, o.kps]).collect();
        let values = self
            .model
            .run(vec![(self.input, Value::from(input).into())], &ids, None)
            .map_err(|e| format!("YuNet inference failed: {}", e))?;
        let mut tensors = Vec::with_capacity(values.len());
        for v in values {
            let t: Tensor<f32> = v.try_into().map_err(|e| format!("unexpected YuNet output: {:?}", e))?;
            tensors.push(t.to_vec());
        }

        let mut candidates = Vec::new();
        for (k, out) in self.outputs.iter().enumerate() {
            let (cls, obj, bbox, kps) = (&tensors[4 * k], &tensors[4 * k + 1], &tensors[4 * k + 2], &tensors[4 * k + 3]);
            let cols = INPUT_SIZE / out.stride;
            let cells = cols * cols;
            if cls.len() < cells || obj.len() < cells || bbox.len() < cells * 4 || kps.len() < cells * 10 {
                return Err(format!("YuNet output of stride {} has an unexpected size", out.stride));
            }
            let s = out.stride as f32;
            for i in 0..cells {
                let score = (cls[i].clamp(0.0, 1.0) * obj[i].clamp(0.0, 1.0)).sqrt();
                if score < SCORE_THRESHOLD || !score.is_finite() {
                    continue;
                }
                let (row, col) = ((i / cols) as f32, (i % cols) as f32);
                let cx = (col + bbox[4 * i]) * s;
                let cy = (row + bbox[4 * i + 1]) * s;
                let w = bbox[4 * i + 2].exp() * s;
                let h = bbox[4 * i + 3].exp() * s;
                let mut points = [(0.0, 0.0); 5];
                for (j, p) in points.iter_mut().enumerate() {
                    *p = ((col + kps[10 * i + 2 * j]) * s, (row + kps[10 * i + 2 * j + 1]) * s);
                }
                candidates.push(Candidate { x: cx - w / 2.0, y: cy - h / 2.0, w, h, score, points });
            }
        }

        let kept = non_maximum_suppression(candidates);
        let (fw, fh) = (width as f32, height as f32);
        let norm = |v: f32, size: f32| (v / scale / size).clamp(0.0, 1.0);
        let mut faces: Vec<DetectedFace> = kept
            .into_iter()
            .map(|c| {
                let x0 = norm(c.x, fw);
                let y0 = norm(c.y, fh);
                let x1 = norm(c.x + c.w, fw);
                let y1 = norm(c.y + c.h, fh);
                let p: Vec<(f32, f32)> = c.points.iter().map(|&(x, y)| (norm(x, fw), norm(y, fh))).collect();
                let (eye_l, eye_r) = if p[0].0 <= p[1].0 { (p[0], p[1]) } else { (p[1], p[0]) };
                let (mouth_l, mouth_r) = if p[3].0 <= p[4].0 { (p[3], p[4]) } else { (p[4], p[3]) };
                DetectedFace {
                    id: String::new(),
                    bbox: BoundingBox { x: x0, y: y0, width: x1 - x0, height: y1 - y0 },
                    confidence: c.score,
                    landmarks: Some(FaceLandmarks {
                        left_eye: eye_l,
                        right_eye: eye_r,
                        nose: p[2],
                        mouth_left: mouth_l,
                        mouth_right: mouth_r,
                    }),
                    embedding: None,
                    person_id: None,
                }
            })
            .filter(|f| f.bbox.width > 0.0 && f.bbox.height > 0.0)
            .collect();
        faces.sort_by(|a, b| {
            a.bbox.y.total_cmp(&b.bbox.y).then(a.bbox.x.total_cmp(&b.bbox.x))
        });
        for (n, face) in faces.iter_mut().enumerate() {
            face.id = format!("face-{}", n + 1);
        }
        Ok(faces)
    }
}

/// `true` if an end-of-image marker (`FF D9`) follows the first start-of-scan marker (`FF DA`).
/// An embedded EXIF thumbnail ends before the main scan and does not count.
fn jpeg_is_complete(data: &[u8]) -> bool {
    let Some(sos) = data.windows(2).position(|w| w == [0xFF, 0xDA]) else {
        return false;
    };
    data[sos + 2..].windows(2).any(|w| w == [0xFF, 0xD9])
}

fn iou(a: &Candidate, b: &Candidate) -> f32 {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    let inter = (x1 - x0).max(0.0) * (y1 - y0).max(0.0);
    let union = a.w * a.h + b.w * b.h - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

/// Keeps the best-scoring candidates; on equal scores the earlier candidate wins.
fn non_maximum_suppression(mut candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by(|&a, &b| candidates[b].score.total_cmp(&candidates[a].score).then(a.cmp(&b)));
    let mut kept: Vec<Candidate> = Vec::new();
    for i in order {
        let c = std::mem::replace(
            &mut candidates[i],
            Candidate { x: 0.0, y: 0.0, w: 0.0, h: 0.0, score: 0.0, points: [(0.0, 0.0); 5] },
        );
        if kept.iter().all(|k| iou(k, &c) < NMS_IOU) {
            kept.push(c);
        }
    }
    kept
}

impl FaceDetector for YuNetDetector {
    fn model_name(&self) -> &str {
        MODEL_NAME
    }

    fn detect(&self, image: &[u8]) -> Result<Vec<DetectedFace>, String> {
        let img = Self::decode(image)?;
        self.detect_rgb(&img)
    }
}
