mod common;

use common::*;
use flagme_host::AppConfig;
use flagme_plugin_ai_tagging::{AiTaggingPlugin, VisionModel};
use flagme_plugin_face_detection::FaceDetectionPlugin;
use flagme_plugin_metadata::MetadataPlugin;
use flagme_plugin_ocr_text::OcrTextPlugin;
use flagme_plugin_privacy_redact::{PrivacyRedactPlugin, PrivacySettings};
use flagme_sdk::{AiInfo, FacesInfo, FileInfo, Plugin, PluginContext, UnifiedDocument};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

fn doc(path: &str) -> UnifiedDocument {
    let filename = Path::new(path).file_name().unwrap().to_string_lossy().to_string();
    UnifiedDocument::new(
        "doc-1",
        FileInfo {
            path: path.to_string(),
            filename,
            size_bytes: 0,
            mime_type: "image/jpeg".to_string(),
            sha256: None,
        },
    )
}

fn ctx(caps: &Arc<LogCaps>, pairs: &[(&str, &str)]) -> PluginContext {
    let settings: HashMap<String, String> =
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    PluginContext::with_settings(caps.clone(), settings)
}

/// File names that used to trigger synthetic results.
const MAGIC_NAMES: &[&str] = &[
    "berlin_sign.jpg",
    "tokyo_metro.jpg",
    "stromzaehler_meter_doc.jpg",
    "portrait_selfie_face_person.jpg",
    "group_family_crowd.jpg",
];

// no-synthetic-output AC-1: unreadable files leave doc.metadata untouched, create no location and log a warning
#[test]
fn test_ac1_metadata_unreadable_file() {
    let caps = Arc::new(LogCaps::default());
    let out = MetadataPlugin::new().process(doc("/nonexistent/berlin.jpg"), &ctx(&caps, &[])).unwrap();
    assert!(out.metadata.is_none());
    assert!(out.location.is_none());
    assert!(caps.warnings().iter().any(|w| w.contains("metadata: cannot read '/nonexistent/berlin.jpg'")));
}

// no-synthetic-output AC-1: readable files without metadata (also < 128 bytes) yield no camera, exposure or GPS values
#[test]
fn test_ac1_metadata_tiny_and_plain_files() {
    let dir = temp_dir("ac1");
    let tiny = write_file(&dir, "tiny.jpg", &[0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x10, b'E', b'x', b'i', b'f', 0, 0]);
    let plain = write_file(&dir, "plain.jpg", &plain_jpeg(40, 30));
    for path in [tiny, plain] {
        let caps = Arc::new(LogCaps::default());
        let out = MetadataPlugin::new().process(doc(&path), &ctx(&caps, &[])).unwrap();
        let m = out.metadata.expect("readable file yields metadata info");
        assert_eq!(
            (m.camera_make, m.camera_model, m.lens_model, m.iso, m.focal_length, m.f_number, m.exposure_time),
            (None, None, None, None, None, None, None),
            "{}",
            path
        );
        assert!(!m.raw_tags.keys().any(|k| k.starts_with("Exif.") || k.starts_with("XMP.") || k.starts_with("IPTC.")));
        assert!(out.location.is_none(), "{}", path);
    }

    // Positive control: real EXIF values are still extracted.
    let exif = write_file(&dir, "exif.jpg", &jpeg_with_exif("TestMake", "TestModel", Some((48.137, 11.575))));
    let caps = Arc::new(LogCaps::default());
    let out = MetadataPlugin::new().process(doc(&exif), &ctx(&caps, &[])).unwrap();
    assert_eq!(out.metadata.unwrap().camera_make.as_deref(), Some("TestMake"));
    let loc = out.location.expect("GPS from EXIF");
    assert!((loc.latitude.unwrap() - 48.137).abs() < 1e-4);
    assert!((loc.longitude.unwrap() - 11.575).abs() < 1e-4);
    let _ = std::fs::remove_dir_all(&dir);
}

