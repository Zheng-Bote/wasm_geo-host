//! Built-in OCR engine: PaddleOCR PP-OCRv5 mobile text detection and Latin recognition
//! (Apache-2.0) run with the pure-Rust inference engine `rten` inside the plugin component
//! (ADR 0009).
//!
//! Detection yields a probability map; connected areas above a threshold become axis-aligned
//! boxes, which are merged into text lines and read with CTC decoding against the model's own
//! character dictionary (836 characters including umlauts and ß). Images are decoded without
//! applying the EXIF orientation; rotated or curved text is not straightened.

use crate::OcrEngine;
use flagme_sdk::{BoundingBox, OcrInfo, TextRegion};
use image::{ImageFormat, RgbImage};
use rten::{Model, Value};
use rten_tensor::prelude::*;
use rten_tensor::{NdTensor, Tensor};

/// Embedded detection model (see `models/SOURCE.md`).
pub static DET_MODEL: &[u8] = include_bytes!("../models/PP-OCRv5_mobile_det.onnx");
/// Embedded Latin recognition model.
pub static REC_MODEL: &[u8] = include_bytes!("../models/latin_PP-OCRv5_mobile_rec.onnx");
/// Configuration of the recognition model; holds the character dictionary.
pub static REC_CONFIG: &str = include_str!("../models/latin_PP-OCRv5_mobile_rec.yml");
/// Value of `OcrEngine::name`.
pub const ENGINE_NAME: &str = "paddleocr-v5-mobile-latin";
/// Number of characters in the dictionary of the Latin model.
pub const DICTIONARY_LEN: usize = 836;

pub const DET_LONG_SIDE: f32 = 960.0;
pub const DET_THRESH: f32 = 0.3;
pub const DET_BOX_THRESH: f32 = 0.6;
pub const DET_UNCLIP: f32 = 1.5;
pub const DET_MIN_SIDE: usize = 3;
pub const REC_HEIGHT: u32 = 48;
pub const REC_MIN_WIDTH: u32 = 320;
pub const LINE_MIN_V_OVERLAP: f32 = 0.5;
pub const LINE_MAX_GAP: f32 = 0.8;

const DET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DET_STD: [f32; 3] = [0.229, 0.224, 0.225];

/// A text box in original-image pixels with its detection score.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    score: f32,
}

impl Rect {
    fn height(&self) -> f32 {
        self.y1 - self.y0
    }
    fn v_overlap(&self, o: &Rect) -> f32 {
        let overlap = (self.y1.min(o.y1) - self.y0.max(o.y0)).max(0.0);
        overlap / self.height().min(o.height()).max(1.0)
    }
}

/// Reads the `character_dict` list of a PaddleOCR `inference.yml`.
pub fn load_dictionary(config: &str) -> Result<Vec<String>, String> {
    let section = config.split("character_dict:").nth(1).ok_or("no character_dict in the model configuration")?;
    let mut dict = Vec::new();
    for line in section.lines().skip(1) {
        let trimmed = line.trim_start();
        let Some(raw) = trimmed.strip_prefix("- ") else {
            if trimmed.is_empty() {
                continue;
            }
            break;
        };
        let raw = raw.trim_end();
        let entry = if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
            raw[1..raw.len() - 1].replace("''", "'")
        } else if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
            raw[1..raw.len() - 1].replace("\\\"", "\"").replace("\\\\", "\\")
        } else {
            raw.to_string()
        };
        dict.push(entry);
    }
    if dict.len() != DICTIONARY_LEN {
        return Err(format!("character dictionary has {} entries, expected {}", dict.len(), DICTIONARY_LEN));
    }
    Ok(dict)
}

/// `true` if an end-of-image marker (`FF D9`) follows the first start-of-scan marker (`FF DA`).
/// Copied from `face-detection` on purpose: plugins must not depend on each other.
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

/// PP-OCRv5 mobile OCR engine for Latin script.
pub struct PaddleOcrEngine {
    det: Model,
    rec: Model,
    dict: Vec<String>,
}

impl PaddleOcrEngine {
    /// Loads both embedded models and the dictionary.
    pub fn new() -> Result<Self, String> {
        let det = Model::load_static_slice(DET_MODEL).map_err(|e| format!("cannot load detection model: {}", e))?;
        let rec = Model::load_static_slice(REC_MODEL).map_err(|e| format!("cannot load recognition model: {}", e))?;
        let dict = load_dictionary(REC_CONFIG)?;
        Ok(Self { det, rec, dict })
    }

