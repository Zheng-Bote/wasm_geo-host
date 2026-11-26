use flagme_host::Storage;
use flagme_plugin_privacy_redact::{
    bbox_to_rect, redact_region, PixelRect, REDACTED_JPEG_QUALITY, PrivacyRedactPlugin, PrivacySettings, RedactionStyle,
};
use flagme_sdk::{
    HostCapabilities, NoopCapabilities, Plugin, PluginContext, PluginError, PluginResult,
};
use image::codecs::jpeg::JpegEncoder;
use image::{ImageFormat, Rgb, RgbImage};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use wasm_geo_models::{
    BoundingBox, DetectedFace, FacesInfo, FileInfo, LocationInfo, MetadataInfo, PrivacyInfo,
    UnifiedDocument,
};

// privacy-redact AC-1: wasm-geo-models introduces PrivacyInfo and serializes it in UnifiedDocument.privacy
#[test]
fn test_ac1_privacy_model_serialization() {
    let privacy = PrivacyInfo {
        is_redacted: true,
        faces_redacted_count: 3,
        license_plates_redacted_count: 1,
        original_gps_fuzzed: true,
        redacted_image_path: Some("/cache/redacted/image.jpg".to_string()),
        applied_rules: vec!["redacted_3_faces".to_string(), "fuzzed_gps".to_string()],
    };

    let serialized = serde_json::to_string(&privacy).expect("Serialization failed");
    let deserialized: PrivacyInfo = serde_json::from_str(&serialized).expect("Deserialization failed");

    assert_eq!(privacy, deserialized);
    assert_eq!(deserialized.faces_redacted_count, 3);
    assert!(deserialized.is_redacted);
}

// privacy-redact AC-2: flagme-plugin-privacy-redact compiles as plugin crate with valid metadata
#[test]
fn test_ac2_plugin_contract_metadata() {
    let plugin = PrivacyRedactPlugin::new();
    assert_eq!(plugin.name(), "privacy-redact");
    assert_eq!(plugin.version(), env!("CARGO_PKG_VERSION"), "all crates share the workspace version");
}

