//! Shared test helpers: real JPEG fixtures and test engines.
//!
//! Test engines live only in tests; production plugins ship no built-in engines (ADR 0002).
#![allow(dead_code)]

use flagme_plugin_ai_tagging::VisionModel;
use flagme_plugin_face_detection::FaceDetector;
use flagme_plugin_ocr_text::OcrEngine;
use flagme_sdk::{
    AiInfo, AiTag, BoundingBox, DetectedFace, FaceLandmarks, HostCapabilities, OcrInfo,
    PluginError, PluginResult, TextRegion,
};
use image::codecs::jpeg::JpegEncoder;
use image::{Rgb, RgbImage};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Capabilities with real filesystem access that record every log message.
#[derive(Default)]
pub struct LogCaps {
    pub logs: Mutex<Vec<(String, String)>>,
}

impl LogCaps {
    pub fn warnings(&self) -> Vec<String> {
        self.logs
            .lock()
            .unwrap()
            .iter()
            .filter(|(level, _)| level == "warn")
            .map(|(_, m)| m.clone())
            .collect()
    }
}

impl HostCapabilities for LogCaps {
    fn fs_read(&self, path: &str) -> PluginResult<Vec<u8>> {
        std::fs::read(path).map_err(|e| PluginError::Io(e.to_string()))
    }
    fn fs_write(&self, path: &str, data: &[u8]) -> PluginResult<()> {
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| PluginError::Io(e.to_string()))?;
        }
        std::fs::write(path, data).map_err(|e| PluginError::Io(e.to_string()))
    }
    fn http_get(&self, _url: &str) -> PluginResult<String> {
        Err(PluginError::CapabilityDenied("http".to_string()))
    }
    fn kv_get(&self, _key: &str) -> PluginResult<Option<String>> {
        Ok(None)
    }
    fn kv_set(&self, _key: &str, _value: &str) -> PluginResult<()> {
        Ok(())
    }
    fn secret_get(&self, _name: &str) -> PluginResult<Option<String>> {
        Ok(None)
    }
    fn log(&self, level: &str, message: &str) {
        self.logs.lock().unwrap().push((level.to_string(), message.to_string()));
    }
}

pub fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flagme_test_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> String {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p.to_string_lossy().to_string()
}

/// A real, decodable JPEG without any metadata.
pub fn plain_jpeg(width: u32, height: u32) -> Vec<u8> {
    let img = RgbImage::from_fn(width, height, |x, y| {
        Rgb([(x * 7 % 256) as u8, (y * 11 % 256) as u8, ((x + y) * 3 % 256) as u8])
    });
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 90).encode_image(&img).unwrap();
    out
}

/// A real JPEG with an EXIF block containing Make, Model and (optionally) GPS coordinates.
pub fn jpeg_with_exif(make: &str, model: &str, gps: Option<(f64, f64)>) -> Vec<u8> {
    let tiff = build_tiff(make, model, gps);
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend(&tiff);
    let mut app1 = vec![0xFF, 0xE1];
    app1.extend(((payload.len() + 2) as u16).to_be_bytes());
    app1.extend(payload);

    let jpeg = plain_jpeg(32, 24);
    let mut out = jpeg[..2].to_vec(); // SOI
    out.extend(app1);
    out.extend(&jpeg[2..]);
    out
}