    /// Text boxes in original pixels, unsorted.
    fn detect(&self, img: &RgbImage) -> Result<Vec<Rect>, String> {
        let (w, h) = img.dimensions();
        let ratio = DET_LONG_SIDE / w.max(h) as f32;
        let side = |s: u32| (((s as f32 * ratio) / 32.0).round() as u32 * 32).max(32);
        let (rw, rh) = (side(w), side(h));
        let resized = image::imageops::resize(img, rw, rh, image::imageops::FilterType::Triangle);
        let mut input = NdTensor::<f32, 4>::zeros([1, 3, rh as usize, rw as usize]);
        for (x, y, p) in resized.enumerate_pixels() {
            let bgr = [p[2], p[1], p[0]];
            for c in 0..3 {
                input[[0, c, y as usize, x as usize]] = (bgr[c] as f32 / 255.0 - DET_MEAN[c]) / DET_STD[c];
            }
        }
        let out = self
            .det
            .run_one(Value::from(input).into(), None)
            .map_err(|e| format!("text detection failed: {}", e))?;
        let map: Tensor<f32> = out.try_into().map_err(|e| format!("unexpected detection output: {:?}", e))?;
        let shape = map.shape().to_vec();
        if shape.len() != 4 {
            return Err(format!("unexpected detection output shape {:?}", shape));
        }
        let (ph, pw) = (shape[2], shape[3]);
        let prob = map.to_vec();
        if prob.len() < ph * pw {
            return Err("detection output too small".to_string());
        }
        let (sx, sy) = (w as f32 / pw as f32, h as f32 / ph as f32);
        let mut seen = vec![false; ph * pw];
        let mut rects = Vec::new();
        let mut stack = Vec::new();
        for start in 0..ph * pw {
            if seen[start] || prob[start] <= DET_THRESH {
                continue;
            }
            seen[start] = true;
            stack.push(start);
            let (mut x0, mut y0, mut x1, mut y1) = (pw, ph, 0usize, 0usize);
            let (mut sum, mut n) = (0.0f32, 0usize);
            while let Some(i) = stack.pop() {
                let (y, x) = (i / pw, i % pw);
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
                sum += prob[i];
                n += 1;
                let mut visit = |j: usize| {
                    if !seen[j] && prob[j] > DET_THRESH {
                        seen[j] = true;
                        stack.push(j);
                    }
                };
                if x > 0 {
                    visit(i - 1);
                }
                if x + 1 < pw {
                    visit(i + 1);
                }
                if y > 0 {
                    visit(i - pw);
                }
                if y + 1 < ph {
                    visit(i + pw);
                }
            }
            let score = sum / n as f32;
            let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
            if score < DET_BOX_THRESH || bw.min(bh) < DET_MIN_SIDE {
                continue;
            }
            let (bw, bh) = (bw as f32, bh as f32);
            let d = bw * bh * DET_UNCLIP / (2.0 * (bw + bh));
            rects.push(Rect {
                x0: ((x0 as f32 - d) * sx).max(0.0),
                y0: ((y0 as f32 - d) * sy).max(0.0),
                x1: ((x1 as f32 + 1.0 + d) * sx).min(w as f32),
                y1: ((y1 as f32 + 1.0 + d) * sy).min(h as f32),
                score,
            });
        }
        Ok(rects)
    }

    /// Reads one line; returns the text and its mean character probability.
    fn read(&self, img: &RgbImage, r: &Rect) -> Result<(String, f32), String> {
        let (x0, y0) = (r.x0.floor() as u32, r.y0.floor() as u32);
        let cw = (r.x1.ceil() as u32).min(img.width()).saturating_sub(x0).max(1);
        let ch = (r.y1.ceil() as u32).min(img.height()).saturating_sub(y0).max(1);
        let crop = image::imageops::crop_imm(img, x0, y0, cw, ch).to_image();
        let width = ((REC_HEIGHT as f32 * cw as f32 / ch as f32).ceil() as u32).max(1);
        let tensor_w = width.max(REC_MIN_WIDTH);
        let resized = image::imageops::resize(&crop, width, REC_HEIGHT, image::imageops::FilterType::Triangle);
        let mut input = NdTensor::<f32, 4>::zeros([1, 3, REC_HEIGHT as usize, tensor_w as usize]);
        for (x, y, p) in resized.enumerate_pixels() {
            let bgr = [p[2], p[1], p[0]];
            for c in 0..3 {
                input[[0, c, y as usize, x as usize]] = (bgr[c] as f32 / 255.0 - 0.5) / 0.5;
            }
        }
        let out = self
            .rec
            .run_one(Value::from(input).into(), None)
            .map_err(|e| format!("text recognition failed: {}", e))?;
        let probs: Tensor<f32> = out.try_into().map_err(|e| format!("unexpected recognition output: {:?}", e))?;
        let shape = probs.shape().to_vec();
        if shape.len() != 3 || shape[2] != self.dict.len() + 2 {
            return Err(format!("unexpected recognition output shape {:?}", shape));
        }
        let (steps, classes) = (shape[1], shape[2]);
        let v = probs.to_vec();
        let (mut text, mut confidences, mut last) = (String::new(), Vec::new(), 0usize);
        for step in 0..steps {
            let row = &v[step * classes..(step + 1) * classes];
            let (idx, p) = row
                .iter()
                .enumerate()
                .fold((0usize, f32::MIN), |best, (i, &x)| if x > best.1 { (i, x) } else { best });
            if idx != 0 && idx != last {
                text.push_str(if idx <= self.dict.len() { &self.dict[idx - 1] } else { " " });
                confidences.push(p);
            }
            last = idx;
        }
        let confidence = if confidences.is_empty() { 0.0 } else { confidences.iter().sum::<f32>() / confidences.len() as f32 };
        Ok((text.trim().to_string(), confidence.clamp(0.0, 1.0)))
    }
}