// no-synthetic-output AC-2: the same bytes under different file names give the same result in every affected plugin
#[test]
fn test_ac2_output_independent_of_file_name() {
    let dir = temp_dir("ac2");
    let bytes = jpeg_with_exif("TestMake", "TestModel", Some((35.68, 139.69)));
    let reference = write_file(&dir, "a.jpg", &bytes);

    let run = |path: &str| {
        let caps = Arc::new(LogCaps::default());
        let c = ctx(&caps, &[]);
        let d = MetadataPlugin::new().process(doc(path), &c).unwrap();
        let d = OcrTextPlugin::with_engine(Box::new(ByteLenOcrEngine)).process(d, &c).unwrap();
        let d = AiTaggingPlugin::with_model(Box::new(ByteLenVisionModel)).process(d, &c).unwrap();
        let d = FaceDetectionPlugin::with_detector(Box::new(OneFaceDetector)).process(d, &c).unwrap();
        (d.metadata, d.location, d.ocr, d.ai, d.faces)
    };

    let expected = run(&reference);
    assert!(expected.0.is_some() && expected.2.is_some() && expected.3.is_some() && expected.4.is_some());
    for name in MAGIC_NAMES {
        let path = write_file(&dir, name, &bytes);
        assert_eq!(run(&path), expected, "result depends on file name '{}'", name);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// no-synthetic-output AC-3: without an engine ocr-text and ai-tagging leave their field None and warn; face-detection has the built-in YuNet engine (ADR 0009) and reports no face in a plain image
#[test]
fn test_ac3_no_engine_no_result() {
    let dir = temp_dir("ac3");
    let bytes = plain_jpeg(64, 48);
    for name in MAGIC_NAMES {
        let path = write_file(&dir, name, &bytes);
        let caps = Arc::new(LogCaps::default());
        // Settings that used to select built-in engines must not bring them back.
        let c = ctx(&caps, &[]);
        let d = OcrTextPlugin::new().process(doc(&path), &c).unwrap();
        let d = AiTaggingPlugin::new().process(d, &c).unwrap();
        let d = FaceDetectionPlugin::new().process(d, &c).unwrap();
        assert!(d.ocr.is_none() && d.ai.is_none(), "{}", name);
        assert_eq!(d.faces.as_ref().map(|f| f.faces.len()), Some(0), "{}: checked, no face", name);

        let w = caps.warnings();
        assert!(w.iter().any(|m| m.starts_with("ocr-text: no OCR engine available (ADR 0002)")), "{:?}", w);
        assert!(w.iter().any(|m| m.starts_with("ai-tagging: no vision model available (ADR 0002)")), "{:?}", w);
        assert!(!w.iter().any(|m| m.starts_with("face-detection:")), "{:?}", w);

        let caps = Arc::new(LogCaps::default());
        let c = ctx(&caps, &[("engine", "default"), ("model", "clip"), ("model_tier", "heavy"), ("tier", "heavy")]);
        let d = OcrTextPlugin::new().process(doc(&path), &c).unwrap();
        let d = AiTaggingPlugin::new().process(d, &c).unwrap();
        let d = FaceDetectionPlugin::new().process(d, &c).unwrap();
        assert!(d.ocr.is_none() && d.ai.is_none(), "{} with engine settings", name);
        assert_eq!(d.faces.as_ref().map(|f| f.faces.len()), Some(0), "{} with engine settings", name);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// no-synthetic-output AC-4: results of a given engine for the image bytes are passed through
#[test]
fn test_ac4_engine_results_passed_through() {
    let dir = temp_dir("ac4a");
    let bytes = plain_jpeg(50, 40);
    let path = write_file(&dir, "img.jpg", &bytes);
    let caps = Arc::new(LogCaps::default());
    let c = ctx(&caps, &[]);

    let ocr = OcrTextPlugin::with_engine(Box::new(ByteLenOcrEngine)).process(doc(&path), &c).unwrap().ocr.unwrap();
    assert_eq!(ocr.combined_text, format!("bytes {}", bytes.len()));

    let ai = AiTaggingPlugin::with_model(Box::new(ByteLenVisionModel)).process(doc(&path), &c).unwrap().ai.unwrap();
    assert_eq!(ai.tags[0].label, format!("bytes {}", bytes.len()));
    assert_eq!(ai.tags[0].model, "test-vision");

    let faces = FaceDetectionPlugin::with_detector(Box::new(OneFaceDetector)).process(doc(&path), &c).unwrap().faces.unwrap();
    assert_eq!(faces.detection_model, "test-face");
    assert_eq!(faces.faces.len(), 1);
    assert!(faces.faces[0].landmarks.is_some());
    assert!(caps.warnings().is_empty(), "{:?}", caps.warnings());
    let _ = std::fs::remove_dir_all(&dir);
}

// no-synthetic-output AC-4: unreadable files and engine errors leave the field None and log a warning
#[test]
fn test_ac4_read_and_engine_errors() {
    let dir = temp_dir("ac4b");
    let path = write_file(&dir, "img.jpg", &plain_jpeg(20, 20));
    let missing = "/nonexistent/img.jpg";

    for (p, expect) in [(missing, "cannot read"), (path.as_str(), "failed")] {
        let caps = Arc::new(LogCaps::default());
        let c = ctx(&caps, &[]);
        let (ocr_engine, ai_model, face_det): (Box<dyn flagme_plugin_ocr_text::OcrEngine>, Box<dyn VisionModel>, Box<dyn flagme_plugin_face_detection::FaceDetector>) =
            if expect == "failed" {
                (Box::new(FailingEngine), Box::new(FailingEngine), Box::new(FailingEngine))
            } else {
                (Box::new(ByteLenOcrEngine), Box::new(ByteLenVisionModel), Box::new(OneFaceDetector))
            };
        let d = OcrTextPlugin::with_engine(ocr_engine).process(doc(p), &c).unwrap();
        let d = AiTaggingPlugin::with_model(ai_model).process(d, &c).unwrap();
        let d = FaceDetectionPlugin::with_detector(face_det).process(d, &c).unwrap();
        assert!(d.ocr.is_none() && d.ai.is_none() && d.faces.is_none(), "{}", expect);
        let w = caps.warnings();
        for prefix in ["ocr-text:", "ai-tagging:", "face-detection:"] {
            assert!(w.iter().any(|m| m.starts_with(prefix) && m.contains(expect)), "{} {}: {:?}", prefix, expect, w);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Vision model returning a fixed-size embedding regardless of the requested dimension.
struct FixedDimModel(Vec<f32>);
impl VisionModel for FixedDimModel {
    fn model_name(&self) -> &str {
        "fixed-dim"
    }
    fn analyze(&self, _image: &[u8], _dim: usize) -> Result<AiInfo, String> {
        Ok(AiInfo { tags: vec![], description: "fixed".into(), embedding: Some(self.0.clone()) })
    }
}

// no-synthetic-output AC-4: ai-tagging normalises embeddings to length 1 and drops embeddings with the wrong dimension
#[test]
fn test_ac4_embedding_normalised_and_checked() {
    let dir = temp_dir("ac4c");
    let path = write_file(&dir, "img.jpg", &plain_jpeg(20, 20));

    let caps = Arc::new(LogCaps::default());
    let ai = AiTaggingPlugin::with_model(Box::new(ByteLenVisionModel))
        .process(doc(&path), &ctx(&caps, &[("embedding_dim", "128")]))
        .unwrap()
        .ai
        .unwrap();
    let emb = ai.embedding.unwrap();
    assert_eq!(emb.len(), 128);
    assert!((emb.iter().map(|v| v * v).sum::<f32>().sqrt() - 1.0).abs() < 1e-5);

    for (vector, reason) in [(vec![1.0f32; 10], "dimension 10, expected 512"), (vec![0.0f32; 512], "zero")] {
        let caps = Arc::new(LogCaps::default());
        let ai = AiTaggingPlugin::with_model(Box::new(FixedDimModel(vector)))
            .process(doc(&path), &ctx(&caps, &[]))
            .unwrap()
            .ai
            .expect("tags/description are kept");
        assert!(ai.embedding.is_none());
        assert!(caps.warnings().iter().any(|w| w.contains("embedding") && w.contains(reason)), "{:?}", caps.warnings());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// no-synthetic-output AC-5: engine-selecting settings warn "not supported yet"; min_confidence and embedding_dim still apply
#[test]
fn test_ac5_engine_settings_not_supported() {
    let dir = temp_dir("ac5");
    let path = write_file(&dir, "img.jpg", &plain_jpeg(20, 20));
    let caps = Arc::new(LogCaps::default());

    OcrTextPlugin::with_engine(Box::new(ByteLenOcrEngine))
        .process(doc(&path), &ctx(&caps, &[("engine", "tesseract")]))
        .unwrap();
    AiTaggingPlugin::with_model(Box::new(ByteLenVisionModel))
        .process(doc(&path), &ctx(&caps, &[("model", "clip"), ("model_tier", "heavy"), ("device", "mps")]))
        .unwrap();
    FaceDetectionPlugin::with_detector(Box::new(OneFaceDetector))
        .process(doc(&path), &ctx(&caps, &[("tier", "heavy"), ("device", "cpu")]))
        .unwrap();

    let mut w = caps.warnings();
    w.sort();
    assert_eq!(
        w,
        vec![
            "settings[ai-tagging]: 'clip' for 'model' is not supported yet (no vision model is built in, see ADR 0002), ignored",
            "settings[ai-tagging]: 'heavy' for 'model_tier' is not supported yet (no vision model is built in, see ADR 0002), ignored",
            "settings[ai-tagging]: 'mps' for 'device' is not supported yet (no vision model is built in, see ADR 0002), ignored",
            "settings[face-detection]: 'cpu' for 'device' is not supported yet (only the built-in YuNet detector is available, see ADR 0009), ignored",
            "settings[face-detection]: 'heavy' for 'tier' is not supported yet (only the built-in YuNet detector is available, see ADR 0009), ignored",
            "settings[ocr-text]: 'tesseract' for 'engine' is not supported yet (no OCR engine is built in, see ADR 0002), ignored",
        ]
    );

    let caps = Arc::new(LogCaps::default());
    let faces = FaceDetectionPlugin::with_detector(Box::new(OneFaceDetector))
        .process(doc(&path), &ctx(&caps, &[("min_confidence", "0.99")]))
        .unwrap()
        .faces
        .unwrap();
    assert!(faces.faces.is_empty(), "min_confidence still applies");
    let ai = AiTaggingPlugin::with_model(Box::new(ByteLenVisionModel))
        .process(doc(&path), &ctx(&caps, &[("embedding_dim", "64")]))
        .unwrap()
        .ai
        .unwrap();
    assert_eq!(ai.embedding.unwrap().len(), 64, "embedding_dim still applies");
    let _ = std::fs::remove_dir_all(&dir);
}

// no-synthetic-output AC-6: shipped configs and the built-in default disable ocr-text and ai-tagging; face-detection has a real engine since ADR 0009 and is enabled (config.docker.yaml is not maintained)
#[test]
fn test_ac6_shipped_configs_disable_experimental_plugins() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let mut configs = vec![("AppConfig::default()".to_string(), AppConfig::default())];
    for name in ["config.yaml", "config.docker.yaml", "config.mac-studio.yaml"] {
        configs.push((name.to_string(), AppConfig::load_from_file(root.join(name)).expect(name)));
    }
    for (name, cfg) in configs {
        for plugin in ["ocr-text", "ai-tagging", "face-detection"] {
            let p = cfg.plugins.get(plugin).unwrap_or_else(|| panic!("{}: {} missing", name, plugin));
            if plugin == "face-detection" {
                assert!(p.enabled || name == "config.docker.yaml", "{}: face-detection must be enabled (ADR 0009)", name);
            } else {
                assert!(!p.enabled, "{}: {} must be disabled (ADR 0002)", name, plugin);
            }
            for key in ["model", "model_tier", "tier", "engine", "device"] {
                assert!(!p.settings.contains_key(key), "{}: {} still sets '{}'", name, plugin, key);
            }
        }
        assert!(cfg.plugins["metadata"].enabled, "{}: metadata stays enabled", name);
    }
}

// no-synthetic-output AC-7: privacy-redact warns and reports no face redaction when no face detection result exists
#[test]
fn test_ac7_privacy_redact_warns_without_face_result() {
    let dir = temp_dir("ac7");
    let path = write_file(&dir, "img.jpg", &plain_jpeg(40, 30));
    let plugin = PrivacyRedactPlugin::with_settings(PrivacySettings {
        fuzz_gps: false,
        strip_serial_numbers: false,
        output_dir: dir.join("out").to_string_lossy().to_string(),
        ..Default::default()
    });

    let caps = Arc::new(LogCaps::default());
    let p = plugin.process(doc(&path), &ctx(&caps, &[])).unwrap().privacy.unwrap();
    assert!(!p.is_redacted);
    assert!(p.applied_rules.is_empty());
    assert!(caps
        .warnings()
        .iter()
        .any(|w| w == &format!("privacy-redact: no face detection result for '{}'; faces NOT redacted", path)));

    // "Detected, no faces" is a valid result and stays silent.
    let caps = Arc::new(LogCaps::default());
    let mut d = doc(&path);
    d.faces = Some(FacesInfo { faces: vec![], detection_model: "test-face".into() });
    plugin.process(d, &ctx(&caps, &[])).unwrap();
    assert!(caps.warnings().is_empty(), "{:?}", caps.warnings());

    // redact_faces = false needs no detection result.
    let caps = Arc::new(LogCaps::default());
    plugin.process(doc(&path), &ctx(&caps, &[("redact_faces", "false")])).unwrap();
    assert!(caps.warnings().is_empty(), "{:?}", caps.warnings());
    let _ = std::fs::remove_dir_all(&dir);
}