/// Little-endian TIFF with IFD0 (Make, Model, GPSInfo pointer) and a GPS IFD.
fn build_tiff(make: &str, model: &str, gps: Option<(f64, f64)>) -> Vec<u8> {
    let ascii = |s: &str| {
        let mut v = s.as_bytes().to_vec();
        v.push(0);
        v
    };
    let make_b = ascii(make);
    let model_b = ascii(model);
    let n_ifd0: u16 = if gps.is_some() { 3 } else { 2 };
    let ifd0_size = 2 + 12 * n_ifd0 as u32 + 4;
    let make_off = 8 + ifd0_size;
    let model_off = make_off + make_b.len() as u32;
    let mut gps_off = model_off + model_b.len() as u32;
    gps_off += gps_off % 2;

    let entry = |tag: u16, typ: u16, count: u32, value: u32| {
        let mut e = Vec::new();
        e.extend(tag.to_le_bytes());
        e.extend(typ.to_le_bytes());
        e.extend(count.to_le_bytes());
        e.extend(value.to_le_bytes());
        e
    };

    let mut t = b"II\x2a\x00".to_vec();
    t.extend(8u32.to_le_bytes());
    t.extend(n_ifd0.to_le_bytes());
    t.extend(entry(0x010F, 2, make_b.len() as u32, make_off));
    t.extend(entry(0x0110, 2, model_b.len() as u32, model_off));
    if gps.is_some() {
        t.extend(entry(0x8825, 4, 1, gps_off));
    }
    t.extend(0u32.to_le_bytes());
    t.extend(&make_b);
    t.extend(&model_b);
    while (t.len() as u32) < gps_off {
        t.push(0);
    }

    if let Some((lat, lon)) = gps {
        let gps_ifd_size = 2 + 12 * 4 + 4;
        let lat_off = gps_off + gps_ifd_size;
        let lon_off = lat_off + 24;
        let ref_val = |c: u8| u32::from_le_bytes([c, 0, 0, 0]);
        t.extend(4u16.to_le_bytes());
        t.extend(entry(0x0001, 2, 2, ref_val(if lat >= 0.0 { b'N' } else { b'S' })));
        t.extend(entry(0x0002, 5, 3, lat_off));
        t.extend(entry(0x0003, 2, 2, ref_val(if lon >= 0.0 { b'E' } else { b'W' })));
        t.extend(entry(0x0004, 5, 3, lon_off));
        t.extend(0u32.to_le_bytes());
        for v in [lat.abs(), lon.abs()] {
            let deg = v.trunc();
            let min = ((v - deg) * 60.0).trunc();
            let sec = ((v - deg) * 60.0 - min) * 60.0;
            for (num, den) in [(deg as u32, 1u32), (min as u32, 1), ((sec * 10000.0).round() as u32, 10000)] {
                t.extend(num.to_le_bytes());
                t.extend(den.to_le_bytes());
            }
        }
    }
    t
}

/// Test OCR engine: one region whose text is the image byte length (depends only on bytes).
pub struct ByteLenOcrEngine;
impl OcrEngine for ByteLenOcrEngine {
    fn name(&self) -> &str {
        "test-ocr"
    }
    fn recognize(&self, image: &[u8]) -> Result<OcrInfo, String> {
        let text = format!("bytes {}", image.len());
        Ok(OcrInfo {
            combined_text: text.clone(),
            regions: vec![TextRegion {
                text,
                confidence: 0.9,
                bbox: BoundingBox { x: 0.1, y: 0.1, width: 0.5, height: 0.1 },
            }],
            primary_language: Some("en".to_string()),
        })
    }
}

/// Test vision model: tag from byte length, embedding of the requested dimension (not normalised).
pub struct ByteLenVisionModel;
impl VisionModel for ByteLenVisionModel {
    fn model_name(&self) -> &str {
        "test-vision"
    }
    fn analyze(&self, image: &[u8], embedding_dim: usize) -> Result<AiInfo, String> {
        Ok(AiInfo {
            tags: vec![AiTag {
                label: format!("bytes {}", image.len()),
                confidence: 0.9,
                model: self.model_name().to_string(),
            }],
            description: "test".to_string(),
            embedding: Some((0..embedding_dim).map(|i| (i % 7) as f32 + 1.0).collect()),
        })
    }
}

/// Test face detector: one face with landmarks; confidence depends on byte length parity.
pub struct OneFaceDetector;
impl FaceDetector for OneFaceDetector {
    fn model_name(&self) -> &str {
        "test-face"
    }
    fn detect(&self, image: &[u8]) -> Result<Vec<DetectedFace>, String> {
        Ok(vec![DetectedFace {
            id: "face-1".to_string(),
            bbox: BoundingBox { x: 0.25, y: 0.2, width: 0.3, height: 0.4 },
            confidence: if image.len() % 2 == 0 { 0.95 } else { 0.94 },
            landmarks: Some(FaceLandmarks {
                left_eye: (0.33, 0.33),
                right_eye: (0.47, 0.33),
                nose: (0.40, 0.42),
                mouth_left: (0.35, 0.50),
                mouth_right: (0.45, 0.50),
            }),
            embedding: None,
            person_id: None,
        }])
    }
}

/// Engines that always fail.
pub struct FailingEngine;
impl OcrEngine for FailingEngine {
    fn name(&self) -> &str {
        "failing"
    }
    fn recognize(&self, _image: &[u8]) -> Result<OcrInfo, String> {
        Err("engine failure".to_string())
    }
}
impl VisionModel for FailingEngine {
    fn model_name(&self) -> &str {
        "failing"
    }
    fn analyze(&self, _image: &[u8], _dim: usize) -> Result<AiInfo, String> {
        Err("engine failure".to_string())
    }
}
impl FaceDetector for FailingEngine {
    fn model_name(&self) -> &str {
        "failing"
    }
    fn detect(&self, _image: &[u8]) -> Result<Vec<DetectedFace>, String> {
        Err("engine failure".to_string())
    }
}