/// Merges boxes on the same line that are close to each other, then sorts in reading order.
fn merge_lines(mut rects: Vec<Rect>) -> Vec<Rect> {
    rects.sort_by(|a, b| (a.y0 + a.y1).total_cmp(&(b.y0 + b.y1)).then(a.x0.total_cmp(&b.x0)));
    let mut lines: Vec<Rect> = Vec::new();
    for r in rects {
        let target = lines.iter_mut().find(|l| {
            let gap = if r.x0 > l.x1 { r.x0 - l.x1 } else if l.x0 > r.x1 { l.x0 - r.x1 } else { 0.0 };
            l.v_overlap(&r) >= LINE_MIN_V_OVERLAP && gap <= LINE_MAX_GAP * l.height().max(r.height())
        });
        match target {
            Some(l) => {
                let weight = |x: &Rect| (x.x1 - x.x0) * x.height();
                let (wl, wr) = (weight(l), weight(&r));
                l.score = (l.score * wl + r.score * wr) / (wl + wr).max(1.0);
                l.x0 = l.x0.min(r.x0);
                l.y0 = l.y0.min(r.y0);
                l.x1 = l.x1.max(r.x1);
                l.y1 = l.y1.max(r.y1);
            }
            None => lines.push(r),
        }
    }
    // Reading order: rows (boxes overlapping vertically by >= 50 %), then left to right.
    lines.sort_by(|a, b| a.y0.total_cmp(&b.y0));
    let mut rows: Vec<Vec<Rect>> = Vec::new();
    for l in lines {
        match rows.last_mut() {
            Some(row) if row.iter().any(|o| o.v_overlap(&l) >= LINE_MIN_V_OVERLAP) => row.push(l),
            _ => rows.push(vec![l]),
        }
    }
    rows.into_iter()
        .flat_map(|mut row| {
            row.sort_by(|a, b| a.x0.total_cmp(&b.x0));
            row
        })
        .collect()
}

impl OcrEngine for PaddleOcrEngine {
    fn name(&self) -> &str {
        ENGINE_NAME
    }

    fn recognize(&self, image: &[u8]) -> Result<OcrInfo, String> {
        self.recognize_limited(image, usize::MAX).map(|(info, _)| info)
    }

    fn recognize_limited(&self, image: &[u8], max_regions: usize) -> Result<(OcrInfo, Option<usize>), String> {
        let img = decode(image)?;
        let (w, h) = (img.width() as f32, img.height() as f32);
        if w == 0.0 || h == 0.0 {
            return Err("empty image".to_string());
        }
        let mut lines = merge_lines(self.detect(&img)?);
        let found = lines.len();
        let truncated = if found > max_regions {
            // Keep the most confident lines, then restore the reading order.
            let mut ranked: Vec<(usize, f32)> = lines.iter().enumerate().map(|(i, l)| (i, l.score)).collect();
            ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let mut keep: Vec<usize> = ranked.into_iter().take(max_regions).map(|(i, _)| i).collect();
            keep.sort_unstable();
            lines = keep.into_iter().map(|i| lines[i]).collect();
            Some(found)
        } else {
            None
        };
        let mut regions = Vec::with_capacity(lines.len());
        for l in &lines {
            let (text, confidence) = self.read(&img, l)?;
            if text.is_empty() {
                continue;
            }
            let x = (l.x0 / w).clamp(0.0, 1.0);
            let y = (l.y0 / h).clamp(0.0, 1.0);
            regions.push(TextRegion {
                text,
                confidence,
                bbox: BoundingBox {
                    x,
                    y,
                    width: (l.x1 / w).clamp(0.0, 1.0) - x,
                    height: (l.y1 / h).clamp(0.0, 1.0) - y,
                },
            });
        }
        let combined_text = regions.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("\n");
        Ok((OcrInfo { combined_text, regions, primary_language: None }, truncated))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_has_umlauts() {
        let d = load_dictionary(REC_CONFIG).unwrap();
        for c in ["ä", "ö", "ü", "Ä", "Ö", "Ü", "ß", "€"] {
            assert!(d.iter().any(|e| e == c), "{}", c);
        }
    }
}