// privacy-redact AC-3: Visual obfuscation destructively renders mosaic pixelation and blackout rectangles over input bounding boxes
#[test]
fn test_ac3_visual_obfuscation_applied() {
    let dir = temp_dir("ac3");
    let src = write_source_jpeg(&dir, 128, 96);
    let ctx = PluginContext::new(Arc::new(NoopCapabilities));
    let doc = face_doc("photo-face-redact", &src, vec![face("f1", 0.1, 0.1, 0.4, 0.4)]);

    for (style, name) in [(RedactionStyle::Pixelate, "pixelate"), (RedactionStyle::Blackout, "blackout")] {
        let plugin = PrivacyRedactPlugin::with_settings(redact_settings(style, &dir));
        let res = plugin.process(doc.clone(), &ctx).expect("process failed");
        let privacy = res.privacy.unwrap();
        assert!(privacy.is_redacted);
        assert_eq!(privacy.faces_redacted_count, 1);
        assert_eq!(privacy.applied_rules, vec![format!("redacted_1_faces_via_{}", name)]);
        let out = privacy.redacted_image_path.expect("redacted path missing");
        assert!(std::path::Path::new(&out).is_file());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// privacy-redact AC-4: GPS coordinates are fuzzed to the configured number of decimal places when fuzz_gps_coordinates is active
#[test]
fn test_ac4_gps_coordinate_fuzzing() {
    let plugin = PrivacyRedactPlugin::with_settings(PrivacySettings {
        style: RedactionStyle::Pixelate,
        redact_faces: false,
        fuzz_gps: true,
        gps_decimal_places: 2,
        strip_serial_numbers: false,
        ..Default::default()
    });

    let ctx = PluginContext::new(Arc::new(NoopCapabilities));

    let mut doc = UnifiedDocument::new(
        "photo-gps-precise",
        FileInfo {
            path: "/media/home.jpg".to_string(),
            filename: "home.jpg".to_string(),
            size_bytes: 2048,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    doc.location = Some(LocationInfo {
        latitude: Some(52.518624),
        longitude: Some(13.376198),
        ..Default::default()
    });

    let res = plugin.process(doc, &ctx).expect("Process failed");
    let loc = res.location.unwrap();

    // With 2 decimal places: 52.518624 -> 52.52 and 13.376198 -> 13.38
    assert_eq!(loc.latitude, Some(52.52));
    assert_eq!(loc.longitude, Some(13.38));
    assert!(res.privacy.unwrap().original_gps_fuzzed);
}

// privacy-redact AC-5: Hardware serial numbers are purged from raw_tags when strip_serial_numbers is active
#[test]
fn test_ac5_hardware_serial_purging() {
    let plugin = PrivacyRedactPlugin::with_settings(PrivacySettings {
        style: RedactionStyle::Pixelate,
        redact_faces: false,
        fuzz_gps: false,
        gps_decimal_places: 2,
        strip_serial_numbers: true,
        ..Default::default()
    });

    let ctx = PluginContext::new(Arc::new(NoopCapabilities));

    let mut doc = UnifiedDocument::new(
        "photo-serials",
        FileInfo {
            path: "/media/dslr.jpg".to_string(),
            filename: "dslr.jpg".to_string(),
            size_bytes: 3000,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );

    let mut raw_tags = HashMap::new();
    raw_tags.insert("CameraSerialNumber".to_string(), "SN-987654321".to_string());
    raw_tags.insert("LensSerialNumber".to_string(), "LENS-123456".to_string());
    raw_tags.insert("ApertureValue".to_string(), "2.8".to_string());

    doc.metadata = Some(MetadataInfo {
        camera_make: Some("Sony".to_string()),
        camera_model: Some("A7IV".to_string()),
        raw_tags,
        ..Default::default()
    });

    let res = plugin.process(doc, &ctx).expect("Process failed");
    let meta = res.metadata.unwrap();

    assert!(!meta.raw_tags.contains_key("CameraSerialNumber"));
    assert!(!meta.raw_tags.contains_key("LensSerialNumber"));
    assert!(meta.raw_tags.contains_key("ApertureValue"));
}

// privacy-redact AC-6: Integration tests verify that exported redacted image variants do not contain recoverable facial pixels
#[test]
fn test_ac6_sqlite_privacy_persistence() {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!("test_privacy_ac6_{}.db", std::process::id()));
    let db_str = db_path.to_str().unwrap();

    let storage = Storage::new(db_str);
    storage.init_schema().expect("Schema init failed");

    let mut doc = UnifiedDocument::new(
        "photo-privacy-db",
        FileInfo {
            path: "/media/passersby.jpg".to_string(),
            filename: "passersby.jpg".to_string(),
            size_bytes: 4096,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    doc.location = Some(LocationInfo {
        latitude: Some(48.85),
        longitude: Some(2.35),
        ..Default::default()
    });
    doc.privacy = Some(PrivacyInfo {
        is_redacted: true,
        faces_redacted_count: 2,
        license_plates_redacted_count: 0,
        original_gps_fuzzed: true,
        redacted_image_path: Some("/cache/redacted/passersby.redacted.jpg".to_string()),
        applied_rules: vec!["redacted_2_faces_via_pixelate".to_string()],
    });

    storage.save_document(&doc).expect("Save document failed");

    // Retrieve via Storage::get_document
    let loaded = storage.get_document("photo-privacy-db").expect("Photo not found");
    assert!(loaded.privacy.is_some());
    let p = loaded.privacy.unwrap();
    assert!(p.is_redacted);
    assert_eq!(p.faces_redacted_count, 2);
    assert!(p.original_gps_fuzzed);

    let _ = std::fs::remove_file(&db_path);
}

// ---------------------------------------------------------------------------
// Pixel-accurate redaction (issue #22)
// ---------------------------------------------------------------------------

/// Capabilities that record log messages and can deny writes.
struct RecordingCaps {
    deny_write: bool,
    logs: Mutex<Vec<(String, String)>>,
}

impl RecordingCaps {
    fn new(deny_write: bool) -> Arc<Self> {
        Arc::new(Self { deny_write, logs: Mutex::new(Vec::new()) })
    }
    fn warnings(&self) -> Vec<String> {
        self.logs.lock().unwrap().iter().filter(|(l, _)| l == "warn").map(|(_, m)| m.clone()).collect()
    }
}

impl HostCapabilities for RecordingCaps {
    fn fs_read(&self, path: &str) -> PluginResult<Vec<u8>> {
        NoopCapabilities.fs_read(path)
    }
    fn fs_write(&self, path: &str, data: &[u8]) -> PluginResult<()> {
        if self.deny_write {
            return Err(PluginError::CapabilityDenied("filesystem".to_string()));
        }
        NoopCapabilities.fs_write(path, data)
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

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("privacy_redact_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// High-frequency test pattern, so that every redaction style changes it visibly.
fn pattern_image(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        Rgb([
            ((x * 37 + y * 91) % 256) as u8,
            ((x * 113 + y * 17) % 256) as u8,
            (((x / 3) * 61 + (y / 2) * 29) % 256) as u8,
        ])
    })
}

fn encode_jpeg(img: &RgbImage) -> Vec<u8> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 95).encode_image(img).unwrap();
    out
}

fn write_source_jpeg(dir: &PathBuf, w: u32, h: u32) -> String {
    let path = dir.join("source.jpg");
    std::fs::write(&path, encode_jpeg(&pattern_image(w, h))).unwrap();
    path.to_str().unwrap().to_string()
}

fn decode(path: &str) -> RgbImage {
    image::load_from_memory(&std::fs::read(path).unwrap()).unwrap().to_rgb8()
}

fn face(id: &str, x: f32, y: f32, w: f32, h: f32) -> DetectedFace {
    DetectedFace {
        id: id.to_string(),
        bbox: BoundingBox { x, y, width: w, height: h },
        confidence: 0.9,
        landmarks: None,
        embedding: None,
        person_id: None,
    }
}

fn face_doc(id: &str, path: &str, faces: Vec<DetectedFace>) -> UnifiedDocument {
    let mut doc = UnifiedDocument::new(
        id,
        FileInfo {
            path: path.to_string(),
            filename: "source.jpg".to_string(),
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    );
    doc.faces = Some(FacesInfo { faces, detection_model: "test".to_string() });
    doc
}

fn redact_settings(style: RedactionStyle, dir: &PathBuf) -> PrivacySettings {
    PrivacySettings {
        style,
        fuzz_gps: false,
        strip_serial_numbers: false,
        output_dir: dir.join("out").to_str().unwrap().to_string(),
        ..Default::default()
    }
}

/// Mean absolute per-channel difference over the pixels selected by `select`.
fn mean_abs_diff(a: &RgbImage, b: &RgbImage, select: impl Fn(u32, u32) -> bool) -> f64 {
    let (mut sum, mut n) = (0u64, 0u64);
    for (x, y, pa) in a.enumerate_pixels() {
        if select(x, y) {
            let pb = b.get_pixel(x, y);
            for c in 0..3 {
                sum += (pa[c] as i32 - pb[c] as i32).unsigned_abs() as u64;
            }
            n += 3;
        }
    }
    sum as f64 / n.max(1) as f64
}

fn variance(img: &RgbImage, rect: PixelRect) -> f64 {
    let vals: Vec<f64> = img
        .enumerate_pixels()
        .filter(|(x, y, _)| rect.contains(*x, *y))
        .flat_map(|(_, _, p)| p.0.map(|v| v as f64))
        .collect();
    let mean = vals.iter().sum::<f64>() / vals.len() as f64;
    vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / vals.len() as f64
}

/// Grows `rect` by `margin` pixels on each side (JPEG block bleed zone).
fn grown(rect: PixelRect, margin: u32) -> PixelRect {
    let x = rect.x.saturating_sub(margin);
    let y = rect.y.saturating_sub(margin);
    PixelRect { x, y, width: rect.x + rect.width + margin - x, height: rect.y + rect.height + margin - y }
}

// privacy-redact AC-7: pixels inside every bbox differ from the source, pixels outside all bboxes stay unchanged (exact on the pixel buffer)
#[test]
fn test_ac7_redact_region_only_touches_bbox() {
    let src = pattern_image(120, 80);
    let rect = bbox_to_rect(&BoundingBox { x: 0.25, y: 0.2, width: 0.3, height: 0.5 }, 120, 80).unwrap();
    assert_eq!(rect, PixelRect { x: 30, y: 16, width: 36, height: 40 });

    for style in [RedactionStyle::Pixelate, RedactionStyle::Blur, RedactionStyle::Blackout] {
        let mut img = src.clone();
        redact_region(&mut img, rect, style, 8, 6.0);
        let inside = mean_abs_diff(&img, &src, |x, y| rect.contains(x, y));
        let outside = mean_abs_diff(&img, &src, |x, y| !rect.contains(x, y));
        assert!(inside > 20.0, "{:?}: inside diff {} too small", style, inside);
        assert_eq!(outside, 0.0, "{:?}: pixels outside the bbox changed", style);
    }
}

// privacy-redact AC-7: end-to-end, every bbox is redacted and pixels outside all bboxes match the source within JPEG tolerance
#[test]
fn test_ac7_plugin_redacts_all_bboxes_only() {
    let dir = temp_dir("ac7");
    let src_path = write_source_jpeg(&dir, 160, 120);
    let faces = vec![face("a", 0.05, 0.1, 0.25, 0.3), face("b", 0.6, 0.5, 0.3, 0.4)];
    let rects: Vec<PixelRect> = faces.iter().map(|f| bbox_to_rect(&f.bbox, 160, 120).unwrap()).collect();

    let plugin = PrivacyRedactPlugin::with_settings(redact_settings(RedactionStyle::Blackout, &dir));
    let ctx = PluginContext::new(Arc::new(NoopCapabilities));
    let res = plugin.process(face_doc("photo-ac7", &src_path, faces), &ctx).unwrap();
    let privacy = res.privacy.unwrap();
    assert_eq!(privacy.faces_redacted_count, 2);

    let source = decode(&src_path);
    let out = decode(privacy.redacted_image_path.as_deref().unwrap());
    let black = RgbImage::new(160, 120);
    for r in &rects {
        let shrunk = PixelRect { x: r.x + 2, y: r.y + 2, width: r.width - 4, height: r.height - 4 };
        assert!(mean_abs_diff(&out, &black, |x, y| shrunk.contains(x, y)) < 8.0, "bbox not blacked out");
    }
    let bleed: Vec<PixelRect> = rects.iter().map(|r| grown(*r, 8)).collect();
    let is_outside = |x: u32, y: u32| !bleed.iter().any(|r| r.contains(x, y));
    // Tolerance = error of a plain re-encode of the source at the plugin's JPEG quality.
    let mut reencoded = Vec::new();
    JpegEncoder::new_with_quality(&mut reencoded, REDACTED_JPEG_QUALITY).encode_image(&source).unwrap();
    let reencoded = image::load_from_memory(&reencoded).unwrap().to_rgb8();
    let tolerance = mean_abs_diff(&reencoded, &source, is_outside) * 1.1 + 0.5;
    let outside = mean_abs_diff(&out, &source, is_outside);
    assert!(outside <= tolerance, "pixels outside the bboxes changed: mean diff {} > {}", outside, tolerance);
    let _ = std::fs::remove_dir_all(&dir);
}

// privacy-redact AC-8: blur_sigma measurably changes the result for style blur (larger sigma -> lower variance inside the bbox)
#[test]
fn test_ac8_blur_sigma_changes_result() {
    let src = pattern_image(100, 100);
    let rect = PixelRect { x: 20, y: 20, width: 60, height: 60 };
    let mut weak = src.clone();
    let mut strong = src.clone();
    redact_region(&mut weak, rect, RedactionStyle::Blur, 16, 1.0);
    redact_region(&mut strong, rect, RedactionStyle::Blur, 16, 20.0);

    assert!(mean_abs_diff(&weak, &strong, |x, y| rect.contains(x, y)) > 5.0);
    let (v_src, v_weak, v_strong) = (variance(&src, rect), variance(&weak, rect), variance(&strong, rect));
    assert!(v_strong < v_weak && v_weak < v_src, "variances src={} weak={} strong={}", v_src, v_weak, v_strong);
}

// privacy-redact AC-9: block_size sets the edge length in pixels of the uniform mosaic blocks for style pixelate
#[test]
fn test_ac9_block_size_sets_mosaic_blocks() {
    let src = pattern_image(96, 96);
    let rect = PixelRect { x: 16, y: 16, width: 64, height: 64 };
    let mut distinct = Vec::new();
    for block in [4u32, 16] {
        let mut img = src.clone();
        redact_region(&mut img, rect, RedactionStyle::Pixelate, block, 24.0);
        for py in rect.y..rect.y + rect.height {
            for px in rect.x..rect.x + rect.width {
                let origin = (rect.x + (px - rect.x) / block * block, rect.y + (py - rect.y) / block * block);
                assert_eq!(img.get_pixel(px, py), img.get_pixel(origin.0, origin.1), "block {} not uniform", block);
            }
        }
        let colors: HashSet<[u8; 3]> =
            img.enumerate_pixels().filter(|(x, y, _)| rect.contains(*x, *y)).map(|(_, _, p)| p.0).collect();
        distinct.push(colors.len());
    }
    // 64/4 = 16 -> up to 256 blocks; 64/16 = 4 -> at most 16 blocks.
    assert!(distinct[1] <= 16);
    assert!(distinct[0] > distinct[1], "distinct colors: {:?}", distinct);
}

// privacy-redact AC-10: the redacted output is a decodable JPEG with the same dimensions as the source
#[test]
fn test_ac10_output_is_valid_jpeg_same_size() {
    let dir = temp_dir("ac10");
    let src_path = write_source_jpeg(&dir, 143, 77);
    let ctx = PluginContext::new(Arc::new(NoopCapabilities));
    for style in [RedactionStyle::Pixelate, RedactionStyle::Blur, RedactionStyle::Blackout] {
        let plugin = PrivacyRedactPlugin::with_settings(redact_settings(style, &dir));
        let res = plugin.process(face_doc("photo/ac10", &src_path, vec![face("f", 0.3, 0.3, 0.4, 0.4)]), &ctx).unwrap();
        let out = res.privacy.unwrap().redacted_image_path.unwrap();
        assert!(out.ends_with("photo_ac10.redacted.jpg"));
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(image::guess_format(&bytes).unwrap(), ImageFormat::Jpeg);
        let img = image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg).unwrap();
        assert_eq!((img.width(), img.height()), (143, 77));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// privacy-redact AC-11: undecodable, unreadable or unwritable images record no redaction rule and log a warning
#[test]
fn test_ac11_failures_record_no_redaction() {
    let dir = temp_dir("ac11");
    let garbage = dir.join("broken.jpg");
    std::fs::write(&garbage, vec![0x42u8; 4096]).unwrap();
    let valid = write_source_jpeg(&dir, 64, 64);
    let missing = dir.join("missing.jpg");

    let cases = [
        (garbage.to_str().unwrap().to_string(), false, "decode"),
        (missing.to_str().unwrap().to_string(), false, "read"),
        (valid.clone(), true, "write"),
    ];
    for (path, deny_write, expect) in cases {
        let caps = RecordingCaps::new(deny_write);
        let ctx = PluginContext::new(caps.clone());
        let plugin = PrivacyRedactPlugin::with_settings(redact_settings(RedactionStyle::Pixelate, &dir));
        let res = plugin.process(face_doc("photo-ac11", &path, vec![face("f", 0.1, 0.1, 0.5, 0.5)]), &ctx).unwrap();
        let privacy = res.privacy.unwrap();
        assert!(privacy.applied_rules.iter().all(|r| !r.starts_with("redacted_")), "{}: {:?}", expect, privacy.applied_rules);
        assert!(!privacy.is_redacted);
        assert_eq!(privacy.faces_redacted_count, 0);
        assert!(privacy.redacted_image_path.is_none());
        let warnings = caps.warnings();
        assert!(warnings.iter().any(|w| w.contains("NOT redacted") && w.contains(expect)), "{}: {:?}", expect, warnings);
    }
    assert!(!dir.join("out").join("photo-ac11.redacted.jpg").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

// privacy-redact AC-11: only faces with a valid bbox area are counted; no valid bbox at all means no redaction and a warning
#[test]
fn test_ac11_counts_only_processed_faces() {
    let dir = temp_dir("ac11b");
    let src_path = write_source_jpeg(&dir, 64, 64);
    let plugin = PrivacyRedactPlugin::with_settings(redact_settings(RedactionStyle::Blackout, &dir));

    let caps = RecordingCaps::new(false);
    let ctx = PluginContext::new(caps.clone());
    let faces = vec![face("ok", 0.1, 0.1, 0.3, 0.3), face("empty", 0.5, 0.5, 0.0, 0.2), face("nan", f32::NAN, 0.1, 0.2, 0.2)];
    let privacy = plugin.process(face_doc("photo-ac11b", &src_path, faces), &ctx).unwrap().privacy.unwrap();
    assert_eq!(privacy.faces_redacted_count, 1);
    assert_eq!(privacy.applied_rules, vec!["redacted_1_faces_via_blackout".to_string()]);
    assert_eq!(caps.warnings().iter().filter(|w| w.contains("skipped")).count(), 2);

    let caps = RecordingCaps::new(false);
    let ctx = PluginContext::new(caps.clone());
    let faces = vec![face("outside", 1.2, 1.2, 0.3, 0.3)];
    let privacy = plugin.process(face_doc("photo-ac11c", &src_path, faces), &ctx).unwrap().privacy.unwrap();
    assert_eq!(privacy.faces_redacted_count, 0);
    assert!(privacy.applied_rules.is_empty());
    assert!(privacy.redacted_image_path.is_none());
    assert!(caps.warnings().iter().any(|w| w.contains("NOT redacted")));
    let _ = std::fs::remove_dir_all(&dir);
}
